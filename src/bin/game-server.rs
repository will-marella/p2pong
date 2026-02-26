// P2Pong Game Server
// TCP-based client-server architecture for reliable multiplayer
//
// Usage: cargo run --bin game-server

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// Import game types
use p2pong::config::PhysicsConfig;
use p2pong::game::{GameState, InputAction, Player as GamePlayer};
use p2pong::network::protocol::BallState;

type ClientId = u64;
type RoomId = String;

const FRAME_DURATION: Duration = Duration::from_millis(16); // ~60 FPS

/// Messages sent from server to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    /// Successful room creation
    RoomCreated { room_id: String },

    /// List of available rooms
    RoomList { rooms: Vec<RoomInfo> },

    /// Successfully joined a room
    RoomJoined {
        room_id: String,
        your_side: Side,
        opponent_id: ClientId,
    },

    /// Waiting for opponent in room
    WaitingForOpponent { room_id: String },

    /// Game is starting
    GameStart { physics: PhysicsConfig },

    /// Authoritative game state update
    GameState {
        ball: BallState,
        left_score: u8,
        right_score: u8,
        game_over: bool,
    },

    /// Opponent input for their paddle
    OpponentInput { action: InputAction },

    /// Game over notification
    GameOver { winner: Side },

    /// Both players ready for rematch
    RematchStarting,

    /// Opponent disconnected
    OpponentDisconnected,

    /// Error message
    Error { message: String },

    /// Ping for RTT measurement
    Ping { timestamp_ms: u64 },

    /// Pong response
    Pong { timestamp_ms: u64 },
}

