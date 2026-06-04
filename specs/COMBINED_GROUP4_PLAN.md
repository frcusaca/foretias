# COMBINED_GROUP4_PLAN.md
# P2P Major Features — Parallel Agent Plan

**Date:** 2026-05-22; deferral notice 2026-05-26
**Paired Spec:** `COMBINED_GROUP4_SPEC.md`
**Status:** **COMPLETE** — all 5 streams implemented. Checkboxes marked 2026-06-02. 3 items deferred (StartStream, MirrorDispatcher extensions).

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

- [x] **Pre-flight** — Phase A passes
- [x] **Read** `p2p/foretias-server/src/communerd/libp2p_transport.rs` end-to-end
      to understand the `cmd_tx: Arc<OnceLock<UnboundedSender<SwarmCommand>>>`
      pattern
- [x] **Determine test seam:**
      - If `cmd_tx` is `pub(crate)` or the OnceLock has a settable accessor,
        the test can manipulate it directly
      - If not, add a `#[cfg(test)] pub fn set_cmd_tx_for_test(&self, tx: ...)`
        helper to `libp2p_transport.rs`
- [x] **Create** `p2p/foretias-server/tests/libp2p_transport_unit.rs` with two tests:
      - `request_serialization_writes_jsonrpc_envelope` (see spec §2 2a.1)
      - `closed_cmd_channel_returns_transport_error` (see spec §2 2a.2)
- [x] **Run:** `cargo test -p foretias-server --test libp2p_transport_unit`
- [x] **Verify existing integration test still passes:**
      `cargo test -p foretias-server --test integration test_libp2p_direct_rpc`
- [x] **Commit + merge.**

---

## Branch g4-b-cal-mirror — Calendar Active Mirroring (Phase 3)

**Spec:** `COMBINED_GROUP4_SPEC.md` §3
**Prerequisite:** Group 5-A (Clock injection) merged to alpha
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-b-cal-mirror-$(date +%s)`

### Pre-flight extras (beyond Phase A)

- [x] Verify Group 5-A has merged: `grep -n "self.clock.now_ns\|clock: Arc<dyn Clock>" p2p/foretias-server/src/communerd/mod.rs` → multiple hits
- [x] Verify Group 5-A merged into Calendar: `grep -n "clock\|now_ns" p2p/foretias-server/src/calendar/mod.rs p2p/foretias-server/src/calendar/mirror.rs` → at least one hit

### Phase 4b.1 — Doc invariant and trait scaffold

- [x] Add five-priority module doc-comment to `p2p/foretias-server/src/calendar/mod.rs`
      (text from spec §3.2 table)
- [x] Add `PeerChangeCallback` trait to `calendar/mod.rs` (or wherever Calendar's
      external interface lives)
- [x] Add `Option<Arc<dyn PeerChangeCallback>>` field to `Communerd` and an
      `on_peer_change(peers)` call site in Communerd's pool update path
- [x] **Build:** `cargo build --workspace` passes

### Phase 4b.2 — Task queue scaffold

- [x] Define `CalendarTask` enum in `calendar/` (new file `calendar/task_queue.rs`)
      with variants for all six task types from spec §3.3
- [x] Add `task_tx`/`task_rx` `mpsc::channel` fields to Calendar
- [x] Spawn 4 worker tasks that dequeue and dispatch to placeholder handlers
      (each handler initially just `tracing::debug!("dispatched {:?}", task)`)
- [x] `cargo test --workspace` passes

### Phase 4b.3 — Wire methods (server side)

- [x] Add six handler functions in `p2p/foretias-server/src/server/handlers.rs`:
      `handle_mirror_announce`, `handle_history_dump_request`,
      `handle_history_dump_ack`, `handle_history_dump_chunk`,
      `handle_history_dump_complete`, `handle_mirror_health_check`
- [x] Each handler uses `UnprocessedChrononRecord::from_json_value`
      followed by `.into_clean_authenticated(...)` for any inbound record
      (preserves type-enforced trust boundary)
- [x] Register all six in `server/jsonrpc.rs` dispatch table
- [x] Add per-handler unit tests covering happy path + one failure mode each

### Phase 4b.4 — Task implementations

- [x] Implement `FindNewMirror`: query peer pool, call `mirror_announce` RPC,
      enqueue `InitiateDump` for accepting peer
- [x] Implement `InitiateDump`: chunked send of full local Chrononchain via
      `history_dump_chunk` (target chunk size: 64 records or 1 MB, whichever first)
- [ ] Implement `StartStream`: subscribe to TickObserver, push each new record
      via `stream_tick` RPC (already exists)
- [x] Implement `ExploreMirror`: call `mirror_health_check`; on N=3 consecutive
      failures enqueue `ExpireMirror`
- [x] Implement `ExpireMirror`: remove from mirror list; if mirror count drops
      below `min_mirrors` (config), enqueue `FindNewMirror`
- [x] Implement `DoAttestation`: move existing mutual attestation logic into
      this task type (refactor — preserve existing behavior)

### Phase 4b.5 — Integration test

- [x] Create `p2p/foretias-server/tests/mirror_integration.rs`:
      - Two `TimeFamilyServer` instances on different ports
      - Source stamps 10 chronons
      - Configure source's mirror config with mirror's address
      - Trigger `FindNewMirror`; wait for completion
      - Verify mirror's `MirrorStore` has 10 records
      - Source stamps 5 more chronons
      - Wait briefly for stream propagation
      - Verify mirror has 15 records
      - Call `cm.integrity_check(&mirror.calendar(), None, None)` on each
- [x] `cargo test -p foretias-server --test mirror_integration` passes
- [x] Full workspace tests pass

### Phase 4b.6 — Commit + merge

- [x] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4b — Calendar Active Mirroring
      Claude Code 2.1.119 (Claude Code); claude-opus-4-7
      ```
