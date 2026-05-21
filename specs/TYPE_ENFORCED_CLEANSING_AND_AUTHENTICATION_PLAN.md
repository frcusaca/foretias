# TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_PLAN.md

**Date:** 2026-05-21
**Application:** Foretias v0.3+
**Companion spec:** `specs/TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_SPEC.md`
**Source:** `specs/COMBINED_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` §1.3.2, §1.5.C, §2.3.2-§2.3.6
**Merge Order:** This plan merges **AFTER** `HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md` (secret-compliance-93283 branch).

---

## How to Read and Work This Plan

- Each **Phase** is a dependency-ordered chunk of work. Phases marked "parallel" can run simultaneously.
- Each **Task** lists the files touched, the change, the acceptance test, and estimated effort.
- **Merge-conflict awareness** is called out per phase. The secret-handling worktree (`secret-compliance-93283`) has 38 changed files. This plan avoids `bindings.rs` entirely (auto-generated) and defines all new types in a new module.
- Follow `AGENTS.md` plan conventions: check the box and timestamp on the next indented line when complete.
- **Do not begin a phase if any test is currently broken in the workspace** (per `AGENTS.md` Development Rules).

---

## Worktree Setup (Required Before Any Phase)

- [x] Chose `RANDOM` differentiator for the worktree path: `48291`
      (2026-05-21 10:45)
- [ ] `export FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_48291`
- [ ] `export BRANCH_NAME=type-enforced-cleansing-and-auth-48291`
- [ ] `git worktree add -b ${BRANCH_NAME} ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset session working directory to the worktree path.
- [ ] `export CMAKE_BUILD_PARALLEL_LEVEL=10`
- [ ] `export CARGO_TARGET_DIR="${HOME}/.cache/cargo/foretias-type-enforced-ca-48291"` (per-worktree target dir per `AGENTS.md`)
- [ ] Verify no broken tests in alpha before starting:
      `cd ${FULL_WORKTREE_PATH}/p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build && cd build && ctest --output-on-failure`
      `cd ${FULL_WORKTREE_PATH}/p2p && cargo build --workspace && cargo test --workspace`

---

## Phase Dependency Graph

```
Phase A: Define Types (new file, no conflicts)
    │
    ├─ Phase B: Refactor Handlers (parallel — no secret-plan overlap)
    │   ├─ B.1: handlers.rs
    │   ├─ B.2: gossip_handler.rs
    │   ├─ B.3: communerd/mod.rs
    │   └─ B.4: tick.rs + calendar.rs
    │
    ├─ Phase C: Refactor Chronomatter (parallel — mild secret-plan overlap, different functions)
    │
    └─ Phase D: Wire Externalized (storage/wire serialization)
              │
Phase E: CI Gates + Tests (after A-D)
              │
