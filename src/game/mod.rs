pub mod state;
pub mod physics;
pub mod input;

pub use state::{GameState, Player};
pub use physics::update;
pub use input::{InputAction, poll_input};