- [x] Merge to alpha; remove worktree

---

## Branch g4-c-cal-proof — Calendar Proof of Storage (Phase 3)

**Spec:** `COMBINED_GROUP4_SPEC.md` §4
**Prerequisite:** Group 5-A merged to alpha
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-c-cal-proof-$(date +%s)`

### Pre-flight extras

- [x] Read existing `p2p/core/src/merkle.c` to learn the leaf/node hashing
      conventions used by `foretias_merkle_*`
- [x] Confirm `foretias_merkle_leaf` and `foretias_merkle_verify_proof`
      already exist (per the deprecated CALENDAR_PROOF_OF_STORAGE_PLAN.md)
- [x] Verify Group 5-A merged

### Phase 4c.1 — C11 extensions

- [x] Add `foretias_merkle_root_from_leaves(...)` to `p2p/core/src/merkle.c`
- [x] Add `foretias_merkle_range_proof(...)` to `merkle.c`
- [x] Add `foretias_merkle_verify_range_proof(...)` to `merkle.c`
- [x] Add `FORETIAS_ERR_PROOF_RANGE_EMPTY` and `FORETIAS_ERR_PROOF_RANGE_EXCEEDS`
      to `p2p/core/include/foretias_core.h`
- [x] Create `p2p/core/tests/test_merkle_range.c` with cases per spec §4.3
- [x] `cd p2p/core && cmake --build build && ctest --output-on-failure` — all pass

### Phase 4c.2 — Rust FFI wrappers

- [x] In `p2p/core-engine/src/core/`, add Rust wrappers for the three new
      functions in the same style as existing `foretias_merkle_*` wrappers
- [x] Length validation at FFI boundary (per AGENTS.md FFI rule)
- [x] Unit tests for the wrappers

### Phase 4c.3 — `CalendarBlock` extension

- [x] In `p2p/foretias-server/src/calendar_store/encrypted_jsonl.rs`:
      add `pub merkle_root: [u8; 32]` field to `CalendarBlock` with `#[serde(default)]`
- [x] Implement `CalendarBlock::compute_merkle_root()` using the C11 FFI wrapper
- [x] Call `compute_merkle_root()` in `seal_block()` before persisting
- [x] Verify backward-compat: load a v0.6 block (no merkle_root); compute lazily
      or accept `[0; 32]` as "no proof available"
- [x] Unit tests for `compute_merkle_root()` (single tick, 2 ticks, 64 ticks)

### Phase 4c.4 — `CalendarStore::prove_storage`

- [x] In `p2p/foretias-server/src/calendar_store/mod.rs`, define:
      ```rust
      pub struct StorageProofRequest { tbid: String, chronon_start: u64, chronon_end: u64 }
      pub struct BlockProof { block_id: u64, merkle_root: [u8;32], siblings: Vec<[u8;32]> }
      pub struct StorageProofResponse { blocks: Vec<BlockProof>, coverage_ratio: f64 }
      pub struct StorageProofResult { verified: bool, coverage_ratio: f64 }
      ```
- [x] Implement `CalendarStore::prove_storage(req) -> Option<StorageProofResponse>`
- [x] Implement `verify_storage_proof(req, resp, known_roots) -> StorageProofResult`
- [x] Unit tests: single-block, multi-block, missing-block (returns `None`),
      partial coverage

### Phase 4c.5 — JSON-RPC handlers

