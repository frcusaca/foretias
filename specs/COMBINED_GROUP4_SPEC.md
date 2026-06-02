# COMBINED_GROUP4_SPEC.md
# P2P Major Features — libp2p Direct Tests, Calendar Mirroring, Proof of Storage

**Date:** 2026-05-22 (re-verified against alpha @ 80f0714); deferral notice 2026-05-26
**Status:** **PARTIALLY MERGED + DEFERRED.** Stream 4a (libp2p unit tests) and Phases 4b.1–4b.5 of Stream 4b (Calendar Active Mirroring) are merged to alpha. All remaining mirror work — Stream 4b.4c (StartStream), 4b.4d (DoAttestation refactor), 4b.6 (graceful shutdown), and the entirety of Stream 4c (Calendar Proof of Storage) — is **POSTPONED**. This spec must be **REVISED** to integrate with Communerdette (`COMBINED_GROUP7_COMMUNERDETTE_SPEC.md`) and the mutual-attestation cadence (`COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md`) **BEFORE** the remaining work resumes. Do not start implementation on the open items until both prerequisite groups reach feature completion AND this spec has been updated to reflect their integration points (specifically: `MirrorDispatcher` per-TBID surface, `CommunerdetteLine` as the mirror-traffic carrier, and the Calendar task queue's role in coordinating both mirroring and FB/GNF cadence).
**Paired Plan:** `COMBINED_GROUP4_PLAN.md`
**Master Coordination:** `COMBINED_GROUP2_SPEC.md` §2.2
**Postponement reason:** Group 7 (Communerdette) establishes a per-external-TBID relationship manager that the mirror protocol should route through, replacing the ad-hoc direct calls currently used in `MirrorDispatcher`. Group 6 (Mutual Attestation) defines Fast Buddies + GanzNeueFreundschaft cadences that share infrastructure with the mirror task queue. Landing further mirror work before Group 7's interface stabilizes risks rework. The already-merged Phases 4b.1–4b.5 stand and continue to compile/test cleanly; they will be revisited (not reverted) when this spec is updated.

---

## 1. Verification Discovery (2026-05-22)

| Original item | Status | Evidence |
|---------------|--------|----------|
| AGENTS.md transport comparison table (4a.1) | ✅ DONE | `AGENTS.md` lines 265-271 |
| `Libp2pTransport` struct exists | ✅ DONE | `p2p/foretias-server/src/communerd/libp2p_transport.rs:15` |
| `test_libp2p_direct_rpc` integration test | ✅ DONE | `p2p/foretias-server/tests/integration.rs:680` |
| `MirrorStore` infrastructure | ✅ DONE | `p2p/foretias-server/src/calendar/mirror.rs:11` |
| Mirror RPC handlers | ❌ OPEN | No `mirror_announce`/`history_dump_*` handlers found |
| `CalendarBlock.merkle_root` | ❌ OPEN | Not in `calendar_store/encrypted_jsonl.rs` |
| Libp2pTransport unit tests | ❌ OPEN | Only integration coverage, no `tests/libp2p_transport_unit.rs` |

Three streams remain:
- **Stream 4a:** Two small unit tests for `Libp2pTransport` (Phase 1)
- **Stream 4b:** Calendar Active Mirroring full implementation (Phase 3)
- **Stream 4c:** Calendar Proof of Storage full implementation (Phase 3)

---

## 2. Stream 4a — Libp2pTransport Unit Tests

**Severity:** MED (integration test exists but no unit-level diagnostics)
**Branch:** `g4-a-libp2p-tests`
**Phase:** 1 (no dependencies)
**Files added:**
- `p2p/foretias-server/tests/libp2p_transport_unit.rs` (new file)

### Tests required

#### 2a.1 Request serialization test

Construct a `Libp2pTransport`, manually inject a fake `cmd_tx` channel,
invoke `stamp(peer, "abcd", "echo")`, and assert the bytes sent on the channel
parse as a JSON-RPC 2.0 envelope:

```json
{
  "jsonrpc": "2.0",
  "method":  "stamp",
  "params":  { "content": "abcd", "echo": "echo" },
  "id":      <integer>
}
```

#### 2a.2 Oneshot error test

Construct the `Libp2pTransport`, send a `stamp` request, then **drop the
receiver side** of the cmd channel before responding. Assert the call returns
`TransportError::Connect` (or `TransportError::Timeout` — accept whichever the
implementation maps a closed channel to) within the configured timeout.

### Implementation notes

`Libp2pTransport`'s `cmd_tx` is stored in an `Arc<OnceLock<...>>`. The unit test
needs to call `set()` on the OnceLock with a hand-built `mpsc::unbounded_channel`'s
`UnboundedSender`. Read the actual struct definition before writing the test;
the spec may need to expose a `#[cfg(test)] pub fn set_cmd_tx_for_test(...)`
helper if the field is otherwise sealed.

### Acceptance

- [ ] New test file compiles and both tests pass
- [ ] `cargo test -p foretias-server --test libp2p_transport_unit` exits 0
- [ ] No existing test regresses

---

## 3. Stream 4b — Calendar Active Mirroring

**Severity:** MAJOR feature
**Branch:** `g4-b-cal-mirror`
**Phase:** 3 (depends on G5-A Clock injection for `registered_at_ns` timestamps)
**Estimated effort:** 5–8 days

### 3.1 Summary

Calendar Active Mirroring lets a TimeFamily proactively discover, recruit, and
maintain mirror nodes for its Chrononchain. A mirror node accepts inbound
history dumps and live streams; a source node manages the mirror pool. Calendar
becomes a **task-driven orchestrator** rather than a passive store.

### 3.2 Calendar Priority Hierarchy (Invariant)

The `Calendar` module-level doc comment must enumerate these five priorities,
which govern resource allocation and task scheduling:

| # | Priority | Responsibility |
|---|----------|----------------|
| 1 | Critical | Record every tick for the family's chronomatter |
| 2 | High | Support local verify requests (look up ticks, validate chains) |
| 3 | Medium-High | Mutual attestation with peers |
| 4 | Medium | Persist family's calendar in the P2P network (find mirrors) |
| 5 | Low | Mirror other calendars' ticks (starvable) |

### 3.3 Architecture

**Task queue:** Calendar owns a bounded `tokio::sync::mpsc` channel and a small
worker pool (default 4 workers, configurable). Task types:

| Task | Trigger | Side effects |
|------|---------|--------------|
| `DoAttestation` | Tick count or wall-clock interval | Stamp exchange with peer |
| `FindNewMirror` | Mirror count below target | `query_community` + `mirror_announce` RPC |
| `InitiateDump` | New mirror enrolled | Stream full history to mirror |
| `StartStream` | Dump complete | Subscribe to local ticks, push to mirror |
| `ExploreMirror` | Periodic health probe | `mirror_health_check` RPC |
| `ExpireMirror` | Health probe fails N times | Remove from mirror list; may enqueue `FindNewMirror` |

**Peer change callback** — Communerd notifies Calendar of pool changes:
```rust
pub trait PeerChangeCallback: Send + Sync {
    fn on_peer_change(&self, peers: Vec<PeerAddr>);
}
```

### 3.4 Wire Protocol (new JSON-RPC methods)

All over existing transports (Noise_XX TCP or libp2p direct):

| Method | Direction | Purpose |
|--------|-----------|---------|
| `mirror_announce` | Source → Candidate | "I have TBID X, will you mirror?" |
| `history_dump_request` | Source → Mirror | "Open a stream for chronons [start..end]" |
| `history_dump_ack` | Mirror → Source | Accept or refuse (with reason) |
| `history_dump_chunk` | Source → Mirror | One chunk of N `ExternalizedChrononRecord`s |
| `history_dump_complete` | Source → Mirror | End of stream marker |
| `mirror_health_check` | Source → Mirror | Liveness probe; mirror echoes current tick count |

All mirror-side handlers use the existing type-enforced trust boundary types
(`UnprocessedChrononRecord` → `CleanAuthenticatedChrononRecord`); the
`integrity_check` guarantee from the type system applies.

### 3.5 Scope (Locked)

**In scope:**
- Task queue (using `tokio::mpsc` directly — no JobQueue abstraction)
- All six wire methods
- All six task types
- Calendar struct doc comment with five-priority hierarchy
- Integration test: two-node source + mirror with full + live-stream phases

**Out of scope (defer to future major):**
- JobQueue abstraction
- Mirror selection scoring beyond simple peer-pool order
- Mirror-to-mirror replication (chained mirrors)
- Cross-TBID mirror sharing optimization
- Proof of Storage (Stream 4c) is INDEPENDENT — they intersect at `CalendarBlock`
  but each stream owns its own additions; the merge must reconcile

### 3.6 Acceptance Criteria

- [ ] `Calendar` doc comment enumerates five priorities
- [ ] `PeerChangeCallback` trait defined and wired
- [ ] All six wire methods registered in `server/jsonrpc.rs` dispatch
- [ ] Task queue processes all six task types end-to-end
- [ ] Two-node integration test: source recruits mirror, dumps 10 chronons,
      then streams 5 more; mirror has all 15 with `integrity_check` passing
- [ ] `cargo test --workspace` passes

---

## 4. Stream 4c — Calendar Proof of Storage

**Severity:** MAJOR feature
**Branch:** `g4-c-cal-proof`
**Phase:** 3 (depends on G5-A Clock injection for `written_at_ns` in `CalendarBlock`)
**Estimated effort:** 3–5 days

### 4.1 Summary

A peer (challenger) can ask any node (responder) to cryptographically prove that
it holds a specific chronon range for a specific TBID. The proof is a Merkle
range proof over `CalendarBlock` leaves. Probity gossip integrates the result.

### 4.2 Design (locked)

#### `CalendarBlock` extension

```rust
struct CalendarBlock {
    block_id:      u64,
    written_at_ns: u64,
    merkle_root:   [u8; 32],   // NEW — SHA-256 over leaves
    ticks:         Vec<ExternalizedChrononRecord>,
}
```

`merkle_root` has `#[serde(default)]` for backward compatibility with v0.6
blocks (default `[0; 32]`; `prove_storage()` on such blocks returns `None`).

#### Leaf and tree

```
leaf[i] = SHA-256(0x00 || canonical_json(ticks[i]))
node    = SHA-256(0x01 || left || right)
root    = recursive_node(leaves)
```

Pad with `[0; 32]` for the next-power-of-two leaf count. This matches the
existing `foretias_merkle_*` C11 conventions (verify against `merkle.c` before
implementation — if it differs, align Rust to C).

### 4.3 C11 Extensions

Three new functions in `p2p/core/src/merkle.c`:
- `foretias_merkle_root_from_leaves(leaves, n, root_out)`
- `foretias_merkle_range_proof(leaves, n, start, end, proof_out)`
- `foretias_merkle_verify_range_proof(root, proof, start, end, result_out)`

Two new error codes in `foretias_core.h`:
- `FORETIAS_ERR_PROOF_RANGE_EMPTY`
- `FORETIAS_ERR_PROOF_RANGE_EXCEEDS`

Tests in `p2p/core/tests/test_merkle_range.c` covering: empty range, single leaf,
two leaves, adjacent range, non-adjacent range, full range, out-of-bounds.

### 4.4 JSON-RPC Surface

Two new methods:

#### `storage_proof_request`
```json
{ "tbid": "...", "chronon_start": N, "chronon_end": M }
```
Response (`StorageProofResponse`):
```json
{
  "root":           "<hex>",
  "blocks":         [ { "block_id": ..., "merkle_root": "...", "siblings": [...] }, ... ],
  "coverage_ratio": 1.0   // (covered_chronons / requested_chronons), 0.0 to 1.0
}
```
Implementation in `server/handlers.rs::handle_storage_proof_request` delegating
to `CalendarStore::prove_storage()`.

#### `storage_proof_verify`
```json
{ "request": <orig>, "response": <resp>, "known_roots": { "<block_id>": "<hex>" } }
```
Response (`StorageProofResult`):
```json
{ "verified": true, "coverage_ratio": 1.0 }
```
The `known_roots` optional map lets a verifier check against trusted roots if
they've cached them; otherwise the verifier trusts the response's roots blindly
(useful for diagnostic purposes only — production verifiers should always
provide known roots).

### 4.5 Probity Integration

In `probity/gossip_handler.rs` (or a new `probity/storage_proof.rs`):

```rust
match result {
    StorageProofResult { verified: true, .. } =>
        publish_probity("storage_verified", responder, +1.0),
    StorageProofResult { verified: false, coverage_ratio } if coverage_ratio < 1.0 =>
        publish_probity("storage_partial", responder, -0.5 * (1.0 - coverage_ratio)),
    StorageProofResult { verified: false, coverage_ratio } =>
        publish_probity("storage_failed", responder, -1.0 * coverage_ratio),
}
```

Register three new attribute names: `"storage_verified"`, `"storage_partial"`,
`"storage_failed"`.

### 4.6 Coordination with Stream 4b

Both streams modify `CalendarBlock`:
- **Stream 4b** uses `CalendarBlock` as-is to ship history dumps.
- **Stream 4c** adds the `merkle_root` field.

Merge order: whichever stream lands first is unaffected. When the second stream
merges, it must:
- Re-run all tests.
- Ensure 4b's dump-chunk wire format still works with the new `merkle_root` field
  (it should, because `#[serde(default)]` makes it optional on deserialize).

### 4.7 Acceptance Criteria

- [ ] Three C11 functions + tests
- [ ] `CalendarBlock.merkle_root` field
- [ ] `prove_storage()` method on `CalendarStore`
- [ ] Two JSON-RPC methods registered
- [ ] Probity integration with three attribute names
- [ ] Integration test: full coverage (`coverage_ratio == 1.0`)
- [ ] Integration test: partial coverage (`coverage_ratio == 0.5`)
- [ ] Backward-compat test: pre-v0.7 block returns `None` from `prove_storage()`
- [ ] `ctest --output-on-failure` and `cargo test --workspace` pass

---

## 5. Stream 4d — GNF/FB Mutual Attestation Protocol

**Severity:** MAJOR feature
**Status:** NEW — added 2026-06-01 after Group 7 completion clarified the attestation flow
**Prerequisites:** Group 7 (Communerdette) feature complete; Calendar task queue scaffold (Phase 4b.2)
**Estimated effort:** 4-6 days

### 5.1 Summary

Mutual attestation is a **proactive** protocol where our Calendar reaches out to a remote TBID, requests their most recent chronon record, stamps it locally, and sends the stamped result back. This establishes a cryptographic attestation relationship between two time beings. The protocol operates at two granularity levels:

- **Chronon-level** (`DoChrononAttestation`): Attest a single chronon (tick)
- **Epoch-level** (`DoEpochAttestation`): Attest an entire epoch block

Both are initiated by our Calendar, routed through CommunerdetteLine to the recipient's Communerd, and processed by the recipient's Calendar.

### 5.2 The Attestation Flow (Outbound — Proactive)

```
Our Calendar                          Remote TBID
    │                                      │
    │  1. Enqueue DoChrononAttestation     │
    │     { target_tbid }                  │
    │────► CommunerdetteLine               │
    │     .get_tick(latest)                │
    │                                      │
    │     ◄── CleanAuthenticated<          │
    │         ChrononRecord>               │
    │                                      │
    │  2. Internal: Chronomatter.stamp()   │
    │     (stamp-free, internal call)      │
    │     → Foretis                        │
    │                                      │
    │  3. Calendar signs Foretis           │
    │     with Calendar's own key          │
    │     → signed bytes                   │
    │                                      │
    │  4. CommunerdetteLine.stamp()        │
    │     (async, fire-and-forget)         │
    │────►                                 │
    │                                      │
    │                                      │  5. handle_route_stamp
    │                                      │     → handle_stamp
    │                                      │     → Chronomatter.stamp()
    │                                      │
```

**Key invariants:**

1. **Initiation is local.** Our Calendar decides when to attest. No external trigger required (though inbound requests via §5.4 can enqueue the same tasks).
2. **Targeting is by TBID.** `DoChrononAttestation { target_tbid }` carries the recipient's Calendar TBID. Communerdette resolves routing.
3. **Internal stamping is stamp-free.** Chronomatter's `stamp()` is called internally (within the same trust boundary) — no signing needed.
4. **Calendar signs at the external boundary.** The `Foretis` result is signed by Calendar with Calendar's own key before transmission.
5. **Transmission is async.** `CommunerdetteLine.stamp()` is fire-and-forget. The attestation is sent; the response (if any) is handled independently.

### 5.3 Task Queue Extensions

Replace the placeholder `DoAttestation { peer: PeerAddr }` with two new variants:

```rust
pub enum CalendarTask {
    // ... existing variants ...

    /// Chronon-level mutual attestation (GNF protocol).
    /// We request the target's latest chronon, stamp it, send it back.
    DoChrononAttestation { target_tbid: String },

    /// Epoch-level mutual attestation (FB protocol).
    /// We request the target's latest epoch block, stamp it, send it back.
    DoEpochAttestation { target_tbid: String },
}
```

**Priority:** Both are priority 3 (Medium-High — Mutual attestation with peers).

**Scheduling:** Calendar enqueues these tasks based on:
- `MutualAttestConfig.every_n_chronons` for chronon-level
- Epoch transition for epoch-level
- Inbound `stamp_my_chronon` / `stamp_my_chronon_block` requests (§5.4)

### 5.4 Inbound Request Methods (Reactive)

Two new JSON-RPC methods allow a remote peer to **request** that we attest their chronon or epoch block. These are received by our Calendar, prioritized, and processed using the same `DoChrononAttestation` / `DoEpochAttestation` task types.

#### `stamp_my_chronon`

A remote peer asks us to stamp their latest chronon.

```json
{
  "jsonrpc": "2.0",
  "method": "stamp_my_chronon",
  "params": {
    "requester_tbid": "<hex>",
    "chronon_number": 42
  },
  "id": 1
}
```

**Handler flow:**
1. Validate `requester_tbid` matches the connection's authenticated TBID (Take 3 gate)
2. Enqueue `DoChrononAttestation { target_tbid: requester_tbid }`
3. Return immediately: `{ "status": "queued" }`
4. Task executes asynchronously per priority 3 scheduling

#### `stamp_my_chronon_block`

A remote peer asks us to stamp their latest epoch block.

```json
{
  "jsonrpc": "2.0",
  "method": "stamp_my_chronon_block",
  "params": {
    "requester_tbid": "<hex>",
    "epoch_number": 3
  },
  "id": 2
}
```

**Handler flow:**
1. Validate `requester_tbid` matches the connection's authenticated TBID
2. Enqueue `DoEpochAttestation { target_tbid: requester_tbid }`
3. Return immediately: `{ "status": "queued" }`
4. Task executes asynchronously per priority 3 scheduling

**Why async?** Attestation is priority 3 — it must not block priorities 1-2 (record ticks, support local verify). Queuing ensures proper scheduling.

### 5.5 DoChrononAttestation Handler (Implementation)

```rust
async fn handle_do_chronon_attestation(
    target_tbid: &str,
    calendar: &Calendar,
    communerd: &Communerd,
) -> Result<(), CalendarError> {
    // 1. Get CommunerdetteLine for target TBID
    let line = communerd.line_for_tbid(target_tbid)
        .ok_or(CalendarError::UnknownTbid(target_tbid.to_string()))?;

    // 2. Request target's latest chronon
    let latest_tick = line.get_tick(u64::MAX).await
        .map_err(|e| CalendarError::TransportError(e))?;

    // 3. Internal stamp (stamp-free — within trust boundary)
    let foretis = calendar.chronomatter().stamp(
        latest_tick.content.clone(),
        &latest_tick.tbid,
    )?;

    // 4. Calendar signs the Foretis with Calendar's own key
    let signed = calendar.sign_foretis(foretis)?;

    // 5. Transmit async (fire-and-forget)
    let _ = line.stamp(signed.content, &signed.echo).await;

    Ok(())
}
```

### 5.6 DoEpochAttestation Handler (Implementation)

Same pattern as §5.5, but operates on epoch blocks:

```rust
async fn handle_do_epoch_attestation(
    target_tbid: &str,
    calendar: &Calendar,
    communerd: &Communerd,
) -> Result<(), CalendarError> {
    let line = communerd.line_for_tbid(target_tbid)
        .ok_or(CalendarError::UnknownTbid(target_tbid.to_string()))?;

    // Request target's latest epoch block
    let epoch_block = line.get_calendar_slice(latest_epoch, 1).await
        .map_err(|e| CalendarError::TransportError(e))?;

    // Internal stamp, Calendar sign, async transmit
    // ... (same pattern as chronon-level)
}
```

### 5.7 External Attestation Storage

When a stamped result is received from a remote peer, the recipient's Calendar stores it as a `CleanAuthenticated<Foretis>`. **No separate `ExternalAttestation` struct is needed** — the `CleanAuthenticated<R>` wrapper already carries:

- The verified `Foretis` payload (`.inner()`)
- The signature bytes (`.signature_bytes()`)
- The signature algorithm (`.signature_algorithm()`)
- The verification guarantee (type-enforced trust boundary)

The `ChrononRecord.external_attestations` field should store `Vec<CleanAuthenticated<Foretis>>` instead of `Vec<ExternalAttestation>`. The existing `ExternalAttestation` struct becomes deprecated and can be removed once all callers migrate.

**Invariant:** Only `CleanAuthenticated<Foretis>` enters the calendar. Raw `Foretis` cannot be stored as an attestation — the compiler enforces this.

### 5.8 MirrorDispatcher Extension

Add two methods to `MirrorDispatcher` for attestation traffic:

```rust
#[async_trait]
pub trait MirrorDispatcher: Send + Sync {
    // ... existing mirror methods ...

    /// Request mutual attestation at chronon level.
    async fn mutual_attest_chronon(
        &self,
        target_tbid: &str,
    ) -> Result<CleanAuthenticated<Foretis>, String>;

    /// Request mutual attestation at epoch level.
    async fn mutual_attest_epoch(
        &self,
        target_tbid: &str,
    ) -> Result<CleanAuthenticated<Foretis>, String>;
}
```

`Communerd` implements these by delegating to `self.line_for_tbid(tbid).stamp(...)`.

### 5.9 Acceptance Criteria

- [ ] `DoChrononAttestation` and `DoEpochAttestation` task variants defined and handled
- [ ] `stamp_my_chronon` and `stamp_my_chronon_block` JSON-RPC methods registered
- [ ] Outbound attestation flow: get_tick → internal stamp → Calendar sign → async transmit
- [ ] `ChrononRecord.external_attestations` stores `Vec<CleanAuthenticated<Foretis>>` (no separate `ExternalAttestation` struct)
- [ ] MirrorDispatcher extended with `mutual_attest_chronon` and `mutual_attest_epoch`
- [ ] Integration test: two-node mutual attestation (chronon-level)
- [ ] Integration test: two-node mutual attestation (epoch-level)
- [ ] Integration test: inbound `stamp_my_chronon` request → queued → executed
- [ ] `cargo test --workspace` passes

---

## 6. Stream 4e — Chronon Retrieval APIs + FB Verification

**Severity:** MAJOR feature
**Status:** NEW — added 2026-06-01
**Prerequisites:** Group 7 (Communerdette) feature complete; Stream 4d (GNF/FB attestation)
**Estimated effort:** 3-5 days

### 6.1 Summary

Two PtP retrieval APIs allow any peer to request chronon records from our Calendar. The Calendar responds with whatever it has: all of it, some of it, or none. Both APIs support an optional flag to include attestations alongside the records.

Additionally, after sending an attestation to our Fast Buddy (FB), we periodically verify that the FB actually recorded it — either immediately after sending or at randomized intervals.

### 6.2 ChrononRecord Retrieval API

A peer requests a single chronon record from our Calendar.

#### JSON-RPC: `get_chronon`

```json
{
  "jsonrpc": "2.0",
  "method": "get_chronon",
  "params": {
    "tbid": "<hex>",
    "chronon_number": 42,
    "include_attestations": true
  },
  "id": 1
}
```

**Parameters:**
- `tbid`: The TBID whose chronon is requested (must match our Calendar's TBID or a mirrored TBID)
- `chronon_number`: Which chronon to retrieve
- `include_attestations` (optional, default `false`): If `true`, include `external_attestations` in the returned `ChrononRecord`

**Response — "I have it":**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "found",
    "record": { /* CleanAuthenticated<ChrononRecord> */ },
    "attestations": [ /* optional, only if include_attestations=true */ ]
  },
  "id": 1
}
```

**Response — "I don't have it":**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "not_found",
    "reason": "chronon_number out of range"
  },
  "id": 1
}
```

