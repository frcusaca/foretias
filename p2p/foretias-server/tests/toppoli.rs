//! Toppoli — Test Of P2P and PTP On Local Integration.
//!
//! Multi-peer in-process harness for foretias integration tests. Up to ~24
//! `TimeFamilyServer` instances run in the same tokio runtime. All state is
//! directly inspectable via `peer(idx).server()` — no mocks or external probes.
//!
//! Intended test classes:
//!   - Liveness (Phase 12): L1/L2/L3 round-trips over real TCP.
//!   - FB gossip (Phase 13): FullyBound transition → gossip arrives at peers.
//!   - GNF (Gossip and Node Failure): propagation under churn.
//!   - General P2P/PTP: any test needing realistic multi-hop message flow.

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use foretias_server::server::TimeFamilyServer;
use foretias_core::config::{CommunerdConfig, MutualAttestConfig};
use foretias_server::communerd::transport::PeerTransport;

// ── Port allocation ───────────────────────────────────────────────────────────

/// Bind an ephemeral port and immediately release it for the server to claim.
/// There is a tiny TOCTOU window; acceptable for controlled in-process tests.
pub fn find_available_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// Pre-allocate N ports and release all at once to minimize time between
/// individual releases. The server binds them shortly after.
fn allocate_ports(n: usize) -> Vec<u16> {
    let ls: Vec<_> = (0..n)
        .map(|_| std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral"))
        .collect();
    let ports: Vec<u16> = ls.iter().map(|l| l.local_addr().unwrap().port()).collect();
    drop(ls);
    ports
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for one peer slot. Set before starting the peer.
/// Topology helpers populate `peers`; tests may also set fields manually.
#[derive(Debug, Clone)]
pub struct ToppliPeerConfig {
    /// Chronon interval in nanoseconds.
    pub chronon_ns: u64,
    /// Peer addresses for `CommunerdConfig::mutual_attest.peers`.
    pub peers: Vec<String>,
    /// Gossip/DHT namespace string.
    pub namespace: String,
    /// Whether to attach a `Communerd` (P2P layer) to this server.
    pub with_communerd: bool,
    /// Request timeout in seconds passed to `MutualAttestConfig`.
    pub request_timeout_secs: u64,
}

impl Default for ToppliPeerConfig {
    fn default() -> Self {
        Self {
            chronon_ns:           100_000_000, // 100 ms
            peers:                vec![],
            namespace:            "toppoli".to_string(),
            with_communerd:       false,
            request_timeout_secs: 5,
        }
    }
}

// ── Running peer ──────────────────────────────────────────────────────────────

/// A peer that has been started.
pub struct ToppliPeer {
    pub idx:    usize,
    /// Bound TCP address, e.g. `"127.0.0.1:51234"`.
    pub addr:   String,
    /// Direct access to the server for introspection and method calls.
    pub server: Arc<TimeFamilyServer>,
    handle:     JoinHandle<()>,
}

impl ToppliPeer {
    pub fn server(&self) -> &Arc<TimeFamilyServer> { &self.server }
}

// ── Harness ───────────────────────────────────────────────────────────────────

/// Multi-peer local integration harness.
///
/// Configs and addresses are indexed by slot. A slot may be in any lifecycle
/// state. Config survives stop/restart — it is mutable only when the peer is
/// not running.
pub struct ToppliHarness {
    /// Per-slot configuration. Never removed; updated while stopped.
    configs: Vec<ToppliPeerConfig>,
    /// Pre-allocated addresses (`"127.0.0.1:<port>"`), indexed by slot.
    addrs:   Vec<String>,
    /// Running peer handles. `None` means the slot exists but is not running.
    peers:   Vec<Option<ToppliPeer>>,
}

impl ToppliHarness {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a harness with `count` slots, all using `config`.
    /// Ports are pre-allocated so topology helpers can wire addresses before
    /// any server is started.
    pub fn with_peers(count: usize, config: ToppliPeerConfig) -> Self {
        let ports = allocate_ports(count);
        let addrs: Vec<String> = ports.iter().map(|p| format!("127.0.0.1:{p}")).collect();
        Self {
            configs: vec![config; count],
            peers:   (0..count).map(|_| None).collect(),
            addrs,
        }
    }

    /// Add one more slot with a custom config. Returns its index.
    /// May be called before or after `start_all`; the new slot starts stopped.
    pub fn add_peer(&mut self, config: ToppliPeerConfig) -> usize {
        let port = find_available_port();
        self.addrs.push(format!("127.0.0.1:{port}"));
        self.configs.push(config);
        self.peers.push(None);
        self.configs.len() - 1
    }

    // ── Topology helpers ──────────────────────────────────────────────────────

    /// Wire every slot to know every other slot. Call before starting any peer.
    /// Panics if any peer is already running (topology is set at start time).
    pub fn topology_full_mesh(&mut self) {
        assert!(
            self.peers.iter().all(|p| p.is_none()),
            "topology_full_mesh must be called before starting any peer"
        );
        let all = self.addrs.clone();
        for (idx, cfg) in self.configs.iter_mut().enumerate() {
            cfg.peers = all.iter().enumerate()
                .filter(|(i, _)| *i != idx)
                .map(|(_, a)| a.clone())
                .collect();
        }
    }

    /// Wire slots as a unidirectional ring: slot N → slot (N+1) % len.
    pub fn topology_ring(&mut self) {
        assert!(
            self.peers.iter().all(|p| p.is_none()),
            "topology_ring must be called before starting any peer"
        );
        let n = self.addrs.len();
        let all = self.addrs.clone();
        for (idx, cfg) in self.configs.iter_mut().enumerate() {
            cfg.peers = vec![all[(idx + 1) % n].clone()];
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Build and start a single peer. Panics if already running.
    pub async fn start_peer(&mut self, idx: usize) {
        assert!(self.peers[idx].is_none(), "peer {idx} is already running");
        let addr = self.addrs[idx].clone();
        let cfg  = &self.configs[idx];

        let server: Arc<TimeFamilyServer> = if cfg.with_communerd {
            Arc::new(
                TimeFamilyServer::new(&addr, cfg.chronon_ns)
                    .expect("create server")
                    .with_communerd(CommunerdConfig {
                        mutual_attest: MutualAttestConfig {
                            peers:                cfg.peers.clone(),
                            every_n_chronons:     1,
                            request_timeout_secs: cfg.request_timeout_secs,
                        },
                        ..Default::default()
                    }),
            )
        } else {
            Arc::new(
                TimeFamilyServer::new(&addr, cfg.chronon_ns)
                    .expect("create server"),
            )
        };

        server.start_daemon_arc();
        let handle = Arc::clone(&server).start().expect("start TCP");
        self.peers[idx] = Some(ToppliPeer { idx, addr, server, handle });
    }

    /// Stop a single peer.
    ///
    /// - Calls `stop_daemon_arc()` to stop the chronomatter daemon.
    /// - Sleeps `drain_ms` to let in-flight requests finish.
    /// - Aborts the TCP listener task.
    ///
    /// The slot's config is preserved for `restart_peer`.
    pub async fn stop_peer(&mut self, idx: usize, drain_ms: u64) {
        if let Some(peer) = self.peers[idx].take() {
            peer.server.stop_daemon_arc();
            if drain_ms > 0 {
                tokio::time::sleep(Duration::from_millis(drain_ms)).await;
            }
            peer.handle.abort();
        }
    }

    /// Stop then restart a peer at the same address with the same config.
    pub async fn restart_peer(&mut self, idx: usize) {
        self.stop_peer(idx, 20).await;
        // Brief pause so the OS reclaims the port before re-binding.
        tokio::time::sleep(Duration::from_millis(30)).await;
        self.start_peer(idx).await;
    }

    /// Start all stopped/reserved slots sequentially.
    pub async fn start_all(&mut self) {
        let stopped: Vec<usize> = (0..self.peers.len())
            .filter(|&i| self.peers[i].is_none())
            .collect();
        for idx in stopped {
            self.start_peer(idx).await;
        }
    }

    /// Stop all running peers sequentially.
    pub async fn stop_all(&mut self, drain_ms: u64) {
        let running: Vec<usize> = (0..self.peers.len())
            .filter(|&i| self.peers[i].is_some())
            .collect();
        for idx in running {
            self.stop_peer(idx, drain_ms).await;
        }
    }

    // ── Inspection ────────────────────────────────────────────────────────────

    /// Number of currently running peers.
    pub fn running_count(&self) -> usize {
        self.peers.iter().filter(|p| p.is_some()).count()
    }

    /// Total number of slots (running + stopped).
    pub fn slot_count(&self) -> usize { self.configs.len() }

    /// Iterate over all running peers.
    pub fn running_peers(&self) -> impl Iterator<Item = &ToppliPeer> {
        self.peers.iter().filter_map(|p| p.as_ref())
    }

    /// Get a running peer by index.
    /// Panics with a clear message if the slot is not running — this is a
    /// test programming error, not a production error.
    pub fn peer(&self, idx: usize) -> &ToppliPeer {
        self.peers[idx]
            .as_ref()
            .unwrap_or_else(|| panic!("peer {idx} is not running"))
    }

    /// Mutable access to a slot's config. Panics if the peer is running
    /// (config changes only take effect on (re)start).
    pub fn config_mut(&mut self, idx: usize) -> &mut ToppliPeerConfig {
        assert!(
            self.peers[idx].is_none(),
            "stop peer {idx} before mutating its config"
        );
        &mut self.configs[idx]
    }

    /// The pre-allocated address for a slot, regardless of running state.
    pub fn addr(&self, idx: usize) -> &str {
        &self.addrs[idx]
    }

    // ── Wait helper ───────────────────────────────────────────────────────────

    /// Poll `predicate` every 20 ms until it returns `true` or `timeout_ms`
    /// elapses. Returns `true` if the predicate was satisfied.
    ///
    /// Use this instead of `sleep` for feature-readiness checks:
    /// ```rust
    /// let ok = h.wait_until(|h| h.running_count() == 3, 2_000).await;
    /// assert!(ok, "three peers did not come up within 2 s");
    /// ```
    pub async fn wait_until(
        &self,
        predicate: impl Fn(&ToppliHarness) -> bool,
        timeout_ms: u64,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if predicate(self) { return true; }
            if tokio::time::Instant::now() >= deadline { return false; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// Drop guard: stop all peers if the harness is dropped without explicit teardown.
impl Drop for ToppliHarness {
    fn drop(&mut self) {
        for peer in self.peers.iter_mut().filter_map(|p| p.take()) {
            peer.server.stop_daemon_arc();
            peer.handle.abort();
        }
    }
}

// ── Test fixture types ────────────────────────────────────────────────────────
//
// Each fixture wraps ToppliHarness and provides domain-specific setup helpers.
// Tests call `MyFixture::setup(...).await`, use the fixture, then call
// `fixture.teardown().await`.
//
// Fixture naming convention: Toppoli<Domain>Test
// (matching Rust struct naming; the word "Test" makes the fixture's role
// clear when reading test code).

/// Fixture for basic peer lifecycle tests.
///
/// Default topology: no mesh (peers do not know each other's addresses).
/// Set `with_communerd = true` in the config and call `topology_full_mesh()`
/// on the harness before starting when you need P2P connectivity.
pub struct ToppliBasicTest {
    pub harness: ToppliHarness,
}

impl ToppliBasicTest {
    pub async fn setup(peers: usize) -> Self {
        let mut harness = ToppliHarness::with_peers(peers, ToppliPeerConfig::default());
        harness.start_all().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { harness }
    }

    pub async fn teardown(mut self) {
        self.harness.stop_all(20).await;
    }
}

/// Fixture for Fully-Bound gossip and ProbityReport tests (Phase 13).
///
/// All peers are started with `with_communerd = true` and a full-mesh topology.
/// Once Phase 13 is implemented this fixture will expose helpers to wait for
/// FullyBound state and inspect ProbityStore for FB reports.
pub struct ToppoliFBProbityTest {
    pub harness: ToppliHarness,
}

impl ToppoliFBProbityTest {
    pub async fn setup(peers: usize) -> Self {
        let cfg = ToppliPeerConfig { with_communerd: true, ..Default::default() };
        let mut harness = ToppliHarness::with_peers(peers, cfg);
        harness.topology_full_mesh();
        harness.start_all().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self { harness }
    }

    pub async fn teardown(mut self) {
        self.harness.stop_all(50).await;
    }
}

/// Fixture for L1/L2/L3 liveness tests (Phase 12).
///
/// All peers started with communerd enabled and full-mesh wiring.
pub struct ToppliLivenessTest {
    pub harness: ToppliHarness,
}

impl ToppliLivenessTest {
    pub async fn setup(peers: usize) -> Self {
        let cfg = ToppliPeerConfig { with_communerd: true, ..Default::default() };
        let mut harness = ToppliHarness::with_peers(peers, cfg);
        harness.topology_full_mesh();
        harness.start_all().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self { harness }
    }

    pub async fn teardown(mut self) {
        self.harness.stop_all(50).await;
    }
}

/// Fixture for Gossip and Node Failure (GNF) tests.
///
/// Starts with a ring topology so tests can explore partial connectivity
/// and churn. Peer count should be at least 6 for meaningful gossip tests;
/// 12–24 for DHT formation tests.
pub struct ToppliGNFTest {
    pub harness: ToppliHarness,
}

impl ToppliGNFTest {
    pub async fn setup_ring(peers: usize) -> Self {
        let cfg = ToppliPeerConfig { with_communerd: true, ..Default::default() };
        let mut harness = ToppliHarness::with_peers(peers, cfg);
        harness.topology_ring();
        harness.start_all().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self { harness }
    }

    pub async fn teardown(mut self) {
        self.harness.stop_all(50).await;
    }
}

// ── Toppoli tests ──────────────────────────────────────────────────────────────
//
// CATEGORY: toppoli — multi-peer in-process integration tests.
// These tests are intentionally SLOW (seconds, not milliseconds) and are
// separated from unit tests and snapshot tests.
//
// Run commands:
//   All toppoli tests:    cargo test -p foretias-server --test toppoli -- --include-ignored
//   One toppoli test:     cargo test -p foretias-server --test toppoli toppoli_peer_restart -- --include-ignored
//   All unit tests only:  cargo test -p foretias-server --lib
//   All snapshot tests:   cargo test -p foretias-server snapshot
//   All non-toppoli:      cargo test -p foretias-server
//   Everything:           cargo test -p foretias-server -- --include-ignored
//
// All toppoli tests carry #[ignore = "toppoli: ..."] so they are listed but
// not run by default. `cargo test` shows them as "(ignored)".

/// Four peers start and stop cleanly.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_peers_start_and_stop() {
    let mut h = ToppliHarness::with_peers(4, ToppliPeerConfig::default());
    h.start_all().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(h.running_count(), 4);
    h.stop_all(20).await;
    assert_eq!(h.running_count(), 0);
}

/// Individual peer stop and restart.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_peer_restart() {
    let mut h = ToppliHarness::with_peers(3, ToppliPeerConfig::default());
    h.start_all().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.running_count(), 3);

    h.stop_peer(1, 20).await;
    assert_eq!(h.running_count(), 2, "after stopping peer 1");

    h.restart_peer(1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.running_count(), 3, "after restarting peer 1");

    h.stop_all(20).await;
    assert_eq!(h.running_count(), 0);
}

/// Mixed-config harness: two default peers plus one custom peer.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_mixed_config() {
    let mut h = ToppliHarness::with_peers(2, ToppliPeerConfig::default());
    let slow_idx = h.add_peer(ToppliPeerConfig {
        chronon_ns: 500_000_000,
        ..Default::default()
    });
    h.start_all().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.running_count(), 3);
    let tbid_0 = h.peer(0).server().get_tbid();
    let tbid_s = h.peer(slow_idx).server().get_tbid();
    assert_ne!(tbid_0, tbid_s, "each peer must have a distinct TBID");
    h.stop_all(20).await;
}

/// 12-peer full-mesh smoke test.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_twelve_peers_full_mesh() {
    let mut h = ToppliHarness::with_peers(12, ToppliPeerConfig::default());
    h.topology_full_mesh();
    h.start_all().await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(h.running_count(), 12);
    for i in 0..12 {
        assert_eq!(h.peer(i).idx, i);
        assert_ne!(h.peer(i).addr, "");
    }
    h.stop_all(20).await;
    assert_eq!(h.running_count(), 0);
}

/// wait_until resolves as soon as condition is met.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_wait_until_resolves() {
    let mut h = ToppliHarness::with_peers(2, ToppliPeerConfig::default());
    h.start_all().await;
    let ok = h.wait_until(|h| h.running_count() == 2, 1_000).await;
    assert!(ok);
    h.stop_all(20).await;
}

