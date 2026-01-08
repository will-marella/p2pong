// Simplified behaviour for Day 2 - just get connectivity working
// Will add proper message streaming in Day 3

use libp2p::{
    ping,
    swarm::NetworkBehaviour,
    PeerId,
};

/// Simple network behaviour using ping to verify connectivity
#[derive(NetworkBehaviour)]
pub struct SimplePongBehaviour {
    ping: ping::Behaviour,
}

impl SimplePongBehaviour {
    pub fn new() -> Self {
        Self {
            ping: ping::Behaviour::new(ping::Config::new()),
        }
    }
}