**Handler:** `handle_get_chronon` in `server/handlers.rs`. Forwards to `Calendar::get_chronon(tbid, chronon_number, include_attestations)`.

### 6.3 ChrononChain Retrieval API

A peer requests a range of chronon records from our Calendar.

#### JSON-RPC: `get_chronon_chain`

```json
{
  "jsonrpc": "2.0",
  "method": "get_chronon_chain",
  "params": {
    "tbid": "<hex>",
    "chronon_start": 10,
    "chronon_end": 50,
    "include_attestations": true
  },
  "id": 2
}
```

**Parameters:**
- `tbid`: The TBID whose chain is requested
- `chronon_start`: Inclusive start
- `chronon_end`: Inclusive end
- `include_attestations` (optional, default `false`): If `true`, include attestations

**Response — "I have all of it":**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "complete",
    "records": [ /* Vec<CleanAuthenticated<ChrononRecord>> */ ],
    "attestations": [ /* optional */ ],
    "coverage": { "requested": 41, "returned": 41 }
  },
  "id": 2
}
```

**Response — "I have some of it":**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "partial",
    "records": [ /* subset we have */ ],
    "attestations": [ /* optional */ ],
    "coverage": { "requested": 41, "returned": 23 },
    "gaps": [ { "start": 30, "end": 45 } ]
  },
  "id": 2
}
```

