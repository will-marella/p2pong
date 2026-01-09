// Network runtime - spawns libp2p in background thread
// Bridges async network with sync game loop via channels

use libp2p::{
    dcutr, gossipsub, identify, identity, noise, relay,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use futures::StreamExt;
use std::sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}};
use std::str::FromStr;
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

/// Connection state tracking for relay
struct ConnectionState {
    relay_connected: bool,
    relay_reservation_ready: bool,
    target_peer_id: Option<PeerId>,
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
    
    // Build swarm using SwarmBuilder with proper relay integration
    let mut swarm = SwarmBuilder::with_existing_identity(local_key.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default().port_reuse(true).nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("Failed to build TCP transport")
        .with_quic()
        .with_dns()
        .expect("Failed to build DNS transport")
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .expect("Failed to build relay client")
        .with_behaviour(|keypair, relay_client| {
            PongBehaviour::new(keypair, local_peer_id, relay_client)
        })
        .expect("Failed to build behaviour")
        .with_swarm_config(|c| c.with_idle_connection_timeout(std::time::Duration::from_secs(60)))
        .build();
    
    // Create and subscribe to game topic
    let topic = gossipsub::IdentTopic::new("p2pong-game");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)
        .expect("Failed to subscribe to game topic");
    println!("📻 Subscribed to topic: p2pong-game");
    
    // Connect to our NYC relay server for NAT traversal
    // The relay client will automatically request a reservation once connected
    let relay_address = "/ip4/143.198.15.158/tcp/4001/p2p/12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X"
        .parse::<Multiaddr>()
        .expect("Invalid relay address");
    
    println!("🔗 Connecting to NYC relay server (143.198.15.158:4001)...");
    match swarm.dial(relay_address) {
        Ok(_) => println!("   ↳ Dialing relay server..."),
        Err(e) => eprintln!("   ✗ Failed to dial relay: {:?}", e),
    }
    
    // Initialize connection state
    let mut conn_state = ConnectionState {
        relay_connected: false,
        relay_reservation_ready: false,
        target_peer_id: None,
    };
    
