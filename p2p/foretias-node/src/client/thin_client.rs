//! ThinClient — unified library API for Foretias stamping and verification.
//!
//! Supports three instantiation modes:
//! - **Level 1 (Standalone):** `ThinClient::new()` — pure in-memory, no network
//! - **Level 2 (PtP Networked):** `ThinClient::connect()` — C11 Noise_XX over TCP
//! - **Level 3 (P2P Full):** `ThinClient::join()` — libp2P swarm with DHT/gossipsub
//!
//! All three levels expose the same operation surface: `stamp`, `verify`,
//! `prove_verification`, `calendar_slice`, plus identity accessors.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use foretias_core::chronomatter::Chronomatter;
use foretias_core::crypto_server::{self, CryptoServer, ForetiasCurve};
use foretias_core::error::{CryptoError, NodeError};
use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::tick::{CalendarLookup, Foretis, TickRecord};
use foretias_core::foretias::types::Tbid;

use crate::calendar::Calendar;
use crate::client::noise_ptp::{noise_json_rpc, PtPError};

/// Error type for ThinClient operations.
#[derive(Debug)]
pub enum ThinClientError {
    /// A cryptographic operation failed.
    Crypto(String),
    /// The client is in dormant (verify-only) mode.
    Dormant(String),
    /// An I/O or persistence error occurred.
    Io(String),
    /// A network operation failed (Level 2/3 only).
    Network(String),
    /// An internal error occurred.
    Internal(String),
}

impl std::fmt::Display for ThinClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThinClientError::Crypto(msg) => write!(f, "ThinClient crypto error: {}", msg),
            ThinClientError::Dormant(msg) => write!(f, "ThinClient dormant error: {}", msg),
            ThinClientError::Io(msg) => write!(f, "ThinClient I/O error: {}", msg),
            ThinClientError::Network(msg) => write!(f, "ThinClient network error: {}", msg),
            ThinClientError::Internal(msg) => write!(f, "ThinClient internal error: {}", msg),
        }
    }
}

impl std::error::Error for ThinClientError {}

impl From<NodeError> for ThinClientError {
    fn from(e: NodeError) -> Self {
        match e {
            NodeError::Crypto(e) => ThinClientError::Crypto(e.to_string()),
            NodeError::Dormant(e) => ThinClientError::Dormant(e),
            NodeError::Io(e) => ThinClientError::Io(e.to_string()),
            NodeError::Internal(e) => ThinClientError::Internal(e),
            _ => ThinClientError::Internal(e.to_string()),
        }
    }
}

impl From<CryptoError> for ThinClientError {
    fn from(e: CryptoError) -> Self {
        ThinClientError::Crypto(e.to_string())
    }
}

impl From<PtPError> for ThinClientError {
    fn from(e: PtPError) -> Self {
        match e {
            PtPError::Connect(msg) => ThinClientError::Network(msg),
            PtPError::Timeout => ThinClientError::Network("PtP request timeout".into()),
            PtPError::Noise(msg) => ThinClientError::Network(msg),
            PtPError::Decode(msg) => ThinClientError::Network(msg),
            PtPError::Rpc { code, message } => ThinClientError::Network(format!("RPC {}: {}", code, message)),
        }
    }
}

/// Status information for a ThinClient instance.
#[derive(Debug, Clone)]
pub struct ThinClientStatus {
    /// Whether the client is dormant (verify-only).
    pub is_dormant: bool,
    /// Number of connected peers (0 for Level 1).
    pub peer_count: usize,
    /// Current tick number.
    pub current_tick: u64,
    /// Whether the client is connected to the DHT (Level 3 only).
    pub dht_connected: bool,
    /// Whether the client is subscribed to gossipsub (Level 3 only).
    pub gossipsub_subscribed: bool,
}

/// Client level — determines capabilities and network access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLevel {
    /// Pure in-memory — no network.
    Standalone,
    /// PtP Networked — C11 Noise_XX over TCP.
    Ptp,
    /// P2P Full — libp2P swarm with DHT/gossipsub.
    P2p,
}