**Response — "I have none":**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "none",
    "coverage": { "requested": 41, "returned": 0 }
  },
  "id": 2
}
```

**Handler:** `handle_get_chronon_chain` in `server/handlers.rs`. Forwards to `Calendar::get_chronon_chain(tbid, start, end, include_attestations)`.

### 6.4 Calendar Implementation

```rust
impl Calendar {
    /// Retrieve a single chronon record.
    pub fn get_chronon(
        &self,
        tbid: &str,
        chronon_number: u64,
        include_attestations: bool,
    ) -> Option<CleanAuthenticated<ChrononRecord>> {
        // Look up in local calendar or mirror store
        // If include_attestations, populate external_attestations field
    }

    /// Retrieve a chain of chronon records.
    pub fn get_chronon_chain(
        &self,
        tbid: &str,
        chronon_start: u64,
        chronon_end: u64,
        include_attestations: bool,
    ) -> ChrononChainResult {
        // Return complete/partial/none with coverage info
    }
}

pub enum ChrononChainResult {
    Complete {
        records: Vec<CleanAuthenticated<ChrononRecord>>,
        attestations: Option<Vec<CleanAuthenticated<Foretis>>>,
        coverage: CoverageInfo,
    },
    Partial {
        records: Vec<CleanAuthenticated<ChrononRecord>>,
        attestations: Option<Vec<CleanAuthenticated<Foretis>>>,
        coverage: CoverageInfo,
        gaps: Vec<Range<u64>>,
    },
    None {
        coverage: CoverageInfo,
    },
}

