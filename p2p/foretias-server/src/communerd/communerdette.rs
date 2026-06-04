//! Communerdette — per-TBID relationship manager.
//!
//! `Communerdette` is private to the `communerd` module. It owns relationship-local
//! state for exactly one external TBID: binding status, active route, stats, and
//! a cancellation token for per-relationship tasks.
//!
//! `CommunerdetteLine` is the public, cloneable capability handle exposed to
//! Calendar, Chronomatter-adjacent orchestration, and TimeFamily code. It points
//! at a `Communerdette` but exposes only safe, TBID-scoped operations.
//!
//! Design invariants:
//! - One Communerdette, one external TBID.
//! - Communerdette does NOT store private key material.
//! - Communerdette does NOT sign for Calendar, Chronomatter, or any local TBID.
//! - `CommunerdetteLine` does NOT expose transport details or mutable state.

use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
#[allow(unused_imports)]
use foretias_core::clock::{Clock, SystemClock};
use foretias_core::crypto_server::CryptoServer;
use foretias_core::error::NodeError;
use foretias_core::foretias::clean_auth::{
    CleanAuthError, CleanAuthenticated, UnverifiedSignatureEnvelope,
};
use foretias_core::foretias::tick::{ChrononRecord, Foretis};
use foretias_core::foretias::types::Tbid;
#[cfg(test)]
use parking_lot::Mutex;
use parking_lot::RwLock;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration as TokioDuration;
use tokio_util::sync::CancellationToken;

use super::transport::{PeerAddr, TransportError};
use super::PeerRegistrationRecord;

/// Private helper surface that Communerdette uses to ask Communerd for
/// DHT lookup, namespace, swarm availability, and transport execution.
///
/// This trait is private to the `communerd` module.  Communerdette never
/// owns the DHT, peer pool, libp2p swarm, or transport pools — it always
/// delegates through this trait.
/// Phase 3.3/12: Host helpers, awaiting consumer tasks.
#[allow(dead_code)]
#[async_trait]
pub(super) trait CommunerdetteHost: Send + Sync {
    /// Look up a TBID in the DHT (cache + live lookup).
    async fn host_lookup_tbid(
        &self,
        tbid_hex: &str,
        namespace: &str,
    ) -> Option<PeerRegistrationRecord>;
    /// Look up a TBID in the local cache only.
    fn host_lookup_tbid_cached(&self, tbid_hex: &str) -> Option<PeerRegistrationRecord>;
    /// Get the current DHT namespace.
    fn host_namespace(&self) -> String;
    /// Check if the libp2p swarm is available.
    fn host_swarm_available(&self) -> bool;
    /// Get the local PeerId.
    fn host_local_peer_id(&self) -> Option<libp2p::PeerId>;
    /// Execute a stamp request via Communerd's transport.
    async fn host_execute_stamp(
        &self,
        peer: &PeerAddr,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError>;
    /// Execute a calendar slice request via Communerd's transport.
    async fn host_execute_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError>;
    /// Execute a channel_bind_challenge RPC (Phase 12.0).
    async fn host_execute_channel_bind_challenge(
        &self,
        peer: &PeerAddr,
        nonce_hex: &str,
        channel_id: &str,
        requester_tbid_hex: &str,
    ) -> Result<serde_json::Value, TransportError>;
    /// Execute a liveness ping (Phase 12.1).
    async fn host_execute_ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
    /// Sign a ProbityReport (Phase 13.1).
    /// Calendar signing stub — returns signed bytes or error.
    fn host_sign_probity_report(
        &self,
        report: &crate::probity::ProbityReport,
    ) -> Result<Vec<u8>, String>;
    /// Publish a signed ProbityReport via gossip (Phase 13.1).
    fn host_publish_probity_report(&self, signed_bytes: Vec<u8>);
}

/// Evidence level for a TBID-to-transport binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TbidBindingStatus {
    /// No binding information available yet.
    Unknown,
    /// A DHT record claims a transport identity for this TBID (unverified).
    ClaimedByDht {
        peer_id: Option<String>,
        json_rpc: Option<String>,
        observed_at_ns: u64,
    },
    /// An application-level proof verified this transport identity for the TBID.
    Verified {
        peer_id: Option<String>,
        json_rpc: Option<String>,
        verified_at_ns: u64,
        proof_expires_at_ns: Option<u64>,
    },
    /// This binding was rejected (e.g. proof verification failed).
    Rejected { reason: String, observed_at_ns: u64 },
}

impl TbidBindingStatus {
    /// Returns true if the binding is strong enough for trust-bearing operations.
    pub fn is_verified(&self) -> bool {
        matches!(self, TbidBindingStatus::Verified { .. })
    }

    /// Returns true if there is at least a DHT-level claim (useful for discovery).
    pub fn has_any_claim(&self) -> bool {
        !matches!(
            self,
            TbidBindingStatus::Unknown | TbidBindingStatus::Rejected { .. }
        )
    }
}

/// Currently preferred transport route for this TBID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActiveRoute {
    /// libp2p direct RPC over request_response/yamux.
    Libp2pDirect,
    /// Custom Noise_XX TCP with JSON-RPC (fallback).
    NoiseJsonRpc,
    /// No usable route is currently available.
    Unavailable,
}

/// Per-relationship statistics.
#[derive(Debug, Clone, Default)]
pub struct CommunerdetteStats {
    pub last_success_ns: Option<u64>,
    pub last_failure_ns: Option<u64>,
    pub consecutive_failures: u32,
    pub libp2p_successes: u64,
    pub libp2p_failures: u64,
    pub noise_successes: u64,
    pub noise_failures: u64,
    pub smoothed_rtt_ms: Option<f64>,
    pub queue_depth: usize,
}

/// Per-route stats for granular tracking (Phase 6).
/// Phase 6: Liveness stats, awaiting queue worker (Phase 5.2).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CommunerdetteRouteStats {
    /// Per-route success count.
    pub success_count: u64,
    /// Per-route failure count.
    pub failure_count: u64,
    /// Monotonic timestamp (ns) of last successful request on this route.
    pub last_success_ns: Option<u64>,
    /// Monotonic timestamp (ns) of last failed request on this route.
    pub last_failure_ns: Option<u64>,
    /// Smoothed RTT in milliseconds (exponential moving average, alpha=0.2).
    pub smoothed_rtt_ms: Option<f64>,
    /// Consecutive failures on this route (triggers backoff).
    pub consecutive_failures: u32,
    /// Monotonic timestamp (ns) of last successful application RPC (stamp, get_tick,
    /// get_calendar_slice). This is the primary liveness signal — if recent enough,
    /// the transport-level ping is unnecessary.
    pub last_application_rpc_ns: Option<u64>,
}

#[allow(dead_code)]
impl CommunerdetteRouteStats {
    /// Record a successful request with measured RTT (ms).
    pub fn record_success(&mut self, now_ns: u64, rtt_ms: f64) {
        self.success_count += 1;
        self.last_success_ns = Some(now_ns);
        self.consecutive_failures = 0;
        self.smoothed_rtt_ms = Some(match self.smoothed_rtt_ms {
            Some(prev) => 0.2 * rtt_ms + 0.8 * prev,
            None => rtt_ms,
        });
    }

    /// Record a successful application RPC (stamp, get_tick, get_calendar_slice).
    pub fn record_application_rpc_success(&mut self, now_ns: u64) {
        self.last_application_rpc_ns = Some(now_ns);
    }

    /// Record a failed request.
    pub fn record_failure(&mut self, now_ns: u64) {
        self.failure_count += 1;
        self.last_failure_ns = Some(now_ns);
        self.consecutive_failures += 1;
    }

    /// Return current backoff multiplier (1.0 = no backoff, doubles per failure, capped at 32x).
    pub fn backoff_multiplier(&self) -> f64 {
        (2_f64).powi(self.consecutive_failures as i32).min(32.0)
    }
}

/// Private state — not exposed through CommunerdetteLine.
#[doc(hidden)]
struct CommunerdetteState {
    binding: TbidBindingStatus,
    active_route: ActiveRoute,
    stats: CommunerdetteStats,
    backoff_until_ns: Option<u64>,
    /// Per-route stats (Phase 6).
    route_stats: std::collections::HashMap<ActiveRoute, CommunerdetteRouteStats>,
    /// Route candidates from DHT/PeerRegistrationRecord (Phase 3).
    route_candidates: Vec<super::PeerRegistrationRecord>,
    /// Whether binding proof has been requested (Phase 7).
    binding_proof_requested: bool,
    /// Whether binding proof has been verified (Phase 7).
    binding_proof_verified: bool,
    /// Last liveness probe timestamp (ns) (Phase 6).
    last_liveness_probe_ns: Option<u64>,
    /// Liveness probe interval in milliseconds (Phase 6).
    liveness_interval_ms: u64,
    /// Liveness loop policy (Phase 12.5).
    liveness_policy: LivenessPolicy,
    /// Shared liveness cycle flags — updated by queue task (application RPC success)
    /// and read by L1/L2/L3 liveness tasks.
    liveness_flags: Arc<LivenessCycleFlags>,
}

impl Default for CommunerdetteState {
    fn default() -> Self {
        Self {
            binding: TbidBindingStatus::Unknown,
            active_route: ActiveRoute::Unavailable,
            stats: CommunerdetteStats::default(),
            backoff_until_ns: None,
            route_stats: std::collections::HashMap::new(),
            route_candidates: Vec::new(),
            binding_proof_requested: false,
            binding_proof_verified: false,
            last_liveness_probe_ns: None,
            liveness_interval_ms: 30_000,
            liveness_policy: LivenessPolicy::default(),
            liveness_flags: LivenessCycleFlags::new(),
        }
    }
}

// ── Phase 12.0 — Channel-Binding Types ───────────────────────────────────────

/// Channel-binding message type — wire-compatible, dual-key signed.
///
/// This is the data the remote party signs during initial channel-binding.
/// The sig fields are hex-encoded to be directly JSON-serializable; the
/// Take 3 pipeline uses `UnverifiedSignatureEnvelope<ChannelBinding>` and produces
/// `CleanFullyAuthenticated<ChannelBinding>` after dual-key verification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelBinding {
    pub responder_tbid: String, // hex-encoded Tbid (96 bytes → 192 hex chars)
    pub nonce_echo: String,     // hex-encoded echoed challenge nonce
    pub channel_id: String,     // transport identity string
    pub fast_sig: String,       // hex-encoded Ed25519 signature (64 bytes)
    pub slow_sig: String,       // hex-encoded SLH-DSA signature (49856 bytes)
}

/// Take 3 UnverifiedSignatureEnvelope stage for channel-binding responses.
///
/// `UnverifiedSignatureEnvelope<ChannelBinding>` holds the parsed-but-unverified wire data.
/// Call methods from `ChannelBindingGate` to verify and produce
/// `CleanFullyAuthenticated<ChannelBinding>`.
pub type UnverifiedSignatureEnvelopeChannelBinding =
    foretias_core::foretias::clean_auth::UnverifiedSignatureEnvelope<ChannelBinding>;

