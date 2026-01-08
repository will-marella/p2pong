// P2Pong custom libp2p protocol definition
// Protocol: /p2pong/1.0.0

use crate::game::InputAction;
use serde::{Serialize, Deserialize};

/// Protocol identifier for libp2p
pub const PROTOCOL_ID: &str = "/p2pong/1.0.0";

/// Ball state for synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
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
    Hello {
        peer_name: String,
    },
    
    /// Acknowledge ready to start game
    Ready,
    
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
            NetworkMessage::Input(InputAction::LeftPaddleUp) => {},
            _ => panic!("Message didn't round-trip correctly"),
        }
    }
}
