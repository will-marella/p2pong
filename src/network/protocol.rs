// P2Pong custom libp2p protocol definition
// Protocol: /p2pong/1.0.0

use crate::game::InputAction;
use serde::{Serialize, Deserialize};

/// Protocol identifier for libp2p
pub const PROTOCOL_ID: &str = "/p2pong/1.0.0";

/// Messages exchanged between peers during gameplay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Player input action
    /// Sent every frame when player presses a key
    Input(InputAction),
    
    /// Handshake message sent on connection
    /// TODO: Add player info, version, etc.
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
