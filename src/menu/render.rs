// Menu rendering with Ratatui

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::state::{MenuItem, MenuState};

/// Render the main menu
pub fn render_menu(frame: &mut Frame, menu_state: &MenuState) {
    let area = frame.area();

    // Draw background
    let bg = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(bg, area);

    // Create layout with title area and menu area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),  // Title area
            Constraint::Percentage(50),  // Menu items (centered)
            Constraint::Percentage(15),  // Controls hint
        ])
        .split(area);

    // Draw ASCII art title
    let title_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  ██████╗ ██████╗ ██████╗  ██████╗ ███╗   ██╗ ██████╗ ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██╔══██╗╚════██╗██╔══██╗██╔═══██╗████╗  ██║██╔════╝ ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██████╔╝ █████╔╝██████╔╝██║   ██║██╔██╗ ██║██║  ███╗",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██╔═══╝ ██╔═══╝ ██╔═══╝ ██║   ██║██║╚██╗██║██║   ██║",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ██║     ███████╗██║     ╚██████╔╝██║ ╚████║╚██████╔╝",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ╚═╝     ╚══════╝╚═╝      ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝ ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
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
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    // Draw current input
    let input_text = if peer_id.is_empty() {
        Span::styled("(paste or type peer ID)", Style::default().fg(Color::DarkGray))
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

    // Split dialog into bot list and hint area
    let inner = dialog_area.inner(ratatui::layout::Margin::new(2, 1));
    let dialog_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    // Render bot list
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
    frame.render_widget(bot_list, dialog_chunks[0]);

    // Draw hint
    let hint = Line::from(vec![
        Span::styled("↑/↓", Style::default().fg(Color::Gray)),
        Span::styled(": Navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Gray)),
        Span::styled(": Select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::Gray)),
        Span::styled(": Cancel", Style::default().fg(Color::DarkGray)),
    ]);

    let hint_widget = Paragraph::new(hint).alignment(Alignment::Center);
    frame.render_widget(hint_widget, dialog_chunks[1]);
}

/// Render waiting for connection screen (for host mode)
pub fn render_waiting_for_connection(
    frame: &mut Frame,
    peer_id: &str,
    copy_feedback: &str,
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
        // Show copy feedback or "Press C to copy", always show "Q to cancel"
        if !copy_feedback.is_empty() {
            Line::from(vec![
                Span::styled(copy_feedback, Style::default().fg(Color::Green)),
                Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Q", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("C", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" to copy  |  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Q", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" to cancel", Style::default().fg(Color::DarkGray)),
            ])
        },
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
