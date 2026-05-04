# Foretias — P2P Sub-Spec 7: Epoch Consensus (v0.8)

**Milestone tag:** `v0.8-epoch-consensus`
**Prereq:** `v0.7-collision-detection` must be tagged.
**Next:** `v0.9+` custom-plugin enclave backends (`FORETIAS_9_ENCLAVE_SPEC.md`).
  Do not consult that document until this milestone is tagged.

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.7-collision-detection` is tagged.
2. Re-read `FORETIAS_0_OVERVIEW.md` §0.4 (enforcement is social, not cryptographic)
   and §0.5 (probity is one number). Epoch snapshots are the network's
   mechanism for producing a cryptographically-attested shared view of peer
   scores — they are not enforcement; they are a durable, verifiable record.
3. Re-read `FORETIAS_2_P2P_6_probity_gossip.md` §6 (`ProbityStore`) —
   epoch snapshots are derived from the store's current scores.
4. Read this document end to end before writing any code.

---

## 1. GOAL

Once per **epoch** (default 1 hour, configurable), a committee of high-probity
peers produces a **FROST-signed `EpochSnapshot`** — a canonical record of
every known peer's current probity score, the committee members who signed
it, and the signing threshold. All nodes adopt the snapshot as the authoritative
probity ground truth for the epoch.

Two additional mechanisms:

- **Special elections** — triggered by a confirmed collision event (v0.7) or
  a threshold of signed malfeasance reports within a short window. An
  out-of-band snapshot is produced immediately, superseding the next
  regular snapshot.
- **Committee selection** — for v0.8, the top-probity peers at freeze time
  form the committee. FOSITAS application logic may override this via the
  `CommitteeSelector` trait; the default implementation is sufficient for
  all v0.2–v0.8 testing.

(@human — FROST (Flexible Round-Optimised Schnorr Threshold signatures)
lets a k-of-n committee produce a single aggregate signature over the
snapshot without any member exposing their private key. The C11 core already
has `frost_ed25519.c`; this sub-spec wires it into the Rust layer via the
existing `CryptoServer` trait extension. The resulting signature is
compact and verifiable by any node with the committee's group public key.)

---

## 2. ACCEPTANCE DEMO

Five nodes. All running mutual-attest (v0.2), gossip (v0.6), and collision
detection (v0.7). Epoch duration set to 2 minutes for the demo.

```bash
# All five nodes started with --epoch-duration-secs 120
# ... (startup elided; same pattern as prior demos) ...

# After 2 minutes:
$ foretias get-latest-epoch --server 127.0.0.1:4001
{
  "epoch_number": 1,
  "epoch_start_ns": ...,
  "epoch_end_ns": ...,
  "peer_scores": [
    {"peer_id": "...", "score": 12.5},
    ...
  ],
  "committee": ["<PeerID-1>", "<PeerID-2>", "<PeerID-3>"],
  "threshold": 2,
  "frost_signature": "<hex>",
  "committee_pubkey": "<hex>"
}

# Verify the FROST signature locally:
$ foretias verify-epoch-snapshot --snapshot epoch_1.json
FROST signature: VALID
Committee: 3 of 3 configured signers present
```

(@human — for the demo, set `committee_size = 3` and `threshold_k = 2`.
A two-of-three threshold is easy to test with five nodes: even if one
committee member is slow or absent the snapshot still closes.)

---

## 3. NEW FILES

```
src/
└── epoch/
    ├── mod.rs             re-exports
    ├── snapshot.rs        EpochSnapshot, PeerScore types
    ├── scheduler.rs       phase timer (gossip/freeze/consensus/publish)
    ├── committee.rs       CommitteeSelector trait + TopProbitySelector
    ├── frost_bridge.rs    CryptoServer → FROST round coordination
    └── handler.rs         snapshot adoption on receive
