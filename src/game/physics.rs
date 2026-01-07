use super::state::{GameState, Player};

// All constants now in virtual coordinates (3x resolution: 1200×600)
const PADDLE_SPEED: f32 = 1800.0; // Virtual units per second (faster movement)
pub const PADDLE_MARGIN: f32 = 18.0; // Distance from edge in virtual coords
pub const PADDLE_WIDTH: f32 = 20.0; // Width in virtual coords (thicker paddles)
pub const BALL_SIZE: f32 = 20.0; // Ball diameter in virtual coords (ball.x/y is center)
const BALL_RADIUS: f32 = BALL_SIZE / 2.0; // Ball radius for collision detection
const WINNING_SCORE: u8 = 5;

pub fn update(state: &mut GameState, dt: f32) {
    if state.game_over {
        return;
    }

    // Update paddle positions based on velocity
    update_paddle(&mut state.left_paddle, dt, state.field_height);
    update_paddle(&mut state.right_paddle, dt, state.field_height);

    // Update ball position
    state.ball.x += state.ball.vx * dt;
    state.ball.y += state.ball.vy * dt;

    // Check wall collisions (top and bottom) - account for ball radius
    if state.ball.y - BALL_RADIUS <= 0.0 {
        state.ball.y = BALL_RADIUS;
        state.ball.vy = state.ball.vy.abs();
    } else if state.ball.y + BALL_RADIUS >= state.field_height {
        state.ball.y = state.field_height - BALL_RADIUS;
        state.ball.vy = -state.ball.vy.abs();
    }

    // Check paddle collisions
    check_paddle_collision(state);

    // Check goals - ball is out when its center crosses the boundary
    if state.ball.x - BALL_RADIUS <= 0.0 {
        // Right player scores
        state.right_score += 1;
        if state.right_score >= WINNING_SCORE {
            state.game_over = true;
            state.winner = Some(Player::Right);
        } else {
            state.reset_ball(Player::Right);
        }
    } else if state.ball.x + BALL_RADIUS >= state.field_width {
        // Left player scores
        state.left_score += 1;
        if state.left_score >= WINNING_SCORE {
            state.game_over = true;
            state.winner = Some(Player::Left);
        } else {
            state.reset_ball(Player::Left);
        }
    }
}

fn update_paddle(paddle: &mut super::state::Paddle, dt: f32, field_height: f32) {
    paddle.y += paddle.velocity * dt;
    
    // Clamp paddle position in virtual coordinates
    paddle.y = paddle.y.max(0.0).min(field_height - paddle.height);
}

fn check_paddle_collision(state: &mut GameState) {
    // Left paddle collision (in virtual coordinates)
    // Ball center is at ball.x, ball.y; ball edges extend by BALL_RADIUS
    let left_paddle_left = PADDLE_MARGIN;
    let left_paddle_right = PADDLE_MARGIN + PADDLE_WIDTH;
    
    // Check if ball's right edge overlaps with paddle
    if state.ball.x - BALL_RADIUS <= left_paddle_right
        && state.ball.x + BALL_RADIUS >= left_paddle_left
        && state.ball.y + BALL_RADIUS >= state.left_paddle.y
        && state.ball.y - BALL_RADIUS <= state.left_paddle.y + state.left_paddle.height
    {
        bounce_off_paddle(
            &mut state.ball,
            state.left_paddle.y,
            state.left_paddle.height,
            true,
        );
        // Move ball just outside paddle
        state.ball.x = left_paddle_right + BALL_RADIUS;
    }

    // Right paddle collision (in virtual coordinates)
    let right_paddle_left = state.field_width - PADDLE_MARGIN - PADDLE_WIDTH;
    let right_paddle_right = state.field_width - PADDLE_MARGIN;
    
    // Check if ball's left edge overlaps with paddle
    if state.ball.x + BALL_RADIUS >= right_paddle_left
        && state.ball.x - BALL_RADIUS <= right_paddle_right
        && state.ball.y + BALL_RADIUS >= state.right_paddle.y
        && state.ball.y - BALL_RADIUS <= state.right_paddle.y + state.right_paddle.height
    {
        bounce_off_paddle(
            &mut state.ball,
            state.right_paddle.y,
            state.right_paddle.height,
            false,
        );
        // Move ball just outside paddle
        state.ball.x = right_paddle_left - BALL_RADIUS;
    }
}

fn bounce_off_paddle(ball: &mut super::state::Ball, paddle_y: f32, paddle_height: f32, is_left: bool) {
    // Calculate where on the paddle the ball hit (0.0 = top, 1.0 = bottom)
    let hit_pos = (ball.y - paddle_y) / paddle_height;
    
    // Map hit position to angle (-60 to 60 degrees)
    // Center hits go straight, edge hits go at steep angles
    let max_angle = std::f32::consts::PI / 3.0; // 60 degrees
    let angle = (hit_pos - 0.5) * 2.0 * max_angle;
    
    // Calculate speed and increase it on each hit
    let current_speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
    let speed = current_speed * 1.1; // 10% speed increase per hit
    
    // Set new velocity based on angle
    if is_left {
        ball.vx = angle.cos() * speed;
        ball.vy = angle.sin() * speed;
    } else {
        ball.vx = -angle.cos() * speed;
        ball.vy = angle.sin() * speed;
    }
}

pub fn move_paddle_up(paddle: &mut super::state::Paddle) {
    paddle.velocity = -PADDLE_SPEED;
}

pub fn move_paddle_down(paddle: &mut super::state::Paddle) {
    paddle.velocity = PADDLE_SPEED;
}

pub fn stop_paddle(paddle: &mut super::state::Paddle) {
    paddle.velocity = 0.0;
}
