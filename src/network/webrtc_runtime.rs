// WebRTC network runtime using str0m Sans-I/O implementation
// Manages P2P connections via WebRTC with explicit I/O control

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::net::{UdpSocket, SocketAddr, IpAddr};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use str0m::{Rtc, Event, Input, Output, IceConnectionState, Candidate};
use str0m::net::{Protocol, Receive};
use str0m::channel::{ChannelId, ChannelConfig, Reliability};
use str0m::change::{SdpOffer, SdpAnswer};

use super::{
    client::{ConnectionMode, NetworkCommand, NetworkEvent},
    protocol::NetworkMessage,
};

// Signaling server address
const SIGNALING_SERVER: &str = "ws://143.198.15.158:8080";

// STUN server for NAT traversal (Cloudflare public STUN server)
const STUN_SERVER: &str = "stun.cloudflare.com:3478";

/// Log diagnostic info to file
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

/// Generate a short, human-friendly peer ID (4 uppercase letters)
fn generate_short_peer_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..4)
        .map(|_| {
            let idx = rng.gen_range(0..26);
            (b'A' + idx) as char
        })
        .collect()
}

/// Discover the local network IP address for LAN connectivity
/// This is critical for ICE to work on the same network!
///
/// When VPN is active, prefers the physical interface (WiFi/Ethernet) over VPN interface
/// to enable P2P connections between peers on the same LAN.
async fn discover_local_ip() -> Result<IpAddr> {
    log_to_file("LOCAL_IP_DISCOVERY", "Starting local IP discovery");

    // Get all network interfaces
    let interfaces = if_addrs::get_if_addrs()
        .map_err(|e| anyhow!("Failed to get network interfaces: {}", e))?;

    log_to_file("INTERFACES_FOUND", &format!("Found {} interfaces", interfaces.len()));

    // Filter to IPv4 addresses only and exclude loopback
    let mut candidates: Vec<(String, std::net::IpAddr)> = interfaces
        .into_iter()
        .filter_map(|iface| {
            if let std::net::IpAddr::V4(ipv4) = iface.addr.ip() {
                if !ipv4.is_loopback() {
                    Some((iface.name, std::net::IpAddr::V4(ipv4)))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    log_to_file("IPV4_CANDIDATES", &format!("Found {} IPv4 candidates: {:?}",
        candidates.len(),
        candidates.iter().map(|(name, ip)| format!("{}={}", name, ip)).collect::<Vec<_>>()
    ));

    if candidates.is_empty() {
        return Err(anyhow!("No suitable network interfaces found"));
    }

    // Detect if we have both VPN and physical interface
    let has_vpn = candidates.iter().any(|(_, ip)| {
        if let std::net::IpAddr::V4(ipv4) = ip {
            ipv4.octets()[0] == 10
        } else {
            false
        }
    });

    let has_home_network = candidates.iter().any(|(_, ip)| {
        if let std::net::IpAddr::V4(ipv4) = ip {
            let octets = ipv4.octets();
            octets[0] == 192 && octets[1] == 168
        } else {
            false
        }
    });

    log_to_file("VPN_DETECTION", &format!("has_vpn={}, has_home_network={}", has_vpn, has_home_network));

    // Scoring function:
    // - If VPN active: PREFER VPN for STUN to work (allows NAT traversal)
    // - If no VPN: prefer home network (192.168.x.x) for local P2P
    candidates.sort_by_key(|(name, ip)| {
        if let std::net::IpAddr::V4(ipv4) = ip {
            let octets = ipv4.octets();

            // VPN interface (10.x.x.x)
            if octets[0] == 10 {
                // If we also have a home network interface, VPN is likely active
                // Prefer VPN when both exist to enable STUN/NAT traversal
                let score = if has_home_network { 0 } else { 2 };
                log_to_file("SCORING", &format!("{} ({}) = {} (VPN)", name, ip, score));
                return score;
            }

            // Home network (192.168.x.x)
            if octets[0] == 192 && octets[1] == 168 {
                // Prefer home network only if no VPN (for local P2P)
                let score = if has_vpn { 1 } else { 0 };
                log_to_file("SCORING", &format!("{} ({}) = {} (home network)", name, ip, score));
                return score;
            }

            // Corporate network (172.16-31.x.x)
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                log_to_file("SCORING", &format!("{} ({}) = 1 (corporate)", name, ip));
                return 1;
            }

            // Other private IPs
            log_to_file("SCORING", &format!("{} ({}) = 3 (other)", name, ip));
            3
        } else {
            log_to_file("SCORING", &format!("{} ({}) = 99 (non-IPv4)", name, ip));
            99
        }
    });

    let (selected_name, selected_ip) = &candidates[0];
    log_to_file("SELECTED_INTERFACE", &format!("Selected {} with IP {}", selected_name, selected_ip));

    Ok(*selected_ip)
}

/// Query STUN server to discover public IP address and port
/// CRITICAL: Must use the same socket that will be used for ICE, otherwise
/// the NAT port mapping will be different and peers won't be able to connect!
async fn query_stun_server(udp_socket: &UdpSocket, stun_server: &str) -> Result<SocketAddr> {
    log_to_file("STUN_RESOLVE", &format!("Resolving STUN server: {}", stun_server));

    // Parse STUN server address - prefer IPv4 for compatibility
    let stun_addr = tokio::net::lookup_host(stun_server)
        .await?
        .find(|addr| addr.is_ipv4())
        .ok_or_else(|| anyhow!("Failed to resolve STUN server to IPv4 address"))?;

    log_to_file("STUN_RESOLVED", &format!("STUN server resolved to: {}", stun_addr));

    // DIAGNOSTIC: Log socket state before cloning
    let original_addr = udp_socket.local_addr()?;
    log_to_file("STUN_CLONE_BEFORE", &format!("Original socket: {}", original_addr));

    // Clone the socket for use in blocking task
    // We MUST use the same socket that will be used for ICE!
    let socket_clone = udp_socket.try_clone()?;

    // DIAGNOSTIC: Verify clone has same address
    let clone_addr = socket_clone.local_addr()?;
    log_to_file("STUN_CLONE_AFTER", &format!("Cloned socket: {}", clone_addr));

    if original_addr != clone_addr {
        log_to_file("STUN_CLONE_MISMATCH", &format!(
            "ERROR: Clone has different address! Original={}, Clone={}",
            original_addr, clone_addr
        ));
    }

    log_to_file("STUN_BINDING_REQUEST", "Sending STUN binding request on ICE socket");

    let client = stunclient::StunClient::new(stun_addr);
    let public_addr = tokio::task::spawn_blocking(move || -> Result<SocketAddr, Box<dyn std::error::Error + Send + Sync>> {
        // DIAGNOSTIC: Log socket state in blocking task
        let addr_in_task = socket_clone.local_addr()?;
        log_to_file("STUN_IN_BLOCKING_TASK", &format!("Socket in blocking task: {}", addr_in_task));

        // Temporarily set timeout for STUN query
        socket_clone.set_read_timeout(Some(Duration::from_secs(5)))?;
        log_to_file("STUN_TIMEOUT_SET", "Read timeout set to 5 seconds");

        let result = client.query_external_address(&socket_clone)?;
        log_to_file("STUN_QUERY_COMPLETE", &format!("STUN returned: {}", result));

        // Reset to non-blocking for ICE
        socket_clone.set_nonblocking(false)?;
        log_to_file("STUN_NONBLOCKING_RESET", "Socket reset to blocking mode");

        Ok(result)
    })
    .await?
    .map_err(|e| anyhow!("STUN query failed: {}", e))?;

    log_to_file("STUN_RESPONSE", &format!("Received public address: {}", public_addr));
    Ok(public_addr)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SignalingMessage {
    Register {
        peer_id: String,
    },
    RegisterOk {
        peer_id: String,
    },
    ListPeers,
    PeerList {
        peers: Vec<String>,
    },
    Offer {
        target: String,
        from: String,
        sdp: String,
    },
    Answer {
        target: String,
        from: String,
        sdp: String,
    },
    IceCandidate {
        target: String,
        from: String,
        candidate: String,
    },
    Error {
        message: String,
    },
}

/// Initialize and run the WebRTC network in a background thread
pub fn spawn_network_thread(
    mode: ConnectionMode,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
    signaling_server: String,
) -> std::io::Result<()> {
    thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            log_to_file("THREAD_SPAWN", "Network thread started");

            // Create minimal Tokio runtime only for signaling phase
            let rt = Runtime::new().expect("Failed to create tokio runtime");
            log_to_file("THREAD_RUNTIME", "Tokio runtime created");

            let result = rt.block_on(async {
                log_to_file("THREAD_ASYNC_START", "Entering async block");

                match setup_signaling_and_sdp(mode.clone(), &event_tx, &signaling_server).await {
                    Ok((rtc, udp_socket, channel_id)) => {
                        log_to_file("SETUP_COMPLETE", "Signaling and SDP setup complete");
                        Ok((rtc, udp_socket, channel_id))
                    }
                    Err(e) => {
                        error!("Setup error: {}", e);
                        log_to_file("SETUP_ERROR", &format!("Setup error: {}", e));
                        Err(e)
                    }
                }
            });

            // Drop Tokio runtime - no longer needed
            drop(rt);

            match result {
                Ok((rtc, udp_socket, channel_id)) => {
                    log_to_file("POLLING_START", "Starting str0m polling loop");

                    if let Err(e) = run_str0m_loop(rtc, udp_socket, channel_id, event_tx, cmd_rx, connected) {
                        error!("Network loop error: {}", e);
                        log_to_file("LOOP_ERROR", &format!("Network loop error: {}", e));
                    }
                }
                Err(e) => {
                    error!("Network setup failed: {}", e);
                    log_to_file("SETUP_FAILED", &format!("Setup failed: {}", e));
                    // Send error event to UI so user sees the error message
                    let _ = event_tx.send(NetworkEvent::Error(e.to_string()));
                }
            }

            log_to_file("THREAD_END", "Network thread ending")
        })).unwrap_or_else(|_| {
            log_to_file("THREAD_PANIC", "PANIC in network thread!");
        });
    });

    Ok(())
}

