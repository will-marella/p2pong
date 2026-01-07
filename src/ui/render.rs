use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::game::{
    state::{VIRTUAL_WIDTH, VIRTUAL_HEIGHT},
    physics::{PADDLE_MARGIN, PADDLE_WIDTH, BALL_SIZE},
    GameState, Player
};
use super::braille::BrailleCanvas;

// Layout: Top bar with score + controls, bordered playable area, bottom border
// Row 0-4: Score area (Braille digits are 16px tall = 4 rows, with padding)
// Row 5: Top border line (1 pixel thick = shares row with score bottom)
// Rows 6 to N-1: Playable area
// Row N: Bottom border line
const UI_HEADER_ROWS: u16 = 5; // Top area before playable field (score + border)
const UI_FOOTER_ROWS: u16 = 1; // Bottom border

pub fn render(frame: &mut Frame, state: &GameState) {
    let area = frame.area();

    // Draw background (true black RGB, not terminal default)
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Create Braille canvas for entire screen (including score area and borders)
    let canvas_width = area.width as usize;
    let canvas_height = area.height as usize;
    let mut canvas = BrailleCanvas::new(canvas_width, canvas_height);
    
    // Draw Braille scores at the top (centered in header area)
    draw_braille_scores(&mut canvas, state);
    
    // Calculate playable area dimensions
    let playable_height_rows = area.height - UI_HEADER_ROWS - UI_FOOTER_ROWS;
    let playable_height_pixels = playable_height_rows as usize * 4;
    let playable_offset_y = UI_HEADER_ROWS as usize * 4; // Start after header
    
    // Draw top border (at the bottom of the header area, top edge of playable area)
    let top_border_y = playable_offset_y - 1; // Just above playable area
    canvas.draw_horizontal_line(top_border_y);
    
    // Draw bottom border (at the bottom edge of playable area)
    let bottom_border_y = playable_offset_y + playable_height_pixels; // Just below playable area
    canvas.draw_horizontal_line(bottom_border_y);
    
    // Calculate scale from virtual to Braille pixels
    let scale_x = (canvas.pixel_width()) as f32 / VIRTUAL_WIDTH;
    let scale_y = playable_height_pixels as f32 / VIRTUAL_HEIGHT;
    
    // Draw paddles in Braille (use same X positions as physics)
    let left_paddle_pixel_y = (state.left_paddle.y * scale_y) as usize + playable_offset_y;
    draw_braille_paddle_at(&mut canvas, left_paddle_pixel_y, state.left_paddle.height, PADDLE_MARGIN, scale_x, scale_y);
    
    let right_paddle_x = VIRTUAL_WIDTH - PADDLE_MARGIN - PADDLE_WIDTH;
    let right_paddle_pixel_y = (state.right_paddle.y * scale_y) as usize + playable_offset_y;
    draw_braille_paddle_at(&mut canvas, right_paddle_pixel_y, state.right_paddle.height, right_paddle_x, scale_x, scale_y);
    
    // Draw ball in Braille
    let ball_pixel_y = (state.ball.y * scale_y) as usize + playable_offset_y;
    draw_braille_ball_at(&mut canvas, state.ball.x, ball_pixel_y, scale_x, scale_y);
    
    // Draw center line
    draw_center_line_at(&mut canvas, scale_x, playable_offset_y, playable_height_pixels);
    
    // Render the entire Braille canvas
    render_braille_canvas(frame, &canvas, area);

    // Draw controls hint (keep as text, below scores)
    draw_controls(frame, area);

    // Draw game over screen if needed
    if state.game_over {
        draw_game_over(frame, state, area);
    }
}

fn draw_braille_paddle_at(canvas: &mut BrailleCanvas, pixel_y: usize, vh: f32, vx: f32, scale_x: f32, scale_y: f32) {
    // Convert virtual X coordinate to Braille pixel coordinates
    let pixel_x = (vx * scale_x) as usize;
    let pixel_height = (vh * scale_y) as usize;
    let pixel_width = (PADDLE_WIDTH * scale_x) as usize;
    
    // Draw solid rectangle
    canvas.fill_rect(pixel_x, pixel_y, pixel_width, pixel_height);
}

