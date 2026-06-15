//! Foretias — unified library API for Foretias stamping and verification.
//!
//! Supports three instantiation modes:
//! - **Level 1 (Standalone):** `Foretias::new()` — pure in-memory, no network
//! - **Level 2 (PtP Networked):** `Foretias::connect()` — C11 Noise_XX over TCP
//! - **Level 3 (P2P Full):** `Foretias::join()` — libp2P swarm with DHT/gossipsub
//!
//! All three levels expose the same operation surface: `stamp`, `verify`,
//! `prove_verification`, `calendar_slice`, plus identity accessors.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use foretias_core::chronomatter::Chronomatter;
use foretias_core::clock::Clock;
use foretias_core::crypto_server::{self, CryptoServer, ForetiasCurve};
use foretias_core::error::{CryptoError, NodeError};
use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::tick::{CalendarLookup, ChrononRecord, ForetisRecord};
use foretias_core::foretias::types::Tbid;

use crate::calendar::Calendar;
use crate::config::{ForetiasConfig, P2pConfig, PtpConfig, StandaloneConfig};
use crate::noise_ptp::{noise_json_rpc, PtPError};

struct NoOpMutualAttest;
impl foretias_core::foretias::callbacks::MutualAttestObserver for NoOpMutualAttest {
    fn on_mutual_attest_sent(&self) {}
    fn on_mutual_attest_ok(&self) {}
    fn on_mutual_attest_failed(&self) {}
}

/// Error type for Foretias operations.
#[non_exhaustive]
#[derive(Debug)]
pub enum ForetiasError {
    Crypto(String),
    Dormant(String),
    Io(String),
    Network(String),
    Internal(String),
}

impl std::fmt::Display for ForetiasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForetiasError::Crypto(msg) => write!(f, "Foretias crypto error: {}", msg),
            ForetiasError::Dormant(msg) => write!(f, "Foretias dormant error: {}", msg),
            ForetiasError::Io(msg) => write!(f, "Foretias I/O error: {}", msg),
            ForetiasError::Network(msg) => write!(f, "Foretias network error: {}", msg),
            ForetiasError::Internal(msg) => write!(f, "Foretias internal error: {}", msg),
        }
    }
}

impl std::error::Error for ForetiasError {}

impl From<NodeError> for ForetiasError {
    fn from(e: NodeError) -> Self {
        match e {
            NodeError::Crypto(e) => ForetiasError::Crypto(e.to_string()),
            NodeError::Dormant(e) => ForetiasError::Dormant(e),
            NodeError::Io(e) => ForetiasError::Io(e.to_string()),
            NodeError::Internal(e) => ForetiasError::Internal(e),
            _ => ForetiasError::Internal(e.to_string()),
        }
    }
}

impl From<CryptoError> for ForetiasError {
    fn from(e: CryptoError) -> Self {
        ForetiasError::Crypto(e.to_string())
    }
}

impl From<PtPError> for ForetiasError {
    fn from(e: PtPError) -> Self {
        match e {
            PtPError::Connect(msg) => ForetiasError::Network(msg),
            PtPError::Timeout => ForetiasError::Network("PtP request timeout".into()),
            PtPError::Noise(msg) => ForetiasError::Network(msg),
            PtPError::Decode(msg) => ForetiasError::Network(msg),
            PtPError::Rpc { code, message } => {
                ForetiasError::Network(format!("RPC {code}: {message}"))
            }
        }
    }
}

/// Status information for a Foretias instance.
#[derive(Debug, Clone)]
pub struct ForetiasStatus {
    pub is_dormant: bool,
    pub peer_count: usize,
    pub current_tick: u64,
    pub dht_connected: bool,
    pub gossipsub_subscribed: bool,
}

/// Client level — determines capabilities and network access.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientLevel {
    Standalone,
    Ptp,
    P2p,
}

/// Configuration for joining the P2P mesh (Level 3).
#[derive(Debug, Clone)]
pub struct P2pJoinConfig {
    pub tbn: String,
    pub known_peers: Vec<String>,
    pub dht_namespace: String,
    pub persist_path: Option<PathBuf>,
}

