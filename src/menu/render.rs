// Menu rendering with Ratatui

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::state::MenuState;
use crate::config::Config;
use crate::game::physics::{BALL_SIZE, VIRTUAL_HEIGHT, VIRTUAL_WIDTH};
use crate::ui::braille::BrailleCanvas;

/// Render the main menu
pub fn render_menu(frame: &mut Frame, menu_state: &MenuState, config: &Config) {
    let area = frame.area();

    // === LAYER 1: Braille Canvas for Ball ===
    let canvas_width = area.width as usize;
    let canvas_height = area.height as usize;
    let mut canvas = BrailleCanvas::new(canvas_width, canvas_height);

    // Draw bouncing ball (reusing game rendering logic)
    draw_menu_ball(&mut canvas, &menu_state.animation_ball, area, config);

    // Render canvas to frame
    render_canvas_to_frame(frame, &canvas, area);

    // === LAYER 2: Menu Content (Ratatui widgets on top) ===

    // Create layout with title area and menu area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35), // Title area
            Constraint::Percentage(50), // Menu items (centered)
            Constraint::Percentage(15), // Controls hint
        ])
        .split(area);

    // Draw ASCII art title
    let title_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  ██████╗ ██████╗ ██████╗  ██████╗ ███╗   ██╗ ██████╗ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██╔══██╗╚════██╗██╔══██╗██╔═══██╗████╗  ██║██╔════╝ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██████╔╝ █████╔╝██████╔╝██║   ██║██╔██╗ ██║██║  ███╗",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██╔═══╝ ██╔═══╝ ██╔═══╝ ██║   ██║██║╚██╗██║██║   ██║",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██║     ███████╗██║     ╚██████╔╝██║ ╚████║╚██████╔╝",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ╚═╝     ╚══════╝╚═╝      ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(""),
    ];

    let title = Paragraph::new(title_text).alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Draw menu items
    let menu_items: Vec<Line> = menu_state
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == menu_state.selected_index;
            let prefix = if is_selected { "  > " } else { "    " };
            let text = format!("{}{}", prefix, item.display_text());

            if is_selected {
                Line::from(Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(text, Style::default().fg(Color::White)))
            }
        })
        .collect();

    let menu = Paragraph::new(menu_items).alignment(Alignment::Center);
    frame.render_widget(menu, chunks[1]);

    // Draw controls hint
    let controls = vec![Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Gray)),
        Span::styled(": Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Gray)),
        Span::styled(": Select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Q/Esc", Style::default().fg(Color::Gray)),
        Span::styled(": Quit", Style::default().fg(Color::DarkGray)),
    ])];

    let controls_widget = Paragraph::new(controls).alignment(Alignment::Center);
    frame.render_widget(controls_widget, chunks[2]);

    // Show appropriate dialog overlay
    if menu_state.in_bot_selection_mode {
        render_bot_selection_dialog(frame, menu_state);
    } else if menu_state.in_input_mode {
        render_peer_id_dialog(frame, &menu_state.peer_id_input);
    }
}

/// Render peer ID input dialog overlay
fn render_peer_id_dialog(frame: &mut Frame, peer_id: &str) {
    let area = frame.area();

    // Create centered dialog box
    let dialog_width = 60.min(area.width - 4);
    let dialog_height = 7;
    let dialog_area = Rect {
        x: (area.width - dialog_width) / 2,
        y: (area.height - dialog_height) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Draw dialog border
    let block = Block::default()
        .title(" Enter Peer ID ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Rgb(20, 20, 20)));

    frame.render_widget(block, dialog_area);

    // Split dialog into input area and hint area
    let inner = dialog_area.inner(ratatui::layout::Margin::new(2, 1));
    let dialog_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    // Draw current input
    let input_text = if peer_id.is_empty() {
        Span::styled("(type peer ID)", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(peer_id, Style::default().fg(Color::White))
    };

    let input_widget = Paragraph::new(Line::from(input_text));
    frame.render_widget(input_widget, dialog_chunks[0]);

    // Draw hint
    let hint = Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Gray)),
        Span::styled(": Confirm  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Gray)),
        Span::styled(": Cancel", Style::default().fg(Color::DarkGray)),
    ]);

    let hint_widget = Paragraph::new(hint).alignment(Alignment::Center);
    frame.render_widget(hint_widget, dialog_chunks[2]);
}

/// Render bot selection dialog overlay
fn render_bot_selection_dialog(frame: &mut Frame, menu_state: &MenuState) {
    let area = frame.area();

    // Create centered dialog box (similar to peer ID dialog)
    let dialog_width = 50.min(area.width - 4);
    let bot_count = menu_state.available_bots.len();
    let dialog_height = (bot_count + 4).min(20) as u16;

    let dialog_area = Rect {
        x: (area.width - dialog_width) / 2,
        y: (area.height - dialog_height) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Draw dialog border
    let block = Block::default()
        .title(" Select Bot Opponent ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Rgb(20, 20, 20)));

    frame.render_widget(block, dialog_area);

    // Render bot list with vertical centering
    let inner = dialog_area.inner(ratatui::layout::Margin::new(2, 1));

    let bot_count = menu_state.available_bots.len() as u16;
    let dialog_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((inner.height.saturating_sub(bot_count)) / 2), // Top spacing
            Constraint::Length(bot_count),                                    // Bot list
            Constraint::Min(0),                                               // Bottom spacing
        ])
        .split(inner);

    let bot_items: Vec<Line> = menu_state
        .available_bots
        .iter()
        .enumerate()
        .map(|(i, bot_type)| {
            let is_selected = i == menu_state.selected_bot_index;
            let prefix = if is_selected { "> " } else { "  " };
            let text = format!("{}{}", prefix, bot_type.display_name());

            if is_selected {
                Line::from(Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(text, Style::default().fg(Color::White)))
            }
        })
        .collect();

    let bot_list = Paragraph::new(bot_items);
    frame.render_widget(bot_list, dialog_chunks[1]);
}

