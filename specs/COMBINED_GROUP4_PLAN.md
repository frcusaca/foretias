# COMBINED_GROUP4_PLAN.md
# P2P Major Features — Parallel Agent Plan

**Date:** 2026-05-22; deferral notice 2026-05-26
**Paired Spec:** `COMBINED_GROUP4_SPEC.md`
**Status:** **PARTIALLY EXECUTED + REMAINING WORK PAUSED.** Stream 4a is merged. Stream 4b is merged through Phase 4b.5 inclusive (see commits 74e06d3, b56915f, c2040c7, 83ba4d9, e742e23). **All remaining tasks** — the rest of Stream 4b (Phases 4b.4c StartStream, 4b.4d DoAttestation refactor, 4b.6 graceful shutdown) and the entirety of Stream 4c (Calendar Proof of Storage) — **are PAUSED.** Do not start them yet.

  Resumption preconditions (must all be true before remaining work begins):
    1. `COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` reaches feature completion (current state: initial implementation committed at 082488a, ongoing).
    2. `COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md` is approved and either implemented or close enough that its interfaces are stable.
    3. **This plan is rewritten** to reflect (a) Communerdette as the mirror-traffic carrier and (b) shared task queue with FB/GNF cadence. The current step-by-step lists in this file MAY be discarded in favor of a new task layout aligned with the post-Communerdette architecture.

  Already-landed work (4a + Phases 4b.1–4b.5) is NOT to be reverted. It stays on alpha and continues to be tested.

**Pre-flight:** `COMBINED_GROUP2_PLAN.md` Phase A (applies when work resumes)

---

## Agent Assignment

| Branch | Stream | Files | Phase | Est. effort |
|--------|--------|-------|-------|-------------|
| `g4-a-libp2p-tests` | 4a — Libp2p unit tests | `tests/libp2p_transport_unit.rs` (new) | 1 | 2-4 hr |
| `g4-b-cal-mirror` | 4b — Active Mirroring | calendar/, server/handlers.rs, communerd/mod.rs | 3 | 5-8 days |
| `g4-c-cal-proof` | 4c — Proof of Storage | core/src/merkle.c, calendar_store/, server/handlers.rs | 3 | 3-5 days |

**Phase 1 branch** (4a) has no dependencies — start immediately.
**Phase 3 branches** (4b, 4c) require Group 5-A (Clock injection) to be merged first.
**4b and 4c can be developed in parallel** (different files, minor `CalendarBlock` coordination).

---

## Branch g4-a-libp2p-tests — Libp2p unit tests (Phase 1)

**Spec:** `COMBINED_GROUP4_SPEC.md` §2
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-a-libp2p-tests-$(date +%s)`

### Steps

- [ ] **Pre-flight** — Phase A passes
- [ ] **Read** `p2p/foretias-server/src/communerd/libp2p_transport.rs` end-to-end
      to understand the `cmd_tx: Arc<OnceLock<UnboundedSender<SwarmCommand>>>`
      pattern
- [ ] **Determine test seam:**
      - If `cmd_tx` is `pub(crate)` or the OnceLock has a settable accessor,
        the test can manipulate it directly
      - If not, add a `#[cfg(test)] pub fn set_cmd_tx_for_test(&self, tx: ...)`
        helper to `libp2p_transport.rs`
- [ ] **Create** `p2p/foretias-server/tests/libp2p_transport_unit.rs` with two tests:
      - `request_serialization_writes_jsonrpc_envelope` (see spec §2 2a.1)
      - `closed_cmd_channel_returns_transport_error` (see spec §2 2a.2)
- [ ] **Run:** `cargo test -p foretias-server --test libp2p_transport_unit`
- [ ] **Verify existing integration test still passes:**
      `cargo test -p foretias-server --test integration test_libp2p_direct_rpc`
- [ ] **Commit + merge.**

---

## Branch g4-b-cal-mirror — Calendar Active Mirroring (Phase 3)

