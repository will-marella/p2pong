// Configuration module for P2Pong
// Handles loading and managing game configuration from TOML file

pub mod loader;
pub mod types;

pub use loader::{create_default_config, get_config_path, load_config};
pub use types::{AIConfig, Config, DisplayConfig, KeyBindings, NetworkConfig, PhysicsConfig};