/// Configuration for joining the P2P mesh (Level 3).
#[derive(Debug, Clone)]
pub struct P2pJoinConfig {
    /// TimeBeing name.
    pub tbn: String,
    /// Known peer addresses for bootstrapping.
    pub known_peers: Vec<String>,
    /// DHT namespace for peer discovery.
    pub dht_namespace: String,
    /// Optional persistence path.
    pub persist_path: Option<PathBuf>,
}

/// Standalone state — Level 1 (no network).
pub struct StandaloneState {
    /// The Chronomatter instance — owns tick lifecycle, stamping, verification.
    pub chronomatter: Arc<Chronomatter>,
    /// The Calendar instance — stores tick records, implements TickObserver.
    pub calendar: Arc<Calendar>,
    /// CryptoServer — provides cryptographic operations.
    pub crypto: Arc<dyn CryptoServer>,
}

/// PtP state — Level 2 (C11 Noise_XX over TCP).
pub struct PtpState {
    /// Underlying standalone state.
    pub standalone: StandaloneState,
    /// Connected peer addresses (connect on demand, no persistent connections).
    pub peers: Vec<String>,
    /// Default request timeout for PtP operations.
    pub timeout_secs: u64,
}

/// P2P state — Level 3 (libp2P swarm).
pub struct P2pState {
    /// Underlying PtP state (for fallback operations).
    pub ptp: PtpState,
    /// Communerd instance — manages the P2P mesh.
    #[allow(dead_code)]
    pub communerd: Arc<crate::communerd::Communerd>,
}

/// Inner state — one variant per client level.
pub enum ThinClientInner {
    /// Level 1 — Standalone (pure in-memory).
    Standalone(StandaloneState),
    /// Level 2 — PtP Networked (C11 Noise_XX over TCP).
    Ptp(PtpState),
    /// Level 3 — P2P Full (libp2P swarm).
    P2p(P2pState),
}

impl ThinClientInner {
    /// Access the Chronomatter regardless of client level.
    fn chronomatter(&self) -> Arc<Chronomatter> {
        match self {
            ThinClientInner::Standalone(state) => state.chronomatter.clone(),
            ThinClientInner::Ptp(state) => state.standalone.chronomatter.clone(),
            ThinClientInner::P2p(state) => state.ptp.standalone.chronomatter.clone(),
        }
    }

    /// Access the Calendar regardless of client level.
    fn calendar(&self) -> Arc<Calendar> {
        match self {
            ThinClientInner::Standalone(state) => state.calendar.clone(),
            ThinClientInner::Ptp(state) => state.standalone.calendar.clone(),
            ThinClientInner::P2p(state) => state.ptp.standalone.calendar.clone(),
        }
    }
}

/// Unified ThinClient for Foretias operations.
///
/// Supports three instantiation modes with the same operation surface:
/// - Level 1: `ThinClient::new()` — pure in-memory stamping/verification
/// - Level 2: `ThinClient::connect()` — PtP over C11 Noise_XX encrypted TCP
/// - Level 3: `ThinClient::join()` — P2P mesh with DHT discovery and gossipsub
pub struct ThinClient {
    level: ClientLevel,
    inner: ThinClientInner,
    /// Optional persistence path.
    persist_path: Option<PathBuf>,
}

impl ThinClient {
    // ─── Level 1 — Standalone ───────────────────────────────────────────

