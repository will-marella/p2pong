// TCP client for P2Pong game server
// Replaces WebRTC P2P with simple client-server TCP connection

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::protocol::{BallState, NetworkMessage};
use crate::config::PhysicsConfig;
use crate::game::InputAction;

/// Messages sent from server to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RoomCreated {
        room_id: String,
    },
    RoomList {
        rooms: Vec<RoomInfo>,
    },
    RoomJoined {
        room_id: String,
        your_side: Side,
        opponent_id: u64,
    },
    WaitingForOpponent {
        room_id: String,
    },
    GameStart {
        physics: PhysicsConfig,
    },
    GameState {
        ball: BallState,
        left_score: u8,
        right_score: u8,
        game_over: bool,
    },
    OpponentInput {
        action: InputAction,
    },
    GameOver {
        winner: Side,
    },
    RematchStarting,
    OpponentDisconnected,
    Error {
        message: String,
    },
    Ping {
        timestamp_ms: u64,
    },
    Pong {
        timestamp_ms: u64,
    },
}

/// Messages sent from clients to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    CreateRoom { room_name: String },
    ListRooms,
    JoinRoom { room_id: String },
    Input { action: InputAction },
    Ready,
    RematchRequest,
    LeaveRoom,
    Ping { timestamp_ms: u64 },
    Pong { timestamp_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: String,
    pub room_name: String,
    pub host_id: u64,
    pub player_count: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Side {
    Left,
    Right,
}

/// Events sent from network thread to game loop
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Connected,
    RoomCreated {
        room_id: String,
    },
    RoomList {
        rooms: Vec<RoomInfo>,
    },
    RoomJoined {
        room_id: String,
        your_side: Side,
    },
    WaitingForOpponent,
    GameStart {
        physics: PhysicsConfig,
    },
    GameState {
        ball: BallState,
        left_score: u8,
        right_score: u8,
        game_over: bool,
    },
    OpponentInput {
        action: InputAction,
    },
    GameOver {
        winner: Side,
    },
    RematchStarting,
    OpponentDisconnected,
    Disconnected,
    Error(String),
}

/// Commands sent from game loop to network thread
#[derive(Debug, Clone)]
pub enum NetworkCommand {
    CreateRoom { room_name: String },
    ListRooms,
    JoinRoom { room_id: String },
    SendInput(InputAction),
    Ready,
    RematchRequest,
    LeaveRoom,
    Disconnect,
}

/// TCP network client - simpler alternative to WebRTC
pub struct TcpNetworkClient {
    tx: Sender<NetworkCommand>,
    rx: Receiver<NetworkEvent>,
    connected: Arc<AtomicBool>,
}

impl TcpNetworkClient {
    /// Connect to game server
    pub fn connect(server_addr: &str) -> io::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let connected = Arc::new(AtomicBool::new(false));

        // Spawn network thread
        let server_addr = server_addr.to_string();
        let connected_clone = connected.clone();

        thread::spawn(move || {
            if let Err(e) = run_network_thread(&server_addr, event_tx, cmd_rx, connected_clone) {
                eprintln!("Network thread error: {}", e);
            }
        });

        Ok(Self {
            tx: cmd_tx,
            rx: event_rx,
            connected,
        })
    }

    /// Check if connected to server
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Send command to server
    pub fn send_command(&self, cmd: NetworkCommand) -> io::Result<()> {
        self.tx
            .send(cmd)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    /// Try to receive event from server (non-blocking)
    pub fn try_recv_event(&self) -> Option<NetworkEvent> {
        match self.rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(NetworkEvent::Disconnected),
        }
    }

    /// Send input action
    pub fn send_input(&self, action: InputAction) -> io::Result<()> {
        self.send_command(NetworkCommand::SendInput(action))
    }
}

/// Message framing: [4-byte length][bincode payload]
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