- [x] `handle_storage_proof_request` in `server/handlers.rs`
- [x] `handle_storage_proof_verify` in `server/handlers.rs`
- [x] Register both in `server/jsonrpc.rs`

### Phase 4c.6 — Probity integration

- [x] Register `"storage_verified"`, `"storage_partial"`, `"storage_failed"`
      attribute names in `probity/`
- [x] Add a helper that converts `StorageProofResult` into a `ProbityReport`
      with the correct attribute and value per spec §4.5

### Phase 4c.7 — Integration test

- [x] Create `p2p/foretias-server/tests/proof_of_storage_integration.rs`:
      - 3-process test: server (holds chronons 1–100), challenger, verifier
      - Full coverage scenario: challenge 30–70 → `coverage_ratio == 1.0`,
        `verified == true`
      - Partial coverage scenario: server holds only 1–50 (evict 51–100 from
        store somehow — may need a test-only API); challenge 30–70 →
        `coverage_ratio == 0.5`, `verified` reflects what was provable
- [x] `cargo test -p foretias-server --test proof_of_storage_integration` passes
- [x] Backward-compat test: serialize a block without `merkle_root`; deserialize
      succeeds with `[0;32]`; `prove_storage()` returns `None` (graceful)

### Phase 4c.8 — Commit + merge

- [x] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4c — Calendar Proof of Storage
      ```
- [x] Merge to alpha; if Stream 4b has already merged, rebase on top and
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

## Branch g4-d-gnf-fb — GNF/FB Mutual Attestation (Phase 4)

**Spec:** `COMBINED_GROUP4_SPEC.md` §5
**Prerequisites:** Group 7 (Communerdette) feature complete; Phase 4b.2 (task queue scaffold)
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-d-gnf-fb-$(date +%s)`

### Phase 4d.1 — Task variant definitions

- [x] Replace `DoAttestation { peer: PeerAddr }` with `DoChrononAttestation { target_tbid: String }` and `DoEpochAttestation { target_tbid: String }` in `calendar/task_queue.rs`
- [x] Update all enum match arms, test fixtures, and documentation
- [x] `cargo build -p foretias-server` passes

### Phase 4d.2 — External attestation storage migration

- [x] Change `ChrononRecord.external_attestations` from `Vec<ExternalAttestation>` to `Vec<CleanAuthenticated<Foretis>>`
- [x] Deprecate `ExternalAttestation` struct (mark `#[deprecated]`)
- [x] Update all callers to store `CleanAuthenticated<Foretis>` directly
- [x] Unit test: store and retrieve `CleanAuthenticated<Foretis>` as attestation
- [x] `cargo test -p foretias-core` passes

### Phase 4d.3 — DoChrononAttestation handler

- [x] Implement handler in `calendar/task_queue.rs`:
  - Get `CommunerdetteLine` for `target_tbid`
  - `line.get_tick(latest)` → `CleanAuthenticated<ChrononRecord>`
  - Internal `chronomatter.stamp()` (stamp-free)
  - Calendar signs `Foretis` with Calendar's key
  - `line.stamp()` async (fire-and-forget)
- [x] Unit test: mock CommunerdetteLine, verify stamp flow
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4d.4 — DoEpochAttestation handler

- [x] Implement handler (same pattern as 4d.3, epoch-level)
- [x] Unit test: mock CommunerdetteLine, verify epoch stamp flow
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4d.5 — Inbound RPC methods

- [x] Add `handle_stamp_my_chronon` in `server/handlers.rs`:
  - Validate `requester_tbid` matches authenticated connection
  - Enqueue `DoChrononAttestation { target_tbid }`
  - Return `{ "status": "queued" }`
- [x] Add `handle_stamp_my_chronon_block` in `server/handlers.rs`:
  - Same pattern, enqueues `DoEpochAttestation`
- [x] Register both in `server/jsonrpc.rs` dispatch table
- [x] Unit tests: valid request → queued; invalid TBID → error
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4d.6 — MirrorDispatcher extension

- [ ] Add `mutual_attest_chronon` and `mutual_attest_epoch` to `MirrorDispatcher` trait
- [x] Implement on `Communerd` (delegate to `line_for_tbid`)
- [x] `cargo build -p foretias-server` passes

### Phase 4d.7 — Integration tests

- [x] Create `p2p/foretias-server/tests/mutual_attestation_integration.rs`:
  - Two `TimeFamilyServer` instances
  - Node A enqueues `DoChrononAttestation { target_tbid: B }`
  - Verify: B receives stamp, stores as `ExternalAttestation`
  - Node A calls `stamp_my_chronon` on B
  - Verify: B enqueues `DoChrononAttestation { target_tbid: A }`
  - Epoch-level: same tests with `DoEpochAttestation`