/// Messages sent from clients to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    /// Create a new room
    CreateRoom { room_name: String },

    /// Request list of available rooms
    ListRooms,

    /// Join an existing room
    JoinRoom { room_id: String },

    /// Player input (paddle movement)
    Input { action: InputAction },

    /// Ready to start game (after joining)
    Ready,

    /// Request rematch
    RematchRequest,

    /// Leave current room
    LeaveRoom,

    /// Ping request
    Ping { timestamp_ms: u64 },

    /// Pong response
    Pong { timestamp_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomInfo {
    room_id: String,
    room_name: String,
    host_id: ClientId,
    player_count: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RoomStatus {
    WaitingForPlayers,
    Ready,
    Playing,
    GameOver,
}

struct GameRoom {
    room_id: RoomId,
    room_name: String,
    host: ClientId,
    guest: Option<ClientId>,
    status: RoomStatus,
    game_state: Option<GameState>,
    physics: PhysicsConfig,
    host_ready: bool,
    guest_ready: bool,
    host_wants_rematch: bool,
    guest_wants_rematch: bool,
    last_update: Instant,
    ball_sequence: u64,
}

impl GameRoom {
    fn new(room_id: RoomId, room_name: String, host: ClientId) -> Self {
        Self {
            room_id,
            room_name,
            host,
            guest: None,
            status: RoomStatus::WaitingForPlayers,
            game_state: None,
            physics: PhysicsConfig::default(),
            host_ready: false,
            guest_ready: false,
            host_wants_rematch: false,
            guest_wants_rematch: false,
            last_update: Instant::now(),
            ball_sequence: 0,
        }
    }

    fn add_guest(&mut self, guest: ClientId) -> bool {
        if self.guest.is_none() {
            self.guest = Some(guest);
            self.status = RoomStatus::Ready;
            true
        } else {
            false
        }
    }

    fn is_full(&self) -> bool {
        self.guest.is_some()
    }

    fn get_side(&self, client_id: ClientId) -> Option<Side> {
        if self.host == client_id {
            Some(Side::Left)
        } else if self.guest == Some(client_id) {
            Some(Side::Right)
        } else {
            None
        }
    }

    fn get_opponent(&self, client_id: ClientId) -> Option<ClientId> {
        if self.host == client_id {
            self.guest
        } else if self.guest == Some(client_id) {
            Some(self.host)
        } else {
            None
        }
    }

    fn start_game(&mut self) {
        self.game_state = Some(GameState::new(0, 0, &self.physics));
        self.status = RoomStatus::Playing;
        self.last_update = Instant::now();
    }

    fn update_game(&mut self, timestep: f32) -> Vec<ServerMessage> {
        let mut messages = Vec::new();

        if let Some(ref mut game_state) = self.game_state {
            if !game_state.game_over {
                // Update physics
                p2pong::game::update_with_events(game_state, timestep);

                // Send state update
                let ball_state = BallState {
                    x: game_state.ball.x,
                    y: game_state.ball.y,
                    vx: game_state.ball.vx,
                    vy: game_state.ball.vy,
                    sequence: self.ball_sequence,
                    timestamp_ms: self.last_update.elapsed().as_millis() as u64,
                };
                self.ball_sequence += 1;

                messages.push(ServerMessage::GameState {
                    ball: ball_state,
                    left_score: game_state.left_score,
                    right_score: game_state.right_score,
                    game_over: game_state.game_over,
                });

                // Check for game over
                if game_state.game_over {
                    self.status = RoomStatus::GameOver;
                    if let Some(winner) = game_state.winner {
                        let winner_side = match winner {
                            GamePlayer::Left => Side::Left,
                            GamePlayer::Right => Side::Right,
                        };
                        messages.push(ServerMessage::GameOver {
                            winner: winner_side,
                        });
                    }
                }
            }
        }

        messages
    }

    fn handle_input(&mut self, client_id: ClientId, action: InputAction) {
        let side = self.get_side(client_id);

        if let Some(ref mut game_state) = self.game_state {
            // Apply input based on player side
            match (side, action) {
                (Some(Side::Left), InputAction::LeftPaddleUp)
                | (Some(Side::Left), InputAction::PlayerPaddleUp) => {
                    p2pong::game::physics::move_paddle_up(
                        &mut game_state.left_paddle,
                        game_state.tap_distance,
                    );
                }
                (Some(Side::Left), InputAction::LeftPaddleDown)
                | (Some(Side::Left), InputAction::PlayerPaddleDown) => {
                    p2pong::game::physics::move_paddle_down(
                        &mut game_state.left_paddle,
                        game_state.field_height,
                        game_state.tap_distance,
                    );
                }
                (Some(Side::Right), InputAction::RightPaddleUp)
                | (Some(Side::Right), InputAction::PlayerPaddleUp) => {
                    p2pong::game::physics::move_paddle_up(
                        &mut game_state.right_paddle,
                        game_state.tap_distance,
                    );
                }
                (Some(Side::Right), InputAction::RightPaddleDown)
                | (Some(Side::Right), InputAction::PlayerPaddleDown) => {
                    p2pong::game::physics::move_paddle_down(
                        &mut game_state.right_paddle,
                        game_state.field_height,
                        game_state.tap_distance,
                    );
                }
                _ => {}
            }
        }
    }

    fn handle_rematch(&mut self, client_id: ClientId) -> Option<ServerMessage> {
        let side = self.get_side(client_id);

        match side {
            Some(Side::Left) => self.host_wants_rematch = true,
            Some(Side::Right) => self.guest_wants_rematch = true,
            None => return None,
        }

        // Both players want rematch
        if self.host_wants_rematch && self.guest_wants_rematch {
            if let Some(ref mut game_state) = self.game_state {
                game_state.reset_game();
                self.status = RoomStatus::Playing;
                self.host_wants_rematch = false;
                self.guest_wants_rematch = false;
                self.ball_sequence = 0;
                return Some(ServerMessage::RematchStarting);
            }
        }

        None
    }
}

struct ClientConnection {
    id: ClientId,
    stream: TcpStream,
    current_room: Option<RoomId>,
}

struct GameServer {
    clients: HashMap<ClientId, ClientConnection>,
    rooms: HashMap<RoomId, GameRoom>,
    next_client_id: ClientId,
    next_room_id: u64,
}

impl GameServer {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
            rooms: HashMap::new(),
            next_client_id: 1,
            next_room_id: 1,
        }
    }

    fn generate_room_id(&mut self) -> RoomId {
        let id = format!("ROOM-{:04}", self.next_room_id);
        self.next_room_id += 1;
        id
    }

    fn create_room(&mut self, client_id: ClientId, room_name: String) -> Result<RoomId, String> {
        let room_id = self.generate_room_id();
        let room = GameRoom::new(room_id.clone(), room_name, client_id);

        self.rooms.insert(room_id.clone(), room);

        if let Some(client) = self.clients.get_mut(&client_id) {
            client.current_room = Some(room_id.clone());
        }

        Ok(room_id)
    }

    fn list_rooms(&self) -> Vec<RoomInfo> {
        self.rooms
            .values()
            .filter(|room| !room.is_full())
            .map(|room| RoomInfo {
                room_id: room.room_id.clone(),
                room_name: room.room_name.clone(),
                host_id: room.host,
                player_count: if room.guest.is_some() { 2 } else { 1 },
            })
            .collect()
    }

    fn join_room(
        &mut self,
        client_id: ClientId,
        room_id: &str,
    ) -> Result<(ClientId, Side), String> {
        let room = self
            .rooms
            .get_mut(room_id)
            .ok_or_else(|| "Room not found".to_string())?;

        if room.is_full() {
            return Err("Room is full".to_string());
        }

        room.add_guest(client_id);

        if let Some(client) = self.clients.get_mut(&client_id) {
            client.current_room = Some(room_id.to_string());
        }

        Ok((room.host, Side::Right))
    }

    fn handle_ready(&mut self, client_id: ClientId) -> Option<Vec<(ClientId, ServerMessage)>> {
        let room_id = self.clients.get(&client_id)?.current_room.clone()?;
        let room = self.rooms.get_mut(&room_id)?;

        let side = room.get_side(client_id)?;

        match side {
            Side::Left => room.host_ready = true,
            Side::Right => room.guest_ready = true,
        }

        // Both players ready - start game
        if room.host_ready && room.guest_ready && room.status == RoomStatus::Ready {
            room.start_game();

            let start_msg = ServerMessage::GameStart {
                physics: room.physics.clone(),
            };

            let mut messages = Vec::new();
            messages.push((room.host, start_msg.clone()));
            if let Some(guest) = room.guest {
                messages.push((guest, start_msg));
            }

            return Some(messages);
        }

        None
    }

    fn remove_client(&mut self, client_id: ClientId) {
        if let Some(client) = self.clients.remove(&client_id) {
            if let Some(room_id) = client.current_room {
                // Notify opponent
                if let Some(room) = self.rooms.get(&room_id) {
                    if let Some(opponent_id) = room.get_opponent(client_id) {
                        let msg = ServerMessage::OpponentDisconnected;
                        let _ = self.send_message(opponent_id, &msg);
                    }
                }

                // Remove room if empty or just cleanup
                self.rooms.remove(&room_id);
            }
        }
    }

    fn send_message(&mut self, client_id: ClientId, msg: &ServerMessage) -> io::Result<()> {
        if let Some(client) = self.clients.get_mut(&client_id) {
            write_message(&mut client.stream, msg)?;
        }
        Ok(())
    }

    fn broadcast_to_room(&mut self, room_id: &str, msg: &ServerMessage) -> io::Result<()> {
        let recipients = if let Some(room) = self.rooms.get(room_id) {
            let mut r = vec![room.host];
            if let Some(guest) = room.guest {
                r.push(guest);
            }
            r
        } else {
            return Ok(());
        };

        for client_id in recipients {
            self.send_message(client_id, msg)?;
        }
        Ok(())
    }
}

