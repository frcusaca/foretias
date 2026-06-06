//! Communerd Server & P2P tiers — bidirectional communication capabilities.
//!
//! | Tier | Type | Capability | Can Accept Incoming? |
//! |------|------|-----------|---------------------|
//! | Reader | `CommunerdReader` (foretias-client) | PtP client — outbound only | No |
//! | Server | `CommunerdServer` | Reader + TCP listener — accepts PtP requests | Yes (PtP) |
//! | P2P | `CommunerdP2P` | Server + libp2P swarm — full mesh | Yes (PtP + P2P) |

use std::sync::Arc;

use super::communerdette::CommunerdetteError;
use super::p2p::swarm::{CommunerdRpcHandler, SwarmCommand};
use super::transport::PeerAddr;
use super::Communerd;
use foretias_core::config::CommunerdConfig;
use foretias_core::error::NodeError;
use foretias_core::foretias::clean_auth::CleanAuthenticated;
use foretias_core::foretias::tick::{ChrononRecord, ForetisRecord};
use libp2p;

/// Server tier — contains CommunerdReader capabilities + TCP listener.
///
/// Accepts incoming PtP requests via the existing Communerd infrastructure.
pub struct CommunerdServer {
    inner: Arc<Communerd>,
}

impl CommunerdServer {
    /// Create a new server with the given configuration.
    pub fn new(config: CommunerdConfig) -> Self {
        Self {
            inner: Arc::new(Communerd::new(config)),
        }
    }

    /// Get the inner Communerd reference.
    pub fn inner(&self) -> &Arc<Communerd> {
        &self.inner
    }

    /// Start liveness pings to configured peers.
    pub fn start_liveness_pings(&self) {
        #[allow(deprecated)]
        self.inner.start_liveness_pings();
    }

    /// Route a stamp through Communerdette — returns `CleanAuthenticated<ForetisRecord>`.
    pub async fn route_stamp(
        &self,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<CleanAuthenticated<ForetisRecord>, CommunerdetteError> {
        self.inner.route_stamp(target_tbid, content_hex, echo).await
    }

    /// Route a stamp through Communerdette with serialization control.
    pub async fn stamp_chronon(
        &self,
        target_tbid: &str,
        content: &[u8],
        serialization: foretias_core::foretias::tick::SerializationAlgorithm,
        echo: &str,
    ) -> Result<CleanAuthenticated<ForetisRecord>, CommunerdetteError> {
        self.inner
            .stamp_chronon(target_tbid, content, serialization, echo)
            .await
    }

    /// Fetch calendar slice from a remote peer.
    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, super::transport::TransportError> {
        self.inner.get_calendar_slice(peer, tick_start, count).await
    }

    /// Get configured peers.
    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.inner.get_peers().await
    }

    /// Get configuration.
    pub fn config(&self) -> &CommunerdConfig {
        self.inner.config()
    }
}

impl Clone for CommunerdServer {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// P2P tier — contains CommunerdServer capabilities + libp2P swarm.
///
/// Full mesh: Kademlia DHT, gossipsub, peer pool, mutual attestation.
/// Gated by `p2p_enabled` in server configuration.
pub struct CommunerdP2P {
    server: CommunerdServer,
}

impl CommunerdP2P {
    /// Create a new P2P instance (P2P not yet enabled — call enable_p2p()).
    pub fn new(config: CommunerdConfig) -> Self {
        Self {
            server: CommunerdServer::new(config),
        }
    }

    /// Get the underlying server.
    pub fn server(&self) -> &CommunerdServer {
        &self.server
    }

    /// Get the inner Communerd reference.
    pub fn inner(&self) -> &Arc<Communerd> {
        self.server.inner()
    }

    /// Enable P2P swarm.
    pub async fn enable_p2p(
        &self,
        listen: Option<libp2p::Multiaddr>,
        dials: Vec<libp2p::Multiaddr>,
        namespace: &str,
        json_rpc_addr: Option<&str>,
        rpc_handler: Option<Arc<dyn CommunerdRpcHandler>>,
    ) -> Result<(), NodeError> {
        self.inner()
            .enable_p2p(listen, dials, namespace, json_rpc_addr, rpc_handler)
            .await
    }

    /// Bootstrap DHT with known bootstrap peers.
    pub async fn bootstrap_dht(&self, bootstrap_addrs: Vec<String>) -> Result<(), NodeError> {
        self.inner().bootstrap_dht(bootstrap_addrs).await
    }

    /// Register self and discover peers via DHT.
    pub async fn register_and_discover(
        &self,
        known_servers: Vec<String>,
        namespace: &str,
        tbid: foretias_core::foretias::types::Tbid,
        chronon_ns: u64,
        json_rpc_addr: &str,
        max_peers: usize,
    ) -> Result<(), NodeError> {
        self.inner()
            .register_and_discover(
                known_servers,
                namespace,
                tbid,
                chronon_ns,
                json_rpc_addr,
                max_peers,
            )
            .await
    }

    /// Local peer ID (available after P2P is enabled).
    pub fn local_peer_id(&self) -> Option<libp2p::PeerId> {
        self.inner().local_peer_id()
    }

    /// Local multiaddr (available after P2P is enabled).
    pub fn local_multiaddr(&self) -> Option<libp2p::Multiaddr> {
        self.inner().local_multiaddr()
    }

    /// Get P2P command channel.
    pub fn p2p_cmd_tx(&self) -> Option<tokio::sync::mpsc::UnboundedSender<SwarmCommand>> {
        self.inner().p2p_cmd_tx()
    }

    /// Start liveness pings.
    pub fn start_liveness_pings(&self) {
        self.server.start_liveness_pings();
    }

    /// Route a stamp through Communerdette — returns `CleanAuthenticated<ForetisRecord>`.
    pub async fn route_stamp(
        &self,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<CleanAuthenticated<ForetisRecord>, CommunerdetteError> {
        self.server
            .route_stamp(target_tbid, content_hex, echo)
            .await
    }

    /// Route a stamp through Communerdette with serialization control.
    pub async fn stamp_chronon(
        &self,
        target_tbid: &str,
        content: &[u8],
        serialization: foretias_core::foretias::tick::SerializationAlgorithm,
        echo: &str,
    ) -> Result<CleanAuthenticated<ForetisRecord>, CommunerdetteError> {
        self.server
            .stamp_chronon(target_tbid, content, serialization, echo)
            .await
    }

    /// Fetch calendar slice from a remote peer.
    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, super::transport::TransportError> {
        self.server
            .get_calendar_slice(peer, tick_start, count)
            .await
    }

    /// Get configured peers.
    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.server.get_peers().await
    }

    /// Get configuration.
    pub fn config(&self) -> &CommunerdConfig {
        self.server.config()
    }

    /// Lookup TBID in DHT.
    pub async fn lookup_tbid(
        &self,
        tbid_hex: &str,
        namespace: &str,
    ) -> Option<super::PeerRegistrationRecord> {
        self.inner().lookup_tbid(tbid_hex, namespace).await
    }

    /// Report probity observation.
    pub fn report_probity(&self, subject: &str, attribute: &str, value: f32) {
        self.inner().report_probity(subject, attribute, value);
    }

    /// Get peer score.
    pub fn get_peer_score(&self, peer_id: &str) -> (f32, usize) {
        self.inner().get_peer_score(peer_id)
    }

    /// Get probity store.
    pub fn probity_store(&self) -> &Arc<crate::probity::ProbityStore> {
        self.inner().probity_store()
    }
}

impl Clone for CommunerdP2P {
    fn clone(&self) -> Self {
        Self {
            server: self.server.clone(),
        }
    }
}
