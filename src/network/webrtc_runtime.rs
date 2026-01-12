// WebRTC network runtime - manages P2P connections via WebRTC
// Bridges async WebRTC with sync game loop via channels

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use super::{
    client::{ConnectionMode, NetworkCommand, NetworkEvent},
    protocol::NetworkMessage,
};

// Signaling server address (will be on your relay VM)
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

// STUN server for NAT traversal
// Using VoIPGratia (stun.voxgratia.org:443) instead of Google's STUN server
// Google's server is blocked on some networks. VoIPGratia works across more network configurations.
const STUN_SERVER: &str = "stun:stun.cloudflare.com:3478";

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
    eprintln!("SPAWN: About to spawn thread!");
    std::io::stderr().flush().ok();

    thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eprintln!("SPAWN: Network thread spawned!");
            std::io::stderr().flush().ok();
            log_to_file("THREAD_SPAWN", "Network thread started");
            let rt = Runtime::new().expect("Failed to create tokio runtime");
            eprintln!("SPAWN: Tokio runtime created!");
            std::io::stderr().flush().ok();
            log_to_file("THREAD_RUNTIME", "Tokio runtime created");

            rt.block_on(async move {
                eprintln!("SPAWN: Entering async block!");
                std::io::stderr().flush().ok();
                log_to_file("THREAD_ASYNC_START", "Entering async block");
                if let Err(e) = run_network(mode, event_tx, cmd_rx, connected).await {
                    error!("Network error: {}", e);
                    eprintln!("SPAWN: Network error: {}", e);
                    std::io::stderr().flush().ok();
                    log_to_file("THREAD_ERROR", &format!("Network error: {}", e));
                }
                log_to_file("THREAD_ASYNC_END", "Exiting async block");
            });
            eprintln!("SPAWN: Thread ending!");
            std::io::stderr().flush().ok();
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

