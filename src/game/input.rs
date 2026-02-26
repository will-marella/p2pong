use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseEventKind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputAction {
    Quit,
    Rematch,
    LeftPaddleUp,
    LeftPaddleDown,
    RightPaddleUp,
    RightPaddleDown,
}

/// Parse a key binding string (e.g., "W", "Up", "Esc") into a KeyCode
fn parse_key_binding(key_str: &str) -> Option<KeyCode> {
    match key_str.to_lowercase().as_str() {
        "w" => Some(KeyCode::Char('w')),
        "s" => Some(KeyCode::Char('s')),
        "a" => Some(KeyCode::Char('a')),
        "d" => Some(KeyCode::Char('d')),
        "q" => Some(KeyCode::Char('q')),
        "r" => Some(KeyCode::Char('r')),
        "p" => Some(KeyCode::Char('p')),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "esc" | "escape" => Some(KeyCode::Esc),
        "enter" => Some(KeyCode::Enter),
        "space" => Some(KeyCode::Char(' ')),
        _ => None,
    }
}

/// Check if a KeyCode matches a config key binding string
fn matches_key(code: &KeyCode, binding: &str) -> bool {
    parse_key_binding(binding) == Some(*code)
}

/// Poll input for local 2-player mode (asymmetric controls)
pub fn poll_input_local_2p(config: &Config) -> Result<Vec<InputAction>, std::io::Error> {
    let bindings = &config.keybindings;
    let mut actions = Vec::new();

    while event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // System actions
                if matches_key(&key.code, &bindings.quit) || key.code == KeyCode::Esc {
                    actions.push(InputAction::Quit);
                }

                // Rematch (always 'R' for now)
                if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                    actions.push(InputAction::Rematch);
                }

                // Left paddle
                if matches_key(&key.code, &bindings.left_paddle_up) {
                    actions.push(InputAction::LeftPaddleUp);
                }
                if matches_key(&key.code, &bindings.left_paddle_down) {
                    actions.push(InputAction::LeftPaddleDown);
                }

                // Right paddle
                if matches_key(&key.code, &bindings.right_paddle_up) {
                    actions.push(InputAction::RightPaddleUp);
                }
                if matches_key(&key.code, &bindings.right_paddle_down) {
                    actions.push(InputAction::RightPaddleDown);
                }
            }
        }
    }

    Ok(actions)
}

/// Poll input for single-player modes where player controls LEFT paddle
#[allow(dead_code)]
pub fn poll_input_player_left(config: &Config) -> Result<Vec<InputAction>, std::io::Error> {
    let bindings = &config.keybindings;
    let mut actions = Vec::new();

    while event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // System actions
                if matches_key(&key.code, &bindings.quit) || key.code == KeyCode::Esc {
                    actions.push(InputAction::Quit);
                }

                // Rematch (always 'R' for now)
                if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                    actions.push(InputAction::Rematch);
                }

                // Player paddle (maps to LEFT paddle actions)
                if matches_key(&key.code, &bindings.player_paddle_up) {
                    actions.push(InputAction::LeftPaddleUp);
                }
                if matches_key(&key.code, &bindings.player_paddle_down) {
                    actions.push(InputAction::LeftPaddleDown);
                }
            }
        }
    }

    Ok(actions)
}

/// Poll input for single-player modes where player controls RIGHT paddle
#[allow(dead_code)]
pub fn poll_input_player_right(config: &Config) -> Result<Vec<InputAction>, std::io::Error> {
    let bindings = &config.keybindings;
    let mut actions = Vec::new();

    while event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // System actions
                if matches_key(&key.code, &bindings.quit) || key.code == KeyCode::Esc {
                    actions.push(InputAction::Quit);
                }

                // Rematch (always 'R' for now)
                if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                    actions.push(InputAction::Rematch);
                }

                // Player paddle (maps to RIGHT paddle actions)
                if matches_key(&key.code, &bindings.player_paddle_up) {
                    actions.push(InputAction::RightPaddleUp);
                }
                if matches_key(&key.code, &bindings.player_paddle_down) {
                    actions.push(InputAction::RightPaddleDown);
                }
            }
        }
    }

    Ok(actions)
}

