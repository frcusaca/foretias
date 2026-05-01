//! Communerd — orchestrator for all extra-family P2P communication.
//!
//! A Communerd is a communard of a time family commune where timing information
//! is shared in communal communion between families, AND he's a nerd about communications.

pub mod transport;
pub mod json_rpc_transport;
pub mod peer_pool;
pub mod dht_peer_source;
pub mod p2p;

use std::sync::{Arc, OnceLock};

use fortias_core::config::NodeConfig;
use fortias_core::collision::{CollisionDetector, CollisionEvent};
use fortias_core::core::bindings::FortiasPubKey32;
use fortias_core::crypto_server::{CryptoServer, new_software, FortiasCurve};
use fortias_core::error::NodeError;
use fortias_core::fortias::callbacks::{CommunityQuery, CommunityResponse, PeerAddr as CorePeerAddr, PeerMessenger, TransportError as CoreTransportError};
use fortias_core::fortias::tick::{Fortis, TickRecord};

use self::json_rpc_transport::JsonRpcTransport;
use self::peer_pool::PeerPool;
use self::p2p::events::NetworkEvent;
use self::p2p::swarm::{build_and_spawn_swarm, SwarmCommand};
use self::transport::{PeerAddr, PeerTransport, TransportError};
use crate::probity::{ProbityReport, ProbityStore, handle_gossip_message};
use libp2p::kad;

/// Communerd — all P2P traffic flows through this component.
///
/// Calendar calls Communerd for all extra-family communication.
/// Communerd knows nothing about Fortias semantics — it's a transparent RPC relay.
pub struct Communerd {
    transport: Arc<dyn PeerTransport>,
    peer_pool: PeerPool,
    config: NodeConfig,
    local_peer_id: Arc<OnceLock<libp2p::PeerId>>,
    p2p_events: Arc<OnceLock<tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>>>,
    p2p_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    p2p_cmd_tx: Arc<OnceLock<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>>,
    probity_store: Arc<ProbityStore>,
    crypto: Arc<dyn CryptoServer>,
    namespace: Arc<std::sync::Mutex<String>>,
    gossip_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    recompute_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    collision_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    heartbeat_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
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
            p2p_cmd_tx: Arc::clone(&self.p2p_cmd_tx),
            probity_store: Arc::clone(&self.probity_store),
            crypto: Arc::clone(&self.crypto),
            namespace: Arc::clone(&self.namespace),
            gossip_task: Arc::clone(&self.gossip_task),
            recompute_task: Arc::clone(&self.recompute_task),
            collision_task: Arc::clone(&self.collision_task),
            heartbeat_task: Arc::clone(&self.heartbeat_task),
        }
    }
}

impl Communerd {
    pub fn new(config: NodeConfig) -> Self {
        let transport: Arc<dyn PeerTransport> = Arc::new(JsonRpcTransport::new(
            config.request_timeout_secs.max(1),
        ));
        let peers: Vec<PeerAddr> = config.peers.iter()
            .map(|p| PeerAddr { json_rpc: p.clone(), peer_id: None, last_seen_ns: 0 })
            .collect();
        let peer_pool = PeerPool::new(peers, Arc::clone(&transport), 30);
        let crypto = Arc::from(new_software(FortiasCurve::Ed25519).expect("failed to create crypto server"));
        Self {
            transport,
            peer_pool,
            config,
            local_peer_id: Arc::new(OnceLock::new()),
            p2p_events: Arc::new(OnceLock::new()),
            p2p_task: Arc::new(OnceLock::new()),
            p2p_cmd_tx: Arc::new(OnceLock::new()),
            probity_store: Arc::new(ProbityStore::new()),
            crypto,
            namespace: Arc::new(std::sync::Mutex::new("mainnet".to_string())),
            gossip_task: Arc::new(OnceLock::new()),
            recompute_task: Arc::new(OnceLock::new()),
            collision_task: Arc::new(OnceLock::new()),
            heartbeat_task: Arc::new(OnceLock::new()),
        }
    }

