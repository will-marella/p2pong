// WebRTC Signaling Server
// Relays SDP offers/answers between peers for WebRTC connection establishment
//
// Usage: cargo run --bin signaling-server

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// Axum HTTP server for WebSocket upgrades
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo,
    },
    response::IntoResponse,
    routing::get,
    Router,
};

type PeerId = String;
type PeerConnections = Arc<RwLock<HashMap<PeerId, tokio::sync::mpsc::UnboundedSender<Message>>>>;
type PeerPairings = Arc<RwLock<HashMap<PeerId, PeerId>>>;

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

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    // Shared state for peer connections and pairings
    let peers: PeerConnections = Arc::new(RwLock::new(HashMap::new()));
    let pairings: PeerPairings = Arc::new(RwLock::new(HashMap::new()));

    // Build Axum router with WebSocket upgrade handler
    let app = Router::new()
        .route("/", get(websocket_handler))
        .with_state((peers, pairings));

    // Create TCP listener for Railway deployment
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("🚀 Signaling server listening on {}", addr);

    // Run Axum server
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// HTTP handler for WebSocket upgrade requests
async fn websocket_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::extract::State((peers, pairings)): axum::extract::State<(PeerConnections, PeerPairings)>,
) -> impl IntoResponse {
    info!("📥 WebSocket upgrade request from {}", addr);
    ws.on_upgrade(move |socket| handle_websocket(socket, addr, peers, pairings))
}

async fn handle_websocket(
    socket: WebSocket,
    addr: SocketAddr,
    peers: PeerConnections,
    pairings: PeerPairings,
) {
    info!("✅ WebSocket connection established from {}", addr);

    let (mut ws_sender, mut ws_receiver) = socket.split();
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
                // Connection reset after signaling is normal, don't log as error
                let error_str = e.to_string();
                if error_str.contains("Connection reset") && peer_id.is_some() {
                    info!("📤 Peer {} disconnected (signaling complete)", peer_id.as_ref().unwrap());
                } else {
                    error!("Error receiving message: {}", e);
                }
                break;
            }
        };

        // Handle close frames gracefully
        if let Message::Close(_) = msg {
            if let Some(id) = &peer_id {
                info!("📤 Peer {} requested close", id);
            } else {
                info!("📤 Connection closed from {}", addr);
            }
            break;
        }

        if let Message::Text(text) = msg {
            match serde_json::from_str::<SignalingMessage>(&text) {
                Ok(signal_msg) => {
                    handle_signaling_message(
                        signal_msg,
                        &mut peer_id,
                        &tx,
                        &peers,
                        &pairings,
                        addr,
                    )
                    .await;
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
        pairings.write().await.remove(&id);
        info!("📤 Peer {} disconnected", id);
    }

    send_task.abort();
}

async fn handle_signaling_message(
    msg: SignalingMessage,
    peer_id: &mut Option<PeerId>,
    tx: &tokio::sync::mpsc::UnboundedSender<Message>,
    peers: &PeerConnections,
    pairings: &PeerPairings,
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

            // Track pairing
            pairings.write().await.insert(from.clone(), target.clone());
            pairings.write().await.insert(target.clone(), from.clone());

            relay_message(
                peers,
                &target,
                SignalingMessage::Offer {
                    target: target.clone(),
                    from,
                    sdp,
                },
                tx,
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
                tx,
            )
            .await;
        }

        SignalingMessage::IceCandidate {
            mut target,
            from,
            candidate,
        } => {
            // Resolve "remote" to actual peer ID
            if target == "remote" {
                if let Some(paired_peer) = pairings.read().await.get(&from) {
                    target = paired_peer.clone();
                } else {
                    warn!("Cannot find paired peer for {}", from);
                    return;
                }
            }

            debug!("🧊 Relaying ICE candidate from {} to {}", from, target);
            relay_message(
                peers,
                &target,
                SignalingMessage::IceCandidate {
                    target: target.clone(),
                    from,
                    candidate,
                },
                tx,
            )
            .await;
        }

        _ => {
            warn!("Unhandled message type");
        }
    }
}

async fn relay_message(
    peers: &PeerConnections,
    target: &str,
    msg: SignalingMessage,
    sender_tx: &tokio::sync::mpsc::UnboundedSender<Message>,
) {
    let peers_lock = peers.read().await;
    if let Some(peer_tx) = peers_lock.get(target) {
        if let Ok(json) = serde_json::to_string(&msg) {
            if peer_tx.send(Message::Text(json)).is_err() {
                error!("Failed to send message to peer {}", target);
            }
        }
    } else {
        warn!("Target peer {} not found, notifying sender", target);

        // Send error back to sender
        let error_msg = SignalingMessage::Error {
            message: format!("Peer {} not found", target),
        };

        if let Ok(json) = serde_json::to_string(&error_msg) {
            let _ = sender_tx.send(Message::Text(json));
        }
    }
}