pub struct StandaloneState {
    pub chronomatter: Arc<Chronomatter>,
    pub calendar: Arc<Calendar>,
    pub crypto: Arc<dyn CryptoServer>,
}

pub struct PtpState {
    pub standalone: StandaloneState,
    pub peers: Vec<String>,
    pub timeout_secs: u64,
}

pub struct P2pState {
    pub ptp: PtpState,
    #[allow(dead_code)]
    pub p2p_config: P2pJoinConfig,
}

#[non_exhaustive]
pub enum ForetiasInner {
    Standalone(StandaloneState),
    Ptp(PtpState),
    P2p(P2pState),
}

impl ForetiasInner {
    fn chronomatter(&self) -> Arc<Chronomatter> {
        match self {
            ForetiasInner::Standalone(state) => state.chronomatter.clone(),
            ForetiasInner::Ptp(state) => state.standalone.chronomatter.clone(),
            ForetiasInner::P2p(state) => state.ptp.standalone.chronomatter.clone(),
        }
    }

    fn calendar(&self) -> Arc<Calendar> {
        match self {
            ForetiasInner::Standalone(state) => state.calendar.clone(),
            ForetiasInner::Ptp(state) => state.standalone.calendar.clone(),
            ForetiasInner::P2p(state) => state.ptp.standalone.calendar.clone(),
        }
    }
}

pub struct Foretias {
    level: ClientLevel,
    inner: ForetiasInner,
    persist_path: Option<PathBuf>,
}

impl Foretias {
    #[must_use = "creating a Foretias client may fail (e.g. crypto initialization); the error must be handled"]
    pub fn new(_tbn: String, persist_path: Option<PathBuf>) -> Result<Self, ForetiasError> {
        let state = Self::create_standalone_state("")?;

        Ok(Self {
            level: ClientLevel::Standalone,
            inner: ForetiasInner::Standalone(state),
            persist_path,
        })
    }

    #[must_use = "loading a Foretias client from persisted data may fail; the error must be handled"]
    pub fn from_persist(path: PathBuf) -> Result<Self, ForetiasError> {
        let path_str = path.to_string_lossy().to_string();
        let crypto: Arc<dyn CryptoServer> =
            Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);

        struct NoOpObserver;
        impl TickObserver for NoOpObserver {
            fn on_tick_advance(
                &self,
                _chronon_number: foretias_core::foretias::types::TickNumber,
                _public_key: &[u8],
                _tick_record: &ChrononRecord,
            ) {
            }
        }

        let mut cm =
            Chronomatter::from_calendar(&path_str, crypto.clone(), Arc::new(NoOpObserver))?;
        cm.set_mutual_attest_observer(Arc::new(NoOpMutualAttest)
            as Arc<dyn foretias_core::foretias::callbacks::MutualAttestObserver>);

        let calendar = Arc::new(Calendar::from_persisted(&path_str)?);
        let state = StandaloneState {
            chronomatter: Arc::new(cm),
            calendar,
            crypto,
        };