    pub fn start_liveness_pings(&self) {
        if !self.config.peers.is_empty() {
            let pool = self.peer_pool.clone();
            tokio::spawn(async move { pool.start_liveness_pings().await });
        }
    }

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

    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<TickRecord>, TransportError> {
        self.transport.get_calendar_slice(peer, tick_start, count).await
    }

    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.peer_pool.get_peers().await
    }

    pub async fn add_peer(&self, addr: PeerAddr) {
        self.peer_pool.add_peer(addr).await;
    }

    pub async fn remove_peer(&self, addr: &PeerAddr) {
        self.peer_pool.remove_peer(addr).await;
    }

    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    pub fn local_peer_id(&self) -> Option<libp2p::PeerId> {
        self.local_peer_id.get().copied()
    }

    pub fn probity_store(&self) -> &Arc<ProbityStore> {
        &self.probity_store
    }

    pub fn p2p_cmd_tx(&self) -> Option<tokio::sync::mpsc::UnboundedSender<SwarmCommand>> {
        self.p2p_cmd_tx.get().cloned()
    }

    pub fn crypto_server(&self) -> Arc<dyn CryptoServer> {
        Arc::clone(&self.crypto)
    }

    pub fn namespace(&self) -> String {
        self.namespace.lock().unwrap().clone()
    }

    pub async fn enable_p2p(
        &self,
        listen: libp2p::Multiaddr,
        dials: Vec<libp2p::Multiaddr>,
        namespace: &str,
        json_rpc_addr: Option<&str>,
    ) -> Result<(), NodeError> {
        if self.local_peer_id.get().is_some() {
            return Ok(());
        }

        *self.namespace.lock().unwrap() = namespace.to_string();

        let handle = build_and_spawn_swarm(listen, dials, namespace, json_rpc_addr).await?;

        let peer_id = handle.local_peer_id;
        let _ = self.local_peer_id.set(peer_id);
        let _ = self.p2p_task.set(handle.task);
        let _ = self.p2p_cmd_tx.set(handle.cmd_tx);

        tracing::info!(peer = %peer_id, "libp2p swarm started");

        // Create collision detector (shared between gossip loop and heartbeat broadcaster)
        let pub_key = self.crypto.public_key();
        let my_pub_key = match pub_key {
            fortias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk,
            _ => FortiasPubKey32 { bytes: [0u8; 32] },
        };
        let detector = Arc::new(CollisionDetector::new(
            peer_id.to_string(), my_pub_key, self.config.collision.nonce_window,
        ));

        // Start gossip event loop
        let events = handle.events;
        let cmd_tx = self.p2p_cmd_tx.get().cloned();
        let probity_store = Arc::clone(&self.probity_store);
        let crypto = Arc::clone(&self.crypto);
        let det = Arc::clone(&detector);
        let task = tokio::spawn(async move {
            Self::gossip_event_loop(events, cmd_tx, probity_store, crypto, Some(det)).await;
        });
        let _ = self.gossip_task.set(task);

        // Start probity score recompute timer
        let probity_store = Arc::clone(&self.probity_store);
        let recompute_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now_ns = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                probity_store.recompute_all(now_ns);
            }
        });
        let _ = self.recompute_task.set(recompute_task);

        // Start heartbeat broadcast task
        let cmd_tx = self.p2p_cmd_tx.get().cloned();
        let ns = self.namespace.clone();
        let interval_secs = self.config.collision.heartbeat_interval_secs.max(5) as u64;
        let crypto_hb = Arc::clone(&self.crypto);
        let peer_id_str = peer_id.to_string();
        let det_hb = Arc::clone(&detector);
        let heartbeat_broadcaster = tokio::spawn(async move {
            Self::heartbeat_broadcast_loop(
                cmd_tx, ns, interval_secs, crypto_hb, peer_id_str, det_hb,
            ).await;
        });
        let _ = self.heartbeat_task.set(heartbeat_broadcaster);

        Ok(())
    }

    async fn heartbeat_broadcast_loop(
        cmd_tx: Option<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>,
        ns: Arc<std::sync::Mutex<String>>,
        interval_secs: u64,
        crypto: Arc<dyn CryptoServer>,
        peer_id_str: String,
        detector: Arc<CollisionDetector>,
    ) {
        let _ = detector;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let mut nonce = [0u8; 16];
            if let Err(e) = crypto.random_bytes(&mut nonce) {
                tracing::warn!("heartbeat nonce generation failed: {e}");
                continue;
            }
            detector.register_own_nonce(nonce);
            let timestamp_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let mut hb = fortias_core::collision::Heartbeat {
                peer_id: peer_id_str.clone(),
                timestamp_ns,
                nonce,
                curve: 1,
                signature: vec![],
            };
            if let Ok(sig) = crypto.sign(&hb.canonical()) {
                hb.signature = sig.bytes.to_vec();
            }
            if let Some(ref tx) = cmd_tx {
                let n = ns.lock().unwrap().clone();
                let _ = tx.send(SwarmCommand::PublishHeartbeat { heartbeat: hb, namespace: n });
            }
        }
    }

    async fn gossip_event_loop(
        mut events: tokio::sync::mpsc::UnboundedReceiver<NetworkEvent>,
        cmd_tx: Option<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>,
        probity_store: Arc<ProbityStore>,
        crypto: Arc<dyn CryptoServer>,
        detector: Option<Arc<CollisionDetector>>,
    ) {
        while let Some(event) = events.recv().await {
            match event {
                NetworkEvent::GossipMessage { data, .. } => {
                    let now_ns = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0);
                    if let Err(e) = handle_gossip_message(&data, &probity_store, crypto.as_ref(), now_ns) {
                        tracing::warn!("gossip message rejected: {e}");
                    }
                }
                NetworkEvent::HeartbeatMessage { data, .. } => {
                    if let Ok(hb) = serde_json::from_slice::<fortias_core::collision::Heartbeat>(&data) {
                        if let Some(det) = &detector {
                            if let Some(CollisionEvent::Confirmed { foreign_heartbeat }) = det.on_heartbeat(&hb, crypto.as_ref()) {
                                tracing::error!(
                                    peer_id = %foreign_heartbeat.peer_id,
                                    nonce = ?foreign_heartbeat.nonce,
                                    "identity collision detected! entering dormancy"
                                );
                                if let Some(ref cmd_tx) = cmd_tx {
                                    let _ = cmd_tx.send(SwarmCommand::EnterDormancy);
                                    tracing::warn!("EnterDormancy command sent to swarm");
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub async fn bootstrap_dht(&self, bootstrap_addrs: Vec<String>) -> Result<(), NodeError> {
        let Some(cmd_tx) = self.p2p_cmd_tx.get() else {
            return Ok(());
        };
        for addr_str in bootstrap_addrs {
            let bootstrap_addr: libp2p::Multiaddr = addr_str.parse()
                .map_err(|e| NodeError::Internal(format!("invalid dht_bootstrap addr: {e}")))?;
            let _ = cmd_tx.send(SwarmCommand::Dial { addr: bootstrap_addr });
        }
        let _ = cmd_tx.send(SwarmCommand::Bootstrap);
        Ok(())
    }

    pub async fn publish_attest_willing(&self, namespace: &str) {
        if let Some(cmd_tx) = self.p2p_cmd_tx.get() {
            let key = kad::RecordKey::new(&format!("{}/fortias/attest-willing/v1", namespace));
            let _ = cmd_tx.send(SwarmCommand::Provide { key });
        }
    }

    pub fn report_probity(&self, subject: &str, attribute: &str, value: f32) {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let reporter = match self.local_peer_id() {
            Some(pid) => pid.to_string(),
            None => return,
        };
        let crypto = self.crypto.clone();
        let mut report = ProbityReport {
            subject: subject.to_string(),
            reporter: reporter.clone(),
            attribute: attribute.to_string(),
            value,
            timestamp_ns: now_ns,
            signature: vec![],
            curve: 1u8,
        };
        if let Ok(sig) = crypto.sign(&report.canonical()) {
            report.signature = sig.bytes.to_vec();
        }
        let _ = self.probity_store.ingest(report.clone());
        if let Some(cmd_tx) = self.p2p_cmd_tx.get() {
            let ns = self.namespace.lock().unwrap().clone();
            let _ = cmd_tx.send(SwarmCommand::PublishProbity { report, namespace: ns });
        }
    }

    pub fn get_peer_score(&self, peer_id: &str) -> (f32, usize) {
        let score = self.probity_store.score(peer_id);
        let count = self.probity_store.report_count(peer_id);
        (score, count)
    }
}

impl PeerMessenger for Communerd {
    fn send_to_peer(
        &self,
        _addr: &CorePeerAddr,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, CoreTransportError> {
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
            auto_attest_every_n: 1,
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
            communerd.add_peer(PeerAddr { json_rpc: "127.0.0.1:4003".into(), peer_id: None, last_seen_ns: 0 }).await;
            let peers = communerd.get_peers().await;
            assert_eq!(peers.len(), 2);
        });
    }

    #[test]
    fn communerd_remove_peer() {
        let communerd = Communerd::new(make_config());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            communerd.remove_peer(&PeerAddr { json_rpc: "127.0.0.1:4002".into(), peer_id: None, last_seen_ns: 0 }).await;
            assert!(communerd.get_peers().await.is_empty());
        });
    }

    #[test]
    fn communerd_clone_shares_state() {
        let communerd = Communerd::new(make_config());
        let c2 = communerd.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            c2.add_peer(PeerAddr { json_rpc: "127.0.0.1:4004".into(), peer_id: None, last_seen_ns: 0 }).await;
            let peers = communerd.get_peers().await;
            assert_eq!(peers.len(), 2);
        });
    }

    #[test]
    fn communerd_config_access() {
        let config = make_config();
        let communerd = Communerd::new(config.clone());
        assert_eq!(communerd.config().peers, config.peers);
        assert_eq!(communerd.config().auto_attest_every_n, 1);
    }

    #[test]
    fn probity_store_default_score_is_zero() {
        let communerd = Communerd::new(make_config());
        let (score, count) = communerd.get_peer_score("unknown-peer");
        assert_eq!(score, 0.0);
        assert_eq!(count, 0);
    }
}
