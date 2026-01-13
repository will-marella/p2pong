// Network client interface for the game loop
// Provides channels to communicate with the libp2p network thread

use super::{protocol::BallState, NetworkMessage};
use crate::game::InputAction;
use std::io;
use std::sync::mpsc;

/// Connection mode for the network layer
#[derive(Debug, Clone)]
pub enum ConnectionMode {
    /// Listen for incoming connections (Host)
    Listen,

    /// Connect to a specific peer (Client)
    /// The "multiaddr" is now just the peer ID to connect to
    Connect { multiaddr: String },
}

/// Handle for the game loop to communicate with the network
/// Uses channels to send/receive messages to/from the async network thread
pub struct NetworkClient {
    /// Send messages TO the network thread
    tx: mpsc::Sender<NetworkCommand>,

    /// Receive messages FROM the network thread
    rx: mpsc::Receiver<NetworkEvent>,

    /// Connection state
    connected: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Commands the game loop sends to the network thread
#[derive(Debug)]
pub enum NetworkCommand {
    /// Send an input action to the opponent
    SendInput(InputAction),

    /// Send a network message (for ball sync, etc.)
    SendMessage(NetworkMessage),

    /// Gracefully disconnect
    Disconnect,
}

/// Events the network thread sends to the game loop
#[derive(Debug)]
pub enum NetworkEvent {
    /// Received input from opponent
    ReceivedInput(InputAction),

    /// Received ball state from host
    ReceivedBallState(BallState),

    /// Received score update from host (authoritative)
    ReceivedScore {
        left: u8,
        right: u8,
        game_over: bool,
    },

    /// Received ping request (respond with pong)
    ReceivedPing { timestamp_ms: u64 },

    /// Received pong response (calculate RTT)
    ReceivedPong { timestamp_ms: u64 },

    /// Received rematch request from opponent
    ReceivedRematchRequest,

    /// Received rematch confirmation (both players ready)
    ReceivedRematchConfirm,

    /// Received quit request from opponent
    ReceivedQuitRequest,

    /// Local peer ID is ready (for host to display)
    LocalPeerIdReady { peer_id: String },

    /// Successfully connected to peer
    Connected { peer_id: String },

    /// Data channel opened and ready for messages
    DataChannelOpened,

    /// Peer disconnected
    Disconnected,

    /// Network error occurred
    Error(String),
}

impl NetworkClient {
    /// Create a new network client (called by start_network)
    pub fn new(
        tx: mpsc::Sender<NetworkCommand>,
        rx: mpsc::Receiver<NetworkEvent>,
        connected: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self { tx, rx, connected }
    }

    /// Check if connected to a peer
    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Send an input action to the opponent
    pub fn send_input(&self, action: InputAction) -> io::Result<()> {
        self.tx
            .send(NetworkCommand::SendInput(action))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    /// Send a network message (for ball sync, etc.)
    pub fn send_message(&self, msg: NetworkMessage) -> io::Result<()> {
        self.tx
            .send(NetworkCommand::SendMessage(msg))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    /// Try to receive network events (non-blocking)
    /// Returns None if no events available
    pub fn try_recv_event(&self) -> Option<NetworkEvent> {
        self.rx.try_recv().ok()
    }

    /// Get all pending remote inputs (non-blocking)
    /// Note: This is deprecated - prefer using try_recv_event() directly in game loop
    pub fn recv_inputs(&self) -> Vec<InputAction> {
        let mut inputs = Vec::new();

        while let Some(event) = self.try_recv_event() {
            match event {
                NetworkEvent::ReceivedInput(action) => inputs.push(action),
                NetworkEvent::ReceivedBallState(_ball_state) => {
                    // Skip ball state events - should be handled in main game loop
                }
                NetworkEvent::ReceivedScore { .. } => {
                    // Skip score events - should be handled in main game loop
                }
                NetworkEvent::ReceivedPing { .. } => {
                    // Skip ping events - should be handled in main game loop
                }
                NetworkEvent::ReceivedPong { .. } => {
                    // Skip pong events - should be handled in main game loop
                }
                NetworkEvent::ReceivedRematchRequest => {
                    // Rematch requests handled in main game loop
                }
                NetworkEvent::ReceivedRematchConfirm => {
                    // Rematch confirmations handled in main game loop
                }
                NetworkEvent::ReceivedQuitRequest => {
                    // Quit requests handled in main game loop
                }
                NetworkEvent::LocalPeerIdReady { .. } => {
                    // Local peer ID - handled in wait_for_connection
                }
                NetworkEvent::Connected { .. } => {
                    // Connection events handled by main game loop
                }
                NetworkEvent::DataChannelOpened => {
                    // Data channel ready - handled in wait_for_connection
                }
                NetworkEvent::Disconnected => {
                    // Disconnection handled by main game loop
                }
                NetworkEvent::Error(_) => {
                    // Error events handled by main game loop
                }
            }
        }

        inputs
    }

    /// Gracefully disconnect from peer
    pub fn disconnect(&self) -> io::Result<()> {
        self.tx
            .send(NetworkCommand::Disconnect)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }
}
