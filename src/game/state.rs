use std::f32::consts::PI;

// Virtual coordinate system - the "true" game field that physics runs in
// All players see the same virtual field, but render it to their terminal size
// High resolution for maximum smoothness with multi-cell rendering
pub const VIRTUAL_WIDTH: f32 = 1200.0;
pub const VIRTUAL_HEIGHT: f32 = 600.0;

// Game constants in virtual coordinates
// With 600 virtual height and ~30 screen rows, each row = 20 virtual units
// So 90 virtual units = ~4.5 screen rows (good paddle size)
const PADDLE_HEIGHT: f32 = 90.0; // About 15% of virtual height (~4.5 screen rows)

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
        Self {
            y,
            height,
        }
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
    pub serve_count: u8, // Track serves for tennis tiebreak pattern
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Player {
    Left,
    Right,
}

impl GameState {
    pub fn new(_width: u16, _height: u16) -> Self {
        // Game always runs in virtual coordinates (independent of terminal size)
        let field_width = VIRTUAL_WIDTH;
        let field_height = VIRTUAL_HEIGHT;

        let mut ball = Ball::new(field_width / 2.0, field_height / 2.0);
        
        // Initial serve towards left player (ball speed scaled with resolution)
        ball.reset(field_width / 2.0, field_height / 2.0, PI, 360.0);

        let center_y = field_height / 2.0 - PADDLE_HEIGHT / 2.0;
        
        Self {
            ball,
            left_paddle: Paddle::new(center_y, PADDLE_HEIGHT),
            right_paddle: Paddle::new(center_y, PADDLE_HEIGHT),
            left_score: 0,
            right_score: 0,
            game_over: false,
            winner: None,
            field_width,
            field_height,
            serve_count: 0,
        }
    }

    pub fn resize(&mut self, _width: u16, _height: u16) {
        // In virtual coordinates, field size never changes
        // Terminal resize only affects rendering, not physics
        // No-op for now, keeping method for potential future use
    }

    pub fn reset_ball(&mut self, _scored_player: Player) {
        // Tennis tiebreak serve pattern:
        // Serve 0: Left
        // Serves 1-2: Right, Right
        // Serves 3-4: Left, Left
        // Serves 5-6: Right, Right
        // etc.
        
        let serve_to_left = if self.serve_count == 0 {
            true
        } else {
            // After first serve, alternate every 2 serves
            // Serves 1-2 go right (false), 3-4 go left (true), 5-6 go right (false), etc.
            ((self.serve_count - 1) / 2) % 2 == 1
        };
        
        let angle = if serve_to_left {
            PI  // Serve left
        } else {
            0.0 // Serve right
        };
        
        self.serve_count += 1;
        
        self.ball.reset(
            self.field_width / 2.0,
            self.field_height / 2.0,
            angle,
            360.0,
        );
    }
}