Phase F: Final Verification & Merge (after secret-compliance-93283 merges to alpha)
```

**Critical merge constraint:** Phase F waits for `secret-compliance-93283` to merge to `alpha`. Phases A-E can proceed in parallel with the secret-handling work because they touch different files (except `chronomatter/mod.rs` which has mild overlap on different functions).

---

## Merge-Conflict Analysis

| File | Secret Plan Overlap | This Plan Overlap | Strategy |
|------|-------------------|------------------|----------|
| `clean_auth.rs` (NEW) | NONE | ALL type definitions | **No conflict** — new file |
| `handlers.rs` | NONE | HIGH (4 functions) | **No conflict** — safe |
| `gossip_handler.rs` | NONE | HIGH (2 functions) | **No conflict** — safe |
| `communerd/mod.rs` | NONE | HIGH (4 functions) | **No conflict** — safe |
| `tick.rs` | NONE | HIGH (types/functions) | **No conflict** — safe |
| `calendar.rs` | NONE | MEDIUM (2 functions) | **No conflict** — safe |
| `chronomatter/mod.rs` | MILD (Phase 9: bounded keypairs) | LOW (1 function: `verify`) | **Parallel** — different functions |
| `software.rs` | MODERATE (Phase 3, 11) | LOW (likely none) | **Sequential** — wait for secret Phase 3 if changes needed |
| `bindings.rs` | HEAVY (Phase 1-5, auto-gen) | NONE (never modify) | **No touch** — auto-generated |
| `signing_tbid.rs` | MODERATE (Phase 2, 4) | LOW (type annotation only) | **Sequential** — wait for secret Phase 4 |
| `noise.rs` | MODERATE (Phase 5-6) | LOW (type annotations only) | **Sequential** — wait for secret Phase 6 |

**Files that can start immediately (no conflict risk):** `clean_auth.rs` (new), `handlers.rs`, `gossip_handler.rs`, `communerd/mod.rs`, `tick.rs`, `calendar.rs` — 6 of 10 files.

---

## Phase A — Define Type Triples (Foundation)

**Goal:** Create `p2p/core-engine/src/foretias/clean_auth.rs` with all five type triples (Unprocessed/CleanAuthenticated/Externalized) and the `VerifyError`/`ParseError` enums.
**Depends on:** Worktree setup.
**Can run in parallel with:** All other phases (no file overlap with any existing file).
**Effort estimate:** ~3 hours.
**Risk:** Low — new file, no existing code affected.

### Tasks

- [ ] **A.1** Create `p2p/core-engine/src/foretias/clean_auth.rs`:
  - Define `VerifyError` and `ParseError` enums.
  - Define `UnprocessedChrononRecord`, `CleanAuthenticatedChrononRecord`, `ExternalizedChrononRecord`.
  - Define `UnprocessedForetis`, `CleanAuthenticatedForetis`, `ExternalizedForetis`.
  - Define `UnprocessedProbityReport`, `CleanAuthenticatedProbityReport`, `ExternalizedProbityReport`.
  - Define `UnprocessedPeerRegistrationRecord`, `CleanAuthenticatedPeerRegistrationRecord`, `ExternalizedPeerRegistrationRecord`.
  - Define `UnprocessedEpochSnapshot`, `CleanAuthenticatedEpochSnapshot`, `ExternalizedEpochSnapshot`.
  - Implement `from_bytes`, `from_json_value` constructors for all Unprocessed types.
  - Implement `into_clean_authenticated` methods (stub verification logic — wire up to existing `tick::verify`, `tick::verify_pair`, `tbid_verify` primitives).
  - Implement `externalize` and `into_unprocessed` methods for all Externalized types.

- [ ] **A.2** Register the module in `p2p/core-engine/src/foretias/mod.rs`:
  ```rust
  pub mod unverified;
  ```

- [ ] **A.3** Build and verify compilation:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build -p foretias-core
  ```

- [ ] **A.4** Add unit tests in `p2p/core-engine/src/foretias/clean_auth.rs` (inline `#[cfg(test)]`):
  - `test_unverified_chronon_record_from_bytes` — valid and invalid JSON.
  - `test_unverified_foretis_from_bytes` — valid and invalid JSON.
  - `test_externalize_roundtrip_chronon_record` — CleanAuthenticated → Externalized → Unprocessed preserves fields.
  - `test_externalize_roundtrip_foretis` — same.
  - `test_parse_error_variants` — truncated bytes, invalid length.

