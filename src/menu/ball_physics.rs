use crate::game::physics::{BALL_SIZE, VIRTUAL_HEIGHT, VIRTUAL_WIDTH};
use crate::game::state::Ball;

const BALL_RADIUS: f32 = BALL_SIZE / 2.0;

/// Initialize menu ball with random angle between 30-60 degrees
/// Uses virtual coordinates (VIRTUAL_WIDTH × VIRTUAL_HEIGHT) like the game
pub fn init_ball_random_angle(ball: &mut Ball) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Random angle between 30-60 degrees (down and right from top-left)
    let angle_deg: f32 = rng.gen_range(30.0..=60.0);
    let angle_rad = angle_deg.to_radians();

    // Ball speed in virtual units per second (slightly slower than game for chill menu vibe)
    const MENU_BALL_SPEED: f32 = 400.0;

    // Start from top-left with margin (in virtual coordinates)
    let start_x = BALL_RADIUS + 20.0;
    let start_y = BALL_RADIUS + 20.0;

    // Use Ball's reset method (reusing game logic!)
    ball.reset(start_x, start_y, angle_rad, MENU_BALL_SPEED);
}

/// Update ball position and handle wall bounces (simplified game physics)
/// Uses virtual coordinate system (VIRTUAL_WIDTH × VIRTUAL_HEIGHT)
pub fn update_ball(ball: &mut Ball, dt: f32) {
    // Update position (same as game physics)
    ball.x += ball.vx * dt;
    ball.y += ball.vy * dt;

    // Bounce off left/right walls (virtual coordinates)
    if ball.x - BALL_RADIUS <= 0.0 {
        ball.x = BALL_RADIUS;
        ball.vx = ball.vx.abs(); // Bounce right
    } else if ball.x + BALL_RADIUS >= VIRTUAL_WIDTH {
        ball.x = VIRTUAL_WIDTH - BALL_RADIUS;
        ball.vx = -ball.vx.abs(); // Bounce left
    }

    // Bounce off top/bottom walls (virtual coordinates)
    if ball.y - BALL_RADIUS <= 0.0 {
        ball.y = BALL_RADIUS;
        ball.vy = ball.vy.abs(); // Bounce down
    } else if ball.y + BALL_RADIUS >= VIRTUAL_HEIGHT {
        ball.y = VIRTUAL_HEIGHT - BALL_RADIUS;
        ball.vy = -ball.vy.abs(); // Bounce up
    }
}