/// Extension trait that adds the inbound gate methods to `UnverifiedSignatureEnvelope<ChannelBinding>`.
///
/// Extension trait rather than inherent impl because `UnverifiedSignatureEnvelope<T>` is defined
/// in core-engine and the orphan rule prevents foreign-type inherent impls here.
/// Import this trait to call `.verify_full()` etc. on `UnverifiedSignatureEnvelope<ChannelBinding>`.
/// Phase 12.0: Channel binding gate methods, awaiting dual-key consumer.
#[allow(dead_code)]
pub trait ChannelBindingGate: Sized {
    /// Verify Ed25519 fast-key signature only.
    /// // VERIFY(remote-tbid, fast-key)
    fn verify_fast_only(
        &self,
        crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<bool, CommunerdetteError>;

    /// Verify SLH-DSA slow-key signature only.
    /// // VERIFY(remote-tbid, slow-key)
    fn verify_slow_only(
        &self,
        crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<bool, CommunerdetteError>;

    /// Verify both keys via TBID V1 C verifier. Returns `CleanFullyAuthenticated<ChannelBinding>`.
    /// // VERIFY(remote-tbid, fast-key+slow-key)
    fn verify_full(
        self,
        crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<
        foretias_core::foretias::clean_auth::CleanFullyAuthenticated<ChannelBinding>,
        CommunerdetteError,
    >;
}

impl ChannelBindingGate for UnverifiedSignatureEnvelopeChannelBinding {
    fn verify_fast_only(
        &self,
        crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<bool, CommunerdetteError> {
        check_binding_fields(self.inner(), nonce, channel_id, target_tbid)?;
        let fast_sig = hex::decode(&self.inner().fast_sig)
            .map_err(|_| CommunerdetteError::Structural("fast_sig not hex".into()))?;
        let msg = binding_msg(nonce, channel_id, &self.inner().responder_tbid);
        let ok = crypto
            .verify_with(
                &target_tbid.ed25519_public_key(),
                "Ed25519",
                &msg,
                &fast_sig,
            )
            .map_err(|e| {
                CommunerdetteError::CleanAuth(CleanAuthError::Crypto(NodeError::Crypto(e)))
            })?;
        Ok(ok)
    }

    fn verify_slow_only(
        &self,
        crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<bool, CommunerdetteError> {
        check_binding_fields(self.inner(), nonce, channel_id, target_tbid)?;
        let slow_sig = hex::decode(&self.inner().slow_sig)
            .map_err(|_| CommunerdetteError::Structural("slow_sig not hex".into()))?;
        let msg = binding_msg(nonce, channel_id, &self.inner().responder_tbid);
        let ok = crypto
            .verify_with(
                &target_tbid.slh_dsa_public_key(),
                "SLH-DSA-SHA2-256f",
                &msg,
                &slow_sig,
            )
            .map_err(|e| {
                CommunerdetteError::CleanAuth(CleanAuthError::Crypto(NodeError::Crypto(e)))
            })?;
        Ok(ok)
    }

    fn verify_full(
        self,
        _crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        target_tbid: &Tbid,
    ) -> Result<
        foretias_core::foretias::clean_auth::CleanFullyAuthenticated<ChannelBinding>,
        CommunerdetteError,
    > {
        check_binding_fields(self.inner(), nonce, channel_id, target_tbid)?;

        let fast_bytes = hex::decode(&self.inner().fast_sig)
            .map_err(|_| CommunerdetteError::Structural("fast_sig not hex".into()))?;
        let slow_bytes = hex::decode(&self.inner().slow_sig)
            .map_err(|_| CommunerdetteError::Structural("slow_sig not hex".into()))?;

        // Reconstruct combined: Ed25519(64) ‖ SLH-DSA(49856) = 49920 bytes
        let mut combined = Vec::with_capacity(49920);
        combined.extend_from_slice(&fast_bytes);
        combined.extend_from_slice(&slow_bytes);

        let msg = binding_msg(nonce, channel_id, &self.inner().responder_tbid);
        let pub_key =
            foretias_core::foretias::types::SignatureBytes::from(target_tbid.raw_bytes().to_vec());
        let sig = foretias_core::foretias::types::SignatureBytes::from(combined);

        // TBID V1 C verifier — authoritative dual-key verification
        let ok = foretias_core::crypto_server::signing_tbid::tbid_verify(&pub_key, &msg, &sig)
            .map_err(|e| {
                CommunerdetteError::CleanAuth(CleanAuthError::Crypto(NodeError::Crypto(e)))
            })?;
        if !ok {
            return Err(CommunerdetteError::CleanAuth(
                CleanAuthError::InvalidSignature,
            ));
        }

        Ok(
            foretias_core::foretias::clean_auth::CleanFullyAuthenticated::from_dual_verified(
                self.into_inner(),
            ),
        )
    }
}

fn check_binding_fields(
    inner: &ChannelBinding,
    nonce: &[u8],
    channel_id: &str,
    target_tbid: &Tbid,
) -> Result<(), CommunerdetteError> {
    let responder = Tbid::from_hex(&inner.responder_tbid)
        .map_err(|_| CommunerdetteError::Structural("bad responder_tbid hex".into()))?;
    if responder != *target_tbid {
        return Err(CommunerdetteError::TbidMismatch {
            expected: target_tbid.to_hex(),
            actual: inner.responder_tbid.clone(),
        });
    }
    let nonce_echo = hex::decode(&inner.nonce_echo)
        .map_err(|_| CommunerdetteError::Structural("nonce_echo not hex".into()))?;
    if nonce_echo != nonce {
        return Err(CommunerdetteError::Structural("nonce_echo mismatch".into()));
    }
    if inner.channel_id != channel_id {
        return Err(CommunerdetteError::Structural("channel_id mismatch".into()));
    }
    Ok(())
}

/// Canonical message: nonce ‖ channel_id_bytes ‖ responder_tbid_hex_bytes.
fn binding_msg(nonce: &[u8], channel_id: &str, responder_tbid_hex: &str) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(nonce);
    msg.extend_from_slice(channel_id.as_bytes());
    msg.extend_from_slice(responder_tbid_hex.as_bytes());
    msg
}

/// Phase 12.0: Channel binding, awaiting state field integration.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub enum ChannelBindingState {
    /// Channel known but binding challenge not yet sent/verified.
    #[default]
    Unbound,
    /// Dual-key proof received and verified; channel is trusted.
    FullyBound(foretias_core::foretias::clean_auth::CleanFullyAuthenticated<ChannelBinding>),
    /// Binding challenge failed; channel is not trusted.
    Rejected { reason: String },
}

/// Read-only diagnostics surface for a Communerdette.
#[derive(Debug, Clone)]
pub struct CommunerdetteStatusSummary {
    pub target_tbid: String,
    pub binding: TbidBindingStatus,
    pub active_route: ActiveRoute,
    pub stats: CommunerdetteStats,
    pub liveness_policy: LivenessPolicy,
}

/// Private per-TBID relationship manager.
#[doc(hidden)]
pub(super) struct Communerdette {
    target_tbid: Tbid,
    state: RwLock<CommunerdetteState>,
    shutdown: CancellationToken,
}

/// Phase 3-8: Deferred methods awaiting queue worker and liveness integration.
#[allow(dead_code)]
impl Communerdette {
    /// Create a new Communerdette for the given TBID.
    pub(super) fn new(target_tbid: Tbid) -> Self {
        Self {
            target_tbid,
            state: RwLock::new(CommunerdetteState::default()),
            shutdown: CancellationToken::new(),
        }
    }

    /// Returns a read-only snapshot of the current status.
    fn status_summary(&self) -> CommunerdetteStatusSummary {
        let state = self.state.read();
        CommunerdetteStatusSummary {
            target_tbid: self.target_tbid.to_hex(),
            binding: state.binding.clone(),
            active_route: state.active_route,
            stats: state.stats.clone(),
            liveness_policy: state.liveness_policy.clone(),
        }
    }

    /// Returns the cancellation token for per-relationship tasks.
    #[doc(hidden)]
    fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    // ── Phase 6: Liveness & Stats ───────────────────────────────────────

    /// Record a successful request on the given route.
    pub(super) fn record_route_success(&self, route: ActiveRoute, now_ns: u64, rtt_ms: f64) {
        let mut state = self.state.write();
        state.stats.libp2p_successes = match route {
            ActiveRoute::Libp2pDirect => state.stats.libp2p_successes + 1,
            _ => state.stats.libp2p_successes,
        };
        state.stats.noise_successes = match route {
            ActiveRoute::NoiseJsonRpc => state.stats.noise_successes + 1,
            _ => state.stats.noise_successes,
        };
        state.stats.last_success_ns = Some(now_ns);
        state.stats.consecutive_failures = 0;
        state.backoff_until_ns = None;

        let smoothed = match state.stats.smoothed_rtt_ms {
            Some(prev) => 0.2 * rtt_ms + 0.8 * prev,
            None => rtt_ms,
        };
        state.stats.smoothed_rtt_ms = Some(smoothed);

        // Per-route stats
        state
            .route_stats
            .entry(route)
            .or_default()
            .record_success(now_ns, rtt_ms);
    }

    /// Record a failed request on the given route.
    pub(super) fn record_route_failure(&self, route: ActiveRoute, now_ns: u64) {
        let mut state = self.state.write();
        state.stats.libp2p_failures = match route {
            ActiveRoute::Libp2pDirect => state.stats.libp2p_failures + 1,
            _ => state.stats.libp2p_failures,
        };
        state.stats.noise_failures = match route {
            ActiveRoute::NoiseJsonRpc => state.stats.noise_failures + 1,
            _ => state.stats.noise_failures,
        };
        state.stats.last_failure_ns = Some(now_ns);
        state.stats.consecutive_failures += 1;

        // Exponential backoff: double delay, cap at 32x
        let current_backoff = state
            .backoff_until_ns
            .map(|b| (now_ns - b) as f64)
            .unwrap_or(1.0);
        let next_backoff = (current_backoff * 2.0).min(32_000_000_000.0); // 32s cap in ns
        state.backoff_until_ns = Some(now_ns + next_backoff as u64);

        // Per-route stats
        state
            .route_stats
            .entry(route)
            .or_default()
            .record_failure(now_ns);
    }

    /// Check if the relationship is currently in backoff.
    pub(super) fn is_in_backoff(&self, now_ns: u64) -> bool {
        let state = self.state.read();
        state
            .backoff_until_ns
            .map(|until| now_ns < until)
            .unwrap_or(false)
    }

    /// Get backoff multiplier for adaptive timeout calculation.
    pub(super) fn backoff_multiplier(&self) -> f64 {
        let state = self.state.read();
        state
            .stats
            .consecutive_failures
            .max(1)
            .checked_pow(2)
            .unwrap_or(32) as f64
    }

    /// Set the active route (called after DHT discovery or route refresh).
    pub(super) fn set_active_route(&self, route: ActiveRoute) {
        let mut state = self.state.write();
        state.active_route = route;
    }

    /// Add a route candidate from DHT/PeerRegistrationRecord.
    pub(super) fn add_route_candidate(&self, record: super::PeerRegistrationRecord) {
        let mut state = self.state.write();
        state.route_candidates.push(record);
        // Prefer libp2p when PeerId is available
        state.active_route = ActiveRoute::Libp2pDirect;
    }

    /// Set liveness probe interval in milliseconds.
    pub(super) fn set_liveness_interval_ms(&self, interval_ms: u64) {
        let mut state = self.state.write();
        state.liveness_interval_ms = interval_ms;
    }

    /// Record that a liveness probe was sent/received.
    pub(super) fn record_liveness_probe(&self, now_ns: u64) {
        let mut state = self.state.write();
        state.last_liveness_probe_ns = Some(now_ns);
    }

    /// Get the last liveness probe timestamp.
    pub(super) fn last_liveness_probe_ns(&self) -> Option<u64> {
        self.state.read().last_liveness_probe_ns
    }

    /// Get liveness probe interval in milliseconds.
    pub(super) fn liveness_interval_ms(&self) -> u64 {
        self.state.read().liveness_interval_ms
    }

    pub(super) fn liveness_flags(&self) -> Arc<LivenessCycleFlags> {
        Arc::clone(&self.state.read().liveness_flags)
    }

    /// Get per-route stats snapshot.
    pub(super) fn route_stats_snapshot(
        &self,
    ) -> std::collections::HashMap<ActiveRoute, CommunerdetteRouteStats> {
        self.state.read().route_stats.clone()
    }

    // ── Phase 3.1: Initial Binding Refresh ──────────────────────────────

    /// Refresh the DHT binding for this TBID.
    ///
    /// Calls back into Communerd's `lookup_tbid`.  DHT results are stored as
    /// `ClaimedByDht`, NOT `Verified`.  Also adds route candidates from the
    /// DHT record and re-evaluates the active route.
    ///
    /// # Arguments
    /// * `host` — Communerd host helper surface (DHT lookup, namespace, etc.)
    /// * `now_ns` — Current monotonic timestamp in nanoseconds
    pub(super) async fn refresh_dht_binding(
        &self,
        host: &dyn CommunerdetteHost,
        now_ns: u64,
    ) -> TbidBindingStatus {
        let tbid_hex = self.target_tbid.to_hex();
        let namespace = host.host_namespace();

        // Check local tbid_index cache first
        if let Some(record) = host.host_lookup_tbid_cached(&tbid_hex) {
            self.update_from_dht_record(&record, now_ns);
            return self.binding_status();
        }

        // DHT lookup
        let result = host.host_lookup_tbid(&tbid_hex, &namespace).await;

        match result {
            Some(record) => {
                self.update_from_dht_record(&record, now_ns);
                TbidBindingStatus::ClaimedByDht {
                    peer_id: Some(record.peer_id),
                    json_rpc: Some(record.json_rpc),
                    observed_at_ns: now_ns,
                }
            }
            None => {
                // No DHT record found — binding stays Unknown
                TbidBindingStatus::Unknown
            }
        }
    }

    fn update_from_dht_record(&self, record: &PeerRegistrationRecord, now_ns: u64) {
        // DHT records are ClaimedByDht, never Verified — trust boundary per spec
        self.mark_binding_claimed_by_dht(
            Some(record.peer_id.clone()),
            Some(record.json_rpc.clone()),
            now_ns,
        );
        self.add_route_candidate(record.clone());
    }

    // ── Phase 3.2: choose_route — improved with libp2p-first policy ─────

    /// Choose the best route using route health and libp2p-first policy.
    ///
    /// Policy:
    /// 1. If swarm available + healthy libp2p candidate → Libp2pDirect
    /// 2. If healthy Noise candidate → NoiseJsonRpc
    /// 3. If swarm available + any libp2p candidate (no health data) → Libp2pDirect
    /// 4. If any Noise candidate (no health data) → NoiseJsonRpc
    /// 5. Otherwise → Unavailable
    ///
    /// # Arguments
    /// * `host` — Communerd host helper surface (swarm availability check)
    pub(super) fn choose_route_with_host(&self, host: &dyn CommunerdetteHost) -> ActiveRoute {
        let swarm_active = host.host_swarm_available();
        let state = self.state.read();

        // First pass: prefer healthy libp2p if swarm active
        if swarm_active {
            for candidate in &state.route_candidates {
                if !candidate.peer_id.is_empty() {
                    // Check if route health is acceptable
                    let route_stats = state.route_stats.get(&ActiveRoute::Libp2pDirect);
                    if route_stats
                        .map(|s| s.consecutive_failures < 5)
                        .unwrap_or(true)
                    {
                        return ActiveRoute::Libp2pDirect;
                    }
                }
            }
        }

        // Second pass: healthy Noise candidates
        let noise_stats = state.route_stats.get(&ActiveRoute::NoiseJsonRpc);
        if noise_stats
            .map(|s| s.consecutive_failures < 5)
            .unwrap_or(false)
        {
            for candidate in &state.route_candidates {
                if !candidate.json_rpc.is_empty() {
                    return ActiveRoute::NoiseJsonRpc;
                }
            }
        }

        // Third pass: any libp2p candidate without health data
        if swarm_active {
            for candidate in &state.route_candidates {
                if !candidate.peer_id.is_empty() {
                    return ActiveRoute::Libp2pDirect;
                }
            }
        }

        // Fourth pass: any Noise candidate without health data
        for candidate in &state.route_candidates {
            if !candidate.json_rpc.is_empty() {
                return ActiveRoute::NoiseJsonRpc;
            }
        }

        ActiveRoute::Unavailable
    }

    /// Route candidates count (for diagnostics).
    pub(super) fn route_candidates_count(&self) -> usize {
        self.state.read().route_candidates.len()
    }

    // ── Phase 7: TBID Binding Proof ─────────────────────────────────────

    /// Request TBID binding proof from remote peer.
    /// Returns Unsupported — proof format not yet in scope.
    pub(super) fn request_tbid_binding_proof(&self) -> Result<(), foretias_core::error::NodeError> {
        let mut state = self.state.write();
        state.binding_proof_requested = true;
        Err(foretias_core::error::NodeError::Unsupported(
            "TBID binding proof not yet implemented; proof transcript format pending",
        ))
    }

    /// Verify TBID binding proof transcript.
    /// Returns Unsupported — proof verification not yet in scope.
    pub(super) fn verify_tbid_binding_proof(
        &self,
        _proof: &[u8],
    ) -> Result<(), foretias_core::error::NodeError> {
        Err(foretias_core::error::NodeError::Unsupported(
            "TBID binding proof verification not yet implemented",
        ))
    }

    /// Mark binding as verified (after successful binding proof).
    pub(super) fn mark_binding_verified(
        &self,
        peer_id: Option<String>,
        json_rpc: Option<String>,
        verified_at_ns: u64,
    ) {
        let mut state = self.state.write();
        state.binding = TbidBindingStatus::Verified {
            peer_id,
            json_rpc,
            verified_at_ns,
            proof_expires_at_ns: None,
        };
        state.binding_proof_verified = true;
    }

    /// Mark binding as rejected.
    pub(super) fn mark_binding_rejected(&self, reason: String, observed_at_ns: u64) {
        let mut state = self.state.write();
        state.binding = TbidBindingStatus::Rejected {
            reason,
            observed_at_ns,
        };
    }

    /// Update binding to ClaimedByDht (from DHT discovery).
    pub(super) fn mark_binding_claimed_by_dht(
        &self,
        peer_id: Option<String>,
        json_rpc: Option<String>,
        observed_at_ns: u64,
    ) {
        let mut state = self.state.write();
        state.binding = TbidBindingStatus::ClaimedByDht {
            peer_id,
            json_rpc,
            observed_at_ns,
        };
    }

    /// Get binding status.
    pub(super) fn binding_status(&self) -> TbidBindingStatus {
        self.state.read().binding.clone()
    }

    /// Check if binding proof has been requested.
    pub(super) fn is_binding_proof_requested(&self) -> bool {
        self.state.read().binding_proof_requested
    }

    /// Check if binding proof has been verified.
    pub(super) fn is_binding_proof_verified(&self) -> bool {
        self.state.read().binding_proof_verified
    }

    // ── Phase 8: Mirror RPC Stubs ───────────────────────────────────────

    /// Mirror request: request mirror history from remote (stub).
    pub(super) fn mirror_request(
        &self,
        _from_tick: u64,
    ) -> Result<serde_json::Value, super::transport::TransportError> {
        Err(super::transport::TransportError::Unsupported(
            "mirror_request not yet implemented".into(),
        ))
    }

    /// Ship batch: send calendar batch to remote mirror (stub).
    pub(super) fn ship_batch(
        &self,
        _batch: serde_json::Value,
    ) -> Result<serde_json::Value, super::transport::TransportError> {
        Err(super::transport::TransportError::Unsupported(
            "ship_batch not yet implemented".into(),
        ))
    }

    /// Ship ack: acknowledge batch receipt (stub).
    pub(super) fn ship_ack(
        &self,
        _batch_id: &str,
    ) -> Result<serde_json::Value, super::transport::TransportError> {
        Err(super::transport::TransportError::Unsupported(
            "ship_ack not yet implemented".into(),
        ))
    }

    /// Stream tick: stream single tick to remote (stub).
    pub(super) fn stream_tick(
        &self,
        _tick_number: u64,
    ) -> Result<serde_json::Value, super::transport::TransportError> {
        Err(super::transport::TransportError::Unsupported(
            "stream_tick not yet implemented".into(),
        ))
    }

    /// Mirror reconcile: reconcile mirror state between local and remote (stub).
    pub(super) fn mirror_reconcile(
        &self,
        _local_tip: u64,
        _remote_tip: u64,
    ) -> Result<serde_json::Value, super::transport::TransportError> {
        Err(super::transport::TransportError::Unsupported(
            "mirror_reconcile not yet implemented".into(),
        ))
    }

    // ── Phase 9: Shutdown ───────────────────────────────────────────────

    /// Shutdown this Communerdette: cancel all per-relationship tasks.
    pub(super) fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Check if this Communerdette has been shut down.
    pub(super) fn is_shutdown(&self) -> bool {
        self.shutdown.is_cancelled()
    }
}

// ---------------------------------------------------------------------------
// Phase 5.1 - Request Priority
// ---------------------------------------------------------------------------

/// Request priority within the per-relationship queue.
///
/// Variants are declared lowest-first so `#[derive(Ord)]` orders
/// `Bulk < Normal < High < Critical`. This makes `Critical` the largest value,
/// which matches both `can_preempt` (Greater = higher priority) and the
/// `BinaryHeap` max-heap semantics used by the queue worker.
/// Phase 5.1: Request priority, awaiting queue worker.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommunerdettePriority {
    /// History dump, large calendar replication batch.
    Bulk,
    /// Stamp request, small calendar slice, mirror negotiation.
    Normal,
    /// Fetch tick needed to verify a Foretis, mutual-attestation evidence.
    High,
    /// TBID binding proof, collision/dormancy control, shutdown-sensitive.
    Critical,
}

// ---------------------------------------------------------------------------
// Phase 5.2 - Queue Worker (spawned on-demand per Communerdette)
// ---------------------------------------------------------------------------

/// Phase 5.2: Queue commands, awaiting queue worker.
#[allow(dead_code)]
enum CommunerdetteCommand {
    /// Fetch a calendar slice.
    CalendarSlice {
        tick_start: u64,
        count: u64,
        timeout: TokioDuration,
        reply: oneshot::Sender<Result<Vec<CleanAuthenticated<ChrononRecord>>, CommunerdetteError>>,
    },
    /// Stamp content on the remote TBID.
    Stamp {
        content: Vec<u8>,
        echo: String,
        timeout: TokioDuration,
        reply: oneshot::Sender<Result<CleanAuthenticated<Foretis>, CommunerdetteError>>,
    },
    /// Shutdown — drain or cancel pending work.
    Shutdown,
}

/// Errors returned by CommunerdetteLine methods.
#[derive(Debug, thiserror::Error)]
pub enum CommunerdetteError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("clean-auth verification failed: {0}")]
    CleanAuth(#[from] CleanAuthError),
    #[error("TBID mismatch: expected {expected}, got {actual}")]
    TbidMismatch { expected: String, actual: String },
    #[error("structurally invalid record: {0}")]
    Structural(String),
    #[error("no route available for TBID")]
    NoRoute,
    #[error("no peer record for TBID")]
    NoPeerRecord,
    #[error("binding insufficient for operation: {0}")]
    BindingInsufficient(String),
    #[error("node error: {0}")]
    Node(#[from] NodeError),
}

/// Phase 5.2: Queue wrapper, awaiting queue worker.
#[allow(dead_code)]
struct QueuedCommand {
    priority: CommunerdettePriority,
    sequence: u64,
    command: CommunerdetteCommand,
}

impl std::fmt::Debug for QueuedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedCommand")
            .field("priority", &self.priority)
            .field("sequence", &self.sequence)
            .field("command", &"(elided)")
            .finish()
    }
}

