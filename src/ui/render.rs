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
    
    // Draw top border (just before playable area starts, where ball bounces at y=0)
    // When ball.y = 0, it's at the top. With offset, that's playable_offset_y.
    // Border should be 1 pixel above where ball can go.
    let top_border_y = playable_offset_y - 1;
    canvas.draw_horizontal_line(top_border_y);
    
    // Draw bottom border (at the last pixel of playable area, where ball bounces at y=VIRTUAL_HEIGHT)
    // When ball.y = VIRTUAL_HEIGHT, pixel_y = VIRTUAL_HEIGHT * scale_y + offset = playable_height_pixels + offset
    // Border should be at the last pixel the ball can reach
    let bottom_border_y = playable_offset_y + playable_height_pixels - 1;
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
    
    // Draw text widgets FIRST (so Braille can render on top)
    draw_controls(frame, area);

    // Draw game over screen if needed
    if state.game_over {
        draw_game_over(frame, state, area);
    }

    // Render the Braille canvas LAST (on top of text, so scores are never covered)
    render_braille_canvas(frame, &canvas, area);
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
    // For rows 1-2 (where text controls are), only render the left portion (scores)
    for y in 0..canvas.pixel_height() / 4 {
        let mut line_text = String::new();
        let cell_width = canvas.pixel_width() / 2;
        
        // For rows 1-2, only render left 70% to leave room for right-aligned text
        let render_width = if y >= 1 && y <= 2 {
            (cell_width * 7 / 10).max(1)
        } else {
            cell_width
        };
        
        for x in 0..render_width {
            let ch = canvas.to_char(x, y);
            // Convert empty Braille to space so text can show through
            if ch == '\u{2800}' {  // Empty Braille character
                line_text.push(' ');
            } else {
                line_text.push(ch);
            }
        }
        
        let paragraph = Paragraph::new(line_text)
            .style(Style::default().fg(Color::White));
        
        let row_area = Rect {
            x: area.x,
            y: area.y + y as u16,
            width: render_width as u16,
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
    // Draw controls as regular text - narrow widgets on right side only
    // This prevents overlapping with Braille scores on the left
    
    let text1 = "W/↑: Up  S/↓: Down";
    let text2 = "Q: Quit";
    
    // Calculate widget width - just wide enough for the text + small margin
    let width1 = (text1.len() as u16 + 2).min(area.width / 2);
    let width2 = (text2.len() as u16 + 2).min(area.width / 2);
    
    let controls_line1 = Paragraph::new(text1)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Right);
    
    let controls_line2 = Paragraph::new(text2)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Right);
    
    // Position widgets: moved up to rows 1-2, and shifted left a bit
    let left_offset = 2; // Shift left by 2 columns
    
    let controls_area1 = Rect {
        x: area.x + area.width.saturating_sub(width1 + left_offset),
        y: area.y + 1, // Moved up from row 2 to row 1
        width: width1,
        height: 1,
    };
    
    let controls_area2 = Rect {
        x: area.x + area.width.saturating_sub(width2 + left_offset),
        y: area.y + 2, // Moved up from row 3 to row 2
        width: width2,
        height: 1,
    };
    
    frame.render_widget(controls_line1, controls_area1);
    frame.render_widget(controls_line2, controls_area2);
}

fn draw_game_over(frame: &mut Frame, state: &GameState, area: Rect) {
    // Display game over message in the top bar (terminal style)
    // Use narrow centered widget to avoid covering Braille scores
    let winner_text = match state.winner {
        Some(Player::Left) => "LEFT WINS",
        Some(Player::Right) => "RIGHT WINS",
        None => "GAME OVER",
    };

    let quit_text = "Press Q to quit";
    
    // Calculate narrow widget widths
    let msg_width = (winner_text.len() as u16 + 4).min(area.width / 2);
    let quit_width = (quit_text.len() as u16 + 4).min(area.width / 2);

    // Simple, bold message in the center (row 3)
    let game_over_msg = Paragraph::new(winner_text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);

    let msg_area = Rect {
        x: area.x + (area.width.saturating_sub(msg_width)) / 2, // Center it
        y: area.y + 3, // Below the scores, in the header area
        width: msg_width,
        height: 1,
    };

    frame.render_widget(game_over_msg, msg_area);
    
    // Show quit hint in row 4
    let quit_hint = Paragraph::new(quit_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    let hint_area = Rect {
        x: area.x + (area.width.saturating_sub(quit_width)) / 2, // Center it
        y: area.y + 4,
        width: quit_width,
        height: 1,
    };

    frame.render_widget(quit_hint, hint_area);
}
