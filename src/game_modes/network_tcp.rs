// TCP client-server networked game mode
// Simplified from WebRTC P2P - server is authoritative

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;

use crate::config::{Config, PhysicsConfig};
use crate::debug;
use crate::game::{self, GameState, InputAction};
use crate::network::tcp_client::{NetworkCommand, NetworkEvent, RoomInfo, Side, TcpNetworkClient};
use crate::ui;
use crate::FIXED_TIMESTEP;

use super::common::limit_frame_rate;

/// Connection state for room management
#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Connecting,
    Connected,
    WaitingForOpponent,
    ReadyToStart,
    Playing,
    GameOver,
    Disconnected,
}

/// Run networked game by creating a room (host)
pub fn run_game_network_host<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    server_addr: &str,
) -> Result<(), io::Error> {
    debug::log("GAME_START", "Network TCP host mode");

    // Connect to server
    let client = TcpNetworkClient::connect(server_addr)?;

    // Wait for connection
    if !wait_for_connection(terminal, &client, 5)? {
        return Ok(()); // Timeout or cancelled
    }

    // Create room
    let room_name = "Game Room".to_string(); // TODO: Get from menu
    client.send_command(NetworkCommand::CreateRoom { room_name })?;

    // Wait for room creation and opponent
    let (room_id, your_side) = wait_for_room_setup(terminal, &client, true)?;

    debug::log(
        "ROOM_CREATED",
        &format!("Room: {}, Side: {:?}", room_id, your_side),
    );

    // Signal ready
    client.send_command(NetworkCommand::Ready)?;

    // Run game
    run_game_tcp(terminal, client, your_side, config)
}

/// Run networked game by joining a room (client)
pub fn run_game_network_client<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    server_addr: &str,
    room_id: &str,
) -> Result<(), io::Error> {
    debug::log(
        "GAME_START",
        &format!("Network TCP client mode, joining room: {}", room_id),
    );

    // Connect to server
    let client = TcpNetworkClient::connect(server_addr)?;

    // Wait for connection
    if !wait_for_connection(terminal, &client, 5)? {
        return Ok(()); // Timeout or cancelled
    }

    // Join room
    client.send_command(NetworkCommand::JoinRoom {
        room_id: room_id.to_string(),
    })?;

    // Wait for room join confirmation
    let (room_id, your_side) = wait_for_room_setup(terminal, &client, false)?;

    debug::log(
        "ROOM_JOINED",
        &format!("Room: {}, Side: {:?}", room_id, your_side),
    );

    // Signal ready
    client.send_command(NetworkCommand::Ready)?;

    // Run game
    run_game_tcp(terminal, client, your_side, config)
}