impl Eq for QueuedCommand {}

impl PartialEq for QueuedCommand {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

// BinaryHeap is a max-heap: higher priority + lower sequence = higher heap value
impl Ord for QueuedCommand {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then(other.sequence.cmp(&self.sequence))
    }
}

impl PartialOrd for QueuedCommand {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Internal executor reference that the Communerdette holds for transport calls.
struct CommunerdetteExecutor {
    host: Arc<dyn CommunerdetteHost>,
    target_tbid: Tbid,
    crypto: Arc<dyn CryptoServer>,
    clock: Arc<dyn Clock>,
    local_calendar_tbid: Option<String>,
}

impl CommunerdetteExecutor {
    fn new(
        host: Arc<dyn CommunerdetteHost>,
        target_tbid: Tbid,
        crypto: Arc<dyn CryptoServer>,
        clock: Arc<dyn Clock>,
        local_calendar_tbid: Option<String>,
    ) -> Self {
        Self {
            host,
            target_tbid,
            crypto,
            clock,
            local_calendar_tbid,
        }
    }

    /// Phase 12.0: FB emission, awaiting channel-bind integration.
    #[allow(dead_code)]
    fn emit_fb_report(&self, value: f32) {
        let Some(reporter_tbid) = &self.local_calendar_tbid else {
            tracing::debug!(target_tbid = %self.target_tbid.to_hex(), "FB emission skipped: no local_calendar_tbid set");
            return;
        };
        let report = crate::probity::ProbityReport {
            subject: self.target_tbid.to_hex(),
            reporter: reporter_tbid.clone(),
            attribute: "fb".to_string(),
            value,
            timestamp_ns: self.clock.now_ns().unwrap_or(0),
            signature: Vec::new(),
            curve: 1,
            slow_signature: vec![],
        };
        match self.host.host_sign_probity_report(&report) {
            Ok(signed) => self.host.host_publish_probity_report(signed),
            Err(e) => {
                tracing::warn!(target_tbid = %self.target_tbid.to_hex(), fb_value = value, "FB emission signing failed: {e}")
            }
        }
    }

    /// Resolve PeerAddr for the target TBID via DHT lookup.
    async fn resolve_peer(&self) -> Result<PeerAddr, CommunerdetteError> {
        let tbid_hex = self.target_tbid.to_hex();
        let ns = self.host.host_namespace();
        let owner = self
            .host
            .host_lookup_tbid(&tbid_hex, &ns)
            .await
            .ok_or_else(|| {
                TransportError::Connect(format!("TBID {} not found in DHT", tbid_hex))
            })?;

        let peer = PeerAddr {
            json_rpc: owner.json_rpc.clone(),
            peer_id: owner.peer_id.parse().ok(),
            last_seen_ns: 0,
        };

        Ok(peer)
    }

    /// Execute calendar_slice RPC with libp2p-first, Noise fallback.
    async fn do_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        // TODO(externalized): wrap outbound request params as Externalized<R> before
        // dispatch once outbound type enforcement is implemented (Phase 11.4).
        self.host
            .host_execute_calendar_slice(peer, tick_start, count)
            .await
    }

    /// Phase 12.0: Channel-bind challenge, awaiting channel-bind integration.
    #[allow(dead_code)]
    async fn do_channel_bind_challenge(
        &self,
        peer: &PeerAddr,
        nonce_hex: &str,
        channel_id: &str,
        requester_tbid_hex: &str,
    ) -> Result<serde_json::Value, TransportError> {
        // TODO(externalized): wrap params as Externalized<R> before dispatch (Phase 11.4).
        self.host
            .host_execute_channel_bind_challenge(peer, nonce_hex, channel_id, requester_tbid_hex)
            .await
    }

    /// Phase 12.1: Transport-level ping, awaiting L1 liveness spawn.
    #[allow(dead_code)]
    async fn do_ping(&self, peer: &PeerAddr) -> Result<(), TransportError> {
        self.host.host_execute_ping(peer).await
    }

    /// Execute stamp RPC with libp2p-first, Noise fallback.
    async fn do_stamp(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        // TODO(externalized): wrap outbound request params as Externalized<R> before
        // dispatch once outbound type enforcement is implemented (Phase 11.4).
        let tbid_hex = self.target_tbid.to_hex();
        self.host
            .host_execute_stamp(peer, &tbid_hex, content_hex, echo)
            .await
    }

    /// Run Take 3 inbound gate for a vector of ChrononRecords.
    ///
    /// For each record: parse as UnverifiedSignatureEnvelope<ChrononRecord>, verify TBID matches,
    /// then run verify(crypto, prev) for chain verification.
    fn gate_chronon_records(
        &self,
        records: Vec<ChrononRecord>,
    ) -> Result<Vec<CleanAuthenticated<ChrononRecord>>, CommunerdetteError> {
        let target_tbid = self.target_tbid;
        let crypto = &*self.crypto;
        let mut authenticated = Vec::new();

        for (idx, record) in records.into_iter().enumerate() {
            let unprocessed = UnverifiedSignatureEnvelope::<ChrononRecord>::from_parsed(record);

            // Structural validation
            if unprocessed.chronon_number() == &0 || unprocessed.public_key().is_empty() {
                return Err(CommunerdetteError::Structural(
                    "chronon_number == 0 or empty public_key".into(),
                ));
            }

            // TBID match
            if *unprocessed.tbid() != target_tbid {
                return Err(CommunerdetteError::TbidMismatch {
                    expected: target_tbid.to_hex(),
                    actual: unprocessed.tbid().to_hex(),
                });
            }

            // Chain verification via Take 3
            let prev = authenticated.last();
            let ca = if idx == 0 {
                // First record: genesis verification (tick 1) or standalone
                unprocessed
                    .verify(crypto, None)
                    .map_err(CommunerdetteError::CleanAuth)?
            } else {
                // Subsequent: chain verify against previous
                unprocessed
                    .into_clean_authenticated(crypto, prev.expect("idx > 0 implies prev exists"))
                    .map_err(CommunerdetteError::CleanAuth)?
            };

            // TODO(slow-key): if record carries a slow-key signature, verify it here
            // against target_tbid's slow public key. Reject if signature is present
            // but invalid. Absence is acceptable. Slow-key verification is currently
            // only required at channel-binding establishment; see Phase 12.0.

            authenticated.push(ca);
        }

        Ok(authenticated)
    }

    /// Run Take 3 inbound gate for a Foretis reply.
    ///
    /// Parse as UnverifiedSignatureEnvelope<Foretis>, run full Take 3 inbound gate.
    ///
    /// Requires the authenticated ChrononRecord for the Foretis's chronon and the
    /// original content bytes.  Callers must fetch the record via execute_tick first
    /// (Phase 11.1 — spec §11.5).
    fn gate_foretis(
        &self,
        raw: serde_json::Value,
        chronon_record: &CleanAuthenticated<ChrononRecord>,
        content: &[u8],
    ) -> Result<CleanAuthenticated<Foretis>, CommunerdetteError> {
        let unprocessed: Result<UnverifiedSignatureEnvelope<Foretis>, _> =
            UnverifiedSignatureEnvelope::<Foretis>::from_json_value_v2(raw);
        let unprocessed = unprocessed.map_err(|e| TransportError::Decode(e.to_string()))?;

        // Structural validation (signature is now in the envelope, not Foretis)
        {
            let f = unprocessed.inner();
            if f.chronon_number == 0 {
                return Err(CommunerdetteError::Structural(
                    "structurally invalid Foretis: chronon_number == 0".into(),
                ));
            }
        }

        // TBID match (before signature check — TBID mismatch is more specific)
        if *unprocessed.tbid() != self.target_tbid {
            return Err(CommunerdetteError::TbidMismatch {
                expected: self.target_tbid.to_hex(),
                actual: unprocessed.tbid().to_hex(),
            });
        }

        if !unprocessed.has_signatures() {
            return Err(CommunerdetteError::Structural(
                "structurally invalid Foretis: no signatures in envelope".into(),
            ));
        }

        // TODO(slow-key): if Foretis carries a slow-key signature, verify it here
        // against target_tbid's slow public key. Reject if signature is present
        // but invalid. Absence is acceptable. Slow-key verification is currently
        // only required at channel-binding establishment; see Phase 12.0.

        // TODO(slow-key): if Foretis carries a slow-key signature, verify it here
        // against target_tbid's slow public key. Reject if signature is present
        // but invalid. Absence is acceptable. Slow-key verification is currently
        // only required at channel-binding establishment; see Phase 12.0.

        // Full fast-key signature verification via Take 3 pipeline
        unprocessed
            .verify(&*self.crypto, chronon_record, content)
            .map_err(CommunerdetteError::CleanAuth)
    }
}

// ---------------------------------------------------------------------------
// Phase 4.1-4.2 - Communerdette request execution
// ---------------------------------------------------------------------------

impl Communerdette {
    /// Execute get_calendar_slice with Take 3 inbound gate.
    async fn execute_calendar_slice(
        executor: &CommunerdetteExecutor,
        tick_start: u64,
        count: u64,
        timeout: TokioDuration,
    ) -> Result<Vec<CleanAuthenticated<ChrononRecord>>, CommunerdetteError> {
        let peer = executor.resolve_peer().await?;

        let raw = tokio::time::timeout(
            timeout,
            executor.do_calendar_slice(&peer, tick_start, count),
        )
        .await
        .map_err(|_| CommunerdetteError::Transport(TransportError::Timeout))??;

        // Run Take 3 inbound gate
        executor.gate_chronon_records(raw)
    }