```

---

## 4. `EpochSnapshot` — `src/epoch/snapshot.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochSnapshot {
    pub epoch_number:     u64,
    pub epoch_start_ns:   u64,
    pub epoch_end_ns:     u64,
    /// All peer scores known to the committee at freeze time, sorted by peer_id.
    pub peer_scores:      Vec<PeerScore>,
    /// PeerIds (hex) of the committee members who contributed shares.
    pub committee:        Vec<String>,
    /// Signing threshold k (k-of-n).
    pub threshold:        u32,
    /// Aggregate FROST-Ed25519 signature over canonical_bytes().
    pub frost_signature:  Vec<u8>,
    /// FROST group public key for this committee.
    pub committee_pubkey: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScore {
    pub peer_id: String,
    pub score:   f32,
}

impl EpochSnapshot {
    /// Canonical bytes for FROST signing — everything except frost_signature.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // Deterministic: encode as JSON with keys sorted, frost_signature omitted.
        let mut val = serde_json::to_value(self).unwrap();
        val.as_object_mut().unwrap().remove("frost_signature");
        // Sort peer_scores by peer_id for determinism across nodes
        if let Some(arr) = val["peer_scores"].as_array_mut() {
            arr.sort_by(|a, b| {
                a["peer_id"].as_str().unwrap_or("")
                    .cmp(b["peer_id"].as_str().unwrap_or(""))
            });
        }
        serde_json::to_vec(&val).unwrap()
    }
}
```

---

## 5. EPOCH PHASES — `src/epoch/scheduler.rs`

```
Phase         Default duration   Action
────────────────────────────────────────────────────────────────────────
Gossip        55 min             Peers accumulate ProbityReports normally.
Freeze         2 min             ProbityStore::ingest() halted; committee
                                 members finalise their local score view.
Consensus      2 min             Committee runs FROST signing rounds.
Publish        1 min             Snapshot gossiped on /foretias/<ns>/epoch/v1;
                                 all peers adopt it as ground truth.
               ─────
               60 min total (default)
```

```rust
// src/epoch/scheduler.rs

pub struct EpochScheduler {
    inner:            Arc<TimeFamilyInner>,
    epoch_duration:   Duration,
    freeze_offset:    Duration,   // from epoch end; default 5 min
    consensus_offset: Duration,   // from epoch end; default 2 min
    publish_offset:   Duration,   // from epoch end; default 1 min
    epoch_number:     AtomicU64,
}

impl EpochScheduler {
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Align to the next epoch boundary (epoch_number × epoch_duration
            // since UNIX epoch). This ensures all nodes on the same namespace
            // run the same epoch at the same wall-clock time.
            let now = now_ns();
            let epoch_ns = self.epoch_duration.as_nanos() as u64;
            let current_epoch = now / epoch_ns;
            let next_boundary = (current_epoch + 1) * epoch_ns;
            let wait = next_boundary.saturating_sub(now);
            tokio::time::sleep(Duration::from_nanos(wait)).await;

            loop {
                let epoch_num = self.epoch_number.fetch_add(1, Ordering::Relaxed);
                self.run_epoch(epoch_num).await;
            }
        })
    }

    async fn run_epoch(&self, epoch_num: u64) {
        let epoch_ns   = self.epoch_duration.as_nanos() as u64;
        let start_ns   = epoch_num * epoch_ns;
        let end_ns     = start_ns + epoch_ns;

        // --- Gossip phase (default 55 min) ---
        let freeze_at = end_ns.saturating_sub(self.freeze_offset.as_nanos() as u64);
        sleep_until_ns(freeze_at).await;

        // --- Freeze phase ---
        self.inner.probity_store.set_frozen(true);
        let consensus_at = end_ns.saturating_sub(self.consensus_offset.as_nanos() as u64);
        sleep_until_ns(consensus_at).await;

        // --- Consensus phase: run FROST if we are a committee member ---
        let (committee, threshold) = self.inner.committee_selector
            .select(&self.inner.probity_store);
        let my_peer_id = &self.inner.local_peer_id_str;

        if committee.contains(my_peer_id) {
            if let Ok(snapshot) = self.run_frost_round(
                epoch_num, start_ns, end_ns, &committee, threshold
            ).await {
                // Store locally and publish
                self.inner.latest_epoch_snapshot.lock().replace(snapshot.clone());
                let _ = self.inner.swarm_handle.lock().as_ref()
                    .map(|h| h.cmd_tx.send(SwarmCommand::PublishEpochSnapshot(snapshot)));
            }
        }

        // --- Publish phase: wait for snapshot from committee if not a member ---
        let publish_at = end_ns.saturating_sub(self.publish_offset.as_nanos() as u64);
        sleep_until_ns(publish_at).await;
        self.inner.probity_store.set_frozen(false);
    }
}
```

---

## 6. COMMITTEE SELECTION — `src/epoch/committee.rs`

```rust
pub trait CommitteeSelector: Send + Sync {
    /// Called at freeze time. Returns (member_peer_ids, threshold_k).
    fn select(&self, store: &ProbityStore) -> (Vec<String>, u32);
}

