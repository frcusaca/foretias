//! Communerd — orchestrator for all extra-family P2P communication.
//!
//! A Communerd is a communard of a time family commune where timing information
//! is shared in communal communion between families, AND he's a nerd about communications.

pub mod transport;
pub mod json_rpc_transport;
pub mod peer_pool;
pub mod p2p;

use std::sync::{Arc, OnceLock};

use fortias_core::config::NodeConfig;
use fortias_core::error::NodeError;
use fortias_core::fortias::callbacks::{CommunityQuery, CommunityResponse, PeerAddr as CorePeerAddr, PeerMessenger, TransportError as CoreTransportError};
use fortias_core::fortias::tick::{Fortis, TickRecord};

use self::json_rpc_transport::JsonRpcTransport;
use self::peer_pool::PeerPool;
use self::p2p::events::NetworkEvent;
use self::p2p::swarm::build_and_spawn_swarm;
use self::transport::{PeerAddr, PeerTransport, TransportError};

/// Communerd — all P2P traffic flows through this component.
///
/// Calendar calls Communerd for all extra-family communication.
/// Communerd knows nothing about Fortias semantics — it's a transparent RPC relay.
pub struct Communerd {
    transport: Arc<dyn PeerTransport>,
    peer_pool: PeerPool,
    config: NodeConfig,
    /// libp2p PeerId of this node, set once by `enable_p2p`.
    local_peer_id: Arc<OnceLock<libp2p::PeerId>>,
    /// Shared event receiver for p2p events (owned by whoever calls `enable_p2p`).
    p2p_events: Arc<OnceLock<tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>>>,
    /// Background task handle for the p2p event loop.
    p2p_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
}

impl Clone for Communerd {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            peer_pool: self.peer_pool.clone(),
            config: self.config.clone(),
            local_peer_id: Arc::clone(&self.local_peer_id),
            p2p_events: Arc::clone(&self.p2p_events),
            p2p_task: Arc::clone(&self.p2p_task),
        }
    }
}

impl Communerd {
    /// Create a new Communerd with the given configuration.
    pub fn new(config: NodeConfig) -> Self {
        let transport: Arc<dyn PeerTransport> = Arc::new(JsonRpcTransport::new(
            config.request_timeout_secs.max(1),
        ));
        let peers: Vec<PeerAddr> = config.peers.iter()
            .map(|p| PeerAddr { json_rpc: p.clone(), peer_id: None })
            .collect();
        let peer_pool = PeerPool::new(peers, Arc::clone(&transport), 30);
        Self {
            transport,
            peer_pool,
            config,
            local_peer_id: Arc::new(OnceLock::new()),
            p2p_events: Arc::new(OnceLock::new()),
            p2p_task: Arc::new(OnceLock::new()),
        }
    }

    /// Start background liveness pings for known peers.
    pub fn start_liveness_pings(&self) {
        if !self.config.peers.is_empty() {
            let pool = self.peer_pool.clone();
            tokio::spawn(async move { pool.start_liveness_pings().await });
        }
    }

    /// Send a stamp request to a peer.
    pub async fn stamp_peer(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<Fortis, TransportError> {
        let result = self.transport.stamp(peer, content_hex, echo).await?;
        let fortis: Fortis = serde_json::from_value(result)
            .map_err(|e| TransportError::Decode(e.to_string()))?;
        Ok(fortis)
    }

    /// Fetch a calendar slice from a peer.
    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<TickRecord>, TransportError> {
        self.transport.get_calendar_slice(peer, tick_start, count).await
    }

    /// Get known peers.
    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.peer_pool.get_peers().await
    }

    /// Add a peer dynamically.
    pub async fn add_peer(&self, addr: PeerAddr) {
        self.peer_pool.add_peer(addr).await;
    }

    /// Remove a peer.
    pub async fn remove_peer(&self, addr: &PeerAddr) {
        self.peer_pool.remove_peer(addr).await;
    }

    /// Return the config.
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Return the local libp2p PeerId, if p2p is enabled.
    pub fn local_peer_id(&self) -> Option<libp2p::PeerId> {
        self.local_peer_id.get().copied()
    }

    /// Enable libp2p p2p transport.
    ///
    /// Spawns a swarm on the given listen address and dials the provided peers.
    /// Returns the swarm handle and an event receiver so the server layer can
    /// run the event loop.  Only the first call succeeds (idempotent guard).
    pub async fn enable_p2p(
        &self,
        listen: libp2p::Multiaddr,
        dials: Vec<libp2p::Multiaddr>,
    ) -> Result<(), NodeError> {
        // Fast path: already enabled
        if self.local_peer_id.get().is_some() {
            return Ok(());
        }

        let handle = build_and_spawn_swarm(listen, dials).await?;

        let peer_id = handle.local_peer_id;
        let _ = self.local_peer_id.set(peer_id);
        let _ = self.p2p_task.set(handle.task);

        tracing::info!(peer = %peer_id, "libp2p swarm started");

        Ok(())
    }

}

/// Implement PeerMessenger trait (used by Calendar for mutual attestation).
impl PeerMessenger for Communerd {
    fn send_to_peer(
        &self,
        _addr: &CorePeerAddr,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, CoreTransportError> {
        // PeerMessenger is sync; actual transport is async.
        // This is a stub for the sync trait — the Calendar component
        // will use the async methods directly via Arc<Communerd>.
        Err(CoreTransportError::Connect(
            "sync PeerMessenger not implemented; use async methods".into(),
        ))
    }

    fn query_community(
        &self,
        query: fortias_core::fortias::callbacks::CommunityQuery,
    ) -> Result<fortias_core::fortias::callbacks::CommunityResponse, CoreTransportError> {
        match query {
            CommunityQuery::KnownPeers => {
                Ok(CommunityResponse::KnownPeers(Vec::new()))
            }
            CommunityQuery::PeerByAddr(addr) => {
                Ok(CommunityResponse::PeerStatus {
                    peer: addr,
                    alive: false,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> NodeConfig {
        NodeConfig {
            listen_addr: "127.0.0.1:0".into(),
            peers: vec!["127.0.0.1:4002".into()],
            mutual_attest_every_n: 1,
            request_timeout_secs: 5,
            ..Default::default()
        }
    }

    #[test]
    fn communerd_creates_with_peers() {
        let config = make_config();
        let communerd = Communerd::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let peers = communerd.get_peers().await;
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0].json_rpc, "127.0.0.1:4002");
        });
    }

    #[test]
    fn communerd_add_peer() {
        let communerd = Communerd::new(make_config());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            communerd.add_peer(PeerAddr { json_rpc: "127.0.0.1:4003".into(), peer_id: None }).await;
            let peers = communerd.get_peers().await;
            assert_eq!(peers.len(), 2);
        });
    }

    #[test]
    fn communerd_remove_peer() {
        let communerd = Communerd::new(make_config());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            communerd.remove_peer(&PeerAddr { json_rpc: "127.0.0.1:4002".into(), peer_id: None }).await;
            assert!(communerd.get_peers().await.is_empty());
        });
    }

    #[test]
    fn communerd_clone_shares_state() {
        let communerd = Communerd::new(make_config());
        let c2 = communerd.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            c2.add_peer(PeerAddr { json_rpc: "127.0.0.1:4004".into(), peer_id: None }).await;
            let peers = communerd.get_peers().await;
            assert_eq!(peers.len(), 2);
        });
    }

    #[test]
    fn communerd_config_access() {
        let config = make_config();
        let communerd = Communerd::new(config.clone());
        assert_eq!(communerd.config().peers, config.peers);
        assert_eq!(communerd.config().mutual_attest_every_n, 1);
    }
}
