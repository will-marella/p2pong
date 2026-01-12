// WebRTC network runtime using str0m Sans-I/O implementation
// Manages P2P connections via WebRTC with explicit I/O control

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::net::UdpSocket;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use str0m::{Rtc, Event, Input, Output, IceConnectionState};
use str0m::net::{Protocol, Receive};
use str0m::channel::{ChannelId, ChannelConfig, Reliability};
use str0m::change::SdpOffer;

use super::{
    client::{ConnectionMode, NetworkCommand, NetworkEvent},
    protocol::NetworkMessage,
};

// Signaling server address
const SIGNALING_SERVER: &str = "ws://143.198.15.158:8080";

/// Log diagnostic info to file
fn log_to_file(category: &str, message: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/p2pong-debug.log")
    {
        let _ = writeln!(file, "[{:013}] [{}] {}", timestamp, category, message);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SignalingMessage {
    Register {
        peer_id: String,
    },
    RegisterOk {
        peer_id: String,
    },
    ListPeers,
    PeerList {
        peers: Vec<String>,
    },
    Offer {
        target: String,
        from: String,
        sdp: String,
    },
    Answer {
        target: String,
        from: String,
        sdp: String,
    },
    IceCandidate {
        target: String,
        from: String,
        candidate: String,
    },
    Error {
        message: String,
    },
}

/// Initialize and run the WebRTC network in a background thread
pub fn spawn_network_thread(
    mode: ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> std::io::Result<()> {
    eprintln!("SPAWN: About to spawn network thread!");
    std::io::stderr().flush().ok();

    thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eprintln!("SPAWN: Network thread spawned!");
            std::io::stderr().flush().ok();
            log_to_file("THREAD_SPAWN", "Network thread started");

            // Create minimal Tokio runtime only for signaling phase
            let rt = Runtime::new().expect("Failed to create tokio runtime");
            eprintln!("SPAWN: Tokio runtime created!");
            std::io::stderr().flush().ok();
            log_to_file("THREAD_RUNTIME", "Tokio runtime created");

            let result = rt.block_on(async {
                eprintln!("SPAWN: Entering async block!");
                std::io::stderr().flush().ok();
                log_to_file("THREAD_ASYNC_START", "Entering async block");

                match setup_signaling_and_sdp(mode.clone(), &event_tx).await {
                    Ok((rtc, udp_socket, channel_id)) => {
                        log_to_file("SETUP_COMPLETE", "Signaling and SDP setup complete");
                        eprintln!("SPAWN: Setup complete, dropping Tokio runtime");
                        std::io::stderr().flush().ok();
                        Ok((rtc, udp_socket, channel_id))
                    }
                    Err(e) => {
                        error!("Setup error: {}", e);
                        log_to_file("SETUP_ERROR", &format!("Setup error: {}", e));
                        Err(e)
                    }
                }
            });

            // Drop Tokio runtime - no longer needed
            drop(rt);

            match result {
                Ok((rtc, udp_socket, channel_id)) => {
                    eprintln!("SPAWN: Running str0m polling loop");
                    std::io::stderr().flush().ok();
                    log_to_file("POLLING_START", "Starting str0m polling loop");

                    if let Err(e) = run_str0m_loop(rtc, udp_socket, channel_id, event_tx, cmd_rx, connected) {
                        error!("Network loop error: {}", e);
                        log_to_file("LOOP_ERROR", &format!("Network loop error: {}", e));
                    }
                }
                Err(e) => {
                    error!("Network setup failed: {}", e);
                    eprintln!("SPAWN: Setup failed: {}", e);
                    std::io::stderr().flush().ok();
                }
            }

            log_to_file("THREAD_END", "Network thread ending");
        })).unwrap_or_else(|_| {
            eprintln!("SPAWN: PANIC in network thread!");
            std::io::stderr().flush().ok();
        });
    });

    eprintln!("SPAWN: Thread spawned, returning Ok!");
    std::io::stderr().flush().ok();
    Ok(())
}