/// Wait for TCP connection to establish
fn wait_for_connection<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: &TcpNetworkClient,
    timeout_secs: u64,
) -> Result<bool, io::Error> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        // Check for ESC key to cancel
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Ok(false);
                }
            }
        }

        // Check for connection events
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::Connected => return Ok(true),
                NetworkEvent::Error(e) => {
                    eprintln!("Connection error: {}", e);
                    return Ok(false);
                }
                NetworkEvent::Disconnected => return Ok(false),
                _ => {}
            }
        }

        // Check timeout
        if start.elapsed() > timeout {
            eprintln!("Connection timeout");
            return Ok(false);
        }

        // Render waiting screen
        terminal.draw(|frame| {
            let area = frame.area();
            let text = format!("Connecting to server...\n\nPress ESC to cancel");
            let para = ratatui::widgets::Paragraph::new(text)
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(para, area);
        })?;

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait for room setup (creation or join) and opponent
fn wait_for_room_setup<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: &TcpNetworkClient,
    is_host: bool,
) -> Result<(String, Side), io::Error> {
    let mut room_id: Option<String> = None;
    let mut your_side: Option<Side> = None;
    let mut waiting_for_opponent = false;

    loop {
        // Check for ESC key to cancel
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
                }
            }
        }

        // Process network events
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::RoomCreated { room_id: rid } => {
                    room_id = Some(rid);
                }
                NetworkEvent::WaitingForOpponent => {
                    waiting_for_opponent = true;
                }
                NetworkEvent::RoomJoined {
                    room_id: rid,
                    your_side: side,
                } => {
                    room_id = Some(rid);
                    your_side = Some(side);

                    // If we have both, we're done
                    if room_id.is_some() && your_side.is_some() {
                        return Ok((room_id.unwrap(), your_side.unwrap()));
                    }
                }
                NetworkEvent::Error(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                NetworkEvent::Disconnected => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Disconnected",
                    ));
                }
                _ => {}
            }
        }

        // Render waiting screen
        terminal.draw(|frame| {
            let area = frame.area();
            let text = if is_host {
                if waiting_for_opponent {
                    format!(
                        "Room created: {}\n\nWaiting for opponent...\n\nPress ESC to cancel",
                        room_id.as_ref().unwrap_or(&"???".to_string())
                    )
                } else {
                    "Creating room...\n\nPress ESC to cancel".to_string()
                }
            } else {
                "Joining room...\n\nPress ESC to cancel".to_string()
            };

            let para = ratatui::widgets::Paragraph::new(text)
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(para, area);
        })?;

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Main game loop for TCP networked game
fn run_game_tcp<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: TcpNetworkClient,
    your_side: Side,
    config: &Config,
) -> Result<(), io::Error> {
    let frame_duration = Duration::from_millis(1000 / config.display.target_fps);

    // Wait for game start with physics config
    let physics = wait_for_game_start(terminal, &client)?;

    let size = terminal.size()?;
    let mut game_state = GameState::new(size.width, size.height, &physics);

    // Dead reckoning for smooth interpolation
    let mut last_ball_update = Instant::now();

    // Input polling based on side
    let poll_input = match your_side {
        Side::Left => crate::game::poll_input_player_left,
        Side::Right => crate::game::poll_input_player_right,
    };

    let mut local_wants_rematch = false;
    let mut peer_wants_rematch = false;

    loop {
        let frame_start = Instant::now();

        // Poll local input
        let actions = poll_input(config)?;

        for action in &actions {
            match action {
                InputAction::Quit => {
                    client.send_command(NetworkCommand::LeaveRoom)?;
                    return Ok(());
                }
                InputAction::Rematch => {
                    if game_state.game_over && !local_wants_rematch {
                        local_wants_rematch = true;
                        client.send_command(NetworkCommand::RematchRequest)?;
                    }
                }
                InputAction::LeftPaddleUp
                | InputAction::LeftPaddleDown
                | InputAction::RightPaddleUp
                | InputAction::RightPaddleDown
                | InputAction::PlayerPaddleUp
                | InputAction::PlayerPaddleDown => {
                    // Send input to server
                    client.send_input(*action)?;
                }
            }
        }

        // Process network events
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::GameState {
                    ball,
                    left_score,
                    right_score,
                    game_over,
                } => {
                    // Update from authoritative server state
                    game_state.ball.x = ball.x;
                    game_state.ball.y = ball.y;
                    game_state.ball.vx = ball.vx;
                    game_state.ball.vy = ball.vy;
                    game_state.left_score = left_score;
                    game_state.right_score = right_score;
                    game_state.game_over = game_over;

                    last_ball_update = Instant::now();
                }
                NetworkEvent::OpponentInput { action } => {
                    // Apply opponent paddle movement
                    apply_paddle_input(&mut game_state, action, !your_side);
                }
                NetworkEvent::GameOver { winner } => {
                    game_state.game_over = true;
                    game_state.winner = Some(match winner {
                        Side::Left => game::Player::Left,
                        Side::Right => game::Player::Right,
                    });
                }
                NetworkEvent::RematchStarting => {
                    game_state.reset_game();
                    local_wants_rematch = false;
                    peer_wants_rematch = false;
                }
                NetworkEvent::OpponentDisconnected => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Opponent disconnected",
                    ));
                }
                NetworkEvent::Disconnected => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Server disconnected",
                    ));
                }
                NetworkEvent::Error(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                _ => {}
            }
        }

        // Dead reckoning: predict ball movement between server updates
        if !game_state.game_over && last_ball_update.elapsed() < Duration::from_millis(100) {
            let dt = FIXED_TIMESTEP;
            game_state.ball.x += game_state.ball.vx * dt;
            game_state.ball.y += game_state.ball.vy * dt;
        }

        // Render
        let overlay = if game_state.game_over {
            let winner_text = match game_state.winner {
                Some(game::Player::Left) => "LEFT WINS",
                Some(game::Player::Right) => "RIGHT WINS",
                None => "GAME OVER",
            };

            if local_wants_rematch && peer_wants_rematch {
                Some(format!("{}\n\nStarting rematch...", winner_text))
            } else if local_wants_rematch {
                Some(format!("{}\n\nWaiting for opponent...", winner_text))
            } else {
                Some(format!(
                    "{}\n\nPress R to rematch\nPress Q to quit",
                    winner_text
                ))
            }
        } else {
            None
        };

        let rtt_ms = None; // TODO: Could implement RTT tracking
        let your_player = Some(match your_side {
            Side::Left => game::Player::Left,
            Side::Right => game::Player::Right,
        });

        terminal.draw(|frame| {
            use crate::ui::{OverlayMessage, OverlayStyle};
            let overlay_msg = overlay.as_ref().map(|text| OverlayMessage {
                lines: text.split('\n').map(|s| s.to_string()).collect(),
                style: OverlayStyle::Info,
            });

            ui::render(
                frame,
                &game_state,
                rtt_ms,
                overlay_msg.as_ref(),
                your_player,
                &config.display,
            );
        })?;

        // Frame rate limiting
        limit_frame_rate(frame_start, frame_duration);
    }
}

