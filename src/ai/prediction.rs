// Trajectory prediction for AI bots

/// Predict where the ball will be when it reaches the paddle's x-position
///
/// Returns the predicted y-position, or None if the ball is moving away from the paddle
/// or won't reach it. Accounts for wall bounces (top and bottom).
///
/// # Arguments
/// * `ball_x`, `ball_y` - Current ball position (center coordinates)
/// * `ball_vx`, `ball_vy` - Current ball velocity (units per frame)
/// * `paddle_x` - The x-position of the paddle we're predicting for
/// * `field_height` - Height of the playing field (for wall bounce calculation)
pub fn predict_ball_intercept(
    ball_x: f32,
    ball_y: f32,
    ball_vx: f32,
    ball_vy: f32,
    paddle_x: f32,
    field_height: f32,
) -> Option<f32> {
    // Check if ball is moving toward the paddle
    let moving_right = ball_vx > 0.0;
    let moving_left = ball_vx < 0.0;
    let paddle_is_right = paddle_x > ball_x;
    let paddle_is_left = paddle_x < ball_x;

    // If ball is moving away from paddle, return None
    if (paddle_is_right && moving_left) || (paddle_is_left && moving_right) {
        return None;
    }

    // If ball is stationary horizontally, it won't reach the paddle
    if ball_vx.abs() < 0.01 {
        return None;
    }

    // Calculate time to reach paddle x-position
    let time_to_intercept = (paddle_x - ball_x) / ball_vx;

    // If negative time (shouldn't happen after above checks, but safety)
    if time_to_intercept < 0.0 {
        return None;
    }

    // Simulate ball position forward, accounting for wall bounces
    let mut predicted_y = ball_y + ball_vy * time_to_intercept;

    // Handle wall bounces (multiple bounces possible for steep angles)
    // Maximum 10 iterations to prevent infinite loops in edge cases
    for _ in 0..10 {
        if predicted_y >= 0.0 && predicted_y <= field_height {
            // Ball is in bounds, we're done
            break;
        }

        if predicted_y < 0.0 {
            // Ball went below bottom wall - reflect it back
            predicted_y = -predicted_y;
        } else if predicted_y > field_height {
            // Ball went above top wall - reflect it back
            predicted_y = 2.0 * field_height - predicted_y;
        }
    }

    // Clamp to field bounds (safety, shouldn't be needed after loop)
    predicted_y = predicted_y.clamp(0.0, field_height);

    Some(predicted_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELD_HEIGHT: f32 = 600.0;
    const LEFT_PADDLE_X: f32 = 18.0 + 10.0; // PADDLE_MARGIN + PADDLE_WIDTH/2
    const RIGHT_PADDLE_X: f32 = 1200.0 - 18.0 - 10.0; // field_width - PADDLE_MARGIN - PADDLE_WIDTH/2

    #[test]
    fn test_simple_intercept_no_bounce() {
        // Ball at center, moving right horizontally
        let predicted = predict_ball_intercept(
            600.0,          // ball_x (center)
            300.0,          // ball_y (center)
            6.0,            // ball_vx (moving right)
            0.0,            // ball_vy (no vertical movement)
            RIGHT_PADDLE_X, // right paddle
            FIELD_HEIGHT,
        );

        // Should predict y=300 (no vertical movement)
        assert!(predicted.is_some());
        assert!((predicted.unwrap() - 300.0).abs() < 1.0);
    }

    #[test]
    fn test_intercept_with_angle_no_bounce() {
        // Ball moving right and up
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            300.0, // ball_y
            6.0,   // ball_vx
            3.0,   // ball_vy (moving up)
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Time to reach paddle: (RIGHT_PADDLE_X - 600) / 6.0 ≈ 95.3 frames
        // Predicted y: 300 + 3.0 * 95.3 ≈ 586
        assert!(predicted.is_some());
        let pred_y = predicted.unwrap();
        assert!(pred_y > 300.0); // Should be higher
        assert!(pred_y < FIELD_HEIGHT); // But still in bounds
    }

    #[test]
    fn test_single_wall_bounce_top() {
        // Ball moving right and up, will hit top wall
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            500.0, // ball_y (near top)
            6.0,   // ball_vx
            4.0,   // ball_vy (moving up fast)
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should predict somewhere valid after bouncing off top
        assert!(predicted.is_some());
        let pred_y = predicted.unwrap();
        assert!(pred_y >= 0.0 && pred_y <= FIELD_HEIGHT);
    }

    #[test]
    fn test_single_wall_bounce_bottom() {
        // Ball moving right and down, will hit bottom wall
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            100.0, // ball_y (near bottom)
            6.0,   // ball_vx
            -4.0,  // ball_vy (moving down)
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should predict somewhere valid after bouncing off bottom
        assert!(predicted.is_some());
        let pred_y = predicted.unwrap();
        assert!(pred_y >= 0.0 && pred_y <= FIELD_HEIGHT);
    }

    #[test]
    fn test_multiple_bounces() {
        // Ball with very steep vertical angle, will bounce multiple times
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            300.0, // ball_y
            6.0,   // ball_vx
            12.0,  // ball_vy (very steep upward)
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should handle multiple bounces and return valid position
        assert!(predicted.is_some());
        let pred_y = predicted.unwrap();
        assert!(pred_y >= 0.0 && pred_y <= FIELD_HEIGHT);
    }

    #[test]
    fn test_ball_moving_away_right_paddle() {
        // Ball moving left (away from right paddle)
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            300.0, // ball_y
            -6.0,  // ball_vx (moving left, away from right paddle)
            0.0,   // ball_vy
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should return None (ball moving away)
        assert!(predicted.is_none());
    }

    #[test]
    fn test_ball_moving_away_left_paddle() {
        // Ball moving right (away from left paddle)
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            300.0, // ball_y
            6.0,   // ball_vx (moving right, away from left paddle)
            0.0,   // ball_vy
            LEFT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should return None (ball moving away)
        assert!(predicted.is_none());
    }

    #[test]
    fn test_ball_stationary() {
        // Ball with zero horizontal velocity
        let predicted = predict_ball_intercept(
            600.0, // ball_x
            300.0, // ball_y
            0.0,   // ball_vx (stationary)
            3.0,   // ball_vy
            RIGHT_PADDLE_X,
            FIELD_HEIGHT,
        );

        // Should return None (won't reach paddle)
        assert!(predicted.is_none());
    }
}