/// Setup signaling and SDP exchange, returns configured Rtc, UDP socket, and optional channel ID
/// Client mode returns the channel_id from add_channel(), host mode returns None (channel comes from Event::ChannelOpen)
async fn setup_signaling_and_sdp(
    mode: ConnectionMode,
    event_tx: &mpsc::Sender<NetworkEvent>,
) -> Result<(Rtc, UdpSocket, Option<ChannelId>)> {
    log_to_file("SETUP_START", "setup_signaling_and_sdp() started");

    // Generate a unique peer ID
    let peer_id = format!("peer-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    info!("Local peer ID: {}", peer_id);
    log_to_file("SETUP_PEER_ID", &peer_id);

    // Connect to signaling server
    log_to_file("SETUP_CONNECT", "Connecting to signaling server");
    let (ws_stream, _) = connect_async(SIGNALING_SERVER).await?;
    info!("Connected to signaling server");
    log_to_file("SETUP_CONNECTED", "Connected to signaling server");

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Register with signaling server
    log_to_file("SETUP_REGISTER", "Sending registration message");
    let register_msg = SignalingMessage::Register {
        peer_id: peer_id.clone(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&register_msg)?))
        .await?;
    log_to_file("SETUP_REGISTER_SENT", "Registration message sent");

    // Wait for registration confirmation
    log_to_file("SETUP_WAIT_REGISTER", "Waiting for registration confirmation");
    if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;
        log_to_file("SETUP_REGISTER_OK", "Registration confirmed");
        match msg {
            SignalingMessage::RegisterOk { .. } => {
                info!("✅ Registered with signaling server");
            }
            _ => {
                return Err(anyhow!("Unexpected registration response"));
            }
        }
    }

    // Create str0m Rtc instance
    log_to_file("SETUP_WEBRTC", "Creating str0m Rtc instance");
    let mut rtc = Rtc::builder()
        .set_rtp_mode(false)  // Data channels only, no RTP media
        .build();
    info!("Created str0m Rtc instance");
    log_to_file("SETUP_WEBRTC_CREATED", "Rtc instance created");

    // Bind UDP socket for ICE
    let udp_socket = UdpSocket::bind("0.0.0.0:0")?;
    udp_socket.set_nonblocking(false)?;
    let local_addr = udp_socket.local_addr()?;
    info!("Bound UDP socket: {}", local_addr);
    log_to_file("SETUP_UDP", &format!("UDP socket bound to {}", local_addr));

    // Handle based on connection mode
    log_to_file("SETUP_MODE_SELECT", &format!("Connection mode: {:?}", mode));
    let channel_id = match mode {
        ConnectionMode::Listen { .. } => {
            log_to_file("SETUP_HOST_MODE", "Entering host mode");
            info!("🎮 Host mode: waiting for client connection...");
            println!("\n🎮 Waiting for client to connect...");
            println!("📋 Your Peer ID: {}", peer_id);
            println!("   Share this with the client to connect!\n");

            handle_host_mode(
                &mut rtc,
                &mut ws_sink,
                &mut ws_stream,
                &peer_id,
                event_tx,
            )
            .await?
        }
        ConnectionMode::Connect { multiaddr } => {
            let target_peer = multiaddr;
            info!("🔌 Client mode: connecting to {}...", target_peer);
            log_to_file("SETUP_CLIENT_MODE", &format!("Connecting to {}", target_peer));

            handle_client_mode(
                &mut rtc,
                &mut ws_sink,
                &mut ws_stream,
                &peer_id,
                event_tx,
                target_peer,
            )
            .await?
        }
    };

    log_to_file("SETUP_COMPLETE", "SDP and ICE exchange complete");
    // For client mode, channel_id is Some(id). For host mode, it's None.
    Ok((rtc, udp_socket, channel_id.into()))
}

