//! Fortias libp2p network behaviour.

use libp2p::{identify, ping, kad, gossipsub, swarm::NetworkBehaviour};

#[derive(NetworkBehaviour)]
pub struct FortiasBehaviour {
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
    pub kad:      kad::Behaviour<kad::store::MemoryStore>,
    pub gossip:   gossipsub::Behaviour,
}

impl FortiasBehaviour {
    pub fn new(local_key: &libp2p::identity::Keypair, namespace: &str, json_rpc_addr: Option<&str>) -> Self {
        let local_peer_id = local_key.public().to_peer_id();

        let agent = match json_rpc_addr {
            Some(addr) => format!("fortias/{} rpc={}", env!("CARGO_PKG_VERSION"), addr),
            None => format!("fortias/{}", env!("CARGO_PKG_VERSION")),
        };

        let kad_protocol = libp2p::StreamProtocol::try_from_owned(
            format!("/fortias/kad/{}/1.0.0", namespace)
        ).expect("valid protocol string");
        let mut kad_cfg = kad::Config::new(kad_protocol);
        kad_cfg.set_query_timeout(std::time::Duration::from_secs(30));

        let store = kad::store::MemoryStore::new(local_peer_id);

        let gossip_cfg = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(std::time::Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .max_transmit_size(65_536)
            .build()
            .expect("valid gossipsub config");

        let gossip = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossip_cfg,
        ).expect("gossipsub init");

        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("/fortias/0.4.0".into(), local_key.public().clone())
                    .with_agent_version(agent),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            kad:  kad::Behaviour::with_config(local_peer_id, store, kad_cfg),
            gossip,
        }
    }
}
