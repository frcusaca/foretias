//! Communerd — orchestrator for all extra-family P2P communication.
//!
//! A Communerd is a communard of a time family commune where timing information
//! is shared in communal communion between families, AND he's a nerd about communications.

pub mod transport;
pub mod json_rpc_transport;
pub mod libp2p_transport;
pub mod peer_pool;
pub mod dht_peer_source;
pub mod capabilities;
pub mod p2p;
pub mod tiers;

pub use tiers::{CommunerdServer, CommunerdP2P};

use std::sync::{Arc, OnceLock};
use rand::seq::SliceRandom;

use foretias_core::config::CommunerdConfig;
use foretias_core::collision::{CollisionDetector, CollisionEvent};
use foretias_core::core::bindings::ForetiasPubKey32;
use foretias_core::crypto_server::{CryptoServer, new_software, ForetiasCurve};
use foretias_core::error::NodeError;
use foretias_core::foretias::callbacks::{CommunityQuery, CommunityResponse, PeerAddr as CorePeerAddr, PeerMessenger, TransportError as CoreTransportError};
use foretias_core::foretias::clean_auth::{UnprocessedForetis, CleanAuthenticatedForetis};
use foretias_core::foretias::tick::{Foretis, ChrononRecord};
use foretias_core::foretias::types::Tbid;

use crate::calendar::Calendar;
use self::json_rpc_transport::JsonRpcTransport;
use self::libp2p_transport::Libp2pTransport;
use self::peer_pool::PeerPool;
use self::p2p::events::NetworkEvent;
use self::p2p::swarm::{build_and_spawn_swarm, CommunerdRpcHandler, SwarmCommand};
use self::transport::{PeerAddr, PeerTransport, TransportError};
use self::capabilities::PeerCapability;
use crate::probity::{ProbityReport, ProbityStore, handle_gossip_message};
use libp2p::kad;
use std::collections::HashMap;

/// Peer registration record stored in the DHT for self-registration and peer discovery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerRegistrationRecord {
    pub peer_id: String,
    pub tbid: String,
    pub multiaddr: String,
    pub json_rpc: String,
    pub chronon_ns: u64,
    pub registered_at_ns: u64,
    #[serde(default = "default_capabilities")]
    pub capabilities: Vec<PeerCapability>,
}

fn default_capabilities() -> Vec<PeerCapability> {
    vec![PeerCapability::AttestWilling]
}

/// Structural validation for a DHT-retrieved PeerRegistrationRecord.
/// Rejects records with empty or malformed fields before trusting any data.
fn validate_peer_registration(record: &PeerRegistrationRecord) -> bool {
    if record.peer_id.is_empty() {
        return false;
    }
    if record.tbid.len() < 64 || record.tbid.chars().any(|c| !c.is_ascii_hexdigit()) {
        return false;
    }
    if record.json_rpc.is_empty() {
        return false;
    }
    true
}

/// Communerd — all P2P traffic flows through this component.
///
/// Calendar calls Communerd for all extra-family communication.
/// Communerd knows nothing about Foretias semantics — it's a transparent RPC relay.
pub struct Communerd {
    transport: Arc<dyn PeerTransport>,
    libp2p_transport: Arc<Libp2pTransport>,
    peer_pool: PeerPool,
    config: CommunerdConfig,
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
    _local_multiaddr_arc: Arc<std::sync::Mutex<Option<libp2p::Multiaddr>>>,
    tbid_index: Arc<std::sync::RwLock<HashMap<String, PeerRegistrationRecord>>>,
    pending_lookups: Arc<std::sync::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<PeerRegistrationRecord>>>>>,
    calendar: Arc<std::sync::RwLock<Option<Arc<Calendar>>>>,
}

impl Clone for Communerd {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            libp2p_transport: Arc::clone(&self.libp2p_transport),
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
            _local_multiaddr_arc: Arc::clone(&self._local_multiaddr_arc),
            tbid_index: Arc::clone(&self.tbid_index),
            pending_lookups: Arc::clone(&self.pending_lookups),
            calendar: Arc::clone(&self.calendar),
        }
    }
}

