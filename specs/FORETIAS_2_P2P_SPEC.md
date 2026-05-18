# Foretias — P2P Specification (v0.2–v0.8 Network Layers)

**Project:** Foretias (Free and Open-source Resilient Time Integrity Attestation Service)
**This document:** Implementation guidance for the P2P network layers — libp2p swarm, probity gossip and aggregation, identity collision detection, epoch consensus framework, and the encrypted calendar persistence upgrade.
**Companion documents:**
- `FORETIAS_OVERVIEW.md` — design invariants, project structure, milestone roadmap. **Read this first.**
- `FORETIAS_MVP_SPEC.md` — v0.1 local-server stack (C11 core, Rust crate, software CryptoServer, Python bindings). The work in this document assumes that stack is in place.
- `FORETIAS_ENCLAVE_SPEC.md` — v0.9+ custom-plugin backends. Restricted; only consulted once v0.8 lands.

**Target:** AI Coding Specialist for execution. Comments to human reader in parenthesis `(@human ...)`.

---

## READING ORDER

**Do not begin work in this document until v0.1 (`v0.1-local-server-mvp`) has tagged.**

Before touching any file in this document:

1. Read `foretias-v1.md`.
2. Read `FORETIAS_OVERVIEW.md` end to end.
3. Read `FORETIAS_MVP_SPEC.md` end to end.
4. Read this document end to end.
5. Confirm to the user which milestone you intend to attempt first (must be v0.2 or later, and the previous milestone must be tagged).

---

## PART 8 — NETWORKING (libp2p)

