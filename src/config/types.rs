// P2Pong configuration types
// All settings with sensible defaults matching current hardcoded values

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub keybindings: KeyBindings,
    #[serde(default)]
    pub physics: PhysicsConfig,
    #[serde(default)]
    pub ai: AIConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub network: NetworkConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keybindings: KeyBindings::default(),
            physics: PhysicsConfig::default(),
            ai: AIConfig::default(),
            display: DisplayConfig::default(),
            network: NetworkConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeyBindings {
    // Player paddle controls (single-player modes: AI, Network)
    pub player_paddle_up: String,
    pub player_paddle_down: String,

    // Left paddle controls (local 2-player mode - left player)
    pub left_paddle_up: String,
    pub left_paddle_down: String,

    // Right paddle controls (local 2-player mode - right player)
    pub right_paddle_up: String,
    pub right_paddle_down: String,

    // Game controls
    pub quit: String,
    pub pause: String, // Future: pause functionality

    // Menu controls
    pub menu_up: String,
    pub menu_down: String,
    pub menu_select: String,
    pub menu_back: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            player_paddle_up: "W".to_string(),
            player_paddle_down: "S".to_string(),
            left_paddle_up: "W".to_string(),
            left_paddle_down: "S".to_string(),
            right_paddle_up: "Up".to_string(),
            right_paddle_down: "Down".to_string(),
            quit: "Q".to_string(),
            pause: "P".to_string(),
            menu_up: "Up".to_string(),
            menu_down: "Down".to_string(),
            menu_select: "Enter".to_string(),
            menu_back: "Esc".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PhysicsConfig {
    // Ball speed in virtual units per second
    pub ball_initial_speed: f32,

    // Paddle height in virtual units
    pub paddle_height: f32,

    // Paddle movement distance per tap
    pub paddle_tap_distance: f32,

    // Score required to win
    pub winning_score: u8,

    // Ball speed increase multiplier on paddle hit (1.1 = 10% increase)
    pub ball_speed_multiplier: f32,

    // Virtual field dimensions (changing these affects game feel)
    pub virtual_width: f32,
    pub virtual_height: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            ball_initial_speed: 600.0,
            paddle_height: 90.0,
            paddle_tap_distance: 40.0,
            winning_score: 5,
            ball_speed_multiplier: 1.1,
            virtual_width: 1200.0,
            virtual_height: 600.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AIConfig {
    // AI difficulty: "easy", "medium", "hard"
    pub difficulty: String,

    // AI reaction delay in milliseconds (higher = easier to beat)
    pub reaction_delay_ms: u64,

    // AI prediction error (0.0 = perfect, 1.0 = very inaccurate)
    pub prediction_error: f32,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            difficulty: "medium".to_string(),
            reaction_delay_ms: 100,
            prediction_error: 0.2,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayConfig {
    // Target frames per second
    pub target_fps: u64,

    // Score display color (RGB values 0-255)
    pub score_color: [u8; 3],

    // Paddle color
    pub paddle_color: [u8; 3],

    // Ball color
    pub ball_color: [u8; 3],

    // Center line color
    pub center_line_color: [u8; 3],
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            score_color: [255, 255, 255],       // White
            paddle_color: [255, 255, 255],      // White
            ball_color: [255, 255, 255],        // White
            center_line_color: [100, 100, 100], // Gray
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfig {
    // Signaling server WebSocket URL
    pub signaling_server: String,

    // Network sync interval in frames (default: 3 frames = ~50ms @ 60fps)
    pub backup_sync_interval: u64,

    // Connection timeout in seconds
    pub connection_timeout_secs: u64,

    // Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            signaling_server: "wss://p2pong-production.up.railway.app".to_string(),
            backup_sync_interval: 3,
            connection_timeout_secs: 300, // 5 minutes - plenty of time for STUN/ICE negotiation
            heartbeat_interval_ms: 2000,
        }
    }
}