async fn run_network(
    mode: ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> Result<()> {
    log_to_file("NETWORK_START", "run_network() started");
    // Generate a unique peer ID
    let peer_id = format!("peer-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    info!("Local peer ID: {}", peer_id);
    log_to_file("NETWORK_PEER_ID", &peer_id);

    // Connect to signaling server
    log_to_file("NETWORK_CONNECT", "Connecting to signaling server");
    let (ws_stream, _) = connect_async(SIGNALING_SERVER).await?;
    info!("Connected to signaling server");
    log_to_file("NETWORK_CONNECTED", "Connected to signaling server");

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Register with signaling server
    log_to_file("NETWORK_REGISTER", "Sending registration message");
    let register_msg = SignalingMessage::Register {
        peer_id: peer_id.clone(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&register_msg)?))
        .await?;
    log_to_file("NETWORK_REGISTER_SENT", "Registration message sent");

    // Wait for registration confirmation
    log_to_file("NETWORK_WAIT_REGISTER", "Waiting for registration confirmation");
    if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;
        log_to_file("NETWORK_REGISTER_OK", "Registration confirmed");
        match msg {
            SignalingMessage::RegisterOk { .. } => {
                info!("✅ Registered with signaling server");
            }
            _ => {
                return Err(anyhow!("Unexpected registration response"));
            }
        }
    }

    // Create WebRTC API with proper ICE keepalive configuration
    log_to_file("NETWORK_WEBRTC", "Creating WebRTC API");
    let media_engine = MediaEngine::default();

    // Configure ICE timeouts to prevent 30-second disconnection timeout
    // Default is 5s disconnected, 25s failed - totaling ~30s before failure
    // We increase these values and ensure keepalive packets are sent frequently
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_ice_timeouts(
        Some(Duration::from_secs(30)),  // Disconnected timeout: 30s (was 5s)
        Some(Duration::from_secs(60)),  // Failed timeout: 60s (was 25s)
        Some(Duration::from_secs(2)),   // Keepalive interval: 2s (was 10s)
    );

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_setting_engine(setting_engine)
        .build();

    // Configure ICE servers (STUN for NAT traversal)
    // Note: We use STUN-only (no TURN) for purely P2P connectivity.
    // The heartbeat mechanism (15s keepalive) prevents ICE timeouts during idle periods.
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![STUN_SERVER.to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create peer connection
    log_to_file("NETWORK_PEER_CONN", "Creating peer connection");
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);
    info!("Created RTCPeerConnection");
    log_to_file("NETWORK_PEER_CONN_CREATED", "Peer connection created");

    // Log configuration details for debugging ICE connectivity
    log_to_file("ICE_CONFIG", &format!("STUN server: {} | ICE timeouts: 30s disconnected, 60s failed, 2s keepalive | Data channel: unordered, max_retransmits=3 | Heartbeat: every 2s", STUN_SERVER));

    // Track data channel
    let data_channel: Arc<AsyncMutex<Option<Arc<RTCDataChannel>>>> =
        Arc::new(AsyncMutex::new(None));

    // Set up connection state handler
    {
        let connected = connected.clone();
        let event_tx = event_tx.clone();
        peer_connection.on_peer_connection_state_change(Box::new(
            move |state: RTCPeerConnectionState| {
                info!("🔄 Connection state changed: {:?}", state);
                match state {
                    RTCPeerConnectionState::Connected => {
                        connected.store(true, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Connected {
                            peer_id: "remote".to_string(),
                        });
                    }
                    RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Closed => {
                        log_to_file("PEER_DISCONNECT", &format!("Peer connection state changed to: {:?}", state));
                        connected.store(false, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    _ => {}
                }
                Box::pin(async {})
            },
        ));
    }

    // Monitor ICE connection state separately from peer connection state
    // ICE (Interactive Connectivity Establishment) manages the low-level P2P connectivity
    // The keepalive mechanism (via Heartbeat messages every 15s) prevents ICE disconnections
    // that would otherwise occur after ~30-40 seconds of inactivity due to RFC 5245 agent timeouts
    {
        peer_connection.on_ice_connection_state_change(Box::new(move |state| {
            let msg = match state {
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::New => "New",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Checking => "Checking",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Connected => "Connected",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Completed => "Completed",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Failed => "Failed",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Disconnected => "Disconnected",
                webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Closed => "Closed",
                _ => "Unknown",
            };
            log_to_file("ICE_STATE", &format!("ICE connection state changed to: {}", msg));
            info!("🧊 ICE connection state: {:?}", state);
            Box::pin(async {})
        }));
    }

    // Monitor ICE gathering state
    {
        peer_connection.on_ice_gathering_state_change(Box::new(move |state| {
            log_to_file(
                "ICE_GATHER",
                &format!("ICE gathering state changed: {:?}", state),
            );
            info!("🧊 ICE gathering state: {:?}", state);
            Box::pin(async {})
        }));
    }

    // Set up ICE candidate callback EARLY to catch candidates from client-side offer creation
    // This must be registered before any ICE gathering starts (including client offer creation)
    // FILTER: Only accept SRFLX candidates to force NAT traversal (blocks local HOST candidates)
    {
        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            Box::pin(async move {
                log_to_file("ICE_CALLBACK", &format!("on_ice_candidate callback fired, candidate is_some={}", candidate.is_some()));

                if let Some(candidate) = candidate {
                    // Log candidate type (host, srflx, prflx, relay)
                    let candidate_type_str = format!("{:?}", candidate.typ);
                    let candidate_type = match candidate_type_str.as_str() {
                        "Host" => "HOST (local IP)",
                        "Srflx" => "SRFLX (reflexive from STUN)",
                        "Prflx" => "PRFLX (peer reflexive)",
                        "Relay" => "RELAY (from TURN server)",
                        other => other,
                    };

                    // FILTER: Only accept SRFLX candidates, block HOST candidates
                    // This forces NAT traversal through the STUN server
                    if candidate_type_str == "Srflx" {
                        log_to_file(
                            "ICE_CANDIDATE",
                            &format!("✅ Accepting LOCAL ICE candidate: {} (address={})",
                                     candidate_type, candidate.address),
                        );
                        // Candidate will be added to local description
                    } else if candidate_type_str == "Host" {
                        log_to_file(
                            "ICE_CANDIDATE",
                            &format!("❌ BLOCKING HOST candidate (address={}) - forcing SRFLX only",
                                     candidate.address),
                        );
                        // Don't add this candidate - it will be ignored
                        return;
                    } else {
                        log_to_file(
                            "ICE_CANDIDATE",
                            &format!("📝 LOCAL ICE candidate: {} (address={})",
                                     candidate_type, candidate.address),
                        );
                    }
                } else {
                    // null candidate means gathering is complete
                    log_to_file("ICE_CANDIDATE", "✅ ICE candidate gathering complete (null candidate received)");
                }
            })
        }));
    }

    // Handle based on connection mode
    log_to_file("NETWORK_MODE_SELECT", &format!("Selecting connection mode"));
    match mode {
        ConnectionMode::Listen { .. } => {
            // Host mode: wait for offer from client
            log_to_file("NETWORK_MODE_HOST", "Entering host mode");
            info!("🎮 Host mode: waiting for client connection...");
            println!("\n🎮 Waiting for client to connect...");
            println!("📋 Your Peer ID: {}", peer_id);
            println!("   Share this with the client to connect!\n");

            log_to_file("NETWORK_CALLING_HOST_MODE", "About to call handle_host_mode()");
            handle_host_mode(
                peer_connection.clone(),
                &mut ws_sink,
                &mut ws_stream,
                data_channel.clone(),
                event_tx.clone(),
                peer_id.clone(),
            )
            .await?;
        }

        ConnectionMode::Connect { multiaddr } => {
            // Client mode: send offer to target peer
            let target_peer = multiaddr; // In our case, multiaddr is just the peer ID
            info!("🔌 Client mode: connecting to {}...", target_peer);

            handle_client_mode(
                peer_connection.clone(),
                &mut ws_sink,
                &mut ws_stream,
                data_channel.clone(),
                event_tx.clone(),
                peer_id.clone(),
                target_peer,
            )
            .await?;
        }
    }

    // Main message loop
    log_to_file("MAIN_LOOP", "Attempting to lock data channel");
    let data_channel_locked = data_channel.lock().await;
    log_to_file("MAIN_LOOP", "Data channel locked, checking if available");
    let dc = match data_channel_locked.as_ref() {
        Some(dc) => {
            log_to_file("MAIN_LOOP", "Data channel available, starting message loop");
            dc.clone()
        }
        None => return Err(anyhow!("Data channel not established")),
    };
    drop(data_channel_locked);

    // Handle incoming data channel messages
    {
        let event_tx = event_tx.clone();
        let dc_for_responses = dc.clone();
        dc.on_message(Box::new(move |msg| {
            let event_tx = event_tx.clone();
            let dc_for_responses = dc_for_responses.clone();
            Box::pin(async move {
                // Log receipt FIRST with timestamp
                use std::time::SystemTime;
                let timestamp = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                log_to_file(
                    "RECV_RAW",
                    &format!("Received message, size={} bytes [timestamp: {}]", msg.data.len(), timestamp),
                );

                if let Ok(network_msg) = NetworkMessage::from_bytes(&msg.data) {
                    // Log decoded message type
                    let msg_type = match &network_msg {
                        NetworkMessage::Input(_) => "Input",
                        NetworkMessage::BallSync(_) => "BallSync",
                        NetworkMessage::ScoreSync { .. } => "ScoreSync",
                        NetworkMessage::Ping { .. } => "Ping",
                        NetworkMessage::Pong { .. } => "Pong",
                        NetworkMessage::Heartbeat { .. } => "Heartbeat",
                        _ => "Other",
                    };
                    log_to_file("RECV_MSG", &format!("Decoded message: {} (size={} bytes)", msg_type, msg.data.len()));

                    match network_msg {
                        NetworkMessage::Input(action) => {
                            let _ = event_tx.send(NetworkEvent::ReceivedInput(action));
                        }
                        NetworkMessage::BallSync(ball_state) => {
                            let _ = event_tx.send(NetworkEvent::ReceivedBallState(ball_state));
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
                            // Auto-respond to ping with pong (for connection testing before game starts)
                            log_to_file("CONN_TEST", &format!("Received ping, sending pong with timestamp {}", timestamp_ms));
                            let dc_clone = dc_for_responses.clone();
                            let pong_msg = NetworkMessage::Pong { timestamp_ms };
                            if let Ok(bytes) = pong_msg.to_bytes() {
                                tokio::spawn(async move {
                                    match dc_clone.send(&bytes.into()).await {
                                        Ok(_) => {
                                            log_to_file("CONN_TEST_SENT", &format!("Pong sent successfully for timestamp {}", timestamp_ms));
                                        }
                                        Err(e) => {
                                            log_to_file("CONN_TEST_ERROR", &format!("Failed to send pong: {}", e));
                                        }
                                    }
                                });
                            } else {
                                log_to_file("CONN_TEST_ERROR", "Failed to serialize pong message");
                            }
                            let _ = event_tx.send(NetworkEvent::ReceivedPing { timestamp_ms });
                        }
                        NetworkMessage::Pong { timestamp_ms } => {
                            let _ = event_tx.send(NetworkEvent::ReceivedPong { timestamp_ms });
                        }
                        NetworkMessage::Heartbeat { sequence } => {
                            // Just silently acknowledge heartbeat - it's only for keepalive
                            log_to_file("HEARTBEAT_RECV", &format!("Received heartbeat #{} for connection keepalive", sequence));
                        }
                        NetworkMessage::Disconnect => {
                            let _ = event_tx.send(NetworkEvent::Disconnected);
                        }
                        _ => {}
                    }
                } else {
                    log_to_file("RECV_ERROR", &format!("Failed to decode message, size={} bytes, raw hex: {:?}", msg.data.len(), msg.data.to_vec()));
                }
            })
        }));
    }

    // Handle outgoing commands from game loop
    // Drain ALL queued messages before sleeping (prevents backlog buildup)
    let mut should_disconnect = false;
    loop {
        // Process all queued messages in one go
        let mut processed_any = false;
        while let Ok(cmd) = cmd_rx.try_recv() {
            processed_any = true;
            match cmd {
                NetworkCommand::SendInput(action) => {
                    let msg = NetworkMessage::Input(action);
                    if let Ok(bytes) = msg.to_bytes() {
                        log_to_file(
                            "SEND_INPUT",
                            &format!("Sending input: {:?}, size={} bytes", action, bytes.len()),
                        );

                        if let Err(e) = dc.send(&bytes.into()).await {
                            error!("Failed to send input: {}", e);
                            log_to_file("SEND_ERROR", &format!("Failed to send input: {}", e));
                        } else {
                            log_to_file("SEND_OK", "Input sent successfully");
                        }
                    }
                }
                NetworkCommand::SendMessage(msg) => {
                    // Log message type
                    let msg_type = match &msg {
                        NetworkMessage::BallSync(_) => "BallSync",
                        NetworkMessage::ScoreSync { .. } => "ScoreSync",
                        NetworkMessage::Ping { .. } => "Ping",
                        NetworkMessage::Pong { .. } => "Pong",
                        NetworkMessage::Heartbeat { .. } => "Heartbeat",
                        _ => "Other",
                    };

                    if let Ok(bytes) = msg.to_bytes() {
                        log_to_file(
                            "SEND_MSG",
                            &format!("Sending {}, size={} bytes", msg_type, bytes.len()),
                        );

                        if let Err(e) = dc.send(&bytes.into()).await {
                            error!("Failed to send message: {}", e);
                            log_to_file(
                                "SEND_ERROR",
                                &format!("Failed to send {}: {}", msg_type, e),
                            );
                        } else {
                            log_to_file("SEND_OK", &format!("{} sent successfully", msg_type));
                        }
                    }
                }
                NetworkCommand::Disconnect => {
                    info!("Disconnecting...");
                    should_disconnect = true;
                    break;
                }
            }
        }

        if should_disconnect {
            break;
        }

        // Only sleep if we didn't process anything (prevents busy-waiting when idle)
        if !processed_any {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    }

    Ok(())
}

