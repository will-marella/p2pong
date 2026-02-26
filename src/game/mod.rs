pub mod input;
pub mod physics;
pub mod state;

#[allow(unused_imports)]
pub use input::{
    poll_input_local_2p, poll_input_player_left, poll_input_player_left_with_mouse,
    poll_input_player_right, poll_input_player_right_with_mouse, InputAction,
};
pub use physics::update_with_events;
pub use state::{GameState, Player};
