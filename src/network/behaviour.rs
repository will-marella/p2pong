// Network behaviour for P2Pong with relay support and NAT traversal
// Supports: gossipsub (game messages), relay (NAT traversal), DCUTR (hole punching)

use libp2p::{
    gossipsub, identify, identity, ping, relay,
    swarm::NetworkBehaviour,
};

#[derive(NetworkBehaviour)]
pub struct PongBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub ping: ping::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub dcutr: libp2p::dcutr::Behaviour,
    pub identify: identify::Behaviour,
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
        ).expect("Valid gossipsub behaviour");

        // Identify behavior (required for relay and DCUTR)
        let identify = identify::Behaviour::new(identify::Config::new(
            "/p2pong/1.0.0".to_string(),
            local_key.public(),
        ));

        // DCUTR for hole punching (direct connection upgrade)
        let dcutr = libp2p::dcutr::Behaviour::new(peer_id);

        Self {
            gossipsub,
            ping: ping::Behaviour::new(ping::Config::new()),
            relay_client,
            dcutr,
            identify,
        }
    }
}
