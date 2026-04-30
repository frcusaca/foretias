# Fortias — P2P Sub-Spec 5: GossipSub, ProbityReport & U-Shape Aggregation (v0.6)

**Milestone tag:** `v0.6-probity-gossip`
**Prereq:** `v0.5-hardening` must be tagged.
**Next:** `FORTIAS_2_P2P_7_collision_detection.md` (v0.7 — heartbeats and
identity collision detection).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.5-hardening` is tagged.
2. Read `FORTIAS_0_OVERVIEW.md` §0.4 (enforcement is social, not cryptographic)
   and §0.5 (probity is one number; attributes are opaque). These invariants
   shape every design decision in this sub-spec.
3. Read `FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md` §6.2 (mutual-attest
   flow) — the failure branches are where `report_probity()` calls are added.
4. Read this document end to end before writing any code.

---

## 1. GOAL

Add **GossipSub** to the libp2p swarm and use it to propagate **signed
`ProbityReport`s** across the network. Implement the **U-shape time-weighted
aggregator** and the **`ProbityStore`**. Wire the v0.2 mutual-attestation
failure branches to automatically emit signed reports.

After v0.6, every node maintains a continuously-updated probity score for
every peer it has observed. Scores are recalculated every 60 seconds from
accumulated signed reports. No central authority assigns scores; they emerge
from signed peer observations gossiped across the mesh.

(@human — the probity layer does not enforce anything. It measures and
gossips. What a node does with scores — whether to route requests only to
high-scoring peers, refuse low-scoring peers, or ignore scores entirely —
is application logic above the P2P layer. The P2P layer's job is:
collect observations, sign them, gossip them, aggregate them into a score
per subject peer. FOSITAS application logic will build on top.)

---

## 2. ACCEPTANCE DEMO

Three nodes A, B, C. A and B run mutual-attestation (v0.2). B deliberately
returns an invalid signature on one mutual-attest response.

```
Expected within 90 s:
- A emits a signed ProbityReport { subject: B, attribute: "correctness", value: -10.0 }
- C (which has never directly interacted with B) sees A's report via GossipSub
- C's ProbityStore records a score for B derived from A's report
- fortias get-peer-score --peer <B tbid>  on node C  →  score < 0
```

(@human — "B deliberately returns an invalid signature" means the
integration test instruments B's JsonRpcTransport to corrupt one byte
of the signature before returning. No manual intervention needed.)

---

## 3. NEW FILES

```
src/
├── probity/
│   ├── mod.rs              re-exports
│   ├── report.rs           ProbityReport type + canonical serialization
│   ├── aggregator.rs       u_shape_weight(), aggregate()
│   ├── store.rs            ProbityStore (ingest, recompute_all, score)
│   └── gossip_handler.rs   GossipSub subscriber + report verifier
└── network/
    └── gossip.rs           GossipSub topic construction + publish helper
```

`src/lib.rs` gains `pub mod probity;`. `src/network/mod.rs` gains `pub mod gossip;`.

---

## 4. `ProbityReport` — `src/probity/report.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbityReport {
    /// PeerId (hex) of the peer being reported on.
    pub subject:      String,
    /// PeerId (hex) of the reporting peer.
    pub reporter:     String,
    /// Opaque attribute name, e.g. "correctness", "liveness", "response_time".
    /// The P2P layer treats this as an opaque string; semantics are defined by
    /// the application layer.
    pub attribute:    String,
    /// Signed magnitude. Convention: positive = good, negative = bad.
    /// No enforced range at this layer; aggregator clamps to [-100, +100].
    pub value:        f32,
    /// Observation time, nanoseconds since UNIX epoch.
    pub timestamp_ns: u64,
    /// Ed25519 or P-256 signature over canonical() bytes.
    pub signature:    Vec<u8>,
    /// 1 = Ed25519, 2 = P-256.
    pub curve:        u8,
}

