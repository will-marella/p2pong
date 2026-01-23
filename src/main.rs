mod ai;
mod config;
mod game;
mod menu;
mod network;
mod ui;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use ai::Bot;
use config::Config;
use game::{poll_input_local_2p, poll_input_player_left, poll_input_player_right, GameState, InputAction};
use menu::{handle_menu_input, render_menu, AppState, GameMode, MenuAction, MenuState};
use network::client::NetworkEvent;
use network::{BallState, ConnectionMode, NetworkMessage};

const TARGET_FPS: u64 = 60;
const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);
const FIXED_TIMESTEP: f32 = 1.0 / 60.0; // Fixed timestep for deterministic physics

// Network sync tuning parameters
const BACKUP_SYNC_INTERVAL: u64 = 3; // Frames between syncs (every 3 frames = ~50ms at 60 FPS, 20 syncs/sec)

// Dead reckoning configuration for client-side prediction
const POSITION_SNAP_THRESHOLD: f32 = 50.0; // Snap if error > 50 virtual units (collision happened)
const POSITION_CORRECTION_ALPHA: f32 = 0.3; // Gentle correction factor for small prediction errors

// Global sync state for sequence tracking
static BALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static LAST_RECEIVED_SEQUENCE: AtomicU64 = AtomicU64::new(0);

// RTT (Round-Trip Time) tracking
static LAST_RTT_MS: AtomicU64 = AtomicU64::new(0);

// Input logging counter (for diagnostics)
static INPUT_SEND_COUNT: AtomicU64 = AtomicU64::new(0);

fn main() -> Result<(), io::Error> {
    // Initialize file-based diagnostic logging
    init_file_logger()?;
    log_to_file("SESSION_START", "P2Pong diagnostic logging initialized");

    // Load configuration
    let config = config::load_config()?;

    // Check for legacy command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        println!("Note: Command line arguments are deprecated. Please use the main menu.");
        println!("Starting menu in 2 seconds...");
        std::thread::sleep(Duration::from_secs(2));
    }

    // Disable debug logging before entering TUI to prevent stderr conflicts
    std::env::remove_var("RUST_LOG");

    // Setup terminal BEFORE entering app loop
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // AppState loop: Menu -> Game -> Menu
    let mut app_state = AppState::Menu;

    let result = loop {
        match app_state {
            AppState::Menu => {
                match run_menu(&mut terminal)? {
                    AppState::Menu => {} // Stay in menu
                    AppState::Game(mode) => {
                        app_state = AppState::Game(mode);
                    }
                    AppState::Exiting => {
                        app_state = AppState::Exiting;
                    }
                }
            }
            AppState::Game(mode) => {
                // Run game, return to menu when done
                match run_game_mode(&mut terminal, mode, &config) {
                    Ok(_) => app_state = AppState::Menu,
                    Err(e) => break Err(e),
                }
            }
            AppState::Exiting => {
                break Ok(());
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
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

        // Small sleep to avoid busy loop
        std::thread::sleep(Duration::from_millis(16));
    }
}

/// Dispatch to appropriate game mode function
fn run_game_mode<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mode: GameMode,
    config: &Config,
) -> Result<(), io::Error> {
    match mode {
        GameMode::LocalTwoPlayer => run_game_local(terminal, config),
        GameMode::NetworkHost => run_game_network_host(terminal, config),
        GameMode::NetworkClient(peer_id) => run_game_network_client(terminal, config, &peer_id),
        GameMode::SinglePlayerAI(bot_type) => run_game_vs_ai(terminal, config, bot_type)
    }
}

