use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputAction {
    Quit,
    Rematch,
    LeftPaddleUp,
    LeftPaddleDown,
    RightPaddleUp,
    RightPaddleDown,
}

/// Poll for input events and return actions.
/// Each Press event generates an immediate action - no state tracking needed.
pub fn poll_input(_timeout: Duration) -> Result<Vec<InputAction>, std::io::Error> {
    let mut actions = Vec::new();

    // Process all pending Press events
    while event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        actions.push(InputAction::Quit);
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        actions.push(InputAction::Rematch);
                    }
                    KeyCode::Char('w') | KeyCode::Char('W') => {
                        actions.push(InputAction::LeftPaddleUp);
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        actions.push(InputAction::LeftPaddleDown);
                    }
                    KeyCode::Up => {
                        actions.push(InputAction::RightPaddleUp);
                    }
                    KeyCode::Down => {
                        actions.push(InputAction::RightPaddleDown);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(actions)
}