/// Host mode: wait for offer from client
async fn handle_host_mode(
    rtc: &mut Rtc,
    ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    peer_id: &str,
    _event_tx: &mpsc::Sender<NetworkEvent>,
) -> Result<Option<ChannelId>> {
    log_to_file("HOST_MODE", "handle_host_mode() started");

    // Wait for offer from client
    let offer_sdp = loop {
        if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
            let msg: SignalingMessage = serde_json::from_str(&text)?;

            match msg {
                SignalingMessage::Offer { from, sdp, .. } => {
                    info!("📥 Received offer from {}", from);
                    log_to_file("HOST_OFFER", &format!("Received offer from {}", from));
                    break sdp;
                }
                _ => {}
            }
        } else {
            return Err(anyhow!("WebSocket closed while waiting for offer"));
        }
    };

    // Accept offer and create answer
    log_to_file("HOST_ACCEPT_OFFER", "Accepting offer from client");
    let offer = SdpOffer::from_sdp_string(&offer_sdp)?;
    let answer = rtc.sdp_api().accept_offer(offer)?;
    info!("📤 Sending answer");
    log_to_file("HOST_ANSWER", "Answer created");

    // Send answer back
    let answer_msg = SignalingMessage::Answer {
        target: "remote".to_string(),
        from: peer_id.to_string(),
        sdp: answer.to_sdp_string(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&answer_msg)?))
        .await?;
    log_to_file("HOST_ANSWER_SENT", "Answer sent to client");

    // Handle ICE candidates
    let _ = handle_ice_candidates(rtc, ws_sink, ws_stream, peer_id, None).await?;
    log_to_file("HOST_ICE_COMPLETE", "ICE candidate exchange complete");

    // In host mode, the channel_id comes from Event::ChannelOpen when remote opens it
    Ok(None)
}

/// Client mode: create offer and send to host
async fn handle_client_mode(
    rtc: &mut Rtc,
    ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    peer_id: &str,
    _event_tx: &mpsc::Sender<NetworkEvent>,
    target_peer: String,
) -> Result<Option<ChannelId>> {
    log_to_file("CLIENT_MODE", "handle_client_mode() started");

    // Create data channel and offer
    let mut change = rtc.sdp_api();
    let channel_id = change.add_channel_with_config(ChannelConfig {
        label: "pong".to_string(),
        ordered: false,  // Allow out-of-order delivery
        reliability: Reliability::MaxRetransmits { retransmits: 3 },
        negotiated: None,
        protocol: String::new(),
    });
    let (offer, _pending) = change.apply()
        .ok_or_else(|| anyhow!("Failed to apply SDP changes"))?;

    info!("📨 Created data channel");
    log_to_file("CLIENT_CHANNEL", &format!("Data channel created: {:?}", channel_id));

    // Send offer to target
    let offer_msg = SignalingMessage::Offer {
        target: target_peer.clone(),
        from: peer_id.to_string(),
        sdp: offer.to_sdp_string(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&offer_msg)?))
        .await?;
    info!("📤 Sent offer to {}", target_peer);
    log_to_file("CLIENT_OFFER_SENT", &format!("Offer sent to {}", target_peer));

    // Wait for answer
    let _answer_sdp = loop {
        if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
            let msg: SignalingMessage = serde_json::from_str(&text)?;

            match msg {
                SignalingMessage::Answer { sdp, .. } => {
                    info!("📥 Received answer");
                    log_to_file("CLIENT_ANSWER", "Received answer from host");
                    break sdp;
                }
                _ => {}
            }
        } else {
            return Err(anyhow!("WebSocket closed while waiting for answer"));
        }
    };

    // Handle ICE candidates
    let _final_channel_id = handle_ice_candidates(rtc, ws_sink, ws_stream, peer_id, Some(channel_id)).await?;
    log_to_file("CLIENT_ICE_COMPLETE", "ICE candidate exchange complete");

    Ok(Some(channel_id))
}