**Spec:** `COMBINED_GROUP4_SPEC.md` §3
**Prerequisite:** Group 5-A (Clock injection) merged to alpha
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-b-cal-mirror-$(date +%s)`

### Pre-flight extras (beyond Phase A)

- [ ] Verify Group 5-A has merged: `grep -n "self.clock.now_ns\|clock: Arc<dyn Clock>" p2p/foretias-server/src/communerd/mod.rs` → multiple hits
- [ ] Verify Group 5-A merged into Calendar: `grep -n "clock\|now_ns" p2p/foretias-server/src/calendar/mod.rs p2p/foretias-server/src/calendar/mirror.rs` → at least one hit

### Phase 4b.1 — Doc invariant and trait scaffold

- [ ] Add five-priority module doc-comment to `p2p/foretias-server/src/calendar/mod.rs`
      (text from spec §3.2 table)
- [ ] Add `PeerChangeCallback` trait to `calendar/mod.rs` (or wherever Calendar's
      external interface lives)
- [ ] Add `Option<Arc<dyn PeerChangeCallback>>` field to `Communerd` and an
      `on_peer_change(peers)` call site in Communerd's pool update path
- [ ] **Build:** `cargo build --workspace` passes

### Phase 4b.2 — Task queue scaffold

- [ ] Define `CalendarTask` enum in `calendar/` (new file `calendar/task_queue.rs`)
      with variants for all six task types from spec §3.3
- [ ] Add `task_tx`/`task_rx` `mpsc::channel` fields to Calendar
- [ ] Spawn 4 worker tasks that dequeue and dispatch to placeholder handlers
      (each handler initially just `tracing::debug!("dispatched {:?}", task)`)
- [ ] `cargo test --workspace` passes

### Phase 4b.3 — Wire methods (server side)

- [ ] Add six handler functions in `p2p/foretias-server/src/server/handlers.rs`:
      `handle_mirror_announce`, `handle_history_dump_request`,
      `handle_history_dump_ack`, `handle_history_dump_chunk`,
      `handle_history_dump_complete`, `handle_mirror_health_check`
- [ ] Each handler uses `UnprocessedChrononRecord::from_json_value`
      followed by `.into_clean_authenticated(...)` for any inbound record
      (preserves type-enforced trust boundary)
- [ ] Register all six in `server/jsonrpc.rs` dispatch table
- [ ] Add per-handler unit tests covering happy path + one failure mode each

### Phase 4b.4 — Task implementations

- [ ] Implement `FindNewMirror`: query peer pool, call `mirror_announce` RPC,
      enqueue `InitiateDump` for accepting peer
- [ ] Implement `InitiateDump`: chunked send of full local Chrononchain via
      `history_dump_chunk` (target chunk size: 64 records or 1 MB, whichever first)
- [ ] Implement `StartStream`: subscribe to TickObserver, push each new record
      via `stream_tick` RPC (already exists)
- [ ] Implement `ExploreMirror`: call `mirror_health_check`; on N=3 consecutive
      failures enqueue `ExpireMirror`
- [ ] Implement `ExpireMirror`: remove from mirror list; if mirror count drops
      below `min_mirrors` (config), enqueue `FindNewMirror`
- [ ] Implement `DoAttestation`: move existing mutual attestation logic into
      this task type (refactor — preserve existing behavior)

### Phase 4b.5 — Integration test

- [ ] Create `p2p/foretias-server/tests/mirror_integration.rs`:
      - Two `TimeFamilyServer` instances on different ports
      - Source stamps 10 chronons
      - Configure source's mirror config with mirror's address
      - Trigger `FindNewMirror`; wait for completion
      - Verify mirror's `MirrorStore` has 10 records
      - Source stamps 5 more chronons
      - Wait briefly for stream propagation
      - Verify mirror has 15 records
      - Call `cm.integrity_check(&mirror.calendar(), None, None)` on each
- [ ] `cargo test -p foretias-server --test mirror_integration` passes
- [ ] Full workspace tests pass

### Phase 4b.6 — Commit + merge

- [ ] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4b — Calendar Active Mirroring
      Claude Code 2.1.119 (Claude Code); claude-opus-4-7
      ```
- [ ] Merge to alpha; remove worktree

---

## Branch g4-c-cal-proof — Calendar Proof of Storage (Phase 3)

**Spec:** `COMBINED_GROUP4_SPEC.md` §4
**Prerequisite:** Group 5-A merged to alpha
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-c-cal-proof-$(date +%s)`

### Pre-flight extras

- [ ] Read existing `p2p/core/src/merkle.c` to learn the leaf/node hashing
      conventions used by `foretias_merkle_*`
- [ ] Confirm `foretias_merkle_leaf` and `foretias_merkle_verify_proof`
      already exist (per the deprecated CALENDAR_PROOF_OF_STORAGE_PLAN.md)
- [ ] Verify Group 5-A merged

### Phase 4c.1 — C11 extensions

- [ ] Add `foretias_merkle_root_from_leaves(...)` to `p2p/core/src/merkle.c`
- [ ] Add `foretias_merkle_range_proof(...)` to `merkle.c`
- [ ] Add `foretias_merkle_verify_range_proof(...)` to `merkle.c`
- [ ] Add `FORETIAS_ERR_PROOF_RANGE_EMPTY` and `FORETIAS_ERR_PROOF_RANGE_EXCEEDS`
      to `p2p/core/include/foretias_core.h`
- [ ] Create `p2p/core/tests/test_merkle_range.c` with cases per spec §4.3
- [ ] `cd p2p/core && cmake --build build && ctest --output-on-failure` — all pass

### Phase 4c.2 — Rust FFI wrappers

- [ ] In `p2p/core-engine/src/core/`, add Rust wrappers for the three new
      functions in the same style as existing `foretias_merkle_*` wrappers
- [ ] Length validation at FFI boundary (per AGENTS.md FFI rule)
- [ ] Unit tests for the wrappers

### Phase 4c.3 — `CalendarBlock` extension

- [ ] In `p2p/foretias-server/src/calendar_store/encrypted_jsonl.rs`:
      add `pub merkle_root: [u8; 32]` field to `CalendarBlock` with `#[serde(default)]`