/// Setup signaling and SDP exchange, returns configured Rtc, UDP socket, and optional channel ID
/// Client mode returns the channel_id from add_channel(), host mode returns None (channel comes from Event::ChannelOpen)
async fn setup_signaling_and_sdp(
    mode: ConnectionMode,
    event_tx: &mpsc::Sender<NetworkEvent>,
    signaling_server: &str,
) -> Result<(Rtc, UdpSocket, Option<ChannelId>)> {
    log_to_file("SETUP_START", "setup_signaling_and_sdp() started");

    // Generate a unique peer ID (4 uppercase letters)
    let peer_id = generate_short_peer_id();
    info!("Local peer ID: {}", peer_id);
    log_to_file("SETUP_PEER_ID", &peer_id);

    // Connect to signaling server
    log_to_file("SETUP_CONNECT", &format!("Connecting to signaling server: {}", signaling_server));
    let (ws_stream, _) = connect_async(signaling_server).await?;
    info!("Connected to signaling server: {}", signaling_server);
    log_to_file("SETUP_CONNECTED", &format!("Connected to signaling server: {}", signaling_server));

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Register with signaling server
    log_to_file("SETUP_REGISTER", "Sending registration message");
    let register_msg = SignalingMessage::Register {
        peer_id: peer_id.clone(),
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&register_msg)?))
        .await?;
    log_to_file("SETUP_REGISTER_SENT", "Registration message sent");

    // Wait for registration confirmation
    log_to_file("SETUP_WAIT_REGISTER", "Waiting for registration confirmation");
    if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let msg: SignalingMessage = serde_json::from_str(&text)?;
        log_to_file("SETUP_REGISTER_OK", "Registration confirmed");
        match msg {
            SignalingMessage::RegisterOk { .. } => {
                info!("✅ Registered with signaling server");
            }
            _ => {
                return Err(anyhow!("Unexpected registration response"));
            }
        }
    }

    // Create str0m Rtc instance
    log_to_file("SETUP_WEBRTC", "Creating str0m Rtc instance");
    let mut rtc = Rtc::builder()
        .set_rtp_mode(false)  // Data channels only, no RTP media
        .build();
    info!("Created str0m Rtc instance");
    log_to_file("SETUP_WEBRTC_CREATED", "Rtc instance created");

    // Discover local network IP FIRST
    // This selects the preferred interface (WiFi over VPN when both available)
    let local_ip = discover_local_ip().await.unwrap_or_else(|_| {
        log_to_file("LOCAL_IP_FALLBACK", "Failed to discover local IP, using 127.0.0.1");
        "127.0.0.1".parse().unwrap()
    });
    log_to_file("LOCAL_IP_DISCOVERED", &format!("Primary local network IP: {}", local_ip));

    // Bind UDP socket to SPECIFIC local IP (not 0.0.0.0)
    // This is critical so that udp_socket.local_addr() returns the actual IP,
    // which str0m needs to match received packets against local candidates!
    let bind_addr = SocketAddr::new(local_ip, 0);  // Port 0 = let OS choose
    let udp_socket = UdpSocket::bind(bind_addr)?;
    udp_socket.set_nonblocking(false)?;
    let host_addr = udp_socket.local_addr()?;
    info!("Bound UDP socket: {}", host_addr);
    log_to_file("SETUP_UDP", &format!("UDP socket bound to {}", host_addr));

    // Add primary host candidate
    let local_cand = Candidate::host(host_addr, "udp")
        .map_err(|e| anyhow!("Failed to create local candidate: {}", e))?;
    let _local_candidate = rtc.add_local_candidate(local_cand)
        .ok_or_else(|| anyhow!("Failed to add local candidate to Rtc"))?;
    info!("Added host ICE candidate: {}", host_addr);
    log_to_file("SETUP_LOCAL_CANDIDATE", &format!("Host candidate added: {}", host_addr));

    // Query STUN server to get public IP/port for NAT traversal
    // DIAGNOSTIC: Log socket state before STUN query
    let socket_before_stun = udp_socket.local_addr()?;
    log_to_file("STUN_SOCKET_BEFORE", &format!("Socket state before STUN: {}", socket_before_stun));

    log_to_file("STUN_QUERY_START", &format!("Querying STUN server: {}", STUN_SERVER));

    // If STUN fails on the primary interface (e.g., WiFi while VPN is active),
    // and we have a VPN interface, try STUN through VPN interface instead
    let stun_result = query_stun_server(&udp_socket, STUN_SERVER).await;

    match stun_result {
        Ok(public_addr) => {
            // DIAGNOSTIC: Log socket state after STUN query
            let socket_after_stun = udp_socket.local_addr()?;
            log_to_file("STUN_SOCKET_AFTER", &format!("Socket state after STUN: {}", socket_after_stun));

            // DIAGNOSTIC: Check if port changed
            if socket_before_stun.port() != socket_after_stun.port() {
                log_to_file("STUN_PORT_CHANGED", &format!(
                    "WARNING: Socket port changed! Before={}, After={}",
                    socket_before_stun.port(),
                    socket_after_stun.port()
                ));
            }

            // DIAGNOSTIC: Compare socket port to STUN reported port
            let port_mismatch = socket_after_stun.port() != public_addr.port();
            log_to_file("STUN_PORT_ANALYSIS", &format!(
                "Socket port={}, STUN public port={}, Mismatch={}",
                socket_after_stun.port(),
                public_addr.port(),
                port_mismatch
            ));

            info!("🌐 Public address from STUN: {}", public_addr);
            log_to_file("STUN_PUBLIC_ADDR", &format!("Public address: {}", public_addr));

            // Add server reflexive candidate (public IP from STUN)
            // DIAGNOSTIC: Log the parameters we're passing to server_reflexive
            log_to_file("SRFLX_CREATE_PARAMS", &format!(
                "Creating srflx candidate: public_addr={}, base_addr={}",
                public_addr, host_addr
            ));
            match Candidate::server_reflexive(public_addr, host_addr, "udp") {
                Ok(srflx_cand) => {
                    if let Some(_) = rtc.add_local_candidate(srflx_cand) {
                        info!("Added server reflexive ICE candidate: {}", public_addr);
                        log_to_file("SETUP_SRFLX_CANDIDATE", &format!("Server reflexive candidate added: {}", public_addr));
                    } else {
                        warn!("Failed to add server reflexive candidate");
                        log_to_file("STUN_ADD_FAILED", "Failed to add srflx candidate to rtc");
                    }
                }
                Err(e) => {
                    warn!("Failed to create server reflexive candidate: {}", e);
                    log_to_file("STUN_CANDIDATE_ERROR", &format!("Failed to create srflx candidate: {}", e));
                }
            }
        }
        Err(e) => {
            warn!("Failed to query STUN server: {}", e);
            log_to_file("STUN_QUERY_FAILED", &format!("STUN query failed: {}, using host candidate only", e));
            // Continue with just host candidate - localhost connections will still work
        }
    }

    // Handle based on connection mode
    log_to_file("SETUP_MODE_SELECT", &format!("Connection mode: {:?}", mode));
    let channel_id = match mode {
        ConnectionMode::Listen { .. } => {
            log_to_file("SETUP_HOST_MODE", &format!("Entering host mode, peer_id: {}", peer_id));
            info!("🎮 Host mode: waiting for client connection...");

            // Send local peer ID to display in TUI
            let _ = event_tx.send(NetworkEvent::LocalPeerIdReady {
                peer_id: peer_id.clone(),
            });

            handle_host_mode(
                &mut rtc,
                &mut ws_sink,
                &mut ws_stream,
                &peer_id,
                event_tx,
            )
            .await?
        }
        ConnectionMode::Connect { multiaddr } => {
            let target_peer = multiaddr;
            log_to_file("SETUP_CLIENT_MODE", &format!("Connecting to peer: {}", target_peer));
            info!("🔌 Client mode: connecting to {}...", target_peer);

            handle_client_mode(
                &mut rtc,
                &mut ws_sink,
                &mut ws_stream,
                &peer_id,
                event_tx,
                target_peer.clone(),
            )
            .await?
        }
    };

    log_to_file("SETUP_COMPLETE", "SDP and ICE exchange complete");
    // For client mode, channel_id is Some(id). For host mode, it's None.
    Ok((rtc, udp_socket, channel_id.into()))
}

