use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::game::{
    state::{VIRTUAL_WIDTH, VIRTUAL_HEIGHT},
    physics::{PADDLE_MARGIN, PADDLE_WIDTH, BALL_SIZE},
    GameState, Player
};
use super::braille::BrailleCanvas;

const UI_HEADER_ROWS: u16 = 3; // Score + controls + spacing

pub fn render(frame: &mut Frame, state: &GameState) {
    let area = frame.area();

    // Draw background (true black RGB, not terminal default)
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Draw scores at the top
    draw_scores(frame, state, area);

    // Create Braille canvas for game area
    let playable_width = area.width as usize;
    let playable_height = (area.height - UI_HEADER_ROWS) as usize;
    
    let mut canvas = BrailleCanvas::new(playable_width, playable_height);
    
    // Calculate scale from virtual to Braille pixels
    let scale_x = canvas.pixel_width() as f32 / VIRTUAL_WIDTH;
    let scale_y = canvas.pixel_height() as f32 / VIRTUAL_HEIGHT;
    
    // Draw paddles in Braille (use same X positions as physics)
    draw_braille_paddle(&mut canvas, state.left_paddle.y, state.left_paddle.height, PADDLE_MARGIN, scale_x, scale_y);
    
    let right_paddle_x = VIRTUAL_WIDTH - PADDLE_MARGIN - PADDLE_WIDTH;
    draw_braille_paddle(&mut canvas, state.right_paddle.y, state.right_paddle.height, right_paddle_x, scale_x, scale_y);
    
    // Draw ball in Braille (2×2 cell square)
    draw_braille_ball(&mut canvas, state.ball.x, state.ball.y, scale_x, scale_y);
    
    // Draw center line
    draw_center_line(&mut canvas, scale_x);
    
    // Render the Braille canvas
    render_braille_canvas(frame, &canvas, area);

    // Draw game over screen if needed
    if state.game_over {
        draw_game_over(frame, state, area);
    }
}

fn draw_braille_paddle(canvas: &mut BrailleCanvas, vy: f32, vh: f32, vx: f32, scale_x: f32, scale_y: f32) {
    // Convert virtual coordinates to Braille pixel coordinates
    let pixel_x = (vx * scale_x) as usize;
    let pixel_y = (vy * scale_y) as usize;
    let pixel_height = (vh * scale_y) as usize;
    let pixel_width = (PADDLE_WIDTH * scale_x) as usize;
    
    // Draw solid rectangle
    canvas.fill_rect(pixel_x, pixel_y, pixel_width, pixel_height);
}

fn draw_braille_ball(canvas: &mut BrailleCanvas, vx: f32, vy: f32, scale_x: f32, scale_y: f32) {
    // Ball position (vx, vy) is the CENTER of the ball in virtual coordinates
    // Convert BALL_SIZE from virtual coords to Braille pixels
    let ball_pixel_width = (BALL_SIZE * scale_x) as usize;
    let ball_pixel_height = (BALL_SIZE * scale_y) as usize;
    
    // Convert ball center to pixel coordinates
    let center_pixel_x = (vx * scale_x) as usize;
    let center_pixel_y = (vy * scale_y) as usize;
    
    // Calculate top-left corner (center the ball on its position)
    let ball_x = center_pixel_x.saturating_sub(ball_pixel_width / 2);
    let ball_y = center_pixel_y.saturating_sub(ball_pixel_height / 2);
    
    // Draw ball as solid rectangle
    canvas.fill_rect(ball_x, ball_y, ball_pixel_width, ball_pixel_height);
}

fn draw_center_line(canvas: &mut BrailleCanvas, scale_x: f32) {
    let center_pixel_x = (VIRTUAL_WIDTH / 2.0 * scale_x) as usize;
    let pixel_height = canvas.pixel_height();
    
    // Draw dotted center line (every other pixel)
    for y in (0..pixel_height).step_by(4) {
        canvas.set_pixel(center_pixel_x, y);
        canvas.set_pixel(center_pixel_x, y + 1);
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
            y: area.y + UI_HEADER_ROWS + y as u16,
            width: area.width,
            height: 1,
        };
        
        frame.render_widget(paragraph, row_area);
    }
}

fn draw_scores(frame: &mut Frame, state: &GameState, area: Rect) {
    let score_text = format!("{}  -  {}", state.left_score, state.right_score);
    let score = Paragraph::new(score_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    
    let score_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    
    frame.render_widget(score, score_area);

    // Draw controls hint
    let controls = Paragraph::new("W/S: Left Paddle  ↑/↓: Right Paddle  Q: Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    
    let controls_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: 1,
    };
    
    frame.render_widget(controls, controls_area);
}

fn draw_game_over(frame: &mut Frame, state: &GameState, area: Rect) {
    let winner_text = match state.winner {
        Some(Player::Left) => "Left Player Wins!",
        Some(Player::Right) => "Right Player Wins!",
        None => "Game Over",
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            winner_text,
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Final Score: {} - {}", state.left_score, state.right_score),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Q to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let game_over = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .style(Style::default().bg(Color::Rgb(0, 0, 0)))
        );

    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 8,
    };

    frame.render_widget(game_over, popup_area);
}