/// Default v0.8 selector: top-scoring peers by current probity score.
pub struct TopProbitySelector {
    pub committee_size: usize,
    pub threshold_k:    u32,
}

impl CommitteeSelector for TopProbitySelector {
    fn select(&self, store: &ProbityStore) -> (Vec<String>, u32) {
        let mut scored: Vec<(String, f32)> = store.all_scores();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let members: Vec<String> = scored.into_iter()
            .take(self.committee_size)
            .map(|(id, _)| id)
            .collect();
        (members, self.threshold_k)
    }
}
```

`Arc<dyn CommitteeSelector>` is held on `TimeFamilyInner`. FOSITAS application
logic supplies a different impl (nobility-based selection) by replacing this
at construction time; the P2P layer does not need to know.

---

## 7. FROST BRIDGE — `src/epoch/frost_bridge.rs`

FROST requires a multi-round interactive protocol between committee members.
For v0.8, the rounds are coordinated over GossipSub using a dedicated topic:

```rust
// src/network/gossip.rs  (addition)
pub fn frost_topic(namespace: &str, epoch_num: u64) -> IdentTopic {
    IdentTopic::new(format!("/foretias/{}/frost/{}/v1", namespace, epoch_num))
}
```

The FROST protocol has two rounds:
- **Round 1**: each member broadcasts their commitment (nonce pair).
- **Round 2**: each member broadcasts their signature share.

The coordinator (lowest-sorted peer_id in the committee) aggregates shares
and produces the final signature.

```rust
// src/epoch/frost_bridge.rs

pub async fn run_frost_round(
    inner:       &TimeFamilyInner,
    epoch_num:   u64,
    start_ns:    u64,
    end_ns:      u64,
    committee:   &[String],
    threshold_k: u32,
    round_timeout: Duration,
) -> Result<EpochSnapshot, NodeError> {
    let scores = inner.probity_store.snapshot_scores();
    let peer_scores: Vec<PeerScore> = {
        let mut v: Vec<PeerScore> = scores.into_iter()
            .map(|(id, s)| PeerScore { peer_id: id, score: s })
            .collect();
        v.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        v
    };

    let partial_snapshot = EpochSnapshot {
        epoch_number: epoch_num, epoch_start_ns: start_ns, epoch_end_ns: end_ns,
        peer_scores, committee: committee.to_vec(), threshold: threshold_k,
        frost_signature: vec![], committee_pubkey: vec![],
    };
    let msg = partial_snapshot.canonical_bytes();

    // Round 1: generate and broadcast our commitment
    let (nonce, commitment) = inner.crypto.frost_commit()?;
    broadcast_frost_msg(inner, epoch_num, FrostMsg::Commitment {
        from: inner.local_peer_id_str.clone(),
        commitment: commitment.bytes.to_vec(),
    }).await;

    // Collect commitments from all committee members (with timeout)
    let commitments = collect_frost_msgs(inner, epoch_num, committee, round_timeout,
        |m| matches!(m, FrostMsg::Commitment { .. })).await?;

    // Round 2: compute and broadcast our share
    let share = inner.crypto.frost_sign_share(&msg, &nonce, &commitments)?;
    broadcast_frost_msg(inner, epoch_num, FrostMsg::Share {
        from: inner.local_peer_id_str.clone(),
        share: share.bytes.to_vec(),
    }).await;

    // Coordinator aggregates (lowest peer_id in committee)
    let is_coordinator = committee.iter().min().map(|m| m == &inner.local_peer_id_str)
        .unwrap_or(false);

    if is_coordinator {
        let shares = collect_frost_msgs(inner, epoch_num, committee, round_timeout,
            |m| matches!(m, FrostMsg::Share { .. })).await?;
        let (frost_sig, group_pubkey) = inner.crypto.frost_aggregate(&msg, &shares)?;
        let mut snapshot = partial_snapshot;
        snapshot.frost_signature  = frost_sig.bytes.to_vec();
        snapshot.committee_pubkey = group_pubkey.bytes.to_vec();
        Ok(snapshot)
    } else {
        // Non-coordinator: wait for the published snapshot via the gossip handler
        wait_for_epoch_snapshot(inner, epoch_num, round_timeout).await
    }
}
```

(@human — the FROST implementation in `frost_ed25519.c` covers key
generation, commitment, share computation, and aggregation. The Rust
bridge calls through the existing `CryptoServer` trait; the software
backend wraps the C11 functions. The GossipSub coordination is the only
new networking piece — the crypto is already in the core library.)

### 7.1 `CryptoServer` additions for FROST

```rust
// src/crypto_server/mod.rs  (additions)
pub trait CryptoServer: Send + Sync {
    // ... existing methods ...