async fn handle_host_mode(
    peer_connection: Arc<RTCPeerConnection>,
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
    data_channel: Arc<AsyncMutex<Option<Arc<RTCDataChannel>>>>,
    event_tx: mpsc::Sender<NetworkEvent>,
    peer_id: String,
) -> Result<()> {
    log_to_file("HOST_MODE", "handle_host_mode() started");
    // Set up data channel handler for incoming connections
    {
        let data_channel = data_channel.clone();
        let event_tx = event_tx.clone();
        peer_connection.on_data_channel(Box::new(move |dc| {
            let data_channel = data_channel.clone();
            let event_tx = event_tx.clone();
            Box::pin(async move {
                info!("📨 Data channel received: {}", dc.label());
                log_to_file(
                    "DC_RECEIVED",
                    &format!("Data channel received: {}", dc.label()),
                );

                // Log the received channel's configuration properties
                log_to_file(
                    "DC_CONFIG",
                    &format!("Ordered: {}, MaxRetransmits: {:?}, MaxPacketLifetime: {:?}",
                        dc.ordered(),
                        dc.max_retransmits(),
                        dc.max_packet_lifetime()
                    ),
                );

                // Check if data channel is already open
                let ready_state = dc.ready_state();
                log_to_file(
                    "DC_STATE",
                    &format!("Data channel ready state: {:?}", ready_state),
                );
                info!("📊 Data channel ready state: {:?}", ready_state);

                if ready_state
                    == webrtc::data_channel::data_channel_state::RTCDataChannelState::Open
                {
                    // Already open - send event immediately
                    log_to_file("DC_ALREADY_OPEN", "Data channel already open (host)");
                    info!("✅ Data channel already open");
                    let _ = event_tx.send(NetworkEvent::DataChannelOpened);

                    // Connection test removed - not reliable over double NAT
                    // The game will work fine with ordered=false config once we're in the game loop
                    log_to_file("DC_READY", "Data channel ready (already open state)");
                } else {
                    // Not open yet - set up on_open callback
                    let event_tx_open = event_tx.clone();
                    let dc_clone = dc.clone();
                    dc.on_open(Box::new(move || {
                        log_to_file(
                            "DC_ON_OPEN",
                            "Data channel on_open callback triggered (host)",
                        );
                        info!("✅ Data channel opened and ready");
                        let _ = event_tx_open.send(NetworkEvent::DataChannelOpened);

                        // Connection test removed - not reliable over double NAT
                        // The game will work fine with ordered=false config once we're in the game loop
                        log_to_file("DC_READY", "Data channel ready (on_open callback)");

                        Box::pin(async {})
                    }));
                }

                *data_channel.lock().await = Some(dc);
            })
        }));
    }

    // Wait for offer from client
    while let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;

        match msg {
            SignalingMessage::Offer { from, sdp, .. } => {
                info!("📥 Received offer from {}", from);

                // Set remote description
                let offer = RTCSessionDescription::offer(sdp)?;
                peer_connection.set_remote_description(offer).await?;

                // Create answer
                let answer = peer_connection.create_answer(None).await?;
                peer_connection
                    .set_local_description(answer.clone())
                    .await?;

                // Send answer back
                let answer_msg = SignalingMessage::Answer {
                    target: from,
                    from: peer_id.clone(),
                    sdp: answer.sdp,
                };
                ws_sink
                    .send(Message::Text(serde_json::to_string(&answer_msg)?))
                    .await?;

                info!("📤 Sent answer");
                break;
            }
            _ => {}
        }
    }

    // Handle ICE candidates
    log_to_file("HOST_MODE", "Calling handle_ice_candidates");
    handle_ice_candidates(peer_connection, ws_sink, ws_stream, peer_id).await?;
    log_to_file("HOST_MODE", "handle_ice_candidates returned");

    Ok(())
}

