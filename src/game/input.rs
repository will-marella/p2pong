use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::{Duration, Instant};

pub enum InputAction {
    Quit,
    LeftPaddleUp,
    LeftPaddleDown,
    LeftPaddleStop,
    RightPaddleUp,
    RightPaddleDown,
    RightPaddleStop,
}

pub struct InputState {
    w_pressed: bool,
    s_pressed: bool,
    up_pressed: bool,
    down_pressed: bool,
    // Track when each key was last seen pressed
    w_last_seen: Option<Instant>,
    s_last_seen: Option<Instant>,
    up_last_seen: Option<Instant>,
    down_last_seen: Option<Instant>,
}

const KEY_TIMEOUT_MS: u128 = 16; // One frame at 60 FPS

impl InputState {
    pub fn new() -> Self {
        Self {
            w_pressed: false,
            s_pressed: false,
            up_pressed: false,
            down_pressed: false,
            w_last_seen: None,
            s_last_seen: None,
            up_last_seen: None,
            down_last_seen: None,
        }
    }

    pub fn poll(&mut self, _timeout: Duration) -> Result<Vec<InputAction>, std::io::Error> {
        let mut actions = Vec::new();
        let now = Instant::now();

        // Process ALL pending events
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            actions.push(InputAction::Quit);
                        }
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            self.w_pressed = true;
                            self.w_last_seen = Some(now);
                            // Opposite key clearing: W clears S
                            self.s_pressed = false;
                            self.s_last_seen = None;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            self.s_pressed = true;
                            self.s_last_seen = Some(now);
                            // Opposite key clearing: S clears W
                            self.w_pressed = false;
                            self.w_last_seen = None;
                        }
                        KeyCode::Up => {
                            self.up_pressed = true;
                            self.up_last_seen = Some(now);
                            // Opposite key clearing: Up clears Down
                            self.down_pressed = false;
                            self.down_last_seen = None;
                        }
                        KeyCode::Down => {
                            self.down_pressed = true;
                            self.down_last_seen = Some(now);
                            // Opposite key clearing: Down clears Up
                            self.up_pressed = false;
                            self.up_last_seen = None;
                        }
                        _ => {}
                    }
                } else if key.kind == KeyEventKind::Release {
                    match key.code {
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            self.w_pressed = false;
                            self.w_last_seen = None;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            self.s_pressed = false;
                            self.s_last_seen = None;
                        }
                        KeyCode::Up => {
                            self.up_pressed = false;
                            self.up_last_seen = None;
                        }
                        KeyCode::Down => {
                            self.down_pressed = false;
                            self.down_last_seen = None;
                        }
                        _ => {}
                    }
                }
            }
        }

        // Timeout check: if key hasn't been seen in KEY_TIMEOUT_MS, assume it's released
        if let Some(last) = self.w_last_seen {
            if now.duration_since(last).as_millis() > KEY_TIMEOUT_MS {
                self.w_pressed = false;
                self.w_last_seen = None;
            }
        }
        if let Some(last) = self.s_last_seen {
            if now.duration_since(last).as_millis() > KEY_TIMEOUT_MS {
                self.s_pressed = false;
                self.s_last_seen = None;
            }
        }
        if let Some(last) = self.up_last_seen {
            if now.duration_since(last).as_millis() > KEY_TIMEOUT_MS {
                self.up_pressed = false;
                self.up_last_seen = None;
            }
        }
        if let Some(last) = self.down_last_seen {
            if now.duration_since(last).as_millis() > KEY_TIMEOUT_MS {
                self.down_pressed = false;
                self.down_last_seen = None;
            }
        }

        // ALWAYS send paddle commands based on current state (every frame)
        // This ensures paddles respond instantly without waiting for state changes
        
        // Left paddle
        if self.w_pressed && !self.s_pressed {
            actions.push(InputAction::LeftPaddleUp);
        } else if self.s_pressed && !self.w_pressed {
            actions.push(InputAction::LeftPaddleDown);
        } else {
            actions.push(InputAction::LeftPaddleStop);
        }

        // Right paddle
        if self.up_pressed && !self.down_pressed {
            actions.push(InputAction::RightPaddleUp);
        } else if self.down_pressed && !self.up_pressed {
            actions.push(InputAction::RightPaddleDown);
        } else {
            actions.push(InputAction::RightPaddleStop);
        }

        Ok(actions)
    }
}