/// Host mode: wait for offer from client
async fn handle_host_mode(
    rtc: &mut Rtc,
    ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    peer_id: &str,
    _event_tx: &mpsc::Sender<NetworkEvent>,
) -> Result<Option<ChannelId>> {
    log_to_file("HOST_MODE", "handle_host_mode() started");

    // Wait for offer from client
    let (offer_sdp, remote_peer_id) = loop {
        if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
            let msg: SignalingMessage = serde_json::from_str(&text)?;

            match msg {
                SignalingMessage::Offer { from, sdp, .. } => {
                    info!("📥 Received offer from {}", from);
                    log_to_file("HOST_OFFER", &format!("Received offer from {}", from));
                    // Debug: log full received offer SDP
                    let offer_candidate_count = sdp.lines().filter(|l| l.starts_with("a=candidate:")).count();
                    log_to_file("SDP_OFFER_FULL", &sdp);
                    log_to_file("SDP_OFFER_CANDIDATES", &format!("Offer has {} ICE candidates", offer_candidate_count));
                    break (sdp, from);  // Capture the remote peer ID for the answer
                }
                _ => {}
            }
        } else {
            return Err(anyhow!("WebSocket closed while waiting for offer"));
        }
    };

    // Accept offer and create answer
    log_to_file("HOST_ACCEPT_OFFER", "Accepting offer from client");
    let offer = SdpOffer::from_sdp_string(&offer_sdp)?;
    let answer = rtc.sdp_api().accept_offer(offer)?;
    info!("📤 Sending answer");
    log_to_file("HOST_ANSWER", "Answer created");

    // Debug: log full SDP and check for candidates
    let answer_sdp = answer.to_sdp_string();
    let answer_candidate_count = answer_sdp.lines().filter(|l| l.starts_with("a=candidate:")).count();
    log_to_file("SDP_ANSWER_FULL", &answer_sdp);
    log_to_file("SDP_ANSWER_CANDIDATES", &format!("Answer has {} ICE candidates", answer_candidate_count));

    // Send answer back to the remote peer that sent the offer
    let answer_msg = SignalingMessage::Answer {
        target: remote_peer_id.clone(),  // Send to the actual peer ID, not "remote"
        from: peer_id.to_string(),
        sdp: answer_sdp,
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&answer_msg)?))
        .await?;
    log_to_file("HOST_ANSWER_SENT", &format!("Answer sent to {}", remote_peer_id));

    // ICE candidates are embedded in SDP (str0m v0.14.x behavior)
    log_to_file("HOST_SDP_COMPLETE", "SDP exchange complete");

    // Properly close WebSocket connection after signaling completes
    info!("Closing signaling connection");
    let _ = ws_sink.close().await;

    // In host mode, the channel_id comes from Event::ChannelOpen when remote opens it
    Ok(None)
}