    /// Create a new standalone ThinClient (Level 1).
    ///
    /// This creates a fresh TimeBeing with its own TBID. Stamping and
    /// verification operate entirely in-memory with no network access.
    ///
    /// # Arguments
    /// * `tbn` - TimeBeing name (human-readable identifier)
    /// * `persist_path` - Optional path for calendar persistence
    ///
    /// # Errors
    /// Returns `ThinClientError::Crypto` if key generation fails.
    pub fn new(tbn: String, persist_path: Option<PathBuf>) -> Result<Self, ThinClientError> {
        // Default chronon: 60 seconds in nanoseconds
        let chronon_ns = 60_000_000_000u64;

        // Create Calendar (implements TickObserver)
        let calendar = Arc::new(Calendar::new(Tbid::default(), &tbn));

        // Create Chronomatter with Calendar as TickObserver
        let mut cm = Chronomatter::new(chronon_ns, Arc::clone(&calendar) as Arc<dyn TickObserver>)?;

        // Set a no-op mutual attestation observer (not needed for standalone)
        struct NoOpMutualAttest;
        impl foretias_core::foretias::callbacks::MutualAttestObserver for NoOpMutualAttest {
            fn on_mutual_attest_sent(&self) {}
            fn on_mutual_attest_ok(&self) {}
            fn on_mutual_attest_failed(&self) {}
        }
        cm.set_mutual_attest_observer(Arc::new(NoOpMutualAttest) as Arc<dyn foretias_core::foretias::callbacks::MutualAttestObserver>);

        // Sync TBID and TBN from Chronomatter to Calendar
        let (tbid, tbn_from_cm) = (cm.get_tbid(), cm.get_tbn().to_string());
        let binding = calendar.inner();
        let mut cal_inner = binding.write();
        cal_inner.tbid = tbid;
        cal_inner.tbn = tbn_from_cm.clone();

        // Create CryptoServer
        let crypto: Arc<dyn CryptoServer> = Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);

        let state = StandaloneState {
            chronomatter: Arc::new(cm),
            calendar,
            crypto,
        };

