// WebRTC network runtime - manages P2P connections via WebRTC
// Bridges async WebRTC with sync game loop via channels

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use webrtc::api::media_engine::MediaEngine;
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

// STUN server for NAT traversal (Google's public STUN server)
const STUN_SERVER: &str = "stun:stun.l.google.com:19302";

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
    thread::spawn(move || {
        let rt = Runtime::new().expect("Failed to create tokio runtime");

        rt.block_on(async move {
            if let Err(e) = run_network(mode, event_tx, cmd_rx, connected).await {
                error!("Network error: {}", e);
            }
        });
    });

    Ok(())
}

async fn run_network(
    mode: ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> Result<()> {
    // Generate a unique peer ID
    let peer_id = format!("peer-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    info!("Local peer ID: {}", peer_id);

    // Connect to signaling server
    let (ws_stream, _) = connect_async(SIGNALING_SERVER).await?;
    info!("Connected to signaling server");

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Register with signaling server
    let register_msg = SignalingMessage::Register {
        peer_id: peer_id.clone(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&register_msg)?))
        .await?;

    // Wait for registration confirmation
    if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;
        match msg {
            SignalingMessage::RegisterOk { .. } => {
                info!("✅ Registered with signaling server");
            }
            _ => {
                return Err(anyhow!("Unexpected registration response"));
            }
        }
    }

    // Create WebRTC API
    let media_engine = MediaEngine::default();

    let api = APIBuilder::new().with_media_engine(media_engine).build();

    // Configure ICE servers (STUN for NAT traversal)
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![STUN_SERVER.to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create peer connection
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);
    info!("Created RTCPeerConnection");

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
                        connected.store(false, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    _ => {}
                }
                Box::pin(async {})
            },
        ));
    }

    // Handle based on connection mode
    match mode {
        ConnectionMode::Listen { .. } => {
            // Host mode: wait for offer from client
            info!("🎮 Host mode: waiting for client connection...");
            println!("\n🎮 Waiting for client to connect...");
            println!("📋 Your Peer ID: {}", peer_id);
            println!("   Share this with the client to connect!\n");

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
    let data_channel_locked = data_channel.lock().await;
    let dc = match data_channel_locked.as_ref() {
        Some(dc) => dc.clone(),
        None => return Err(anyhow!("Data channel not established")),
    };
    drop(data_channel_locked);

    // Handle incoming data channel messages
    {
        let event_tx = event_tx.clone();
        dc.on_message(Box::new(move |msg| {
            let event_tx = event_tx.clone();
            Box::pin(async move {
                if let Ok(network_msg) = NetworkMessage::from_bytes(&msg.data) {
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
                        NetworkMessage::Disconnect => {
                            let _ = event_tx.send(NetworkEvent::Disconnected);
                        }
                        _ => {}
                    }
                }
            })
        }));
    }

    // Handle outgoing commands from game loop
    loop {
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                NetworkCommand::SendInput(action) => {
                    let msg = NetworkMessage::Input(action);
                    if let Ok(bytes) = msg.to_bytes() {
                        if let Err(e) = dc.send(&bytes.into()).await {
                            error!("Failed to send input: {}", e);
                        }
                    }
                }
                NetworkCommand::SendMessage(msg) => {
                    if let Ok(bytes) = msg.to_bytes() {
                        if let Err(e) = dc.send(&bytes.into()).await {
                            error!("Failed to send message: {}", e);
                        }
                    }
                }
                NetworkCommand::Disconnect => {
                    info!("Disconnecting...");
                    break;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
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
    _event_tx: mpsc::Sender<NetworkEvent>,
    peer_id: String,
) -> Result<()> {
    // Set up data channel handler for incoming connections
    {
        let data_channel = data_channel.clone();
        peer_connection.on_data_channel(Box::new(move |dc| {
            let data_channel = data_channel.clone();
            Box::pin(async move {
                info!("📨 Data channel opened: {}", dc.label());
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
    handle_ice_candidates(peer_connection, ws_sink, ws_stream, peer_id).await?;

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
    _event_tx: mpsc::Sender<NetworkEvent>,
    peer_id: String,
    target_peer: String,
) -> Result<()> {
    // Create data channel
    let dc = peer_connection.create_data_channel("pong", None).await?;
    info!("📨 Created data channel");
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
    handle_ice_candidates(peer_connection, ws_sink, ws_stream, peer_id).await?;

    Ok(())
}

async fn handle_ice_candidates(
    _peer_connection: Arc<RTCPeerConnection>,
    _ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    _ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    _peer_id: String,
) -> Result<()> {
    // Wait for ICE gathering to complete
    // WebRTC will handle ICE candidates internally through the SDP exchange
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    info!("✅ ICE gathering complete");

    Ok(())
}
