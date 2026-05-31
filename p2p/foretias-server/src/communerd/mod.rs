//! Communerd — orchestrator for all extra-family P2P communication.
//!
//! A Communerd is a communard of a time family commune where timing information
//! is shared in communal communion between families, AND he's a nerd about communications.

mod communerdette;
pub mod transport;
pub mod json_rpc_transport;
pub mod libp2p_transport;
pub mod peer_pool;
pub mod dht_peer_source;
pub mod capabilities;
pub mod p2p;
pub mod tiers;

pub use tiers::{CommunerdServer, CommunerdP2P};
pub use communerdette::{CommunerdetteLine, CommunerdetteStatusSummary, TbidBindingStatus, ActiveRoute, CommunerdetteStats};

use std::sync::{Arc, OnceLock};
use dashmap::DashMap;
use rand::seq::SliceRandom;

use foretias_core::clock::{Clock, SystemClock};
use foretias_core::config::CommunerdConfig;
use foretias_core::collision::{CollisionDetector, CollisionEvent};
use foretias_core::core::bindings::ForetiasPubKey32;
use foretias_core::crypto_server::{CryptoServer, new_software, ForetiasCurve};
use foretias_core::error::NodeError;
use foretias_core::foretias::callbacks::{CommunityQuery, CommunityResponse, PeerAddr as CorePeerAddr, PeerChangeCallback, PeerMessenger, TransportError as CoreTransportError};
use foretias_core::foretias::clean_auth::{UnverifiedSignatureEnvelope, CleanAuthenticated, CleanFullyAuthenticated};
use foretias_core::foretias::tick::{Foretis, ChrononRecord};
use foretias_core::foretias::types::Tbid;
use foretias_core::foretias::family_record::FamilyRecord;

use self::communerdette::Communerdette;

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
///
/// The `signature` field is an Ed25519 signature over `canonical_payload()` produced
/// by the TBID owner's signing key. Records without a signature are accepted under
/// a legacy compatibility window; tighten to required after all peers upgrade
/// (TODO(post-v0.7): require signature).
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
    /// Ed25519 signature over canonical_payload(). `#[serde(default)]` allows
    /// legacy un-signed records from pre-signing peers to deserialize; the
    /// consume path treats empty as "legacy" and logs a warn.
    #[serde(default)]
    pub signature: Vec<u8>,
}

fn default_capabilities() -> Vec<PeerCapability> {
    vec![PeerCapability::AttestWilling]
}

/// Stable per-variant discriminant byte for canonical capability encoding.
/// Changing this is a wire-breaking change.
fn capability_discriminant(cap: &PeerCapability) -> u8 {
    match cap {
        PeerCapability::AttestWilling => 1,
        PeerCapability::MirrorWilling => 2,
        PeerCapability::VerifierWilling => 3,
    }
}

impl PeerRegistrationRecord {
    /// Canonical byte representation for signing — postcard encoding, signature excluded.
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut no_sig = self.clone();
        no_sig.signature = Vec::new();
        postcard::to_allocvec(&no_sig).expect("postcard serialize PeerRegistrationRecord")
    }
}

