mod game;
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

use game::{poll_input, GameState, InputAction};
use network::client::NetworkEvent;
use network::{BallState, ConnectionMode, NetworkMessage};

const TARGET_FPS: u64 = 60;
const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);
const FIXED_TIMESTEP: f32 = 1.0 / 60.0; // Fixed timestep for deterministic physics

// Network sync tuning parameters
const BACKUP_SYNC_INTERVAL: u64 = 10; // Frames between syncs (every 10 frames = ~166ms at 60 FPS, 6 syncs/sec)

// Dead reckoning configuration for client-side prediction
// The client simulates ball movement between host updates for physics-correct straight-line motion
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
    // This runs BEFORE TUI starts and persists throughout the session
    init_file_logger()?;
    log_to_file("SESSION_START", "P2Pong diagnostic logging initialized");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let network_mode = parse_args(&args)?;

    // Initialize network and wait for connection BEFORE starting TUI
    let (network_client, player_role) = if let Some(ref mode) = network_mode {
        let client = network::start_network(mode.clone())?;
        let role = match mode {
            ConnectionMode::Listen { .. } => PlayerRole::Host,
            ConnectionMode::Connect { .. } => PlayerRole::Client,
        };

        // Wait for connection with simple spinner (no TUI yet)
        wait_for_connection(&client, &role)?;

        (Some(client), role)
    } else {
        (None, PlayerRole::Host) // Local mode
    };

    // Disable debug logging before entering TUI to prevent stderr conflicts
    // (RUST_LOG debug output will corrupt the terminal interface)
    std::env::remove_var("RUST_LOG");

    // Give any pending log messages time to flush
    std::thread::sleep(Duration::from_millis(100));

    // Setup terminal (only after connection established)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run game
    let result = run_game(&mut terminal, network_client, player_role);

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

/// Parse command line arguments for network mode
fn parse_args(args: &[String]) -> Result<Option<ConnectionMode>, io::Error> {
    if args.len() == 1 {
        // No arguments - local mode (no networking)
        return Ok(None);
    }

    match args[1].as_str() {
        "--listen" | "-l" | "--host" => Ok(Some(ConnectionMode::Listen)),
        "--connect" | "-c" => {
            if args.len() < 3 {
                eprintln!("Error: --connect requires a peer ID");
                eprintln!("Usage: {} --connect <peer-id>", args[0]);
                std::process::exit(1);
            }

            let peer_id = args[2].clone();
            Ok(Some(ConnectionMode::Connect { multiaddr: peer_id }))
        }
        "--help" | "-h" => {
            print_usage(&args[0]);
            std::process::exit(0);
        }
        _ => {
            eprintln!("Unknown argument: {}", args[1]);
            print_usage(&args[0]);
            std::process::exit(1);
        }
    }
}

fn print_usage(program: &str) {
    println!("P2Pong - Peer-to-Peer Terminal Pong (WebRTC Edition)");
    println!();
    println!("Usage:");
    println!(
        "  {}                              # Local mode (no networking)",
        program
    );
    println!(
        "  {} --listen                     # Host a game (wait for connections)",
        program
    );
    println!(
        "  {} --connect <peer-id>          # Connect to a hosted game",
        program
    );
    println!();
    println!("Examples:");
    println!("  # Host a game:");
    println!("  {}  --listen", program);
    println!();
    println!("  # Connect to host:");
    println!("  {}  --connect peer-a1b2c3d4", program);
    println!();
    println!("Note: WebRTC uses ICE/STUN for automatic NAT traversal.");
    println!("      The host will display their peer ID when ready.");
}

/// Player role determines who controls ball physics
#[derive(Debug)]
enum PlayerRole {
    Host,   // Controls ball physics (left paddle)
    Client, // Receives ball state (right paddle)
}

/// Initialize file-based logging that persists even after TUI starts
fn init_file_logger() -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    // Create/truncate log file
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

/// Thread-safe logging to file with timestamp
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

