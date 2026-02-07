use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;

use crate::config::Config;
use crate::debug;
use crate::game::{self, poll_input_player_left, poll_input_player_right, GameState, InputAction};
use crate::menu;
use crate::network::client::NetworkEvent;
use crate::network::{self, BallState, ConnectionMode, NetworkMessage};
use crate::ui;
use crate::BACKUP_SYNC_INTERVAL;
use crate::FIXED_TIMESTEP;
use crate::POSITION_CORRECTION_ALPHA;
use crate::POSITION_SNAP_THRESHOLD;

use super::common::limit_frame_rate;

/// Player role determines who controls ball physics
#[derive(Debug)]
enum PlayerRole {
    Host,   // Controls ball physics (left paddle)
    Client, // Receives ball state (right paddle)
}

/// Network synchronization state for a networked game session
/// Replaces global AtomicU64 statics with proper local state
struct NetworkSyncState {
    /// Sequence number for ball state messages sent by host
    ball_sequence: u64,

    /// Last received ball sequence number (client-side tracking)
    last_received_sequence: u64,

    /// Last measured round-trip time in milliseconds
    last_rtt_ms: u64,

    /// Debug counter for input sends (used for logging first N inputs)
    input_send_count: u64,
}

impl Default for NetworkSyncState {
    fn default() -> Self {
        Self {
            ball_sequence: 0,
            last_received_sequence: 0,
            last_rtt_ms: 0,
            input_send_count: 0,
        }
    }
}

/// Run networked game as host
pub fn run_game_network_host<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
) -> Result<(), io::Error> {
    debug::log("GAME_START", "Network host mode");

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
pub fn run_game_network_client<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    peer_id: &str,
) -> Result<(), io::Error> {
    debug::log(
        "GAME_START",
        &format!("Network client mode, peer: {}", peer_id),
    );

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

/// Run networked game (common code for host and client)
fn run_game_networked<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    network_client: network::NetworkClient,
    player_role: PlayerRole,
    config: &Config,
) -> Result<(), io::Error> {
    let game_start = Instant::now();

    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height);
    let mut frame_count: u64 = 0;

    // Network synchronization state (replaces global atomics)
    let mut sync_state = NetworkSyncState::default();

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
            let _ = network_client.send_message(NetworkMessage::Ping {
                timestamp_ms: timestamp,
            });
            last_ping_time = Instant::now();
        }

        // Send periodic heartbeat
        if last_heartbeat_time.elapsed() > Duration::from_millis(2000) {
            let _ = network_client.send_message(NetworkMessage::Heartbeat {
                sequence: heartbeat_sequence,
            });
            debug::log(
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
                        if ball_state.sequence > sync_state.last_received_sequence {
                            sync_state.last_received_sequence = ball_state.sequence;

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
                            sync_state.last_rtt_ms = rtt;
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
                if sync_state.input_send_count < 5 {
                    debug::log(
                        "GAME_INPUT",
                        &format!(
                            "Sending input #{}: {:?}",
                            sync_state.input_send_count, action
                        ),
                    );
                }
                sync_state.input_send_count += 1;
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
                    let sequence = sync_state.ball_sequence;
                    sync_state.ball_sequence += 1;
                    let ball_state = BallState {
                        x: game_state.ball.x,
                        y: game_state.ball.y,
                        vx: game_state.ball.vx,
                        vy: game_state.ball.vy,
                        sequence,
                        timestamp_ms: now.elapsed().as_millis() as u64,
                    };

                    if sequence % 30 == 0 {
                        debug::log(
                            "GAME_SEND_MARKER",
                            &format!("Sending seq={} at frame={}", sequence, frame_count),
                        );
                    }

                    let msg = NetworkMessage::BallSync(ball_state);
                    if let Err(e) = network_client.send_message(msg) {
                        debug::log(
                            "GAME_SEND_ERROR",
                            &format!("Failed to send seq={}: {}", sequence, e),
                        );
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
        let rtt_ms = Some(sync_state.last_rtt_ms);
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
        limit_frame_rate(now);
    }
}

/// Wait for peer connection with TUI display
/// Returns Some(peer_id) if connected, None if user cancelled
fn wait_for_connection_tui<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: &network::NetworkClient,
    player_role: &PlayerRole,
    target_peer_id: Option<String>, // For client mode: the peer we're connecting to
    timeout_secs: u64,
) -> Result<Option<String>, io::Error> {
    let mut peer_connected = false;
    let mut data_channel_ready = false;
    let mut peer_id = String::from("waiting...");
    let mut copy_feedback = String::new();
    let connection_start = Instant::now();

    debug::log(
        "WAIT_START",
        &format!("Waiting for connection as {:?}", player_role),
    );

    loop {
        // Check for timeout (configurable via config.network.connection_timeout_secs)
        if connection_start.elapsed() > Duration::from_secs(timeout_secs) {
            debug::log("CONN_TIMEOUT", "Connection timeout");
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
                            debug::log("WAIT_CANCELLED", "User cancelled connection wait");
                            return Ok(None); // User cancelled
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            // Try to copy peer ID to clipboard
                            if peer_id != "waiting..." {
                                match arboard::Clipboard::new() {
                                    Ok(mut clipboard) => match clipboard.set_text(&peer_id) {
                                        Ok(_) => {
                                            copy_feedback = "Copied to clipboard!".to_string();
                                            debug::log(
                                                "PEER_ID_COPIED",
                                                &format!("Copied peer ID: {}", peer_id),
                                            );
                                        }
                                        Err(e) => {
                                            copy_feedback = format!("Copy failed: {}", e);
                                            debug::log(
                                                "COPY_FAILED",
                                                &format!("Failed to copy: {}", e),
                                            );
                                        }
                                    },
                                    Err(e) => {
                                        copy_feedback = format!("Clipboard unavailable: {}", e);
                                        debug::log(
                                            "CLIPBOARD_ERROR",
                                            &format!("Clipboard error: {}", e),
                                        );
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
                    debug::log(
                        "LOCAL_PEER_ID",
                        &format!("Local peer ID ready: {}", peer_id),
                    );
                }
                NetworkEvent::Connected { peer_id: id } => {
                    peer_connected = true;
                    debug::log("PEER_CONN", &format!("Peer connected: {}", id));
                }
                NetworkEvent::DataChannelOpened => {
                    data_channel_ready = true;
                    debug::log("DC_OPENED", "Data channel opened");
                }
                NetworkEvent::Error(msg) => {
                    debug::log("NET_ERROR", &format!("Network error: {}", msg));

                    // Show error overlay and wait for user acknowledgment
                    loop {
                        let error_overlay = ui::OverlayMessage::error(vec![
                            "Connection Failed".to_string(),
                            "".to_string(),
                            msg.clone(),
                            "".to_string(),
                            "Press Q to return to menu".to_string(),
                        ]);

                        terminal.draw(|f| match player_role {
                            PlayerRole::Host => {
                                menu::render_waiting_for_connection(
                                    f,
                                    &peer_id,
                                    &copy_feedback,
                                    Some(&error_overlay),
                                );
                            }
                            PlayerRole::Client => {
                                let target = target_peer_id.as_deref().unwrap_or("unknown");
                                menu::render_connecting_to_peer(f, target, Some(&error_overlay));
                            }
                        })?;

                        // Wait for user to press Q
                        if event::poll(Duration::from_millis(100))? {
                            if let Event::Key(key) = event::read()? {
                                if key.kind == KeyEventKind::Press {
                                    if matches!(
                                        key.code,
                                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                                    ) {
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
            debug::log("READY", "Connection ready - starting game");
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