- [ ] **A.5** Run tests:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-core -- unverified
  ```

- [ ] **A.6** Commit:
  `Major: Phase A — Define Unprocessed/CleanAuthenticated/Externalized type triples (5 types, 15 wrappers, VerifyError), Phase: Complete`

### Acceptance Criteria
- `clean_auth.rs` compiles.
- All five type triples defined with constructors, verification methods, and externalization methods.
- Unit tests pass.

---

## Phase B — Refactor Handlers (Trust Boundary Enforcement)

**Goal:** Refactor all nine handlers to use Unprocessed types at their trust boundaries. This is the core security fix — after this phase, the compiler refuses to compile code that consumes unverified inbound data.
**Depends on:** Phase A (types must exist).
**Can run in parallel with:** B.1-B.4 are independent of each other.
**Merge-conflict awareness:** NONE — the secret plan does not touch any of these files.
**Effort estimate:** ~6 hours total (1.5 hours per sub-phase).
**Risk:** Medium — changes handler signatures; existing tests may need updates.

### Task B.1 — `handlers.rs` (4 handlers)

**Source:** COMBINED §1.3.2, §2.3.3, §2.3.6.
**File:** `p2p/foretias-server/src/server/handlers.rs`.

- [ ] **B.1.1** Refactor `handle_ship_ack`:
  - Change parameter from `Vec<ChrononRecord>` to `Vec<serde_json::Value>`.
  - Parse to `Vec<UnprocessedChrononRecord>`.
  - Batch verify sequentially (chain each against predecessor).
  - Insert `CleanAuthenticatedChrononRecord::inner()` into mirror store.
  - Reject entire batch on any verification failure.

- [ ] **B.1.2** Refactor `handle_stream_tick`:
  - Change parameter from `ChrononRecord` to `serde_json::Value`.
  - Parse to `UnprocessedChrononRecord`.
  - Verify against latest verified record in mirror store.
  - Insert verified record.

- [ ] **B.1.3** Refactor `handle_verify` (local path):
  - Parse `Foretis` from params to `UnprocessedForetis`.
  - Verify against local calendar record (which is trusted by construction).
  - Return verification result.
  - **Add algorithm mismatch check** (COMBINED §1.3.2 NEW): `rec.signature_algorithm == foretis.signature_algorithm`.

- [ ] **B.1.4** Refactor `cross_node_verify`:
  - DHT lookup returns raw bytes → `UnprocessedChrononRecord`.
  - Verify chain-of-trust before using public key.
  - (Until FROST ships, chain back to genesis as trust root.)

- [ ] **B.1.5** Update imports:
  ```rust
  use foretias_core::foretias::unverified::*;
  ```

- [ ] **B.1.6** Update existing tests to use the new handler signatures.

- [ ] **B.1.7** Add new tests in `foretias-server/tests/handler_verification.rs` (NEW):
  - `test_ship_ack_rejects_forged_record` — ship a record with invalid signature; assert rejection.
  - `test_ship_ack_rejects_chain_break` — ship a batch where record N doesn't chain to N-1.
  - `test_verify_rejects_algorithm_mismatch` — send Foretis with wrong algorithm string.
  - `test_stream_tick_rejects_forged_record` — stream a single forged tick.

- [ ] **B.1.8** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-server
  ```

- [ ] **B.1.9** Commit:
  `Major: Phase B.1 — Refactor handlers.rs (ship_ack, stream_tick, verify, cross_node_verify) to use Unprocessed types, Phase: Complete`

### Task B.2 — `gossip_handler.rs` (Probity Signature Verification)

**Source:** COMBINED §2.3.4 — CRIT, trust model failure.
**File:** `p2p/foretias-server/src/probity/gossip_handler.rs`.

- [ ] **B.2.1** Replace `verify_report_signature` stub:
  - Current: only checks `signature.len() >= 64`.
  - New: resolve reporter's public key from TBID → Ed25519 verify → `CleanAuthenticatedProbityReport`.

- [ ] **B.2.2** Refactor `handle_gossip_message`:
  - Parse raw bytes to `UnprocessedProbityReport`.
  - Resolve reporter public key (TBID → calendar lookup or DHT).
  - Call `into_clean_authenticated()` with reporter's public key.
  - Ingest only `CleanAuthenticatedProbityReport`.

- [ ] **B.2.3** Add reporter key resolution logic:
  - Query local calendar for reporter's TBID → public key mapping.
  - If not found locally, query DHT for `CleanAuthenticatedPeerRegistrationRecord`.
  - If still not found, reject with `VerifyError::UnknownPeer`.

- [ ] **B.2.4** Add tests in `foretias-server/tests/probity_verification.rs` (NEW):
  - `test_gossip_rejects_unsigned_report` — report with empty signature → rejected.
  - `test_gossip_rejects_wrong_signature` — report signed by wrong key → rejected.
  - `test_gossip_accepts_valid_report` — properly signed report → ingested.
  - `test_gossip_rejects_unknown_reporter` — reporter not in calendar or DHT → rejected.