fn run_network_thread(
    server_addr: &str,
    event_tx: Sender<NetworkEvent>,
    cmd_rx: Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> io::Result<()> {
    // Connect to server
    let mut stream = TcpStream::connect(server_addr)?;
    stream.set_nonblocking(true)?;
    stream.set_nodelay(true)?; // Disable Nagle's algorithm for lower latency

    connected.store(true, Ordering::Relaxed);
    let _ = event_tx.send(NetworkEvent::Connected);

    let mut last_ping = Instant::now();

    loop {
        // Handle incoming messages from server
        match read_message::<ServerMessage>(&mut stream) {
            Ok(msg) => {
                let event = match msg {
                    ServerMessage::RoomCreated { room_id } => NetworkEvent::RoomCreated { room_id },
                    ServerMessage::RoomList { rooms } => NetworkEvent::RoomList { rooms },
                    ServerMessage::RoomJoined {
                        room_id, your_side, ..
                    } => NetworkEvent::RoomJoined { room_id, your_side },
                    ServerMessage::WaitingForOpponent { .. } => NetworkEvent::WaitingForOpponent,
                    ServerMessage::GameStart { physics } => NetworkEvent::GameStart { physics },
                    ServerMessage::GameState {
                        ball,
                        left_score,
                        right_score,
                        game_over,
                    } => NetworkEvent::GameState {
                        ball,
                        left_score,
                        right_score,
                        game_over,
                    },
                    ServerMessage::OpponentInput { action } => {
                        NetworkEvent::OpponentInput { action }
                    }
                    ServerMessage::GameOver { winner } => NetworkEvent::GameOver { winner },
                    ServerMessage::RematchStarting => NetworkEvent::RematchStarting,
                    ServerMessage::OpponentDisconnected => NetworkEvent::OpponentDisconnected,
                    ServerMessage::Error { message } => NetworkEvent::Error(message),
                    ServerMessage::Ping { timestamp_ms } => {
                        // Respond to ping
                        let pong = ClientMessage::Pong { timestamp_ms };
                        let _ = write_message(&mut stream, &pong);
                        continue; // Don't send to game loop
                    }
                    ServerMessage::Pong { .. } => {
                        // RTT measurement complete
                        continue; // Don't send to game loop
                    }
                };

                if event_tx.send(event).is_err() {
                    break; // Game loop disconnected
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // No message available
            }
            Err(e) => {
                eprintln!("Error reading from server: {}", e);
                break;
            }
        }

        // Handle outgoing commands from game loop
        match cmd_rx.try_recv() {
            Ok(cmd) => {
                let msg = match cmd {
                    NetworkCommand::CreateRoom { room_name } => {
                        ClientMessage::CreateRoom { room_name }
                    }
                    NetworkCommand::ListRooms => ClientMessage::ListRooms,
                    NetworkCommand::JoinRoom { room_id } => ClientMessage::JoinRoom { room_id },
                    NetworkCommand::SendInput(action) => ClientMessage::Input { action },
                    NetworkCommand::Ready => ClientMessage::Ready,
                    NetworkCommand::RematchRequest => ClientMessage::RematchRequest,
                    NetworkCommand::LeaveRoom => ClientMessage::LeaveRoom,
                    NetworkCommand::Disconnect => {
                        let _ = write_message(&mut stream, &ClientMessage::LeaveRoom);
                        break;
                    }
                };

                if let Err(e) = write_message(&mut stream, &msg) {
                    eprintln!("Error writing to server: {}", e);
                    break;
                }
            }
            Err(TryRecvError::Empty) => {
                // No command available
            }
            Err(TryRecvError::Disconnected) => {
                // Game loop disconnected
                break;
            }
        }

        // Send periodic ping for keepalive
        if last_ping.elapsed() > Duration::from_secs(2) {
            let ping = ClientMessage::Ping {
                timestamp_ms: last_ping.elapsed().as_millis() as u64,
            };
            let _ = write_message(&mut stream, &ping);
            last_ping = Instant::now();
        }

        // Small sleep to prevent busy waiting
        thread::sleep(Duration::from_millis(1));
    }

    connected.store(false, Ordering::Relaxed);
    let _ = event_tx.send(NetworkEvent::Disconnected);

    Ok(())
}
