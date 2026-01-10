// Network runtime - spawns libp2p in background thread
// Bridges async network with sync game loop via channels

use futures::StreamExt;
use libp2p::{
    dcutr, gossipsub, identify, identity, noise, relay, swarm::SwarmEvent, tcp, yamux, Multiaddr,
    PeerId, SwarmBuilder,
};
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;

use super::{
    behaviour::PongBehaviour,
    client::{NetworkCommand, NetworkEvent},
    protocol::NetworkMessage,
};

// Relay server configuration (QUIC-only for better NAT traversal)
const RELAY_ADDRESS: &str =
    "/ip4/143.198.15.158/udp/4001/quic-v1/p2p/12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X";
const RELAY_PEER_ID: &str = "12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X";

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

/// Connection state tracking for relay and DCUTR
struct ConnectionState {
    relay_connected: bool,
    relay_reservation_ready: bool,
    target_peer_id: Option<PeerId>,

    // Game peer connection tracking (for DCUTR requirement)
    game_peer_relay_connection: Option<libp2p::swarm::ConnectionId>,
    game_peer_direct_connection: Option<libp2p::swarm::ConnectionId>,
    awaiting_dcutr: bool,
    dcutr_deadline: Option<tokio::time::Instant>,

    // External address discovery (needed before dialing peer)
    external_address_discovered: bool,
}