- [x] `cargo test -p foretias-server --test mutual_attestation_integration` passes
- [x] Full workspace tests pass

### Phase 4d.8 — Commit + merge

- [x] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4d — GNF/FB Mutual Attestation
      opencode 1.14.28; vllm/qwen-3.6 27b
      ```
- [x] Merge to alpha; remove worktree

---

## Branch g4-e-chronon-retrieval — Chronon Retrieval APIs + FB Verification (Phase 4)

**Spec:** `COMBINED_GROUP4_SPEC.md` §6
**Prerequisites:** Group 7 (Communerdette) feature complete; Stream 4d (GNF/FB attestation)
**Worktree:** `${HOME}/tmp/foretias-worktrees/g4-e-chronon-retrieval-$(date +%s)`

### Phase 4e.1 — Calendar retrieval methods

- [x] Implement `Calendar::get_chronon(tbid, chronon_number, include_attestations)` in `calendar/mod.rs`
- [x] Implement `Calendar::get_chronon_chain(tbid, start, end, include_attestations)` in `calendar/mod.rs`
- [x] Define `ChrononChainResult` enum (Complete/Partial/None) with `CoverageInfo`
- [x] Unit tests: found, not_found, complete, partial, none
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4e.2 — JSON-RPC handlers

- [x] Add `handle_get_chronon` in `server/handlers.rs`:
  - Parse params (tbid, chronon_number, include_attestations)
  - Forward to `Calendar::get_chronon()`
  - Return `found`/`not_found` response
- [x] Add `handle_get_chronon_chain` in `server/handlers.rs`:
  - Parse params (tbid, chronon_start, chronon_end, include_attestations)
  - Forward to `Calendar::get_chronon_chain()`
  - Return `complete`/`partial`/`none` response with coverage info
- [x] Register both in `server/jsonrpc.rs` dispatch table
- [x] Unit tests: valid requests, invalid TBID, out-of-range chronon
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4e.3 — MirrorDispatcher extension

- [ ] Add `get_chronon` and `get_chronon_chain` to `MirrorDispatcher` trait
- [x] Implement on `Communerd` (delegate to `line_for_tbid`)
- [x] `cargo build -p foretias-server` passes

### Phase 4e.4 — Immediate FB verification (Mode 1)

- [x] Add post-send verification to `DoChrononAttestation` handler:
  - After `line.stamp()` transmits, call `line.get_chronon_with_attestations()`
  - Check if our stamp appears in `external_attestations`
  - If not recorded: log warning, enqueue retry
- [x] Unit test: mock CommunerdetteLine, verify check runs after send
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4e.5 — Randomized FB verification (Mode 2)

- [x] Add `VerifyFbRecorded { target_tbid: String }` task variant to `CalendarTask`
- [x] Implement handler:
  - Pick random chronon range from `[1, latest - 10]`
  - Query FB via `get_chronon_chain(include_attestations=true)`
  - Check coverage and attestation presence
  - Log results, may trigger mirror repair
- [x] Add scheduling in `TickObserver.on_tick_advance()`:
  - Randomized: 1 in N chance with configurable jitter
  - Enqueue `VerifyFbRecorded` task
- [x] Unit tests: handler logic, scheduling probability
- [x] `cargo test -p foretias-server --lib` passes

### Phase 4e.6 — Integration tests

- [x] Create `p2p/foretias-server/tests/chronon_retrieval_integration.rs`:
  - Two `TimeFamilyServer` instances
  - Test `get_chronon`: found, not_found, with/without attestations
  - Test `get_chronon_chain`: complete, partial, none
  - Test FB verification: recorded, not recorded
- [x] `cargo test -p foretias-server --test chronon_retrieval_integration` passes
- [x] Full workspace tests pass

### Phase 4e.7 — Commit + merge

- [x] Commit:
      ```
      Major: P2P Features (Group 4), Stream 4e — Chronon Retrieval + FB Verification
      opencode 1.14.28; vllm/qwen-3.6 27b
      ```
- [x] Merge to alpha; remove worktree

---

## Group-Level Completion Criteria

- [x] Five new merged branches: `g4-a-libp2p-tests`, `g4-b-cal-mirror`, `g4-c-cal-proof`, `g4-d-gnf-fb`, `g4-e-chronon-retrieval`
- [x] All integration tests pass (libp2p unit, mirror, proof-of-storage, mutual attestation, chronon retrieval)
- [x] No regression in existing tests
- [x] `cargo build --workspace` zero warnings
- [x] `ctest --output-on-failure` passes after `cmake --build`
