# CLIPPY_FIX_PLAN.md

**Plan: Clippy Warning Remediation — Complete Fix Plan**
**Paired Spec: CLIPPY_FIX_SPEC.md**
**Status: PROPOSED**
**Date: 2026-06-04**
**BRANCH_NAME: tooling/clippy-fixes**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/CLIPPY_FIXES_####**

---

## TODOs

### Wave 0: Setup

- [ ] Create worktree `git worktree add -b tooling/clippy-fixes ${HOME}/tmp/foretias-worktrees/CLIPPY_FIXES_####`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path
- [ ] Verify baseline: `cargo test --workspace` passes
- [ ] Capture baseline: `cargo clippy --workspace --all-targets 2>&1 | grep -c "^warning"` (expect ~166)

### Wave 1: Auto-Fixable Mechanical Fixes (No Tests Needed)

These can be fixed with `cargo clippy --fix` or direct edits. No behavior changes.

- [x] Run `cargo clippy --fix --workspace --allow-dirty` for auto-fixable lints
      (2026-06-04 15:30)
- [x] A1: `clone_on_copy` (26 instances) — remove `.clone()` on Copy types
      (2026-06-04 15:30)
- [x] A3: `redundant_closure` (5 instances) — replace `.map(|x| f(x))` with `.map(f)`
      (2026-06-04 15:30)
- [x] A6: `derivable_impls` (3 instances) — replace manual `impl Default` with `#[derive(Default)]`
      (2026-06-04 15:30)
- [x] A8: `single_match` (1 instance) — replace `match { A => ..., _ => {} }` with `if let`
      (2026-06-04 15:30)
- [x] A9: `unnecessary_lazy_evaluations` (2 instances) — `.or_else(|| x)` → `.or(x)`
      (2026-06-04 15:30)
- [x] A10: `unnecessary_cast` (2 instances) — remove same-type casts
      (2026-06-04 15:30)
- [x] A11: `needless_borrow` (1 instance) — remove unnecessary `&`
      (2026-06-04 15:30)
- [x] A12: `useless_conversion` (3 instances) — remove `.into()` / `NodeError::from()`
      (2026-06-04 15:30)
- [x] A13: `question_mark` (2 instances) — `let Some(x) = y else { return None }` → `let x = y?`
      (2026-06-04 15:30)
- [x] A14: `needless_range_loop` (5 instances) — use `.iter_mut().enumerate()`
      (2026-06-04 15:30)
- [x] A15: `collapsible_match` (1 instance) — collapse nested match
      (2026-06-04 15:30)
- [x] A18: `manual_contains` (1 instance) — `.iter().any()` → `.contains()`
      (2026-06-04 15:30)
- [x] A19: `new_without_default` (1 instance) — add `impl Default for ProbityStore`
      (2026-06-04 15:30)
- [x] A20: `unwrap_or_default` (1 instance) — `.or_insert_with(Vec::new)` → `.or_default()`
      (2026-06-04 15:30)
- [x] A23: `zombie_processes` (2 instances) — ensure `wait()` on all paths
      (2026-06-04 15:30)
- [x] A24: `unused_variables` (1 instance) — prefix with `_`
      (2026-06-04 15:30)
- [x] A25: `empty_line_after_doc_comment` (1 instance) — remove empty line
      (2026-06-04 15:30)
- [x] A27: `explicit_auto_deref` (1 instance) — `&*x` → `&x`
      (2026-06-04 15:30)
- [x] A33: `writing_&Vec` / `writing_&PathBuf` (2 instances) — use slice/Path refs
      (2026-06-04 15:30)
- [x] A34: `push_after_creation` (1 instance) — `vec![]` + `.push()` → `vec![item]`
      (2026-06-04 15:30)
- [x] A36: `replacing_text_with_itself` (8 instances) — remove no-op replacements
      (2026-06-04 15:30)
- [x] A37: `borrowed_expression` (1 instance) — remove unnecessary borrow
      (2026-06-04 15:30)
- [x] Verify: `cargo test --workspace` passes (266 tests)
      (2026-06-04 15:30)
- [x] Check progress: `cargo clippy --workspace --all-targets 2>&1 | grep -c "^warning"` (166 → 48)
      (2026-06-04 15:30)

### Wave 2: Struct Extraction (Tests First)

