// P2P networking module for P2Pong
// Handles libp2p connection, message passing, and game synchronization

pub mod behaviour;
pub mod client;
pub mod protocol;
pub mod runtime;

pub use client::{ConnectionMode, NetworkClient};
pub use protocol::{BallState, NetworkMessage};

use std::io;
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};

/// Initialize and start the network layer
/// Returns a NetworkClient handle for the game loop to communicate with
pub fn start_network(mode: ConnectionMode) -> io::Result<NetworkClient> {
    // Create channels for bidirectional communication
    let (event_tx, event_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();

    // Create shared connection state flag
    let connected = Arc::new(AtomicBool::new(false));

    // Spawn network thread with libp2p runtime
    runtime::spawn_network_thread(mode, event_tx, cmd_rx, connected.clone())?;

    // Return client handle for game loop
    Ok(NetworkClient::new(cmd_tx, event_rx, connected))
}