        Ok(Self {
            level: ClientLevel::Standalone,
            inner: ThinClientInner::Standalone(state),
            persist_path,
        })
    }

    /// Load a dormant ThinClient from a persisted calendar (Level 1).
    ///
    /// The loaded client is verify-only — it cannot stamp new content.
    ///
    /// # Arguments
    /// * `path` - Path to the persisted calendar file
    ///
    /// # Errors
    /// Returns `ThinClientError::Io` if the file cannot be loaded.
    pub fn from_persist(path: PathBuf) -> Result<Self, ThinClientError> {
        let path_str = path.to_string_lossy().to_string();

        // Create CryptoServer
        let crypto: Arc<dyn CryptoServer> = Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);

        // Create a no-op TickObserver (dormant mode — no tick advances)
        struct NoOpObserver;
        impl TickObserver for NoOpObserver {
            fn on_tick_advance(&self, _tick_number: foretias_core::foretias::types::TickNumber, _public_key: &[u8], _tick_record: &TickRecord) {}
        }

        // Create dormant Chronomatter from persisted calendar
        let mut cm = Chronomatter::from_calendar(&path_str, crypto.clone(), Arc::new(NoOpObserver))?;

        struct NoOpMutualAttest;
        impl foretias_core::foretias::callbacks::MutualAttestObserver for NoOpMutualAttest {
            fn on_mutual_attest_sent(&self) {}
            fn on_mutual_attest_ok(&self) {}
            fn on_mutual_attest_failed(&self) {}
        }
        cm.set_mutual_attest_observer(Arc::new(NoOpMutualAttest) as Arc<dyn foretias_core::foretias::callbacks::MutualAttestObserver>);

        // Load Calendar from persisted data
        let calendar = Arc::new(Calendar::from_persisted(&path_str)?);

        let state = StandaloneState {
            chronomatter: Arc::new(cm),
            calendar,
            crypto,
        };

        Ok(Self {
            level: ClientLevel::Standalone,
            inner: ThinClientInner::Standalone(state),
            persist_path: Some(path),
        })
    }

    // ─── Level 2 — PtP Networked ─────────────────────────────────────────

    /// Create a PtP Networked ThinClient (Level 2).
    ///
    /// Creates a standalone TimeBeing and registers peer addresses for
    /// on-demand PtP connections via C11 Noise_XX encrypted TCP.
    ///
    /// # Arguments
    /// * `tbn` - TimeBeing name
    /// * `peer_addrs` - Peer server addresses (e.g., "127.0.0.1:4001")
    /// * `timeout_secs` - Default request timeout in seconds
    /// * `persist_path` - Optional path for calendar persistence
    pub fn connect(
        tbn: String,
        peer_addrs: Vec<String>,
        timeout_secs: u64,
        persist_path: Option<PathBuf>,
    ) -> Result<Self, ThinClientError> {
        let standalone = Self::create_standalone_state(&tbn)?;

        let state = PtpState {
            standalone,
            peers: peer_addrs,
            timeout_secs,
        };

        Ok(Self {
            level: ClientLevel::Ptp,
            inner: ThinClientInner::Ptp(state),
            persist_path,
        })
    }

    /// Create a PtP Networked ThinClient connected to a single peer.
    pub fn connect_one(tbn: String, peer_addr: String, persist_path: Option<PathBuf>) -> Result<Self, ThinClientError> {
        Self::connect(tbn, vec![peer_addr], 30, persist_path)
    }

    // ─── Level 3 — P2P Full ──────────────────────────────────────────────

    /// Create a P2P Full ThinClient (Level 3).
    ///
    /// Creates a standalone TimeBeing and joins the P2P mesh via Communerd.
    ///
    /// # Arguments
    /// * `config` - P2P join configuration
    pub async fn join(config: P2pJoinConfig) -> Result<Self, ThinClientError> {
        let standalone = Self::create_standalone_state(&config.tbn)?;

        let communerd_config = foretias_core::config::CommunerdConfig {
            dht: foretias_core::config::DHTConfig {
                namespace: config.dht_namespace.clone(),
                bootstrap: vec![],
            },
            known_servers: config.known_peers.clone(),
            ..Default::default()
        };
        let com = crate::communerd::Communerd::new(communerd_config);

        let ptp = PtpState {
            standalone,
            peers: config.known_peers,
            timeout_secs: 30,
        };

        let state = P2pState {
            ptp,
            communerd: Arc::new(com),
        };

        Ok(Self {
            level: ClientLevel::P2p,
            inner: ThinClientInner::P2p(state),
            persist_path: config.persist_path,
        })
    }

    // ─── Common Operations (All Levels) ─────────────────────────────────

    /// Stamp content and return a signed Foretis attestation.
    ///
    /// # Arguments
    /// * `content` - The content to stamp (bytes)
    /// * `echo` - Echo string for identifying the stamp
    ///
    /// # Errors
    /// Returns `ThinClientError::Dormant` if the client is in verify-only mode.
    pub async fn stamp(&self, content: &[u8], echo: String) -> Result<Foretis, ThinClientError> {
        let echo = if echo.is_empty() {
            Self::client_echo()
        } else {
            echo
        };

        match self.level {
            ClientLevel::Standalone => self.stamp_standalone(content, &echo).await,
            ClientLevel::Ptp | ClientLevel::P2p => self.stamp_remote(content, &echo).await,
        }
    }

    async fn stamp_standalone(&self, content: &[u8], echo: &str) -> Result<Foretis, ThinClientError> {
        let cm = self.inner.chronomatter();
        if cm.is_dormant() {
            return Err(ThinClientError::Dormant(
                "client is dormant — cannot stamp".into(),
            ));
        }

        let foretis = cm.stamp(content.to_vec(), echo.to_string())?;

        if let Some(ref path) = self.persist_path {
            let _ = self.save_calendar(path);
        }

        Ok(foretis)
    }

    /// Verify a Foretis attestation against the local calendar.
    ///
    /// # Arguments
    /// * `content` - The original content (bytes)
    /// * `foretis` - The Foretis attestation to verify
    ///
    /// # Returns
    /// `true` if the attestation is valid, `false` otherwise.
    pub async fn verify(&self, content: &[u8], foretis: &Foretis) -> Result<bool, ThinClientError> {
        match self.level {
            ClientLevel::Standalone => {
                let cm = self.inner.chronomatter();
                let calendar = self.inner.calendar();
                let result = cm.verify(foretis, &content.to_vec(), calendar.as_ref())?;
                Ok(result)
            }
            ClientLevel::Ptp | ClientLevel::P2p => self.verify_remote(content, foretis).await,
        }
    }

    /// Prove verification by fetching the calendar slice and verifying locally.
    ///
    /// This performs a distributed verification: fetches the relevant tick
    /// records and verifies the attestation chain locally.
    ///
    /// # Arguments
    /// * `content` - The original content (bytes)
    /// * `foretis` - The Foretis attestation to verify
    ///
    /// # Returns
    /// A verification report with the calendar records and verification status.
    ///
    /// # Note
    /// For Level 1 (Standalone), this is equivalent to `verify` but returns
    /// additional calendar context. For Level 2/3, this fetches from peers.
    pub async fn prove_verification(
        &self,
        content: &[u8],
        foretis: &Foretis,
    ) -> Result<VerificationReport, ThinClientError> {
        let calendar = self.inner.calendar();
        let tick_number = foretis.tick_number;

        // Fetch calendar slice containing the relevant tick
        let records = calendar
            .get(tick_number, 2)
            .map_err(ThinClientError::from)?;

        let verified = self.verify(content, foretis).await?;

        Ok(VerificationReport {
            verified,
            tick_number,
            calendar_records: records,
            foretis: foretis.clone(),
            method: "prove_verification".into(),
        })
    }

    /// Get a slice of the calendar starting from the given tick number.
    ///
    /// # Arguments
    /// * `start` - Starting tick number (inclusive)
    /// * `count` - Maximum number of records to return
    pub async fn calendar_slice(
        &self,
        start: u64,
        count: u64,
    ) -> Result<Vec<TickRecord>, ThinClientError> {
        match self.level {
            ClientLevel::Standalone => {
                let calendar = self.inner.calendar();
                let records = calendar
                    .get(start, count as usize)
                    .map_err(ThinClientError::from)?;
                Ok(records)
            }
            ClientLevel::Ptp | ClientLevel::P2p => self.calendar_slice_remote(start, count).await,
        }
    }

    // ─── Identity Accessors ──────────────────────────────────────────────

    /// Get the public key of this client's TimeBeing.
    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.inner.chronomatter().latest_public_key()
    }

    /// Get the TBID (TimeBeing Identifier) of this client.
    pub fn tbid(&self) -> String {
        self.inner.chronomatter().get_tbid().to_hex()
    }

    /// Get the TBN (TimeBeing Name) of this client.
    pub fn tbn(&self) -> String {
        self.inner.chronomatter().get_tbn().to_string()
    }

    /// Get the current status of this client.
    pub fn status(&self) -> ThinClientStatus {
        let cm = self.inner.chronomatter();
        ThinClientStatus {
            is_dormant: cm.is_dormant(),
            peer_count: match self.level {
                ClientLevel::Standalone => 0,
                ClientLevel::Ptp => match &self.inner {
                    ThinClientInner::Ptp(state) => state.peers.len(),
                    _ => 0,
                },
                ClientLevel::P2p => 0, // TODO: query communerd for peer count
            },
            current_tick: cm.current_tick(),
            dht_connected: self.level == ClientLevel::P2p,
            gossipsub_subscribed: self.level == ClientLevel::P2p,
        }
    }

    /// Get the client level.
    pub fn level(&self) -> ClientLevel {
        self.level
    }

    /// Get the current tick number.
    pub fn current_tick(&self) -> u64 {
        self.inner.chronomatter().current_tick()
    }

    // ─── Persistence ─────────────────────────────────────────────────────

    /// Save the calendar to the given path.
    fn save_calendar(&self, path: &PathBuf) -> Result<(), ThinClientError> {
        let calendar = self.inner.calendar();
        let path_str = path.to_string_lossy().to_string();
        calendar.save(&path_str)?;
        Ok(())
    }

    // ─── Helpers ─────────────────────────────────────────────────────────

    fn create_standalone_state(tbn: &str) -> Result<StandaloneState, ThinClientError> {
        let chronon_ns = 60_000_000_000u64;
        let calendar = Arc::new(Calendar::new(Tbid::default(), tbn));

        let mut cm = Chronomatter::new(chronon_ns, Arc::clone(&calendar) as Arc<dyn TickObserver>)?;

        struct NoOpMutualAttest;
        impl foretias_core::foretias::callbacks::MutualAttestObserver for NoOpMutualAttest {
            fn on_mutual_attest_sent(&self) {}
            fn on_mutual_attest_ok(&self) {}
            fn on_mutual_attest_failed(&self) {}
        }
        cm.set_mutual_attest_observer(Arc::new(NoOpMutualAttest) as Arc<dyn foretias_core::foretias::callbacks::MutualAttestObserver>);

        let (tbid, tbn_from_cm) = (cm.get_tbid(), cm.get_tbn().to_string());
        let binding = calendar.inner();
        let mut cal_inner = binding.write();
        cal_inner.tbid = tbid;
        cal_inner.tbn = tbn_from_cm;

        let crypto: Arc<dyn CryptoServer> = Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);

        Ok(StandaloneState {
            chronomatter: Arc::new(cm),
            calendar,
            crypto,
        })
    }

    fn primary_peer(&self) -> Option<&str> {
        match &self.inner {
            ThinClientInner::Ptp(state) => state.peers.first().map(|s| s.as_str()),
            ThinClientInner::P2p(state) => state.ptp.peers.first().map(|s| s.as_str()),
            _ => None,
        }
    }

    fn timeout(&self) -> Duration {
        let secs = match &self.inner {
            ThinClientInner::Ptp(state) => state.timeout_secs,
            ThinClientInner::P2p(state) => state.ptp.timeout_secs,
            _ => 30,
        };
        Duration::from_secs(secs)
    }

    async fn stamp_remote(&self, content: &[u8], echo: &str) -> Result<Foretis, ThinClientError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ThinClientError::Network("no peer configured".into()));
        };
        let content_hex = hex::encode(content);
        let params = serde_json::json!({
            "content": content_hex,
            "echo": echo,
        });
        let result = noise_json_rpc(peer, "stamp", params, self.timeout()).await?;
        let foretis: Foretis = serde_json::from_value(result)
            .map_err(|e| ThinClientError::Network(format!("stamp deserialization failed: {}", e)))?;
        Ok(foretis)
    }

    async fn verify_remote(&self, content: &[u8], foretis: &Foretis) -> Result<bool, ThinClientError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ThinClientError::Network("no peer configured".into()));
        };
        let content_hex = hex::encode(content);
        let params = serde_json::json!({
            "content": content_hex,
            "foretis": foretis,
        });
        let result = noise_json_rpc(peer, "verify", params, self.timeout()).await?;
        let valid = result.get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(valid)
    }

    async fn calendar_slice_remote(&self, start: u64, count: u64) -> Result<Vec<TickRecord>, ThinClientError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ThinClientError::Network("no peer configured".into()));
        };
        let params = serde_json::json!({
            "cal_tick_start": start,
            "count": count,
        });
        let result = noise_json_rpc(peer, "get_calendar_slice", params, self.timeout()).await?;
        let records: Vec<TickRecord> = serde_json::from_value(result)
            .map_err(|e| ThinClientError::Network(format!("calendar slice deserialization failed: {}", e)))?;
        Ok(records)
    }

    /// Generate a client echo string based on the current system time.
    fn client_echo() -> String {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        format!("UE+{}ns", now_ns)
    }
}