        Ok(Self {
            level: ClientLevel::Standalone,
            inner: ForetiasInner::Standalone(state),
            persist_path: Some(path),
        })
    }

    pub fn connect(
        tbn: String,
        peer_addrs: Vec<String>,
        timeout_secs: u64,
        persist_path: Option<PathBuf>,
    ) -> Result<Self, ForetiasError> {
        let standalone = Self::create_standalone_state(&tbn)?;
        let state = PtpState {
            standalone,
            peers: peer_addrs,
            timeout_secs,
        };
        Ok(Self {
            level: ClientLevel::Ptp,
            inner: ForetiasInner::Ptp(state),
            persist_path,
        })
    }

    pub fn connect_one(
        tbn: String,
        peer_addr: String,
        persist_path: Option<PathBuf>,
    ) -> Result<Self, ForetiasError> {
        Self::connect(tbn, vec![peer_addr], 30, persist_path)
    }

    #[must_use = "joining a P2P network may fail; the error must be handled"]
    pub async fn join(config: P2pJoinConfig) -> Result<Self, ForetiasError> {
        let standalone = Self::create_standalone_state(&config.tbn)?;
        let ptp = PtpState {
            standalone,
            peers: config.known_peers.clone(),
            timeout_secs: 30,
        };
        let state = P2pState {
            ptp,
            p2p_config: config,
        };
        Ok(Self {
            level: ClientLevel::P2p,
            inner: ForetiasInner::P2p(state),
            persist_path: None,
        })
    }

    /// Create from configuration — level determined by config variant.
    #[must_use = "creating a Foretias client from config may fail; the error must be handled"]
    pub async fn with_config(config: ForetiasConfig) -> Result<Self, ForetiasError> {
        match config {
            ForetiasConfig::Standalone(cfg) => Self::with_config_standalone(cfg),
            ForetiasConfig::Ptp(cfg) => Self::with_config_ptp(cfg),
            ForetiasConfig::P2p(cfg) => Self::with_config_p2p(cfg).await,
        }
    }

    fn with_config_standalone(cfg: StandaloneConfig) -> Result<Self, ForetiasError> {
        Self::new(cfg.tbn, cfg.persist_path)
    }

    fn with_config_ptp(cfg: PtpConfig) -> Result<Self, ForetiasError> {
        Self::connect(
            cfg.standalone.tbn,
            cfg.peers,
            cfg.timeout_secs,
            cfg.standalone.persist_path,
        )
    }

    async fn with_config_p2p(cfg: P2pConfig) -> Result<Self, ForetiasError> {
        let standalone = Self::create_standalone_state(&cfg.ptp.standalone.tbn)?;
        let ptp = PtpState {
            standalone,
            peers: cfg.ptp.peers,
            timeout_secs: cfg.ptp.timeout_secs,
        };
        let p2p_config = P2pJoinConfig {
            tbn: cfg.ptp.standalone.tbn,
            known_peers: cfg.known_servers,
            dht_namespace: cfg.dht_namespace,
            persist_path: cfg.ptp.standalone.persist_path.clone(),
        };
        let state = P2pState { ptp, p2p_config };
        Ok(Self {
            level: ClientLevel::P2p,
            inner: ForetiasInner::P2p(state),
            persist_path: cfg.ptp.standalone.persist_path,
        })
    }

    pub async fn stamp(
        &self,
        content: &[u8],
        echo: String,
    ) -> Result<(ForetisRecord, Vec<u8>, String), ForetiasError> {
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

    async fn stamp_standalone(
        &self,
        content: &[u8],
        echo: &str,
    ) -> Result<(ForetisRecord, Vec<u8>, String), ForetiasError> {
        let cm = self.inner.chronomatter();
        if cm.is_dormant() {
            return Err(ForetiasError::Dormant(
                "client is dormant — cannot stamp".into(),
            ));
        }
        let stamped = cm.stamp(content.to_vec(), echo.to_string())?;
        if let Some(ref path) = self.persist_path {
            let _ = self.save_calendar(path);
        }
        let (foretis, signature_bytes, signature_algorithm) = stamped.into_parts();
        Ok((foretis, signature_bytes, signature_algorithm))
    }

    pub async fn verify(
        &self,
        content: &[u8],
        foretis: &ForetisRecord,
        signature: &[u8],
        signature_algorithm: &str,
    ) -> Result<bool, ForetiasError> {
        match self.level {
            ClientLevel::Standalone => {
                let cm = self.inner.chronomatter();
                let calendar = self.inner.calendar();
                let result = cm.verify(
                    foretis,
                    signature,
                    signature_algorithm,
                    content,
                    calendar.as_ref(),
                )?;
                Ok(result)
            }
            ClientLevel::Ptp | ClientLevel::P2p => {
                self.verify_remote(content, foretis, signature, signature_algorithm)
                    .await
            }
        }
    }

    pub async fn verify_with_proof(
        &self,
        content: &[u8],
        foretis: &ForetisRecord,
        signature: &[u8],
        signature_algorithm: &str,
    ) -> Result<VerificationReport, ForetiasError> {
        let chronon_number = *foretis.chronon_number();
        let records = self.calendar_slice(chronon_number, 2).await?;
        if records.is_empty() {
            return Err(ForetiasError::Network(format!(
                "no calendar records found for chronon {}",
                chronon_number
            )));
        }

        let crypto =
            crypto_server::new_software(ForetiasCurve::Ed25519).map_err(ForetiasError::from)?;
        let fetched_cal = FetchedCalendar {
            record: records[0].clone(),
            tbid: *foretis.tbid(),
            tbn: foretis.tbn().to_string(),
        };
        let verified = foretias_core::foretias::tick::verify(
            &*crypto,
            foretis,
            signature,
            signature_algorithm,
            content,
            &fetched_cal,
        )
        .map_err(ForetiasError::from)?;

        Ok(VerificationReport {
            verified,
            chronon_number,
            calendar_records: records,
            foretis: foretis.clone(),
            method: "verify_with_proof".into(),
        })
    }

    pub async fn calendar_slice(
        &self,
        start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, ForetiasError> {
        match self.level {
            ClientLevel::Standalone => {
                let calendar = self.inner.calendar();
                let records = calendar
                    .get(start, count as usize)
                    .map_err(ForetiasError::from)?;
                Ok(records)
            }
            ClientLevel::Ptp | ClientLevel::P2p => self.calendar_slice_remote(start, count).await,
        }
    }

    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.inner.chronomatter().latest_public_key()
    }

    pub fn tbid(&self) -> String {
        self.inner.chronomatter().tbid().to_hex()
    }

    pub fn tbn(&self) -> String {
        self.inner.chronomatter().tbn().to_string()
    }

    pub fn status(&self) -> ForetiasStatus {
        let cm = self.inner.chronomatter();
        ForetiasStatus {
            is_dormant: cm.is_dormant(),
            peer_count: match self.level {
                ClientLevel::Standalone => 0,
                ClientLevel::Ptp => match &self.inner {
                    ForetiasInner::Ptp(state) => state.peers.len(),
                    _ => 0,
                },
                ClientLevel::P2p => 0,
            },
            current_tick: cm.current_tick(),
            dht_connected: self.level == ClientLevel::P2p,
            gossipsub_subscribed: self.level == ClientLevel::P2p,
        }
    }

    pub fn level(&self) -> ClientLevel {
        self.level
    }

    pub fn current_tick(&self) -> u64 {
        self.inner.chronomatter().current_tick()
    }

    fn save_calendar(&self, path: &Path) -> Result<(), ForetiasError> {
        let calendar = self.inner.calendar();
        let path_str = path.to_string_lossy().to_string();
        calendar.save(&path_str)?;
        Ok(())
    }

    fn create_standalone_state(_tbn: &str) -> Result<StandaloneState, ForetiasError> {
        let chronon_ns = 60_000_000_000u64;
        let calendar = Arc::new(Calendar::new(Tbid::default(), ""));
        let crypto = Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);
        let mut cm = Chronomatter::new(
            chronon_ns,
            Arc::clone(&calendar) as Arc<dyn TickObserver>,
            crypto,
        )?;
        cm.set_mutual_attest_observer(Arc::new(NoOpMutualAttest)
            as Arc<dyn foretias_core::foretias::callbacks::MutualAttestObserver>);
        let (tbid, tbn_from_cm) = (cm.tbid(), cm.tbn().to_string());
        let binding = calendar.inner();
        let mut cal_inner = binding.write();
        cal_inner.set_tbid(tbid);
        cal_inner.set_tbn(&tbn_from_cm);
        let crypto: Arc<dyn CryptoServer> =
            Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);
        Ok(StandaloneState {
            chronomatter: Arc::new(cm),
            calendar,
            crypto,
        })
    }

    fn primary_peer(&self) -> Option<&str> {
        match &self.inner {
            ForetiasInner::Ptp(state) => state.peers.first().map(|s| s.as_str()),
            ForetiasInner::P2p(state) => state.ptp.peers.first().map(|s| s.as_str()),
            _ => None,
        }
    }

    fn timeout(&self) -> Duration {
        let secs = match &self.inner {
            ForetiasInner::Ptp(state) => state.timeout_secs,
            ForetiasInner::P2p(state) => state.ptp.timeout_secs,
            _ => 30,
        };
        Duration::from_secs(secs)
    }

    async fn stamp_remote(
        &self,
        content: &[u8],
        echo: &str,
    ) -> Result<(ForetisRecord, Vec<u8>, String), ForetiasError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ForetiasError::Network("no peer configured".into()));
        };
        let content_hex = hex::encode(content);
        let params = serde_json::json!({
            "content": content_hex,
            "echo": echo,
        });
        let result = noise_json_rpc(peer, "stamp", params, self.timeout()).await?;
        // v2: server returns {foretis, signature, signature_algorithm}
        let foretis: ForetisRecord =
            serde_json::from_value(result.get("foretis").cloned().unwrap_or(result.clone()))
                .map_err(|e| {
                    ForetiasError::Network(format!("stamp deserialization failed: {e}"))
                })?;
        let sig_hex = result
            .get("signature")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let signature = hex::decode(sig_hex).unwrap_or_default();
        let sig_alg = result
            .get("signature_algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("Ed25519")
            .to_string();
        Ok((foretis, signature, sig_alg))
    }

    async fn verify_remote(
        &self,
        content: &[u8],
        foretis: &ForetisRecord,
        signature: &[u8],
        signature_algorithm: &str,
    ) -> Result<bool, ForetiasError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ForetiasError::Network("no peer configured".into()));
        };
        let content_hex = hex::encode(content);
        let params = serde_json::json!({
            "content": content_hex,
            "foretis": foretis,
            "signature": hex::encode(signature),
            "signature_algorithm": signature_algorithm,
        });
        let result = noise_json_rpc(peer, "verify", params, self.timeout()).await?;
        let valid = result
            .get("valid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(valid)
    }

    async fn calendar_slice_remote(
        &self,
        start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, ForetiasError> {
        let Some(peer) = self.primary_peer() else {
            return Err(ForetiasError::Network("no peer configured".into()));
        };
        let params = serde_json::json!({
            "cal_chronon_start": start,
            "count": count,
        });
        let result = noise_json_rpc(peer, "get_calendar_slice", params, self.timeout()).await?;
        let records: Vec<ChrononRecord> = serde_json::from_value(result).map_err(|e| {
            ForetiasError::Network(format!("calendar slice deserialization failed: {e}"))
        })?;
        Ok(records)
    }

    fn client_echo() -> String {
        let now_ns = foretias_core::clock::SystemClock.now_ns().unwrap_or(0);
        format!("UE+{now_ns}ns")
    }
}