    /// Execute get_tick as get_calendar_slice(tick_number, 1) with exactly-one validation.
    async fn execute_tick(
        executor: &CommunerdetteExecutor,
        tick_number: u64,
        timeout: TokioDuration,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CommunerdetteError> {
        let slice = Self::execute_calendar_slice(executor, tick_number, 1, timeout).await?;
        match slice.into_iter().next() {
            Some(record) => Ok(record),
            None => Err(CommunerdetteError::Structural(
                "empty calendar slice for single tick request".into(),
            )),
        }
    }

    /// Execute stamp with full Take 3 inbound gate (Phase 11.1 — spec §11.5).
    ///
    /// After receiving the Foretis, we fetch the corresponding ChrononRecord so
    /// gate_foretis can run the real fast-key signature check rather than from_trusted.
    async fn execute_stamp(
        executor: &CommunerdetteExecutor,
        content: Vec<u8>,
        echo: String,
        timeout: TokioDuration,
    ) -> Result<CleanAuthenticated<Foretis>, CommunerdetteError> {
        let content_hex = hex::encode(&content);
        let peer = executor.resolve_peer().await?;

        let raw = tokio::time::timeout(timeout, executor.do_stamp(&peer, &content_hex, &echo))
            .await
            .map_err(|_| CommunerdetteError::Transport(TransportError::Timeout))??;

        // Parse just enough to get chronon_number before consuming raw
        let chronon_number = {
            let tmp = UnverifiedSignatureEnvelope::<Foretis>::from_json_value_v2(raw.clone())
                .map_err(|e| TransportError::Decode(e.to_string()))?;
            *tmp.chronon_number()
        };

        // Fetch the authenticated ChrononRecord for this tick (needed by gate_foretis)
        let chronon_record = Self::execute_tick(executor, chronon_number, timeout).await?;

        // Run full Take 3 inbound gate — fast-key verified
        executor.gate_foretis(raw, &chronon_record, &content)
    }

    /// Phase 5.2: Queue worker, awaiting integration.
    #[allow(dead_code)]
    fn spawn_queue_task(
        executor: Arc<CommunerdetteExecutor>,
        mut rx: mpsc::Receiver<(CommunerdettePriority, CommunerdetteCommand)>,
        sequence: Arc<AtomicU64>,
        liveness_flags: Arc<LivenessCycleFlags>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut heap = BinaryHeap::new();

            loop {
                tokio::select! {
                    biased;

                    // Receive new command from channel
                    Some((priority, command)) = rx.recv() => {
                        let seq = sequence.fetch_add(1, Ordering::Relaxed);
                        if let CommunerdetteCommand::Shutdown = &command {
                            heap.push(QueuedCommand { priority, sequence: seq, command });
                            // Drain remaining
                            while let Ok((p, c)) = rx.try_recv() {
                                let s = sequence.fetch_add(1, Ordering::Relaxed);
                                heap.push(QueuedCommand { priority: p, sequence: s, command: c });
                            }
                            break;
                        }
                        heap.push(QueuedCommand { priority, sequence: seq, command });
                    }

                    // Process highest-priority from heap
                    else => {
                        if let Some(cmd) = heap.pop() {
                            let exec = Arc::clone(&executor);
                            match cmd.command {
                                CommunerdetteCommand::CalendarSlice { tick_start, count, timeout, reply } => {
                                    let result = Self::execute_calendar_slice(&exec, tick_start, count, timeout).await;
                                    if result.is_ok() {
                                        let now_ns = exec.clock.now_ns().unwrap_or(0);
                                        liveness_flags.last_application_rpc_ns.store(now_ns, Ordering::Relaxed);
                                    }
                                    let _ = reply.send(result);
                                }
                                CommunerdetteCommand::Stamp { content, echo, timeout, reply } => {
                                    let result = Self::execute_stamp(&exec, content, echo, timeout).await;
                                    if result.is_ok() {
                                        let now_ns = exec.clock.now_ns().unwrap_or(0);
                                        liveness_flags.last_application_rpc_ns.store(now_ns, Ordering::Relaxed);
                                    }
                                    let _ = reply.send(result);
                                }
                                CommunerdetteCommand::Shutdown => {
                                    // Worker exits on shutdown
                                    break;
                                }
                            }
                        } else {
                            tokio::time::sleep(TokioDuration::from_millis(10)).await;
                        }
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Phase 12.0 — Channel-Binding Task
// ---------------------------------------------------------------------------

/// Phase 12.0: Channel-binding task, awaiting integration.
#[allow(dead_code)]
impl Communerdette {
    /// Initiate channel-binding for a new peer address (Phase 12.0 — spec §12.0).
    ///
    /// Sends a dual-key challenge, verifies the response, and updates the
    /// binding state.  Returns the `CleanFullyAuthenticated<ChannelBinding>`
    /// on success so callers can record or log the proof.
    pub(super) fn spawn_channel_bind_task(
        executor: Arc<CommunerdetteExecutor>,
        channel_id: String,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<
        Option<foretias_core::foretias::clean_auth::CleanFullyAuthenticated<ChannelBinding>>,
    > {
        tokio::spawn(async move {
            use rand::Rng;
            if cancel.is_cancelled() {
                return None;
            }

            // 1. Generate nonce
            let nonce: [u8; 32] = rand::thread_rng().gen();
            let nonce_hex = hex::encode(nonce);
            let requester_tbid_hex = executor.target_tbid.to_hex();

            // 2. Resolve peer
            let peer = match executor.resolve_peer().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(target_tbid = %requester_tbid_hex, "channel_bind: resolve_peer failed: {e}");
                    return None;
                }
            };

            // 3. Send challenge
            let raw = match tokio::time::timeout(
                TokioDuration::from_secs(30),
                executor.do_channel_bind_challenge(
                    &peer,
                    &nonce_hex,
                    &channel_id,
                    &requester_tbid_hex,
                ),
            )
            .await
            {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => {
                    tracing::warn!(target_tbid = %requester_tbid_hex, "channel_bind: challenge RPC failed: {e}");
                    return None;
                }
                Err(_) => {
                    tracing::warn!(target_tbid = %requester_tbid_hex, "channel_bind: challenge timed out");
                    return None;
                }
            };

            // 4. Parse + dual-key verify
            let unprocessed = match UnverifiedSignatureEnvelopeChannelBinding::from_json_value(raw)
            {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(target_tbid = %requester_tbid_hex, "channel_bind: parse failed: {e:?}");
                    return None;
                }
            };

            // VERIFY(remote-tbid, fast-key+slow-key)
            match unprocessed.verify_full(
                &*executor.crypto,
                &nonce,
                &channel_id,
                &executor.target_tbid,
            ) {
                Ok(binding) => {
                    tracing::info!(target_tbid = %requester_tbid_hex, channel_id = %channel_id, "channel bound (FullyBound)");
                    executor.emit_fb_report(1.0);
                    Some(binding)
                }
                Err(e) => {
                    tracing::warn!(target_tbid = %requester_tbid_hex, channel_id = %channel_id, "channel_bind: verify failed: {e:?}");
                    None
                }
            }
        })
    }

    /// Convenience entry point for callers outside `communerdette` (e.g. the
    /// gossip event loop in `mod.rs`).
    ///
    /// Creates a `CommunerdetteExecutor` from the provided components, then
    /// spawns the channel-bind task on the Tokio runtime.  The resulting
    /// `JoinHandle` is intentionally detached — the task manages its own
    /// lifecycle through the Communerdette's cancellation token.
    pub(super) fn trigger_channel_bind(
        &self,
        host: Arc<dyn CommunerdetteHost>,
        crypto: Arc<dyn CryptoServer>,
        clock: Arc<dyn Clock>,
        channel_id: String,
    ) {
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            self.target_tbid,
            crypto,
            clock,
            None, // local_calendar_tbid — not available in gossip-loop context
        ));
        let cancel = self.shutdown.clone();
        // spawn_channel_bind_task already spawns internally via tokio::spawn;
        // calling it directly avoids the double-spawn bug.
        let _handle = Self::spawn_channel_bind_task(executor, channel_id, cancel);
    }
}

// ---------------------------------------------------------------------------
// Phase 12.1 — L1 Liveness Task
// ---------------------------------------------------------------------------

impl Communerdette {
    /// Phase 12.1: L1 liveness, awaiting spawn.
    #[allow(dead_code)]
    pub(super) fn spawn_l1_liveness_task(
        executor: Arc<CommunerdetteExecutor>,
        interval_ms: u64,
        cancel: CancellationToken,
        flags: Arc<LivenessCycleFlags>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use std::sync::atomic::Ordering;
            let mut interval = tokio::time::interval(TokioDuration::from_millis(interval_ms));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {},
                }

                // Primary signal: check if a recent application RPC succeeded.
                let now_ns = executor.clock.now_ns().unwrap_or(0);
                let last_rpc_ns = flags.last_application_rpc_ns.load(Ordering::Relaxed);
                if last_rpc_ns > 0 {
                    let elapsed_ns = now_ns.saturating_sub(last_rpc_ns);
                    let threshold_ns = interval_ms.saturating_mul(1_000_000);
                    if elapsed_ns < threshold_ns {
                        tracing::trace!(
                            target_tbid = %executor.target_tbid.to_hex(),
                            elapsed_ms = elapsed_ns / 1_000_000,
                            "L1 ok via recent application RPC"
                        );
                        flags.l1_last_ok.store(true, Ordering::Relaxed);
                        continue;
                    }
                }

                // Secondary signal: fall back to transport-level ping.
                let peer = match executor.resolve_peer().await {
                    Ok(p) => p,
                    Err(_) => {
                        flags.l1_last_ok.store(false, Ordering::Relaxed);
                        continue;
                    }
                };
                let ok = match tokio::time::timeout(
                    TokioDuration::from_secs(5),
                    executor.do_ping(&peer),
                )
                .await
                {
                    Ok(Ok(())) => {
                        tracing::trace!(target_tbid = %executor.target_tbid.to_hex(), "L1 ok via transport ping");
                        true
                    }
                    Ok(Err(e)) => {
                        tracing::debug!(target_tbid = %executor.target_tbid.to_hex(), "L1 ping failed: {e}");
                        false
                    }
                    Err(_) => {
                        tracing::debug!(target_tbid = %executor.target_tbid.to_hex(), "L1 ping timed out");
                        false
                    }
                };
                flags.l1_last_ok.store(ok, Ordering::Relaxed);
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Phase 12.3 — L2 TBID Identity Confirmed (authenticated ping)
// ---------------------------------------------------------------------------

/// Phase 12.3: L2 pong, awaiting L2 task.
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthenticatedPong {
    pub responder_tbid: String,
    pub challenge_echo: String,
    pub signature: String,
    pub signature_algorithm: String,
}

/// Phase 12.3: L2 pong, awaiting L2 task.
#[allow(dead_code)]
pub struct UnverifiedSignatureEnvelopeAuthenticatedPong {
    inner: AuthenticatedPong,
}

#[allow(dead_code)]
impl UnverifiedSignatureEnvelopeAuthenticatedPong {
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, TransportError> {
        let inner: AuthenticatedPong =
            serde_json::from_value(v).map_err(|e| TransportError::Decode(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Verify: TBID match, challenge echo match, fast-key signature.
    /// // VERIFY(remote-tbid, fast-key)
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        challenge: &[u8],
        target_tbid: &Tbid,
    ) -> Result<CleanAuthenticated<AuthenticatedPong>, CommunerdetteError> {
        let responder = Tbid::from_hex(&self.inner.responder_tbid)
            .map_err(|_| CommunerdetteError::Structural("bad responder_tbid".into()))?;
        if responder != *target_tbid {
            return Err(CommunerdetteError::TbidMismatch {
                expected: target_tbid.to_hex(),
                actual: self.inner.responder_tbid,
            });
        }
        let echo = hex::decode(&self.inner.challenge_echo)
            .map_err(|_| CommunerdetteError::Structural("challenge_echo not hex".into()))?;
        if echo != challenge {
            return Err(CommunerdetteError::Structural(
                "challenge_echo mismatch".into(),
            ));
        }
        let sig = hex::decode(&self.inner.signature)
            .map_err(|_| CommunerdetteError::Structural("signature not hex".into()))?;
        let mut msg = Vec::new();
        msg.extend_from_slice(challenge);
        msg.extend_from_slice(target_tbid.to_hex().as_bytes());
        // VERIFY(remote-tbid, fast-key)
        let ok = crypto
            .verify_with(&target_tbid.ed25519_public_key(), "Ed25519", &msg, &sig)
            .map_err(|e| {
                CommunerdetteError::CleanAuth(CleanAuthError::Crypto(NodeError::Crypto(e)))
            })?;
        if !ok {
            return Err(CommunerdetteError::CleanAuth(
                CleanAuthError::InvalidSignature,
            ));
        }
        Ok(CleanAuthenticated::from_trusted(self.inner))
    }
}

impl Communerdette {
    /// Phase 12.3: L2 liveness, awaiting spawn.
    #[allow(dead_code)]
    pub(super) fn spawn_l2_liveness_task(
        executor: Arc<CommunerdetteExecutor>,
        interval_ms: u64,
        cancel: CancellationToken,
        flags: Arc<LivenessCycleFlags>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use rand::Rng;
            use std::sync::atomic::Ordering;
            let mut interval = tokio::time::interval(TokioDuration::from_millis(interval_ms));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {},
                }
                // Run-order: skip if L1 failed this cycle.
                if !flags.l1_last_ok.load(Ordering::Relaxed) {
                    tracing::trace!(target_tbid = %executor.target_tbid.to_hex(), "L2 skipped: L1 failed");
                    flags.l2_last_ok.store(false, Ordering::Relaxed);
                    continue;
                }
                let peer = match executor.resolve_peer().await {
                    Ok(p) => p,
                    Err(_) => {
                        flags.l2_last_ok.store(false, Ordering::Relaxed);
                        continue;
                    }
                };
                let challenge: [u8; 32] = rand::thread_rng().gen();
                let challenge_hex = hex::encode(challenge);
                let tbid_hex = executor.target_tbid.to_hex();

                let raw = match tokio::time::timeout(
                    TokioDuration::from_secs(10),
                    executor
                        .host
                        .host_execute_stamp(&peer, &tbid_hex, &challenge_hex, "auth-ping"),
                )
                .await
                {
                    Ok(Ok(v)) => v,
                    _ => {
                        flags.l2_last_ok.store(false, Ordering::Relaxed);
                        continue;
                    }
                };

                let unprocessed =
                    match UnverifiedSignatureEnvelopeAuthenticatedPong::from_json_value(raw) {
                        Ok(u) => u,
                        Err(_) => {
                            flags.l2_last_ok.store(false, Ordering::Relaxed);
                            continue;
                        }
                    };
                let ok = match unprocessed.verify(
                    &*executor.crypto,
                    &challenge,
                    &executor.target_tbid,
                ) {
                    Ok(_) => {
                        tracing::trace!(target_tbid = %tbid_hex, "L2 auth-ping ok");
                        true
                    }
                    Err(e) => {
                        tracing::warn!(target_tbid = %tbid_hex, "L2 auth-ping failed: {e:?}");
                        false
                    }
                };
                flags.l2_last_ok.store(ok, Ordering::Relaxed);
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Phase 12.4 — L3 Chronomatter Responsive
// ---------------------------------------------------------------------------

impl Communerdette {
    /// Phase 12.4: L3 liveness, awaiting spawn.
    #[allow(dead_code)]
    pub(super) fn spawn_l3_liveness_task(
        executor: Arc<CommunerdetteExecutor>,
        interval_ms: u64,
        cancel: CancellationToken,
        flags: Arc<LivenessCycleFlags>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use std::sync::atomic::Ordering;
            let mut interval = tokio::time::interval(TokioDuration::from_millis(interval_ms));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {},
                }
                // Run-order: skip if L2 failed or binding is Rejected.
                if !flags.l2_last_ok.load(Ordering::Relaxed) {
                    tracing::trace!(target_tbid = %executor.target_tbid.to_hex(), "L3 skipped: L2 failed");
                    continue;
                }
                if flags.binding_rejected.load(Ordering::Relaxed) {
                    tracing::trace!(target_tbid = %executor.target_tbid.to_hex(), "L3 skipped: binding rejected");
                    continue;
                }
                let content = b"liveness-probe".to_vec();
                match Communerdette::execute_stamp(
                    &executor,
                    content,
                    "liveness".into(),
                    TokioDuration::from_secs(15),
                )
                .await
                {
                    Ok(_) => {
                        tracing::trace!(target_tbid = %executor.target_tbid.to_hex(), "L3 stamp ok");
                    }
                    Err(e) => {
                        tracing::debug!(target_tbid = %executor.target_tbid.to_hex(), "L3 stamp failed: {e:?}");
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Phase 12.5 — Liveness Loop Policy and Run-Order Enforcement
// ---------------------------------------------------------------------------

/// Shared atomic flags for cross-task run-order enforcement (Phase 12.5).
///
/// L1 writes `l1_last_ok`; L2 reads it before running and writes `l2_last_ok`;
/// L3 reads both. `binding_rejected` is set by Communerdette state transitions.
pub(super) struct LivenessCycleFlags {
    pub l1_last_ok: std::sync::atomic::AtomicBool,
    pub l2_last_ok: std::sync::atomic::AtomicBool,
    /// Set to true when TbidBindingStatus transitions to Rejected.
    pub binding_rejected: std::sync::atomic::AtomicBool,
    /// Monotonic timestamp (ns) of last successful application RPC.
    /// Updated by queue task on stamp/get_tick/get_calendar_slice success.
    /// Read by L1 liveness task as the primary liveness signal.
    pub last_application_rpc_ns: AtomicU64,
}

impl LivenessCycleFlags {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            l1_last_ok: std::sync::atomic::AtomicBool::new(true),
            l2_last_ok: std::sync::atomic::AtomicBool::new(true),
            binding_rejected: std::sync::atomic::AtomicBool::new(false),
            last_application_rpc_ns: AtomicU64::new(0),
        })
    }
}

/// Per-relationship liveness loop configuration (Phase 12.5 — spec §12.4).
#[derive(Debug, Clone)]
pub struct LivenessPolicy {
    pub l1_interval_ms: u64,
    pub l2_interval_ms: u64,
    pub l3_interval_ms: u64,
    /// Whether to run L2 (TBID identity) health checks.
    pub run_l2: bool,
    /// Whether to run L3 (Chronomatter) health checks.
    pub run_l3: bool,
}

impl Default for LivenessPolicy {
    fn default() -> Self {
        Self {
            l1_interval_ms: 5_000,
            l2_interval_ms: 30_000,
            l3_interval_ms: 86_400_000,
            run_l2: true,
            run_l3: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 4.1-4.2 - CommunerdetteLine request methods
// ---------------------------------------------------------------------------

impl CommunerdetteLine {
    /// Fetch a calendar slice from the remote TBID.
    ///
    /// Returns clean-authenticated ChrononRecords after running the Take 3
    /// inbound gate. Each record's TBID is verified to match the target TBID.
    /// Chain verification is applied for consecutive records.
    ///
    /// Applies a timeout (default 15s) to prevent indefinite waits.
    pub async fn get_calendar_slice(
        &self,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<CleanAuthenticated<ChrononRecord>>, CommunerdetteError> {
        let timeout = TokioDuration::from_secs(15);
        let executor = CommunerdetteExecutor::new(
            Arc::clone(&self.host),
            self.target_tbid,
            Arc::clone(&self.crypto),
            Arc::clone(&self.clock),
            None,
        );
        let result =
            Communerdette::execute_calendar_slice(&executor, tick_start, count, timeout).await;
        if result.is_ok() {
            let now_ns = self.clock.now_ns().unwrap_or(0);
            let flags = self.inner.liveness_flags();
            flags
                .last_application_rpc_ns
                .store(now_ns, std::sync::atomic::Ordering::Relaxed);
        }
        result
    }

    /// Fetch a single tick from the remote TBID.
    ///
    /// Implemented as `get_calendar_slice(tick_number, 1)` with exactly-one validation.
    /// Returns `CleanAuthenticated<ChrononRecord>` after the Take 3 inbound gate.
    pub async fn get_tick(
        &self,
        tick_number: u64,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CommunerdetteError> {
        let timeout = TokioDuration::from_secs(15);
        let executor = CommunerdetteExecutor::new(
            Arc::clone(&self.host),
            self.target_tbid,
            Arc::clone(&self.crypto),
            Arc::clone(&self.clock),
            None,
        );
        let result = Communerdette::execute_tick(&executor, tick_number, timeout).await;
        if result.is_ok() {
            let now_ns = self.clock.now_ns().unwrap_or(0);
            let flags = self.inner.liveness_flags();
            flags
                .last_application_rpc_ns
                .store(now_ns, std::sync::atomic::Ordering::Relaxed);
        }
        result
    }

    /// Stamp content on the remote TBID.
    ///
    /// Returns a clean-authenticated Foretis after running the Take 3 inbound gate.
    /// The Foretis's TBID is verified to match the target TBID.
    ///
    /// This method asks the remote TBID to stamp supplied content;
    /// it does not sign local Calendar or Chronomatter messages.
    pub async fn stamp(
        &self,
        content: Vec<u8>,
        echo: String,
    ) -> Result<CleanAuthenticated<Foretis>, CommunerdetteError> {
        let timeout = TokioDuration::from_secs(15);
        let executor = CommunerdetteExecutor::new(
            Arc::clone(&self.host),
            self.target_tbid,
            Arc::clone(&self.crypto),
            Arc::clone(&self.clock),
            None,
        );
        let result = Communerdette::execute_stamp(&executor, content, echo, timeout).await;
        if result.is_ok() {
            let now_ns = self.clock.now_ns().unwrap_or(0);
            let flags = self.inner.liveness_flags();
            flags
                .last_application_rpc_ns
                .store(now_ns, std::sync::atomic::Ordering::Relaxed);
        }
        result
    }
}
///
/// Holds a reference to the private `Communerdette` but exposes only safe,
/// narrow methods. Cloning this handle does NOT create a new relationship;
/// it points at the same underlying state.
#[derive(Clone)]
pub struct CommunerdetteLine {
    target_tbid: Tbid,
    inner: Arc<Communerdette>,
    host: Arc<dyn CommunerdetteHost>,
    crypto: Arc<dyn CryptoServer>,
    clock: Arc<dyn Clock>,
}

impl CommunerdetteLine {
    /// Internal constructor — called only by Communerd's registry.
    #[doc(hidden)]
    pub(crate) fn new(
        target_tbid: Tbid,
        inner: Arc<Communerdette>,
        host: Arc<dyn CommunerdetteHost>,
        crypto: Arc<dyn CryptoServer>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target_tbid,
            inner,
            host,
            crypto,
            clock,
        }
    }

    /// Returns the TBID this line is scoped to.
    pub fn target_tbid(&self) -> Tbid {
        self.target_tbid
    }

    /// Returns a read-only diagnostics snapshot for this relationship.
    pub fn status_summary(&self) -> CommunerdetteStatusSummary {
        self.inner.status_summary()
    }

    /// Returns the cancellation token associated with this Communerdette.
    #[doc(hidden)]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.inner.shutdown_token()
    }

    /// Stamp content on the remote TBID with explicit serialization control.
    ///
    /// Serializes `content` using the specified algorithm and delegates to
    /// [`stamp`](Self::stamp). The remote TBID receives the serialized bytes
    /// (hex-encoded at the transport layer) and returns a
    /// `CleanAuthenticated<Foretis>` through the Take 3 inbound gate.
    pub async fn stamp_chronon(
        &self,
        content: &[u8],
        serialization: foretias_core::foretias::tick::SerializationAlgorithm,
        echo: String,
    ) -> Result<CleanAuthenticated<Foretis>, CommunerdetteError> {
        let serialized = serialization.serialize(content).map_err(|e| {
            CommunerdetteError::Structural(format!(
                "{} serialization failed: {e}",
                serialization.name()
            ))
        })?;
        self.stamp(serialized, echo).await
    }
}

impl std::fmt::Debug for CommunerdetteLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommunerdetteLine")
            .field("target_tbid", &self.target_tbid.to_hex())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::foretias::clean_auth::{Externalized, UnverifiedSignatureEnvelope};
    use foretias_core::foretias::encoding::{FTByteArray, FTByteVector};

    fn make_line() -> CommunerdetteLine {
        let tbid = Tbid::from_raw([0u8; 96]);
        let host: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let crypto = foretias_core::crypto_server::new_software(
            foretias_core::crypto_server::ForetiasCurve::Ed25519,
        )
        .expect("libsodium must be available")
        .into();
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        CommunerdetteLine::new(
            tbid,
            Arc::new(Communerdette::new(tbid)),
            host,
            crypto,
            clock,
        )
    }

    #[test]
    fn communerdette_line_can_be_cloned() {
        let line1 = make_line();
        let line2 = line1.clone();
        assert_eq!(line1.target_tbid(), line2.target_tbid());
    }

    #[test]
    fn clone_does_not_expose_mutable_state() {
        let line1 = make_line();
        let line2 = line1.clone();

        let summary1 = line1.status_summary();
        let summary2 = line2.status_summary();

        assert_eq!(summary1.target_tbid, summary2.target_tbid);
        assert_eq!(summary1.binding, summary2.binding);
        assert_eq!(summary1.active_route, summary2.active_route);
    }

    #[test]
    fn status_summary_is_read_only() {
        let line = make_line();
        let summary = line.status_summary();

        assert!(matches!(summary.binding, TbidBindingStatus::Unknown));
        assert_eq!(summary.active_route, ActiveRoute::Unavailable);
        assert_eq!(summary.stats.consecutive_failures, 0);
        assert_eq!(summary.stats.libp2p_successes, 0);
        assert_eq!(summary.stats.queue_depth, 0);
    }

    #[test]
    fn different_lines_for_different_tbids() {
        let tbid_a = Tbid::from_raw([0u8; 96]);
        let tbid_b = Tbid::from_raw([1u8; 96]);

        let host_a: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let host_b: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let crypto_a = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let crypto_b = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock_a: Arc<dyn Clock> = Arc::new(SystemClock);
        let clock_b: Arc<dyn Clock> = Arc::new(SystemClock);

        let line_a = CommunerdetteLine::new(
            tbid_a,
            Arc::new(Communerdette::new(tbid_a)),
            host_a,
            crypto_a,
            clock_a,
        );
        let line_b = CommunerdetteLine::new(
            tbid_b,
            Arc::new(Communerdette::new(tbid_b)),
            host_b,
            crypto_b,
            clock_b,
        );

        assert_ne!(line_a.target_tbid(), line_b.target_tbid());
    }

    #[test]
    fn same_arc_is_shared_across_clones() {
        let tbid = Tbid::from_raw([5u8; 96]);
        let inner = Arc::new(Communerdette::new(tbid));
        let host: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let line1 = CommunerdetteLine::new(
            tbid,
            Arc::clone(&inner),
            Arc::clone(&host),
            crypto.clone(),
            Arc::clone(&clock),
        );
        let line2 = CommunerdetteLine::new(
            tbid,
            Arc::clone(&inner),
            Arc::clone(&host),
            crypto.clone(),
            Arc::clone(&clock),
        );

        assert_eq!(line1.target_tbid(), line2.target_tbid());
        assert!(Arc::ptr_eq(&inner, &line1.inner));
    }

    #[test]
    fn tbid_binding_status_verified() {
        let status = TbidBindingStatus::Verified {
            peer_id: Some("test-peer".into()),
            json_rpc: Some("127.0.0.1:4002".into()),
            verified_at_ns: 1_000_000,
            proof_expires_at_ns: None,
        };
        assert!(status.is_verified());
        assert!(status.has_any_claim());
    }

    #[test]
    fn tbid_binding_status_unknown_has_no_claim() {
        let status = TbidBindingStatus::Unknown;
        assert!(!status.is_verified());
        assert!(!status.has_any_claim());
    }

    #[test]
    fn tbid_binding_status_rejected_has_no_claim() {
        let status = TbidBindingStatus::Rejected {
            reason: "bad proof".into(),
            observed_at_ns: 999,
        };
        assert!(!status.is_verified());
        assert!(!status.has_any_claim());
    }

    #[test]
    fn tbid_binding_status_claimed_is_not_verified() {
        let status = TbidBindingStatus::ClaimedByDht {
            peer_id: Some("dht-peer".into()),
            json_rpc: Some("127.0.0.1:5000".into()),
            observed_at_ns: 42,
        };
        assert!(!status.is_verified());
        assert!(status.has_any_claim());
    }

    #[test]
    fn shutdown_token_clone_does_not_cancel_original() {
        let line = make_line();
        let token1 = line.shutdown_token();
        let token2 = line.shutdown_token();

        assert!(!token1.is_cancelled());
        assert!(!token2.is_cancelled());
    }

    #[test]
    fn clean_auth_types_are_importable() {
        let _check: fn() -> bool = || {
            std::mem::size_of::<UnverifiedSignatureEnvelope<u8>>() > 0
                && std::mem::size_of::<CleanAuthenticated<u8>>() > 0
                && std::mem::size_of::<Externalized<u8>>() > 0
        };
        assert!(_check());
    }

    // ── Phase 3.4: DHT claim, missing record, route selection ───────────

    struct MockHost {
        dht_record: Option<PeerRegistrationRecord>,
        cached_record: Option<PeerRegistrationRecord>,
        swarm_available: bool,
        local_peer_id: Option<libp2p::PeerId>,
        namespace: String,
    }

    #[async_trait]
    impl CommunerdetteHost for MockHost {
        async fn host_lookup_tbid(
            &self,
            _tbid_hex: &str,
            _namespace: &str,
        ) -> Option<PeerRegistrationRecord> {
            self.dht_record.clone()
        }

        fn host_lookup_tbid_cached(&self, _tbid_hex: &str) -> Option<PeerRegistrationRecord> {
            self.cached_record.clone()
        }

        fn host_namespace(&self) -> String {
            self.namespace.clone()
        }

        fn host_swarm_available(&self) -> bool {
            self.swarm_available
        }

        fn host_local_peer_id(&self) -> Option<libp2p::PeerId> {
            self.local_peer_id
        }

        async fn host_execute_stamp(
            &self,
            _peer: &PeerAddr,
            _target_tbid: &str,
            _content_hex: &str,
            _echo: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        async fn host_execute_calendar_slice(
            &self,
            _peer: &PeerAddr,
            _tick_start: u64,
            _count: u64,
        ) -> Result<Vec<ChrononRecord>, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        async fn host_execute_channel_bind_challenge(
            &self,
            _peer: &PeerAddr,
            _nonce_hex: &str,
            _channel_id: &str,
            _requester_tbid_hex: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        async fn host_execute_ping(&self, _peer: &PeerAddr) -> Result<(), TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        fn host_sign_probity_report(
            &self,
            _report: &crate::probity::ProbityReport,
        ) -> Result<Vec<u8>, String> {
            Ok(vec![0xAA; 64])
        }

        fn host_publish_probity_report(&self, _signed_bytes: Vec<u8>) {}
    }

    fn make_record(peer_id: &str, json_rpc: &str) -> PeerRegistrationRecord {
        PeerRegistrationRecord {
            peer_id: peer_id.to_string(),
            tbid: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            multiaddr: "/ip4/127.0.0.1/tcp/9901".to_string(),
            json_rpc: json_rpc.to_string(),
            chronon_ns: 60_000_000_000,
            registered_at_ns: 1_000_000_000,
            capabilities: vec![],
            signature: Vec::new(),
        }
    }

    #[tokio::test]
    async fn dht_claim_updates_binding_to_claimed_by_dht() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);
        let host = MockHost {
            dht_record: Some(make_record("peer-123", "127.0.0.1:4002")),
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let status = comm.refresh_dht_binding(&host, 1_000_000_000).await;

        assert!(matches!(&status, TbidBindingStatus::ClaimedByDht { .. }));
        assert!(!status.is_verified()); // DHT claims are NOT verified
        assert!(status.has_any_claim());

        // State was updated
        assert_eq!(comm.binding_status(), status);
        assert_eq!(comm.route_candidates_count(), 1);
    }

    #[tokio::test]
    async fn missing_dht_record_leaves_route_unavailable() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);
        let host = MockHost {
            dht_record: None, // No DHT record
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let status = comm.refresh_dht_binding(&host, 1_000_000_000).await;

        assert!(matches!(status, TbidBindingStatus::Unknown));
        assert!(!status.has_any_claim());
        assert_eq!(comm.route_candidates_count(), 0);
    }

    #[tokio::test]
    async fn cached_dht_record_updates_binding() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);
        let host = MockHost {
            dht_record: None, // No live DHT
            cached_record: Some(make_record("peer-456", "127.0.0.1:5000")),
            swarm_available: false,
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let status = comm.refresh_dht_binding(&host, 2_000_000_000).await;

        // Cached record is checked first, so binding updates
        assert!(matches!(&status, TbidBindingStatus::ClaimedByDht { .. }));
        assert_eq!(comm.route_candidates_count(), 1);
    }

    #[test]
    fn route_selection_prefers_libp2p_when_swarm_available() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);

        // Add a candidate with a PeerId
        comm.add_route_candidate(make_record("peer-libp2p", "127.0.0.1:4002"));

        let host = MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: true, // Swarm IS available
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let route = comm.choose_route_with_host(&host);
        assert_eq!(route, ActiveRoute::Libp2pDirect);
    }

    #[test]
    fn route_selection_falls_back_to_noise_when_no_swarm() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);

        // Add a candidate with PeerId, but swarm NOT available
        comm.add_route_candidate(make_record("peer-libp2p", "127.0.0.1:4002"));

        let host = MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false, // Swarm NOT available
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let route = comm.choose_route_with_host(&host);
        // Falls back to NoiseJsonRpc since swarm is unavailable
        assert_eq!(route, ActiveRoute::NoiseJsonRpc);
    }

    #[test]
    fn route_selection_unavailable_when_no_candidates() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let comm = Communerdette::new(tbid);

        let host = MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "testnet".to_string(),
        };

        let route = comm.choose_route_with_host(&host);
        assert_eq!(route, ActiveRoute::Unavailable);
    }

    // ── Phase 4.4: Unit tests for calendar slice, tick, stamp ────────────

    fn make_test_chronon_record(tbid: &Tbid, chronon_number: u64) -> ChrononRecord {
        ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(vec![1u8; 32]),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![2u8; 64]),
            backward_foretis: FTByteVector::from(vec![3u8; 64]),
            aa_nonce: FTByteArray::from([4u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: *tbid,
        }
    }

    fn make_test_foretis(tbid: &Tbid) -> Foretis {
        Foretis {
            chronon_number: 1,
            content_hash: FTByteArray::from([5u8; 32]),
            tbid: *tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        }
    }

    #[test]
    fn gate_chronon_records_rejects_chronon_number_zero() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let bad_record = ChrononRecord {
            chronon_number: 0,
            public_key: FTByteVector::from(vec![1u8; 32]),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![]),
            backward_foretis: FTByteVector::from(vec![]),
            aa_nonce: FTByteArray::from([0u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: Tbid::from_raw([0u8; 96]),
        };

        let result = executor.gate_chronon_records(vec![bad_record]);
        assert!(matches!(result, Err(CommunerdetteError::Structural(_))));
    }

    #[test]
    fn gate_chronon_records_rejects_empty_public_key() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let bad_record = ChrononRecord {
            chronon_number: 1,
            public_key: FTByteVector::from(vec![]),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![]),
            backward_foretis: FTByteVector::from(vec![]),
            aa_nonce: FTByteArray::from([0u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: Tbid::from_raw([0u8; 96]),
        };

        let result = executor.gate_chronon_records(vec![bad_record]);
        assert!(matches!(result, Err(CommunerdetteError::Structural(_))));
    }

    #[test]
    fn gate_chronon_records_rejects_tbid_mismatch() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let other_tbid = Tbid::from_raw([1u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto,
            clock,
            None,
        );

        let record = make_test_chronon_record(&other_tbid, 1);
        let result = executor.gate_chronon_records(vec![record]);

        assert!(matches!(
            result,
            Err(CommunerdetteError::TbidMismatch { .. })
        ));
        if let Err(CommunerdetteError::TbidMismatch { expected, actual }) = result {
            assert_eq!(expected, target_tbid.to_hex());
            assert_eq!(actual, other_tbid.to_hex());
        }
    }

    #[test]
    fn gate_chronon_records_returns_clean_authenticated_on_valid_genesis() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto,
            clock,
            None,
        );