impl ProbityReport {
    /// Canonical byte representation for signing — fixed field order,
    /// no signature field. Any change to this function is a wire-breaking change.
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.subject.as_bytes());   buf.push(0);
        buf.extend_from_slice(self.reporter.as_bytes());  buf.push(0);
        buf.extend_from_slice(self.attribute.as_bytes()); buf.push(0);
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.push(self.curve);
        buf
    }
}
```

---

## 5. U-SHAPE AGGREGATOR — `src/probity/aggregator.rs`

The U-shape weighting curve gives full weight to very recent observations,
lets weight decay toward a floor for medium-aged observations, then allows
ancient-but-uncontradicted observations to climb back toward full weight.
This models the intuition that a recent report is highly relevant, a
week-old report may be stale, but a month-old uncontradicted record of
good behaviour is meaningful again.

```rust
#[derive(Debug, Clone, Copy)]
pub struct UShapeConfig {
    pub hot_window_ns:    u64,   // observations this fresh get max weight. Default: 1 h
    pub valley_center_ns: u64,   // weight reaches floor here. Default: 7 d
    pub ancient_start_ns: u64,   // weight starts climbing again. Default: 30 d
    pub valley_floor:     f32,   // minimum weight. Default: 0.1
    pub max_weight:       f32,   // maximum weight. Default: 1.0
}

impl Default for UShapeConfig {
    fn default() -> Self {
        Self {
            hot_window_ns:    3_600_000_000_000,         //  1 h
            valley_center_ns: 604_800_000_000_000,       //  7 d
            ancient_start_ns: 2_592_000_000_000_000,     // 30 d
            valley_floor:     0.1,
            max_weight:       1.0,
        }
    }
}

pub fn u_shape_weight(age_ns: u64, cfg: &UShapeConfig) -> f32 {
    let age   = age_ns as f32;
    let hot   = cfg.hot_window_ns    as f32;
    let valley = cfg.valley_center_ns as f32;
    let ancient = cfg.ancient_start_ns as f32;

    if age_ns <= cfg.hot_window_ns {
        cfg.max_weight
    } else if age_ns <= cfg.valley_center_ns {
        let t = (age - hot) / (valley - hot);
        cfg.max_weight - (cfg.max_weight - cfg.valley_floor) * t
    } else if age_ns <= cfg.ancient_start_ns {
        cfg.valley_floor
    } else {
        let t = 1.0 - (ancient / age).clamp(0.0, 1.0);
        cfg.valley_floor + (cfg.max_weight - cfg.valley_floor) * t
    }
}

/// Aggregate a probity score for one subject from a slice of reports.
///
/// score = Σ  u_shape_weight(now - report.timestamp_ns)
///           × reporter_credibility(report.reporter)   [clamped to ≥ 0]
///           × report.value
///
/// Result clamped to [-100.0, +100.0].
pub fn aggregate(
    reports:     &[ProbityReport],
    now_ns:      u64,
    credibility: &dyn Fn(&str) -> f32,
    cfg:         &UShapeConfig,
) -> f32 {
    let mut score = 0.0f32;
    for r in reports {
        if r.timestamp_ns > now_ns { continue; }            // reject future-dated
        let age = now_ns - r.timestamp_ns;
        let w   = u_shape_weight(age, cfg);
        let cr  = credibility(&r.reporter).max(0.0);        // negative reporters contribute nothing
        score  += w * cr * r.value;
    }
    score.clamp(-100.0, 100.0)
}
```

---

## 6. `ProbityStore` — `src/probity/store.rs`

```rust
use std::collections::HashMap;
use parking_lot::RwLock;

pub struct ProbityStore {
    reports:              RwLock<HashMap<String, Vec<ProbityReport>>>,
    scores:               RwLock<HashMap<String, f32>>,
    config:               UShapeConfig,
    max_reports_per_peer: usize,   // default 500; bounds memory
}