// Message framing: [4-byte length][bincode payload]
fn write_message<T: Serialize>(stream: &mut TcpStream, msg: &T) -> io::Result<()> {
    let bytes =
        bincode::serialize(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(&bytes)?;
    stream.flush()?;

    Ok(())
}

fn read_message<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> io::Result<T> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Message too large",
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;

    bincode::deserialize(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn handle_client_message(
    server: &mut GameServer,
    client_id: ClientId,
    msg: ClientMessage,
) -> io::Result<()> {
    match msg {
        ClientMessage::CreateRoom { room_name } => match server.create_room(client_id, room_name) {
            Ok(room_id) => {
                let response = ServerMessage::WaitingForOpponent {
                    room_id: room_id.clone(),
                };
                server.send_message(client_id, &response)?;

                let created = ServerMessage::RoomCreated { room_id };
                server.send_message(client_id, &created)?;
            }
            Err(err) => {
                let response = ServerMessage::Error { message: err };
                server.send_message(client_id, &response)?;
            }
        },

        ClientMessage::ListRooms => {
            let rooms = server.list_rooms();
            let response = ServerMessage::RoomList { rooms };
            server.send_message(client_id, &response)?;
        }

        ClientMessage::JoinRoom { room_id } => {
            match server.join_room(client_id, &room_id) {
                Ok((opponent_id, your_side)) => {
                    let response = ServerMessage::RoomJoined {
                        room_id: room_id.clone(),
                        your_side,
                        opponent_id,
                    };
                    server.send_message(client_id, &response)?;

                    // Notify host that guest joined
                    let host_msg = ServerMessage::RoomJoined {
                        room_id,
                        your_side: Side::Left,
                        opponent_id: client_id,
                    };
                    server.send_message(opponent_id, &host_msg)?;
                }
                Err(err) => {
                    let response = ServerMessage::Error { message: err };
                    server.send_message(client_id, &response)?;
                }
            }
        }

        ClientMessage::Ready => {
            if let Some(messages) = server.handle_ready(client_id) {
                for (recipient, msg) in messages {
                    server.send_message(recipient, &msg)?;
                }
            }
        }

        ClientMessage::Input { action } => {
            let room_id = server
                .clients
                .get(&client_id)
                .and_then(|c| c.current_room.clone());

            if let Some(room_id) = room_id {
                if let Some(room) = server.rooms.get_mut(&room_id) {
                    let opponent_id = room.get_opponent(client_id);
                    room.handle_input(client_id, action);

                    // Forward input to opponent
                    if let Some(opponent_id) = opponent_id {
                        let msg = ServerMessage::OpponentInput { action };
                        server.send_message(opponent_id, &msg)?;
                    }
                }
            }
        }

        ClientMessage::RematchRequest => {
            let room_id = server
                .clients
                .get(&client_id)
                .and_then(|c| c.current_room.clone());

            if let Some(room_id) = room_id {
                if let Some(room) = server.rooms.get_mut(&room_id) {
                    if let Some(msg) = room.handle_rematch(client_id) {
                        server.broadcast_to_room(&room_id, &msg)?;
                    }
                }
            }
        }

        ClientMessage::LeaveRoom => {
            server.remove_client(client_id);
        }

        ClientMessage::Ping { timestamp_ms } => {
            let response = ServerMessage::Pong { timestamp_ms };
            server.send_message(client_id, &response)?;
        }

        ClientMessage::Pong { .. } => {
            // RTT measurement - could log this
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    println!("🎮 P2Pong Game Server Starting...");

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = TcpListener::bind(&addr)?;
    listener.set_nonblocking(true)?;

    println!("🚀 Server listening on {}", addr);

    let server = Arc::new(RwLock::new(GameServer::new()));
    let mut last_update = Instant::now();

    loop {
        let now = Instant::now();
        let delta = now.duration_since(last_update);

        // Accept new connections
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("✅ New connection from {}", addr);
                stream.set_nonblocking(true)?;

                let mut server_lock = server.write().unwrap();
                let client_id = server_lock.next_client_id;
                server_lock.next_client_id += 1;

                server_lock.clients.insert(
                    client_id,
                    ClientConnection {
                        id: client_id,
                        stream,
                        current_room: None,
                    },
                );
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No new connections
            }
            Err(e) => {
                eprintln!("❌ Error accepting connection: {}", e);
            }
        }

        // Handle client messages
        let mut server_lock = server.write().unwrap();
        let client_ids: Vec<ClientId> = server_lock.clients.keys().copied().collect();

        for client_id in client_ids {
            if let Some(client) = server_lock.clients.get_mut(&client_id) {
                match read_message::<ClientMessage>(&mut client.stream) {
                    Ok(msg) => {
                        drop(client); // Release borrow
                        if let Err(e) = handle_client_message(&mut server_lock, client_id, msg) {
                            eprintln!("❌ Error handling client {}: {}", client_id, e);
                            server_lock.remove_client(client_id);
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // No message available
                    }
                    Err(_) => {
                        // Client disconnected or error
                        drop(client);
                        server_lock.remove_client(client_id);
                    }
                }
            }
        }

        // Update game rooms
        if delta >= FRAME_DURATION {
            let timestep = delta.as_secs_f32();
            let room_ids: Vec<RoomId> = server_lock.rooms.keys().cloned().collect();

            for room_id in room_ids {
                if let Some(room) = server_lock.rooms.get_mut(&room_id) {
                    if room.status == RoomStatus::Playing {
                        let messages = room.update_game(timestep);

                        for msg in messages {
                            let _ = server_lock.broadcast_to_room(&room_id, &msg);
                        }
                    }
                }
            }

            last_update = now;
        }

        drop(server_lock);

        // Small sleep to prevent busy waiting
        std::thread::sleep(Duration::from_millis(1));
    }
}
