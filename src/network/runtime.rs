// Network runtime - spawns libp2p in background thread
// Bridges async network with sync game loop via channels

use libp2p::{
    gossipsub, identity, noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use futures::StreamExt;
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use tokio::runtime::Runtime;

use super::{
    behaviour::PongBehaviour,
    client::{NetworkCommand, NetworkEvent},
    protocol::NetworkMessage,
};

/// Initialize and run the libp2p network in a background thread
pub fn spawn_network_thread(
    mode: super::client::ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> std::io::Result<()> {
    thread::spawn(move || {
        // Create tokio runtime for async network operations
        let rt = Runtime::new().expect("Failed to create tokio runtime");
        
        rt.block_on(async move {
            if let Err(e) = run_network(mode, event_tx, cmd_rx, connected).await {
                eprintln!("Network error: {}", e);
            }
        });
    });
    
    Ok(())
}

/// Main network event loop
async fn run_network(
    mode: super::client::ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> std::io::Result<()> {
    // Generate identity (keypair) for this peer
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    
    println!("Local peer id: {}", local_peer_id);
    
    // Build TCP transport with Noise encryption and Yamux multiplexing
    let transport = tcp::tokio::Transport::default()
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(noise::Config::new(&local_key).expect("Failed to create noise config"))
        .multiplex(yamux::Config::default())
        .boxed();
    
    // Create swarm with gossipsub behaviour
    let behaviour = PongBehaviour::new(&local_key);
    let mut swarm = Swarm::new(transport, behaviour, local_peer_id, libp2p::swarm::Config::with_tokio_executor());
    
    // Create and subscribe to game topic
    let topic = gossipsub::IdentTopic::new("p2pong-game");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)
        .expect("Failed to subscribe to game topic");
    println!("📻 Subscribed to topic: p2pong-game");
    
    // Start listening or connect based on mode
    match mode {
        super::client::ConnectionMode::Listen { port } => {
            let listen_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port)
                .parse()
                .expect("Invalid listen address");
            
            swarm.listen_on(listen_addr.clone())
                .expect("Failed to start listening");
            
            println!("Listening on {}/p2p/{}", listen_addr, local_peer_id);
            println!("Share this address with your opponent:");
            println!("  /ip4/127.0.0.1/tcp/{}/p2p/{}", port, local_peer_id);
        }
        super::client::ConnectionMode::Connect { multiaddr } => {
            let remote_addr: Multiaddr = multiaddr.parse()
                .expect("Invalid multiaddr");
            
            println!("Connecting to {}", remote_addr);
            swarm.dial(remote_addr)
                .expect("Failed to dial peer");
        }
    }
    
    // Main event loop
    let mut peer_id: Option<PeerId> = None;
    let game_topic = gossipsub::IdentTopic::new("p2pong-game");
    
    loop {
        tokio::select! {
            // Handle swarm events (incoming connections, messages, etc.)
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::ConnectionEstablished { peer_id: peer, .. } => {
                        println!("✅ Connection established with {}", peer);
                        peer_id = Some(peer);
                        connected.store(true, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Connected {
                            peer_id: peer.to_string(),
                        });
                    }
                    SwarmEvent::ConnectionClosed { peer_id: peer, cause, .. } => {
                        println!("❌ Connection closed with {}: {:?}", peer, cause);
                        connected.store(false, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("🎧 Listening on {}/p2p/{}", address, local_peer_id);
                    }
                    SwarmEvent::Behaviour(event) => {
                        use super::behaviour::PongBehaviourEvent;
                        use libp2p::gossipsub::Event as GossipsubEvent;
                        
                        match event {
                            PongBehaviourEvent::Gossipsub(GossipsubEvent::Message {
                                message,
                                propagation_source,
                                ..
                            }) => {
                                // Ignore own messages
                                if propagation_source == local_peer_id {
                                    continue;
                                }
                                
                                // Deserialize network message
                                if let Ok(msg) = bincode::deserialize::<NetworkMessage>(&message.data) {
                                    match msg {
                                        NetworkMessage::Input(action) => {
                                            let _ = event_tx.send(NetworkEvent::ReceivedInput(action));
                                        }
                                        NetworkMessage::BallSync(ball_state) => {
                                            let _ = event_tx.send(NetworkEvent::ReceivedBallState(ball_state));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            PongBehaviourEvent::Ping(_) => {
                                // Connection health check
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            
            // Poll commands from game loop (non-blocking)
            // For Day 2, we'll skip command handling - just getting connectivity working
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                // Check for commands
                if let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        NetworkCommand::SendInput(action) => {
                            let msg = NetworkMessage::Input(action);
                            let bytes = bincode::serialize(&msg)
                                .expect("Failed to serialize input");
                            
                            let _ = swarm.behaviour_mut().gossipsub.publish(
                                game_topic.clone(),
                                bytes
                            );
                        }
                        NetworkCommand::SendMessage(msg) => {
                            let bytes = bincode::serialize(&msg)
                                .expect("Failed to serialize message");
                            
                            let _ = swarm.behaviour_mut().gossipsub.publish(
                                game_topic.clone(),
                                bytes
                            );
                        }
                        NetworkCommand::Disconnect => {
                            println!("Disconnecting...");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}
