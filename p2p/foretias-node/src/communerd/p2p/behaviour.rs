//! Foretias libp2p network behaviour.

use libp2p::{identify, ping, kad, gossipsub, request_response, swarm::NetworkBehaviour};
use super::rpc_protocol::ForetiasRpcProtocol;
use std::sync::Arc;

#[derive(NetworkBehaviour)]
pub struct ForetiasBehaviour {
    pub identify:          identify::Behaviour,
    pub ping:              ping::Behaviour,
    pub kad:               kad::Behaviour<kad::store::MemoryStore>,
    pub gossip:            gossipsub::Behaviour,
    pub request_response:  request_response::Behaviour<ForetiasRpcProtocol>,
}

impl ForetiasBehaviour {
    pub fn new(
        local_key: &libp2p::identity::Keypair,
        namespace: &str,
        json_rpc_addr: Option<&str>,
    ) -> Self {
        let local_peer_id = local_key.public().to_peer_id();

        let agent = match json_rpc_addr {
            Some(addr) => format!("foretias/{} rpc={}", env!("CARGO_PKG_VERSION"), addr),
            None => format!("foretias/{}", env!("CARGO_PKG_VERSION")),
        };

        let kad_protocol = libp2p::StreamProtocol::try_from_owned(
            format!("/foretias/kad/{}/1.0.0", namespace)
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

        let rpc_protocol = ForetiasRpcProtocol::new(namespace);
        let request_response = request_response::Behaviour::new(
            rpc_protocol.clone(),
            request_response::Config::default(),
        );

        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("/foretias/0.4.0".into(), local_key.public().clone())
                    .with_agent_version(agent),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            kad:  kad::Behaviour::with_config(local_peer_id, store, kad_cfg),
            gossip,
            request_response,
        }
    }
}

/// Shared namespace holder for constructing ForetiasRpcProtocol instances.
pub struct RpcProtocolFactory {
    namespace: Arc<String>,
}

impl RpcProtocolFactory {
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: Arc::new(namespace.to_string()),
        }
    }

    pub fn create_protocol(&self) -> ForetiasRpcProtocol {
        ForetiasRpcProtocol::new(&self.namespace)
    }
}

impl Clone for RpcProtocolFactory {
    fn clone(&self) -> Self {
        Self {
            namespace: Arc::clone(&self.namespace),
        }
    }
}

impl std::fmt::Debug for RpcProtocolFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcProtocolFactory").field("namespace", &self.namespace).finish()
    }
}
