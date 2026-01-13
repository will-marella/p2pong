// Backboard bot - instant tracker for training mode

use crate::game::{GameState, InputAction};
use super::Bot;

/// A simple training bot that tracks the ball's Y position instantly
///
/// This bot is designed as a "backboard" or training mode:
/// - Instantly tracks ball when it's coming toward the bot
/// - Returns to center when ball is moving away
/// - No prediction errors or delays (perfect tracking)
/// - Good for beginners learning controls
pub struct BackboardBot {
    name: String,
    movement_threshold: f32,  // How far from target before moving
}

impl BackboardBot {
    /// Create a new BackboardBot
    pub fn new() -> Self {
        Self {
            name: "Backboard".to_string(),
            movement_threshold: 30.0,  // Threshold for smooth movement
        }
    }
}

impl Bot for BackboardBot {
    fn get_action(&mut self, game_state: &GameState, _dt: f32) -> Option<InputAction> {
        // Right paddle (AI side)
        let paddle_center_y = game_state.right_paddle.y + (game_state.right_paddle.height / 2.0);
        let field_center_y = game_state.field_height / 2.0;

        // Determine target position based on ball direction
        let target_y = if game_state.ball.vx > 0.0 {
            // Ball is moving toward bot - track ball position
            game_state.ball.y
        } else {
            // Ball is moving away - stay near center
            field_center_y
        };

        let diff = target_y - paddle_center_y;

        // Only move if significantly away from target
        if diff.abs() < self.movement_threshold {
            None  // Close enough, don't move
        } else if diff > 0.0 {
            Some(InputAction::RightPaddleDown)  // Target below, move down
        } else {
            Some(InputAction::RightPaddleUp)    // Target above, move up
        }
    }

    fn reset(&mut self) {
        // Simple tracker has no state to reset
    }

    fn name(&self) -> &str {
        &self.name
    }
}
