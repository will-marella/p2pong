use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use super::braille::BrailleCanvas;
use super::overlay::{render_overlay, OverlayMessage};
use crate::config::types::DisplayConfig;
use crate::game::{
    physics::{BALL_SIZE, PADDLE_MARGIN, PADDLE_WIDTH},
    GameState, Player,
};

/// Convert RGB array to ratatui Color
fn rgb_to_color(rgb: [u8; 3]) -> Color {
    Color::Rgb(rgb[0], rgb[1], rgb[2])
}

// Layout: Top bar with score + controls, bordered playable area, bottom border
// Row 0-4: Score area (Braille digits are 16px tall = 4 rows, with padding)
// Row 5: Gap row (ensures score and border don't share Braille cells)
// Row 6: Top border line (1 pixel thick at start of row 6)
// Rows 7 to N-1: Playable area
// Row N: Bottom border line
const UI_HEADER_ROWS: u16 = 6; // Top area before playable field (score + gap + border)
const UI_FOOTER_ROWS: u16 = 1; // Bottom border

pub fn render(
    frame: &mut Frame,
    state: &GameState,
    rtt_ms: Option<u64>,
    overlay: Option<&OverlayMessage>,
    _your_player: Option<Player>,
    display_config: &DisplayConfig,
) {
    let area = frame.area();

    // Draw background (true black RGB, not terminal default)
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Create Braille canvas for entire screen (including score area and borders)
    let canvas_width = area.width as usize;
    let canvas_height = area.height as usize;
    let mut canvas = BrailleCanvas::new(canvas_width, canvas_height);

    // Calculate playable area dimensions
    let playable_height_rows = area.height - UI_HEADER_ROWS - UI_FOOTER_ROWS;
    let playable_height_pixels = playable_height_rows as usize * 4;
    let playable_offset_y = UI_HEADER_ROWS as usize * 4; // Start after header

    // Calculate scale from virtual to Braille pixels
    let scale_x = (canvas.pixel_width()) as f32 / state.field_width;
    let scale_y = playable_height_pixels as f32 / state.field_height;

    // DRAW ORDER (optimized to minimize color bleeding):
    // 1. Center line (background layer)
    // 2. Borders (field markers)
    // 3. Scores (information)
    // 4. Paddles (interactive elements)
    // 5. Ball (top layer, most visible)

    // 1. Draw center line (background layer)
    let center_line_color = Some(rgb_to_color(display_config.center_line_color));
    draw_center_line_at(
        &mut canvas,
        scale_x,
        playable_offset_y,
        playable_height_pixels,
        state.field_width,
        center_line_color,
    );

    // 2. Draw borders (field markers)
    let border_color = Some(rgb_to_color(display_config.border_color));
    // Top border (just before playable area - at pixel 23, the last pixel of row 5)
    // This ensures border is in its own row, separate from scores (which end at pixel 17 in row 4)
    let top_border_y = playable_offset_y - 1;
    canvas.draw_horizontal_line(top_border_y, border_color);
    // Bottom border (at the last pixel of playable area, where ball bounces at y=VIRTUAL_HEIGHT)
    let bottom_border_y = playable_offset_y + playable_height_pixels - 1;
    canvas.draw_horizontal_line(bottom_border_y, border_color);

    // 3. Draw scores (information layer)
    let score_color = Some(rgb_to_color(display_config.score_color));
    draw_braille_scores(&mut canvas, state, score_color);

    // 4. Draw paddles (interactive elements)
    let paddle_color = Some(rgb_to_color(display_config.paddle_color));
    let left_paddle_pixel_y = (state.left_paddle.y * scale_y) as usize + playable_offset_y;
    draw_braille_paddle_at(
        &mut canvas,
        left_paddle_pixel_y,
        state.left_paddle.height,
        PADDLE_MARGIN,
        scale_x,
        scale_y,
        paddle_color,
    );

    let right_paddle_x = state.field_width - PADDLE_MARGIN - PADDLE_WIDTH;
    let right_paddle_pixel_y = (state.right_paddle.y * scale_y) as usize + playable_offset_y;
    draw_braille_paddle_at(
        &mut canvas,
        right_paddle_pixel_y,
        state.right_paddle.height,
        right_paddle_x,
        scale_x,
        scale_y,
        paddle_color,
    );

    // 5. Draw ball (top layer, highest priority visibility)
    let ball_color = Some(rgb_to_color(display_config.ball_color));
    let ball_pixel_y = (state.ball.y * scale_y) as usize + playable_offset_y;
    draw_braille_ball_at(
        &mut canvas,
        state.ball.x,
        ball_pixel_y,
        scale_x,
        scale_y,
        ball_color,
    );

    // Draw RTT if networked (top right corner)
    if let Some(rtt) = rtt_ms {
        draw_rtt(frame, area, rtt);
    }

    // Render the Braille canvas (pass whether RTT is shown to adjust rendering)
    render_braille_canvas(frame, &canvas, area, rtt_ms.is_some());

    // Render overlay message if present (on top of everything)
    if let Some(overlay_message) = overlay {
        render_overlay(frame, overlay_message, area);
    }
}

fn draw_braille_paddle_at(
    canvas: &mut BrailleCanvas,
    pixel_y: usize,
    vh: f32,
    vx: f32,
    scale_x: f32,
    scale_y: f32,
    color: Option<Color>,
) {
    // Convert virtual X coordinate to Braille pixel coordinates
    let pixel_x = (vx * scale_x) as usize;
    let pixel_height = (vh * scale_y) as usize;
    let pixel_width = (PADDLE_WIDTH * scale_x) as usize;

    // Draw solid rectangle with color
    canvas.fill_rect_with_color(pixel_x, pixel_y, pixel_width, pixel_height, color);
}

