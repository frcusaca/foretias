//! Foretias libp2p network behaviour.

use libp2p::{
    gossipsub, identify, kad, ping, request_response, swarm::NetworkBehaviour, StreamProtocol,
};
use std::iter;
use std::sync::Arc;

#[derive(NetworkBehaviour)]
pub struct ForetiasBehaviour {
    pub(super) identify: identify::Behaviour,
    pub(super) ping: ping::Behaviour,
    pub(super) kad: kad::Behaviour<kad::store::MemoryStore>,
    pub(super) gossip: gossipsub::Behaviour,
    pub(super) request_response: request_response::Behaviour<super::rpc_protocol::ForetiasRpcCodec>,
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

        let kad_protocol =
            libp2p::StreamProtocol::try_from_owned(format!("/foretias/kad/{}/1.0.0", namespace))
                .expect("valid protocol string");
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
        )
        .expect("gossipsub init");

        let protocols = iter::once((
            StreamProtocol::try_from_owned(format!("/foretias/{}/rpc/1.0.0", namespace))
                .expect("valid protocol string"),
            request_response::ProtocolSupport::Full,
        ));
        let request_response = request_response::Behaviour::with_codec(
            super::rpc_protocol::ForetiasRpcCodec,
            protocols,
            request_response::Config::default(),
        );

        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("/foretias/0.4.0".into(), local_key.public().clone())
                    .with_agent_version(agent),
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            kad: kad::Behaviour::with_config(local_peer_id, store, kad_cfg),
            gossip,
            request_response,
        }
    }
}

/// Shared namespace holder for creating protocol names for direct RPC requests.
#[derive(Clone, Debug)]
pub struct RpcProtocolFactory {
    namespace: Arc<String>,
}

impl RpcProtocolFactory {
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: Arc::new(namespace.to_string()),
        }
    }

    pub fn create_protocol(&self) -> StreamProtocol {
        StreamProtocol::try_from_owned(format!("/foretias/{}/rpc/1.0.0", self.namespace))
            .expect("valid protocol string")
    }
}