impl ProbityStore {
    /// Ingest a verified report. Caller must have verified the signature already.
    pub fn ingest(&self, report: ProbityReport) -> Result<(), NodeError> {
        let mut guard = self.reports.write();
        let list = guard.entry(report.subject.clone()).or_default();
        // Deduplicate by (reporter, attribute, timestamp_ns)
        let dup = list.iter().any(|r|
            r.reporter     == report.reporter &&
            r.attribute    == report.attribute &&
            r.timestamp_ns == report.timestamp_ns
        );
        if dup { return Ok(()); }
        list.push(report);
        if list.len() > self.max_reports_per_peer {
            list.sort_by_key(|r| r.timestamp_ns);
            let excess = list.len() - self.max_reports_per_peer;
            list.drain(0..excess);    // evict oldest
        }
        Ok(())
    }

    /// Recompute all scores. Call on a 60 s timer.
    ///
    /// Two-pass Eigentrust-style convergence:
    ///   Pass 1 — assume credibility = 1.0 for all reporters.
    ///   Pass 2 — use pass-1 scores normalised to [0, 1] as credibility.
    /// One iteration is sufficient for MVP; the network naturally enforces
    /// that high-probity reporters have higher report weight.
    pub fn recompute_all(&self, now_ns: u64) {
        let reports = self.reports.read();

        let cred1 = |_: &str| 1.0f32;
        let pass1: HashMap<String, f32> = reports.iter()
            .map(|(subj, reps)| (subj.clone(), aggregate(reps, now_ns, &cred1, &self.config)))
            .collect();

        let cred2 = |peer: &str| -> f32 {
            pass1.get(peer).copied().unwrap_or(0.0).max(0.0) / 100.0
        };

        let mut scores = self.scores.write();
        scores.clear();
        for (subj, reps) in reports.iter() {
            scores.insert(subj.clone(), aggregate(reps, now_ns, &cred2, &self.config));
        }
    }

    pub fn score(&self, peer_id: &str) -> f32 {
        self.scores.read().get(peer_id).copied().unwrap_or(0.0)
    }
}
```

`Arc<ProbityStore>` is held on `TimeFamilyInner`. The recompute timer is a
Tokio interval task started alongside the other component tasks in `TimeFamily::start`.

---

## 7. GOSSIPSUB — `src/network/gossip.rs`

### 7.1 Add GossipSub to `FortiasBehaviour`

```rust
// src/network/behaviour.rs  (extended from v0.4)
use libp2p::gossipsub;

#[derive(NetworkBehaviour)]
pub struct FortiasBehaviour {
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
    pub kad:      kad::Behaviour<kad::store::MemoryStore>,
    pub gossip:   gossipsub::Behaviour,              // NEW
}
```

Cargo.toml: add `"gossipsub"` to the libp2p features list.

### 7.2 Topic construction

Topic IDs are derived from the DHT namespace so nodes on different namespaces
cannot cross-contaminate:

```rust
// src/network/gossip.rs
use libp2p::gossipsub::{IdentTopic, TopicHash};

pub fn probity_topic(namespace: &str) -> IdentTopic {
    IdentTopic::new(format!("/fortias/{}/probity/v1", namespace))
}
```

### 7.3 GossipSub configuration

```rust
let gossip_cfg = gossipsub::ConfigBuilder::default()
    .heartbeat_interval(std::time::Duration::from_secs(10))
    .validation_mode(gossipsub::ValidationMode::Strict)
    .max_transmit_size(65_536)           // 64 KB; ProbityReport is << 1 KB
    .build()
    .expect("valid gossipsub config");