- [ ] Implement `CalendarBlock::compute_merkle_root()` using the C11 FFI wrapper
- [ ] Call `compute_merkle_root()` in `seal_block()` before persisting
- [ ] Verify backward-compat: load a v0.6 block (no merkle_root); compute lazily
      or accept `[0; 32]` as "no proof available"
- [ ] Unit tests for `compute_merkle_root()` (single tick, 2 ticks, 64 ticks)

### Phase 4c.4 — `CalendarStore::prove_storage`

- [ ] In `p2p/foretias-server/src/calendar_store/mod.rs`, define:
      ```rust
      pub struct StorageProofRequest { tbid: String, chronon_start: u64, chronon_end: u64 }
      pub struct BlockProof { block_id: u64, merkle_root: [u8;32], siblings: Vec<[u8;32]> }
      pub struct StorageProofResponse { blocks: Vec<BlockProof>, coverage_ratio: f64 }
      pub struct StorageProofResult { verified: bool, coverage_ratio: f64 }
      ```
- [ ] Implement `CalendarStore::prove_storage(req) -> Option<StorageProofResponse>`
- [ ] Implement `verify_storage_proof(req, resp, known_roots) -> StorageProofResult`
- [ ] Unit tests: single-block, multi-block, missing-block (returns `None`),
      partial coverage

### Phase 4c.5 — JSON-RPC handlers

- [ ] `handle_storage_proof_request` in `server/handlers.rs`
- [ ] `handle_storage_proof_verify` in `server/handlers.rs`
- [ ] Register both in `server/jsonrpc.rs`

### Phase 4c.6 — Probity integration

- [ ] Register `"storage_verified"`, `"storage_partial"`, `"storage_failed"`
      attribute names in `probity/`
- [ ] Add a helper that converts `StorageProofResult` into a `ProbityReport`
      with the correct attribute and value per spec §4.5

### Phase 4c.7 — Integration test

- [ ] Create `p2p/foretias-server/tests/proof_of_storage_integration.rs`:
      - 3-process test: server (holds chronons 1–100), challenger, verifier
      - Full coverage scenario: challenge 30–70 → `coverage_ratio == 1.0`,
        `verified == true`
      - Partial coverage scenario: server holds only 1–50 (evict 51–100 from
        store somehow — may need a test-only API); challenge 30–70 →
        `coverage_ratio == 0.5`, `verified` reflects what was provable
- [ ] `cargo test -p foretias-server --test proof_of_storage_integration` passes
- [ ] Backward-compat test: serialize a block without `merkle_root`; deserialize
      succeeds with `[0;32]`; `prove_storage()` returns `None` (graceful)

### Phase 4c.8 — Commit + merge

- [ ] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4c — Calendar Proof of Storage
      ```
- [ ] Merge to alpha; if Stream 4b has already merged, rebase on top and
      re-run all tests before pushing

---

## Coordination Between 4b and 4c

Both streams modify `CalendarBlock`:

| Stream | Adds | Modifies methods |
|--------|------|------------------|
| 4b | (no new fields) | `seal_block`, dump-chunk serialization |
| 4c | `merkle_root: [u8; 32]` | `seal_block` to compute root |

**Merge resolution:** Whichever lands second must:
1. Rebase on top of alpha
2. Verify `seal_block` correctly does both: compute merkle_root AND any 4b changes
3. Re-run mirror integration test AND proof-of-storage integration test

If a conflict arises in `seal_block`, the agents must talk to the master
coordinator before resolving.

---

## Group-Level Completion Criteria

- [ ] Three new merged branches: `g4-a-libp2p-tests`, `g4-b-cal-mirror`, `g4-c-cal-proof`
- [ ] All integration tests pass (libp2p unit, mirror, proof-of-storage)
- [ ] No regression in existing tests
- [ ] `cargo build --workspace` zero warnings
- [ ] `ctest --output-on-failure` passes after `cmake --build`
