use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::game::{state::{VIRTUAL_WIDTH, VIRTUAL_HEIGHT}, GameState, Player};

const UI_HEADER_ROWS: u16 = 3; // Score + controls + spacing
const PADDLE_MARGIN_SCREEN: u16 = 2; // Margin from edge in screen coords

// Coordinate mapper - converts virtual game coords to terminal screen coords
struct CoordMapper {
    scale_x: f32,
    scale_y: f32,
    screen_width: u16,
    screen_height: u16,
}

impl CoordMapper {
    fn new(screen_width: u16, screen_height: u16) -> Self {
        let playable_height = screen_height.saturating_sub(UI_HEADER_ROWS);
        
        Self {
            scale_x: screen_width as f32 / VIRTUAL_WIDTH,
            scale_y: playable_height as f32 / VIRTUAL_HEIGHT,
            screen_width,
            screen_height,
        }
    }
    
    // Convert virtual (x, y) to screen coordinates
    fn to_screen(&self, vx: f32, vy: f32) -> (u16, u16) {
        let sx = (vx * self.scale_x).clamp(0.0, (self.screen_width - 1) as f32) as u16;
        let sy = (vy * self.scale_y).clamp(0.0, (self.screen_height - UI_HEADER_ROWS - 1) as f32) as u16;
        
        (sx, sy + UI_HEADER_ROWS)
    }
    
    // Convert virtual height to screen height
    fn to_screen_height(&self, vh: f32) -> u16 {
        (vh * self.scale_y).max(1.0) as u16
    }
    
    // Convert virtual width to screen width
    fn to_screen_width(&self, vw: f32) -> u16 {
        (vw * self.scale_x).max(1.0) as u16
    }
}

pub fn render(frame: &mut Frame, state: &GameState) {
    let area = frame.area();
    let mapper = CoordMapper::new(area.width, area.height);

    // Draw background
    let bg = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(bg, area);

    // Draw scores at the top
    draw_scores(frame, state, area);

    // Draw center line
    draw_center_line(frame, area);

    // Draw paddles (convert virtual coords to screen coords)
    draw_paddle(
        frame,
        state.left_paddle.y,
        state.left_paddle.height,
        PADDLE_MARGIN_SCREEN,
        &mapper,
    );
    
    let right_paddle_x = area.width.saturating_sub(PADDLE_MARGIN_SCREEN + mapper.to_screen_width(4.0));
    draw_paddle(
        frame,
        state.right_paddle.y,
        state.right_paddle.height,
        right_paddle_x,
        &mapper,
    );

    // Draw ball
    draw_ball(frame, state.ball.x, state.ball.y, &mapper);

    // Draw game over screen if needed
    if state.game_over {
        draw_game_over(frame, state, area);
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

fn draw_center_line(frame: &mut Frame, area: Rect) {
    let center_x = area.width / 2;
    
    for y in UI_HEADER_ROWS..area.height {
        if y % 2 == 0 {
            let line = Paragraph::new("│")
                .style(Style::default().fg(Color::DarkGray));
            
            let line_area = Rect {
                x: center_x,
                y,
                width: 1,
                height: 1,
            };
            
            frame.render_widget(line, line_area);
        }
    }
}

fn draw_paddle(frame: &mut Frame, vy: f32, vh: f32, screen_x: u16, mapper: &CoordMapper) {
    // Sub-pixel rendering using half-block characters
    // Calculate exact screen position with fractional part
    let exact_y = (vy * mapper.scale_y) + UI_HEADER_ROWS as f32;
    let exact_height = vh * mapper.scale_y;
    
    let start_row = exact_y.floor() as u16;
    let end_row = (exact_y + exact_height).ceil() as u16;
    
    let screen_width = mapper.to_screen_width(4.0);
    
    for row in start_row..end_row {
        if row < UI_HEADER_ROWS || row >= mapper.screen_height {
            continue;
        }
        
        // Calculate fractional coverage of this row
        let row_start = row as f32;
        let row_end = (row + 1) as f32;
        
        let paddle_top_in_row = exact_y.max(row_start) - row_start;
        let paddle_bottom_in_row = (exact_y + exact_height).min(row_end) - row_start;
        
        // Determine which character to use based on coverage
        let paddle_char = if paddle_top_in_row <= 0.25 && paddle_bottom_in_row >= 0.75 {
            // Covers most/all of the cell
            "█".repeat(screen_width as usize)
        } else if paddle_bottom_in_row <= 0.5 {
            // Only covers top half
            "▀".repeat(screen_width as usize)
        } else if paddle_top_in_row >= 0.5 {
            // Only covers bottom half
            "▄".repeat(screen_width as usize)
        } else {
            // Covers full cell
            "█".repeat(screen_width as usize)
        };
        
        let paddle = Paragraph::new(paddle_char)
            .style(Style::default().fg(Color::White));
        
        let paddle_area = Rect {
            x: screen_x,
            y: row,
            width: screen_width,
            height: 1,
        };
        
        frame.render_widget(paddle, paddle_area);
    }
}

fn draw_ball(frame: &mut Frame, vx: f32, vy: f32, mapper: &CoordMapper) {
    // Sub-pixel rendering for the ball using half-blocks
    let exact_x = (vx * mapper.scale_x);
    let exact_y = (vy * mapper.scale_y) + UI_HEADER_ROWS as f32;
    
    let ball_x = exact_x.floor() as u16;
    let ball_y = exact_y.floor() as u16;
    
    // Get fractional parts to determine which half-block to use
    let frac_y = exact_y - exact_y.floor();
    
    // Choose character based on vertical position within the cell
    let ball_char = if frac_y < 0.33 {
        "▀"  // Top half
    } else if frac_y > 0.66 {
        "▄"  // Bottom half
    } else {
        "●"  // Middle (full circle)
    };
    
    let ball = Paragraph::new(ball_char)
        .style(Style::default().fg(Color::White));
    
    let ball_area = Rect {
        x: ball_x.min(mapper.screen_width.saturating_sub(1)),
        y: ball_y.min(mapper.screen_height.saturating_sub(1)),
        width: 1,
        height: 1,
    };
    
    frame.render_widget(ball, ball_area);
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
                .style(Style::default().bg(Color::Black))
        );

    let popup_area = Rect {
        x: area.width / 4,
        y: area.height / 3,
        width: area.width / 2,
        height: 8,
    };

    frame.render_widget(game_over, popup_area);
}