fn draw_braille_ball_at(canvas: &mut BrailleCanvas, vx: f32, pixel_y: usize, scale_x: f32, scale_y: f32) {
    // Ball position (vx, pixel_y) - vx is virtual X, pixel_y is absolute pixel Y
    // Convert BALL_SIZE from virtual coords to Braille pixels
    let ball_pixel_width = (BALL_SIZE * scale_x) as usize;
    let ball_pixel_height = (BALL_SIZE * scale_y) as usize;
    
    // Convert ball center X to pixel coordinates
    let center_pixel_x = (vx * scale_x) as usize;
    
    // Calculate top-left corner (center the ball on its position)
    let ball_x = center_pixel_x.saturating_sub(ball_pixel_width / 2);
    let ball_y = pixel_y.saturating_sub(ball_pixel_height / 2);
    
    // Draw ball as solid rectangle
    canvas.fill_rect(ball_x, ball_y, ball_pixel_width, ball_pixel_height);
}

fn draw_center_line_at(canvas: &mut BrailleCanvas, scale_x: f32, offset_y: usize, height: usize) {
    let center_pixel_x = (VIRTUAL_WIDTH / 2.0 * scale_x) as usize;
    
    // Draw dotted center line (every other pixel) in playable area only
    for y in (0..height).step_by(4) {
        let pixel_y = offset_y + y;
        canvas.set_pixel(center_pixel_x, pixel_y);
        canvas.set_pixel(center_pixel_x, pixel_y + 1);
    }
}

fn render_braille_canvas(frame: &mut Frame, canvas: &BrailleCanvas, area: Rect) {
    // Render each row of the Braille canvas
    for y in 0..canvas.pixel_height() / 4 {
        let mut line_text = String::new();
        for x in 0..canvas.pixel_width() / 2 {
            line_text.push(canvas.to_char(x, y));
        }
        
        let paragraph = Paragraph::new(line_text)
            .style(Style::default().fg(Color::White));
        
        let row_area = Rect {
            x: area.x,
            y: area.y + y as u16,
            width: area.width,
            height: 1,
        };
        
        frame.render_widget(paragraph, row_area);
    }
}

fn draw_braille_scores(canvas: &mut BrailleCanvas, state: &GameState) {
    // Each digit is 10 pixels wide × 16 pixels tall (4 cell rows)
    // Center the scores in the header area (5 rows = 20 pixels)
    let canvas_width_pixels = canvas.pixel_width();
    
    // Left score position (left third of screen, centered horizontally)
    let left_score_x = (canvas_width_pixels / 3).saturating_sub(5);
    
    // Right score position (right third of screen, centered horizontally)
    let right_score_x = (canvas_width_pixels * 2 / 3).saturating_sub(5);
    
    // Y position: center 16px tall digits in 20px header (5 rows * 4 pixels)
    // Top margin: (20 - 16) / 2 = 2 pixels
    let score_y = 2;
    
    // Draw left score
    canvas.draw_digit(state.left_score, left_score_x, score_y);
    
    // Draw right score
    canvas.draw_digit(state.right_score, right_score_x, score_y);
}

fn draw_controls(frame: &mut Frame, area: Rect) {
    // Draw controls hint as text
    let controls = Paragraph::new("W/S: Left  ↑/↓: Right  Q: Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    
    let controls_area = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: 1,
    };
    
    frame.render_widget(controls, controls_area);
}

fn draw_game_over(frame: &mut Frame, state: &GameState, area: Rect) {
    // Display game over message in the top bar (terminal style)
    let winner_text = match state.winner {
        Some(Player::Left) => "LEFT WINS",
        Some(Player::Right) => "RIGHT WINS",
        None => "GAME OVER",
    };

    // Simple, bold message in the score area (row 3)
    let game_over_msg = Paragraph::new(winner_text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);

    let msg_area = Rect {
        x: area.x,
        y: area.y + 3, // Below the scores, in the header area
        width: area.width,
        height: 1,
    };

    frame.render_widget(game_over_msg, msg_area);
    
    // Show quit hint in row 4
    let quit_hint = Paragraph::new("Press Q to quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    let hint_area = Rect {
        x: area.x,
        y: area.y + 4,
        width: area.width,
        height: 1,
    };

    frame.render_widget(quit_hint, hint_area);
}