/// Main network event loop
async fn run_network(
    mode: super::client::ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> std::io::Result<()> {
    // Track start time for debugging timing issues
    let start_time = Instant::now();

    // Generate identity (keypair) for this peer
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    println!("Local peer id: {}", local_peer_id);
    eprintln!("[{:?}] 🕐 Session started", start_time.elapsed());

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

    eprintln!("✅ Swarm initialized with behaviours:");
    eprintln!("   - Gossipsub (game messages)");
    eprintln!("   - Ping (connection health)");
    eprintln!("   - Relay Client (NAT traversal)");
    eprintln!("   - DCUTR (hole punching - listening for NewExternalAddrCandidate events)");
    eprintln!("   - Identify (peer discovery & external IP observation)");
    eprintln!("   - AutoNAT (NAT status detection)");
    eprintln!("   - UPnP (automatic port forwarding)");
    eprintln!("");

    // Create and subscribe to game topic
    let topic = gossipsub::IdentTopic::new("p2pong-game");
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&topic)
        .expect("Failed to subscribe to game topic");
    println!("📻 Subscribed to topic: p2pong-game");

    // Connect to our NYC relay server for NAT traversal
    // The relay client will automatically request a reservation once connected
    let relay_address = RELAY_ADDRESS
        .parse::<Multiaddr>()
        .expect("Invalid relay address");

    println!("🔗 Connecting to relay server via QUIC (143.198.15.158:4001/udp)...");
    match swarm.dial(relay_address) {
        Ok(_) => {}
        Err(e) => eprintln!("   ✗ Failed to dial relay via QUIC: {:?}", e),
    }

    // Initialize connection state
    let mut conn_state = ConnectionState {
        relay_connected: false,
        relay_reservation_ready: false,
        target_peer_id: None,
        game_peer_relay_connection: None,
        game_peer_direct_connection: None,
        awaiting_dcutr: false,
        dcutr_deadline: None,
        external_address_discovered: false,
    };

    // Start listening or connect based on mode
    match mode {
        super::client::ConnectionMode::Listen { port, external_ip } => {
            // Listen on TCP
            let tcp_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port)
                .parse()
                .expect("Invalid TCP listen address");
            swarm
                .listen_on(tcp_addr.clone())
                .expect("Failed to start TCP listening");

            // Listen on QUIC/UDP (better for NAT traversal)
            let quic_addr: Multiaddr = format!("/ip4/0.0.0.0/udp/{}/quic-v1", port)
                .parse()
                .expect("Invalid QUIC listen address");
            swarm
                .listen_on(quic_addr.clone())
                .expect("Failed to start QUIC listening");

            println!("🎧 Listening on TCP: {}/p2p/{}", tcp_addr, local_peer_id);
            println!("🎧 Listening on QUIC: {}/p2p/{}", quic_addr, local_peer_id);

            // If external IP is provided, add it as external addresses
            // This prevents ephemeral port issues on public IP hosts
            if let Some(ref ip) = external_ip {
                println!();
                println!("🌐 Adding manual external addresses (fixes NAT port mapping):");

                // Add TCP external address
                let tcp_external = format!("/ip4/{}/tcp/{}", ip, port);
                if let Ok(addr) = tcp_external.parse::<Multiaddr>() {
                    swarm.add_external_address(addr.clone());
                    println!("   ✅ TCP: {}", addr);
                }

                // Add QUIC external address
                let quic_external = format!("/ip4/{}/udp/{}/quic-v1", ip, port);
                if let Ok(addr) = quic_external.parse::<Multiaddr>() {
                    swarm.add_external_address(addr.clone());
                    println!("   ✅ QUIC: {}", addr);
                }

                println!("   ↳ DCUTR will use these addresses (not ephemeral ports)");
            }

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
            // CRITICAL: Even in connect mode, we need to listen on a port!
            // This is required for DCUTR hole-punching to work properly.

            // Listen on random TCP port
            let tcp_addr: Multiaddr = "/ip4/0.0.0.0/tcp/0"
                .parse()
                .expect("Invalid TCP listen address");
            match swarm.listen_on(tcp_addr) {
                Ok(_) => eprintln!("🎧 Client listening on random TCP port for DCUTR"),
                Err(e) => eprintln!("⚠️  Failed to start TCP listening: {:?}", e),
            }

            // Listen on random QUIC/UDP port (better for NAT traversal)
            let quic_addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1"
                .parse()
                .expect("Invalid QUIC listen address");
            match swarm.listen_on(quic_addr) {
                Ok(_) => eprintln!("🎧 Client listening on random QUIC port for DCUTR"),
                Err(e) => eprintln!("⚠️  Failed to start QUIC listening: {:?}", e),
            }

            // Parse the multiaddr - could be just a peer ID or a full multiaddr
            let addr_str = multiaddr.trim();

            // Check if it's just a peer ID (format: /p2p/PEER_ID)
            if addr_str.starts_with("/p2p/")
                && !addr_str.contains("/ip4/")
                && !addr_str.contains("/ip6/")
            {
                // Extract peer ID from /p2p/PEER_ID format
                let peer_id_str = addr_str.trim_start_matches("/p2p/");
                let target_peer = PeerId::from_str(peer_id_str).expect("Invalid peer ID");

                println!("🔌 Target peer: {}", target_peer);
                println!("🔄 Connecting to relay first, then will connect to peer...");
                conn_state.target_peer_id = Some(target_peer);
            } else {
                // It's a full multiaddr with IP - try to dial directly
                let remote_addr: Multiaddr = addr_str.parse().expect("Invalid multiaddr");

                println!("🔌 Connecting to {}", remote_addr);
                swarm.dial(remote_addr).expect("Failed to dial peer");
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
                    SwarmEvent::ConnectionEstablished { peer_id: peer, endpoint, connection_id, .. } => {
                        // Determine connection type by checking endpoint address
                        let endpoint_str = format!("{:?}", endpoint);
                        let is_relayed = endpoint_str.contains("p2p-circuit");
                        let is_quic = endpoint_str.contains("quic");
                        let conn_type = if is_relayed {
                            "relay circuit"
                        } else if is_quic {
                            "direct QUIC/UDP"
                        } else {
                            "direct TCP"
                        };

                        println!("✅ Connection established with {} via {}", peer, conn_type);

                        // Check if this is our relay server
                        if peer.to_string() == RELAY_PEER_ID {
                            println!("🎉 Connected to relay server");
                            conn_state.relay_connected = true;

                            // Listen on relay circuit to trigger reservation
                            let relay_listen_addr = format!("/ip4/143.198.15.158/udp/4001/quic-v1/p2p/{}/p2p-circuit", peer)
                                .parse::<Multiaddr>()
                                .expect("Invalid relay listen address");

                            if let Err(e) = swarm.listen_on(relay_listen_addr) {
                                eprintln!("⚠️  Failed to listen on relay: {:?}", e);
                            }
                        } else {
                            // This is our game opponent
                            if is_relayed {
                                println!("   ↳ Via relay - waiting for DCUTR hole punch...");

                                // Store relay connection ID and set DCUTR deadline
                                conn_state.game_peer_relay_connection = Some(connection_id);
                                conn_state.awaiting_dcutr = true;
                                conn_state.dcutr_deadline = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(5)
                                );

                                // DON'T notify game yet - wait for DCUTR to succeed
                            } else {
                                println!("   ↳ 🚀 Direct P2P connection established!");

                                // Direct connection established - store it and notify game
                                conn_state.game_peer_direct_connection = Some(connection_id);
                                conn_state.awaiting_dcutr = false;

                                peer_id = Some(peer);
                                connected.store(true, Ordering::Relaxed);
                                let _ = event_tx.send(NetworkEvent::Connected {
                                    peer_id: peer.to_string(),
                                });
                            }
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id: peer, cause, .. } => {
                        println!("❌ Connection closed with {}: {:?}", peer, cause);
                        connected.store(false, Ordering::Relaxed);
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        let addr_str = address.to_string();
                        if addr_str.contains("quic") {
                            println!("🎧 Listening on QUIC: {}", address);
                        } else if !addr_str.contains("p2p-circuit") {
                            println!("🎧 Listening on TCP: {}", address);
                        }
                    }
                    SwarmEvent::Dialing { peer_id, .. } => {
                        if let Some(peer) = peer_id {
                            eprintln!("📞 Dialing peer: {}", peer);
                        }
                    }
                    SwarmEvent::NewExternalAddrCandidate { address } => {
                        let addr_str = address.to_string();
                        if !addr_str.contains("p2p-circuit") {
                            let transport = if addr_str.contains("quic") { "QUIC" } else { "TCP" };
                            eprintln!("🌐 External address candidate ({}) for DCUTR: {}", transport, address);
                        }
                    }
                    SwarmEvent::ExternalAddrConfirmed { address } => {
                        let addr_str = address.to_string();
                        if !addr_str.contains("p2p-circuit") {
                            let transport = if addr_str.contains("quic") { "QUIC" } else { "TCP" };
                            eprintln!("✅ External address confirmed ({}) for DCUTR: {}", transport, address);
                        }
                    }
                    SwarmEvent::ExternalAddrExpired { address } => {
                        if !address.to_string().contains("p2p-circuit") {
                            eprintln!("⚠️  External address expired: {}", address);
                        }
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
                                eprintln!("📥 Received Gossipsub message from: {}", propagation_source);

                                // Ignore own messages
                                if propagation_source == local_peer_id {
                                    eprintln!("   ↳ Ignoring own message");
                                    continue;
                                }

                                // Deserialize network message
                                if let Ok(msg) = bincode::deserialize::<NetworkMessage>(&message.data) {
                                    eprintln!("   ↳ Message type: {:?}", msg);
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
                                } else {
                                    eprintln!("   ↳ Failed to deserialize message");
                                }
                            }
                            PongBehaviourEvent::Ping(_) => {
                                // Connection health check
                            }
                            PongBehaviourEvent::Identify(identify_event) => {
                                use libp2p::identify::Event as IdentifyEvent;

                                match identify_event {
                                    IdentifyEvent::Received { peer_id, info, .. } => {
                                        let is_relay_server = peer_id.to_string() == RELAY_PEER_ID;
                                        let addr_str = info.observed_addr.to_string();
                                        let is_relay_circuit = addr_str.contains("p2p-circuit");
                                        let is_real_ip = addr_str.contains("/ip4/") || addr_str.contains("/ip6/");

                                        if is_real_ip && !is_relay_circuit {
                                            let transport = if addr_str.contains("quic") { "QUIC" } else { "TCP" };
                                            eprintln!("🔍 Observed external address ({}) from {}: {}",
                                                transport,
                                                if is_relay_server { "relay" } else { "peer" },
                                                info.observed_addr
                                            );
                                        }

                                        // If this is from relay and we're waiting to connect
                                        if is_relay_server && !conn_state.external_address_discovered {
                                            conn_state.external_address_discovered = true;
                                            eprintln!("✅ External IP discovered");

                                            if conn_state.relay_reservation_ready {
                                                if let Some(target) = conn_state.target_peer_id.take() {
                                                    let relay_addr = format!("{}/p2p-circuit/p2p/{}", RELAY_ADDRESS, target)
                                                        .parse::<Multiaddr>()
                                                        .expect("Invalid relay circuit address");

                                                    println!("🚀 Connecting to peer via relay...");
                                                    match swarm.dial(relay_addr) {
                                                        Ok(_) => println!("⏳ DCUTR will attempt hole punch after relay connection"),
                                                        Err(e) => eprintln!("❌ Failed to dial through relay: {:?}", e),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    IdentifyEvent::Sent { .. } => {
                                        // Don't spam logs with every sent identify
                                    }
                                    IdentifyEvent::Pushed { .. } => {
                                        // Don't spam logs with pushed updates
                                    }
                                    IdentifyEvent::Error { peer_id, error, .. } => {
                                        eprintln!("⚠️  Identify: Error with {}: {:?}", peer_id, error);
                                    }
                                }
                            }
                            PongBehaviourEvent::RelayClient(relay_event) => {
                                use libp2p::relay::client::Event as RelayEvent;

                                // Check if we got a reservation
                                if matches!(relay_event, RelayEvent::ReservationReqAccepted { .. }) {
                                    println!("✨ Relay reservation ready");
                                    conn_state.relay_reservation_ready = true;

                                    // If we already discovered external IP, dial now
                                    if conn_state.external_address_discovered {
                                        if let Some(target) = conn_state.target_peer_id.take() {
                                            let relay_addr = format!("{}/p2p-circuit/p2p/{}", RELAY_ADDRESS, target)
                                                .parse::<Multiaddr>()
                                                .expect("Invalid relay circuit address");

                                            println!("🚀 Connecting to peer via relay...");
                                            match swarm.dial(relay_addr) {
                                                Ok(_) => println!("⏳ DCUTR will attempt hole punch after relay connection"),
                                                Err(e) => eprintln!("❌ Failed to dial: {:?}", e),
                                            }
                                        }
                                    } else if conn_state.target_peer_id.is_some() {
                                        println!("⏳ Waiting to discover external IP...");
                                    }
                                }
                            }
                            PongBehaviourEvent::Dcutr(dcutr_event) => {
                                use libp2p::dcutr::Event as DcutrEvent;

                                match dcutr_event.result {
                                    Ok(direct_connection_id) => {
                                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                                        eprintln!("🎯 DCUTR SUCCESS! Direct P2P connection established");
                                        eprintln!("   Peer: {}", dcutr_event.remote_peer_id);

                                        conn_state.game_peer_direct_connection = Some(direct_connection_id);
                                        conn_state.awaiting_dcutr = false;
                                        conn_state.dcutr_deadline = None;

                                        // Close relay connection
                                        if let Some(relay_conn_id) = conn_state.game_peer_relay_connection.take() {
                                            swarm.close_connection(relay_conn_id);
                                        }

                                        // Add peer to Gossipsub mesh
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&dcutr_event.remote_peer_id);
                                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                                        // Notify game to start
                                        peer_id = Some(dcutr_event.remote_peer_id);
                                        connected.store(true, Ordering::Relaxed);
                                        let _ = event_tx.send(NetworkEvent::Connected {
                                            peer_id: dcutr_event.remote_peer_id.to_string(),
                                        });
                                    }
                                    Err(err) => {
                                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                                        eprintln!("❌ DCUTR FAILED: {:?}", err);
                                        eprintln!("   Peer: {}", dcutr_event.remote_peer_id);
                                        eprintln!("   Direct P2P connection impossible - disconnecting");
                                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                                        // Close relay connection
                                        if let Some(relay_conn_id) = conn_state.game_peer_relay_connection.take() {
                                            swarm.close_connection(relay_conn_id);
                                        }

                                        conn_state.awaiting_dcutr = false;
                                        conn_state.dcutr_deadline = None;

                                        let _ = event_tx.send(NetworkEvent::Error(format!(
                                            "Direct P2P connection failed: {:?}\n\
                                             NAT type prevents peer-to-peer gameplay.",
                                            err
                                        )));
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
                            PongBehaviourEvent::Upnp(upnp_event) => {
                                use libp2p::upnp::Event as UpnpEvent;

                                match upnp_event {
                                    UpnpEvent::NewExternalAddr(addr) => {
                                        println!("🔓 UPnP: Port forwarding established!");
                                        println!("   ↳ External address: {}", addr);
                                        println!("   ↳ Direct connections should now work");
                                    }
                                    UpnpEvent::GatewayNotFound => {
                                        eprintln!("⚠️  UPnP: No gateway found (not on local network or no UPnP support)");
                                    }
                                    UpnpEvent::NonRoutableGateway => {
                                        eprintln!("⚠️  UPnP: Gateway is not routable (double NAT scenario)");
                                    }
                                    UpnpEvent::ExpiredExternalAddr(addr) => {
                                        eprintln!("⚠️  UPnP: Port forwarding expired: {}", addr);
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
                            if pid.to_string() == RELAY_PEER_ID {
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
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                // Check DCUTR timeout
                if let Some(deadline) = conn_state.dcutr_deadline {
                    if tokio::time::Instant::now() > deadline && conn_state.awaiting_dcutr {
                        eprintln!();
                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!("⏰ DCUTR TIMEOUT after 5 seconds");
                        eprintln!("   No direct connection established");
                        eprintln!("   DCUTR event never fired - possible network issue");
                        eprintln!("");
                        eprintln!("   ❌ DISCONNECTING - Direct connection required");
                        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        eprintln!();

                        if let Some(relay_conn_id) = conn_state.game_peer_relay_connection.take() {
                            swarm.close_connection(relay_conn_id);
                        }

                        conn_state.dcutr_deadline = None;
                        conn_state.awaiting_dcutr = false;

                        let _ = event_tx.send(NetworkEvent::Error(
                            "Connection timeout: DCUTR did not complete within 5 seconds.\n\
                             Your network may not support hole punching.".to_string()
                        ));
                    }
                }

                // Check for commands
                if let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        NetworkCommand::SendInput(action) => {
                            let msg = NetworkMessage::Input(action);
                            let bytes = bincode::serialize(&msg)
                                .expect("Failed to serialize input");

                            match swarm.behaviour_mut().gossipsub.publish(
                                game_topic.clone(),
                                bytes
                            ) {
                                Ok(msg_id) => {
                                    eprintln!("📤 Published input to Gossipsub (msg_id: {:?})", msg_id);
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to publish input: {:?}", e);
                                }
                            }
                        }
                        NetworkCommand::SendMessage(msg) => {
                            let bytes = bincode::serialize(&msg)
                                .expect("Failed to serialize message");

                            match swarm.behaviour_mut().gossipsub.publish(
                                game_topic.clone(),
                                bytes
                            ) {
                                Ok(msg_id) => {
                                    eprintln!("📤 Published message to Gossipsub (msg_id: {:?})", msg_id);
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to publish message: {:?}", e);
                                }
                            }
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
