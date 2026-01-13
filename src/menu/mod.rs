// Menu module for P2Pong
// Handles main menu UI, navigation, and game mode selection

pub mod input;
pub mod render;
pub mod state;

pub use input::{handle_menu_input, try_paste_from_clipboard, MenuAction};
pub use render::{render_menu, render_waiting_for_connection};
pub use state::{AppState, GameMode, MenuItem, MenuState};
