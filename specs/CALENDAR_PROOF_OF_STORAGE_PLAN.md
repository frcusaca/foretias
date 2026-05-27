# Plan: Calendar Proof-of-Storage for Chrononchain Ranges

**Paired with:** `CALENDAR_PROOF_OF_STORAGE_SPEC.md`
**Status:** DEPRECATED — superseded by `COMBINED_GROUP4_PLAN.md` Stream 4c (2026-05-22).
           All open tasks re-listed in COMBINED_GROUP4_PLAN.md. Do not update this file.
           **Further note (2026-05-26):** Stream 4c is itself now DEFERRED until Communerdette (Group 7) reaches feature completion. See `COMBINED_GROUP4_SPEC.md` header for resumption preconditions.
**Target milestone:** v0.7 (aligned with encrypted calendar persistence)

---

## Phase 0 — Prerequisites

- [ ] v0.1-local-server-mvp must be passing (baseline)
- [ ] v0.7 calendar persistence (`EncryptedJsonlCalendarStore`) must be implemented
- [ ] `foretias_merkle_leaf()` and `foretias_merkle_verify_proof()` must exist in C11 core

## Phase 1 — C11 Core Extensions (3 tasks)

- [ ] Add `foretias_merkle_root_from_leaves()` to `merkle.c` — compute root from leaf array
- [ ] Add `foretias_merkle_range_proof()` to `merkle.c` — generate siblings for a range
- [ ] Add `foretias_merkle_verify_range_proof()` to `merkle.c` — verify range against known root
- [ ] Add `FORETIAS_ERR_PROOF_RANGE_EMPTY` and `FORETIAS_ERR_PROOF_RANGE_EXCEEDS` to `foretias_core.h`
- [ ] Write `test_merkle_range.c` — cover empty range, full range, single leaf, adjacent leaves, non-adjacent leaves
- [ ] Run `cd foretias/p2p/core && cmake -B build && cmake --build build && ctest --output-on-failure`

## Phase 2 — Rust Domain Type Extensions (3 tasks)

- [ ] Extend `CalendarBlock` in `foretias-node/src/calendar_store/encrypted_jsonl.rs`:
  - Add `merkle_root: [u8; 32]` field with `#[serde(default)]`
  - Implement `compute_merkle_root()` — hash each tick leaf, build Merkle tree
  - Implement `prove_range(indices: &[u64]) -> BlockProof`
- [ ] Add `StorageProofRequest`, `StorageProofResponse`, `BlockProof`, `StorageProofResult` types in `foretias-node/src/calendar_store/mod.rs`
- [ ] Implement `CalendarStore::prove_storage(tbid, chronon_start, chronon_end) -> Option<StorageProofResponse>` in `foretias-node/src/calendar_store/mod.rs`
- [ ] Write unit tests for `CalendarBlock::compute_merkle_root()` and `prove_range()`

## Phase 3 — JSON-RPC Surface (2 tasks)

- [ ] Wire `storage_proof_request` handler in `foretias-node/src/server/handlers.rs`
  - Accept `StorageProofRequest`, delegate to `CalendarStore::prove_storage()`
  - Return `StorageProofResponse` or error
- [ ] Wire `storage_proof_verify` handler in `foretias-node/src/server/handlers.rs`
  - Accept request + response + optional known roots
  - Verify per spec §5, return `StorageProofResult`
- [ ] Update `foretias-node/src/server/jsonrpc.rs` to dispatch new methods

## Phase 4 — Probity Integration (1 task)

- [ ] Register new probity attribute names in `foretias-node/src/probity/gossip_handler.rs`:
  - `"storage_verified"`, `"storage_failed"`, `"storage_partial"`
- [ ] Wire `StorageProofResult` → `ProbityReport` conversion:
  - On success → publish `ProbityReport("storage_verified", +1.0)` for the responder
  - On failure → publish `ProbityReport("storage_failed", -1.0 * coverage_ratio)`
  - On partial → publish `ProbityReport("storage_partial", -0.5 * (1.0 - coverage_ratio))`

## Phase 5 — Integration Tests (2 tasks)

- [ ] 3-process integration test (server + challenger + verifier):
  1. Server stores chronons 1–100 for a TBID
  2. Challenger requests proof for chronons 30–70
  3. Server responds with `StorageProofResponse`
  4. Verifier validates response → `coverage_ratio == 1.0`
- [ ] Partial coverage test:
  1. Server has chronons 1–50 (evicted 51–100)
  2. Challenger requests proof for chronons 30–70
  3. Server responds with proof for 30–50 only
  4. Verifier validates → `coverage_ratio == 0.5`, reports `"storage_partial"`

## Phase 6 — Pre-v0.7 Backward Compatibility (1 task)

- [ ] Verify that `CalendarBlock` deserialization with missing `merkle_root`
  defaults to `[0; 32]` (serde `default` attribute)
- [ ] Verify that `prove_storage()` on pre-v0.7 blocks gracefully returns
  `None` (no merkle root = no proof available)

## Phase 7 — Cleanup & Review

- [ ] Run `cargo clippy` on `foretias-node/`
- [ ] Run `cargo test` on `foretias-node/`
- [ ] Run full `make test` (C11 + Rust + Python)
- [ ] Confirm v0.1 integration test still passes

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Merkle range proof is too large for gossipsub | Medium | High | Cap `MAX_PROOF_RANGE` at 1024; optimize sibling packing |
| Pre-v0.7 blocks break proof flow | Low | Medium | `#[serde(default)]` on `merkle_root`; graceful `None` return |
| C11 FFI adds latency to proof gen | Low | Low | Range proofs are O(log N); negligible for N < 1024 |
| Spec diverges from `CHRONONCHAIN_NAMING_SPEC` | Low | Low | Use `chronon_number`, `ChrononChainRecord` terminology throughout |