        let record = make_test_chronon_record(&target_tbid, 1);
        let result = executor.gate_chronon_records(vec![record]);

        assert!(result.is_ok());
        let records = result.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(*records[0].chronon_number(), 1);
    }

    /// Dummy CleanAuthenticated<ChrononRecord> for structural-rejection tests.
    /// The record is never reached in those tests so content doesn't need to match.
    fn dummy_chronon_record(tbid: &Tbid) -> CleanAuthenticated<ChrononRecord> {
        CleanAuthenticated::from_trusted(make_test_chronon_record(tbid, 1))
    }

    #[test]
    fn gate_foretis_rejects_chronon_number_zero() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let bad_foretis = Foretis {
            chronon_number: 0,
            content_hash: FTByteArray::from([0u8; 32]),
            tbid,
            echo: "test".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+0ns".to_string(),
        };
        let json = serde_json::json!({ "foretis": bad_foretis });
        let rec = dummy_chronon_record(&tbid);

        let result = executor.gate_foretis(json, &rec, b"");
        assert!(matches!(result, Err(CommunerdetteError::Structural(_))));
    }

    #[test]
    fn gate_foretis_rejects_empty_signature() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let bad_foretis = Foretis {
            chronon_number: 1,
            content_hash: FTByteArray::from([0u8; 32]),
            tbid,
            echo: "test".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+0ns".to_string(),
        };
        let json = serde_json::json!({ "foretis": bad_foretis });
        let rec = dummy_chronon_record(&tbid);

        let result = executor.gate_foretis(json, &rec, b"");
        assert!(matches!(result, Err(CommunerdetteError::Structural(_))));
    }

    #[test]
    fn gate_foretis_rejects_empty_signature_algorithm() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let bad_foretis = Foretis {
            chronon_number: 1,
            content_hash: FTByteArray::from([0u8; 32]),
            tbid,
            echo: "test".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+0ns".to_string(),
        };
        let json = serde_json::json!({ "foretis": bad_foretis });
        let rec = dummy_chronon_record(&tbid);

        let result = executor.gate_foretis(json, &rec, b"");
        assert!(matches!(result, Err(CommunerdetteError::Structural(_))));
    }

    #[test]
    fn gate_foretis_rejects_tbid_mismatch() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let other_tbid = Tbid::from_raw([2u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto,
            clock,
            None,
        );

        let foretis = make_test_foretis(&other_tbid);
        let json = serde_json::json!({ "foretis": foretis });
        let rec = dummy_chronon_record(&target_tbid);

        let result = executor.gate_foretis(json, &rec, b"");

        assert!(matches!(
            result,
            Err(CommunerdetteError::TbidMismatch { .. })
        ));
        if let Err(CommunerdetteError::TbidMismatch { expected, actual }) = result {
            assert_eq!(expected, target_tbid.to_hex());
            assert_eq!(actual, other_tbid.to_hex());
        }
    }

    #[test]
    fn gate_foretis_returns_clean_authenticated_on_valid() {
        use foretias_core::foretias::types::SignatureAlgorithm;

        let target_tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let content = b"valid foretis content" as &[u8];
        let chronon_number: u64 = 1;

        // v2: sign the postcard-encoded Foretis (not tbid || chronon_number || content)
        let content_hash = crypto.sha256(content).expect("sha256");

        let foretis_for_signing = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid: target_tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        let sig_input = foretis_for_signing.sig_input_bytes();

        let sig = crypto
            .sign_with(&sig_input, SignatureAlgorithm::Ed25519)
            .expect("sign");
        let pub_key = crypto.public_key();

        // ChrononRecord with the CryptoServer's real public key (from_trusted: test-local data)
        let chronon_record = CleanAuthenticated::from_trusted(ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(match pub_key {
                foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
                _ => panic!("expected Ed25519"),
            }),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![]),
            backward_foretis: FTByteVector::from(vec![]),
            aa_nonce: FTByteArray::from([0u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: target_tbid,
        });

        let foretis = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid: target_tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto.clone(),
            clock,
            None,
        );

        let json = serde_json::json!({
            "foretis": foretis,
            "signature": hex::encode(&sig),
            "signature_algorithm": "Ed25519",
        });
        let result = executor.gate_foretis(json, &chronon_record, content);
        assert!(
            result.is_ok(),
            "gate_foretis must accept valid signed Foretis: {:?}",
            result
        );
        let ca = result.unwrap();
        assert_eq!(*ca.chronon_number(), 1);
        assert!(ca.is_authenticated_quickly());
        assert!(ca.is_authenticated_quickly());
    }

    #[test]
    fn gate_foretis_rejects_foretis_with_wrong_signature() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let content = b"some content" as &[u8];
        let chronon_number: u64 = 1;
        let content_hash = crypto.sha256(content).expect("sha256");
        let pub_key = crypto.public_key();

        let chronon_record = CleanAuthenticated::from_trusted(ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(match pub_key {
                foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
                _ => panic!("expected Ed25519"),
            }),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![]),
            backward_foretis: FTByteVector::from(vec![]),
            aa_nonce: FTByteArray::from([0u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: target_tbid,
        });

        // Wrong signature — 64 bytes of garbage, not a real Ed25519 signature
        let foretis = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid: target_tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto.clone(),
            clock,
            None,
        );

        let garbage_sig = vec![0xABu8; 64];
        let json = serde_json::json!({
            "foretis": foretis,
            "signature": hex::encode(&garbage_sig),
            "signature_algorithm": "Ed25519",
        });
        let result = executor.gate_foretis(json, &chronon_record, content);
        assert!(
            matches!(result, Err(CommunerdetteError::CleanAuth(_))),
            "gate_foretis must reject Foretis with wrong signature: {:?}",
            result
        );
    }

    #[test]
    fn gate_foretis_rejects_invalid_json() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            tbid,
            crypto,
            clock,
            None,
        );

        let tbid_for_dummy = Tbid::from_raw([0u8; 96]);
        let rec = dummy_chronon_record(&tbid_for_dummy);
        let result =
            executor.gate_foretis(serde_json::Value::String("not an object".into()), &rec, b"");
        assert!(matches!(result, Err(CommunerdetteError::Transport(_))));
    }

    #[tokio::test]
    async fn execute_calendar_slice_returns_transport_error_when_no_peer() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let host: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(host, tbid, crypto, clock, None);

        let result =
            Communerdette::execute_calendar_slice(&executor, 1, 10, TokioDuration::from_secs(5))
                .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_stamp_returns_transport_error_when_no_peer() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let host: Arc<dyn CommunerdetteHost> = Arc::new(MockHost {
            dht_record: None,
            cached_record: None,
            swarm_available: false,
            local_peer_id: None,
            namespace: "test".to_string(),
        });
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(host, tbid, crypto, clock, None);

        let result = Communerdette::execute_stamp(
            &executor,
            b"hello".to_vec(),
            "test".into(),
            TokioDuration::from_secs(5),
        )
        .await;

        assert!(result.is_err());
    }

    // ── Phase 5.3: Priority policy ───────────────────────────────────────

    impl CommunerdettePriority {
        /// Returns the maximum queue depth allowed for this priority level.
        /// Higher-priority requests are allowed to accumulate deeper queues.
        pub fn max_queue_depth(&self) -> usize {
            match self {
                CommunerdettePriority::Critical => 100,
                CommunerdettePriority::High => 50,
                CommunerdettePriority::Normal => 20,
                CommunerdettePriority::Bulk => 5,
            }
        }

        /// Returns the timeout multiplier for this priority level.
        /// Critical requests get longer timeouts to avoid premature cancellation.
        pub fn timeout_multiplier(&self) -> f64 {
            match self {
                CommunerdettePriority::Critical => 3.0,
                CommunerdettePriority::High => 2.0,
                CommunerdettePriority::Normal => 1.0,
                CommunerdettePriority::Bulk => 0.5,
            }
        }

        /// Check if this priority should preempt a lower-priority request.
        pub fn can_preempt(&self, other: &CommunerdettePriority) -> bool {
            self.cmp(other) == std::cmp::Ordering::Greater
        }
    }

    #[test]
    fn priority_critical_has_longest_timeout_multiplier() {
        assert_eq!(CommunerdettePriority::Critical.timeout_multiplier(), 3.0);
        assert_eq!(CommunerdettePriority::High.timeout_multiplier(), 2.0);
        assert_eq!(CommunerdettePriority::Normal.timeout_multiplier(), 1.0);
        assert_eq!(CommunerdettePriority::Bulk.timeout_multiplier(), 0.5);
    }

    #[test]
    fn priority_critical_has_largest_max_queue_depth() {
        assert_eq!(CommunerdettePriority::Critical.max_queue_depth(), 100);
        assert_eq!(CommunerdettePriority::High.max_queue_depth(), 50);
        assert_eq!(CommunerdettePriority::Normal.max_queue_depth(), 20);
        assert_eq!(CommunerdettePriority::Bulk.max_queue_depth(), 5);
    }

    #[test]
    fn priority_can_preempt_returns_true_for_higher() {
        assert!(CommunerdettePriority::Critical.can_preempt(&CommunerdettePriority::High));
        assert!(CommunerdettePriority::Critical.can_preempt(&CommunerdettePriority::Normal));
        assert!(CommunerdettePriority::Critical.can_preempt(&CommunerdettePriority::Bulk));
        assert!(CommunerdettePriority::High.can_preempt(&CommunerdettePriority::Normal));
        assert!(CommunerdettePriority::High.can_preempt(&CommunerdettePriority::Bulk));
        assert!(CommunerdettePriority::Normal.can_preempt(&CommunerdettePriority::Bulk));
    }

    #[test]
    fn priority_can_preempt_returns_false_for_equal_or_lower() {
        assert!(!CommunerdettePriority::Critical.can_preempt(&CommunerdettePriority::Critical));
        assert!(!CommunerdettePriority::Bulk.can_preempt(&CommunerdettePriority::Critical));
        assert!(!CommunerdettePriority::Bulk.can_preempt(&CommunerdettePriority::Normal));
        assert!(!CommunerdettePriority::Normal.can_preempt(&CommunerdettePriority::Normal));
    }

    // ── Phase 5.4: Tests for priority ordering and timeouts ──────────────

    #[test]
    fn queued_command_heap_orders_by_priority_then_sequence() {
        let mut heap = BinaryHeap::new();

        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Bulk,
            sequence: 0,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Critical,
            sequence: 1,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 2,
            command: CommunerdetteCommand::Shutdown,
        });

        let first = heap.pop().unwrap();
        assert!(matches!(first.priority, CommunerdettePriority::Critical));
        assert_eq!(first.sequence, 1);

        let second = heap.pop().unwrap();
        assert!(matches!(second.priority, CommunerdettePriority::Normal));
        assert_eq!(second.sequence, 2);

        let third = heap.pop().unwrap();
        assert!(matches!(third.priority, CommunerdettePriority::Bulk));
        assert_eq!(third.sequence, 0);
    }

    #[test]
    fn queued_command_same_priority_orders_by_earlier_sequence_first() {
        let mut heap = BinaryHeap::new();

        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 5,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 2,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 8,
            command: CommunerdetteCommand::Shutdown,
        });

        let first = heap.pop().unwrap();
        assert_eq!(first.sequence, 2);

        let second = heap.pop().unwrap();
        assert_eq!(second.sequence, 5);

        let third = heap.pop().unwrap();
        assert_eq!(third.sequence, 8);
    }

    #[test]
    fn queued_command_mixed_priorities_fifo_within_priority() {
        let mut heap = BinaryHeap::new();

        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Critical,
            sequence: 0,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::High,
            sequence: 1,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::Critical,
            sequence: 2,
            command: CommunerdetteCommand::Shutdown,
        });
        heap.push(QueuedCommand {
            priority: CommunerdettePriority::High,
            sequence: 3,
            command: CommunerdetteCommand::Shutdown,
        });

        let first = heap.pop().unwrap();
        assert!(matches!(first.priority, CommunerdettePriority::Critical));
        assert_eq!(first.sequence, 0);

        let second = heap.pop().unwrap();
        assert!(matches!(second.priority, CommunerdettePriority::Critical));
        assert_eq!(second.sequence, 2);

        let third = heap.pop().unwrap();
        assert!(matches!(third.priority, CommunerdettePriority::High));
        assert_eq!(third.sequence, 1);

        let fourth = heap.pop().unwrap();
        assert!(matches!(fourth.priority, CommunerdettePriority::High));
        assert_eq!(fourth.sequence, 3);
    }

    #[test]
    fn queued_command_ord_is_consistent() {
        let a = QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 42,
            command: CommunerdetteCommand::Shutdown,
        };
        let b = QueuedCommand {
            priority: CommunerdettePriority::Normal,
            sequence: 42,
            command: CommunerdetteCommand::Shutdown,
        };

        assert_eq!(a, b);
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        assert_eq!(a.partial_cmp(&b), Some(std::cmp::Ordering::Equal));
    }

    #[test]
    fn queued_command_different_priorities_are_not_equal() {
        let a = QueuedCommand {
            priority: CommunerdettePriority::Critical,
            sequence: 0,
            command: CommunerdetteCommand::Shutdown,
        };
        let b = QueuedCommand {
            priority: CommunerdettePriority::Bulk,
            sequence: 0,
            command: CommunerdetteCommand::Shutdown,
        };

        assert_ne!(a, b);
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Greater);
    }

    // ── Phase 7.4 — bad-auth gate tests ──────────────────────────────────────

    /// A chained ChrononRecord with a cryptographically wrong forward/backward
    /// signature must not produce `CleanAuthenticated<ChrononRecord>`.
    ///
    /// Genesis (tick 1, tb_version=0) passes without crypto verification (PQC
    /// genesis is deferred). Tick 2 onward requires `verify_pair`, which calls
    /// `crypto.verify_with` on the chained signatures. Garbage bytes there must
    /// produce a `CleanAuth` error and block the record from being authenticated.
    #[test]
    fn gate_chronon_records_rejects_bad_chained_signature() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto.clone(),
            clock,
            None,
        );

        let record1 = make_test_chronon_record(&target_tbid, 1);
        // Record 2 with deliberately wrong (garbage) forward/backward signatures.
        let record2 = make_test_chronon_record(&target_tbid, 2);

        let result = executor.gate_chronon_records(vec![record1, record2]);

        assert!(
            result.is_err(),
            "chained record with bad signature must not produce CleanAuthenticated"
        );
        assert!(
            matches!(result, Err(CommunerdetteError::CleanAuth(_))),
            "expected CleanAuth error, got: {:?}",
            result.err()
        );
    }

    /// Phase 11.1 — gate_foretis now performs full fast-key signature verification.
    /// A Foretis reply with matching TBID but garbage signature bytes must be
    /// rejected with CleanAuth error. (Previously gate_foretis used from_trusted
    /// and silently accepted bad signatures — that was the known correctness bug.)
    #[test]
    fn gate_foretis_phase11_rejects_bad_signature_after_fix() {
        let target_tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let content = b"content bytes" as &[u8];
        let content_hash = crypto.sha256(content).expect("sha256");
        let pub_key = crypto.public_key();

        let chronon_record = CleanAuthenticated::from_trusted(ChrononRecord {
            chronon_number: 1,
            public_key: FTByteVector::from(match pub_key {
                foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
                _ => panic!("expected Ed25519"),
            }),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![]),
            backward_foretis: FTByteVector::from(vec![]),
            aa_nonce: FTByteArray::from([0u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: target_tbid,
        });

        let foretis_bad_sig = Foretis {
            chronon_number: 1,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid: target_tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        let garbage_sig = vec![0xDEu8; 64];
        let json = serde_json::json!({
            "foretis": foretis_bad_sig,
            "signature": hex::encode(&garbage_sig),
            "signature_algorithm": "Ed25519",
        });

        let executor = CommunerdetteExecutor::new(
            Arc::new(MockHost {
                dht_record: None,
                cached_record: None,
                swarm_available: false,
                local_peer_id: None,
                namespace: "test".to_string(),
            }),
            target_tbid,
            crypto.clone(),
            clock,
            None,
        );

        let result = executor.gate_foretis(json, &chronon_record, content);
        assert!(
            matches!(result, Err(CommunerdetteError::CleanAuth(_))),
            "gate_foretis must reject Foretis with garbage signature: {:?}",
            result
        );
    }

    // ── Phase 12.0 — Channel-Binding unit tests ───────────────────────────────

    /// Build a valid dual-key signed ChannelBindResponse for testing.
    fn make_valid_bind_response(
        _crypto: &dyn CryptoServer,
        nonce: &[u8],
        channel_id: &str,
        tbid: &Tbid,
        tbid_secret: &foretias_core::foretias::types::TbidSecret,
    ) -> serde_json::Value {
        let responder_tbid_hex = tbid.to_hex();
        let mut msg = Vec::new();
        msg.extend_from_slice(nonce);
        msg.extend_from_slice(channel_id.as_bytes());
        msg.extend_from_slice(responder_tbid_hex.as_bytes());

        let combined = tbid_secret.sign(&msg).expect("sign");
        let sig_bytes = combined.as_bytes();
        serde_json::json!({
            "responder_tbid": responder_tbid_hex,
            "nonce_echo": hex::encode(nonce),
            "channel_id": channel_id,
            "fast_sig": hex::encode(&sig_bytes[..64]),
            "slow_sig": hex::encode(&sig_bytes[64..]),
        })
    }

    #[test]
    fn channel_bind_verify_full_accepts_valid_dual_sig() {
        use foretias_core::foretias::types::TbidSecret;
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let (tbid, secret) = TbidSecret::generate().expect("keygen");
        let nonce = b"test_nonce_32bytes_paddedxxxxxx!!" as &[u8];
        let channel_id = "127.0.0.1:9000";

        let resp_json = make_valid_bind_response(&*crypto, nonce, channel_id, &tbid, &secret);
        let unprocessed =
            UnverifiedSignatureEnvelopeChannelBinding::from_json_value(resp_json).expect("parse");
        let result = unprocessed.verify_full(&*crypto, nonce, channel_id, &tbid);
        assert!(
            result.is_ok(),
            "verify_full must accept valid dual-signed response: {:?}",
            result
        );
        let binding = result.unwrap();
        assert!(binding.is_authenticated_fully());
        assert!(binding.is_authenticated_quickly());
    }

    #[test]
    fn channel_bind_verify_full_rejects_wrong_fast_sig() {
        use foretias_core::foretias::types::TbidSecret;
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let (tbid, secret) = TbidSecret::generate().expect("keygen");
        let nonce = b"test_nonce_32bytes_paddedxxxxxx!!" as &[u8];
        let channel_id = "127.0.0.1:9000";

        let mut resp_json = make_valid_bind_response(&*crypto, nonce, channel_id, &tbid, &secret);
        // Corrupt fast_sig
        resp_json["fast_sig"] = serde_json::json!(hex::encode(vec![0xABu8; 64]));
        let unprocessed =
            UnverifiedSignatureEnvelopeChannelBinding::from_json_value(resp_json).expect("parse");
        let result = unprocessed.verify_full(&*crypto, nonce, channel_id, &tbid);
        assert!(
            matches!(result, Err(CommunerdetteError::CleanAuth(_))),
            "must reject wrong fast_sig: {:?}",
            result
        );
    }

    #[test]
    fn channel_bind_verify_full_rejects_wrong_slow_sig() {
        use foretias_core::foretias::types::TbidSecret;
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let (tbid, secret) = TbidSecret::generate().expect("keygen");
        let nonce = b"test_nonce_32bytes_paddedxxxxxx!!" as &[u8];
        let channel_id = "127.0.0.1:9000";

        let mut resp_json = make_valid_bind_response(&*crypto, nonce, channel_id, &tbid, &secret);
        // Corrupt slow_sig with wrong-length garbage
        resp_json["slow_sig"] = serde_json::json!(hex::encode(vec![0xCDu8; 100]));
        let unprocessed =
            UnverifiedSignatureEnvelopeChannelBinding::from_json_value(resp_json).expect("parse");
        let result = unprocessed.verify_full(&*crypto, nonce, channel_id, &tbid);
        assert!(result.is_err(), "must reject wrong slow_sig: {:?}", result);
    }

    #[test]
    fn channel_bind_verify_full_rejects_nonce_mismatch() {
        use foretias_core::foretias::types::TbidSecret;
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let (tbid, secret) = TbidSecret::generate().expect("keygen");
        let nonce = b"test_nonce_32bytes_paddedxxxxxx!!" as &[u8];
        let channel_id = "127.0.0.1:9000";

        let resp_json = make_valid_bind_response(&*crypto, nonce, channel_id, &tbid, &secret);
        let unprocessed =
            UnverifiedSignatureEnvelopeChannelBinding::from_json_value(resp_json).expect("parse");
        let wrong_nonce = b"different_nonce_32bytes_paddxx!!" as &[u8];
        let result = unprocessed.verify_full(&*crypto, wrong_nonce, channel_id, &tbid);
        assert!(
            matches!(result, Err(CommunerdetteError::Structural(_))),
            "must reject nonce mismatch: {:?}",
            result
        );
    }

    #[test]
    fn channel_bind_verify_full_rejects_tbid_mismatch() {
        use foretias_core::foretias::types::TbidSecret;
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let (tbid, secret) = TbidSecret::generate().expect("keygen");
        let (other_tbid, _) = TbidSecret::generate().expect("keygen");
        let nonce = b"test_nonce_32bytes_paddedxxxxxx!!" as &[u8];
        let channel_id = "127.0.0.1:9000";

        let resp_json = make_valid_bind_response(&*crypto, nonce, channel_id, &tbid, &secret);
        let unprocessed =
            UnverifiedSignatureEnvelopeChannelBinding::from_json_value(resp_json).expect("parse");
        let result = unprocessed.verify_full(&*crypto, nonce, channel_id, &other_tbid);
        assert!(
            matches!(result, Err(CommunerdetteError::TbidMismatch { .. })),
            "must reject TBID mismatch: {:?}",
            result
        );
    }

    // ── Phase 4.4: execute_tick and execute_stamp integration ────────────

    /// A configurable mock host for Phase 4.4 tests — returns caller-supplied
    /// calendar slice and stamp responses rather than always erroring.
    struct ConfigurableMockHost {
        dht_record: Option<PeerRegistrationRecord>,
        calendar_response: Mutex<Option<Vec<ChrononRecord>>>,
        stamp_response: Mutex<Option<serde_json::Value>>,
    }

    impl ConfigurableMockHost {
        fn new(dht_record: Option<PeerRegistrationRecord>) -> Self {
            Self {
                dht_record,
                calendar_response: Mutex::new(None),
                stamp_response: Mutex::new(None),
            }
        }

        fn set_calendar_response(&self, records: Vec<ChrononRecord>) {
            *self.calendar_response.lock() = Some(records);
        }

        fn set_stamp_response(&self, v: serde_json::Value) {
            *self.stamp_response.lock() = Some(v);
        }
    }

    #[async_trait]
    impl CommunerdetteHost for ConfigurableMockHost {
        async fn host_lookup_tbid(
            &self,
            _tbid_hex: &str,
            _namespace: &str,
        ) -> Option<PeerRegistrationRecord> {
            self.dht_record.clone()
        }
        fn host_lookup_tbid_cached(&self, _tbid_hex: &str) -> Option<PeerRegistrationRecord> {
            self.dht_record.clone()
        }
        fn host_namespace(&self) -> String {
            "test".to_string()
        }
        fn host_swarm_available(&self) -> bool {
            false
        }
        fn host_local_peer_id(&self) -> Option<libp2p::PeerId> {
            None
        }

        async fn host_execute_stamp(
            &self,
            _peer: &PeerAddr,
            _target_tbid: &str,
            _content_hex: &str,
            _echo: &str,
        ) -> Result<serde_json::Value, TransportError> {
            self.stamp_response
                .lock()
                .clone()
                .ok_or_else(|| TransportError::Unsupported("no stamp response set".into()))
        }

        async fn host_execute_calendar_slice(
            &self,
            _peer: &PeerAddr,
            _tick_start: u64,
            _count: u64,
        ) -> Result<Vec<ChrononRecord>, TransportError> {
            self.calendar_response
                .lock()
                .clone()
                .ok_or_else(|| TransportError::Unsupported("no calendar response set".into()))
        }

        async fn host_execute_channel_bind_challenge(
            &self,
            _peer: &PeerAddr,
            _nonce_hex: &str,
            _channel_id: &str,
            _requester_tbid_hex: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        async fn host_execute_ping(&self, _peer: &PeerAddr) -> Result<(), TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        fn host_sign_probity_report(
            &self,
            _report: &crate::probity::ProbityReport,
        ) -> Result<Vec<u8>, String> {
            Ok(vec![0xBB; 64])
        }

        fn host_publish_probity_report(&self, _signed_bytes: Vec<u8>) {}
    }

    #[tokio::test]
    async fn execute_tick_returns_error_on_empty_slice() {
        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let host = Arc::new(ConfigurableMockHost::new(Some(make_record(
            "peer-1",
            "127.0.0.1:4002",
        ))));
        // Return empty slice — execute_tick must error.
        host.set_calendar_response(vec![]);

        let executor = CommunerdetteExecutor::new(host, tbid, crypto, clock, None);
        let result = Communerdette::execute_tick(&executor, 1, TokioDuration::from_secs(5)).await;

        assert!(
            result.is_err(),
            "execute_tick must error when calendar slice is empty"
        );
        assert!(
            matches!(result, Err(CommunerdetteError::Structural(_))),
            "error must be Structural, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn execute_stamp_produces_clean_authenticated_foretis() {
        use foretias_core::foretias::types::SignatureAlgorithm;

        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let content = b"stamp-test-content" as &[u8];
        let chronon_number: u64 = 1;

        // v2: sign the postcard-encoded Foretis
        let content_hash = crypto.sha256(content).expect("sha256");

        let foretis_for_signing = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        let sig = crypto
            .sign_with(
                &foretis_for_signing.sig_input_bytes(),
                SignatureAlgorithm::Ed25519,
            )
            .expect("sign");
        let pub_key_bytes = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };

        // ChrononRecord whose public_key matches the crypto server — genesis, tb_version=0.
        let chronon_record = ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(pub_key_bytes),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![2u8; 64]),
            backward_foretis: FTByteVector::from(vec![3u8; 64]),
            aa_nonce: FTByteArray::from([4u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid,
        };

        let host = Arc::new(ConfigurableMockHost::new(Some(make_record(
            "peer-1",
            "127.0.0.1:4002",
        ))));
        host.set_calendar_response(vec![chronon_record]);
        host.set_stamp_response(serde_json::json!({
            "foretis": foretis_for_signing,
            "signature": hex::encode(&sig),
            "signature_algorithm": "Ed25519",
        }));

        let executor = CommunerdetteExecutor::new(host, tbid, crypto, clock, None);
        let result = Communerdette::execute_stamp(
            &executor,
            content.to_vec(),
            "test".to_string(),
            TokioDuration::from_secs(5),
        )
        .await;

        assert!(
            result.is_ok(),
            "execute_stamp must succeed with valid signed Foretis: {:?}",
            result
        );
        let ca = result.unwrap();
        assert_eq!(
            *ca.tbid(),
            tbid,
            "returned Foretis must carry the target TBID"
        );
        assert_eq!(*ca.chronon_number(), chronon_number);
        assert!(
            ca.is_authenticated_quickly(),
            "returned Foretis must be authenticated"
        );
        assert!(ca.is_authenticated_quickly());
    }

    // ── Phase 13.5 — FB emission unit tests ────────────────────────────────────

    /// Mock host that captures sign and publish calls for FB emission assertions.
    struct FBMockHost {
        sign_calls: std::sync::Arc<Mutex<Vec<crate::probity::ProbityReport>>>,
        publish_calls: std::sync::Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl FBMockHost {
        fn new() -> (
            Self,
            std::sync::Arc<Mutex<Vec<crate::probity::ProbityReport>>>,
            std::sync::Arc<Mutex<Vec<Vec<u8>>>>,
        ) {
            let sign_calls = std::sync::Arc::new(Mutex::new(Vec::new()));
            let publish_calls = std::sync::Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    sign_calls: std::sync::Arc::clone(&sign_calls),
                    publish_calls: std::sync::Arc::clone(&publish_calls),
                },
                sign_calls,
                publish_calls,
            )
        }
    }

    #[async_trait]
    impl CommunerdetteHost for FBMockHost {
        async fn host_lookup_tbid(
            &self,
            _tbid_hex: &str,
            _namespace: &str,
        ) -> Option<PeerRegistrationRecord> {
            None
        }
        fn host_lookup_tbid_cached(&self, _tbid_hex: &str) -> Option<PeerRegistrationRecord> {
            None
        }
        fn host_namespace(&self) -> String {
            "test".to_string()
        }
        fn host_swarm_available(&self) -> bool {
            false
        }
        fn host_local_peer_id(&self) -> Option<libp2p::PeerId> {
            None
        }
        async fn host_execute_stamp(
            &self,
            _: &PeerAddr,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }
        async fn host_execute_calendar_slice(
            &self,
            _: &PeerAddr,
            _: u64,
            _: u64,
        ) -> Result<Vec<ChrononRecord>, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }
        async fn host_execute_channel_bind_challenge(
            &self,
            _: &PeerAddr,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }
        async fn host_execute_ping(&self, _: &PeerAddr) -> Result<(), TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }
        // SIGN(report → host_sign_probity_report)
        fn host_sign_probity_report(
            &self,
            report: &crate::probity::ProbityReport,
        ) -> Result<Vec<u8>, String> {
            self.sign_calls.lock().push(report.clone());
            Ok(vec![0xAA; 64])
        }
        fn host_publish_probity_report(&self, signed_bytes: Vec<u8>) {
            self.publish_calls.lock().push(signed_bytes);
        }
    }

    #[test]
    fn emit_fb_established_builds_correct_report() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([1u8; 96]);
        let local_calendar_tbid = Some("local-cal-tbid-hex".to_string());

        let (host_impl, sign_calls, publish_calls) = FBMockHost::new();
        let host: Arc<dyn CommunerdetteHost> = Arc::new(host_impl);
        let executor = CommunerdetteExecutor::new(
            Arc::clone(&host),
            target_tbid,
            crypto,
            clock,
            local_calendar_tbid,
        );

        executor.emit_fb_report(1.0);

        let calls = sign_calls.lock();
        assert_eq!(
            calls.len(),
            1,
            "host_sign_probity_report must be called exactly once"
        );
        let report = &calls[0];
        assert_eq!(report.attribute, "fb", "attribute must be 'fb'");
        assert_eq!(report.value, 1.0, "value must be 1.0 for established");
        assert_eq!(
            report.reporter, "local-cal-tbid-hex",
            "reporter must be local_calendar_tbid"
        );
        assert_eq!(
            report.subject,
            target_tbid.to_hex(),
            "subject must be target_tbid"
        );
        assert_eq!(report.curve, 1, "curve must be Ed25519 (1)");
        assert!(
            report.signature.is_empty(),
            "signature must be empty (pre-sign)"
        );
        drop(calls);

        let pubs = publish_calls.lock();
        assert_eq!(
            pubs.len(),
            1,
            "host_publish_probity_report must be called exactly once"
        );
        assert_eq!(
            pubs[0],
            vec![0xAAu8; 64],
            "published bytes must match signed output"
        );
    }

    #[test]
    fn emit_fb_lost_builds_correct_report() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([2u8; 96]);
        let local_calendar_tbid = Some("local-cal-tbid-hex".to_string());

        let (host_impl, sign_calls, publish_calls) = FBMockHost::new();
        let host: Arc<dyn CommunerdetteHost> = Arc::new(host_impl);
        let executor = CommunerdetteExecutor::new(
            Arc::clone(&host),
            target_tbid,
            crypto,
            clock,
            local_calendar_tbid,
        );

        executor.emit_fb_report(-1.0);

        let calls = sign_calls.lock();
        assert_eq!(
            calls.len(),
            1,
            "host_sign_probity_report must be called exactly once"
        );
        let report = &calls[0];
        assert_eq!(report.attribute, "fb", "attribute must be 'fb'");
        assert_eq!(report.value, -1.0, "value must be -1.0 for lost");
        assert_eq!(
            report.reporter, "local-cal-tbid-hex",
            "reporter must be local_calendar_tbid"
        );
        assert_eq!(
            report.subject,
            target_tbid.to_hex(),
            "subject must be target_tbid"
        );
        drop(calls);

        let pubs = publish_calls.lock();
        assert_eq!(
            pubs.len(),
            1,
            "host_publish_probity_report must be called exactly once"
        );
    }

    #[test]
    fn emit_fb_skipped_when_no_local_calendar_tbid() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([3u8; 96]);

        let (host_impl, sign_calls, publish_calls) = FBMockHost::new();
        let host: Arc<dyn CommunerdetteHost> = Arc::new(host_impl);
        let executor =
            CommunerdetteExecutor::new(Arc::clone(&host), target_tbid, crypto, clock, None);

        executor.emit_fb_report(1.0);

        let calls = sign_calls.lock();
        assert_eq!(
            calls.len(),
            0,
            "host_sign_probity_report must NOT be called when local_calendar_tbid is None"
        );
        drop(calls);

        let pubs = publish_calls.lock();
        assert_eq!(
            pubs.len(),
            0,
            "host_publish_probity_report must NOT be called when local_calendar_tbid is None"
        );
    }

    // ── Phase 12 — Liveness task unit tests ──────────────────────────────────

    /// Mock host for liveness task tests. Supports configurable responses for
    /// ping (L1), stamp with optional dynamic signing (L2), and calendar (L3).
    struct LivenessMockHost {
        dht_record: Option<PeerRegistrationRecord>,
        ping_succeeds: bool,
        stamp_response: Mutex<Option<serde_json::Value>>,
        calendar_response: Mutex<Option<Vec<ChrononRecord>>>,
        /// If set, dynamically signs auth-ping challenges using this crypto server.
        auth_ping_crypto: Option<Arc<dyn CryptoServer>>,
        /// TBID hex — must match for dynamic signing to activate.
        target_tbid_hex: String,
        /// If true, corrupt the challenge_echo in the dynamic auth-ping response.
        corrupt_challenge_echo: bool,
        /// Counts host_execute_stamp calls (for verifying L3 skip behavior).
        stamp_call_count: std::sync::atomic::AtomicU64,
    }

    impl LivenessMockHost {
        fn new(dht_record: Option<PeerRegistrationRecord>, ping_succeeds: bool) -> Self {
            Self {
                dht_record,
                ping_succeeds,
                stamp_response: Mutex::new(None),
                calendar_response: Mutex::new(None),
                auth_ping_crypto: None,
                target_tbid_hex: String::new(),
                corrupt_challenge_echo: false,
                stamp_call_count: std::sync::atomic::AtomicU64::new(0),
            }
        }

        fn with_auth_ping_crypto(
            mut self,
            crypto: Arc<dyn CryptoServer>,
            target_tbid_hex: String,
        ) -> Self {
            self.auth_ping_crypto = Some(crypto);
            self.target_tbid_hex = target_tbid_hex;
            self
        }

        fn with_corrupt_challenge_echo(mut self) -> Self {
            self.corrupt_challenge_echo = true;
            self
        }

        fn set_stamp_response(&self, v: serde_json::Value) {
            *self.stamp_response.lock() = Some(v);
        }

        fn set_calendar_response(&self, records: Vec<ChrononRecord>) {
            *self.calendar_response.lock() = Some(records);
        }
    }

    #[async_trait]
    impl CommunerdetteHost for LivenessMockHost {
        async fn host_lookup_tbid(
            &self,
            _tbid_hex: &str,
            _namespace: &str,
        ) -> Option<PeerRegistrationRecord> {
            self.dht_record.clone()
        }

        fn host_lookup_tbid_cached(&self, _tbid_hex: &str) -> Option<PeerRegistrationRecord> {
            self.dht_record.clone()
        }

        fn host_namespace(&self) -> String {
            "test".to_string()
        }
        fn host_swarm_available(&self) -> bool {
            false
        }
        fn host_local_peer_id(&self) -> Option<libp2p::PeerId> {
            None
        }

        async fn host_execute_stamp(
            &self,
            _peer: &PeerAddr,
            target_tbid: &str,
            content_hex: &str,
            _echo: &str,
        ) -> Result<serde_json::Value, TransportError> {
            self.stamp_call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Dynamic signing for auth-ping success tests
            if let Some(ref crypto) = self.auth_ping_crypto {
                if target_tbid == self.target_tbid_hex {
                    let challenge = hex::decode(content_hex)
                        .map_err(|_| TransportError::Decode("bad hex".into()))?;
                    let mut msg = challenge;
                    msg.extend_from_slice(target_tbid.as_bytes());
                    // SIGN(challenge || target_tbid_hex)
                    let sig = crypto
                        .sign_with(
                            &msg,
                            foretias_core::foretias::types::SignatureAlgorithm::Ed25519,
                        )
                        .map_err(|e| TransportError::Decode(e.to_string()))?;
                    let echo = if self.corrupt_challenge_echo {
                        "0000000000000000000000000000000000000000000000000000000000000000"
                            .to_string()
                    } else {
                        content_hex.to_string()
                    };
                    return Ok(serde_json::json!({
                        "responder_tbid": target_tbid,
                        "challenge_echo": echo,
                        "signature": hex::encode(&sig),
                        "signature_algorithm": "Ed25519",
                    }));
                }
            }
            // Static fallback
            self.stamp_response
                .lock()
                .clone()
                .ok_or_else(|| TransportError::Unsupported("no stamp response set".into()))
        }

        async fn host_execute_calendar_slice(
            &self,
            _peer: &PeerAddr,
            _tick_start: u64,
            _count: u64,
        ) -> Result<Vec<ChrononRecord>, TransportError> {
            self.calendar_response
                .lock()
                .clone()
                .ok_or_else(|| TransportError::Unsupported("no calendar response set".into()))
        }

        async fn host_execute_channel_bind_challenge(
            &self,
            _peer: &PeerAddr,
            _nonce_hex: &str,
            _channel_id: &str,
            _requester_tbid_hex: &str,
        ) -> Result<serde_json::Value, TransportError> {
            Err(TransportError::Unsupported("mock".into()))
        }

        async fn host_execute_ping(&self, _peer: &PeerAddr) -> Result<(), TransportError> {
            if self.ping_succeeds {
                Ok(())
            } else {
                Err(TransportError::Unsupported("mock ping failed".into()))
            }
        }

        fn host_sign_probity_report(
            &self,
            _report: &crate::probity::ProbityReport,
        ) -> Result<Vec<u8>, String> {
            Ok(vec![0xCC; 64])
        }

        fn host_publish_probity_report(&self, _signed_bytes: Vec<u8>) {}
    }

    // ── L1 tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn l1_task_records_success_on_pong() {
        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(LivenessMockHost::new(Some(record), true));
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0xAA; 96]);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l1_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(200)).await;

        assert!(
            flags.l1_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L1 must record success when ping returns Ok"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l1_task_records_failure_on_error() {
        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(LivenessMockHost::new(Some(record), false));
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0xBB; 96]);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l1_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(200)).await;

        assert!(
            !flags.l1_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L1 must record failure when ping returns Err"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    // ── L2 tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn l2_promotes_claimed_by_dht_to_verified() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid_bytes = [0u8; 96];
        tbid_bytes[..32].copy_from_slice(&pub_key);
        let target_tbid = Tbid::from_raw(tbid_bytes);
        let target_tbid_hex = target_tbid.to_hex();

        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(
            LivenessMockHost::new(Some(record), true)
                .with_auth_ping_crypto(Arc::clone(&crypto), target_tbid_hex),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l2_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(300)).await;

        assert!(
            flags.l2_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L2 must record success when auth-ping verifies correctly"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l2_sets_rejected_on_wrong_tbid() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let target_tbid = Tbid::from_raw([0xCC; 96]);

        let record = make_record("peer-1", "127.0.0.1:4002");
        let host_inner = LivenessMockHost::new(Some(record), true);
        let wrong_tbid_hex = hex::encode([0xAAu8; 96]);
        host_inner.set_stamp_response(serde_json::json!({
            "responder_tbid": wrong_tbid_hex,
            "challenge_echo": "0000000000000000000000000000000000000000000000000000000000000000",
            "signature": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "signature_algorithm": "Ed25519",
        }));
        let host: Arc<dyn CommunerdetteHost> = Arc::new(host_inner);

        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l2_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(300)).await;

        assert!(
            !flags.l2_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L2 must fail when responder_tbid doesn't match target"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l2_sets_rejected_on_wrong_challenge_echo() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid_bytes = [0u8; 96];
        tbid_bytes[..32].copy_from_slice(&pub_key);
        let target_tbid = Tbid::from_raw(tbid_bytes);
        let target_tbid_hex = target_tbid.to_hex();

        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(
            LivenessMockHost::new(Some(record), true)
                .with_auth_ping_crypto(Arc::clone(&crypto), target_tbid_hex)
                .with_corrupt_challenge_echo(),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l2_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(300)).await;

        assert!(
            !flags.l2_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L2 must fail when challenge_echo doesn't match the challenge"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l2_sets_rejected_on_invalid_signature() {
        let crypto_executor: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        // Different key pair — will produce an invalid signature
        let crypto_wrong: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let pub_key = match crypto_executor.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid_bytes = [0u8; 96];
        tbid_bytes[..32].copy_from_slice(&pub_key);
        let target_tbid = Tbid::from_raw(tbid_bytes);
        let target_tbid_hex = target_tbid.to_hex();

        let record = make_record("peer-1", "127.0.0.1:4002");
        // Mock signs with wrong key — signature won't verify against TBID's public key
        let host: Arc<dyn CommunerdetteHost> = Arc::new(
            LivenessMockHost::new(Some(record), true)
                .with_auth_ping_crypto(crypto_wrong, target_tbid_hex),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto_executor,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l2_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(300)).await;

        assert!(
            !flags.l2_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L2 must fail when signature is invalid (wrong key)"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    // ── L3 tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn l3_records_success_on_valid_foretis() {
        use foretias_core::foretias::types::SignatureAlgorithm;

        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid_bytes = [0u8; 96];
        tbid_bytes[..32].copy_from_slice(&pub_key);
        let target_tbid = Tbid::from_raw(tbid_bytes);
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        // Build the Foretis that L3's execute_stamp will receive
        let content = b"liveness-probe";
        let content_hash = crypto.sha256(content).expect("sha256");
        let foretis_for_signing = Foretis {
            chronon_number: 1,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid: target_tbid,
            echo: "liveness".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        // SIGN(foretis_sig_input_bytes)
        let sig = crypto
            .sign_with(
                &foretis_for_signing.sig_input_bytes(),
                SignatureAlgorithm::Ed25519,
            )
            .expect("sign");

        // ChrononRecord with matching public key (genesis, tb_version=0)
        let chronon_record = ChrononRecord {
            chronon_number: 1,
            public_key: FTByteVector::from(pub_key),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![2u8; 64]),
            backward_foretis: FTByteVector::from(vec![3u8; 64]),
            aa_nonce: FTByteArray::from([4u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid: target_tbid,
        };

        let record = make_record("peer-1", "127.0.0.1:4002");
        let host = Arc::new(LivenessMockHost::new(Some(record), true));
        host.set_calendar_response(vec![chronon_record]);
        host.set_stamp_response(serde_json::json!({
            "foretis": foretis_for_signing,
            "signature": hex::encode(&sig),
            "signature_algorithm": "Ed25519",
        }));

        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l3_liveness_task(executor, 200, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(500)).await;

        // L3 doesn't write to flags — verify the task ran without panicking
        cancel.cancel();
        let result = handle.await;
        assert!(result.is_ok(), "L3 task must not panic with valid Foretis");
    }

    #[tokio::test]
    async fn l3_records_failure_on_gate_reject() {
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0xDD; 96]);

        let record = make_record("peer-1", "127.0.0.1:4002");
        let host = Arc::new(LivenessMockHost::new(Some(record), true));
        let host_ref = Arc::clone(&host);

        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        // L2 is OK but binding is rejected — L3 must skip
        flags
            .binding_rejected
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let cancel = CancellationToken::new();

        let handle =
            Communerdette::spawn_l3_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(300)).await;

        assert_eq!(
            host_ref
                .stamp_call_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "L3 must skip execute_stamp when binding_rejected is true"
        );

        cancel.cancel();
        let result = handle.await;
        assert!(
            result.is_ok(),
            "L3 task must not panic when binding is rejected"
        );
    }

    // ── Application RPC liveness hierarchy tests ────────────────────────

    #[tokio::test]
    async fn l1_skips_ping_when_recent_application_rpc_exists() {
        let record = make_record("peer-1", "127.0.0.1:4002");
        let ping_succeeds = false;
        let host: Arc<dyn CommunerdetteHost> =
            Arc::new(LivenessMockHost::new(Some(record), ping_succeeds));
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0xEE; 96]);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        // Record a recent application RPC success.
        let now_ns = executor.clock.now_ns().unwrap_or(0);
        flags
            .last_application_rpc_ns
            .store(now_ns, std::sync::atomic::Ordering::Relaxed);

        let handle =
            Communerdette::spawn_l1_liveness_task(executor, 500, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(200)).await;

        assert!(
            flags.l1_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L1 must record success when recent application RPC exists (even if ping would fail)"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l1_falls_back_to_ping_when_no_recent_application_rpc() {
        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(LivenessMockHost::new(Some(record), true));
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0xFF; 96]);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        // No application RPC recorded (last_application_rpc_ns = 0).
        // L1 should fall back to transport ping, which succeeds.
        let handle =
            Communerdette::spawn_l1_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(200)).await;

        assert!(
            flags.l1_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L1 must record success via transport ping when no recent application RPC"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn l1_fails_when_no_rpc_and_ping_fails() {
        let record = make_record("peer-1", "127.0.0.1:4002");
        let host: Arc<dyn CommunerdetteHost> = Arc::new(LivenessMockHost::new(Some(record), false));
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let target_tbid = Tbid::from_raw([0x11; 96]);
        let executor = Arc::new(CommunerdetteExecutor::new(
            host,
            target_tbid,
            crypto,
            clock,
            None,
        ));
        let flags = LivenessCycleFlags::new();
        let cancel = CancellationToken::new();

        // No application RPC, ping fails -> L1 must fail.
        let handle =
            Communerdette::spawn_l1_liveness_task(executor, 50, cancel.clone(), flags.clone());

        tokio::time::sleep(TokioDuration::from_millis(200)).await;

        assert!(
            !flags.l1_last_ok.load(std::sync::atomic::Ordering::Relaxed),
            "L1 must fail when no application RPC and ping fails"
        );

        cancel.cancel();
        let _ = handle.await;
    }

    #[test]
    fn record_application_rpc_success_updates_timestamp() {
        let mut stats = CommunerdetteRouteStats::default();
        assert!(stats.last_application_rpc_ns.is_none());

        stats.record_application_rpc_success(1_000_000_000);
        assert_eq!(stats.last_application_rpc_ns, Some(1_000_000_000));

        stats.record_application_rpc_success(2_000_000_000);
        assert_eq!(stats.last_application_rpc_ns, Some(2_000_000_000));
    }

    #[test]
    fn liveness_policy_default_l3_is_daily() {
        let policy = LivenessPolicy::default();
        assert_eq!(policy.l1_interval_ms, 5_000);
        assert_eq!(policy.l2_interval_ms, 30_000);
        assert_eq!(policy.l3_interval_ms, 86_400_000);
        assert!(policy.run_l2);
        assert!(policy.run_l3);
    }

    #[tokio::test]
    async fn stamp_chronon_postcard_serialization_produces_correct_content_hash() {
        use foretias_core::foretias::tick::SerializationAlgorithm;
        use foretias_core::foretias::types::SignatureAlgorithm;

        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let original_content = b"chronon stamp test payload" as &[u8];
        let serialized = SerializationAlgorithm::Postcard
            .serialize(original_content)
            .unwrap();
        let chronon_number: u64 = 1;

        let content_hash = crypto.sha256(&serialized).expect("sha256");
        let foretis_for_signing = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        let sig = crypto
            .sign_with(
                &foretis_for_signing.sig_input_bytes(),
                SignatureAlgorithm::Ed25519,
            )
            .expect("sign");
        let pub_key_bytes = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let chronon_record = ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(pub_key_bytes),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![2u8; 64]),
            backward_foretis: FTByteVector::from(vec![3u8; 64]),
            aa_nonce: FTByteArray::from([4u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid,
        };

        let host = Arc::new(ConfigurableMockHost::new(Some(make_record(
            "peer-1",
            "127.0.0.1:4002",
        ))));
        host.set_calendar_response(vec![chronon_record]);
        host.set_stamp_response(serde_json::json!({
            "foretis": foretis_for_signing,
            "signature": hex::encode(&sig),
            "signature_algorithm": "Ed25519",
        }));

        let executor = CommunerdetteExecutor::new(host, tbid, crypto.clone(), clock, None);
        let result = Communerdette::execute_stamp(
            &executor,
            serialized.clone(),
            "test".to_string(),
            TokioDuration::from_secs(5),
        )
        .await;

        assert!(
            result.is_ok(),
            "stamp_chronon with postcard must succeed: {:?}",
            result
        );
        let ca = result.unwrap();
        let expected_hash = crypto.sha256(&serialized).unwrap();
        assert_eq!(ca.content_hash().as_slice(), &expected_hash.bytes);
    }

    #[tokio::test]
    async fn stamp_chronon_bincode_serialization_produces_correct_content_hash() {
        use foretias_core::foretias::tick::SerializationAlgorithm;
        use foretias_core::foretias::types::SignatureAlgorithm;

        let tbid = Tbid::from_raw([0u8; 96]);
        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);

        let original_content = b"chronon stamp test payload" as &[u8];
        let serialized = SerializationAlgorithm::Bincode
            .serialize(original_content)
            .unwrap();
        let chronon_number: u64 = 1;

        let content_hash = crypto.sha256(&serialized).expect("sha256");
        let foretis_for_signing = Foretis {
            chronon_number,
            content_hash: FTByteArray::from(content_hash.bytes),
            tbid,
            echo: "test".to_string(),
            tbn: "test-tb".to_string(),
            time_being_reference_time: "UE+1000000000ns".to_string(),
        };
        let sig = crypto
            .sign_with(
                &foretis_for_signing.sig_input_bytes(),
                SignatureAlgorithm::Ed25519,
            )
            .expect("sign");
        let pub_key_bytes = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let chronon_record = ChrononRecord {
            chronon_number,
            public_key: FTByteVector::from(pub_key_bytes),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: FTByteVector::from(vec![2u8; 64]),
            backward_foretis: FTByteVector::from(vec![3u8; 64]),
            aa_nonce: FTByteArray::from([4u8; 16]),
            chronon_stamp_count: 0,
            external_attestations: vec![],
            tb_version: 0,
            tbid,
        };

        let host = Arc::new(ConfigurableMockHost::new(Some(make_record(
            "peer-1",
            "127.0.0.1:4002",
        ))));
        host.set_calendar_response(vec![chronon_record]);
        host.set_stamp_response(serde_json::json!({
            "foretis": foretis_for_signing,
            "signature": hex::encode(&sig),
            "signature_algorithm": "Ed25519",
        }));

        let executor = CommunerdetteExecutor::new(host, tbid, crypto.clone(), clock, None);
        let result = Communerdette::execute_stamp(
            &executor,
            serialized.clone(),
            "test".to_string(),
            TokioDuration::from_secs(5),
        )
        .await;

        assert!(
            result.is_ok(),
            "stamp_chronon with bincode must succeed: {:?}",
            result
        );
        let ca = result.unwrap();
        let expected_hash = crypto.sha256(&serialized).unwrap();
        assert_eq!(ca.content_hash().as_slice(), &expected_hash.bytes);
    }

    #[tokio::test]
    async fn stamp_chronon_different_algorithms_produce_different_content_hashes() {
        use foretias_core::foretias::tick::SerializationAlgorithm;

        let crypto: Arc<dyn foretias_core::crypto_server::CryptoServer> = Arc::from(
            foretias_core::crypto_server::new_software(
                foretias_core::crypto_server::ForetiasCurve::Ed25519,
            )
            .expect("libsodium"),
        );

        let content = b"same content, different serialization" as &[u8];
        let postcard_bytes = SerializationAlgorithm::Postcard.serialize(content).unwrap();
        let bincode_bytes = SerializationAlgorithm::Bincode.serialize(content).unwrap();

        let hash_postcard = crypto.sha256(&postcard_bytes).unwrap();
        let hash_bincode = crypto.sha256(&bincode_bytes).unwrap();

        assert_ne!(
            hash_postcard.bytes, hash_bincode.bytes,
            "different serialization algorithms must produce different content hashes"
        );
    }
}