impl Communerd {
    pub fn new(config: CommunerdConfig) -> Self {
        let transport: Arc<dyn PeerTransport> = Arc::new(JsonRpcTransport::new(
            config.mutual_attest.request_timeout_secs.max(1),
        ));
        let libp2p_transport = Arc::new(Libp2pTransport::new(
            config.mutual_attest.request_timeout_secs.max(1),
        ));
        let peers: Vec<PeerAddr> = config.mutual_attest.peers.iter()
            .map(|p| PeerAddr { json_rpc: p.clone(), peer_id: None, last_seen_ns: 0 })
            .collect();
        let peer_pool = PeerPool::new(peers, Arc::clone(&transport), 30);
        let crypto = Arc::from(new_software(ForetiasCurve::Ed25519).expect("libsodium must be available at runtime"));
        Self {
            transport,
            libp2p_transport,
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
            _local_multiaddr_arc: Arc::new(std::sync::Mutex::new(None)),
            tbid_index: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_lookups: Arc::new(std::sync::Mutex::new(HashMap::new())),
            calendar: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub fn start_liveness_pings(&self) {
        if !self.config.mutual_attest.peers.is_empty() {
            let pool = self.peer_pool.clone();
            tokio::spawn(async move { pool.start_liveness_pings().await });
        }
    }

    pub async fn stamp_peer(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<Foretis, TransportError> {
        let result = if peer.peer_id.is_some() && self.p2p_cmd_tx.get().is_some() {
            match self.libp2p_transport.stamp(peer, content_hex, echo).await {
                Ok(r) => {
                    tracing::debug!(peer = %peer, transport = "libp2p-direct", "communerd: stamp succeeded");
                    r
                }
                Err(e) => {
                    tracing::debug!(peer = %peer, ?e, transport = "libp2p-direct", "communerd: stamp via libp2p failed, falling back");
                    self.transport.stamp(peer, content_hex, echo).await?
                }
            }
        } else {
            self.transport.stamp(peer, content_hex, echo).await?
        };
        let unprocessed = UnprocessedForetis::from_json_value(result)
            .map_err(|e| TransportError::Decode(e.to_string()))?;
        let f = unprocessed.inner();
        if f.chronon_number == 0 || f.signature.is_empty() || f.signature_algorithm.is_empty() {
            return Err(TransportError::Decode("structurally invalid Foretis: chronon_number == 0, empty signature, or empty signature_algorithm".into()));
        }
        let foretis = CleanAuthenticatedForetis::from_trusted(unprocessed.into_inner()).into_inner();
        Ok(foretis)
    }

    pub async fn route_stamp(
        &self,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<Foretis, TransportError> {
        let owner = self.lookup_tbid(target_tbid, &self.namespace()).await
            .ok_or_else(|| TransportError::Decode(format!("TBID {} not found in DHT", target_tbid)))?;
        let peer = PeerAddr {
            json_rpc: owner.json_rpc,
            peer_id: owner.peer_id.parse().ok(),
            last_seen_ns: 0,
        };
        let result = if peer.peer_id.is_some() && self.p2p_cmd_tx.get().is_some() {
            match self.libp2p_transport.route_stamp(&peer, target_tbid, content_hex, echo).await {
                Ok(r) => {
                    tracing::debug!(peer = %peer, transport = "libp2p-direct", "communerd: route_stamp succeeded");
                    r
                }
                Err(e) => {
                    tracing::debug!(peer = %peer, ?e, transport = "libp2p-direct", "communerd: route_stamp via libp2p failed, falling back");
                    self.transport.route_stamp(&peer, target_tbid, content_hex, echo).await?
                }
            }
        } else {
            self.transport.route_stamp(&peer, target_tbid, content_hex, echo).await?
        };
        let unprocessed = UnprocessedForetis::from_json_value(result)
            .map_err(|e| TransportError::Decode(e.to_string()))?;
        let f = unprocessed.inner();
        if f.chronon_number == 0 || f.signature.is_empty() || f.signature_algorithm.is_empty() {
            return Err(TransportError::Decode("structurally invalid Foretis: chronon_number == 0, empty signature, or empty signature_algorithm".into()));
        }
        let foretis = CleanAuthenticatedForetis::from_trusted(unprocessed.into_inner()).into_inner();
        Ok(foretis)
    }

    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        // TODO(Phase B.4): Add chain verification for returned ChrononRecords using
        // UnprocessedChrononRecord -> CleanAuthenticatedChrononRecord flow.
        if peer.peer_id.is_some() && self.p2p_cmd_tx.get().is_some() {
            match self.libp2p_transport.get_calendar_slice(peer, tick_start, count).await {
                Ok(r) => {
                    tracing::debug!(peer = %peer, transport = "libp2p-direct", "communerd: get_calendar_slice succeeded");
                    return Ok(r);
                }
                Err(e) => {
                    tracing::debug!(peer = %peer, ?e, transport = "libp2p-direct", "communerd: get_calendar_slice via libp2p failed, falling back");
                }
            }
        }
        self.transport.get_calendar_slice(peer, tick_start, count).await
    }

    pub async fn add_peer(&self, addr: PeerAddr) {
        self.peer_pool.add_peer(addr).await;
    }

    pub async fn remove_peer(&self, addr: &PeerAddr) {
        self.peer_pool.remove_peer(addr).await;
    }

    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.peer_pool.get_peers().await
    }

    pub fn config(&self) -> &CommunerdConfig {
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

    pub fn set_calendar(&self, calendar: Arc<Calendar>) {
        *self.calendar.write().unwrap() = Some(calendar);
    }

    pub fn namespace(&self) -> String {
        self.namespace.lock().unwrap().clone()
    }

    pub fn local_multiaddr(&self) -> Option<libp2p::Multiaddr> {
        self._local_multiaddr_arc.lock().unwrap().clone()
    }

    pub async fn enable_p2p(
        &self,
        listen: Option<libp2p::Multiaddr>,
        dials: Vec<libp2p::Multiaddr>,
        namespace: &str,
        json_rpc_addr: Option<&str>,
        rpc_handler: Option<Arc<dyn CommunerdRpcHandler>>,
    ) -> Result<(), NodeError> {
        if self.local_peer_id.get().is_some() {
            return Ok(());
        }

        *self.namespace.lock().unwrap() = namespace.to_string();

        let handle = build_and_spawn_swarm(listen, dials, namespace, json_rpc_addr, rpc_handler).await?;

        let peer_id = handle.local_peer_id;
        let _ = self.local_peer_id.set(peer_id);
        let _ = self.p2p_task.set(handle.task);
        let _ = self.p2p_cmd_tx.set(handle.cmd_tx.clone());
        let _ = self.libp2p_transport.set_cmd_tx(handle.cmd_tx);
        *self._local_multiaddr_arc.lock().unwrap() = handle.local_multiaddr.lock().unwrap().clone();

        tracing::info!(component = "communerd", peer = %peer_id, "communerd: libp2p swarm started");

        // Create collision detector (shared between gossip loop and heartbeat broadcaster)
        let pub_key = self.crypto.public_key();
        let my_pub_key = match pub_key {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk,
            _ => ForetiasPubKey32 { bytes: [0u8; 32] },
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
        let peer_pool = self.peer_pool.clone();
        let tbid_index = Arc::clone(&self.tbid_index);
        let pending_lookups = Arc::clone(&self.pending_lookups);
        let task = tokio::spawn(async move {
            Self::gossip_event_loop(events, cmd_tx, probity_store, crypto, Some(det), peer_pool, tbid_index, pending_lookups).await;
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
            let mut hb = foretias_core::collision::Heartbeat {
                peer_id: peer_id_str.clone(),
                timestamp_ns,
                nonce: nonce.into(),
                curve: 1,
                signature: vec![].into(),
            };
            if let Ok(sig) = crypto.sign(&hb.canonical()) {
                hb.signature = sig.bytes.to_vec().into();
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
        peer_pool: PeerPool,
        tbid_index: Arc<std::sync::RwLock<HashMap<String, PeerRegistrationRecord>>>,
        pending_lookups: Arc<std::sync::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<PeerRegistrationRecord>>>>>,
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
                    if let Ok(hb) = serde_json::from_slice::<foretias_core::collision::Heartbeat>(&data) {
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
                NetworkEvent::RecordRetrieved { key, records } => {
                    if (&*key.to_vec()).ends_with(b"/peers/v1") {
                        for record in &records {
                            if let Ok(peer_record) = serde_json::from_slice::<PeerRegistrationRecord>(&record.value) {
                                if !validate_peer_registration(&peer_record) {
                                    tracing::warn!(component = "communerd", peer_id = %peer_record.peer_id, "communerd: DHT peer record failed structural validation, skipping");
                                    continue;
                                }
                                let peer_addr = PeerAddr {
                                    json_rpc: peer_record.json_rpc.clone(),
                                    peer_id: peer_record.peer_id.parse().ok(),
                                    last_seen_ns: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_nanos() as u64)
                                        .unwrap_or(0),
                                };
                                peer_pool.add_peer(peer_addr).await;
                                tracing::info!(component = "communerd", peer = %peer_record.peer_id, "communerd: DHT-discovered peer added to pool");
                            }
                        }
                    }
                    let key_bytes = key.to_vec();
                    if key_bytes.ends_with(b"/v1") {
                        let key_str = String::from_utf8_lossy(&key_bytes);
                        if key_str.contains("/tbid/") {
                            for record in &records {
                                if let Ok(peer_record) = serde_json::from_slice::<PeerRegistrationRecord>(&record.value) {
                                    if !validate_peer_registration(&peer_record) {
                                        tracing::warn!(component = "communerd", tbid = %peer_record.tbid, "communerd: DHT TBID record failed structural validation, skipping");
                                        continue;
                                    }
                                    let tbid_hex = peer_record.tbid.clone();
                                    tbid_index.write().unwrap().insert(tbid_hex.clone(), peer_record.clone());
                                    tracing::debug!(tbid = %tbid_hex, "TBID index record cached");
                                }
                            }
                            if let Some(sender) = pending_lookups.lock().unwrap().remove(&key) {
                                let result = records.iter().find_map(|r| {
                                    serde_json::from_slice::<PeerRegistrationRecord>(&r.value).ok().filter(|rec| validate_peer_registration(rec))
                                });
                                let _ = sender.send(result);
                            }
                        }
                    }
                }
                NetworkEvent::DhtPeerDiscovered { peer_id, addresses: _ } => {
                    let peer_addr = PeerAddr {
                        json_rpc: String::new(),
                        peer_id: Some(peer_id),
                        last_seen_ns: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0),
                    };
                    peer_pool.add_peer(peer_addr).await;
                    tracing::info!(component = "communerd", peer = %peer_id, "communerd: DHT-discovered peer added to pool");
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
            let key = kad::RecordKey::new(&format!("{}/foretias/attest-willing/v1", namespace));
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

    pub async fn register_and_discover(
        &self,
        known_servers: Vec<String>,
        namespace: &str,
        tbid: Tbid,
        chronon_ns: u64,
        json_rpc_addr: &str,
        _max_peers: usize,
    ) -> Result<(), NodeError> {
        let Some(cmd_tx) = self.p2p_cmd_tx.get() else {
            return Err(NodeError::Internal("P2P not enabled".into()));
        };

        let peer_id = self.local_peer_id.get()
            .copied()
            .ok_or_else(|| NodeError::Internal("PeerId not available".into()))?;

        // Step 1: Shuffle known servers so each node connects in random order
        let mut servers: Vec<String> = known_servers
            .iter()
            .filter(|s| {
                // Self-recognition: skip dialing our own RPC address
                if *s == json_rpc_addr {
                    tracing::debug!(addr = %s, "skipping self-dial (matches own RPC address)");
                    false
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        let mut rng = rand::thread_rng();
        servers.shuffle(&mut rng);

        // Dial all (non-self) known servers in random order
        for addr_str in &servers {
            let multiaddr = resolve_known_server(addr_str)?;
            let _ = cmd_tx.send(SwarmCommand::Dial { addr: multiaddr });
        }

        // Wait a moment for connections to establish
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Step 2: Get our listen address
        let my_multiaddr = self.local_multiaddr()
            .ok_or_else(|| NodeError::Internal("timed out waiting for listen address".into()))?;

        // Step 3: Bootstrap DHT
        let _ = cmd_tx.send(SwarmCommand::Bootstrap);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Step 4: Self-register (PUT record)
        let key = kad::RecordKey::new(&format!("/foretias/{}/peers/v1", namespace));
        let peer_record = PeerRegistrationRecord {
            peer_id: peer_id.to_string(),
            tbid: tbid.to_hex(),
            multiaddr: my_multiaddr.to_string(),
            json_rpc: json_rpc_addr.to_string(),
            chronon_ns,
            registered_at_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
            capabilities: vec![PeerCapability::AttestWilling],
        };
        let record = kad::Record {
            key: key.clone(),
            value: serde_json::to_vec(&peer_record)
                .map_err(|e| NodeError::Internal(format!("serialization error: {e}")))?,
            publisher: Some(peer_id),
            expires: None,
        };
        let _ = cmd_tx.send(SwarmCommand::PutRecord { key: key.clone(), record });
        tracing::info!(component = "communerd", peer = %peer_id, "communerd: self-registration initiated");

        let tbid_hex = tbid.to_hex();
        let tbid_key = kad::RecordKey::new(&format!("/foretias/{}/tbid/{}/v1", namespace, tbid_hex));
        let tbid_record = kad::Record {
            key: tbid_key.clone(),
            value: serde_json::to_vec(&peer_record)
                .map_err(|e| NodeError::Internal(format!("serialization error: {e}")))?,
            publisher: Some(peer_id),
            expires: None,
        };
        let _ = cmd_tx.send(SwarmCommand::PutRecord { key: tbid_key.clone(), record: tbid_record });
        tracing::info!(component = "communerd", tbid = %tbid_hex, "communerd: TBID index record published");

        // Step 5: Discover peers (GET record)
        let _ = cmd_tx.send(SwarmCommand::GetRecord { key: key.clone() });

        // Step 6: Start background registration refresh task
        let cmd_tx_clone = cmd_tx.clone();
        let ns = self.namespace.clone();
        let tbid_arc = tbid;
        let chronon = chronon_ns;
        let rpc = json_rpc_addr.to_string();
        let ma_arc = self._local_multiaddr_arc.clone();
        let pid = peer_id;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                Self::refresh_self_registration(
                    cmd_tx_clone.clone(), ns.clone(), tbid_arc, chronon, &rpc, ma_arc.clone(), pid,
                ).await;
            }
        });

        Ok(())
    }

    async fn refresh_self_registration(
        cmd_tx: tokio::sync::mpsc::UnboundedSender<SwarmCommand>,
        namespace: Arc<std::sync::Mutex<String>>,
        tbid: Tbid,
        chronon_ns: u64,
        json_rpc_addr: &str,
        local_multiaddr: Arc<std::sync::Mutex<Option<libp2p::Multiaddr>>>,
        peer_id: libp2p::PeerId,
    ) {
        let ns = namespace.lock().unwrap().clone();
        let key = kad::RecordKey::new(&format!("/foretias/{}/peers/v1", ns));
        let ma = match local_multiaddr.lock().unwrap().clone() {
            Some(m) => m.to_string(),
            None => return,
        };
        let peer_record = PeerRegistrationRecord {
            peer_id: peer_id.to_string(),
            tbid: tbid.to_hex(),
            multiaddr: ma,
            json_rpc: json_rpc_addr.to_string(),
            chronon_ns,
            registered_at_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
            capabilities: vec![PeerCapability::AttestWilling],
        };
        let record = match serde_json::to_vec(&peer_record) {
            Ok(v) => kad::Record {
                key: key.clone(),
                value: v,
                publisher: Some(peer_id),
                expires: None,
            },
            Err(e) => {
                tracing::warn!("failed to serialize peer record for refresh: {e}");
                return;
            }
        };
        let _ = cmd_tx.send(SwarmCommand::PutRecord { key, record });
        let tbid_hex = tbid.to_hex();
        let tbid_key = kad::RecordKey::new(&format!("/foretias/{}/tbid/{}/v1", ns, tbid_hex));
        let tbid_record = kad::Record {
            key: tbid_key.clone(),
            value: serde_json::to_vec(&peer_record).unwrap_or_default(),
            publisher: Some(peer_id),
            expires: None,
        };
        let _ = cmd_tx.send(SwarmCommand::PutRecord { key: tbid_key, record: tbid_record });
        tracing::debug!(component = "communerd", peer = %peer_id, "communerd: self-registration refreshed");
    }

    pub fn lookup_tbid_cached(&self, tbid_hex: &str) -> Option<PeerRegistrationRecord> {
        self.tbid_index.read().unwrap().get(tbid_hex).cloned()
    }

    pub async fn lookup_tbid(&self, tbid_hex: &str, namespace: &str) -> Option<PeerRegistrationRecord> {
        if let Some(record) = self.lookup_tbid_cached(tbid_hex) {
            return Some(record);
        }
        let Some(cmd_tx) = self.p2p_cmd_tx.get() else {
            return None;
        };
        let key = kad::RecordKey::new(&format!("/foretias/{}/tbid/{}/v1", namespace, tbid_hex));
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_lookups.lock().unwrap().insert(key.clone(), tx);
        let _ = cmd_tx.send(SwarmCommand::GetRecord { key: key.clone() });
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                tracing::debug!(tbid = %tbid_hex, "TBID lookup channel closed");
                None
            }
            Err(_) => {
                tracing::debug!(tbid = %tbid_hex, "TBID lookup timed out");
                self.pending_lookups.lock().unwrap().remove(&key);
                None
            }
        }
    }
}

fn resolve_known_server(addr_str: &str) -> Result<libp2p::Multiaddr, NodeError> {
    let parts: Vec<&str> = addr_str.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(NodeError::Internal(format!("invalid known server address: {}", addr_str)));
    }
    let port: u16 = parts[0].parse()
        .map_err(|e| NodeError::Internal(format!("invalid port in known server: {e}")))?;
    let host = parts[1];

    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        Ok(format!("/ip4/{}/tcp/{}", host, port).parse()
            .map_err(|e| NodeError::Internal(format!("invalid multiaddr parse: {}", e)))?)
    } else if host.parse::<std::net::Ipv6Addr>().is_ok() {
        Ok(format!("/ip6/{}/tcp/{}", host, port).parse()
            .map_err(|e| NodeError::Internal(format!("invalid multiaddr parse: {}", e)))?)
    } else {
        Ok(format!("/dns/{}/tcp/{}", host, port).parse()
            .map_err(|e| NodeError::Internal(format!("invalid multiaddr parse: {}", e)))?)
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
        query: foretias_core::foretias::callbacks::CommunityQuery,
    ) -> Result<foretias_core::foretias::callbacks::CommunityResponse, CoreTransportError> {
        match query {
            CommunityQuery::KnownPeers => {
                // Bridge sync trait → async PeerPool via tokio block_on.
                // Safe: we hold no async-unsafe locks here, and get_peers() is a
                // non-blocking read (tokio::sync::RwLock::read is future-based).
                let peers = match tokio::runtime::Handle::try_current() {
                    Ok(handle) => {
                        handle.block_on(async { self.peer_pool.get_peers().await })
                    }
                    Err(_) => {
                        tracing::warn!("query_community: no tokio runtime available, returning empty peer list");
                        Vec::new()
                    }
                };
                // Map our PeerAddr (with json_rpc, peer_id, last_seen_ns) to core PeerAddr (json_rpc only)
                let core_peers: Vec<CorePeerAddr> = peers
                    .into_iter()
                    .filter(|p| !p.json_rpc.is_empty())
                    .map(|p| CorePeerAddr { json_rpc: p.json_rpc })
                    .collect();
                Ok(CommunityResponse::KnownPeers(core_peers))
            }
            CommunityQuery::PeerByAddr(addr) => {
                // Check liveness: is this peer in our pool and does it have a recent last_seen_ns?
                let alive = match tokio::runtime::Handle::try_current() {
                    Ok(handle) => {
                        handle.block_on(async {
                            let peers = self.peer_pool.get_peers().await;
                            peers.iter().any(|p| p.json_rpc == addr.json_rpc && p.last_seen_ns > 0)
                        })
                    }
                    Err(_) => false,
                };
                Ok(CommunityResponse::PeerStatus {
                    peer: addr,
                    alive,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::config::MutualAttestConfig;

    fn make_config() -> CommunerdConfig {
        CommunerdConfig {
            mutual_attest: MutualAttestConfig {
                peers: vec!["127.0.0.1:4002".into()],
                every_n_chronons: 1,
                request_timeout_secs: 5,
            },
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
        assert_eq!(communerd.config().mutual_attest.peers, config.mutual_attest.peers);
        assert_eq!(communerd.config().mutual_attest.every_n_chronons, 1);
    }

    #[test]
    fn probity_store_default_score_is_zero() {
        let communerd = Communerd::new(make_config());
        let (score, count) = communerd.get_peer_score("unknown-peer");
        assert_eq!(score, 0.0);
        assert_eq!(count, 0);
    }
}
