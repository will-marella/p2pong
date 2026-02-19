// Configuration module for P2Pong
// Handles loading and managing game configuration from TOML file

pub mod loader;
pub mod types;

pub use loader::load_config;
pub use types::{Config, PhysicsConfig};