/// Structural + cryptographic validation for a DHT-retrieved PeerRegistrationRecord.
///
/// Returns Ok(true) for a record that passes both checks, Ok(false) for one that
/// fails structural or signature validation, and Err for an unexpected crypto error
/// (caller treats both Ok(false) and Err the same: skip the record with a warn log).
///
/// Legacy compatibility (transitional): a record with `signature.is_empty()` is
/// accepted with a debug log. TODO(post-v0.7): drop the legacy branch and reject.
#[doc(hidden)]
pub fn validate_peer_registration(
    record: &PeerRegistrationRecord,
    crypto: &dyn CryptoServer,
) -> Result<bool, NodeError> {
    // Structural checks (unchanged).
    if record.peer_id.is_empty() {
        return Ok(false);
    }
    if record.tbid.len() < 64 || record.tbid.chars().any(|c| !c.is_ascii_hexdigit()) {
        return Ok(false);
    }
    if record.json_rpc.is_empty() {
        return Ok(false);
    }

    // Signature check.
    if record.signature.is_empty() {
        tracing::debug!(
            tbid = %record.tbid,
            "DHT record without signature (pre-signing peer); accepted under legacy compat"
        );
        return Ok(true);
    }
    let pubkey = match hex::decode(&record.tbid) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(tbid = %record.tbid, "DHT record TBID is not valid hex: {e}");
            return Ok(false);
        }
    };
    if pubkey.len() < 32 {
        tracing::warn!(tbid = %record.tbid, "DHT record TBID too short for Ed25519 pubkey extraction");
        return Ok(false);
    }
    let canonical = record.canonical_payload();
    let valid = crypto
        .verify_with(&pubkey[..32], "Ed25519", &canonical, &record.signature)
        .map_err(|e| NodeError::Crypto(e))?;
    if !valid {
        tracing::warn!(
            tbid = %record.tbid,
            "DHT record signature verification failed; discarding"
        );
    }
    Ok(valid)
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
    clock: Arc<dyn Clock>,
    namespace: Arc<std::sync::Mutex<String>>,
    gossip_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    recompute_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    collision_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    heartbeat_task: Arc<OnceLock<tokio::task::JoinHandle<()>>>,
    _local_multiaddr_arc: Arc<std::sync::Mutex<Option<libp2p::Multiaddr>>>,
    tbid_index: Arc<std::sync::RwLock<HashMap<String, PeerRegistrationRecord>>>,
    pending_lookups: Arc<std::sync::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<PeerRegistrationRecord>>>>>,
    calendar: Arc<std::sync::RwLock<Option<Arc<Calendar>>>>,
    communerdettes: Arc<DashMap<Tbid, Arc<communerdette::Communerdette>>>,
    /// Family Cache: TBID → CleanFullyAuthenticated<FamilyRecord>.
    /// Indexed by every member TBID for fast reverse lookup.
    family_cache: Arc<DashMap<String, Arc<CleanFullyAuthenticated<FamilyRecord>>>>,
    /// Group 4b: peer-pool change callback (typically the Calendar).
    /// Fires after every peer add/remove with the current peer-pool snapshot.
    peer_change_cb: Arc<std::sync::Mutex<Option<Arc<dyn PeerChangeCallback>>>>,
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
            clock: Arc::clone(&self.clock),
            namespace: Arc::clone(&self.namespace),
            gossip_task: Arc::clone(&self.gossip_task),
            recompute_task: Arc::clone(&self.recompute_task),
            collision_task: Arc::clone(&self.collision_task),
            heartbeat_task: Arc::clone(&self.heartbeat_task),
            _local_multiaddr_arc: Arc::clone(&self._local_multiaddr_arc),
            tbid_index: Arc::clone(&self.tbid_index),
            pending_lookups: Arc::clone(&self.pending_lookups),
            calendar: Arc::clone(&self.calendar),
            communerdettes: Arc::clone(&self.communerdettes),
            family_cache: Arc::clone(&self.family_cache),
            peer_change_cb: Arc::clone(&self.peer_change_cb),
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
            clock: Arc::new(SystemClock),
            namespace: Arc::new(std::sync::Mutex::new("mainnet".to_string())),
            gossip_task: Arc::new(OnceLock::new()),
            recompute_task: Arc::new(OnceLock::new()),
            collision_task: Arc::new(OnceLock::new()),
            heartbeat_task: Arc::new(OnceLock::new()),
            _local_multiaddr_arc: Arc::new(std::sync::Mutex::new(None)),
            tbid_index: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_lookups: Arc::new(std::sync::Mutex::new(HashMap::new())),
            calendar: Arc::new(std::sync::RwLock::new(None)),
            communerdettes: Arc::new(DashMap::new()),
            family_cache: Arc::new(DashMap::new()),
            peer_change_cb: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Returns a CommunerdetteLine for the given TBID, creating one if needed.
    pub fn line_for_tbid(&self, tbid: Tbid) -> CommunerdetteLine {
        let host: Arc<dyn communerdette::CommunerdetteHost> = Arc::new(self.clone());
        let crypto = Arc::clone(&self.crypto);
        let clock = Arc::clone(&self.clock);
        if let Some(entry) = self.communerdettes.get(&tbid) {
            CommunerdetteLine::new(tbid, Arc::clone(entry.value()), Arc::clone(&host), Arc::clone(&crypto), Arc::clone(&clock))
        } else {
            let communerdette = Arc::new(Communerdette::new(tbid));
            self.communerdettes.insert(tbid, Arc::clone(&communerdette));
            CommunerdetteLine::new(tbid, communerdette, host, crypto, clock)
        }
    }

    /// Returns a CommunerdetteLine for the given TBID only if one already exists.
    pub fn try_line_for_tbid(&self, tbid: Tbid) -> Option<CommunerdetteLine> {
        let host: Arc<dyn communerdette::CommunerdetteHost> = Arc::new(self.clone());
        let crypto = Arc::clone(&self.crypto);
        let clock = Arc::clone(&self.clock);
        self.communerdettes.get(&tbid).map(|entry| {
            CommunerdetteLine::new(tbid, Arc::clone(entry.value()), Arc::clone(&host), Arc::clone(&crypto), Arc::clone(&clock))
        })
    }

    /// Insert a verified FamilyRecord into the family cache.
    /// Indexes by every member TBID for O(1) reverse lookup.
    pub fn family_cache_insert(&self, record: Arc<CleanFullyAuthenticated<FamilyRecord>>) {
        for member_tbid_hex in record.inner().members.iter() {
            self.family_cache.insert(member_tbid_hex.clone(), Arc::clone(&record));
        }
    }

    /// Lookup a FamilyRecord by any member TBID.
    pub fn family_cache_lookup(&self, tbid_hex: &str) -> Option<Arc<CleanFullyAuthenticated<FamilyRecord>>> {
        self.family_cache.get(tbid_hex).map(|entry| Arc::clone(entry.value()))
    }

    #[deprecated(note = "Use Communerdette L1 liveness loop (Phase 12.1) instead. \
        PeerPool liveness is superseded by per-TBID Communerdette-driven liveness.")]
    pub fn start_liveness_pings(&self) {
        if !self.config.mutual_attest.peers.is_empty() {
            let pool = self.peer_pool.clone();
            tokio::spawn(async move { pool.start_liveness_pings().await });
        }
    }

    #[deprecated(note = "Use CommunerdetteLine::stamp via line_for_tbid. stamp_peer bypasses the Take 3 inbound gate.")]
    pub async fn stamp_peer(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<Foretis, TransportError> {
        let result = if peer.peer_id.is_some() && self.p2p_cmd_tx.get().is_some() {
            match self.libp2p_transport.stamp(peer, content_hex, echo).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!(peer = %peer, ?e, "communerd: stamp via libp2p failed, falling back");
                    self.transport.stamp(peer, content_hex, echo).await?
                }
            }
        } else {
            self.transport.stamp(peer, content_hex, echo).await?
        };
        let unprocessed = UnverifiedSignatureEnvelope::<Foretis>::from_json_value(result)
            .map_err(|e| TransportError::Decode(e.to_string()))?;
        let f = unprocessed.inner();
        if f.chronon_number == 0 || f.signature.is_empty() || f.signature_algorithm.is_empty() {
            return Err(TransportError::Decode("structurally invalid Foretis".into()));
        }
        #[allow(deprecated)]
        Ok(CleanAuthenticated::<Foretis>::from_trusted(unprocessed.into_inner()).into_inner())
    }

    /// Route a stamp request through Communerdette for the target TBID.
    ///
    /// Returns `CleanAuthenticated<Foretis>` — fully gated through the Take 3
    /// inbound pipeline. Old direct-transport path was removed in Phase 4.3.
    pub async fn route_stamp(
        &self,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<foretias_core::foretias::clean_auth::CleanAuthenticated<Foretis>, communerdette::CommunerdetteError> {
        let content = hex::decode(content_hex)
            .map_err(|e| communerdette::CommunerdetteError::Structural(format!("content not hex: {e}")))?;
        let tbid = Tbid::from_hex(target_tbid)
            .map_err(|e| communerdette::CommunerdetteError::Structural(format!("bad tbid hex: {e}")))?;
        self.line_for_tbid(tbid).stamp(content, echo.to_string()).await
    }

    pub async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        // TODO(Phase B.4): Add chain verification for returned ChrononRecords using
        // UnverifiedSignatureEnvelopeChrononRecord -> CleanAuthenticatedChrononRecord flow.
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
        self.notify_peer_change().await;
    }

    pub async fn remove_peer(&self, addr: &PeerAddr) {
        self.peer_pool.remove_peer(addr).await;
        self.notify_peer_change().await;
    }

    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.peer_pool.get_peers().await
    }

    /// Register a callback that fires after every peer-pool change.
    ///
    /// Typically wired by `TimeFamilyServer` so the Calendar can react to
    /// peer-pool churn (Group 4b mirror discovery). Overwrites any prior
    /// callback; passing `None` clears it.
    pub fn set_peer_change_callback(&self, cb: Option<Arc<dyn PeerChangeCallback>>) {
        *self.peer_change_cb.lock().unwrap() = cb;
    }

    /// Fire the peer-change callback (if registered) with the current snapshot
    /// of the peer pool. The snapshot is converted to `foretias_core` PeerAddr
    /// so Calendar code does not depend on Communerd transport types.
    async fn notify_peer_change(&self) {
        let cb = self.peer_change_cb.lock().unwrap().clone();
        if let Some(cb) = cb {
            let peers = self.peer_pool.get_peers().await;
            let core_peers: Vec<CorePeerAddr> = peers
                .into_iter()
                .map(|p| CorePeerAddr { json_rpc: p.json_rpc })
                .collect();
            cb.on_peer_change(core_peers);
        }
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
        let clock_gossip = Arc::clone(&self.clock);
        let det = Arc::clone(&detector);
        let peer_pool = self.peer_pool.clone();
        let tbid_index = Arc::clone(&self.tbid_index);
        let pending_lookups = Arc::clone(&self.pending_lookups);
        let task = tokio::spawn(async move {
            Self::gossip_event_loop(events, cmd_tx, probity_store, crypto, clock_gossip, Some(det), peer_pool, tbid_index, pending_lookups).await;
        });
        let _ = self.gossip_task.set(task);

        // Start probity score recompute timer
        let probity_store = Arc::clone(&self.probity_store);
        let clock_rt = Arc::clone(&self.clock);
        let recompute_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now_ns = clock_rt.now_ns().unwrap_or(0);
                probity_store.recompute_all(now_ns);
            }
        });
        let _ = self.recompute_task.set(recompute_task);

        // Start heartbeat broadcast task
        let cmd_tx = self.p2p_cmd_tx.get().cloned();
        let ns = self.namespace.clone();
        let interval_secs = self.config.collision.heartbeat_interval_secs.max(5) as u64;
        let crypto_hb = Arc::clone(&self.crypto);
        let clock_hb = Arc::clone(&self.clock);
        let peer_id_str = peer_id.to_string();
        let det_hb = Arc::clone(&detector);
        let heartbeat_broadcaster = tokio::spawn(async move {
            Self::heartbeat_broadcast_loop(
                cmd_tx, ns, interval_secs, crypto_hb, clock_hb, peer_id_str, det_hb,
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
        clock: Arc<dyn Clock>,
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
            let timestamp_ns = clock.now_ns().unwrap_or(0);
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
        clock: Arc<dyn Clock>,
        detector: Option<Arc<CollisionDetector>>,
        peer_pool: PeerPool,
        tbid_index: Arc<std::sync::RwLock<HashMap<String, PeerRegistrationRecord>>>,
        pending_lookups: Arc<std::sync::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<PeerRegistrationRecord>>>>>,
    ) {
        while let Some(event) = events.recv().await {
            match event {
                NetworkEvent::GossipMessage { data, .. } => {
                    let now_ns = clock.now_ns().unwrap_or(0);
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
                                match validate_peer_registration(&peer_record, crypto.as_ref()) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        tracing::warn!(component = "communerd", peer_id = %peer_record.peer_id, "communerd: DHT peer record failed structural or signature validation, skipping");
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::warn!(component = "communerd", peer_id = %peer_record.peer_id, error = %e, "communerd: DHT peer record validation errored, skipping");
                                        continue;
                                    }
                                }
                                let peer_addr = PeerAddr {
                                    json_rpc: peer_record.json_rpc.clone(),
                                    peer_id: peer_record.peer_id.parse().ok(),
                                    last_seen_ns: clock.now_ns().unwrap_or(0),
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
                                    match validate_peer_registration(&peer_record, crypto.as_ref()) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            tracing::warn!(component = "communerd", tbid = %peer_record.tbid, "communerd: DHT TBID record failed structural or signature validation, skipping");
                                            continue;
                                        }
                                        Err(e) => {
                                            tracing::warn!(component = "communerd", tbid = %peer_record.tbid, error = %e, "communerd: DHT TBID record validation errored, skipping");
                                            continue;
                                        }
                                    }
                                    let tbid_hex = peer_record.tbid.clone();
                                    tbid_index.write().unwrap().insert(tbid_hex.clone(), peer_record.clone());
                                    tracing::debug!(tbid = %tbid_hex, "TBID index record cached");
                                }
                            }
                            if let Some(sender) = pending_lookups.lock().unwrap().remove(&key) {
                                let result = records.iter().find_map(|r| {
                                    serde_json::from_slice::<PeerRegistrationRecord>(&r.value)
                                        .ok()
                                        .filter(|rec| {
                                            matches!(
                                                validate_peer_registration(rec, crypto.as_ref()),
                                                Ok(true)
                                            )
                                        })
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
                        last_seen_ns: clock.now_ns().unwrap_or(0),
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
        let now_ns = self.clock.now_ns().unwrap_or(0);
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
            slow_signature: vec![],
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

        // Step 4: Self-register (PUT record).
        // Build the record with empty signature, sign canonical_payload, then
        // populate the signature field before serialization. Both DHT records
        // (peers/v1 and tbid/v1) carry the same signed PeerRegistrationRecord.
        let key = kad::RecordKey::new(&format!("/foretias/{}/peers/v1", namespace));
        let mut peer_record = PeerRegistrationRecord {
            peer_id: peer_id.to_string(),
            tbid: tbid.to_hex(),
            multiaddr: my_multiaddr.to_string(),
            json_rpc: json_rpc_addr.to_string(),
            chronon_ns,
            registered_at_ns: self.clock.now_ns().unwrap_or(0),
            capabilities: vec![PeerCapability::AttestWilling],
            signature: Vec::new(),
        };
        let canonical = peer_record.canonical_payload();
        let sig = self
            .crypto
            .sign(&canonical)
            .map_err(|e| NodeError::Internal(format!("DHT record sign: {e}")))?;
        peer_record.signature = sig.bytes.to_vec();

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
        let clock_refresh = Arc::clone(&self.clock);
        let crypto_refresh = Arc::clone(&self.crypto);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                Self::refresh_self_registration(
                    cmd_tx_clone.clone(), ns.clone(), tbid_arc, chronon, &rpc, ma_arc.clone(), pid, Arc::clone(&clock_refresh), Arc::clone(&crypto_refresh),
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
        clock: Arc<dyn Clock>,
        crypto: Arc<dyn CryptoServer>,
    ) {
        let ns = namespace.lock().unwrap().clone();
        let key = kad::RecordKey::new(&format!("/foretias/{}/peers/v1", ns));
        let ma = match local_multiaddr.lock().unwrap().clone() {
            Some(m) => m.to_string(),
            None => return,
        };
        let mut peer_record = PeerRegistrationRecord {
            peer_id: peer_id.to_string(),
            tbid: tbid.to_hex(),
            multiaddr: ma,
            json_rpc: json_rpc_addr.to_string(),
            chronon_ns,
            registered_at_ns: clock.now_ns().unwrap_or(0),
            capabilities: vec![PeerCapability::AttestWilling],
            signature: Vec::new(),
        };
        let canonical = peer_record.canonical_payload();
        match crypto.sign(&canonical) {
            Ok(sig) => {
                peer_record.signature = sig.bytes.to_vec();
            }
            Err(e) => {
                tracing::warn!("failed to sign refresh peer record: {e}");
                return;
            }
        }
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

    /// Publish a FamilyRecord to the DHT under the `/family/{tbid}/v1` key.
    /// Called once per family by the Communerd that owns it.
    pub fn publish_family_record(&self, ns: &str, record: &FamilyRecord) {
        let Some(peer_id) = self.local_peer_id.get().cloned() else { return };
        let Some(cmd_tx) = self.p2p_cmd_tx.get() else { return };
        let value = match serde_json::to_vec(record) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("failed to serialize FamilyRecord for DHT: {e}");
                return;
            }
        };
        // Publish under each member's family key
        for member_tbid_hex in &record.members {
            let key = kad::RecordKey::new(&format!("/foretias/{}/family/{}/v1", ns, member_tbid_hex));
            let kad_record = kad::Record {
                key: key.clone(),
                value: value.clone(),
                publisher: Some(peer_id),
                expires: None,
            };
            let _ = cmd_tx.send(SwarmCommand::PutRecord { key, record: kad_record });
        }
        tracing::debug!(component = "communerd", members = %record.members.len(), "communerd: FamilyRecord published to DHT");
    }

    /// Fetch FamilyRecord from DHT, verify (full gate), insert into family cache.
    /// Returns the cached record on success, None on any failure.
    pub async fn fetch_and_cache_family_record(&self, tbid_hex: &str, ns: &str) -> Option<Arc<CleanFullyAuthenticated<FamilyRecord>>> {
        // Check local cache first
        if let Some(cached) = self.family_cache_lookup(tbid_hex) {
            return Some(cached);
        }
        // DHT lookup
        let key = kad::RecordKey::new(&format!("/foretias/{}/family/{}/v1", ns, tbid_hex));
        let Some(cmd_tx) = self.p2p_cmd_tx.get() else { return None };
        let (tx, rx) = tokio::sync::oneshot::channel();
        // Use pending_lookups with a wrapper key to get raw kad::Record back
        self.pending_lookups.lock().unwrap().insert(key.clone(), tx);
        let _ = cmd_tx.send(SwarmCommand::GetRecord { key: key.clone() });
        let raw_bytes = match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(Some(record))) => serde_json::to_vec(&record).ok()?,
            Ok(Ok(None)) | Ok(Err(_)) => {
                tracing::debug!(tbid = %tbid_hex, "FamilyRecord not found in DHT");
                return None;
            }
            Err(_) => {
                tracing::debug!(tbid = %tbid_hex, "FamilyRecord DHT lookup timed out");
                self.pending_lookups.lock().unwrap().remove(&key);
                return None;
            }
        };
        let family_record: FamilyRecord = match serde_json::from_slice(&raw_bytes) {
            Ok(fr) => fr,
            Err(e) => {
                tracing::warn!(tbid = %tbid_hex, "failed to deserialize FamilyRecord: {e}");
                return None;
            }
        };
        let envelope = UnverifiedSignatureEnvelope::from_parsed(family_record);
        // Full gate verification
        let ca = match envelope.verify_family_record(&*self.crypto, &[0u8; 32]) {
            Ok(cfa) => cfa,
            Err(e) => {
                tracing::warn!(tbid = %tbid_hex, "FamilyRecord gate verification failed: {e}");
                return None;
            }
        };
        let arc = Arc::new(ca);
        self.family_cache_insert(Arc::clone(&arc));
        Some(arc)
    }

    /// Phase 9: Shutdown all Communerdette relationships.
    ///
    /// Cancels all per-relationship tasks, drops cancellation tokens.
    /// After this call, all CommunerdetteLines for affected TBIDs
    /// will report their shutdown tokens as cancelled.
    pub fn shutdown_relationships(&self) {
        tracing::info!(component = "communerd", "shutting down all communerdette relationships");
        for entry in self.communerdettes.iter() {
            entry.value().shutdown();
        }
    }

    /// Phase 6: PeerPool migration bridge.
    ///
    /// When DHT or PeerPool state changes, update the relevant Communerdette
    /// so that relationship-local memory stays current.
    ///
    /// This is called from the gossip event loop when a TBID record is retrieved.
    pub(super) fn bridge_dht_to_communerdette(&self, tbid_hex: &str, record: &PeerRegistrationRecord) {
        let tbid = match Tbid::from_hex(tbid_hex) {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!(tbid = %tbid_hex, "invalid TBID in DHT bridge, skipping");
                return;
            }
        };
        if let Some(entry) = self.communerdettes.get(&tbid) {
            let now_ns = self.clock.now_ns().unwrap_or(0);
            entry.value().mark_binding_claimed_by_dht(
                Some(record.peer_id.clone()),
                Some(record.json_rpc.clone()),
                now_ns,
            );
            entry.value().add_route_candidate(record.clone());
        }
    }

    /// Get calendar slice by TBID via CommunerdetteLine.
    pub async fn get_calendar_slice_by_tbid(
        &self,
        tbid_hex: &str,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        let tbid = Tbid::from_hex(tbid_hex)
            .map_err(|e| TransportError::Decode(format!("invalid TBID: {e}")))?;
        let _line = self.line_for_tbid(tbid);
        let owner = self.lookup_tbid(tbid_hex, &self.namespace()).await
            .ok_or_else(|| TransportError::Decode(format!("TBID {} not found in DHT", tbid_hex)))?;
        let peer = PeerAddr {
            json_rpc: owner.json_rpc,
            peer_id: owner.peer_id.parse().ok(),
            last_seen_ns: 0,
        };
        self.get_calendar_slice(&peer, tick_start, count).await
    }
}