    /// Generate a FROST nonce and public commitment for one signing round.
    fn frost_commit(&self) -> Result<(FrostNonce, FrostCommitment), NodeError>;

    /// Compute this member's FROST signature share.
    fn frost_sign_share(
        &self, msg: &[u8], nonce: &FrostNonce,
        commitments: &[FrostCommitment],
    ) -> Result<FrostShare, NodeError>;

    /// Aggregate shares into a final signature. Coordinator only.
    fn frost_aggregate(
        &self, msg: &[u8], shares: &[FrostShare],
    ) -> Result<(FrostiasSig64, ForetiasPubKey32), NodeError>;

    /// Verify a FROST aggregate signature against a group public key.
    fn frost_verify(
        &self, msg: &[u8], sig: &ForetiasSig64, group_pubkey: &ForetiasPubKey32,
    ) -> Result<bool, NodeError>;
}
```

Wrapper types:

```rust
pub struct FrostNonce      { pub(crate) inner: [u8; 64] }  // secret; zeroize on drop
pub struct FrostCommitment { pub bytes: [u8; 64] }          // public; share freely
pub struct FrostShare      { pub bytes: [u8; 32] }          // partial signature
```

---

## 8. SNAPSHOT ADOPTION — `src/epoch/handler.rs`

Called from the swarm event loop on every gossip message on the epoch topic:

```rust
pub fn handle_epoch_snapshot(
    data:   &[u8],
    inner:  &TimeFamilyInner,
) -> Result<(), NodeError> {
    let snapshot: EpochSnapshot = serde_json::from_slice(data)?;

    // 1. Basic sanity: epoch_number must be ≥ current epoch
    let current = inner.epoch_scheduler.current_epoch_number();
    if snapshot.epoch_number < current {
        return Ok(());   // stale snapshot; discard
    }

    // 2. Verify FROST aggregate signature
    let msg = snapshot.canonical_bytes();
    let sig = ForetiasSig64 { bytes: snapshot.frost_signature.clone().try_into()
        .map_err(|_| NodeError::BadFormat("frost_signature length"))? };
    let pubkey = ForetiasPubKey32 { bytes: snapshot.committee_pubkey.clone().try_into()
        .map_err(|_| NodeError::BadFormat("committee_pubkey length"))? };
    if !inner.crypto.frost_verify(&msg, &sig, &pubkey)? {
        return Err(NodeError::BadFormat("invalid FROST signature on epoch snapshot"));
    }

    // 3. Adopt: update ProbityStore with snapshot scores as ground truth
    inner.probity_store.apply_epoch_snapshot(&snapshot);

    // 4. Store as latest
    *inner.latest_epoch_snapshot.lock() = Some(snapshot.clone());

    tracing::info!(epoch = snapshot.epoch_number, "epoch snapshot adopted");
    Ok(())
}
```

`ProbityStore::apply_epoch_snapshot` sets the authoritative base scores for
all listed peers, overriding computed scores. Any per-peer reports accumulated
since the snapshot was signed are then re-applied on top (they are already in
the store; `recompute_all` is called immediately after adoption).

---

## 9. SPECIAL ELECTIONS — `src/epoch/special_election.rs`

A special election skips the normal phase schedule and runs an emergency FROST
signing round immediately.

Triggers:
- `NodeEvent::CollisionConfirmed` (from v0.7 collision detector)
- `TimeFamily::request_special_election(reason)` — application API
- A threshold of signed malfeasance reports against one peer within a
  configurable window (default: 5 reports × `-10.0` or lower within 5 min)

```rust
pub async fn trigger_special_election(
    inner:  Arc<TimeFamilyInner>,
    reason: SpecialElectionReason,
) {
    tracing::info!(?reason, "special election triggered");

    // Cancel current epoch's consensus phase if running
    inner.epoch_scheduler.cancel_current_consensus().await;

    // Run an emergency FROST round with the current probity view
    let epoch_num = inner.epoch_scheduler.next_epoch_number();
    let committee = inner.committee_selector.select(&inner.probity_store);

    if let Ok(snapshot) = run_frost_round(
        &inner, epoch_num, now_ns(), now_ns() + 120_000_000_000,   // 2 min window
        &committee.0, committee.1,
        Duration::from_secs(60),   // tighter timeout for emergency
    ).await {
        // Publish supersedes next regular snapshot
        inner.latest_epoch_snapshot.lock().replace(snapshot.clone());
        if let Some(h) = inner.swarm_handle.lock().as_ref() {
            let _ = h.cmd_tx.send(SwarmCommand::PublishEpochSnapshot(snapshot));
        }
    }
}
```

---

## 10. NEW CLI / RPC COMMANDS

```
Method: get_latest_epoch
Params: {}
Result: EpochSnapshot or null if none yet produced