pub struct CoverageInfo {
    pub requested: u64,
    pub returned: u64,
}
```

### 6.5 FB Verification Protocol

After sending an attestation to our Fast Buddy, we verify that the FB actually recorded it. Two modes:

#### Mode 1: Immediate Verification (Post-Send Check)

Right after `DoChrononAttestation` transmits a stamp to our FB:

```
1. We send attestation to FB (via CommunerdetteLine.stamp)
2. Wait for transmission confirmation
3. Immediately query FB: get_chronon(tbid, chronon_number, include_attestations=true)
4. Check: does the returned ChrononRecord.external_attestations include our stamp?
5. If YES → FB recorded it. Mark relationship healthy.
6. If NO → FB did not record it. Log warning, enqueue retry.
```

**Implementation:** Add to `DoChrononAttestation` handler after step 5 (transmit):
```rust
// 6. Verify FB recorded our attestation
let response = line.get_chronon_with_attestations(target_tbid, chronon_number).await?;
let recorded = response.external_attestations
    .iter()
    .any(|att| att.echo == our_echo && att.content_hash == our_content_hash);

if !recorded {
    warn!("FB did not record our attestation at chronon {}", chronon_number);
    // Enqueue retry or mark relationship degraded
}
```

#### Mode 2: Randomized Verification (Periodic Check)

At randomized intervals (configurable, default: every 10-30 chronons with jitter), verify a random historical chronon:

```
1. Pick a random chronon_number from [1, latest_chronon - 10]
2. Query FB: get_chronon_chain(tbid, random_start, random_end, include_attestations=true)
3. Check: do the returned records include our attestation stamps?
4. If coverage is complete and attestations match → relationship healthy
5. If gaps or missing attestations → log warning, may trigger mirror repair
```

**Implementation:** New task variant `VerifyFbRecorded`:
```rust
/// Verify that our FB recorded our attestation stamps.
/// Runs at randomized intervals.
VerifyFbRecorded { target_tbid: String },
```

**Scheduling:** Enqueued by Calendar's `TickObserver.on_tick_advance()` with randomized jitter:
```rust
fn on_tick_advance(&self, chronon_number: u64) {
    // Randomized: 1 in N chance, where N is configurable with jitter
    if rng.gen_bool(1.0 / self.fb_verify_interval) {
        self.enqueue_task(CalendarTask::VerifyFbRecorded {
            target_tbid: self.fb_tbid.clone(),
        });
    }
}
```

### 6.6 MirrorDispatcher Extension

Add retrieval methods to `MirrorDispatcher`:

```rust
#[async_trait]
pub trait MirrorDispatcher: Send + Sync {
    // ... existing methods ...

