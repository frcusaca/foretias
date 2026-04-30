//! Fortias libp2p network behaviour.

use libp2p::{identify, ping, swarm::NetworkBehaviour};

#[derive(NetworkBehaviour)]
pub struct FortiasBehaviour {
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
}

impl FortiasBehaviour {
    pub fn new(local_key: &libp2p::identity::Keypair) -> Self {
        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("/fortias/0.3.0".into(), local_key.public().clone())
                    .with_agent_version(format!("fortias/{}", env!("CARGO_PKG_VERSION"))),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
        }
    }
}