/// Run local 2-player game (no networking)
fn run_game_local<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
) -> Result<(), io::Error> {
    log_to_file("GAME_START", "Local 2-player mode");

    let mut last_frame = Instant::now();
    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);

    loop {
        let now = Instant::now();
        last_frame = now;

        // Check for terminal resize
        let size = terminal.size()?;
        if size.width as f32 != game_state.field_width
            || size.height as f32 != game_state.field_height
        {
            game_state.resize(size.width, size.height);
        }

        // Handle input (both paddles)
        let actions = poll_input_local_2p(config)?;

        for action in &actions {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::Rematch => {
                    if game_state.game_over {
                        game_state.reset_game();
                    }
                }
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.right_paddle, game_state.field_height);
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.right_paddle, game_state.field_height);
                }
            }
        }

        // Update physics
        let _events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);

        // Create overlay message if game is over
        let overlay = if game_state.game_over {
            let winner_text = match game_state.winner.unwrap() {
                game::Player::Left => "LEFT WINS",
                game::Player::Right => "RIGHT WINS",
            };
            Some(ui::OverlayMessage::info(vec![
                winner_text.to_string(),
                "".to_string(),
                "R to Rematch  |  Q to Quit".to_string(),
            ]))
        } else {
            None
        };

        terminal.draw(|f| ui::render(f, &game_state, None, overlay.as_ref(), None))?;

        // Frame rate limiting
        let elapsed = now.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}

/// Run single-player game against AI
fn run_game_vs_ai<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    bot_type: ai::BotType,
) -> Result<(), io::Error> {
    log_to_file("GAME_START", &format!("Single player vs AI mode: {:?}", bot_type));

    let mut last_frame = Instant::now();
    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);

    // Create bot instance using factory
    let mut bot = ai::create_bot(bot_type);

    loop {
        let now = Instant::now();
        last_frame = now;

        // Check for terminal resize
        let size = terminal.size()?;
        if size.width as f32 != game_state.field_width
            || size.height as f32 != game_state.field_height
        {
            game_state.resize(size.width, size.height);
        }

        // Handle player input (left paddle only)
        let actions = poll_input_player_left(config)?;

        for action in &actions {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::Rematch => {
                    if game_state.game_over {
                        game_state.reset_game();
                        bot.reset();
                    }
                }
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.left_paddle, game_state.field_height);
                }
                _ => {} // Ignore right paddle inputs
            }
        }

        // Bot input (right paddle)
        if let Some(bot_action) = bot.get_action(&game_state, FIXED_TIMESTEP) {
            match bot_action {
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.right_paddle, game_state.field_height);
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.right_paddle, game_state.field_height);
                }
                _ => {} // Bot should only move right paddle
            }
        }

        // Update physics
        let events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);

        // Reset bot state on new round (but keep rendering game over state)
        if events.goal_scored && !game_state.game_over {
            bot.reset();
        }

        // Create overlay message if game is over
        let overlay = if game_state.game_over {
            let winner_text = match game_state.winner.unwrap() {
                game::Player::Left => "YOU WIN!",
                game::Player::Right => "BOT WINS",
            };
            Some(ui::OverlayMessage::info(vec![
                winner_text.to_string(),
                "".to_string(),
                "R to Rematch  |  Q to Quit".to_string(),
            ]))
        } else {
            None
        };

        terminal.draw(|f| ui::render(f, &game_state, None, overlay.as_ref(), Some(game::Player::Left)))?;

        // Frame rate limiting
        let elapsed = now.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}

/// Run networked game as host
fn run_game_network_host<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
) -> Result<(), io::Error> {
    log_to_file("GAME_START", "Network host mode");

    // Initialize network
    let network_client = network::start_network(
        ConnectionMode::Listen,
        config.network.signaling_server.clone(),
    )?;

    // Wait for connection with TUI display
    match wait_for_connection_tui(
        terminal,
        &network_client,
        &PlayerRole::Host,
        None,
        config.network.connection_timeout_secs,
    )? {
        Some(_peer_id) => {
            // Connection established, start game
            run_game_networked(terminal, network_client, PlayerRole::Host, config)
        }
        None => {
            // User cancelled, return to menu
            Ok(())
        }
    }
}