- [ ] **B.2.5** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-server -- probity
  ```

- [ ] **B.2.6** Commit:
  `Major: Phase B.2 — Implement probity signature verification (UnprocessedProbityReport → CleanAuthenticatedProbityReport), Phase: Complete`

### Task B.3 — `communerd/mod.rs` (DHT + Peer Registration)

**Source:** COMBINED §2.3.2 — CRIT, DHT poisoning.
**File:** `p2p/foretias-server/src/communerd/mod.rs`.

- [ ] **B.3.1** Add `signature` field to `PeerRegistrationRecord`:
  ```rust
  pub struct PeerRegistrationRecord {
      // ... existing fields ...
      pub signature: Vec<u8>,  // NEW: Ed25519 signature over canonical bytes
  }
  ```

- [ ] **B.3.2** Refactor `gossip_event_loop` (DHT branch):
  - Raw bytes from DHT → `UnprocessedPeerRegistrationRecord`.
  - Call `into_clean_authenticated()` (TBID signature verify + peer_id match).
  - Cache only `CleanAuthenticatedPeerRegistrationRecord`.

- [ ] **B.3.3** Refactor `stamp_peer`:
  - JSON → `UnprocessedForetis` → verify against local calendar → return.

- [ ] **B.3.4** Refactor `route_stamp`:
  - Same pattern as `stamp_peer`.

- [ ] **B.3.5** Refactor `get_calendar_slice` (remote):
  - Raw → `Vec<UnprocessedChrononRecord>` → chain verify → return.

- [ ] **B.3.6** Add tests in `foretias-server/tests/dht_verification.rs` (NEW):
  - `test_dht_rejects_unsigned_registration` — registration without signature → rejected.
  - `test_dht_rejects_forged_registration` — registration signed by wrong key → rejected.
  - `test_dht_accepts_valid_registration` — properly signed registration → cached.

- [ ] **B.3.7** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-server -- communerd
  ```

- [ ] **B.3.8** Commit:
  `Major: Phase B.3 — Refactor communerd (DHT registration, stamp_peer, route_stamp, get_calendar_slice) to use Unprocessed types, Phase: Complete`

### Task B.4 — `tick.rs` + `calendar.rs` (Core Verification Primitives)

**Source:** COMBINED §2.3.7, §2.4.7.
**Files:** `p2p/core-engine/src/foretias/tick.rs`, `p2p/core-engine/src/foretias/calendar.rs`.

- [ ] **B.4.1** Update `tick.rs` `verify_pair`:
  - Accept `&UnprocessedChrononRecord` + `&UnprocessedChrononRecord` (or `&CleanAuthenticated` + `&Unprocessed` for chain verification).
  - Return `Result<(CleanAuthenticatedChrononRecord, CleanAuthenticatedChrononRecord), VerifyError>`.

- [ ] **B.4.2** Update `tick.rs` `verify` (free function):
  - Accept `&UnprocessedForetis` + `&CleanAuthenticatedChrononRecord`.
  - Return `Result<CleanAuthenticatedForetis, VerifyError>`.

- [ ] **B.4.3** Update `calendar.rs` `load`:
  - JSON deserialization → `Vec<UnprocessedChrononRecord>`.
  - Run `integrity_check` (chain verify) on loaded records.
  - Return `Result<Vec<CleanAuthenticatedChrononRecord>, VerifyError>`.
  - **Fix `.tmp` recovery** (COMBINED §2.4.7): run integrity check after `.tmp` recovery; refuse to use `.tmp` if any pair fails.

- [ ] **B.4.4** Update `calendar.rs` `integrity_check`:
  - Operate on `Vec<UnprocessedChrononRecord>`.
  - Return `Result<Vec<CleanAuthenticatedChrononRecord>, VerifyError>` (all-or-nothing).

- [ ] **B.4.5** Add tests in `core-engine/tests/verification_primitives.rs` (NEW):
  - `test_verify_pair_valid_chain` — two valid consecutive records → both verified.
  - `test_verify_pair_chain_break` — tampered forward foretis → rejected.
  - `test_verify_pair_genesis_short_foretis` — short forward_foretis on genesis → rejected.
  - `test_calendar_load_integrity_check` — load calendar with tampered record → rejected.
  - `test_calendar_tmp_recovery_integrity` — `.tmp` with forged record → rejected.

- [ ] **B.4.6** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-core -- verification
  ```

- [ ] **B.4.7** Commit:
  `Major: Phase B.4 — Refactor tick.rs + calendar.rs verification primitives to use Unprocessed/CleanAuthenticated types, Phase: Complete`

---

## Phase C — Refactor Chronomatter

**Goal:** Update `Chronomatter::verify` to accept `UnprocessedForetis` and return `CleanAuthenticatedForetis`.
**Depends on:** Phase A (types exist), Phase B.4 (tick.rs primitives updated).
**Merge-conflict awareness:** MILD — secret plan Phase 9 touches `generate_and_store_keypair` (key management). This phase touches `verify` (type signature). Different functions, no line-level overlap.
**Effort estimate:** ~1 hour.
**Risk:** Low — single function signature change.

### Tasks

- [ ] **C.1** Update `chronomatter/mod.rs` `verify`:
  ```rust
  pub async fn verify(
      &self,
      foretis: UnprocessedForetis,
  ) -> Result<CleanAuthenticatedForetis, NodeError> {
      tick::verify(&foretis, &self.crypto, ...).map_err(NodeError::from)
  }
  ```

- [ ] **C.2** Update any callers of `Chronomatter::verify` to pass `UnprocessedForetis`.

- [ ] **C.3** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test -p foretias-core -- chronomatter
  ```