let gossip = gossipsub::Behaviour::new(
    gossipsub::MessageAuthenticity::Signed(keypair.clone()),
    gossip_cfg,
).expect("gossipsub init");
```

After the swarm is built, subscribe to the probity topic:

```rust
swarm.behaviour_mut().gossip.subscribe(&probity_topic(&namespace))?;
```

### 7.4 Publish helper

```rust
pub fn publish_probity_report(
    swarm:     &mut libp2p::Swarm<FortiasBehaviour>,
    report:    &ProbityReport,
    namespace: &str,
) -> Result<(), NodeError> {
    let bytes = serde_json::to_vec(report)?;
    swarm.behaviour_mut().gossip
        .publish(probity_topic(namespace), bytes)
        .map_err(|e| NodeError::Internal(format!("gossip publish: {e}")))?;
    Ok(())
}
```

---

## 8. GOSSIP HANDLER — `src/probity/gossip_handler.rs`

Called from the swarm event loop on every
`FortiasBehaviourEvent::Gossip(gossipsub::Event::Message { .. })` for the
probity topic:

```rust
pub fn handle_gossip_message(
    data:         &[u8],
    store:        &ProbityStore,
    crypto:       &dyn CryptoServer,
    now_ns:       u64,
) -> Result<(), NodeError> {
    let report: ProbityReport = serde_json::from_slice(data)
        .map_err(|e| NodeError::BadFormat("ProbityReport deserialization"))?;

    // 1. Reject future-dated reports (> 5 min clock skew tolerance)
    if report.timestamp_ns > now_ns + 5 * 60 * 1_000_000_000 {
        return Err(NodeError::Stale("future-dated probity report"));
    }

    // 2. Reject reports from peers with score < -50 (too dishonourable to report)
    //    On first ingestion the reporter has no score yet (defaults to 0.0), so
    //    this gate only activates once the network has accumulated enough history.
    if store.score(&report.reporter) < -50.0 {
        return Ok(());   // silently drop; do not propagate
    }

    // 3. Verify reporter signature over canonical bytes
    verify_report_signature(&report, crypto)?;

    // 4. Ingest
    store.ingest(report)?;
    Ok(())
}

fn verify_report_signature(report: &ProbityReport, crypto: &dyn CryptoServer)
    -> Result<(), NodeError>
{
    // For v0.6, require Ed25519 (curve == 1). P-256 path added when needed.
    if report.curve != 1 {
        return Err(NodeError::Unsupported("only Ed25519 probity signatures in v0.6"));
    }
    // The reporter's public key is not transmitted in the report — it must be
    // looked up from our known-peers table or calendar.
    // For v0.6: if we don't have the reporter's public key, accept the report
    // but mark it as unverified. Full key resolution is a v0.7 enhancement.
    // TODO v0.7: resolve reporter public key and verify.
    //
    // (@human — this is a deliberate deferral. Full verification requires
    // knowing the reporter's current public key, which means either caching
    // their calendar slice or fetching it on demand. For the v0.6 demo the
    // network is small and we already have keys in memory from mutual-attest.
    // In v0.7, key resolution is driven by the identify cache populated from
    // the libp2p handshake and cross-attest flow.)
    Ok(())
}
```

(@human — the signature verification deferral is noted explicitly as a
TODO. It is intentional for v0.6: the demo works because the test peers
have already exchanged public keys via direct mutual-attest. Production
use requires full resolution, addressed in v0.7.)

---

## 9. WIRING MUTUAL-ATTEST OUTCOMES TO PROBITY REPORTS

In `src/peer_conn/mod.rs`, `CrossAttestScheduler::on_response` (v0.2 §7),
add `report_probity()` calls at the two failure sites and one success site:

```rust
// Success: peer responded with a valid attestation
self.time_family.report_probity(&from_peer_id_str, "correctness", 1.0);
self.time_family.report_probity(&from_peer_id_str, "liveness",    1.0);

// Failure: verification failed (bad signature, wrong hash, echo mismatch)
self.time_family.report_probity(&from_peer_id_str, "correctness", -10.0);

// Failure: timeout or transport error
self.time_family.report_probity(&from_peer_id_str, "liveness", -1.0);

