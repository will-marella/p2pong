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
    let exact_x = vx * mapper.scale_x;
    let exact_y = (vy * mapper.scale_y) + UI_HEADER_ROWS as f32;
    
    // Calculate which 2×2 grid the ball occupies (centered on the ball's position)
    let grid_x = (exact_x - 0.5).floor() as i32;
    let grid_y = (exact_y - 0.5).floor() as i32;
    
    // Get position within the 2×2 grid (0.0 to 1.0 for each)
    let _local_x = (exact_x - 0.5) - (exact_x - 0.5).floor();
    let local_y = (exact_y - 0.5) - (exact_y - 0.5).floor();
    
    // Determine pattern based on vertical position within grid
    // This creates a connected 2×2 shape that shifts smoothly
    let (top_left, top_right, bottom_left, bottom_right) = if local_y < 0.5 {
        // Ball in top half of grid - use full blocks on top, half-blocks on bottom
        ("█", "█", "▀", "▀")
    } else {
        // Ball in bottom half of grid - use half-blocks on top, full blocks on bottom
        ("▄", "▄", "█", "█")
    };
    
    // Draw all 4 cells of the 2×2 grid with bounds checking
    let cells = [
        (grid_x, grid_y, top_left),
        (grid_x + 1, grid_y, top_right),
        (grid_x, grid_y + 1, bottom_left),
        (grid_x + 1, grid_y + 1, bottom_right),
    ];
    
    for (cell_x, cell_y, character) in cells {
        // Convert to u16 with bounds checking
        if cell_x >= 0 && cell_y >= 0 {
            let ux = cell_x as u16;
            let uy = cell_y as u16;
            
            if ux < mapper.screen_width && uy < mapper.screen_height {
                let ball = Paragraph::new(character)
                    .style(Style::default().fg(Color::White));
                frame.render_widget(ball, Rect { x: ux, y: uy, width: 1, height: 1 });
            }
        }
    }
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