/// Exchange ICE candidates with remote peer
async fn handle_ice_candidates(
    _rtc: &mut Rtc,
    _ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    _peer_id: &str,
    channel_id: Option<ChannelId>,
) -> Result<Option<ChannelId>> {
    info!("🧊 Starting ICE candidate exchange...");
    log_to_file("ICE_START", "ICE candidate exchange starting");

    // For now, use a simple timeout-based approach
    let start_time = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(5);
    let completion_wait = Duration::from_millis(300);

    let mut remote_candidates_received = 0;

    loop {
        let elapsed = start_time.elapsed();

        // Complete if minimum wait elapsed
        if elapsed > completion_wait {
            log_to_file(
                "ICE_COMPLETE_MIN_WAIT",
                &format!("Minimum wait elapsed, remote_received={}", remote_candidates_received),
            );
            break;
        }

        // Hard timeout
        if elapsed > max_wait {
            log_to_file(
                "ICE_TIMEOUT",
                &format!("Hard timeout reached: remote_received={}", remote_candidates_received),
            );
            break;
        }

        // Check for remote candidates via WebSocket
        let remaining = completion_wait.saturating_sub(elapsed);
        let timeout_duration = Duration::from_millis(50).min(remaining);

        let select_timeout = tokio::time::sleep(timeout_duration);
        tokio::pin!(select_timeout);

        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SignalingMessage>(&text) {
                            Ok(SignalingMessage::IceCandidate { .. }) => {
                                remote_candidates_received += 1;
                                log_to_file("ICE_RECV", &format!("Remote ICE candidate #{}", remote_candidates_received));
                                debug!("🧊 Remote ICE candidate received");
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ = &mut select_timeout => {
                // Timeout - continue loop
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    log_to_file(
        "ICE_COMPLETE",
        &format!("ICE candidate exchange complete, received={}", remote_candidates_received),
    );
    info!("✅ ICE candidate exchange complete");

    // ICE exchange complete. Return the channel_id from setup phase.
    // The polling loop will confirm the channel is open via Event::ChannelOpen.
    info!("✅ Returning channel_id after ICE exchange");
    Ok(channel_id)
}

/// Main synchronous polling loop for str0m
fn run_str0m_loop(
    mut rtc: Rtc,
    udp_socket: UdpSocket,
    initial_channel_id: Option<ChannelId>,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> Result<()> {
    log_to_file("POLLING_LOOP", "Starting main polling loop");
    info!("🔄 Starting WebRTC polling loop");

    let mut buf = vec![0u8; 8192];
    // Client mode provides the channel_id from setup; host mode gets it from Event::ChannelOpen
    let mut active_channel_id: Option<ChannelId> = initial_channel_id;

    loop {
        // Phase 1: Poll str0m for outputs
        loop {
            match rtc.poll_output()? {
                Output::Transmit(transmit) => {
                    // Send UDP packet to remote peer
                    match udp_socket.send_to(&transmit.contents, transmit.destination) {
                        Ok(_) => {
                            log_to_file(
                                "UDP_SEND",
                                &format!("Sent {} bytes to {}", transmit.contents.len(), transmit.destination),
                            );
                        }
                        Err(e) => {
                            warn!("Failed to send UDP packet: {}", e);
                            log_to_file("UDP_SEND_ERROR", &format!("Failed to send: {}", e));
                        }
                    }
                }
                Output::Timeout(deadline) => {
                    // str0m says we should wait until deadline for next event
                    let now = Instant::now();
                    if deadline > now {
                        let duration = deadline - now;
                        udp_socket.set_read_timeout(Some(duration))?;
                    } else {
                        udp_socket.set_read_timeout(Some(Duration::from_millis(100)))?;
                    }
                    break; // Exit poll loop to wait for input
                }
                Output::Event(event) => {
                    // Process str0m event
                    handle_str0m_event(
                        event,
                        &event_tx,
                        &connected,
                        &mut active_channel_id,
                    )?;
                }
            }
        }

        // Phase 2: Wait for UDP input or timeout
        match udp_socket.recv_from(&mut buf) {
            Ok((n, source)) => {
                // Received UDP packet - pass to str0m
                let receive = Receive {
                    proto: Protocol::Udp,
                    source,
                    destination: udp_socket.local_addr()?,
                    contents: buf[..n].try_into()?,
                };
                rtc.handle_input(Input::Receive(Instant::now(), receive))?;
                log_to_file("UDP_RECV", &format!("Received {} bytes from {}", n, source));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout - notify str0m
                rtc.handle_input(Input::Timeout(Instant::now()))?;
            }
            Err(e) => {
                error!("UDP socket error: {}", e);
                return Err(e.into());
            }
        }

        // Phase 3: Process commands from game loop (non-blocking)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                NetworkCommand::SendInput(action) => {
                    if let Some(cid) = active_channel_id {
                        let msg = NetworkMessage::Input(action);
                        if let Ok(bytes) = msg.to_bytes() {
                            if let Some(mut channel) = rtc.channel(cid) {
                                match channel.write(true, &bytes) {
                                    Ok(_) => {
                                        log_to_file("SEND_INPUT", &format!("Input sent, {} bytes", bytes.len()));
                                    }
                                    Err(e) => {
                                        warn!("Failed to send input: {}", e);
                                        log_to_file("SEND_INPUT_ERROR", &format!("Send error: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
                NetworkCommand::SendMessage(msg) => {
                    if let Some(cid) = active_channel_id {
                        if let Ok(bytes) = msg.to_bytes() {
                            if let Some(mut channel) = rtc.channel(cid) {
                                match channel.write(true, &bytes) {
                                    Ok(_) => {
                                        log_to_file("SEND_MESSAGE", &format!("Message sent, {} bytes", bytes.len()));
                                    }
                                    Err(e) => {
                                        warn!("Failed to send message: {}", e);
                                        log_to_file("SEND_MESSAGE_ERROR", &format!("Send error: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
                NetworkCommand::Disconnect => {
                    log_to_file("DISCONNECT", "Disconnect command received");
                    return Ok(());
                }
            }
        }
    }
}

/// Handle events from str0m
fn handle_str0m_event(
    event: Event,
    event_tx: &mpsc::Sender<NetworkEvent>,
    connected: &Arc<AtomicBool>,
    active_channel_id: &mut Option<ChannelId>,
) -> Result<()> {
    match event {
        Event::IceConnectionStateChange(state) => {
            match state {
                IceConnectionState::Connected => {
                    info!("🔗 ICE connection established");
                    log_to_file("ICE_CONNECTED", "ICE connection state: Connected");
                    connected.store(true, Ordering::Relaxed);
                    let _ = event_tx.send(NetworkEvent::Connected {
                        peer_id: "remote".to_string(),
                    });
                }
                IceConnectionState::Disconnected => {
                    info!("❌ ICE connection disconnected");
                    log_to_file("ICE_DISCONNECTED", "ICE connection state: Disconnected");
                    connected.store(false, Ordering::Relaxed);
                    let _ = event_tx.send(NetworkEvent::Disconnected);
                }
                _ => {}
            }
        }
        Event::ChannelOpen(cid, label) => {
            info!("📨 Data channel opened: {}", label);
            log_to_file("CHANNEL_OPEN", &format!("Data channel opened: {}", label));
            *active_channel_id = Some(cid);
            let _ = event_tx.send(NetworkEvent::DataChannelOpened);
        }
        Event::ChannelData(channel_data) => {
            // Received data on channel
            if let Ok(msg) = NetworkMessage::from_bytes(&channel_data.data) {
                match msg {
                    NetworkMessage::Input(action) => {
                        let _ = event_tx.send(NetworkEvent::ReceivedInput(action));
                    }
                    NetworkMessage::BallSync(state) => {
                        let _ = event_tx.send(NetworkEvent::ReceivedBallState(state));
                    }
                    NetworkMessage::ScoreSync {
                        left,
                        right,
                        game_over,
                    } => {
                        let _ = event_tx.send(NetworkEvent::ReceivedScore {
                            left,
                            right,
                            game_over,
                        });
                    }
                    NetworkMessage::Ping { timestamp_ms } => {
                        let _ = event_tx.send(NetworkEvent::ReceivedPing { timestamp_ms });
                    }
                    NetworkMessage::Pong { timestamp_ms } => {
                        let _ = event_tx.send(NetworkEvent::ReceivedPong { timestamp_ms });
                    }
                    NetworkMessage::Heartbeat { sequence } => {
                        log_to_file("HEARTBEAT_RECV", &format!("Heartbeat #{}", sequence));
                    }
                    NetworkMessage::Disconnect => {
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    _ => {}
                }
            } else {
                log_to_file("DECODE_ERROR", &format!("Failed to decode message"));
            }
        }
        _ => {}
    }

    Ok(())
}