/// Wait for game start signal from server
fn wait_for_game_start<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: &TcpNetworkClient,
) -> Result<PhysicsConfig, io::Error> {
    loop {
        // Check for ESC key to cancel
        while event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
                }
            }
        }

        // Check for game start event
        while let Some(event) = client.try_recv_event() {
            match event {
                NetworkEvent::GameStart { physics } => {
                    return Ok(physics);
                }
                NetworkEvent::Error(e) => {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                NetworkEvent::Disconnected => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Disconnected",
                    ));
                }
                _ => {}
            }
        }

        // Render waiting screen
        terminal.draw(|frame| {
            let area = frame.area();
            let text = "Waiting for game to start...\n\nPress ESC to cancel";
            let para = ratatui::widgets::Paragraph::new(text)
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(para, area);
        })?;

        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Apply paddle input based on side
fn apply_paddle_input(game_state: &mut GameState, action: InputAction, side: Side) {
    match (side, action) {
        (Side::Left, InputAction::LeftPaddleUp) | (Side::Left, InputAction::PlayerPaddleUp) => {
            game::physics::move_paddle_up(&mut game_state.left_paddle, game_state.tap_distance);
        }
        (Side::Left, InputAction::LeftPaddleDown) | (Side::Left, InputAction::PlayerPaddleDown) => {
            game::physics::move_paddle_down(
                &mut game_state.left_paddle,
                game_state.field_height,
                game_state.tap_distance,
            );
        }
        (Side::Right, InputAction::RightPaddleUp) | (Side::Right, InputAction::PlayerPaddleUp) => {
            game::physics::move_paddle_up(&mut game_state.right_paddle, game_state.tap_distance);
        }
        (Side::Right, InputAction::RightPaddleDown)
        | (Side::Right, InputAction::PlayerPaddleDown) => {
            game::physics::move_paddle_down(
                &mut game_state.right_paddle,
                game_state.field_height,
                game_state.tap_distance,
            );
        }
        _ => {}
    }
}

/// Helper to convert Side to opposite side
impl std::ops::Not for Side {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}
