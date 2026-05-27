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

## 5. Out of Scope for Group 4

- FROST epoch snapshots
- Cross-mirror chained replication
- Storage proof aggregation across multiple TBIDs
- Mirror reputation scoring algorithms
- Encrypted dump streams (existing transport encryption is reused)
