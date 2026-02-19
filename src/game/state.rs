use std::f32::consts::PI;

use crate::config::PhysicsConfig;

// Virtual coordinate system - the "true" game field that physics runs in
// All players see the same virtual field, but render it to their terminal size
// High resolution for maximum smoothness with multi-cell rendering
pub const VIRTUAL_WIDTH: f32 = 1200.0;
pub const VIRTUAL_HEIGHT: f32 = 600.0;

#[derive(Debug, Clone)]
pub struct Ball {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

impl Ball {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
        }
    }

    pub fn reset(&mut self, x: f32, y: f32, angle: f32, speed: f32) {
        self.x = x;
        self.y = y;
        self.vx = angle.cos() * speed;
        self.vy = angle.sin() * speed;
    }
}

#[derive(Debug, Clone)]
pub struct Paddle {
    pub y: f32,
    pub height: f32,
}

impl Paddle {
    pub fn new(y: f32, height: f32) -> Self {
        Self { y, height }
    }
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub ball: Ball,
    pub left_paddle: Paddle,
    pub right_paddle: Paddle,
    pub left_score: u8,
    pub right_score: u8,
    pub game_over: bool,
    pub winner: Option<Player>,
    pub field_width: f32,
    pub field_height: f32,
    pub serve_count: u8,            // Track serves for tennis tiebreak pattern
    pub ball_speed: f32,            // Initial ball speed in virtual units per second
    pub winning_score: u8,          // Score required to win
    pub tap_distance: f32,          // Paddle movement distance per tap
    pub speed_increase_factor: f32, // Ball speed multiplier on each paddle hit
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Player {
    Left,
    Right,
}

impl GameState {
    pub fn new(_width: u16, _height: u16, physics: &PhysicsConfig) -> Self {
        let field_width = physics.virtual_width;
        let field_height = physics.virtual_height;
        let ball_speed = physics.ball_initial_speed;
        let paddle_height = physics.paddle_height;
        let winning_score = physics.winning_score;
        let tap_distance = physics.paddle_tap_distance;
        let speed_increase_factor = physics.ball_speed_multiplier;

        let mut ball = Ball::new(field_width / 2.0, field_height / 2.0);

        // Initial serve towards left player (ball will be frozen during countdown)
        ball.reset(field_width / 2.0, field_height / 2.0, PI, ball_speed);

        let center_y = field_height / 2.0 - paddle_height / 2.0;

        Self {
            ball,
            left_paddle: Paddle::new(center_y, paddle_height),
            right_paddle: Paddle::new(center_y, paddle_height),
            left_score: 0,
            right_score: 0,
            game_over: false,
            winner: None,
            field_width,
            field_height,
            serve_count: 1, // Start at 1 since initial serve was to left (counts as serve 0)
            ball_speed,
            winning_score,
            tap_distance,
            speed_increase_factor,
        }
    }

    /// Reset the entire game for a rematch (scores, game_over, winner, ball, paddles)
    pub fn reset_game(&mut self) {
        // Reset scores and game state
        self.left_score = 0;
        self.right_score = 0;
        self.game_over = false;
        self.winner = None;
        self.serve_count = 1;

        // Reset ball to center with initial serve
        self.ball.reset(
            self.field_width / 2.0,
            self.field_height / 2.0,
            PI, // Initial serve towards left player
            self.ball_speed,
        );

        // Reset paddles to center
        let center_y = self.field_height / 2.0 - self.left_paddle.height / 2.0;
        self.left_paddle.y = center_y;
        self.right_paddle.y = center_y;
    }

    pub fn reset_ball(&mut self, _scored_player: Player) {
        // Tennis snake serve pattern:
        // Serve 0: Left (1 serve)
        // Serves 1-2: Right, Right (2 serves)
        // Serves 3-4: Left, Left (2 serves)
        // Serves 5-6: Right, Right (2 serves)
        // Pattern: L, R-R, L-L, R-R, L-L, ...

        let serve_to_left = match self.serve_count {
            0 => true, // First serve: left
            n => {
                // After first serve: alternate every 2 serves
                // Serves 1-2: right, 3-4: left, 5-6: right, etc.
                ((n - 1) / 2) % 2 == 1
            }
        };

        let angle = if serve_to_left {
            PI // Serve left
        } else {
            0.0 // Serve right
        };

        self.serve_count += 1;

        self.ball.reset(
            self.field_width / 2.0,
            self.field_height / 2.0,
            angle,
            self.ball_speed,
        );
    }
}
