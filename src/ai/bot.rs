// Bot trait for AI opponents

use crate::game::{GameState, InputAction};

/// Trait for AI bot implementations
///
/// Bots can maintain internal state and make decisions based on the game state.
/// The trait is designed to be simple and easy to implement - just decide what
/// action to take each frame.
pub trait Bot {
    /// Decide what action the bot should take this frame
    ///
    /// # Arguments
    /// * `game_state` - Current game state (ball position, paddles, scores, etc.)
    /// * `dt` - Delta time since last frame (for time-based decisions)
    ///
    /// # Returns
    /// * `Some(InputAction)` - The action the bot wants to take
    /// * `None` - Bot doesn't want to move this frame
    fn get_action(&mut self, game_state: &GameState, dt: f32) -> Option<InputAction>;

    /// Reset bot internal state (called when new game/round starts)
    fn reset(&mut self);

    /// Bot name for debugging/display
    fn name(&self) -> &str;
}