/// wait_until times out when condition is never met.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_wait_until_times_out() {
    let h = ToppliHarness::with_peers(2, ToppliPeerConfig::default());
    let ok = h.wait_until(|h| h.running_count() == 2, 100).await;
    assert!(!ok, "should time out");
}

/// Fixture convenience: ToppliBasicTest wraps setup/teardown.
#[tokio::test]
#[ignore = "toppoli: multi-peer integration test; run with --include-ignored"]
async fn toppoli_basic_fixture_setup_teardown() {
    let f = ToppliBasicTest::setup(3).await;
    assert_eq!(f.harness.running_count(), 3);
    f.teardown().await;
}

/// FB gossip: 2 peers, communerd wired, ProbityStore accessible.
///
/// Verifies: both peers start with communerd, ProbityStore is reachable,
/// TBID hexes are obtainable, and report_count is inspectable.
///
/// Known limitation: `trigger_channel_bind` passes `local_calendar_tbid: None`
/// (communerdette.rs:1383), so `emit_fb_report` early-returns. Full end-to-end
/// FB gossip propagation requires `local_calendar_tbid` to be plumbed through
/// the gossip-loop context. This test documents the expected integration
/// surface until that production code gap is addressed.
#[tokio::test]
#[ignore = "toppoli: FB gossip integration test; run with --include-ignored"]
async fn toppoli_fb_gossip_propagation() {
    let f = ToppoliFBProbityTest::setup(2).await;

    let ok = f.harness.wait_until(|h| h.running_count() == 2, 5_000).await;
    assert!(ok, "peers should be running");

    let peer0_tbid_hex = f.harness.peer(0).server.chronomatter().get_tbid().to_hex();
    let peer1_tbid_hex = f.harness.peer(1).server.chronomatter().get_tbid().to_hex();

    assert_ne!(peer0_tbid_hex, peer1_tbid_hex, "peers must have distinct TBIDs");
    assert_eq!(peer0_tbid_hex.len(), 192, "TBID hex must be 192 chars (96 bytes)");
    assert_eq!(peer1_tbid_hex.len(), 192, "TBID hex must be 192 chars (96 bytes)");

    assert!(f.harness.peer(0).server.communerd().is_some(), "peer 0 must have communerd");
    assert!(f.harness.peer(1).server.communerd().is_some(), "peer 1 must have communerd");

    let store_0 = f.harness.peer(0).server.communerd().unwrap().probity_store();
    let store_1 = f.harness.peer(1).server.communerd().unwrap().probity_store();

    assert_eq!(store_0.score(&peer0_tbid_hex), 0.0, "peer 0 score defaults to 0.0");
    assert_eq!(store_1.score(&peer1_tbid_hex), 0.0, "peer 1 score defaults to 0.0");

    // Give time for channel binding attempts (even though FB emission is skipped
    // due to local_calendar_tbid=None in trigger_channel_bind).
    tokio::time::sleep(Duration::from_secs(3)).await;

    let count_0 = store_0.report_count(&peer0_tbid_hex);
    let count_1 = store_1.report_count(&peer1_tbid_hex);

    tracing::info!(
        peer0_tbid = %peer0_tbid_hex,
        peer1_tbid = %peer1_tbid_hex,
        store_0_count = count_0,
        store_1_count = count_1,
        "FB gossip propagation test: report counts after 3 s"
    );

    f.teardown().await;
}

/// L1 liveness: peer 0 sends a JSON-RPC `ping` to peer 1 over Noise_XX TCP
/// and asserts `{"pong": true}` within 2 seconds.
#[tokio::test]
#[ignore = "toppoli: L1 ping round trip; run with --include-ignored"]
async fn toppoli_l1_ping_round_trip() {
    let f = ToppliBasicTest::setup(2).await;

    let peer1_addr = f.harness.peer(1).addr.clone();
    let transport = foretias_server::communerd::json_rpc_transport::JsonRpcTransport::new(2);
    let peer = foretias_server::communerd::transport::PeerAddr {
        json_rpc:  peer1_addr,
        peer_id:   None,
        last_seen_ns: 0,
    };

    let result = tokio::time::timeout(
        Duration::from_secs(2),
        transport.ping(&peer),
    )
    .await;

    assert!(result.is_ok(), "ping timed out after 2 s");
    assert!(result.unwrap().is_ok(), "ping returned transport error");

    f.teardown().await;
}