/// Client mode: create offer and send to host
async fn handle_client_mode(
    rtc: &mut Rtc,
    ws_sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    ws_stream: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    peer_id: &str,
    _event_tx: &mpsc::Sender<NetworkEvent>,
    target_peer: String,
) -> Result<Option<ChannelId>> {
    log_to_file("CLIENT_MODE", "handle_client_mode() started");

    // Create data channel and offer
    let mut change = rtc.sdp_api();
    let channel_id = change.add_channel_with_config(ChannelConfig {
        label: "pong".to_string(),
        ordered: false,  // Allow out-of-order delivery
        reliability: Reliability::MaxRetransmits { retransmits: 3 },
        negotiated: None,
        protocol: String::new(),
    });
    let (offer, pending) = change.apply()
        .ok_or_else(|| anyhow!("Failed to apply SDP changes"))?;

    info!("📨 Created data channel");
    log_to_file("CLIENT_CHANNEL", &format!("Data channel created: {:?}", channel_id));

    // Debug: log full SDP and check for candidates
    let offer_sdp = offer.to_sdp_string();
    let candidate_count = offer_sdp.lines().filter(|l| l.starts_with("a=candidate:")).count();
    log_to_file("SDP_OFFER_FULL", &offer_sdp);
    log_to_file("SDP_OFFER_CANDIDATES", &format!("Offer has {} ICE candidates", candidate_count));

    // Send offer to target
    let offer_msg = SignalingMessage::Offer {
        target: target_peer.clone(),
        from: peer_id.to_string(),
        sdp: offer_sdp,
    };
    ws_sink
        .send(Message::Text(serde_json::to_string(&offer_msg)?))
        .await?;
    info!("📤 Sent offer to {}", target_peer);
    log_to_file("CLIENT_OFFER_SENT", &format!("Offer sent to {}", target_peer));

    // Wait for answer and apply it
    let answer_sdp = loop {
        if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
            let msg: SignalingMessage = serde_json::from_str(&text)?;

            match msg {
                SignalingMessage::Answer { sdp, .. } => {
                    info!("📥 Received answer");
                    log_to_file("CLIENT_ANSWER", "Received answer from host");
                    // Debug: log full received answer SDP
                    let answer_candidate_count = sdp.lines().filter(|l| l.starts_with("a=candidate:")).count();
                    log_to_file("SDP_ANSWER_FULL", &sdp);
                    log_to_file("SDP_ANSWER_CANDIDATES", &format!("Answer has {} ICE candidates", answer_candidate_count));
                    break sdp;
                }
                SignalingMessage::Error { message } => {
                    log_to_file("CLIENT_ERROR", &format!("Server error: {}", message));
                    return Err(anyhow!("Connection failed: {}", message));
                }
                _ => {}
            }
        } else {
            return Err(anyhow!("WebSocket closed while waiting for answer"));
        }
    };

    // Apply the answer to complete the SDP negotiation
    // CRITICAL: This completes the WebRTC session setup
    log_to_file("CLIENT_APPLY_ANSWER", "Applying SDP answer");

    // Parse the answer string into an SdpAnswer
    let answer_obj = SdpAnswer::from_sdp_string(&answer_sdp)
        .map_err(|e| anyhow!("Failed to parse answer SDP: {}", e))?;

    // Complete the SDP negotiation by accepting the answer
    rtc.sdp_api()
        .accept_answer(pending, answer_obj)
        .map_err(|e| anyhow!("Failed to accept answer: {}", e))?;

    info!("✅ SDP negotiation complete");
    log_to_file("CLIENT_ANSWER_APPLIED", "SDP answer accepted, session setup complete");

    // ICE candidates are embedded in SDP (str0m v0.14.x behavior)
    log_to_file("CLIENT_SDP_COMPLETE", "SDP exchange complete");

    // Properly close WebSocket connection after signaling completes
    info!("Closing signaling connection");
    let _ = ws_sink.close().await;

    Ok(Some(channel_id))
}