- [ ] A5: `too_many_arguments` — `gossip_event_loop` (11 args)
  - [ ] Write test: `test_gossip_loop_config_construction` in `communerd/mod.rs`
  - [ ] Create `GossipLoopConfig` struct with all 11 fields
  - [ ] Refactor `gossip_event_loop` to take `GossipLoopConfig`
  - [ ] Run tests, verify pass
- [ ] A5: `too_many_arguments` — `refresh_self_registration` (9 args)
  - [ ] Write test: `test_registration_config_construction` in `communerd/mod.rs`
  - [ ] Create `RegistrationConfig` struct
  - [ ] Refactor `refresh_self_registration` to take `RegistrationConfig`
  - [ ] Run tests, verify pass
- [ ] Verify: `cargo test --workspace` passes

### Wave 3: Type Aliases + Visibility

- [ ] A4: `type_complexity` (5 instances)
  - [ ] Add `type PendingFamilyLookupMap = ...` alias in `communerd/mod.rs`
  - [ ] Add `type PendingLookupMap = ...` alias in `communerd/mod.rs`
  - [ ] Add type aliases in `communerdette.rs` test fixture
  - [ ] Add type aliases in `core-engine`
  - [ ] Verify: `cargo build --workspace` passes
- [ ] A7: `private_interfaces` (6 instances)
  - [ ] Widen `CommunerdetteExecutor` to `pub(super)` OR narrow methods to `pub(self)`
  - [ ] Widen `Communerdette` to `pub(crate)` OR narrow `CommunerdetteLine::new` to `pub(super)`
  - [ ] Widen `CommunerdetteHost` to `pub(crate)` OR narrow `CommunerdetteLine::new`
  - [ ] Verify: `cargo build --workspace` passes
- [ ] A16: `large_enum_variant` (1 instance)
  - [ ] Write test: `test_network_event_sizes` — verify enum size after boxing
  - [ ] Box `NetworkEvent::Identified { info: Box<identify::Info> }`
  - [ ] Update all construction sites
  - [ ] Run tests, verify pass
- [ ] A17: `dead_code` (1 instance)
  - [ ] Decide: remove `verify_report_signature` OR add `#[allow(dead_code)]`
  - [ ] Apply decision
- [ ] Verify: `cargo test --workspace` passes

### Wave 4: `too_many_lines` Refactoring (Tests First, Then Extract)

#### D6: Communerd Type Aliases (No Tests)

- [ ] D6: Add `type` aliases for `pending_family_lookups` and `pending_lookups` in `communerd/mod.rs`
- [ ] Verify: `cargo build --workspace` passes

#### D1: `verify_pair` Refactoring (~120 lines → ~30 lines)

**File:** `core-engine/src/foretias/tick.rs:405`

- [ ] D1.1: Write regression tests for current `verify_pair` behavior
  - [ ] `test_verify_pair_genesis_path_valid` — genesis tick with valid signatures
  - [ ] `test_verify_pair_genesis_path_invalid` — genesis tick with tampered data
  - [ ] `test_verify_pair_normal_path_valid` — normal tick verification
  - [ ] `test_verify_pair_normal_path_invalid` — normal tick with bad signature
  - [ ] `test_verify_pair_genesis_short_forward` — forward_foretis < 64 bytes
  - [ ] `test_verify_pair_genesis_mismatched_genesis` — forward ≠ backward genesis
  - [ ] Run tests against current implementation — ALL must pass
