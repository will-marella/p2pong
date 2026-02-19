// P2P networking module for P2Pong
// Handles WebRTC connection, message passing, and game synchronization

pub mod client;
pub mod protocol;
pub mod webrtc_runtime;

pub use client::{ConnectionMode, NetworkClient};
pub use protocol::{BallState, NetworkMessage};

use std::io;
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};

/// Initialize and start the network layer
/// Returns a NetworkClient handle for the game loop to communicate with
pub fn start_network(mode: ConnectionMode, signaling_server: String) -> io::Result<NetworkClient> {
    // Create channels for bidirectional communication
    let (event_tx, event_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();

    // Create shared connection state flag (used by network thread to track state)
    let connected = Arc::new(AtomicBool::new(false));

    // Spawn network thread with WebRTC runtime
    webrtc_runtime::spawn_network_thread(mode, event_tx, cmd_rx, connected, signaling_server)?;

    // Return client handle for game loop
    Ok(NetworkClient::new(cmd_tx, event_rx))
}