async fn handle_client_mode(
    peer_connection: Arc<RTCPeerConnection>,
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
    data_channel: Arc<AsyncMutex<Option<Arc<RTCDataChannel>>>>,
    event_tx: mpsc::Sender<NetworkEvent>,
    peer_id: String,
    target_peer: String,
) -> Result<()> {
    log_to_file("CLIENT_MODE", "handle_client_mode() started");
    // Create data channel optimized for low-latency gaming
    // - Unordered: Prevents head-of-line blocking when packets are lost
    // - Multiple retries: Allow 3 retransmits to ensure critical keepalive messages get through
    // This allows ICE keepalives to succeed while still maintaining low latency for game state
    let mut config = webrtc::data_channel::data_channel_init::RTCDataChannelInit::default();
    config.ordered = Some(false);          // Allow out-of-order delivery
    config.max_retransmits = Some(3);      // Allow 3 retransmits - ensures critical messages reach peer
                                           // For game state: newer updates replace older ones
                                           // so lossy transmission is acceptable

    let dc = peer_connection
        .create_data_channel("pong", Some(config))
        .await?;
    info!("📨 Created data channel (unordered, unreliable - optimized for low latency)");

    // Log the created channel's configuration properties
    log_to_file(
        "DC_CONFIG",
        &format!("Ordered: {}, MaxRetransmits: {:?}, MaxPacketLifetime: {:?}",
            dc.ordered(),
            dc.max_retransmits(),
            dc.max_packet_lifetime()
        ),
    );

    // Check if data channel is already open
    let ready_state = dc.ready_state();
    log_to_file(
        "DC_STATE",
        &format!("Data channel ready state: {:?}", ready_state),
    );
    info!("📊 Data channel ready state: {:?}", ready_state);

    if ready_state == webrtc::data_channel::data_channel_state::RTCDataChannelState::Open {
        // Already open - send event immediately
        log_to_file("DC_ALREADY_OPEN", "Data channel already open (client)");
        info!("✅ Data channel already open");
        let _ = event_tx.send(NetworkEvent::DataChannelOpened);

        // Connection test removed - not reliable over double NAT
        // The game will work fine with ordered=false config once we're in the game loop
        log_to_file("DC_READY", "Data channel ready (already open state)");
    } else {
        // Not open yet - set up on_open callback
        let event_tx_open = event_tx.clone();
        let dc_clone = dc.clone();
        dc.on_open(Box::new(move || {
            log_to_file(
                "DC_ON_OPEN",
                "Data channel on_open callback triggered (client)",
            );
            info!("✅ Data channel opened and ready");
            let _ = event_tx_open.send(NetworkEvent::DataChannelOpened);

            // Connection test removed - not reliable over double NAT
            // The game will work fine with ordered=false config once we're in the game loop
            log_to_file("DC_READY", "Data channel ready (on_open callback)");

            Box::pin(async {})
        }));
    }

    *data_channel.lock().await = Some(dc);

    // Create offer
    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer.clone()).await?;

    // Send offer to target
    let offer_msg = SignalingMessage::Offer {
        target: target_peer.clone(),
        from: peer_id.clone(),
        sdp: offer.sdp,
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&offer_msg)?))
        .await?;
    info!("📤 Sent offer to {}", target_peer);

    // Wait for answer
    while let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;

        match msg {
            SignalingMessage::Answer { sdp, .. } => {
                info!("📥 Received answer");

                let answer = RTCSessionDescription::answer(sdp)?;
                peer_connection.set_remote_description(answer).await?;
                break;
            }
            _ => {}
        }
    }

    // Handle ICE candidates
    log_to_file("CLIENT_MODE", "Calling handle_ice_candidates");
    handle_ice_candidates(peer_connection, ws_sink, ws_stream, peer_id).await?;
    log_to_file("CLIENT_MODE", "handle_ice_candidates returned");

    Ok(())
}