Method: verify_epoch_snapshot
Params: { "snapshot": <EpochSnapshot JSON> }
Result: { "valid": true, "epoch_number": 1, "committee_size": 3 }
```

```bash
foretias get-latest-epoch [--server host:port]
foretias verify-epoch-snapshot --snapshot <path-to-json>
```

---

## 11. CONFIGURATION ADDITIONS

```json
{
  "epoch": {
    "duration_secs":           3600,
    "freeze_offset_secs":       300,
    "consensus_offset_secs":    120,
    "publish_offset_secs":       60,
    "committee_size":             7,
    "threshold_k":                5,
    "frost_round_timeout_secs":  90,
    "special_election_triggers": ["collision", "high_malfeasance"],
    "malfeasance_threshold_reports": 5,
    "malfeasance_window_secs":      300
  }
}
```

---

## 12. TEST PLAN

- `epoch_snapshot_canonical_bytes_deterministic` — same snapshot always produces same bytes.
- `committee_selector_top_probity` — 10 peers with known scores; confirm top-N selected.
- `frost_commit_sign_aggregate_verify` — single node produces all three FROST artefacts in order; `frost_verify` returns true.
- `snapshot_adoption_rejects_stale_epoch`
- `snapshot_adoption_rejects_bad_frost_sig`
- Integration `five_nodes_produce_epoch_snapshot` — five nodes, epoch duration 2 min; within 3 min at least one `get_latest_epoch` call returns a snapshot with `"valid": true` FROST sig; all five nodes return the same epoch number.
- `special_election_on_collision` — trigger a collision (v0.7 path); confirm a special-election snapshot is produced within 90 s.
- All v0.1–v0.7 tests pass.

---

## 13. NON-GOALS FOR v0.8

- ❌ Nobility-based committee selection — FOSITAS application layer replaces `TopProbitySelector`.
- ❌ Rotating/random committee selection — future sub-spec.
- ❌ Cross-epoch score continuity beyond one `apply_epoch_snapshot` call — the store simply adopts the snapshot; historical aggregation across epochs is application logic.
- ❌ Custom-plugin (enclave) backend for FROST — v0.9+ (`FORETIAS_9_ENCLAVE_SPEC.md`).

---

## 14. MILESTONE CHECKLIST

```
[ ] v0.8.1  EpochSnapshot + PeerScore types; canonical_bytes() deterministic test.
[ ] v0.8.2  CryptoServer FROST methods on software backend
            (frost_commit, frost_sign_share, frost_aggregate, frost_verify).
[ ] v0.8.3  EpochScheduler: phase timer aligned to wall-clock epoch boundaries.
[ ] v0.8.4  CommitteeSelector trait + TopProbitySelector.
[ ] v0.8.5  FrostMsg types; frost topic; Round 1 and Round 2 GossipSub coordination.
[ ] v0.8.6  frost_bridge::run_frost_round: coordinator and non-coordinator paths.
[ ] v0.8.7  Snapshot adoption handler: FROST verify + ProbityStore update.
[ ] v0.8.8  Special election trigger: collision path + malfeasance threshold path.
[ ] v0.8.9  get_latest_epoch + verify_epoch_snapshot RPC methods and CLI commands.
[ ] v0.8.10 five_nodes_produce_epoch_snapshot integration test.
[ ] v0.8.11 special_election_on_collision integration test.
[ ] v0.8.12 All prior tests pass.
[ ] v0.8.13 TAG: v0.8-epoch-consensus.
```

(@human — v0.8 is the final milestone of the P2P series. At this point
the full network stack is operational: two-node direct mutual attestation
(v0.2), libp2p transport (v0.3), DHT discovery (v0.4), hardened and
encrypted storage (v0.5), gossip-based probity scoring (v0.6), collision
detection and dormancy (v0.7), and epoch-level FROST consensus (v0.8).
v0.9+ introduces custom-plugin enclave backends — consult
FORETIAS_9_ENCLAVE_SPEC.md only after this tag.)

---
# END OF SUB-SPEC 7 (v0.8)