    /// Retrieve a single chronon record.
    async fn get_chronon(
        &self,
        target_tbid: &str,
        chronon_number: u64,
        include_attestations: bool,
    ) -> Result<CleanAuthenticated<ChrononRecord>, String>;

    /// Retrieve a chain of chronon records.
    async fn get_chronon_chain(
        &self,
        target_tbid: &str,
        chronon_start: u64,
        chronon_end: u64,
        include_attestations: bool,
    ) -> Result<ChrononChainResult, String>;
}
```

### 6.7 Acceptance Criteria

- [ ] `get_chronon` JSON-RPC method registered and working
- [ ] `get_chronon_chain` JSON-RPC method registered and working
- [ ] Three response statuses: `complete`, `partial`, `none` — all tested
- [ ] `include_attestations` flag works for both APIs
- [ ] `Calendar::get_chronon()` and `Calendar::get_chronon_chain()` implemented
- [ ] Immediate FB verification (Mode 1) after `DoChrononAttestation`
- [ ] Randomized FB verification (Mode 2) via `VerifyFbRecorded` task
- [ ] Integration test: chronon retrieval (found/not_found)
- [ ] Integration test: chain retrieval (complete/partial/none)
- [ ] Integration test: FB verification (recorded/not recorded)
- [ ] `cargo test --workspace` passes
