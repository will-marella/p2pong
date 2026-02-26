// P2Pong library - shared modules for game client and server

pub mod config;
pub mod debug;
pub mod game;
pub mod network;

// Re-export commonly used types
pub use game::{GameState, InputAction, Player};

// Fixed timestep for deterministic physics
pub const FIXED_TIMESTEP: f32 = 1.0 / 60.0;