/// Main synchronous polling loop for str0m
fn run_str0m_loop(
    mut rtc: Rtc,
    udp_socket: UdpSocket,
    initial_channel_id: Option<ChannelId>,
    event_tx: mpsc::Sender<NetworkEvent>,
    cmd_rx: mpsc::Receiver<NetworkCommand>,
    connected: Arc<AtomicBool>,
) -> Result<()> {
    log_to_file("POLLING_LOOP", "Starting main polling loop");
    info!("🔄 Starting WebRTC polling loop");

    let mut buf = vec![0u8; 8192];
    // Client mode provides the channel_id from setup; host mode gets it from Event::ChannelOpen
    let mut active_channel_id: Option<ChannelId> = initial_channel_id;

    // Track str0m's requested deadline separately from socket timeout
    // str0m needs to be notified at its requested deadline for ICE keepalives
    let mut str0m_deadline: Option<Instant> = None;

    loop {
        // Phase 1: Poll str0m for outputs
        loop {
            match rtc.poll_output()? {
                Output::Transmit(transmit) => {
                    // Send UDP packet to remote peer
                    match udp_socket.send_to(&transmit.contents, transmit.destination) {
                        Ok(_) => {
                            // DIAGNOSTIC: Log source and destination for send
                            let local = udp_socket.local_addr().unwrap_or_else(|_| "unknown".parse().unwrap());
                            log_to_file(
                                "UDP_SEND",
                                &format!("Sent {} bytes: {}→{}",
                                    transmit.contents.len(),
                                    local,
                                    transmit.destination
                                ),
                            );
                        }
                        Err(e) => {
                            warn!("Failed to send UDP packet: {}", e);
                            log_to_file("UDP_SEND_ERROR", &format!("Failed to send: {}", e));
                        }
                    }
                }
                Output::Timeout(deadline) => {
                    // str0m says we should wait until deadline for next event
                    // IMPORTANT: Save the deadline - we MUST notify str0m when we reach it
                    // for ICE keepalives to work! But use short socket timeout to drain commands.
                    str0m_deadline = Some(deadline);

                    // Use short socket timeout (10ms) so we can drain command channel frequently
                    // This prevents the 2-5 second delays we saw earlier
                    udp_socket.set_read_timeout(Some(Duration::from_millis(10)))?;
                    break; // Exit poll loop to wait for input
                }
                Output::Event(event) => {
                    // Process str0m event
                    handle_str0m_event(
                        event,
                        &event_tx,
                        &connected,
                        &mut active_channel_id,
                    )?;
                }
            }
        }

        // Phase 2: Wait for UDP input or timeout
        match udp_socket.recv_from(&mut buf) {
            Ok((n, source)) => {
                // Received UDP packet - pass to str0m
                log_to_file("UDP_RECV", &format!("Received {} bytes from {}", n, source));
                let receive = Receive {
                    proto: Protocol::Udp,
                    source,
                    destination: udp_socket.local_addr()?,
                    contents: buf[..n].try_into()?,
                };
                rtc.handle_input(Input::Receive(Instant::now(), receive))?;

                // Clear deadline - str0m will set a new one after processing this packet
                str0m_deadline = None;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {
                // Socket timeout (10ms) - only notify str0m if we've reached its deadline
                let now = Instant::now();

                // Check if we've reached str0m's requested deadline
                if let Some(deadline) = str0m_deadline {
                    if now >= deadline {
                        // Reached the deadline str0m requested - notify it
                        rtc.handle_input(Input::Timeout(now))?;
                        str0m_deadline = None; // Clear deadline after notifying
                        log_to_file("STR0M_DEADLINE_REACHED", "Notified str0m of deadline");
                    }
                    // else: Not yet at deadline, just continue to drain commands
                } else {
                    // No deadline set, notify anyway (shouldn't happen in normal operation)
                    rtc.handle_input(Input::Timeout(now))?;
                }
            }
            Err(e) => {
                error!("UDP socket error: {}", e);
                log_to_file("UDP_ERROR", &format!("Socket error: {}", e));
                return Err(e.into());
            }
        }

        // Phase 3: Process commands from game loop (non-blocking)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                NetworkCommand::SendInput(action) => {
                    if let Some(cid) = active_channel_id {
                        let msg = NetworkMessage::Input(action);
                        if let Ok(bytes) = msg.to_bytes() {
                            if let Some(mut channel) = rtc.channel(cid) {
                                match channel.write(true, &bytes) {
                                    Ok(_) => {
                                        log_to_file("SEND_INPUT", &format!("Input sent, {} bytes", bytes.len()));
                                    }
                                    Err(e) => {
                                        warn!("Failed to send input: {}", e);
                                        log_to_file("SEND_INPUT_ERROR", &format!("Send error: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
                NetworkCommand::SendMessage(msg) => {
                    if let Some(cid) = active_channel_id {
                        if let Ok(bytes) = msg.to_bytes() {
                            if let Some(mut channel) = rtc.channel(cid) {
                                // Log sequence for BallSync to track delivery
                                if let NetworkMessage::BallSync(ref state) = msg {
                                    log_to_file("SEND_BALLSYNC", &format!("Attempting send seq={}, {} bytes", state.sequence, bytes.len()));
                                }

                                match channel.write(true, &bytes) {
                                    Ok(_) => {
                                        if let NetworkMessage::BallSync(ref state) = msg {
                                            log_to_file("SEND_BALLSYNC_OK", &format!("channel.write OK seq={}", state.sequence));
                                        } else {
                                            log_to_file("SEND_MESSAGE", &format!("Message sent, {} bytes", bytes.len()));
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to send message: {}", e);
                                        log_to_file("SEND_MESSAGE_ERROR", &format!("Send error: {}", e));
                                    }
                                }
                            }
                        }
                    }
                }
                NetworkCommand::Disconnect => {
                    log_to_file("DISCONNECT", "Disconnect command received");
                    return Ok(());
                }
            }
        }
    }
}

/// Handle events from str0m
fn handle_str0m_event(
    event: Event,
    event_tx: &mpsc::Sender<NetworkEvent>,
    connected: &Arc<AtomicBool>,
    active_channel_id: &mut Option<ChannelId>,
) -> Result<()> {
    match event {
        Event::Connected => {
            // ICE + DTLS are both ready - connection is fully established
            info!("🔗 WebRTC connection fully established (ICE + DTLS)");
            log_to_file("CONNECTED", "Full connection established");
            connected.store(true, Ordering::Relaxed);
            let _ = event_tx.send(NetworkEvent::Connected {
                peer_id: "remote".to_string(),
            });
        }
        Event::IceConnectionStateChange(state) => {
            // Log all ICE state changes for debugging
            log_to_file("ICE_STATE", &format!("ICE state: {:?}", state));
            match state {
                IceConnectionState::New => {
                    log_to_file("ICE_STATE_NEW", "ICE gathering addresses");
                }
                IceConnectionState::Checking => {
                    log_to_file("ICE_STATE_CHECKING", "ICE checking candidate pairs");
                }
                IceConnectionState::Connected => {
                    log_to_file("ICE_STATE_CONNECTED", "ICE found working pair");
                    // Note: Event::Connected is the actual connection signal, not this
                }
                IceConnectionState::Completed => {
                    log_to_file("ICE_STATE_COMPLETED", "ICE completed all checks");
                }
                IceConnectionState::Disconnected => {
                    info!("❌ ICE connection disconnected");
                    log_to_file("ICE_STATE_DISCONNECTED", "ICE connection lost");
                    connected.store(false, Ordering::Relaxed);
                    let _ = event_tx.send(NetworkEvent::Disconnected);
                }
            }
        }
        Event::ChannelOpen(cid, label) => {
            info!("📨 Data channel opened: {}", label);
            log_to_file("CHANNEL_OPEN", &format!("Data channel opened: {}", label));
            *active_channel_id = Some(cid);
            let _ = event_tx.send(NetworkEvent::DataChannelOpened);
        }
        Event::ChannelData(channel_data) => {
            // Received data on channel
            log_to_file("CHANNEL_DATA", &format!("Received {} bytes", channel_data.data.len()));
            if let Ok(msg) = NetworkMessage::from_bytes(&channel_data.data) {
                match msg {
                    NetworkMessage::Input(action) => {
                        log_to_file("RECV_INPUT", &format!("Input: {:?}", action));
                        let _ = event_tx.send(NetworkEvent::ReceivedInput(action));
                    }
                    NetworkMessage::BallSync(state) => {
                        log_to_file("RECV_BALLSYNC", &format!("seq={}, pos=({:.2}, {:.2})", state.sequence, state.x, state.y));
                        let _ = event_tx.send(NetworkEvent::ReceivedBallState(state));
                    }
                    NetworkMessage::ScoreSync {
                        left,
                        right,
                        game_over,
                    } => {
                        log_to_file("RECV_SCORE", &format!("Score: {} - {}, game_over={}", left, right, game_over));
                        let _ = event_tx.send(NetworkEvent::ReceivedScore {
                            left,
                            right,
                            game_over,
                        });
                    }
                    NetworkMessage::Ping { timestamp_ms } => {
                        log_to_file("RECV_PING", &format!("Ping: {}", timestamp_ms));
                        let _ = event_tx.send(NetworkEvent::ReceivedPing { timestamp_ms });
                    }
                    NetworkMessage::Pong { timestamp_ms } => {
                        log_to_file("RECV_PONG", &format!("Pong: {}", timestamp_ms));
                        let _ = event_tx.send(NetworkEvent::ReceivedPong { timestamp_ms });
                    }
                    NetworkMessage::Heartbeat { sequence } => {
                        log_to_file("HEARTBEAT_RECV", &format!("Heartbeat #{}", sequence));
                    }
                    NetworkMessage::RematchRequest => {
                        log_to_file("RECV_REMATCH_REQUEST", "Opponent wants to rematch");
                        let _ = event_tx.send(NetworkEvent::ReceivedRematchRequest);
                    }
                    NetworkMessage::RematchConfirm => {
                        log_to_file("RECV_REMATCH_CONFIRM", "Both players ready to rematch");
                        let _ = event_tx.send(NetworkEvent::ReceivedRematchConfirm);
                    }
                    NetworkMessage::QuitRequest => {
                        log_to_file("RECV_QUIT_REQUEST", "Opponent wants to quit");
                        let _ = event_tx.send(NetworkEvent::ReceivedQuitRequest);
                    }
                    NetworkMessage::Disconnect => {
                        let _ = event_tx.send(NetworkEvent::Disconnected);
                    }
                    _ => {}
                }
            } else {
                log_to_file("DECODE_ERROR", &format!("Failed to decode message"));
            }
        }
        _ => {
            // Note: str0m v0.14.x embeds ICE candidates in SDP, not as separate events
            // If we need Trickle ICE in the future, we'd need to upgrade str0m or
            // manually gather candidates before SDP creation
        }
    }

    Ok(())
}