fn draw_braille_ball_at(
    canvas: &mut BrailleCanvas,
    vx: f32,
    pixel_y: usize,
    scale_x: f32,
    scale_y: f32,
    color: Option<Color>,
) {
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
    canvas.fill_rect_with_color(ball_x, ball_y, ball_pixel_width, ball_pixel_height, color);
}

fn draw_center_line_at(
    canvas: &mut BrailleCanvas,
    scale_x: f32,
    offset_y: usize,
    height: usize,
    field_width: f32,
    color: Option<Color>,
) {
    let center_pixel_x = (field_width / 2.0 * scale_x) as usize;

    // Draw dotted center line with 4-pixel padding from borders
    // This ensures center line doesn't share Braille cells with top/bottom borders
    const BORDER_PADDING: usize = 4; // One full Braille row
    let start_y = BORDER_PADDING;
    let end_y = height.saturating_sub(BORDER_PADDING);

    // Draw dotted center line (every 4 pixels = one row, draw 2 pixels per dot)
    for y in (start_y..end_y).step_by(4) {
        let pixel_y = offset_y + y;
        canvas.set_pixel_with_color(center_pixel_x, pixel_y, color);
        canvas.set_pixel_with_color(center_pixel_x, pixel_y + 1, color);
    }
}

fn draw_rtt(frame: &mut Frame, area: Rect, rtt_ms: u64) {
    // Show RTT in top right corner
    let rtt_text = if rtt_ms > 0 {
        format!("RTT: {}ms", rtt_ms)
    } else {
        "RTT: ---".to_string()
    };

    let rtt_color = if rtt_ms < 50 {
        Color::Green
    } else if rtt_ms < 100 {
        Color::Yellow
    } else {
        Color::Red
    };

    let width = rtt_text.len() as u16;
    let left_offset = 2;

    let rtt_widget = Paragraph::new(rtt_text).style(Style::default().fg(rtt_color));

    let rtt_area = Rect {
        x: area.x + area.width.saturating_sub(width + left_offset),
        y: area.y,
        width,
        height: 1,
    };

    frame.render_widget(rtt_widget, rtt_area);
}

fn render_braille_canvas(frame: &mut Frame, canvas: &BrailleCanvas, area: Rect, show_rtt: bool) {
    // Render each row of the Braille canvas
    // For row 0 (where RTT is), render left portion only IF RTT is being displayed
    // For row 3 (where game over is), render left and right segments (skip center fifth)

    for y in 0..canvas.pixel_height() / 4 {
        let cell_width = canvas.pixel_width() / 2;

        if y == 3 {
            // Special handling for row 3: render in two segments to skip center fifth

            // Left segment: 0 to 2/5 (40%)
            let left_segment_width = (cell_width * 2 / 5).max(1);
            let mut left_spans = Vec::new();
            for x in 0..left_segment_width {
                let ch = canvas.to_char(x, y);
                let color = canvas.get_color(x, y).unwrap_or(Color::White);
                let display_ch = if ch == '\u{2800}' { ' ' } else { ch };
                left_spans.push(Span::styled(
                    display_ch.to_string(),
                    Style::default().fg(color),
                ));
            }

            let left_paragraph = Paragraph::new(Line::from(left_spans));

            let left_area = Rect {
                x: area.x,
                y: area.y + y as u16,
                width: left_segment_width as u16,
                height: 1,
            };

            frame.render_widget(left_paragraph, left_area);

            // Right segment: 3/5 (60%) to end
            let right_start = cell_width * 3 / 5;
            let right_segment_width = cell_width - right_start;
            let mut right_spans = Vec::new();
            for x in right_start..cell_width {
                let ch = canvas.to_char(x, y);
                let color = canvas.get_color(x, y).unwrap_or(Color::White);
                let display_ch = if ch == '\u{2800}' { ' ' } else { ch };
                right_spans.push(Span::styled(
                    display_ch.to_string(),
                    Style::default().fg(color),
                ));
            }

            let right_paragraph = Paragraph::new(Line::from(right_spans));

            let right_area = Rect {
                x: area.x + right_start as u16,
                y: area.y + y as u16,
                width: right_segment_width as u16,
                height: 1,
            };

            frame.render_widget(right_paragraph, right_area);
        } else {
            // Normal rendering for other rows
            let mut spans = Vec::new();

            // For row 0, only reserve space for RTT if it's actually being shown
            let render_width = if y == 0 && show_rtt {
                (cell_width * 7 / 10).max(1)
            } else {
                cell_width
            };

            for x in 0..render_width {
                let ch = canvas.to_char(x, y);
                let color = canvas.get_color(x, y).unwrap_or(Color::White);
                // Convert empty Braille to space so text can show through
                let display_ch = if ch == '\u{2800}' { ' ' } else { ch };
                spans.push(Span::styled(
                    display_ch.to_string(),
                    Style::default().fg(color),
                ));
            }

            let paragraph = Paragraph::new(Line::from(spans));

            let row_area = Rect {
                x: area.x,
                y: area.y + y as u16,
                width: render_width as u16,
                height: 1,
            };

            frame.render_widget(paragraph, row_area);
        }
    }
}

fn draw_braille_scores(canvas: &mut BrailleCanvas, state: &GameState, color: Option<Color>) {
    // Each digit is 10 pixels wide × 16 pixels tall (4 cell rows)
    // Two-digit numbers: 22 pixels wide (10 + 2 gap + 10)
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
    canvas.draw_number(state.left_score, left_score_x, score_y, color);

    // Draw right score
    canvas.draw_number(state.right_score, right_score_x, score_y, color);
}
