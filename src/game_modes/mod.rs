mod ai;
mod common;
mod local;
mod network;

pub use ai::run_game_vs_ai;
pub use local::run_game_local;
pub use network::{run_game_network_client, run_game_network_host};
