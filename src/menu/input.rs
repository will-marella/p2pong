// Menu input handling

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::io;
use std::time::Duration;

use super::state::{GameMode, MenuItem, MenuState};

/// Menu action result
pub enum MenuAction {
    /// Continue in menu
    None,
    /// Start a game mode
    StartGame(GameMode),
    /// Exit application
    Quit,
}

/// Handle menu input and return the next action
pub fn handle_menu_input(menu_state: &mut MenuState) -> Result<MenuAction, io::Error> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                return Ok(handle_key_press(menu_state, key.code));
            }
        }
    }

    Ok(MenuAction::None)
}

fn handle_key_press(menu_state: &mut MenuState, key_code: KeyCode) -> MenuAction {
    // If in bot selection mode, handle that first
    if menu_state.in_bot_selection_mode {
        return handle_bot_selection_input(menu_state, key_code);
    }

    // If in peer ID input mode, handle input differently
    if menu_state.in_input_mode {
        return handle_peer_id_input(menu_state, key_code);
    }

    // Normal menu navigation
    match key_code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            menu_state.select_previous();
            MenuAction::None
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            menu_state.select_next();
            MenuAction::None
        }
        KeyCode::Enter | KeyCode::Char(' ') => handle_menu_selection(menu_state),
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => MenuAction::Quit,
        _ => MenuAction::None,
    }
}

fn handle_menu_selection(menu_state: &mut MenuState) -> MenuAction {
    match menu_state.selected_item() {
        MenuItem::LocalTwoPlayer => MenuAction::StartGame(GameMode::LocalTwoPlayer),
        MenuItem::HostP2P => MenuAction::StartGame(GameMode::NetworkHost),
        MenuItem::JoinP2P => {
            // Enter peer ID input mode
            menu_state.start_peer_id_input();
            MenuAction::None
        }
        MenuItem::SinglePlayerAI => {
            // Enter bot selection mode
            menu_state.start_bot_selection();
            MenuAction::None
        }
        MenuItem::Quit => MenuAction::Quit,
    }
}

fn handle_peer_id_input(menu_state: &mut MenuState, key_code: KeyCode) -> MenuAction {
    match key_code {
        KeyCode::Enter => {
            let peer_id = menu_state.submit_peer_id();
            if !peer_id.is_empty() {
                MenuAction::StartGame(GameMode::NetworkClient(peer_id))
            } else {
                MenuAction::None
            }
        }
        KeyCode::Esc => {
            menu_state.cancel_peer_id_input();
            MenuAction::None
        }
        KeyCode::Backspace => {
            menu_state.backspace_peer_id();
            MenuAction::None
        }
        KeyCode::Char(c) => {
            // Add character to peer ID (alphanumeric and hyphens only)
            if c.is_alphanumeric() || c == '-' {
                menu_state.add_char_to_peer_id(c);
            }
            MenuAction::None
        }
        _ => MenuAction::None,
    }
}

fn handle_bot_selection_input(menu_state: &mut MenuState, key_code: KeyCode) -> MenuAction {
    match key_code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            menu_state.select_previous_bot();
            MenuAction::None
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            menu_state.select_next_bot();
            MenuAction::None
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let bot_type = menu_state.submit_bot_selection();
            MenuAction::StartGame(GameMode::SinglePlayerAI(bot_type))
        }
        KeyCode::Esc => {
            menu_state.cancel_bot_selection();
            MenuAction::None
        }
        _ => MenuAction::None,
    }
}