// Refused { Dormant }: legitimate; no probity impact
// Refused { RateLimited }: mild negative (peer is overloaded but not dishonest)
self.time_family.report_probity(&from_peer_id_str, "availability", -0.5);
```

`TimeFamily::report_probity` packages, signs, and gossips the report:

```rust
impl TimeFamily {
    pub fn report_probity(&self, subject: &str, attribute: &str, value: f32) {
        let now_ns = now_ns();
        let reporter = self.inner.peer_id_str.clone();
        let curve = 1u8; // Ed25519
        let mut report = ProbityReport {
            subject: subject.to_string(),
            reporter,
            attribute: attribute.to_string(),
            value,
            timestamp_ns: now_ns,
            signature: vec![],
            curve,
        };
        if let Ok(sig) = self.inner.crypto.sign(&report.canonical()) {
            report.signature = sig.bytes.to_vec();
            // Ingest locally too
            let _ = self.inner.probity_store.ingest(report.clone());
            // Gossip to network
            if let Some(handle) = self.inner.swarm_handle.lock().as_ref() {
                // Send to swarm task via mpsc command channel
                let _ = handle.cmd_tx.send(SwarmCommand::PublishProbity(report));
            }
        }
    }
}
```

---

## 10. NEW CLI COMMAND

```
fortias get-peer-score --peer <tbid-or-peer-id>
```

Calls the new `get_peer_score` admin JSON-RPC method:

```
Method: get_peer_score
Params: { "peer_id": "<hex>" }
Result: { "peer_id": "<hex>", "score": -3.5, "report_count": 12 }
```

---

## 11. CONFIGURATION ADDITIONS

```json
{
  "probity": {
    "recompute_interval_secs": 60,
    "max_reports_per_peer":    500,
    "u_shape": {
      "hot_window_hours":    1,
      "valley_center_days":  7,
      "ancient_start_days":  30,
      "valley_floor":        0.1
    }
  },
  "network": {
    "gossipsub_heartbeat_secs": 10,
    "gossipsub_max_message_bytes": 65536
  }
}
```

---

## 12. TEST PLAN

- `u_shape_weight_boundaries` — values at hot_window, valley_center, ancient_start, and beyond; compare to expected floats within 1e-4.
- `aggregate_empty_reports` — returns 0.0.
- `aggregate_future_dated_rejected`.
- `probity_store_ingest_deduplicates`.
- `probity_store_evicts_oldest_over_cap`.
- `probity_store_recompute_two_pass` — reporter with score 0 has zero weight in pass 2.
- `gossip_handler_rejects_future_dated`.
- `gossip_handler_rejects_dishonourable_reporter` — reporter score −60 → report silently dropped.
- Integration: `three_nodes_probity_gossip` — A reports on B; C sees the report and scores B < 0. Uses instrumented B (bad-sig on one response).
- All v0.1–v0.5 tests pass.

---

## 13. NON-GOALS FOR v0.6

- ❌ Full probity-report signature verification — deferred to v0.7 (requires key resolution).
- ❌ Application-layer enforcement on scores (refusing low-scoring peers) — that is FOSITAS logic above this layer.
- ❌ Epoch consensus on scores — v0.8.
- ❌ Probity reports from collision events — v0.7.

---

## 14. MILESTONE CHECKLIST

```
[ ] v0.6.1  ProbityReport type + canonical() + serde roundtrip test.
[ ] v0.6.2  u_shape_weight() + aggregate() + unit tests.
[ ] v0.6.3  ProbityStore: ingest, eviction, recompute_all (two-pass), score().
[ ] v0.6.4  Add gossipsub to FortiasBehaviour; topic construction; subscribe on start.
[ ] v0.6.5  gossip_handler: deserialise, future-date check, score gate, ingest.
[ ] v0.6.6  TimeFamily::report_probity(): sign, ingest locally, gossip.
[ ] v0.6.7  Wire mutual-attest outcome sites to report_probity() calls.
[ ] v0.6.8  Recompute timer task (60 s interval).
[ ] v0.6.9  get_peer_score admin RPC + CLI command.
[ ] v0.6.10 three_nodes_probity_gossip integration test passes.
[ ] v0.6.11 All prior tests pass.
[ ] v0.6.12 TAG: v0.6-probity-gossip.
```

---
# END OF SUB-SPEC 5 (v0.6)
