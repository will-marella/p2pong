// WebRTC Signaling Server
// Relays SDP offers/answers between peers for WebRTC connection establishment
//
// Usage: cargo run --bin signaling-server

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

type PeerId = String;
type PeerConnections = Arc<RwLock<HashMap<PeerId, tokio::sync::mpsc::UnboundedSender<Message>>>>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SignalingMessage {
    /// Register a peer with the signaling server
    Register { peer_id: PeerId },

    /// Server response to registration
    RegisterOk { peer_id: PeerId },

    /// List available peers
    ListPeers,

    /// Response with list of peers
    PeerList { peers: Vec<PeerId> },

    /// Send an SDP offer to a peer
    Offer {
        target: PeerId,
        from: PeerId,
        sdp: String,
    },

    /// Send an SDP answer to a peer
    Answer {
        target: PeerId,
        from: PeerId,
        sdp: String,
    },

    /// Send an ICE candidate to a peer
    IceCandidate {
        target: PeerId,
        from: PeerId,
        candidate: String,
    },

    /// Error response
    Error { message: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(&addr).await?;
    info!("🚀 Signaling server listening on {}", addr);

    let peers: PeerConnections = Arc::new(RwLock::new(HashMap::new()));

    while let Ok((stream, addr)) = listener.accept().await {
        let peers = peers.clone();
        tokio::spawn(handle_connection(stream, addr, peers));
    }

    Ok(())
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr, peers: PeerConnections) {
    info!("📥 New connection from {}", addr);

    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut peer_id: Option<PeerId> = None;

    // Spawn task to send messages to this peer
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("Error receiving message: {}", e);
                break;
            }
        };

        if let Message::Text(text) = msg {
            match serde_json::from_str::<SignalingMessage>(&text) {
                Ok(signal_msg) => {
                    handle_signaling_message(signal_msg, &mut peer_id, &tx, &peers, addr).await;
                }
                Err(e) => {
                    warn!("Failed to parse message: {}", e);
                    let error_msg = SignalingMessage::Error {
                        message: format!("Invalid message format: {}", e),
                    };
                    if let Ok(json) = serde_json::to_string(&error_msg) {
                        let _ = tx.send(Message::Text(json));
                    }
                }
            }
        }
    }

    // Clean up on disconnect
    if let Some(id) = peer_id {
        peers.write().await.remove(&id);
        info!("📤 Peer {} disconnected", id);
    }

    send_task.abort();
}

async fn handle_signaling_message(
    msg: SignalingMessage,
    peer_id: &mut Option<PeerId>,
    tx: &tokio::sync::mpsc::UnboundedSender<Message>,
    peers: &PeerConnections,
    addr: SocketAddr,
) {
    match msg {
        SignalingMessage::Register { peer_id: new_id } => {
            info!("✅ Peer registered: {} from {}", new_id, addr);
            *peer_id = Some(new_id.clone());
            peers.write().await.insert(new_id.clone(), tx.clone());

            let response = SignalingMessage::RegisterOk { peer_id: new_id };
            if let Ok(json) = serde_json::to_string(&response) {
                let _ = tx.send(Message::Text(json));
            }
        }

        SignalingMessage::ListPeers => {
            let peer_list: Vec<PeerId> = peers.read().await.keys().cloned().collect();
            info!(
                "📋 Peer list requested, {} peers available",
                peer_list.len()
            );

            let response = SignalingMessage::PeerList { peers: peer_list };
            if let Ok(json) = serde_json::to_string(&response) {
                let _ = tx.send(Message::Text(json));
            }
        }

        SignalingMessage::Offer { target, from, sdp } => {
            info!("📨 Relaying offer from {} to {}", from, target);
            relay_message(
                peers,
                &target,
                SignalingMessage::Offer {
                    target: target.clone(),
                    from,
                    sdp,
                },
            )
            .await;
        }

        SignalingMessage::Answer { target, from, sdp } => {
            info!("📨 Relaying answer from {} to {}", from, target);
            relay_message(
                peers,
                &target,
                SignalingMessage::Answer {
                    target: target.clone(),
                    from,
                    sdp,
                },
            )
            .await;
        }

        SignalingMessage::IceCandidate {
            target,
            from,
            candidate,
        } => {
            info!("🧊 Relaying ICE candidate from {} to {}", from, target);
            relay_message(
                peers,
                &target,
                SignalingMessage::IceCandidate {
                    target: target.clone(),
                    from,
                    candidate,
                },
            )
            .await;
        }

        _ => {
            warn!("Unhandled message type");
        }
    }
}

async fn relay_message(peers: &PeerConnections, target: &str, msg: SignalingMessage) {
    let peers_lock = peers.read().await;
    if let Some(peer_tx) = peers_lock.get(target) {
        if let Ok(json) = serde_json::to_string(&msg) {
            if peer_tx.send(Message::Text(json)).is_err() {
                error!("Failed to send message to peer {}", target);
            }
        }
    } else {
        warn!("Target peer {} not found", target);
    }
}