/// Render connecting to peer screen (for client mode)
pub fn render_connecting_to_peer(
    frame: &mut Frame,
    target_peer_id: &str,
    overlay: Option<&crate::ui::OverlayMessage>,
) {
    let area = frame.area();

    // Draw background
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Only show the connecting UI if there's no overlay (clean background for errors)
    if overlay.is_none() {
        // Create centered layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Min(10),
                Constraint::Percentage(30),
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Connecting to peer...",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        // Peer ID box
        let peer_id_lines = vec![
            Line::from(Span::styled(
                "Target Peer ID:",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                target_peer_id,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Q",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
            ]),
        ];

        let peer_id_widget = Paragraph::new(peer_id_lines)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .style(Style::default().bg(Color::Rgb(20, 20, 20))),
            );

        // Center the peer ID box
        let box_width = (target_peer_id.len() as u16 + 10)
            .max(50)
            .min(area.width - 4);
        let peer_id_area = Rect {
            x: (area.width.saturating_sub(box_width)) / 2,
            y: chunks[1].y,
            width: box_width,
            height: 7,
        };

        frame.render_widget(peer_id_widget, peer_id_area);
    }

    // Render overlay if provided (on clean background if error)
    if let Some(overlay_msg) = overlay {
        crate::ui::overlay::render_overlay(frame, overlay_msg, area);
    }
}

/// Render waiting for connection screen (for host mode)
pub fn render_waiting_for_connection(
    frame: &mut Frame,
    peer_id: &str,
    overlay: Option<&crate::ui::OverlayMessage>,
) {
    let area = frame.area();

    // Draw background
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Create centered layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(10),
            Constraint::Percentage(30),
        ])
        .split(area);

    // Title
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Waiting for connection...",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Peer ID box
    let peer_id_lines = vec![
        Line::from(Span::styled(
            "Share this Peer ID:",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            peer_id,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Q",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let peer_id_widget = Paragraph::new(peer_id_lines)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .style(Style::default().bg(Color::Rgb(20, 20, 20))),
        );

    // Center the peer ID box (constant height now)
    let box_width = (peer_id.len() as u16 + 10).max(50).min(area.width - 4);
    let peer_id_area = Rect {
        x: (area.width.saturating_sub(box_width)) / 2,
        y: chunks[1].y,
        width: box_width,
        height: 7,
    };

    frame.render_widget(peer_id_widget, peer_id_area);

    // Render overlay if provided
    if let Some(overlay_msg) = overlay {
        crate::ui::overlay::render_overlay(frame, overlay_msg, area);
    }
}

/// Draw menu ball using game's Ball struct and rendering logic
/// Ball coordinates are in VIRTUAL coordinates (1200×600), just like the game
fn draw_menu_ball(
    canvas: &mut BrailleCanvas,
    ball: &crate::game::state::Ball,
    _area: Rect,
    config: &Config,
) {
    // Calculate scale from VIRTUAL coordinates to Braille pixels
    // This is exactly how the game does it!
    let scale_x = canvas.pixel_width() as f32 / VIRTUAL_WIDTH;
    let scale_y = canvas.pixel_height() as f32 / VIRTUAL_HEIGHT;

    // Convert ball position from virtual coords to Braille pixels
    let ball_pixel_x = (ball.x * scale_x) as usize;
    let ball_pixel_y = (ball.y * scale_y) as usize;

    // Ball size in Braille pixels (scaled from virtual size)
    let ball_pixel_width = (BALL_SIZE * scale_x) as usize;
    let ball_pixel_height = (BALL_SIZE * scale_y) as usize;

    // Calculate top-left corner (ball.x/y is the center)
    let ball_x = ball_pixel_x.saturating_sub(ball_pixel_width / 2);
    let ball_y = ball_pixel_y.saturating_sub(ball_pixel_height / 2);

    // Use ball color from config (same as game!)
    let ball_color = Some(Color::Rgb(
        config.display.ball_color[0],
        config.display.ball_color[1],
        config.display.ball_color[2],
    ));

    // Draw ball (reusing game rendering approach)
    canvas.fill_rect_with_color(
        ball_x,
        ball_y,
        ball_pixel_width,
        ball_pixel_height,
        ball_color,
    );
}

/// Render Braille canvas to frame
fn render_canvas_to_frame(frame: &mut Frame, canvas: &BrailleCanvas, area: Rect) {
    // Convert each row of canvas to Ratatui widgets
    for y in 0..(canvas.pixel_height() / 4) {
        let cell_width = canvas.pixel_width() / 2;
        let mut spans = Vec::new();

        for x in 0..cell_width {
            let ch = canvas.to_char(x, y);
            let color = canvas.get_color(x, y).unwrap_or(Color::White);

            // Convert empty Braille to space for transparency
            let display_ch = if ch == '\u{2800}' { ' ' } else { ch };

            spans.push(Span::styled(
                display_ch.to_string(),
                Style::default().fg(color).bg(Color::Rgb(0, 0, 0)),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans));
        let row_area = Rect {
            x: area.x,
            y: area.y + y as u16,
            width: cell_width as u16,
            height: 1,
        };

        frame.render_widget(paragraph, row_area);
    }
}