/// Convert mouse row position to virtual Y coordinate for paddle
/// Centers the paddle on the mouse cursor position
fn convert_mouse_row_to_virtual_y(
    mouse_row: u16,
    terminal_height: u16,
    field_height: f32,
    paddle_height: f32,
) -> f32 {
    // Map mouse row to full terminal height range [0..terminal_height] -> [0..field_height]
    let normalized = mouse_row as f32 / terminal_height as f32;
    let virtual_y = normalized * field_height;

    // Center paddle on cursor position
    let paddle_y = virtual_y - (paddle_height / 2.0);

    // Clamp to valid bounds
    paddle_y.max(0.0).min(field_height - paddle_height)
}

/// Poll input for player controlling LEFT paddle with mouse support
/// Returns (keyboard actions, optional mouse paddle position)
pub fn poll_input_player_left_with_mouse(
    config: &Config,
    terminal_height: u16,
    field_height: f32,
    paddle_height: f32,
) -> Result<(Vec<InputAction>, Option<f32>), std::io::Error> {
    let bindings = &config.keybindings;
    let mut actions = Vec::new();
    let mut mouse_paddle_y = None;

    while event::poll(Duration::from_millis(0))? {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // System actions
                if matches_key(&key.code, &bindings.quit) || key.code == KeyCode::Esc {
                    actions.push(InputAction::Quit);
                }

                // Rematch (always 'R' for now)
                if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                    actions.push(InputAction::Rematch);
                }

                // Player paddle (maps to LEFT paddle actions)
                if matches_key(&key.code, &bindings.player_paddle_up) {
                    actions.push(InputAction::LeftPaddleUp);
                }
                if matches_key(&key.code, &bindings.player_paddle_down) {
                    actions.push(InputAction::LeftPaddleDown);
                }
            }
            Event::Mouse(mouse) if config.input.mouse_control_enabled => {
                if mouse.kind == MouseEventKind::Moved
                    || mouse.kind == MouseEventKind::Drag(event::MouseButton::Left)
                {
                    mouse_paddle_y = Some(convert_mouse_row_to_virtual_y(
                        mouse.row,
                        terminal_height,
                        field_height,
                        paddle_height,
                    ));
                }
            }
            _ => {}
        }
    }

    Ok((actions, mouse_paddle_y))
}

/// Poll input for player controlling RIGHT paddle with mouse support
/// Returns (keyboard actions, optional mouse paddle position)
pub fn poll_input_player_right_with_mouse(
    config: &Config,
    terminal_height: u16,
    field_height: f32,
    paddle_height: f32,
) -> Result<(Vec<InputAction>, Option<f32>), std::io::Error> {
    let bindings = &config.keybindings;
    let mut actions = Vec::new();
    let mut mouse_paddle_y = None;

    while event::poll(Duration::from_millis(0))? {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // System actions
                if matches_key(&key.code, &bindings.quit) || key.code == KeyCode::Esc {
                    actions.push(InputAction::Quit);
                }

                // Rematch (always 'R' for now)
                if key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R') {
                    actions.push(InputAction::Rematch);
                }

                // Player paddle (maps to RIGHT paddle actions)
                if matches_key(&key.code, &bindings.player_paddle_up) {
                    actions.push(InputAction::RightPaddleUp);
                }
                if matches_key(&key.code, &bindings.player_paddle_down) {
                    actions.push(InputAction::RightPaddleDown);
                }
            }
            Event::Mouse(mouse) if config.input.mouse_control_enabled => {
                if mouse.kind == MouseEventKind::Moved
                    || mouse.kind == MouseEventKind::Drag(event::MouseButton::Left)
                {
                    mouse_paddle_y = Some(convert_mouse_row_to_virtual_y(
                        mouse.row,
                        terminal_height,
                        field_height,
                        paddle_height,
                    ));
                }
            }
            _ => {}
        }
    }

    Ok((actions, mouse_paddle_y))
}