/// Run networked game as client
fn run_game_network_client<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    peer_id: &str,
) -> Result<(), io::Error> {
    log_to_file("GAME_START", &format!("Network client mode, peer: {}", peer_id));

    // Initialize network
    let network_client = network::start_network(
        ConnectionMode::Connect {
            multiaddr: peer_id.to_string(),
        },
        config.network.signaling_server.clone(),
    )?;

    // Wait for connection with TUI display
    match wait_for_connection_tui(
        terminal,
        &network_client,
        &PlayerRole::Client,
        Some(peer_id.to_string()),
        config.network.connection_timeout_secs,
    )? {
        Some(_peer_id) => {
            // Connection established, start game
            run_game_networked(terminal, network_client, PlayerRole::Client, config)
        }
        None => {
            // User cancelled, return to menu
            Ok(())
        }
    }
}

/// Player role determines who controls ball physics
#[derive(Debug)]
enum PlayerRole {
    Host,   // Controls ball physics (left paddle)
    Client, // Receives ball state (right paddle)
}

/// Run networked game (common code for host and client)
fn run_game_networked<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    network_client: network::NetworkClient,
    player_role: PlayerRole,
    config: &Config,
) -> Result<(), io::Error> {
    let mut last_frame = Instant::now();
    let game_start = Instant::now();

    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);
    let mut frame_count: u64 = 0;

    // RTT measurement
    let mut last_ping_time = Instant::now();
    let mut ping_timestamp: Option<u64> = None;

    // Connection keepalive via heartbeat
    let mut last_heartbeat_time = Instant::now();
    let mut heartbeat_sequence: u32 = 0;

    // Rematch coordination state
    let mut local_wants_rematch = false;
    let mut peer_wants_rematch = false;

    loop {
        let now = Instant::now();
        last_frame = now;

        // Check for terminal resize
        let size = terminal.size()?;
        if size.width as f32 != game_state.field_width
            || size.height as f32 != game_state.field_height
        {
            game_state.resize(size.width, size.height);
        }

        // Handle local input (mode-aware based on role)
        let local_actions = match player_role {
            PlayerRole::Host => poll_input_player_left(config)?,
            PlayerRole::Client => poll_input_player_right(config)?,
        };

        // Handle remote input and network events
        let mut remote_actions = Vec::new();

        // Send periodic ping for RTT measurement
        if last_ping_time.elapsed() > Duration::from_millis(1000) {
            let timestamp = game_start.elapsed().as_millis() as u64;
            ping_timestamp = Some(timestamp);
            let _ = network_client.send_message(NetworkMessage::Ping { timestamp_ms: timestamp });
            last_ping_time = Instant::now();
        }

        // Send periodic heartbeat
        if last_heartbeat_time.elapsed() > Duration::from_millis(2000) {
            let _ = network_client.send_message(NetworkMessage::Heartbeat {
                sequence: heartbeat_sequence,
            });
            log_to_file(
                "HEARTBEAT_SEND",
                &format!("Sending keepalive heartbeat #{}", heartbeat_sequence),
            );
            heartbeat_sequence = heartbeat_sequence.wrapping_add(1);
            last_heartbeat_time = Instant::now();
        }

        // Process network events
        while let Some(event) = network_client.try_recv_event() {
            match event {
                NetworkEvent::ReceivedInput(action) => remote_actions.push(action),
                NetworkEvent::ReceivedBallState(ball_state) => {
                    if matches!(player_role, PlayerRole::Client) {
                        if ball_state.sequence > LAST_RECEIVED_SEQUENCE.load(Ordering::SeqCst) {
                            LAST_RECEIVED_SEQUENCE.store(ball_state.sequence, Ordering::SeqCst);

                            let error_x = ball_state.x - game_state.ball.x;
                            let error_y = ball_state.y - game_state.ball.y;
                            let error_magnitude = (error_x * error_x + error_y * error_y).sqrt();

                            if error_magnitude > POSITION_SNAP_THRESHOLD {
                                game_state.ball.x = ball_state.x;
                                game_state.ball.y = ball_state.y;
                            } else {
                                game_state.ball.x += error_x * POSITION_CORRECTION_ALPHA;
                                game_state.ball.y += error_y * POSITION_CORRECTION_ALPHA;
                            }

                            game_state.ball.vx = ball_state.vx;
                            game_state.ball.vy = ball_state.vy;
                        }
                    }
                }
                NetworkEvent::ReceivedScore {
                    left,
                    right,
                    game_over,
                } => {
                    if matches!(player_role, PlayerRole::Client) {
                        game_state.left_score = left;
                        game_state.right_score = right;
                        game_state.game_over = game_over;

                        // Determine winner when game is over
                        if game_over {
                            if left > right {
                                game_state.winner = Some(game::Player::Left);
                            } else if right > left {
                                game_state.winner = Some(game::Player::Right);
                            }
                        }
                    }
                }
                NetworkEvent::ReceivedPing { timestamp_ms } => {
                    let _ = network_client.send_message(NetworkMessage::Pong { timestamp_ms });
                }
                NetworkEvent::ReceivedPong { timestamp_ms } => {
                    if let Some(sent_timestamp) = ping_timestamp {
                        if timestamp_ms == sent_timestamp {
                            let current_time = game_start.elapsed().as_millis() as u64;
                            let rtt = current_time.saturating_sub(timestamp_ms);
                            LAST_RTT_MS.store(rtt, Ordering::Relaxed);
                            ping_timestamp = None;
                        }
                    }
                }
                NetworkEvent::ReceivedRematchRequest => {
                    peer_wants_rematch = true;
                    // If both want rematch, send confirm and reset
                    if local_wants_rematch {
                        let _ = network_client.send_message(NetworkMessage::RematchConfirm);
                        game_state.reset_game();
                        local_wants_rematch = false;
                        peer_wants_rematch = false;
                    }
                }
                NetworkEvent::ReceivedRematchConfirm => {
                    // Peer confirmed rematch, reset game
                    game_state.reset_game();
                    local_wants_rematch = false;
                    peer_wants_rematch = false;
                }
                NetworkEvent::ReceivedQuitRequest => {
                    // Peer wants to quit, exit immediately
                    return Ok(());
                }
                NetworkEvent::Disconnected => {
                    eprintln!("❌ Peer disconnected!");
                    return Ok(());
                }
                NetworkEvent::Error(msg) => {
                    eprintln!("⚠️  Network error: {}", msg);
                }
                _ => {}
            }
        }

        // Process all actions
        for action in local_actions.iter().chain(remote_actions.iter()) {
            match action {
                InputAction::Quit => {
                    // Send quit request to peer and exit
                    let _ = network_client.send_message(NetworkMessage::QuitRequest);
                    return Ok(());
                }
                InputAction::Rematch => {
                    // Only handle rematch if game is over
                    if game_state.game_over {
                        local_wants_rematch = true;
                        // Send rematch request to peer
                        let _ = network_client.send_message(NetworkMessage::RematchRequest);
                        // If peer already wants rematch, send confirm and reset
                        if peer_wants_rematch {
                            let _ = network_client.send_message(NetworkMessage::RematchConfirm);
                            game_state.reset_game();
                            local_wants_rematch = false;
                            peer_wants_rematch = false;
                        }
                    }
                }
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.left_paddle, game_state.field_height);
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(&mut game_state.right_paddle, game_state.field_height);
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(&mut game_state.right_paddle, game_state.field_height);
                }
            }
        }

        // Send local inputs to opponent
        for action in &local_actions {
            let should_send = match (&player_role, action) {
                (PlayerRole::Host, InputAction::LeftPaddleUp) => true,
                (PlayerRole::Host, InputAction::LeftPaddleDown) => true,
                (PlayerRole::Client, InputAction::RightPaddleUp) => true,
                (PlayerRole::Client, InputAction::RightPaddleDown) => true,
                _ => false,
            };

            if should_send && *action != InputAction::Quit {
                let count = INPUT_SEND_COUNT.fetch_add(1, Ordering::Relaxed);
                if count < 5 {
                    log_to_file("GAME_INPUT", &format!("Sending input #{}: {:?}", count, action));
                }
                let _ = network_client.send_input(*action);
            }
        }

        // Update physics based on role
        match player_role {
            PlayerRole::Host => {
                let prev_left_score = game_state.left_score;
                let prev_right_score = game_state.right_score;

                let physics_events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);
                frame_count += 1;

                // Send score sync if changed
                if game_state.left_score != prev_left_score
                    || game_state.right_score != prev_right_score
                {
                    let msg = NetworkMessage::ScoreSync {
                        left: game_state.left_score,
                        right: game_state.right_score,
                        game_over: game_state.game_over,
                    };
                    let _ = network_client.send_message(msg);
                }

                // Event-based ball sync + periodic backup
                let should_sync = physics_events.any() || frame_count % BACKUP_SYNC_INTERVAL == 0;

                if should_sync {
                    let sequence = BALL_SEQUENCE.fetch_add(1, Ordering::SeqCst);
                    let ball_state = BallState {
                        x: game_state.ball.x,
                        y: game_state.ball.y,
                        vx: game_state.ball.vx,
                        vy: game_state.ball.vy,
                        sequence,
                        timestamp_ms: now.elapsed().as_millis() as u64,
                    };

                    if sequence % 30 == 0 {
                        log_to_file(
                            "GAME_SEND_MARKER",
                            &format!("Sending seq={} at frame={}", sequence, frame_count),
                        );
                    }

                    let msg = NetworkMessage::BallSync(ball_state);
                    if let Err(e) = network_client.send_message(msg) {
                        log_to_file("GAME_SEND_ERROR", &format!("Failed to send seq={}: {}", sequence, e));
                    }
                }
            }
            PlayerRole::Client => {
                // Dead reckoning
                game_state.ball.x += game_state.ball.vx * FIXED_TIMESTEP;
                game_state.ball.y += game_state.ball.vy * FIXED_TIMESTEP;
            }
        }

        // Render with overlay for game over and rematch status
        let rtt_ms = Some(LAST_RTT_MS.load(Ordering::Relaxed));
        let overlay = if game_state.game_over {
            // Determine winner text based on role and winner
            let winner_text = match (game_state.winner.unwrap(), &player_role) {
                (game::Player::Left, PlayerRole::Host) => "YOU WIN!",
                (game::Player::Left, PlayerRole::Client) => "YOU LOSE",
                (game::Player::Right, PlayerRole::Host) => "YOU LOSE",
                (game::Player::Right, PlayerRole::Client) => "YOU WIN!",
            };

            // Build status message based on rematch state
            let status_text = if local_wants_rematch && peer_wants_rematch {
                "Both ready! Restarting..."
            } else if local_wants_rematch {
                "Waiting for opponent..."
            } else if peer_wants_rematch {
                "Opponent ready! Press R to Rematch"
            } else {
                "R to Rematch  |  Q to Quit"
            };

            Some(ui::OverlayMessage::info(vec![
                winner_text.to_string(),
                "".to_string(),
                status_text.to_string(),
            ]))
        } else {
            None
        };

        // Determine your player based on role
        let your_player = match player_role {
            PlayerRole::Host => Some(game::Player::Left),
            PlayerRole::Client => Some(game::Player::Right),
        };

        terminal.draw(|f| ui::render(f, &game_state, rtt_ms, overlay.as_ref(), your_player))?;

        // Frame rate limiting
        let elapsed = now.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}

