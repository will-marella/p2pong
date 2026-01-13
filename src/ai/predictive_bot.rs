// Predictive bot with imperfect trajectory prediction

use crate::game::{GameState, InputAction};
use super::Bot;
use super::prediction::predict_ball_intercept;
use std::time::Instant;
use rand::{Rng, thread_rng};
use rand::rngs::ThreadRng;
use rand_distr::{Distribution, Normal};

/// Configuration for a predictive bot's behavior
#[derive(Debug, Clone)]
pub struct PredictiveBotConfig {
    pub name: String,
    pub error_stddev: f32,                   // Standard deviation of prediction error (normal distribution)
    pub catastrophic_miss_rate: f32,         // Probability of total whiff
    pub reaction_delay_ms: u64,              // Delay between actions
    pub prediction_update_interval_ms: u64,  // How often bot recalculates prediction
    pub movement_threshold: f32,             // Dead zone to avoid jittery movement
}

/// Predictive bot that uses trajectory prediction with human-like errors
pub struct PredictiveBot {
    config: PredictiveBotConfig,

    // Cached prediction state
    last_prediction_time: Instant,
    cached_target_y: Option<f32>,  // None = return to center

    // Reaction delay
    last_action_time: Instant,

    // RNG for error injection
    rng: ThreadRng,
}

impl PredictiveBot {
    /// Create a new PredictiveBot with the given configuration
    pub fn new(config: PredictiveBotConfig) -> Self {
        Self {
            config,
            last_prediction_time: Instant::now(),
            cached_target_y: None,
            last_action_time: Instant::now(),
            rng: thread_rng(),
        }
    }

    /// Create an Easy difficulty bot (high variance, frequent mistakes)
    pub fn easy() -> Self {
        Self::new(PredictiveBotConfig {
            name: "Easy".to_string(),
            error_stddev: 35.0,                  // High variance: ±35 units (1σ), ±70 units (2σ)
            catastrophic_miss_rate: 0.12,        // 12% total whiffs
            reaction_delay_ms: 200,
            prediction_update_interval_ms: 250,
            movement_threshold: 40.0,
        })
    }

    /// Create a Medium difficulty bot (moderate variance, occasional mistakes)
    pub fn medium() -> Self {
        Self::new(PredictiveBotConfig {
            name: "Medium".to_string(),
            error_stddev: 18.0,                  // Medium variance: ±18 units (1σ), ±36 units (2σ)
            catastrophic_miss_rate: 0.05,        // 5% whiffs
            reaction_delay_ms: 120,
            prediction_update_interval_ms: 150,
            movement_threshold: 30.0,
        })
    }

    /// Create a Hard difficulty bot (low variance, rare mistakes)
    pub fn hard() -> Self {
        Self::new(PredictiveBotConfig {
            name: "Hard".to_string(),
            error_stddev: 8.0,                   // Low variance: ±8 units (1σ), ±16 units (2σ)
            catastrophic_miss_rate: 0.02,        // 2% whiffs (rare)
            reaction_delay_ms: 60,
            prediction_update_interval_ms: 80,
            movement_threshold: 20.0,
        })
    }

    /// Update the cached prediction based on current game state
    fn update_prediction(&mut self, game_state: &GameState) {
        // Calculate paddle x-position (right paddle for AI)
        let paddle_x = game_state.field_width - 18.0 - 10.0; // PADDLE_MARGIN - PADDLE_WIDTH/2

        // Predict where ball will be when it reaches the paddle
        let true_prediction = predict_ball_intercept(
            game_state.ball.x,
            game_state.ball.y,
            game_state.ball.vx,
            game_state.ball.vy,
            paddle_x,
            game_state.field_height,
        );

        // Apply imperfect prediction (human-like errors)
        self.cached_target_y = match true_prediction {
            Some(true_y) => self.apply_prediction_error(true_y),
            None => None,  // Ball moving away or won't reach paddle
        };

        // Update timestamp
        self.last_prediction_time = Instant::now();
    }

    /// Apply prediction error to simulate human imperfection
    ///
    /// Returns None if catastrophic miss (bot gives up on this shot),
    /// otherwise returns the predicted y-position with gaussian error applied
    fn apply_prediction_error(&mut self, true_y: f32) -> Option<f32> {
        // 1. Catastrophic miss: occasionally the bot totally whiffs
        if self.rng.gen::<f32>() < self.config.catastrophic_miss_rate {
            return None;  // Total miss - bot gives up
        }

        // 2. Sample error from normal distribution
        let normal = Normal::new(0.0, self.config.error_stddev).unwrap();
        let error = normal.sample(&mut self.rng);

        // 3. Apply error to true prediction
        Some(true_y + error)
    }

    /// Check if it's time to update the prediction
    fn should_update_prediction(&self) -> bool {
        self.last_prediction_time.elapsed().as_millis()
            >= self.config.prediction_update_interval_ms as u128
    }

    /// Check if reaction delay has passed
    fn can_act(&self) -> bool {
        self.last_action_time.elapsed().as_millis()
            >= self.config.reaction_delay_ms as u128
    }
}

impl Bot for PredictiveBot {
    fn get_action(&mut self, game_state: &GameState, _dt: f32) -> Option<InputAction> {
        // 1. Update prediction if interval has passed
        if self.should_update_prediction() {
            self.update_prediction(game_state);
        }

        // 2. Check reaction delay
        if !self.can_act() {
            return None;  // Still in reaction delay
        }

        // 3. Determine target position
        let paddle_center_y = game_state.right_paddle.y + (game_state.right_paddle.height / 2.0);
        let field_center_y = game_state.field_height / 2.0;

        let target_y = match self.cached_target_y {
            Some(y) => y,           // Move toward predicted position
            None => field_center_y, // Ball moving away or catastrophic miss → return to center
        };

        // 4. Calculate difference from target
        let diff = target_y - paddle_center_y;

        // 5. Check movement threshold (avoid jittery movement)
        if diff.abs() < self.config.movement_threshold {
            return None;  // Close enough, don't move
        }

        // 6. Update action timestamp and return move command
        self.last_action_time = Instant::now();

        if diff > 0.0 {
            Some(InputAction::RightPaddleDown)
        } else {
            Some(InputAction::RightPaddleUp)
        }
    }

    fn reset(&mut self) {
        // Reset all timers and cached state when round starts
        self.last_prediction_time = Instant::now();
        self.last_action_time = Instant::now();
        self.cached_target_y = None;
    }

    fn name(&self) -> &str {
        &self.config.name
    }
}
