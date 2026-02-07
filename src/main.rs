mod ai;
mod config;
mod debug;
mod game;
mod game_modes;
mod menu;
mod network;
mod ui;

// Standard library imports
use std::io;
use std::time::{Duration, Instant};

// External crate imports
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

// Internal crate imports
use config::Config;
use menu::{handle_menu_input, render_menu, AppState, GameMode, MenuAction, MenuState};

const TARGET_FPS: u64 = 60;
pub const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);
pub const FIXED_TIMESTEP: f32 = 1.0 / 60.0; // Fixed timestep for deterministic physics

// Network sync tuning parameters
pub const BACKUP_SYNC_INTERVAL: u64 = 3; // Frames between syncs (every 3 frames = ~50ms at 60 FPS, 20 syncs/sec)

// Dead reckoning configuration for client-side prediction
pub const POSITION_SNAP_THRESHOLD: f32 = 50.0; // Snap if error > 50 virtual units (collision happened)
pub const POSITION_CORRECTION_ALPHA: f32 = 0.3; // Gentle correction factor for small prediction errors

fn main() -> Result<(), io::Error> {
    // Check for --debug flag to enable diagnostic logging
    let debug_enabled = std::env::args().any(|arg| arg == "--debug" || arg == "-d");

    // Initialize debug logging system (opt-in via --debug flag)
    debug::init(debug_enabled)?;
    debug::log("SESSION_START", "P2Pong debug logging initialized");

    // Load configuration
    let config = config::load_config()?;

    // Setup terminal BEFORE entering app loop
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // AppState loop: Menu -> Game -> Menu
    let mut app_state = AppState::Menu;

    loop {
        match app_state {
            AppState::Menu => {
                app_state = run_menu(&mut terminal)?;
            }
            AppState::Game(mode) => {
                run_game_mode(&mut terminal, mode, &config)?;
                app_state = AppState::Menu;
            }
            AppState::Exiting => break,
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Run the main menu and return next app state
fn run_menu<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
) -> Result<AppState, io::Error> {
    let mut menu_state = MenuState::new();

    loop {
        // Render menu
        terminal.draw(|f| render_menu(f, &menu_state))?;

        // Handle input
        match handle_menu_input(&mut menu_state)? {
            MenuAction::None => {} // Continue in menu
            MenuAction::StartGame(mode) => {
                return Ok(AppState::Game(mode));
            }
            MenuAction::Quit => {
                return Ok(AppState::Exiting);
            }
        }
    }
}

/// Dispatch to appropriate game mode function
fn run_game_mode<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mode: GameMode,
    config: &Config,
) -> Result<(), io::Error> {
    match mode {
        GameMode::LocalTwoPlayer => game_modes::run_game_local(terminal, config),
        GameMode::NetworkHost => game_modes::run_game_network_host(terminal, config),
        GameMode::NetworkClient(peer_id) => {
            game_modes::run_game_network_client(terminal, config, &peer_id)
        }
        GameMode::SinglePlayerAI(bot_type) => game_modes::run_game_vs_ai(terminal, config, bot_type),
    }
}
