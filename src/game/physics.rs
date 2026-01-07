use super::state::{GameState, Player};

// All constants now in virtual coordinates
const PADDLE_SPEED: f32 = 250.0; // Virtual units per second (increased for faster movement)
const PADDLE_MARGIN: f32 = 3.0; // Distance from edge in virtual coords
const PADDLE_WIDTH: f32 = 2.0; // Width in virtual coords
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

    // Check wall collisions (top and bottom)
    if state.ball.y <= 0.0 {
        state.ball.y = 0.0;
        state.ball.vy = state.ball.vy.abs();
    } else if state.ball.y >= state.field_height {
        state.ball.y = state.field_height;
        state.ball.vy = -state.ball.vy.abs();
    }

    // Check paddle collisions
    check_paddle_collision(state);

    // Check goals
    if state.ball.x <= 0.0 {
        // Right player scores
        state.right_score += 1;
        if state.right_score >= WINNING_SCORE {
            state.game_over = true;
            state.winner = Some(Player::Right);
        } else {
            state.reset_ball(Player::Right);
        }
    } else if state.ball.x >= state.field_width {
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
    let left_paddle_right_edge = PADDLE_MARGIN + PADDLE_WIDTH;
    if state.ball.x <= left_paddle_right_edge
        && state.ball.x >= PADDLE_MARGIN
        && state.ball.y >= state.left_paddle.y
        && state.ball.y <= state.left_paddle.y + state.left_paddle.height
    {
        bounce_off_paddle(
            &mut state.ball,
            state.left_paddle.y,
            state.left_paddle.height,
            true,
        );
        state.ball.x = left_paddle_right_edge;
    }

    // Right paddle collision (in virtual coordinates)
    let right_paddle_x = state.field_width - PADDLE_MARGIN - PADDLE_WIDTH;
    if state.ball.x >= right_paddle_x
        && state.ball.x <= state.field_width - PADDLE_MARGIN
        && state.ball.y >= state.right_paddle.y
        && state.ball.y <= state.right_paddle.y + state.right_paddle.height
    {
        bounce_off_paddle(
            &mut state.ball,
            state.right_paddle.y,
            state.right_paddle.height,
            false,
        );
        state.ball.x = right_paddle_x;
    }
}

fn bounce_off_paddle(ball: &mut super::state::Ball, paddle_y: f32, paddle_height: f32, is_left: bool) {
    // Calculate where on the paddle the ball hit (0.0 = top, 1.0 = bottom)
    let hit_pos = (ball.y - paddle_y) / paddle_height;
    
    // Map hit position to angle (-60 to 60 degrees)
    // Center hits go straight, edge hits go at steep angles
    let max_angle = std::f32::consts::PI / 3.0; // 60 degrees
    let angle = (hit_pos - 0.5) * 2.0 * max_angle;
    
    // Calculate speed (maintain current speed)
    let speed = (ball.vx * ball.vx + ball.vy * ball.vy).sqrt();
    
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