async fn handle_ice_candidates(
    peer_connection: Arc<RTCPeerConnection>,
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
    peer_id: String,
) -> Result<()> {
    info!("🧊 Starting ICE candidate exchange...");

    // Create channel to send ICE candidates from callback to main loop
    let (ice_tx, mut ice_rx) = tokio::sync::mpsc::unbounded_channel();
    let candidates_sent = Arc::new(tokio::sync::Mutex::new(false));

    // Set up ICE candidate handler to send local candidates to remote peer
    {
        let peer_id = peer_id.clone();
        let candidates_sent = candidates_sent.clone();

        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let ice_tx = ice_tx.clone();
            let peer_id = peer_id.clone();
            let candidates_sent = candidates_sent.clone();

            Box::pin(async move {
                log_to_file("ICE_CALLBACK", &format!("on_ice_candidate callback fired, candidate is_some={}", candidate.is_some()));

                if let Some(candidate) = candidate {
                    // Log candidate type (host, srflx, prflx, relay)
                    let candidate_type_str = format!("{:?}", candidate.typ);
                    let candidate_type = match candidate_type_str.as_str() {
                        "Host" => "HOST (local IP)",
                        "Srflx" => "SRFLX (reflexive from STUN)",
                        "Prflx" => "PRFLX (peer reflexive)",
                        "Relay" => "RELAY (from TURN server)",
                        other => other,
                    };

                    log_to_file(
                        "ICE_CANDIDATE",
                        &format!("Local ICE candidate: {} (address={})",
                                 candidate_type, candidate.address),
                    );

                    // Convert candidate to JSON
                    match candidate.to_json() {
                        Ok(init) => {
                            let candidate_json = match serde_json::to_string(&init) {
                                Ok(json) => json,
                                Err(e) => {
                                    error!("Failed to serialize ICE candidate: {}", e);
                                    return;
                                }
                            };

                            debug!("🧊 Local ICE candidate ({}): {}", candidate_type, candidate.address);

                            let msg = SignalingMessage::IceCandidate {
                                target: "remote".to_string(),
                                from: peer_id.clone(),
                                candidate: candidate_json,
                            };

                            let _ = ice_tx.send(msg);
                        }
                        Err(e) => {
                            error!("Failed to convert ICE candidate to JSON: {}", e);
                        }
                    }
                } else {
                    // null candidate means gathering is complete
                    *candidates_sent.lock().await = true;
                    log_to_file("ICE_CANDIDATE", "✅ ICE candidate gathering complete (null candidate received)");
                    info!("✅ ICE candidate gathering complete");
                }
            })
        }));
    }

    // Receive and relay ICE candidates
    // Check completion at each iteration instead of waiting for a long timeout
    let start_time = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(5);

    let mut remote_candidates_received = 0;

    // Wait minimally before completing
    let completion_wait = Duration::from_millis(300);

    loop {
        let candidates_sent = *candidates_sent.lock().await;
        let elapsed = start_time.elapsed();

        // Complete if:
        // 1. We've waited minimum time (to allow initial ICE exchange)
        // 2. Hard timeout reached
        if elapsed > completion_wait {
            log_to_file("ICE_COMPLETE_MIN_WAIT", &format!("Minimum wait elapsed, candidates_sent={}, remote_received={}", candidates_sent, remote_candidates_received));
            break;
        }

        if start_time.elapsed() > max_wait {
            log_to_file("ICE_TIMEOUT", &format!("Hard timeout reached: candidates_sent={}, remote_received={}", candidates_sent, remote_candidates_received));
            break;
        }

        // Calculate remaining time to wait
        let remaining = completion_wait.saturating_sub(elapsed);
        let timeout_duration = Duration::from_millis(50).min(remaining);

        // Short timeout for select to allow responsive completion checking
        let select_timeout = tokio::time::sleep(timeout_duration);
        tokio::pin!(select_timeout);

        tokio::select! {
            // Send local ICE candidates via WebSocket
            Some(msg) = ice_rx.recv() => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if let Err(e) = ws_sink.send(Message::Text(json)).await {
                        error!("Failed to send ICE candidate: {}", e);
                    }
                }
            }

            // Receive remote ICE candidates
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SignalingMessage>(&text) {
                            Ok(SignalingMessage::IceCandidate { candidate, .. }) => {
                                match serde_json::from_str::<RTCIceCandidateInit>(&candidate) {
                                    Ok(init) => {
                                        remote_candidates_received += 1;
                                        log_to_file("ICE_RECV", &format!("Remote ICE candidate #{}", remote_candidates_received));
                                        debug!("🧊 Remote ICE candidate received ({})", remote_candidates_received);

                                        if let Err(e) = peer_connection.add_ice_candidate(init).await {
                                            warn!("Failed to add ICE candidate: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        log_to_file("ICE_PARSE_ERROR", &format!("Failed to parse ICE candidate: {}", e));
                                        warn!("Failed to parse ICE candidate: {}", e);
                                    }
                                }
                            }
                            Ok(_) => {
                                // Ignore other message types during ICE exchange
                            }
                            Err(e) => {
                                warn!("Failed to parse signaling message: {}", e);
                            }
                        }
                    }
                    Some(Ok(_)) => {
                        // Ignore non-text messages
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error during ICE exchange: {}", e);
                    }
                    None => {
                        warn!("WebSocket closed during ICE exchange");
                        break;
                    }
                }
            }

            _ = &mut select_timeout => {
                // Short timeout hit - loop will check completion condition again
            }
        }
    }

    // Give a small grace period for any trailing candidates
    tokio::time::sleep(Duration::from_millis(50)).await;
    log_to_file("ICE_COMPLETE", &format!("ICE candidate exchange complete, sent={}, received={}", *candidates_sent.lock().await, remote_candidates_received));
    info!("✅ ICE candidate exchange complete");

    info!("🔌 ICE negotiation complete, waiting for connection...");
    log_to_file("ICE_DONE", "handle_ice_candidates() returning");

    Ok(())
}