(@human — this is unchanged from earlier drafts of the spec in substance.
Key change: the swarm keypair comes from the CryptoServer, not from
libp2p's own key generation.)

### 8.1 Swarm Construction — `src/network/swarm.rs`

```rust
use libp2p::{kad, gossipsub, identify, ping, relay, autonat, noise, yamux, tcp, quic, SwarmBuilder};
use libp2p::swarm::NetworkBehaviour;
use std::sync::Arc;
use crate::crypto_server::CryptoServer;

#[derive(NetworkBehaviour)]
pub struct ForetiasBehaviour {
    pub kad:      kad::Behaviour<kad::store::MemoryStore>,
    pub gossip:   gossipsub::Behaviour,
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
    pub relay:    relay::client::Behaviour,
    pub autonat:  autonat::Behaviour,
}

pub async fn build_swarm(
    server: Arc<dyn CryptoServer>,
    config: &crate::config::NodeConfig,
) -> Result<libp2p::Swarm<ForetiasBehaviour>, crate::error::NodeError> {
    // Derive a libp2p::identity::Keypair from our CryptoServer.
    //
    // Important: libp2p needs to be able to sign handshake messages. For the
    // software backend we can extract the Ed25519 key bytes directly since
    // the software server holds them. For a custom plugin backend, signing
    // routes through the trait's sign() method instead — libp2p performs
    // few signatures per session (handshake + identify), so the extra round
    // trip is acceptable. Per-message traffic uses the Noise session keys
    // derived during handshake, so the bulk path is unaffected.
    //
    // (See FORETIAS_ENCLAVE_SPEC.md §E.4 for the bridge details when a
    // custom plugin is in use.)
    let keypair = libp2p_keypair_from_crypto_server(server.clone())?;

    // ... rest of swarm construction is straightforward libp2p ...
}
```

### 8.2 Everything else (DHT, GossipSub, presence, discovery)

Standard libp2p configuration. Ported into the `foretias_p2p` crate under
`src/network/`. Notable settings:

- DHT: Kademlia in private namespace mode, namespace key from
  `[dht].namespace_secret` (see `FORETIAS_OVERVIEW.md` Part 15).
- GossipSub: private topic IDs derived from the namespace, mesh sized per
  config.
- Identify and Ping behaviours are stock.
- Relay and AutoNAT enabled by default for NAT traversal.

---

## PART 9 — PROBITY LAYER

This is the centerpiece of the honor system plumbing.

### 9.1 `ProbityReport` — `src/probity/report.rs`

```rust
use serde::{Deserialize, Serialize};
use libp2p::PeerId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbityReport {
    pub subject:       String,  // PeerId of reported peer
    pub reporter:      String,  // PeerId of reporting peer
    pub attribute:     String,  // opaque name — e.g. "correctness", "latency"
    pub value:         f32,     // signed magnitude
    pub timestamp_ns:  u64,     // observation time, ns since Unix epoch
    pub signature:     Vec<u8>, // reporter's Ed25519/P-256 sig over canonical serialization
    pub curve:         u8,      // 1 = Ed25519, 2 = P-256
}

impl ProbityReport {
    /// Canonical bytes to sign — fixed field order, no signature field.
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.subject.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.reporter.as_bytes());
        buf.push(0);
        buf.extend_from_slice(self.attribute.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.push(self.curve);
        buf
    }
}
```

### 9.2 U-Shape Aggregator — `src/probity/aggregator.rs`

```rust
#[derive(Debug, Clone, Copy)]
pub struct UShapeConfig {
    /// Hot window — recent observations at full weight. Default 1 hour.
    pub hot_window_ns: u64,
    /// Valley bottom — minimum weight. Default 7 days from observation.
    pub valley_center_ns: u64,
    /// Ancient wisdom rise — weight starts climbing again. Default 30 days.
    pub ancient_start_ns: u64,
    /// Minimum floor weight in the valley. Default 0.1.
    pub valley_floor: f32,
    /// Maximum weight. Default 1.0.
    pub max_weight: f32,
}

impl Default for UShapeConfig {
    fn default() -> Self {
        Self {
            hot_window_ns:    3_600_000_000_000,           //   1 h
            valley_center_ns: 604_800_000_000_000,         //   7 d
            ancient_start_ns: 2_592_000_000_000_000,       //  30 d
            valley_floor:     0.1,
            max_weight:       1.0,
        }
    }
}

/// Compute a U-shape weight for an observation of age `age_ns`.
pub fn u_shape_weight(age_ns: u64, cfg: &UShapeConfig) -> f32 {
    let age = age_ns as f32;
    let hot = cfg.hot_window_ns as f32;
    let valley = cfg.valley_center_ns as f32;
    let ancient = cfg.ancient_start_ns as f32;
    let floor = cfg.valley_floor;
    let max = cfg.max_weight;

    if age_ns <= cfg.hot_window_ns {
        max
    } else if age_ns <= cfg.valley_center_ns {
        // descend from max → floor
        let t = (age - hot) / (valley - hot);
        max - (max - floor) * t
    } else if age_ns <= cfg.ancient_start_ns {
        // stay at floor
        floor
    } else {
        // ascend from floor → max with asymptote (approach max but never reach)
        let t = 1.0 - (ancient / age).max(0.0).min(1.0);
        floor + (max - floor) * t
    }
}

/// Aggregate probity score from a slice of reports.
///
/// score = Σ  u_shape_weight(now - report.timestamp) *
///           reporter_credibility(report.reporter) *
///           report.value
///
/// Clamped to [-100.0, +100.0].
pub fn aggregate(
    reports: &[ProbityReport],
    now_ns:  u64,
    credibility: &dyn Fn(&str) -> f32,
    cfg: &UShapeConfig,
) -> f32 {
    let mut score = 0.0f32;
    for r in reports {
        if r.timestamp_ns > now_ns { continue; }          // future-dated; reject
        let age = now_ns - r.timestamp_ns;
        let w   = u_shape_weight(age, cfg);
        let cr  = credibility(&r.reporter).max(0.0);      // negative reporters don't contribute
        score += w * cr * r.value;
    }
    score.clamp(-100.0, 100.0)
}
```

### 9.3 Probity Store — `src/probity/store.rs`

```rust
use std::collections::HashMap;
use parking_lot::RwLock;

pub struct ProbityStore {
    // per-subject list of reports
    reports: RwLock<HashMap<String, Vec<ProbityReport>>>,
    // computed scores, refreshed on chronon
    scores:  RwLock<HashMap<String, f32>>,
    config:  UShapeConfig,
    /// Maximum reports kept per subject to bound memory.
    max_reports_per_subject: usize,
}

impl ProbityStore {
    pub fn ingest(&self, report: ProbityReport) -> Result<(), NodeError> {
        // Signature verification is the caller's responsibility (done in gossip_handler).
        let mut guard = self.reports.write();
        let list = guard.entry(report.subject.clone()).or_default();
        // Deduplicate by (reporter, attribute, timestamp)
        let dup = list.iter().any(|r|
            r.reporter  == report.reporter &&
            r.attribute == report.attribute &&
            r.timestamp_ns == report.timestamp_ns
        );
        if !dup {
            list.push(report);
            if list.len() > self.max_reports_per_subject {
                // Drop oldest
                list.sort_by_key(|r| r.timestamp_ns);
                let overflow = list.len() - self.max_reports_per_subject;
                list.drain(0..overflow);
            }
        }
        Ok(())
    }

    pub fn recompute_all(&self, now_ns: u64) {
        let reports = self.reports.read();
        let mut scores = self.scores.write();
        scores.clear();

        // Two-pass: first pass assumes credibility = 1.0 for everyone, second
        // pass uses first-pass scores as credibility. This is one iteration
        // of Eigentrust-style convergence — enough for MVP.
        let cred_pass1 = |_peer: &str| -> f32 { 1.0 };
        let mut pass1 = HashMap::new();
        for (subject, reports) in reports.iter() {
            let s = aggregate(reports, now_ns, &cred_pass1, &self.config);
            pass1.insert(subject.clone(), s);
        }

        let cred_pass2 = |peer: &str| -> f32 {
            pass1.get(peer).copied().unwrap_or(0.0).max(0.0) / 100.0
        };
        for (subject, reports) in reports.iter() {
            let s = aggregate(reports, now_ns, &cred_pass2, &self.config);
            scores.insert(subject.clone(), s);
        }
    }

    pub fn score(&self, peer_id: &str) -> f32 {
        self.scores.read().get(peer_id).copied().unwrap_or(0.0)
    }
}
```

### 9.4 Gossip Handler — `src/probity/gossip_handler.rs`

- Subscribe to gossipsub topic `/foretias/probity/v1`
- Every published message is a serialized `ProbityReport`
- On receive:
  - Verify reporter signature
  - Reject if `timestamp_ns` is too far in the future (> 5 minutes)
  - Reject if reporter score is below `-50` (too dishonourable to report)
  - Otherwise call `ProbityStore::ingest`
- On `recompute_all` timer (every 60 seconds default):
  - Recompute all scores
  - Emit `NodeEvent::ProbityUpdated { peer_id, score }` for any change

### 9.5 Application API for Emitting Reports

```rust
// Exposed via the top-level Node API
impl Node {
    /// Called from application layer when the local entity observes something
    /// probity-relevant about a peer. The P2P layer packages, signs, and gossips.
    pub fn report_probity(
        &self,
        subject_peer:  &str,
        attribute:     &str,
        value:         f32,
    ) -> Result<(), NodeError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
        let mut report = ProbityReport {
            subject:      subject_peer.to_string(),
            reporter:     self.peer_id().to_string(),
            attribute:    attribute.to_string(),
            value,
            timestamp_ns: now,
            signature:    Vec::new(),
            curve:        match self.server.curve() {
                ForetiasCurve::Ed25519 => 1,
                ForetiasCurve::P256    => 2,
            },
        };
        let sig = self.server.sign(&report.canonical())?;
        report.signature = sig.bytes.to_vec();
        // publish to gossipsub
        self.network.publish("/foretias/probity/v1", bincode::encode_to_vec(&report, bincode::config::standard())?)?;
        Ok(())
    }
}
```

(@human — this is the application-side interface. The Foretias application logic decides what attribute names to use — `"liveness"`, `"correctness"`, `"answers_hard_questions"`, etc. — and what values to send. The P2P layer signs them, gossips them, and aggregates them. The application never touches signatures directly.)

---

## PART 10 — IDENTITY COLLISION DETECTION

### 10.1 Signed Heartbeats

Every node broadcasts a signed heartbeat every `heartbeat_secs` (default 30) on gossipsub topic `/foretias/heartbeat/v1`.

```rust
#[derive(Serialize, Deserialize)]
pub struct Heartbeat {
    pub peer_id:      String,
    pub timestamp_ns: u64,
    pub nonce:        [u8; 16],   // fresh random each heartbeat
    pub curve:        u8,
    pub signature:    Vec<u8>,
}
```

The canonical signed bytes are `peer_id || timestamp_ns || nonce || curve`.

### 10.2 Detection — `src/collision/detector.rs`

```rust
pub struct CollisionDetector {
    my_peer_id: String,
    my_pub_key: PublicKeyBytes,
    shutdown_tx: tokio::sync::broadcast::Sender<CollisionEvent>,
}

#[derive(Debug, Clone)]
pub enum CollisionEvent {
    /// Another valid signature seen for our peer_id — cryptographic proof of collision.
    Confirmed {
        first_heartbeat:  Heartbeat,
        second_heartbeat: Heartbeat,
    },
}

impl CollisionDetector {
    pub fn on_heartbeat(&self, hb: Heartbeat) -> Option<CollisionEvent> {
        if hb.peer_id != self.my_peer_id {
            return None;  // not about us
        }

        // Verify signature with our own public key. If it verifies AND it's
        // a heartbeat we didn't send (nonce we don't recognize), we have
        // cryptographic proof that another instance is signing with our key.
        let valid = verify_heartbeat_with(&self.my_pub_key, &hb);
        if !valid { return None; }   // bogus report; ignore

        // At this point another entity is genuinely signing as us.
        let my_hb = self.last_self_heartbeat();
        Some(CollisionEvent::Confirmed {
            first_heartbeat:  my_hb,
            second_heartbeat: hb,
        })
    }
}
```

### 10.3 Escalation — `src/liege/stub.rs`

```rust
/// Liege channel — stubbed. Real implementation is FOSITAS application logic
/// doing stable-marriage matching of peers based on closeness + probity.
pub trait LiegeChannel: Send + Sync {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError>;
}

#[derive(Debug, Clone)]
pub enum HelpReason {
    IdentityCollision {
        first_heartbeat:  Heartbeat,
        second_heartbeat: Heartbeat,
    },
    SuspectedBadActor { peer_id: String },
}

pub struct StubLiegeChannel {
    sender: tokio::sync::mpsc::UnboundedSender<HelpReason>,
}

impl LiegeChannel for StubLiegeChannel {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError> {
        // Just push to an internal channel; application can poll.
        self.sender.send(reason).map_err(|e| NodeError::Internal(e.to_string()))?;
        Ok(())
    }
}
```

### 10.4 Termination Behavior

On `CollisionEvent::Confirmed`:

1. Attempt to notify liege via `LiegeChannel::send_help(HelpReason::IdentityCollision{..})`.
2. Wait up to 30 seconds for application response (app may decide to migrate, sacrifice, etc.).
3. Regardless of response: **stop participating in the P2P network.**
   - Stop gossipsub publishing and subscription.
   - Close all libp2p connections.
   - Stop DHT announcements.
   - Remain a local process that can still serve `stamp()` / `verify()` requests to local clients, but never rejoins the network.
4. Emit `NodeEvent::Terminated { reason: CollisionDetected }` so the application knows.

(@human — the local-operation-continues bit matters for ongoing obligations the application has. A Foretias time family that has been locally stamping things for a user shouldn't suddenly stop stamping just because someone collided with it on the network — it just stops *publishing* those stamps via P2P.)

---

## PART 11 — EPOCH CONSENSUS FRAMEWORK

### 11.1 Purpose

Once per epoch (default 1 hour, configurable), a committee of high-probity peers produces a **FROST-signed snapshot** of the network's probity view:

```rust
#[derive(Serialize, Deserialize)]
pub struct EpochSnapshot {
    pub epoch_number:   u64,
    pub epoch_start_ns: u64,
    pub epoch_end_ns:   u64,
    pub peer_scores:    Vec<PeerScore>,    // sorted by peer_id
    pub committee:      Vec<String>,       // PeerIds of signers
    pub threshold:      u32,               // k of n
    pub frost_signature: Vec<u8>,          // aggregate signature
    pub committee_pubkey: Vec<u8>,         // group public key
}

#[derive(Serialize, Deserialize)]
pub struct PeerScore {
    pub peer_id:  String,
    pub score:    f32,
    pub level:    Option<u32>,             // nobility level if assigned
}
```

### 11.2 Phases

```
Phase                Duration (default)    Action
──────────────────────────────────────────────────────────────────────────
Gossip               55 min                 Peers accumulate signed probity reports
Freeze               2 min                  Ingestion halts; committee finalizes local view
Consensus            2 min                  Committee runs FROST signing over canonical snapshot
Publish              1 min                  Snapshot gossiped; peers adopt as ground truth
                     ─────
                     60 min total
```

Epoch duration is one config knob: `epoch.duration_secs` (default 3600).

### 11.3 Committee Selection

(@human — you deferred specifics on who signs. For the v0.6 epoch consensus
MVP, use a simple approach: the top-probity-scored peers at freeze time form
a committee. In a later release we switch to rotating/random selection.
FOSITAS application logic may override this entirely with its nobility-based
committee selection.)

```rust
pub trait CommitteeSelector: Send + Sync {
    /// Called at freeze. Returns (committee_members, threshold).
    fn select(&self, probity_store: &ProbityStore) -> (Vec<String>, u32);
}

pub struct TopProbityCommitteeSelector {
    pub committee_size: usize,
    pub threshold: u32,
}

impl CommitteeSelector for TopProbityCommitteeSelector {
    fn select(&self, store: &ProbityStore) -> (Vec<String>, u32) {
        let mut scored: Vec<_> = store.scores.read()
            .iter()
            .map(|(id, s)| (id.clone(), *s))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let members: Vec<String> = scored.into_iter()
            .take(self.committee_size)
            .map(|(id, _)| id)
            .collect();
        (members, self.threshold)
    }
}
```

### 11.4 Special Elections

Triggered by:

- `NodeEvent::CollisionDetected` from any peer's heartbeat broadcast
- An application call to `Node::request_special_election(reason)`
- A threshold number of signed malfeasance reports against one peer in a short window

Special elections bypass the normal epoch schedule and produce an out-of-band snapshot that supersedes the next regular snapshot.

### 11.5 Configuration

`config/default.toml` additions:

```toml
[epoch]
duration_secs           = 3600         # 1 hour
freeze_offset_secs      = 300          # 5 min from end
consensus_offset_secs   = 120          # 2 min from end
publish_offset_secs     = 60           # 1 min from end
committee_size          = 7
threshold_k             = 5
special_election_triggers = ["collision", "high_malfeasance"]
```

---

## PART 12 — CALENDAR PERSISTENCE

### 12.1 v0.7 Format — Encrypted JSONL

(@AI Coding Specialist — note: the v0.1 MVP keeps the existing plaintext
JSON calendar format inherited from the Python prototype. The encrypted
JSONL format below is the v0.7 target; see `FORETIAS_OVERVIEW.md` Part 14
for the milestone schedule.)

```
<encrypted block 1>
<encrypted block 2>
<encrypted block 3>
...
```

Each line is base64-encoded ciphertext. Ciphertext is the result of sealing a block of JSON via `CryptoServer::seal_for_self`.

```rust
#[derive(Serialize, Deserialize)]
pub struct CalendarBlock {
    pub block_id:       u64,           // monotonically increasing
    pub written_at_ns:  u64,
    pub tick_records:   Vec<ChrononRecord>,
    pub foretises:       Vec<Foretis>,   // Foretises issued in this block
}
```

```rust
// src/calendar_store/encrypted_jsonl.rs

pub struct EncryptedJsonlCalendarStore {
    path:    std::path::PathBuf,
    server:  Arc<dyn CryptoServer>,
    policy:  CalendarStoragePolicy,
    lru:     BinBasedLru,
}

impl EncryptedJsonlCalendarStore {
    pub fn append_block(&self, block: CalendarBlock) -> Result<(), NodeError> {
        let json = serde_json::to_vec(&block)?;
        let sealed = self.server.seal_for_self(&json)?;
        let line = base64::encode(bincode::encode_to_vec(&sealed, bincode::config::standard())?);
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)?
            .write_all(format!("{}\n", line).as_bytes())?;
        Ok(())
    }

    pub fn read_all_blocks(&self) -> Result<Vec<CalendarBlock>, NodeError> {
        let content = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        for line in content.lines() {
            let sealed_bytes = base64::decode(line.trim())?;
            let (sealed, _): (SealedBlob, _) = bincode::decode_from_slice(&sealed_bytes, bincode::config::standard())?;
            let plain = self.server.unseal_for_self(&sealed)?;
            let block: CalendarBlock = serde_json::from_slice(&plain)?;
            out.push(block);
        }
        Ok(out)
    }
}
```

### 12.2 Storage Policy Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CalendarStoragePolicy {
    /// Store calendars for every peer we see (expensive, complete).
    Everything,
    /// Store only our own calendar (and our liege's — see 12.3).
    MyOwn,
    /// MyOwn + LRU cache of recently-used peers' calendars.
    MyOwnPlusLru {
        max_bytes_total: u64,
    },
}
```

### 12.3 Bin-Based LRU — `src/calendar_store/lru.rs`

```rust
/// Calendars are classified into bins. Eviction always comes from Unused
/// first, and discards randomly to avoid a clairvoyant attacker.
#[derive(Debug, Clone, Copy)]
pub enum CalendarBin {
    MyOwn,   // this node's calendar or liege's — never evicted
    Used,    // recently read OR written
    Unused,  // not touched since last epoch
}

pub struct BinBasedLru {
    max_bytes: u64,
    current_bytes: AtomicU64,
    records: RwLock<HashMap<String, CalendarRecord>>,
}

pub struct CalendarRecord {
    pub peer_id:     String,
    pub bin:         CalendarBin,
    pub size_bytes:  u64,
    pub last_touch:  u64,                // ns since Unix epoch
    pub path:        std::path::PathBuf,
}

impl BinBasedLru {
    pub fn touch(&self, peer_id: &str) {
        let mut guard = self.records.write();
        if let Some(rec) = guard.get_mut(peer_id) {
            rec.last_touch = now_ns();
            rec.bin = CalendarBin::Used;
        }
    }

    pub fn demote_stale_to_unused(&self, stale_threshold_ns: u64) {
        let now = now_ns();
        let mut guard = self.records.write();
        for rec in guard.values_mut() {
            if matches!(rec.bin, CalendarBin::Used)
               && (now - rec.last_touch) > stale_threshold_ns {
                rec.bin = CalendarBin::Unused;
            }
        }
    }

    pub fn evict_to_fit(&self, incoming_bytes: u64) -> Result<(), NodeError> {
        while self.current_bytes.load(Ordering::Relaxed) + incoming_bytes > self.max_bytes {
            let victim = self.pick_unused_random();
            if let Some(rec) = victim {
                std::fs::remove_file(&rec.path)?;
                self.current_bytes.fetch_sub(rec.size_bytes, Ordering::Relaxed);
                self.records.write().remove(&rec.peer_id);
            } else {
                // No Unused left to evict — we have to fail.
                return Err(NodeError::OutOfSpace);
            }
        }
        Ok(())
    }

    fn pick_unused_random(&self) -> Option<CalendarRecord> {
        let guard = self.records.read();
        let unused: Vec<_> = guard.values()
            .filter(|r| matches!(r.bin, CalendarBin::Unused))
            .cloned()
            .collect();
        if unused.is_empty() { return None; }
        use rand::seq::SliceRandom;
        unused.choose(&mut rand::thread_rng()).cloned()
    }
}
```

(@human — the bin scheme (my_own / used / unused) deliberately never discards from my_own or used when unused is non-empty. If the application pins everything as "used" we may run out of space and surface an error; that's correct behavior — the application has to size the budget.)

---

# END OF P2P SPECIFICATION

(@human — together with `FORETIAS_OVERVIEW.md` and `FORETIAS_MVP_SPEC.md`, this document covers everything from `v0.0-baseline` through `v0.8-p2p-demo`. The v0.9+ work is in `FORETIAS_ENCLAVE_SPEC.md`, intentionally held back until the network stack is operational.)