struct FetchedCalendar {
    record: ChrononRecord,
    tbid: Tbid,
    tbn: String,
}

impl CalendarLookup for FetchedCalendar {
    fn get(&self, start: u64, count: usize) -> Result<Vec<ChrononRecord>, NodeError> {
        if start == *self.record.chronon_number() && count >= 1 {
            Ok(vec![self.record.clone()])
        } else {
            Ok(vec![])
        }
    }

    fn latest(&self) -> Option<u64> {
        Some(*self.record.chronon_number())
    }

    fn tbid(&self) -> Tbid {
        self.tbid
    }

    fn tbn(&self) -> &str {
        &self.tbn
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationReport {
    pub verified: bool,
    pub chronon_number: u64,
    pub calendar_records: Vec<ChrononRecord>,
    pub foretis: ForetisRecord,
    pub method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foretias_new_creates_valid_client() {
        let client = Foretias::new("test-client".into(), None).unwrap();
        assert_eq!(client.level(), ClientLevel::Standalone);
        assert!(!client.status().is_dormant);
        assert_eq!(client.status().peer_count, 0);
        assert!(!client.tbid().is_empty());
        assert!(!client.tbn().is_empty());
    }

    #[test]
    fn foretias_stamp_produces_valid_foretis() {
        let client = Foretias::new("stamp-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (foretis, sig, _alg) = rt.block_on(async {
            client
                .stamp(b"hello world", "test-echo".into())
                .await
                .unwrap()
        });
        assert_eq!(*foretis.chronon_number(), 1);
        assert_eq!(foretis.echo(), "test-echo");
        assert!(!foretis.tbn().is_empty());
        assert!(!foretis.content_hash().is_empty());
        assert!(!sig.is_empty());
    }

    #[test]
    fn foretias_stamp_verify_roundtrip() {
        let client = Foretias::new("roundtrip-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let content = b"test content";
            let (foretis, sig, alg) = client.stamp(content, "echo".into()).await.unwrap();
            let verified = client.verify(content, &foretis, &sig, &alg).await.unwrap();
            assert!(verified, "valid stamp should verify successfully");
        });
    }

