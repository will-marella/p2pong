// Overlay message system for displaying centered text on screen

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// A message to display as an overlay in the center of the screen
#[derive(Debug, Clone)]
pub struct OverlayMessage {
    /// Lines of text to display
    pub lines: Vec<String>,
    /// Optional title for the overlay box
    pub title: Option<String>,
    /// Style preset for the overlay
    pub style: OverlayStyle,
}

/// Predefined styles for overlay messages
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayStyle {
    /// Informational message (white/gray)
    Info,
    /// Warning message (yellow)
    Warning,
    /// Error message (red)
    Error,
    /// Success message (green)
    Success,
}

impl OverlayMessage {
    /// Create a new overlay message with the given lines
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines,
            title: None,
            style: OverlayStyle::Info,
        }
    }

    /// Create an info-style message
    pub fn info(lines: Vec<String>) -> Self {
        Self {
            lines,
            title: None,
            style: OverlayStyle::Info,
        }
    }

    /// Create a warning-style message
    pub fn warning(lines: Vec<String>) -> Self {
        Self {
            lines,
            title: None,
            style: OverlayStyle::Warning,
        }
    }

    /// Create an error-style message
    pub fn error(lines: Vec<String>) -> Self {
        Self {
            lines,
            title: None,
            style: OverlayStyle::Error,
        }
    }

    /// Create a success-style message
    pub fn success(lines: Vec<String>) -> Self {
        Self {
            lines,
            title: None,
            style: OverlayStyle::Success,
        }
    }

    /// Set the title for this message
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    /// Get the color for the border and title based on style
    fn border_color(&self) -> Color {
        match self.style {
            OverlayStyle::Info => Color::Cyan,
            OverlayStyle::Warning => Color::Yellow,
            OverlayStyle::Error => Color::Red,
            OverlayStyle::Success => Color::Green,
        }
    }

    /// Get the color for the message text based on style
    fn text_color(&self) -> Color {
        match self.style {
            OverlayStyle::Info => Color::White,
            OverlayStyle::Warning => Color::Yellow,
            OverlayStyle::Error => Color::LightRed,
            OverlayStyle::Success => Color::LightGreen,
        }
    }
}

/// Render an overlay message in the center of the screen
pub fn render_overlay(frame: &mut Frame, message: &OverlayMessage, area: Rect) {
    // Calculate overlay dimensions based on content
    let max_line_length = message
        .lines
        .iter()
        .map(|line| line.len())
        .max()
        .unwrap_or(0);

    // Add padding for borders and spacing
    let overlay_width = (max_line_length as u16 + 6).min(area.width - 4);
    let overlay_height = (message.lines.len() as u16 + 4).min(area.height - 4);

    // Center the overlay
    let overlay_area = Rect {
        x: area.x + (area.width.saturating_sub(overlay_width)) / 2,
        y: area.y + (area.height.saturating_sub(overlay_height)) / 2,
        width: overlay_width,
        height: overlay_height,
    };

    // Clear the area behind the overlay
    frame.render_widget(Clear, overlay_area);

    // Create the border block
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(message.border_color()))
        .style(Style::default().bg(Color::Rgb(20, 20, 20)));

    if let Some(ref title) = message.title {
        block = block.title(format!(" {} ", title));
    }

    frame.render_widget(block, overlay_area);

    // Render the message text inside the block
    let inner_area = overlay_area.inner(ratatui::layout::Margin::new(2, 1));

    let text_lines: Vec<Line> = message
        .lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.clone(),
                Style::default().fg(message.text_color()),
            ))
        })
        .collect();

    let paragraph = Paragraph::new(text_lines).alignment(Alignment::Center);

    frame.render_widget(paragraph, inner_area);
}