    // Start listening or connect based on mode
    match mode {
        super::client::ConnectionMode::Listen { port } => {
            let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port)
                .parse()
                .expect("Invalid listen address");
            
            swarm.listen_on(listen_addr.clone())
                .expect("Failed to start listening");
            
            println!("🎧 Listening on {}/p2p/{}", listen_addr, local_peer_id);
            println!();
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("📋 Share this Peer ID with your opponent:");
            println!();
            println!("   {}", local_peer_id);
            println!();
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!();
            println!("💡 They should run:");
            println!("   ./p2pong --connect {}", local_peer_id);
            println!();
        }
        super::client::ConnectionMode::Connect { multiaddr } => {
            // Parse the multiaddr - could be just a peer ID or a full multiaddr
            let addr_str = multiaddr.trim();
            
            // Check if it's just a peer ID (format: /p2p/PEER_ID)
            if addr_str.starts_with("/p2p/") && !addr_str.contains("/ip4/") && !addr_str.contains("/ip6/") {
                // Extract peer ID from /p2p/PEER_ID format
                let peer_id_str = addr_str.trim_start_matches("/p2p/");
                let target_peer = PeerId::from_str(peer_id_str)
                    .expect("Invalid peer ID");
                
                println!("🔌 Target peer: {}", target_peer);
                println!("🔄 Connecting to relay first, then will connect to peer...");
                conn_state.target_peer_id = Some(target_peer);
            } else {
                // It's a full multiaddr with IP - try to dial directly
                let remote_addr: Multiaddr = addr_str.parse()
                    .expect("Invalid multiaddr");
                
                println!("🔌 Connecting to {}", remote_addr);
                swarm.dial(remote_addr)
                    .expect("Failed to dial peer");
                println!("⏳ Waiting for connection (direct or via relay)...");
            }
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
                    SwarmEvent::ConnectionEstablished { peer_id: peer, endpoint, .. } => {
                        // Determine connection type by checking endpoint address
                        let endpoint_str = format!("{:?}", endpoint);
                        let is_relayed = endpoint_str.contains("p2p-circuit");
                        let conn_type = if is_relayed { "relay circuit" } else { "direct" };
                        
                        println!("✅ Connection established with {} (type: {})", peer, conn_type);
                        
                        // Check if this is our relay server
                        if peer.to_string() == "12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X" {
                            println!("🎉 Connected to NYC relay server!");
                            println!("   ↳ Endpoint: {:?}", endpoint);
                            println!("   ↳ Requesting relay reservation...");
                            
                            conn_state.relay_connected = true;
                            
                            // Listen on relay circuit to trigger reservation
                            let relay_listen_addr = format!("/ip4/143.198.15.158/tcp/4001/p2p/{}/p2p-circuit", peer)
                                .parse::<Multiaddr>()
                                .expect("Invalid relay listen address");
                            
                            match swarm.listen_on(relay_listen_addr) {
                                Ok(_) => println!("   ↳ Listening on relay circuit..."),
                                Err(e) => eprintln!("   ✗ Failed to listen on relay: {:?}", e),
                            }
                        } else {
                            // This is our game opponent
                            if is_relayed {
                                println!("   ↳ Using relay circuit (DCUTR will attempt direct upgrade)");
                            } else {
                                println!("   ↳ 🚀 Direct peer-to-peer connection!");
                            }
                            
                            peer_id = Some(peer);
                            connected.store(true, Ordering::Relaxed);
                            let _ = event_tx.send(NetworkEvent::Connected {
                                peer_id: peer.to_string(),
                            });
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id: peer, cause, .. } => {
                        println!("❌ Connection closed with {}: {:?}", peer, cause);
                        connected.store(false, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("🎧 Listening on {}/p2p/{}", address, local_peer_id);
                    }
                    SwarmEvent::ExternalAddrConfirmed { address } => {
                        println!("📍 External address discovered: {}", address);
                        println!("   ↳ This address is reachable from the internet");
                        println!("   ↳ DCUTR can use this for hole punching");
                    }
                    SwarmEvent::ExternalAddrExpired { address } => {
                        println!("⚠️  External address expired: {}", address);
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
                                        NetworkMessage::ScoreSync { left, right, game_over } => {
                                            let _ = event_tx.send(NetworkEvent::ReceivedScore { 
                                                left, 
                                                right, 
                                                game_over 
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            PongBehaviourEvent::Ping(_) => {
                                // Connection health check
                            }
                            PongBehaviourEvent::Identify(_) => {
                                // Peer identification (required by relay/dcutr)
                            }
                            PongBehaviourEvent::RelayClient(relay_event) => {
                                use libp2p::relay::client::Event as RelayEvent;
                                
                                // Log relay events for debugging
                                println!("🔄 Relay: {:?}", relay_event);
                                
                                // Check if we got a reservation
                                if matches!(relay_event, RelayEvent::ReservationReqAccepted { .. }) {
                                    conn_state.relay_reservation_ready = true;
                                    
                                    // If we have a target peer waiting, dial them now via relay
                                    if let Some(target) = conn_state.target_peer_id {
                                        println!("✨ Relay reservation ready! Dialing peer through relay...");
                                        
                                        // Build relay circuit address to target peer
                                        let relay_addr = format!(
                                            "/ip4/143.198.15.158/tcp/4001/p2p/12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X/p2p-circuit/p2p/{}",
                                            target
                                        ).parse::<Multiaddr>()
                                        .expect("Invalid relay circuit address");
                                        
                                        println!("🔗 Connecting via relay: {}", relay_addr);
                                        
                                        match swarm.dial(relay_addr) {
                                            Ok(_) => {
                                                println!("⏳ Dialing peer through relay circuit...");
                                                conn_state.target_peer_id = None; // Clear so we don't dial again
                                            }
                                            Err(e) => eprintln!("❌ Failed to dial through relay: {:?}", e),
                                        }
                                    }
                                }
                            }
                            PongBehaviourEvent::Dcutr(dcutr_event) => {
                                // Parse DCUTR (hole punching) events to show meaningful status
                                match dcutr_event.result {
                                    Ok(connection_id) => {
                                        println!("🎯 DCUTR: ✅ SUCCESS! Hole punch complete");
                                        println!("   ↳ Peer: {}", dcutr_event.remote_peer_id);
                                        println!("   ↳ Direct P2P connection established (ConnectionId: {:?})", connection_id);
                                        println!("   ↳ Game traffic now using direct peer-to-peer!");
                                    }
                                    Err(err) => {
                                        println!("🎯 DCUTR: ⚠️  Hole punch failed");
                                        println!("   ↳ Peer: {}", dcutr_event.remote_peer_id);
                                        println!("   ↳ Reason: {}", err);
                                        println!("   ↳ Continuing via relay connection");
                                    }
                                }
                            }
                            PongBehaviourEvent::Autonat(autonat_event) => {
                                use libp2p::autonat::Event as AutonatEvent;
                                
                                match autonat_event {
                                    AutonatEvent::StatusChanged { old, new } => {
                                        println!("🌐 AutoNAT: Status changed from {:?} to {:?}", old, new);
                                        
                                        use libp2p::autonat::NatStatus;
                                        match new {
                                            NatStatus::Public(addr) => {
                                                println!("   ↳ ✅ Public address! Directly reachable from internet");
                                                println!("   ↳ Address: {}", addr);
                                                println!("   ↳ DCUTR hole punching should work well!");
                                            }
                                            NatStatus::Private => {
                                                println!("   ↳ 🔒 Behind NAT (private network)");
                                                println!("   ↳ DCUTR will attempt hole punching");
                                                println!("   ↳ Success depends on NAT type");
                                            }
                                            NatStatus::Unknown => {
                                                println!("   ↳ ❓ NAT status unknown (still probing...)");
                                            }
                                        }
                                    }
                                    AutonatEvent::InboundProbe(probe_event) => {
                                        // Another peer is probing us to help determine our NAT status
                                        use libp2p::autonat::InboundProbeEvent;
                                        match probe_event {
                                            InboundProbeEvent::Request { peer, .. } => {
                                                println!("🌐 AutoNAT: Received probe request from {}", peer);
                                            }
                                            InboundProbeEvent::Response { peer, .. } => {
                                                println!("🌐 AutoNAT: Sent probe response to {}", peer);
                                            }
                                            InboundProbeEvent::Error { peer, error, .. } => {
                                                println!("🌐 AutoNAT: Probe error from {}: {:?}", peer, error);
                                            }
                                        }
                                    }
                                    AutonatEvent::OutboundProbe(probe_event) => {
                                        // We're probing another peer to determine our NAT status
                                        use libp2p::autonat::OutboundProbeEvent;
                                        match probe_event {
                                            OutboundProbeEvent::Request { peer, .. } => {
                                                println!("🌐 AutoNAT: Probing {} for NAT status...", peer);
                                            }
                                            OutboundProbeEvent::Response { peer, .. } => {
                                                println!("🌐 AutoNAT: Got probe response from {}", peer);
                                            }
                                            OutboundProbeEvent::Error { peer, error, .. } => {
                                                if let Some(p) = peer {
                                                    println!("🌐 AutoNAT: Probe to {} failed: {:?}", p, error);
                                                } else {
                                                    println!("🌐 AutoNAT: Probe failed: {:?}", error);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        eprintln!("❌ Failed to connect to {:?}: {}", peer_id, error);
                        
                        // Check if this was the relay server
                        if let Some(pid) = peer_id {
                            if pid.to_string() == "12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X" {
                                eprintln!("⚠️  Relay server connection failed!");
                                eprintln!("   Check that relay is running and accessible.");
                            }
                        }
                    }
                    SwarmEvent::IncomingConnectionError { send_back_addr, error, .. } => {
                        eprintln!("❌ Incoming connection error from {}: {}", send_back_addr, error);
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