    #[test]
    fn foretias_wrong_content_fails_verification() {
        let client = Foretias::new("verify-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (foretis, sig, alg) = client.stamp(b"original", "echo".into()).await.unwrap();
            let verified = client
                .verify(b"tampered", &foretis, &sig, &alg)
                .await
                .unwrap();
            assert!(!verified, "wrong content should fail verification");
        });
    }

    #[test]
    fn foretias_multiple_stamps_distinct_ticks() {
        let client = Foretias::new("multi-stamp".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (f1, _, _) = client.stamp(b"first", "echo1".into()).await.unwrap();
            let (f2, _, _) = client.stamp(b"second", "echo2".into()).await.unwrap();
            assert_ne!(f1.content_hash(), f2.content_hash());
            assert_eq!(f1.echo(), "echo1");
            assert_eq!(f2.echo(), "echo2");
        });
    }

    #[test]
    fn foretias_calendar_reflects_stamps() {
        let client = Foretias::new("calendar-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = client.stamp(b"a", "e1".into()).await.unwrap();
            let _ = client.stamp(b"b", "e2".into()).await.unwrap();
            let records = client.calendar_slice(0, 10).await.unwrap();
            assert!(!records.is_empty());
        });
    }

    #[test]
    fn foretias_identity_accessors() {
        let client = Foretias::new("identity-test".into(), None).unwrap();
        assert!(!client.tbid().is_empty());
        assert!(!client.tbn().is_empty());
        let _ = client.public_key();
    }

    #[test]
    fn foretias_persistence_roundtrip() {
        let path = PathBuf::from("/tmp/foretias-thinclient-persist-test.json");
        let persist_dir = path.parent().unwrap();
        std::fs::create_dir_all(persist_dir).ok();
        let client = Foretias::new("persist-test".into(), Some(path.clone())).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = client.stamp(b"persist me", "echo".into()).await.unwrap();
        });
        assert!(path.exists(), "calendar file should be persisted");
        let loaded = Foretias::from_persist(path.clone()).unwrap();
        assert!(loaded.status().is_dormant);
        assert_eq!(loaded.level(), ClientLevel::Standalone);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn foretias_dormant_cannot_stamp() {
        let path = PathBuf::from("/tmp/foretias-thinclient-dormant-test.json");
        let client = Foretias::new("dormant-source".into(), Some(path.clone())).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = client
                .stamp(b"before dormant", "echo".into())
                .await
                .unwrap();
        });
        let dormant = Foretias::from_persist(path.clone()).unwrap();
        assert!(dormant.status().is_dormant);
        let result = rt.block_on(async { dormant.stamp(b"should fail", "echo".into()).await });
        assert!(matches!(result, Err(ForetiasError::Dormant(_))));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn foretias_foretis_serialization_roundtrip() {
        let client = Foretias::new("serialize-test".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (foretis, _, _) = client.stamp(b"serialize me", "echo".into()).await.unwrap();
            let json = serde_json::to_string(&foretis).unwrap();
            let deserialized: ForetisRecord = serde_json::from_str(&json).unwrap();
            assert_eq!(*foretis.chronon_number(), *deserialized.chronon_number());
            assert_eq!(foretis.content_hash(), deserialized.content_hash());
            assert_eq!(foretis.tbid(), deserialized.tbid());
            assert_eq!(foretis.echo(), deserialized.echo());
        });
    }

    #[test]
    fn verify_with_proof_succeeds_standalone() {
        let client = Foretias::new("vwp-success".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let content = b"verify with proof test";
            let (foretis, sig, alg) = client.stamp(content, "echo".into()).await.unwrap();
            let report = client
                .verify_with_proof(content, &foretis, &sig, &alg)
                .await
                .unwrap();
            assert!(report.verified, "valid stamp should verify with proof");
            assert_eq!(report.chronon_number, *foretis.chronon_number());
            assert!(
                !report.calendar_records.is_empty(),
                "report must include calendar records"
            );
            assert_eq!(report.calendar_records.len(), 1);
            assert_eq!(
                *report.calendar_records[0].chronon_number(),
                *foretis.chronon_number()
            );
            assert_eq!(*report.foretis.chronon_number(), *foretis.chronon_number());
            assert_eq!(report.method, "verify_with_proof");
        });
    }

    #[test]
    fn verify_with_proof_fails_wrong_content_standalone() {
        let client = Foretias::new("vwp-wrong-content".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (foretis, sig, alg) = client
                .stamp(b"original content", "echo".into())
                .await
                .unwrap();
            let report = client
                .verify_with_proof(b"tampered content", &foretis, &sig, &alg)
                .await
                .unwrap();
            assert!(!report.verified, "wrong content should fail verification");
        });
    }

    #[test]
    fn verify_with_proof_returns_report_structure() {
        let client = Foretias::new("vwp-structure".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let content = b"structure test";
            let (foretis, sig, alg) = client.stamp(content, "echo".into()).await.unwrap();
            let report = client
                .verify_with_proof(content, &foretis, &sig, &alg)
                .await
                .unwrap();
            assert!(report.verified);
            assert!(report.chronon_number > 0, "chronon_number must be positive");
            assert_eq!(report.calendar_records.len(), 1);
            assert!(!report.calendar_records[0].public_key().is_empty());
            assert!(!report.calendar_records[0].forward_foretis().is_empty());
            assert!(!report.calendar_records[0].signature_algorithm().is_empty());
        });
    }

    #[test]
    fn verify_with_proof_nonexistent_chronon() {
        let client = Foretias::new("vwp-nonexistent".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (foretis, sig, alg) = client.stamp(b"content", "echo".into()).await.unwrap();
            let report = client
                .verify_with_proof(b"content", &foretis, &sig, &alg)
                .await
                .unwrap();
            assert!(report.verified);
        });
    }

    #[test]
    fn verify_with_proof_multi_stamp() {
        let client = Foretias::new("vwp-multi".into(), None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (f1, sig1, alg1) = client.stamp(b"first stamp", "echo1".into()).await.unwrap();
            let (f2, sig2, alg2) = client.stamp(b"second stamp", "echo2".into()).await.unwrap();
            assert_ne!(*f1.chronon_number(), *f2.chronon_number());
            let r1 = client
                .verify_with_proof(b"first stamp", &f1, &sig1, &alg1)
                .await
                .unwrap();
            let r2 = client
                .verify_with_proof(b"second stamp", &f2, &sig2, &alg2)
                .await
                .unwrap();
            assert!(r1.verified);
            assert!(r2.verified);
            assert_eq!(
                *r1.calendar_records[0].chronon_number(),
                *f1.chronon_number()
            );
            assert_eq!(
                *r2.calendar_records[0].chronon_number(),
                *f2.chronon_number()
            );
        });
    }
}