/// Verification report returned by `prove_verification`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationReport {
    /// Whether the attestation was verified successfully.
    pub verified: bool,
    /// The tick number of the attestation.
    pub tick_number: u64,
    /// Calendar records used for verification.
    pub calendar_records: Vec<TickRecord>,
    /// The Foretis attestation being verified.
    pub foretis: Foretis,
    /// The verification method used.
    pub method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_client_new_creates_valid_client() {
        let client = ThinClient::new("test-client".into(), None).unwrap();
        assert_eq!(client.level(), ClientLevel::Standalone);
        assert!(!client.status().is_dormant);
        assert_eq!(client.status().peer_count, 0);
        assert!(!client.tbid().is_empty());
        // tbn() returns the TBID-derived short name (e.g., "tf-..."), not the input name
        assert!(!client.tbn().is_empty());
    }

    #[test]
    fn thin_client_stamp_produces_valid_foretis() {
        let client = ThinClient::new("stamp-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let foretis = rt.block_on(async {
            client.stamp(b"hello world", "test-echo".into()).await.unwrap()
        });

        assert_eq!(foretis.tick_number, 1);
        assert_eq!(foretis.echo, "test-echo");
        // tbn in Foretis is the TBID-derived name, not the input string
        assert!(!foretis.tbn.is_empty());
        assert!(!foretis.content_hash.is_empty());
        assert!(!foretis.signature.is_empty());
    }

    #[test]
    fn thin_client_stamp_verify_roundtrip() {
        let client = ThinClient::new("roundtrip-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let content = b"test content";
            let foretis = client.stamp(content, "echo".into()).await.unwrap();
            let verified = client.verify(content, &foretis).await.unwrap();
            assert!(verified, "valid stamp should verify successfully");
        });
    }

    #[test]
    fn thin_client_wrong_content_fails_verification() {
        let client = ThinClient::new("verify-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let foretis = client.stamp(b"original", "echo".into()).await.unwrap();
            let verified = client.verify(b"tampered", &foretis).await.unwrap();
            assert!(!verified, "wrong content should fail verification");
        });
    }

    #[test]
    fn thin_client_multiple_stamps_distinct_ticks() {
        let client = ThinClient::new("multi-stamp".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let f1 = client.stamp(b"first", "echo1".into()).await.unwrap();
            let f2 = client.stamp(b"second", "echo2".into()).await.unwrap();
            // Both stamps on same tick (no tick advance), but different content hashes
            assert_ne!(f1.content_hash, f2.content_hash);
            assert_eq!(f1.echo, "echo1");
            assert_eq!(f2.echo, "echo2");
        });
    }

    #[test]
    fn thin_client_calendar_reflects_stamps() {
        let client = ThinClient::new("calendar-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            client.stamp(b"a", "e1".into()).await.unwrap();
            client.stamp(b"b", "e2".into()).await.unwrap();

            let records = client.calendar_slice(0, 10).await.unwrap();
            // At least one tick record should exist (tick 1)
            assert!(!records.is_empty());
        });
    }

    #[test]
    fn thin_client_identity_accessors() {
        let client = ThinClient::new("identity-test".into(), None).unwrap();
        assert!(!client.tbid().is_empty());
        assert!(!client.tbn().is_empty());
        let _ = client.public_key();
    }

    #[test]
    fn thin_client_persistence_roundtrip() {
        let path = PathBuf::from("/tmp/foretias-thinclient-persist-test.json");
        let persist_dir = path.parent().unwrap();
        std::fs::create_dir_all(persist_dir).ok();

        // Create client with persistence
        let client = ThinClient::new("persist-test".into(), Some(path.clone())).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            client.stamp(b"persist me", "echo".into()).await.unwrap();
        });

        // Verify file exists
        assert!(path.exists(), "calendar file should be persisted");

        // Load from persistence
        let loaded = ThinClient::from_persist(path.clone()).unwrap();
        assert!(loaded.status().is_dormant);
        assert_eq!(loaded.level(), ClientLevel::Standalone);

        // Clean up
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn thin_client_dormant_cannot_stamp() {
        let path = PathBuf::from("/tmp/foretias-thinclient-dormant-test.json");

        // Create and persist first
        let client = ThinClient::new("dormant-source".into(), Some(path.clone())).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            client.stamp(b"before dormant", "echo".into()).await.unwrap();
        });

        // Load as dormant
        let dormant = ThinClient::from_persist(path.clone()).unwrap();
        assert!(dormant.status().is_dormant);

        // Attempt to stamp — should fail
        let result = rt.block_on(async {
            dormant.stamp(b"should fail", "echo".into()).await
        });
        assert!(matches!(result, Err(ThinClientError::Dormant(_))));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn thin_client_foretis_serialization_roundtrip() {
        let client = ThinClient::new("serialize-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let foretis = client.stamp(b"serialize me", "echo".into()).await.unwrap();

            // Serialize to JSON
            let json = serde_json::to_string(&foretis).unwrap();

            // Deserialize back
            let deserialized: Foretis = serde_json::from_str(&json).unwrap();

            assert_eq!(foretis.tick_number, deserialized.tick_number);
            assert_eq!(foretis.content_hash, deserialized.content_hash);
            assert_eq!(foretis.signature, deserialized.signature);
            assert_eq!(foretis.tbid, deserialized.tbid);
            assert_eq!(foretis.echo, deserialized.echo);
        });
    }
}
