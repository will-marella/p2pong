pub mod input;
pub mod physics;
pub mod state;

pub use input::{
    poll_input_local_2p, poll_input_player_left, poll_input_player_right, InputAction,
};
pub use physics::update_with_events;
pub use state::{GameState, Player};
