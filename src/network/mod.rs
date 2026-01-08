// P2P networking module for P2Pong
// Handles libp2p connection, message passing, and game synchronization

pub mod protocol;
pub mod client;
pub mod simple_behaviour;
pub mod runtime;

pub use client::{NetworkClient, ConnectionMode};
pub use protocol::NetworkMessage;

use std::io;
use std::sync::mpsc;

/// Initialize and start the network layer
/// Returns a NetworkClient handle for the game loop to communicate with
pub fn start_network(mode: ConnectionMode) -> io::Result<NetworkClient> {
    // Create channels for bidirectional communication
    let (event_tx, event_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    
    // Spawn network thread with libp2p runtime
    runtime::spawn_network_thread(mode, event_tx, cmd_rx)?;
    
    // Return client handle for game loop
    Ok(NetworkClient::new(cmd_tx, event_rx))
}
