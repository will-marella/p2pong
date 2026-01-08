// Network behaviour for P2Pong using gossipsub for message exchange
// Day 3: Using pub/sub instead of request-response for simpler bidirectional messaging

use libp2p::{
    gossipsub, identity, ping,
    swarm::NetworkBehaviour,
};

#[derive(NetworkBehaviour)]
pub struct PongBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub ping: ping::Behaviour,
}

impl PongBehaviour {
    pub fn new(local_key: &identity::Keypair) -> Self {
        // Configure gossipsub with signed message authentication
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .build()
            .expect("Valid gossipsub config");
        
        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        ).expect("Valid gossipsub behaviour");

        Self {
            gossipsub,
            ping: ping::Behaviour::new(ping::Config::new()),
        }
    }
}