- [ ] D1.2: Extract `extract_genesis_sigs(forward, backward) -> (Sig, Sig, &[u8], &[u8])`
  - [ ] Move genesis splitting logic to new function
  - [ ] Write unit tests: `test_extract_genesis_sigs_*` (3 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D1.3: Extract `verify_genesis_signature(crypto, pubkey, blob, sig) -> bool`
  - [ ] Move genesis verification to new function
  - [ ] Write unit tests: `test_verify_genesis_signature_*` (2 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D1.4: Extract `build_genesis_attest_blob(tbid, curr, stamps, nonce, genesis) -> Vec<u8>`
  - [ ] Move genesis blob construction to new function
  - [ ] Write unit tests: `test_build_genesis_attest_blob_*` (2 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D1.5: Extract `build_normal_attest_blob(tbid, prev, curr, stamps, nonce) -> Vec<u8>`
  - [ ] Move normal blob construction to new function
  - [ ] Write unit tests: `test_build_normal_attest_blob_*` (1 test from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D1.6: Rewrite `verify_pair` to use extracted functions
  - [ ] `verify_pair` should now be < 100 lines
  - [ ] Run ALL tests — verify no regression
  - [ ] Count lines: `wc -l` on function → confirm < 100

#### D2: `handle_storage_proof_verify` Refactoring (~130 lines → ~30 lines)

**File:** `foretias-server/src/server/handlers.rs:1177`

- [ ] D2.1: Write regression tests for current handler behavior
  - [ ] `test_handle_storage_proof_verify_valid` — valid request/response
  - [ ] `test_handle_storage_proof_verify_missing_request` — error case
  - [ ] `test_handle_storage_proof_verify_invalid_merkle_root` — error case
  - [ ] Run tests against current implementation — ALL must pass
- [ ] D2.2: Extract `parse_storage_proof_request(params) -> Result<Req, JsonRpcResponse>`
  - [ ] Move request parsing to new function
  - [ ] Write unit tests: `test_parse_request_*` (3 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D2.3: Extract `parse_single_block(bv, i) -> Result<BlockProof, String>`
  - [ ] Move block parsing to new function
  - [ ] Write unit tests: `test_parse_block_*` (3 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D2.4: Extract `parse_storage_proof_response(params) -> Result<Resp, JsonRpcResponse>`
  - [ ] Move response parsing to new function
  - [ ] Write unit tests: `test_parse_response_*` (1 test from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D2.5: Extract `parse_known_roots(params) -> Vec<[u8; 32]>`
  - [ ] Move known_roots parsing to new function
  - [ ] Write unit tests: `test_parse_known_roots_*` (2 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D2.6: Rewrite handler to use extracted functions
  - [ ] Handler should now be < 100 lines
  - [ ] Run ALL tests — verify no regression

#### D3: `handle_do_chronon_attestation` Refactoring (~155 lines → ~30 lines)

**File:** `foretias-server/src/calendar/task_queue.rs:305`

- [ ] D3.1: Write regression tests for current handler
  - [ ] `test_chronon_attestation_no_communerd` — graceful degradation
  - [ ] `test_chronon_attestation_no_chronomatter` — graceful degradation
  - [ ] `test_chronon_attestation_invalid_tbid` — error handling
  - [ ] Run tests against current implementation
- [ ] D3.2: Extract `gate_attestation_resources(ctx) -> Result<(Arc<Communerd>, Arc<Chronomatter>), ()`
  - [ ] Move resource gate to new function (SHARED with D4)
  - [ ] Write unit tests: `test_gate_*` (3 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D3.3: Extract `fetch_target_latest_tick(line, tbid) -> Result<..., ()`
  - [ ] Move tick fetching to new function
  - [ ] Write unit tests (mock CommunerdetteLine)
  - [ ] Run ALL tests — verify no regression
- [ ] D3.4: Extract `stamp_and_sign_chronon(chronomatter, ctx, tbid) -> Result<..., ()`
  - [ ] Move stamping + signing to new function
  - [ ] Write unit tests: `test_stamp_content_format`, `test_stamp_and_sign_*`
  - [ ] Run ALL tests — verify no regression
- [ ] D3.5: Extract `transmit_chronon_attestation(line, bytes, echo) -> Result<..., ()`
  - [ ] Move transmission to new function (SHARED with D4)
  - [ ] Write unit tests (mock line.stamp)
  - [ ] Run ALL tests — verify no regression
- [ ] D3.6: Extract `verify_attestation_recorded(line, chronon, echo) -> bool`
  - [ ] Move FB verification to new function
  - [ ] Write unit tests: `test_verify_attestation_*` (2 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D3.7: Rewrite handler to use extracted functions
  - [ ] Handler should now be < 100 lines
  - [ ] Run ALL tests — verify no regression

#### D4: `handle_do_epoch_attestation` Refactoring (~120 lines → ~30 lines)

**File:** `foretias-server/src/calendar/task_queue.rs:470`

- [ ] D4.1: Write regression tests for current handler
  - [ ] `test_epoch_attestation_no_communerd` — graceful degradation
  - [ ] `test_epoch_attestation_empty_slice` — error handling
  - [ ] Run tests against current implementation
- [ ] D4.2: Reuse `gate_attestation_resources` from D3
- [ ] D4.3: Extract `fetch_target_latest_epoch(line, tbid) -> Result<ChrononRecord, ()`
  - [ ] Move epoch fetching to new function
  - [ ] Write unit tests (mock line.get_calendar_slice)
  - [ ] Run ALL tests — verify no regression
- [ ] D4.4: Extract `stamp_and_sign_epoch(chronomatter, ctx, tbid) -> Result<..., ()`
  - [ ] Move epoch stamping + signing to new function
  - [ ] Write unit tests
  - [ ] Run ALL tests — verify no regression
- [ ] D4.5: Reuse `transmit_chronon_attestation` from D3
- [ ] D4.6: Rewrite handler to use extracted functions
  - [ ] Handler should now be < 100 lines
  - [ ] Run ALL tests — verify no regression

#### D5: `handle_verify_fb_recorded` Refactoring (~135 lines → ~30 lines)

**File:** `foretias-server/src/calendar/task_queue.rs:781`

- [ ] D5.1: Write regression tests for current handler
  - [ ] `test_verify_fb_no_communerd` — graceful degradation
  - [ ] `test_verify_fb_zero_chronon` — early return
  - [ ] Run tests against current implementation
- [ ] D5.2: Extract `gate_communerd_resource(ctx) -> Result<Arc<Communerd>, ()`
  - [ ] Move resource gate to new function
  - [ ] Write unit tests
  - [ ] Run ALL tests — verify no regression
- [ ] D5.3: Extract `pick_random_chronon_range(latest) -> (u64, u64)`
  - [ ] Move range selection to new function
  - [ ] Write unit tests: `test_pick_random_range_*` (2 tests from spec)
  - [ ] Run ALL tests — verify no regression
- [ ] D5.4: Extract `fetch_and_analyze_coverage(line, start, count) -> CoverageResult`
  - [ ] Move coverage analysis to new function
  - [ ] Write unit tests: `test_coverage_ratio_calculation`
  - [ ] Run ALL tests — verify no regression
- [ ] D5.5: Extract `log_coverage_warnings(tbid, coverage: &CoverageResult)`
  - [ ] Move warning logging to new function
  - [ ] Write unit tests: `test_log_warnings_on_poor_coverage`
  - [ ] Run ALL tests — verify no regression
- [ ] D5.6: Rewrite handler to use extracted functions
  - [ ] Handler should now be < 100 lines
  - [ ] Run ALL tests — verify no regression

#### D7: `gossip_event_loop` (too_many_arguments + too_many_lines)

**File:** `foretias-server/src/communerd/mod.rs:551`

- [ ] D7.1: Reuse `GossipLoopConfig` from Wave 2 (A5)
- [ ] D7.2: If still > 100 lines after struct extraction, extract sub-functions
  - [ ] Analyze remaining structure for extraction candidates
  - [ ] Extract as needed
  - [ ] Run ALL tests — verify no regression

### Wave 5: High-Value Pedantic Warnings

- [ ] B1: `cast_possible_truncation` (~10 instances)
  - [ ] `main.rs:646` — `u64 as usize` → `usize::try_from(start)`
  - [ ] `calendar_store/lru.rs:205` — `u128 as u64` → `u64::try_from(...)`
  - [ ] `tests/toppoli.rs:634` — `usize as u32` → add `#[allow]` with justification
  - [ ] `core-engine` — ~5 instances → fix or suppress with justification
  - [ ] Write test: `test_cast_truncation_safety`
  - [ ] Run tests, verify pass
- [ ] B2: `float_cmp` (~5 instances)
  - [ ] Replace `assert_eq!(a, 0.0)` with `assert!((a - 0.0).abs() < f32::EPSILON)`
  - [ ] Files: `probity/aggregator.rs`, `probity/store.rs`, `probity/mod.rs`
  - [ ] Run tests, verify pass
- [ ] B3: `items_after_statements` (~5 instances)
  - [ ] Move `const FIXED_NS` to top of scope in `calendar_store/encrypted_jsonl.rs`
  - [ ] Run tests, verify pass
- [ ] B4: `match_wildcard_for_single_variants` (~5 instances)
  - [ ] Replace `_` with specific variant in `main.rs`, `probity/gossip_handler.rs`
  - [ ] Run tests, verify pass
- [ ] Verify: `cargo test --workspace` passes

### Wave 6: Suppressions (With Justification)

- [ ] C1: `too_many_arguments` — CLI commands
  - [ ] Add `#[allow(clippy::too_many_arguments)]` to `cmd_serve`, `cmd_verify`, `cmd_prove_verification`
  - [ ] Add comment: "CLI command functions derive args from clap Args struct; grouping would add indirection without clarity"
- [ ] C2: `large_enum_variant` — Already fixed in Wave 3 (A16)
- [ ] C3: `private_interfaces` — Already fixed in Wave 3 (A7)
- [ ] Verify: `cargo build --workspace` passes

### Wave 7: Deprecated API Migration

- [ ] A2: `deprecated ed25519_sign` (15 instances)
  - [ ] Write test: `test_ed25519_sign_with_handle_roundtrip`
  - [ ] Write test: `test_calendar_signatures_still_verify`
  - [ ] Migrate `core/signing.rs` (3 instances)
  - [ ] Migrate `foretias/calendar.rs` (6+ instances)
  - [ ] Migrate `collision/detector.rs` (1 instance)
  - [ ] Run ALL ed25519 tests — verify no regression
- [ ] Verify: `cargo test --workspace` passes

### Wave 8: Final Verification

- [ ] Run `cargo clippy --workspace --all-targets 2>&1 | grep -c "^warning"` → expect 0
- [ ] Run `cargo clippy --workspace --all-targets -- -W clippy::pedantic 2>&1 | grep -c "^warning"` → expect low count (only suppressed items)
- [ ] Run `cargo test --workspace` → ALL tests pass
- [ ] Run `cargo fmt --check` → no formatting changes
- [ ] Update `docs/security/clippy-baseline.md` → "Zero warnings (2026-06-04)"
- [ ] Update `specs/INDEX.md` → mark CLIPPY_FIX as complete

### Finalize

- [ ] Verify all work is complete in ${FULL_WORKTREE_PATH} and committed to tooling/clippy-fixes
- [ ] Merge tooling/clippy-fixes to alpha
- [ ] Cleanup:
  - [ ] Check that CLIPPY_FIX_PLAN.md has all but Cleanup checkboxes completed
  - [ ] This is the last checkbox to be checked in my CLIPPY_FIX_PLAN.md

---

## Dependencies

| Task | Blocks | Blocked By |
|------|--------|------------|
| Wave 1 | Wave 2-8 | None |
| Wave 2 | Wave 4 (D7) | Wave 1 |
| Wave 3 | Wave 4 | Wave 1 |
| Wave 4 (D6) | D1-D5, D7 | Wave 3 |
| Wave 4 (D1) | D2 | Wave 3 |
| Wave 4 (D2) | D3 | D1 |
| Wave 4 (D3) | D4 | D2 |
| Wave 4 (D4) | D5 | D3 (shares functions) |
| Wave 4 (D5) | D7 | D4 |
| Wave 4 (D7) | Wave 5 | D3, Wave 2 |
| Wave 5 | Wave 6 | Wave 4 |
| Wave 6 | Wave 7 | Wave 5 |
| Wave 7 | Wave 8 | Wave 6 |
| Wave 8 | Finalize | Wave 7 |

## Parallel Opportunities

- Wave 1 items can be done in parallel (independent mechanical fixes)
- D1-D5 can be done in parallel (different files, different functions)
- Wave 5 items can be done in parallel (different files)

## Estimated Effort

| Wave | Tasks | Estimated Time |
|------|-------|----------------|
| Wave 0 | 4 | 10 min |
| Wave 1 | 30+ | 30 min (mostly auto-fix) |
| Wave 2 | 6 | 30 min |
| Wave 3 | 10 | 30 min |
| Wave 4 | 40+ | 3-4 hours (tests + refactoring) |
| Wave 5 | 10 | 30 min |
| Wave 6 | 3 | 10 min |
| Wave 7 | 8 | 30 min |
| Wave 8 | 8 | 20 min |
| **Total** | **~120** | **~5-6 hours** |
