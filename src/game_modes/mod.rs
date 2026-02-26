mod ai;
mod common;
mod local;
mod network;
mod network_tcp;

pub use ai::run_game_vs_ai;
pub use local::run_game_local;
pub use network::{run_game_network_client, run_game_network_host};
pub use network_tcp::{
    run_game_network_client as run_game_tcp_client, run_game_network_host as run_game_tcp_host,
};