/// Initialize file-based logging
fn init_file_logger() -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/p2pong-debug.log")?;

    writeln!(file, "=== P2Pong Debug Log ===")?;
    writeln!(file, "Session started: {:?}", std::time::SystemTime::now())?;
    writeln!(file, "To monitor: tail -f /tmp/p2pong-debug.log")?;
    writeln!(file, "========================================\n")?;

    Ok(())
}

/// Thread-safe logging to file
fn log_to_file(category: &str, message: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/p2pong-debug.log")
    {
        let _ = writeln!(file, "[{:013}] [{}] {}", timestamp, category, message);
    }
}

/// Wait for peer connection with TUI display
/// Returns Some(peer_id) if connected, None if user cancelled
fn wait_for_connection_tui<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: &network::NetworkClient,
    player_role: &PlayerRole,
    target_peer_id: Option<String>,  // For client mode: the peer we're connecting to
    timeout_secs: u64,
) -> Result<Option<String>, io::Error> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    let mut peer_connected = false;
    let mut data_channel_ready = false;
    let mut peer_id = String::from("waiting...");
    let mut copy_feedback = String::new();
    let connection_start = Instant::now();

    log_to_file("WAIT_START", &format!("Waiting for connection as {:?}", player_role));

    loop {
        // Check for timeout (configurable via config.network.connection_timeout_secs)
        if connection_start.elapsed() > Duration::from_secs(timeout_secs) {
            log_to_file("CONN_TIMEOUT", "Connection timeout");
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Connection timeout - peer may not exist or be offline",
            ));
        }

        // Check for user input (Q to cancel, C to copy)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            log_to_file("WAIT_CANCELLED", "User cancelled connection wait");
                            return Ok(None); // User cancelled
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            // Try to copy peer ID to clipboard
                            if peer_id != "waiting..." {
                                match arboard::Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        match clipboard.set_text(&peer_id) {
                                            Ok(_) => {
                                                copy_feedback = "Copied to clipboard!".to_string();
                                                log_to_file("PEER_ID_COPIED", &format!("Copied peer ID: {}", peer_id));
                                            }
                                            Err(e) => {
                                                copy_feedback = format!("Copy failed: {}", e);
                                                log_to_file("COPY_FAILED", &format!("Failed to copy: {}", e));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        copy_feedback = format!("Clipboard unavailable: {}", e);
                                        log_to_file("CLIPBOARD_ERROR", &format!("Clipboard error: {}", e));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Drain network events
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::LocalPeerIdReady { peer_id: id } => {
                    peer_id = id;
                    log_to_file("LOCAL_PEER_ID", &format!("Local peer ID ready: {}", peer_id));
                }
                NetworkEvent::Connected { peer_id: id } => {
                    peer_connected = true;
                    log_to_file("PEER_CONN", &format!("Peer connected: {}", id));
                }
                NetworkEvent::DataChannelOpened => {
                    data_channel_ready = true;
                    log_to_file("DC_OPENED", "Data channel opened");
                }
                NetworkEvent::Error(msg) => {
                    log_to_file("NET_ERROR", &format!("Network error: {}", msg));

                    // Show error overlay and wait for user acknowledgment
                    loop {
                        let error_overlay = ui::OverlayMessage::error(vec![
                            "Connection Failed".to_string(),
                            "".to_string(),
                            msg.clone(),
                            "".to_string(),
                            "Press Q to return to menu".to_string(),
                        ]);

                        terminal.draw(|f| {
                            match player_role {
                                PlayerRole::Host => {
                                    menu::render_waiting_for_connection(f, &peer_id, &copy_feedback, Some(&error_overlay));
                                }
                                PlayerRole::Client => {
                                    let target = target_peer_id.as_deref().unwrap_or("unknown");
                                    menu::render_connecting_to_peer(f, target, Some(&error_overlay));
                                }
                            }
                        })?;

                        // Wait for user to press Q
                        if event::poll(Duration::from_millis(100))? {
                            if let Event::Key(key) = event::read()? {
                                if key.kind == KeyEventKind::Press {
                                    if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc) {
                                        return Ok(None); // Return to menu
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Check if connection is ready
        if peer_connected && data_channel_ready {
            log_to_file("READY", "Connection ready - starting game");
            return Ok(Some(peer_id));
        }

        // Render waiting screen (different for host vs client)
        terminal.draw(|f| {
            match player_role {
                PlayerRole::Host => {
                    // Host: show "Share this Peer ID:" screen
                    menu::render_waiting_for_connection(f, &peer_id, &copy_feedback, None);
                }
                PlayerRole::Client => {
                    // Client: show "Connecting to peer..." screen
                    let target = target_peer_id.as_deref().unwrap_or("unknown");
                    menu::render_connecting_to_peer(f, target, None);
                }
            }
        })?;
    }
}