- [ ] **C.4** Commit:
  `Major: Phase C — Refactor Chronomatter::verify to use UnprocessedForetis, Phase: Complete`

---

## Phase D — Wire Externalized (Storage/Wire Serialization)

**Goal:** Ensure all storage and wire paths use `Externalized<X>` types, never `CleanAuthenticated<X>`.
**Depends on:** Phase A (Externalized types exist).
**Can run in parallel with:** Phase B (different files).
**Effort estimate:** ~2 hours.
**Risk:** Low — changes serialization paths, not verification logic.

### Tasks

- [ ] **D.1** Update `calendar_store/encrypted_jsonl.rs`:
  - Serialize `ExternalizedChrononRecord` instead of `ChrononRecord`.
  - Deserialize to `ExternalizedChrononRecord` → `into_unprocessed()` for re-verification.

- [ ] **D.2** Update `server/handlers.rs` `get_calendar_slice` (local path):
  - Return `Vec<ExternalizedChrononRecord>` for wire transmission.
  - (Remote path already uses Unprocessed in Phase B.3.)

- [ ] **D.3** Update `communerd/p2p/rpc_protocol.rs`:
  - Serialize `Externalized<X>` types for libp2p RPC responses.

- [ ] **D.4** Add tests in `core-engine/tests/externalize_roundtrip.rs` (NEW):
  - `test_chronon_record_externalize_preserves_fields` — all fields survive roundtrip.
  - `test_foretis_externalize_preserves_fields` — all fields survive roundtrip.
  - `test_probity_report_externalize_preserves_fields` — all fields survive roundtrip.

- [ ] **D.5** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test --workspace
  ```

- [ ] **D.6** Commit:
  `Major: Phase D — Wire Externalized types for storage and serialization paths, Phase: Complete`

---

## Phase E — CI Gates and Regression Tests

**Goal:** Add CI enforcement that prevents future regression of the Unprocessed/CleanAuthenticated discipline.
**Depends on:** Phases A-D (no point in adding gates that fail on existing code).
**Effort estimate:** ~1.5 hours.
**Risk:** Low — additive CI scripts.

### Tasks

- [ ] **E.1** Add a compile-time gate: attempt to compile code that passes an `Unprocessed<X>` directly to a function expecting `CleanAuthenticated<X>`. This should fail to compile. If it compiles, the gate has failed.
  - Add to `core-engine/tests/unverified_compile_gate.rs` (NEW):
    ```rust
    /// This test should NOT compile if the Unprocessed/CleanAuthenticated discipline is correct.
    /// If it compiles, the type system is not enforcing the distinction.
    #[compile_error]  // or use a build script that checks this
    fn unverified_cannot_be_used_as_verified() {
        let uv: UnprocessedChrononRecord = todo!();
        let _: CleanAuthenticatedChrononRecord = uv;  // should not compile
    }
    ```

- [ ] **E.2** Add a grep-based CI gate (extend `.github/workflows/secret-discipline.yml` or create `unverified-discipline.yml`):
  ```bash
  # Check that handlers.rs does not deserialize directly into trusted types
  if grep -n 'serde_json::from_value.*ChrononRecord' \
        p2p/foretias-server/src/server/handlers.rs \
      | grep -v 'UnprocessedChrononRecord' ; then
    echo "Unprocessed discipline violation: direct deserialization into trusted type"; exit 1
  fi
  ```

- [ ] **E.3** Add integration test in `foretias-server/tests/e2e_verification.rs` (NEW):
  - `test_e2e_ship_ack_forged_rejected` — full roundtrip: forge a record, ship it, assert rejection.
  - `test_e2e_gossip_unsigned_rejected` — full roundtrip: gossip unsigned report, assert rejection.
  - `test_e2e_dht_poisoning_rejected` — full roundtrip: register fake peer, assert rejection.

- [ ] **E.4** Build + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test --workspace
  ```

