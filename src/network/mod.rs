// P2P networking module for P2Pong
// Handles libp2p connection, message passing, and game synchronization

pub mod protocol;
pub mod client;

pub use client::{NetworkClient, ConnectionMode};
pub use protocol::NetworkMessage;

use crate::game::InputAction;
use std::io;

/// Initialize and start the network layer
/// Returns a NetworkClient handle for the game loop to communicate with
pub fn start_network(mode: ConnectionMode) -> io::Result<NetworkClient> {
    // TODO: Implementation in Day 2-3
    todo!("Network initialization will be implemented in Day 2")
}