/// Wait for peer connection AND data channel to be ready before starting game
/// Shows a simple braille spinner animation on stderr
fn wait_for_connection(
    client: &network::NetworkClient,
    player_role: &PlayerRole,
) -> Result<(), io::Error> {
    use std::io::Write;

    // Braille spinner frames
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut frame = 0;

    let mut peer_connected = false;
    let mut data_channel_ready = false;

    log_to_file(
        "WAIT_START",
        &format!("Waiting for connection as {:?}", player_role),
    );

    loop {
        // Drain network events
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::Connected { .. } => {
                    peer_connected = true;
                    log_to_file("PEER_CONN", "Peer connection state changed to Connected");
                    eprint!("\r\x1b[K");
                    eprintln!("🔗 Peer connected, waiting for data channel...");
                }
                NetworkEvent::DataChannelOpened => {
                    data_channel_ready = true;
                    log_to_file("DC_OPENED", "DataChannel on_open callback fired");
                }
                NetworkEvent::Error(msg) => {
                    log_to_file("NET_ERROR", &format!("Network error: {}", msg));
                    eprint!("\r\x1b[K");
                    eprintln!("⚠️  Network error: {}", msg);
                }
                _ => {}
            }
        }

        // Check if both peer is connected AND data channel is ready
        if peer_connected && data_channel_ready {
            log_to_file("READY", "Both peer connected AND data channel ready");

            // Clear the spinner line and print success
            eprint!("\r\x1b[K");
            eprintln!("✅ Connected and ready! Stabilizing connection...");

            // Give WebRTC state machine time to fully stabilize before messages flow
            // This prevents the race condition where early messages are dropped/queued
            std::thread::sleep(Duration::from_millis(500));

            log_to_file(
                "START_GAME",
                "Exiting wait_for_connection, starting game loop",
            );
            eprintln!("✅ Connection stable! Starting game...\n");
            return Ok(());
        }

        // Update message based on state
        let message = match (peer_connected, player_role) {
            (false, PlayerRole::Host) => "Waiting for opponent to connect...",
            (false, PlayerRole::Client) => "Connecting to host...",
            (true, _) => "Waiting for data channel to open...",
        };

        // Print spinner
        eprint!("\r{} {} ", spinner[frame % spinner.len()], message);
        std::io::stderr().flush()?;

        frame += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn run_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    network_client: Option<network::NetworkClient>,
    player_role: PlayerRole,
) -> Result<(), io::Error> {
    log_to_file("GAME_START", "Game loop started");

    let mut last_frame = Instant::now();
    let game_start = Instant::now();

    // Initialize game state with terminal dimensions
    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);
    let mut frame_count: u64 = 0;

    // RTT measurement
    let mut last_ping_time = Instant::now();
    let mut ping_timestamp: Option<u64> = None;

    loop {
        let now = Instant::now();
        let _dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // Check for terminal resize
        let size = terminal.size()?;
        if size.width as f32 != game_state.field_width
            || size.height as f32 != game_state.field_height
        {
            game_state.resize(size.width, size.height);
        }

        // Handle local input
        let all_local_actions = poll_input(Duration::from_millis(1))?;

        // Filter local actions based on player role (in network mode)
        let local_actions: Vec<InputAction> = if network_client.is_some() {
            all_local_actions
                .into_iter()
                .filter(|action| {
                    match (&player_role, action) {
                        // Host can only control left paddle
                        (PlayerRole::Host, InputAction::LeftPaddleUp) => true,
                        (PlayerRole::Host, InputAction::LeftPaddleDown) => true,
                        // Client can only control right paddle
                        (PlayerRole::Client, InputAction::RightPaddleUp) => true,
                        (PlayerRole::Client, InputAction::RightPaddleDown) => true,
                        // Quit is always allowed
                        (_, InputAction::Quit) => true,
                        // Block opposite paddle controls
                        _ => false,
                    }
                })
                .collect()
        } else {
            // Local mode: allow all inputs
            all_local_actions
        };

        // Handle remote input and ball sync (if networked)
        let mut remote_actions = Vec::new();
        if let Some(ref client) = network_client {
            // Send periodic ping for RTT measurement (every 1000ms)
            if last_ping_time.elapsed() > Duration::from_millis(1000) {
                let timestamp = game_start.elapsed().as_millis() as u64;
                ping_timestamp = Some(timestamp);
                let _ = client.send_message(NetworkMessage::Ping {
                    timestamp_ms: timestamp,
                });
                last_ping_time = Instant::now();
            }

            // Process all network events
            while let Some(event) = client.try_recv_event() {
                match event {
                    NetworkEvent::ReceivedInput(action) => remote_actions.push(action),
                    NetworkEvent::ReceivedBallState(ball_state) => {
                        // Apply authoritative ball state from host (client only)
                        if matches!(player_role, PlayerRole::Client) {
                            // Only apply if sequence is newer (prevents old/duplicate updates)
                            if ball_state.sequence > LAST_RECEIVED_SEQUENCE.load(Ordering::SeqCst) {
                                LAST_RECEIVED_SEQUENCE.store(ball_state.sequence, Ordering::SeqCst);

                                // Dead reckoning correction: compare predicted position with host's authoritative position
                                let error_x = ball_state.x - game_state.ball.x;
                                let error_y = ball_state.y - game_state.ball.y;
                                let error_magnitude =
                                    (error_x * error_x + error_y * error_y).sqrt();

                                if error_magnitude > POSITION_SNAP_THRESHOLD {
                                    // Large error: collision or significant event happened on host
                                    // Snap immediately (collisions are instant anyway, so this looks natural)
                                    game_state.ball.x = ball_state.x;
                                    game_state.ball.y = ball_state.y;
                                } else {
                                    // Small error: normal prediction drift from network latency
                                    // Gently correct over a few frames to avoid visible jumps
                                    game_state.ball.x += error_x * POSITION_CORRECTION_ALPHA;
                                    game_state.ball.y += error_y * POSITION_CORRECTION_ALPHA;
                                }

                                // Always update velocity instantly (no lerping!)
                                // Velocity changes in Pong are instantaneous (bounces, serves)
                                // Lerping velocity creates rounded corners which looks wrong
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
                        // Apply authoritative score from host (client only)
                        if matches!(player_role, PlayerRole::Client) {
                            game_state.left_score = left;
                            game_state.right_score = right;
                            game_state.game_over = game_over;
                        }
                    }
                    NetworkEvent::ReceivedPing { timestamp_ms } => {
                        // Respond to ping with pong
                        let _ = client.send_message(NetworkMessage::Pong { timestamp_ms });
                    }
                    NetworkEvent::ReceivedPong { timestamp_ms } => {
                        // Calculate RTT from pong response
                        if let Some(sent_timestamp) = ping_timestamp {
                            if timestamp_ms == sent_timestamp {
                                let current_time = game_start.elapsed().as_millis() as u64;
                                let rtt = current_time.saturating_sub(timestamp_ms);
                                LAST_RTT_MS.store(rtt, Ordering::Relaxed);
                                ping_timestamp = None; // Clear to avoid duplicate calculations
                            }
                        }
                    }
                    NetworkEvent::Connected { peer_id } => {
                        eprintln!("✅ Connected to peer: {}", peer_id);
                    }
                    NetworkEvent::DataChannelOpened => {
                        // Data channel ready - already handled in wait_for_connection
                        // Ignore during gameplay
                    }
                    NetworkEvent::Disconnected => {
                        eprintln!("❌ Peer disconnected!");
                    }
                    NetworkEvent::Error(msg) => {
                        eprintln!("⚠️  Network error: {}", msg);
                    }
                }
            }
        }

        // Process all actions (filtered local + remote)
        for action in local_actions.iter().chain(remote_actions.iter()) {
            match action {
                InputAction::Quit => return Ok(()),
                InputAction::LeftPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::LeftPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::RightPaddleUp => {
                    game::physics::move_paddle_up(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                    );
                }
                InputAction::RightPaddleDown => {
                    game::physics::move_paddle_down(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                    );
                }
            }
        }

        // Send local inputs to opponent (filtered by player role)
        if let Some(ref client) = network_client {
            for action in &local_actions {
                let should_send = match (&player_role, action) {
                    (PlayerRole::Host, InputAction::LeftPaddleUp) => true,
                    (PlayerRole::Host, InputAction::LeftPaddleDown) => true,
                    (PlayerRole::Client, InputAction::RightPaddleUp) => true,
                    (PlayerRole::Client, InputAction::RightPaddleDown) => true,
                    _ => false,
                };

                if should_send && *action != InputAction::Quit {
                    // Log first few inputs for diagnostics
                    let count = INPUT_SEND_COUNT.fetch_add(1, Ordering::Relaxed);
                    if count < 5 {
                        log_to_file(
                            "GAME_INPUT",
                            &format!("Sending input #{}: {:?}", count, action),
                        );
                    }

                    let _ = client.send_input(*action);
                }
            }
        }

        // Update game physics (host-authoritative ball)
        if network_client.is_some() {
            match player_role {
                PlayerRole::Host => {
                    // Track score before update
                    let prev_left_score = game_state.left_score;
                    let prev_right_score = game_state.right_score;

                    // Host: Run full physics with fixed timestep (deterministic)
                    let physics_events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);

                    frame_count += 1;

                    // Send score sync immediately if score changed
                    if game_state.left_score != prev_left_score
                        || game_state.right_score != prev_right_score
                    {
                        if let Some(ref client) = network_client {
                            let msg = NetworkMessage::ScoreSync {
                                left: game_state.left_score,
                                right: game_state.right_score,
                                game_over: game_state.game_over,
                            };
                            let _ = client.send_message(msg);
                        }
                    }

                    // Event-based ball sync + periodic backup
                    let should_sync =
                        physics_events.any() || frame_count % BACKUP_SYNC_INTERVAL == 0;

                    if should_sync {
                        if let Some(ref client) = network_client {
                            // Log first few syncs for diagnostics
                            if frame_count <= 5 {
                                log_to_file(
                                    "GAME_SYNC",
                                    &format!("Sending ball sync, frame={}", frame_count),
                                );
                            }

                            let ball_state = BallState {
                                x: game_state.ball.x,
                                y: game_state.ball.y,
                                vx: game_state.ball.vx,
                                vy: game_state.ball.vy,
                                sequence: BALL_SEQUENCE.fetch_add(1, Ordering::SeqCst),
                                timestamp_ms: now.elapsed().as_millis() as u64,
                            };
                            let msg = NetworkMessage::BallSync(ball_state);
                            let _ = client.send_message(msg);
                        }
                    }
                }
                PlayerRole::Client => {
                    // Client: Run simplified ball physics (dead reckoning)
                    // Between host updates, client simulates straight-line ball movement
                    // This creates physics-correct linear motion instead of curved lerp paths
                    // Host remains authoritative - corrections applied when updates arrive

                    // Simple dead reckoning: move ball according to current velocity
                    game_state.ball.x += game_state.ball.vx * FIXED_TIMESTEP;
                    game_state.ball.y += game_state.ball.vy * FIXED_TIMESTEP;

                    // Note: No collision detection on client - host handles that authoritatively
                    // When collisions happen, host will send corrected position and client will snap
                }
            }
        } else {
            // Local mode: run normal physics with fixed timestep
            let _events = game::update_with_events(&mut game_state, FIXED_TIMESTEP);
        }

        // Render (pass RTT if networked)
        let rtt_ms = if network_client.is_some() {
            Some(LAST_RTT_MS.load(Ordering::Relaxed))
        } else {
            None
        };
        terminal.draw(|f| ui::render(f, &game_state, rtt_ms))?;

        // Frame rate limiting
        let elapsed = now.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}