// ── Group 4b: MirrorDispatcher implementation ─────────────────────────────

#[async_trait::async_trait]
impl crate::calendar::MirrorDispatcher for Communerd {
    async fn known_peers(&self) -> Vec<foretias_core::foretias::callbacks::PeerAddr> {
        self.peer_pool
            .get_peers()
            .await
            .into_iter()
            .filter(|p| !p.json_rpc.is_empty())
            .map(|p| foretias_core::foretias::callbacks::PeerAddr {
                json_rpc: p.json_rpc,
            })
            .collect()
    }

    async fn mirror_announce(
        &self,
        peer: &foretias_core::foretias::callbacks::PeerAddr,
        local_tbid_hex: &str,
    ) -> Result<bool, String> {
        let resp = mirror_rpc(
            &self.json_rpc_transport(),
            peer,
            "mirror_announce",
            serde_json::json!({ "tbid": local_tbid_hex }),
        )
        .await?;
        Ok(resp
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| s == "accept")
            .unwrap_or(false))
    }

    async fn history_dump_chunk(
        &self,
        peer: &foretias_core::foretias::callbacks::PeerAddr,
        local_tbid_hex: &str,
        records: Vec<ChrononRecord>,
    ) -> Result<u64, String> {
        let resp = mirror_rpc(
            &self.json_rpc_transport(),
            peer,
            "history_dump_chunk",
            serde_json::json!({
                "tbid": local_tbid_hex,
                "records": records,
            }),
        )
        .await?;
        Ok(resp
            .get("accepted_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0))
    }

    async fn history_dump_complete(
        &self,
        peer: &foretias_core::foretias::callbacks::PeerAddr,
        local_tbid_hex: &str,
        total_records: u64,
    ) -> Result<u64, String> {
        let resp = mirror_rpc(
            &self.json_rpc_transport(),
            peer,
            "history_dump_complete",
            serde_json::json!({
                "tbid": local_tbid_hex,
                "total_records": total_records,
            }),
        )
        .await?;
        Ok(resp
            .get("tick_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0))
    }

    async fn mirror_health_check(
        &self,
        peer: &foretias_core::foretias::callbacks::PeerAddr,
        local_tbid_hex: &str,
    ) -> Result<u64, String> {
        let resp = mirror_rpc(
            &self.json_rpc_transport(),
            peer,
            "mirror_health_check",
            serde_json::json!({ "tbid": local_tbid_hex }),
        )
        .await?;
        Ok(resp
            .get("tick_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0))
    }
}

impl Communerd {
    /// Borrow the JsonRpcTransport for direct RPC use. Used by
    /// MirrorDispatcher to invoke methods that don't fit the legacy
    /// `PeerTransport` surface (mirror_announce, history_dump_*, etc.).
    fn json_rpc_transport(&self) -> Arc<JsonRpcTransport> {
        // Concrete transport — for MirrorDispatcher we always want the
        // direct Noise_XX TCP path because mirror methods are not yet
        // wired into the libp2p request_response surface.
        Arc::new(JsonRpcTransport::new(
            self.config.mutual_attest.request_timeout_secs.max(1),
        ))
    }
}

/// Issue a JSON-RPC call via the Noise_XX TCP transport and return the
/// "result" object on success.
async fn mirror_rpc(
    transport: &JsonRpcTransport,
    peer: &foretias_core::foretias::callbacks::PeerAddr,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let transport_peer = PeerAddr {
        json_rpc: peer.json_rpc.clone(),
        peer_id: None,
        last_seen_ns: 0,
    };
    transport
        .json_rpc_call(&transport_peer, method, params)
        .await
        .map_err(|e| format!("{e}"))
}

// ── Phase 3.3: CommunerdetteHost trait implementation ────────────────────

#[async_trait::async_trait]
impl communerdette::CommunerdetteHost for Communerd {
    async fn host_lookup_tbid(&self, tbid_hex: &str, namespace: &str) -> Option<PeerRegistrationRecord> {
        self.lookup_tbid(tbid_hex, namespace).await
    }

    fn host_lookup_tbid_cached(&self, tbid_hex: &str) -> Option<PeerRegistrationRecord> {
        self.lookup_tbid_cached(tbid_hex)
    }

    fn host_namespace(&self) -> String {
        self.namespace()
    }

    fn host_swarm_available(&self) -> bool {
        self.p2p_cmd_tx().is_some()
    }

    fn host_local_peer_id(&self) -> Option<libp2p::PeerId> {
        self.local_peer_id()
    }

    async fn host_execute_stamp(
        &self,
        peer: &PeerAddr,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        if peer.peer_id.is_some() && self.p2p_cmd_tx().is_some() {
            match self.libp2p_transport.route_stamp(peer, target_tbid, content_hex, echo).await {
                Ok(r) => Ok(r),
                Err(e) => {
                    tracing::debug!(%peer, ?e, "communerdette: libp2p stamp failed, falling back");
                    self.transport.route_stamp(peer, target_tbid, content_hex, echo).await
                }
            }
        } else {
            self.transport.route_stamp(peer, target_tbid, content_hex, echo).await
        }
    }

    async fn host_execute_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        if peer.peer_id.is_some() && self.p2p_cmd_tx().is_some() {
            match self.libp2p_transport.get_calendar_slice(peer, tick_start, count).await {
                Ok(r) => Ok(r),
                Err(e) => {
                    tracing::debug!(%peer, ?e, "communerdette: libp2p calendar_slice failed, falling back");
                    self.transport.get_calendar_slice(peer, tick_start, count).await
                }
            }
        } else {
            self.transport.get_calendar_slice(peer, tick_start, count).await
        }
    }

    async fn host_execute_channel_bind_challenge(
        &self,
        peer: &PeerAddr,
        nonce_hex: &str,
        channel_id: &str,
        requester_tbid_hex: &str,
    ) -> Result<serde_json::Value, TransportError> {
        // TODO(externalized): wrap params as Externalized<R> before dispatch (Phase 11.4).
        self.transport.channel_bind_challenge(peer, nonce_hex, channel_id, requester_tbid_hex).await
    }

    async fn host_execute_ping(&self, peer: &PeerAddr) -> Result<(), TransportError> {
        self.transport.ping(peer).await
    }

    fn host_sign_probity_report(&self, report: &crate::probity::ProbityReport) -> Result<Vec<u8>, String> {
        let canonical = report.canonical();
        self.crypto.sign(&canonical)
            .map(|sig| sig.bytes.to_vec())
            .map_err(|e| format!("signing failed: {e}"))
    }

    fn host_publish_probity_report(&self, signed_bytes: Vec<u8>) {
        tracing::debug!(len = signed_bytes.len(), "communerd: probity report published (stub)");
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

    #[test]
    fn line_for_tbid_creates_and_returns_line() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([0u8; 96]);
        let line = communerd.line_for_tbid(tbid);
        assert_eq!(line.target_tbid(), tbid);
    }

    #[test]
    fn same_tbid_returns_shared_state() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([1u8; 96]);
        let line1 = communerd.line_for_tbid(tbid);
        let line2 = communerd.line_for_tbid(tbid);
        assert_eq!(line1.target_tbid(), line2.target_tbid());
    }

    #[test]
    fn try_line_for_tbid_returns_none_for_unknown() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([0u8; 96]);
        assert!(communerd.try_line_for_tbid(tbid).is_none());
    }

    #[test]
    fn try_line_for_tbid_returns_some_after_line_for_tbid() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([5u8; 96]);
        communerd.line_for_tbid(tbid);
        assert!(communerd.try_line_for_tbid(tbid).is_some());
    }

    #[test]
    fn different_tbids_get_different_lines() {
        let communerd = Communerd::new(make_config());
        let tbid_a = Tbid::from_raw([10u8; 96]);
        let tbid_b = Tbid::from_raw([20u8; 96]);
        let line_a = communerd.line_for_tbid(tbid_a);
        let line_b = communerd.line_for_tbid(tbid_b);
        assert_ne!(line_a.target_tbid(), line_b.target_tbid());
    }

    #[test]
    fn clone_shares_communerdette_registry() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([42u8; 96]);
        communerd.line_for_tbid(tbid);
        let c2 = communerd.clone();
        assert!(c2.try_line_for_tbid(tbid).is_some());
    }

    #[test]
    fn shutdown_relationships_cancels_all() {
        let communerd = Communerd::new(make_config());
        let tbid = Tbid::from_raw([100u8; 96]);
        let line = communerd.line_for_tbid(tbid);
        assert!(!line.shutdown_token().is_cancelled());

        communerd.shutdown_relationships();
        assert!(line.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_relationships_does_not_affect_new_lines() {
        let communerd = Communerd::new(make_config());
        communerd.shutdown_relationships();

        let tbid = Tbid::from_raw([200u8; 96]);
        let line = communerd.line_for_tbid(tbid);
        assert!(!line.shutdown_token().is_cancelled());
    }

    // ── Group 4b: peer-change callback ──────────────────────────────────────

    /// Captures `on_peer_change` invocations for assertions.
    #[derive(Default)]
    struct CaptureCallback {
        events: std::sync::Mutex<Vec<Vec<CorePeerAddr>>>,
    }

    impl PeerChangeCallback for CaptureCallback {
        fn on_peer_change(&self, peers: Vec<CorePeerAddr>) {
            self.events.lock().unwrap().push(peers);
        }
    }

    impl CaptureCallback {
        fn events(&self) -> Vec<Vec<CorePeerAddr>> {
            self.events.lock().unwrap().clone()
        }
    }

    #[tokio::test]
    async fn peer_change_callback_fires_on_add_peer() {
        let communerd = Communerd::new(make_config());
        let cb = Arc::new(CaptureCallback::default());
        communerd.set_peer_change_callback(Some(Arc::clone(&cb) as Arc<dyn PeerChangeCallback>));

        let starting = cb.events().len();
        communerd
            .add_peer(PeerAddr {
                json_rpc: "127.0.0.1:7777".into(),
                peer_id: None,
                last_seen_ns: 0,
            })
            .await;

        let events = cb.events();
        assert!(
            events.len() > starting,
            "add_peer must fire at least one peer-change event"
        );
        let latest = events.last().expect("at least one event");
        assert!(
            latest.iter().any(|p| p.json_rpc == "127.0.0.1:7777"),
            "snapshot must include the newly-added peer"
        );
    }

    #[tokio::test]
    async fn peer_change_callback_fires_on_remove_peer() {
        let communerd = Communerd::new(make_config());
        let new_peer = PeerAddr {
            json_rpc: "127.0.0.1:8888".into(),
            peer_id: None,
            last_seen_ns: 0,
        };
        communerd.add_peer(new_peer.clone()).await;

        let cb = Arc::new(CaptureCallback::default());
        communerd.set_peer_change_callback(Some(Arc::clone(&cb) as Arc<dyn PeerChangeCallback>));

        communerd.remove_peer(&new_peer).await;

        let events = cb.events();
        assert!(!events.is_empty(), "remove_peer must fire a peer-change event");
        let latest = events.last().expect("at least one event");
        assert!(
            !latest.iter().any(|p| p.json_rpc == "127.0.0.1:8888"),
            "snapshot after remove_peer must not include the removed peer"
        );
    }

    #[tokio::test]
    async fn peer_change_callback_can_be_cleared() {
        let communerd = Communerd::new(make_config());
        let cb = Arc::new(CaptureCallback::default());
        communerd.set_peer_change_callback(Some(Arc::clone(&cb) as Arc<dyn PeerChangeCallback>));

        // Clear the callback before any pool change.
        communerd.set_peer_change_callback(None);
        communerd
            .add_peer(PeerAddr {
                json_rpc: "127.0.0.1:9999".into(),
                peer_id: None,
                last_seen_ns: 0,
            })
            .await;

        assert!(
            cb.events().is_empty(),
            "cleared callback must not receive events"
        );
    }
}
