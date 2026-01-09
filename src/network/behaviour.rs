// Network behaviour for P2Pong with relay support and NAT traversal
// Supports: gossipsub (game messages), relay (NAT traversal), DCUTR (hole punching), AutoNAT (external address discovery)

use libp2p::{autonat, gossipsub, identify, identity, ping, relay, swarm::NetworkBehaviour};

#[derive(NetworkBehaviour)]
pub struct PongBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub ping: ping::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub dcutr: libp2p::dcutr::Behaviour,
    pub identify: identify::Behaviour,
    pub autonat: autonat::Behaviour,
}

impl PongBehaviour {
    /// Create a new PongBehaviour for use with SwarmBuilder
    /// The relay_client is provided by SwarmBuilder's .with_relay_client()
    pub fn new(
        local_key: &identity::Keypair,
        peer_id: libp2p::PeerId,
        relay_client: relay::client::Behaviour,
    ) -> Self {
        // Configure gossipsub with signed message authentication
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .build()
            .expect("Valid gossipsub config");

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .expect("Valid gossipsub behaviour");

        // Identify behavior (required for relay and DCUTR)
        let identify = identify::Behaviour::new(identify::Config::new(
            "/p2pong/1.0.0".to_string(),
            local_key.public(),
        ));

        // DCUTR for hole punching (direct connection upgrade)
        let dcutr = libp2p::dcutr::Behaviour::new(peer_id);

        // AutoNAT for discovering external addresses (needed for DCUTR to work)
        let autonat = autonat::Behaviour::new(
            peer_id,
            autonat::Config {
                // Retry interval: how often to probe for external address
                retry_interval: std::time::Duration::from_secs(30),
                // Initial delay before first probe
                boot_delay: std::time::Duration::from_secs(5),
                // Refresh interval: how often to refresh address after success
                refresh_interval: std::time::Duration::from_secs(60),
                // Confidence threshold: how many successful probes before we're confident
                confidence_max: 3,
                // Only servers can dial us for probing (not other clients)
                only_global_ips: false,
                // Throttle config for incoming dial-back requests
                throttle_server_period: std::time::Duration::from_secs(1),
                ..Default::default()
            },
        );

        Self {
            gossipsub,
            ping: ping::Behaviour::new(ping::Config::new()),
            relay_client,
            dcutr,
            identify,
            autonat,
        }
    }
}
