// P2Pong network protocol definition
// Messages exchanged over WebRTC data channels

use crate::game::InputAction;
use serde::{Deserialize, Serialize};

/// Ball state for synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub sequence: u64, // Monotonic sequence number to detect old/duplicate updates
    pub timestamp_ms: u64, // Timestamp for latency measurement
}

/// Messages exchanged between peers during gameplay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Player input action
    Input(InputAction),

    /// Ball physics state (sent by host)
    BallSync(BallState),

    /// Score update from host (authoritative)
    ScoreSync {
        left: u8,
        right: u8,
        game_over: bool,
    },

    /// Handshake message sent on connection
    Hello { peer_name: String },

    /// RTT measurement request
    Ping { timestamp_ms: u64 },

    /// RTT measurement response
    Pong { timestamp_ms: u64 },

    /// Connection keepalive (sent periodically to maintain ICE connection)
    /// Contains a simple counter to verify bidirectional delivery
    Heartbeat { sequence: u32 },

    /// Request to rematch (reset game)
    RematchRequest,

    /// Confirm that both players are ready to rematch
    RematchConfirm,

    /// Request to quit and return to menu
    QuitRequest,

    /// Graceful disconnect
    Disconnect,
}

impl NetworkMessage {
    /// Serialize message to bytes for transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = NetworkMessage::Input(InputAction::LeftPaddleUp);
        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        match decoded {
            NetworkMessage::Input(InputAction::LeftPaddleUp) => {}
            _ => panic!("Message didn't round-trip correctly"),
        }
    }

    #[test]
    fn test_heartbeat_serialization() {
        let msg = NetworkMessage::Heartbeat { sequence: 42 };
        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        match decoded {
            NetworkMessage::Heartbeat { sequence: 42 } => {}
            _ => panic!("Heartbeat didn't round-trip correctly, got: {:?}", decoded),
        }
    }

    #[test]
    fn test_all_message_sizes() {
        let messages = vec![
            ("Input", NetworkMessage::Input(InputAction::LeftPaddleUp)),
            (
                "Ping",
                NetworkMessage::Ping {
                    timestamp_ms: 12345,
                },
            ),
            (
                "Pong",
                NetworkMessage::Pong {
                    timestamp_ms: 12345,
                },
            ),
            ("Heartbeat", NetworkMessage::Heartbeat { sequence: 0 }),
            (
                "BallSync",
                NetworkMessage::BallSync(BallState {
                    x: 1.0,
                    y: 2.0,
                    vx: 3.0,
                    vy: 4.0,
                    sequence: 0,
                    timestamp_ms: 0,
                }),
            ),
        ];

        for (name, msg) in messages {
            let bytes = msg.to_bytes().unwrap();
            let _ = (name, bytes); // Verify serialization doesn't panic
        }
    }
}