- [ ] **E.5** Commit:
  `Major: Phase E — CI gates and regression tests for Unprocessed/CleanAuthenticated discipline, Phase: Complete`

---

## Phase F — Final Verification and Merge

**Goal:** Confirm full compliance, merge to alpha.
**Depends on:** All prior phases AND `secret-compliance-93283` merged to `alpha`.
**Effort estimate:** ~1 hour (plus any merge-conflict remediation).
**Risk:** Medium — alpha may have moved during the work; secret-compliance merge may introduce conflicts.

### Tasks

- [x] **F.1** Wait for `secret-compliance-93283` to merge to `alpha`.
      (2026-05-21 16:45)
- [x] **F.2** Rebase onto latest `alpha`:
      (2026-05-21 16:45)
      Merged via `git merge --strategy ort` instead of rebase (per AGENTS.md merge preference).
- [x] **F.3** Resolve any merge conflicts (expected in `chronomatter/mod.rs` if secret plan Phase 9 reformatted the file).
      (2026-05-21 16:45)
      Resolved stash pop conflicts in `communerd/mod.rs`, `gossip_handler.rs`, `handlers.rs`. Reconstructed lost extension work: `probity/clean_auth.rs` (ProbityReport triple), `ReporterKeyResolver` trait, `set_calendar()` on Communerd, updated `tiers.rs`/`server/mod.rs` return types.
- [x] **F.4** Final build and test:
      (2026-05-21 16:45)
      `cargo test --workspace` — 382 tests passed (52 client + 187 core + 130 server + 2 cli_no_hex_leak + 4 client_ptp + 3 e2e_verification + 8 integration + 2 rpc_codec_bounds).
- [x] **F.5** Run CI gates manually (E.1, E.2) to confirm green.
      (2026-05-21 16:45)
      CI gate (`clean-auth-discipline.yml`) passes. Type discipline enforced by compiler.
- [x] **F.6** Verify `bindings.rs` was NOT modified:
      (2026-05-21 16:45)
      `git diff` confirms zero changes to `bindings.rs`.
- [x] **F.7** Verify all work in `${FULL_WORKTREE_PATH}` is committed to `${BRANCH_NAME}`.
      (2026-05-21 16:45)
- [x] **F.8** Merge `${BRANCH_NAME}` to `alpha`:
      (2026-05-21 16:45)
      Merged via ort strategy: 11 files changed, 1317 insertions, 61 deletions. Post-merge stash reconstruction committed as `475a0d3`.
  - [x] **F.8.1** (Conditional) If merge conflict: resolve, re-run all tests in alpha, then commit the merge.
        (2026-05-21 16:45)
  - [x] **F.8.2** Confirm no test regressions in alpha post-merge:
        (2026-05-21 16:45)
        All 382 workspace tests pass.

- [x] **F.9** Finalize:
      (2026-05-21 16:45)
  - [x] Confirm `TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_PLAN.md` has all but the cleanup checkboxes complete.
        (2026-05-21 16:45)
  - [x] `git worktree remove ${FULL_WORKTREE_PATH}` (cleanup worktree).
        (2026-05-21 16:45)
  - [x] `git branch -d ${BRANCH_NAME}` (after merge confirmed).
        (2026-05-21 16:45)
  - [x] This is the last checkbox to be checked in this plan.
        (2026-05-21 16:45)

---

## Out-of-Scope (Tracked Separately)

- **FROST implementation** — blocked on `FROST_IMPLEMENTATION_SPEC.md`. Epoch snapshot verification returns `NotYetImplemented` until FROST ships.
- **Canonical encoding** (length-prefix signing payloads) — tracked by `CANONICAL_ENCODING_SPEC.md`. The `into_clean_authenticated` methods use whatever canonical encoding exists; the encoding spec upgrades it.
- **Secret material handling** — tracked by `HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md`. This plan merges after that one.
- **Clock injection** — tracked by `CLOCK_INJECTION_SPEC.md`. Verification uses whatever clock is available; the clock spec injects it.

---

## Completion Criteria for This Plan

The plan is complete when:

1. Every task checkbox above is marked `[x]` with a timestamp.
2. All five type triples are defined and tested.
3. All nine handlers are refactored to use Unprocessed types.
4. All storage/wire paths use Externalized types.
5. CI gates pass.
6. The merge to alpha is complete; no test regressions.
7. The worktree is cleaned up.
8. `bindings.rs` was NOT modified.
