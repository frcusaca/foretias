# Plan: Terminology Normalization — Auto-Attestation, Mutual Attestation, Chronon Attestation

**Scope:** Step (terminology consistency pass)
**Status:** Draft — pending human review
**Based on:** `TERMINOLOGY_NORMALIZATION_SPEC.md`

---

## Worktree Setup

- [ ] Create worktree `git worktree add -b feat/terminology-normalization /home/hcbusy/tmp/foretias-worktrees/TERMINOLOGY_NORMALIZATION_${RANDOM}`
- [ ] `cd /home/hcbusy/tmp/foretias-worktrees/TERMINOLOGY_NORMALIZATION_${RANDOM}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Config & Type Renames (core-engine)

**Goal:** Rename `AutoAttestConfig` → `MutualAttestConfig`, `AutoAttestObserver` → `MutualAttestObserver` and all related fields/methods.

- [ ] `p2p/core-engine/src/config/p2p.rs` — rename `AutoAttestConfig` → `MutualAttestConfig`; rename field `auto_attest` → `mutual_attest` in `CommunerdConfig`; rename `default_auto_attest_every_n` → `default_mutual_attest_every_n`
- [ ] `p2p/core-engine/src/config/node.rs` — remove deprecated `auto_attest_every_n` field from `NodeConfig` if still present (verify first)
- [ ] `p2p/core-engine/src/config/time_family.rs` — update all accessor methods: `peers()` → `mutual_attest_peers()`, `auto_attest_every_n()` → `mutual_attest_every_n()`, `request_timeout_secs()` → `mutual_attest_request_timeout_secs()`; update all `self.communerd.auto_attest` → `self.communerd.mutual_attest`
- [ ] `p2p/core-engine/src/config/mod.rs` — update re-exports: `AutoAttestConfig` → `MutualAttestConfig`
- [ ] `p2p/core-engine/src/foretias/callbacks.rs` — rename `AutoAttestObserver` → `MutualAttestObserver`; rename methods `on_auto_attest_sent()` → `on_mutual_attest_sent()`, `on_auto_attest_ok()` → `on_mutual_attest_ok()`, `on_auto_attest_failed()` → `on_mutual_attest_failed()`
- [ ] `p2p/core-engine/src/foretias/mod.rs` — update re-exports if `AutoAttestObserver` is re-exported here
- [ ] `p2p/core-engine/src/chronomatter/mod.rs` — rename `auto_attest_observer` → `mutual_attest_observer`; rename `set_auto_attest_observer()` → `set_mutual_attest_observer()`; update all references to `AutoAttestObserver` → `MutualAttestObserver`
- [ ] Run `cargo check -p foretias-core` — verify no compilation errors after Phase 1

---

## Phase 2: Metrics & Observers (foretias-node)

**Goal:** Rename metrics counters and observer usage in foretias-node.

- [ ] `p2p/foretias-node/src/metrics.rs` — rename `auto_attest_sent` → `mutual_attest_sent`, `auto_attest_ok` → `mutual_attest_ok`, `auto_attest_failed` → `mutual_attest_failed` (fields, constructors, JSON keys, increment methods); rename `on_auto_attest_sent` → `on_mutual_attest_sent` etc. in `impl MutualAttestObserver for NodeMetrics`
- [ ] `p2p/foretias-node/src/server/mod.rs` — update `AutoAttestObserver` → `MutualAttestObserver` in import and usage; update `set_auto_attest_observer` → `set_mutual_attest_observer`
- [ ] `p2p/foretias-node/src/communerd/mod.rs` — update all `config.auto_attest` → `config.mutual_attest`; update `AutoAttestConfig` → `MutualAttestConfig` in imports and usage
- [ ] Run `cargo check -p foretias-node` — verify no compilation errors after Phase 2

---

## Phase 3: CLI & Python Bindings

**Goal:** Rename CLI flag and Python binding fields.

- [ ] `p2p/foretias-node/src/main.rs` — rename CLI flag `--auto-attest-every-chronons` → `--mutually-attest-every-chronons`; rename struct field `auto_attest_every_chronons` → `mutually_attest_every_chronons`; update all references (help text, CLI struct, `cmd_serve` signature, `cmd_inspect_attestations` peer display code `c.config().auto_attest` → `c.config().mutual_attest`)
- [ ] `p2p/foretias-python/src/lib.rs` — rename `auto_attest_every_n` → `mutually_attest_every_n` in `PyNodeConfig`; update field access `c.communerd.auto_attest` → `c.communerd.mutual_attest`
- [ ] `p2p/foretias-node/tests/integration.rs` — rename `AutoAttestConfig` → `MutualAttestConfig`; update field access `.auto_attest` → `.mutual_attest`; update test function name `test_two_nodes_auto_attest` → `test_two_nodes_mutual_attest` (optional — keep if not desired)
- [ ] Run `cargo check -p foretias-node` — verify no compilation errors

---

## Phase 4: Shared Variable Renames

**Goal:** Rename `ma_blob` → `attest_blob` in shared code.

- [ ] `p2p/core-engine/src/foretias/tick.rs` — rename `ma_blob` → `attest_blob` in `auto_attestation_blob_with_count()` and all tests
- [ ] `p2p/core-engine/src/foretias/calendar.rs` — rename `ma_blob` → `attest_blob` in all test code
- [ ] `p2p/core-engine/src/chronomatter/mod.rs` — rename `ma_blob` → `attest_blob` in `build_auto_attestation()`
- [ ] Run `cargo test -p foretias-core` — verify tests pass

---

## Phase 5: Spec & Documentation Files

**Goal:** Normalize prose in specs and documentation. These are independent tasks that can be done in parallel.

- [ ] `README.md` — "auto-attestation" → "mutual attestation" in peer flag context (line 64); CLI flag `--auto-attest-every-chronons` → `--mutually-attest-every-chronons` (line 65); verify "auto-attestation" is NOT used for cross-node elsewhere
- [ ] `HOWTO.md` — "auto-attesting" → "mutually attesting" in all cross-node references (lines 8, 115, 203, 205, 226, 258); "auto-attestation" → "mutual attestation" where cross-node
- [ ] `AGENTS.md` — `auto_attest_every_chronons` → `mutually_attest_every_chronons` (line 464)
- [ ] `specs/CLI_SPECIFIED.md` — flag rename `--auto-attest-every-chronons` → `--mutually-attest-every-chronons` (lines 64, 349); description updates
- [ ] `specs/FORETIAS_0_OVERVIEW.md` — context check: "auto-attestation" is used for Chronomatter self-linking (line 144) — this is CORRECT, leave as-is
- [ ] `specs/CHRONONCHAIN_NAMING_SPEC.md` — line 28: "mutual auto-attestation" → "auto-attestation"
- [ ] `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` — prose normalization where `auto_attest` appears in cross-calendar context
- [ ] `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — line 25: verify terminology note is correct (it distinguishes auto vs mutual correctly)
- [ ] `specs/foretias-v1.md` — line 52: "self-transition" → "existing chrononchain"; line 67: "self-transition" → "existing chrononchain"
- [ ] `specs/foretias-v1.md` — line 25: "tick chain" → "chronon chain" (×3 occurrences)
- [ ] `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — line 25: "intra-node tick chaining" → "intra-node chronon chaining"
- [ ] `specs/CONFIG_HIERARCHY_PLAN.md` — `AutoAttestConfig` → `MutualAttestConfig` references; `auto_attest` → `mutual_attest` field references
- [ ] `specs/CLEANUP_REPAIRS_SPEC.md` — `AutoAttestConfig` → `MutualAttestConfig` references (historical doc, optional)
- [ ] `specs/CLEANUP_REPAIRS_PLAN.md` — `AutoAttestConfig` → `MutualAttestConfig` references (historical doc, optional)
- [ ] `specs/FORETIAS_HOWTO_SPEC.md` — "auto-attestation" → "mutual attestation" where cross-node context
- [ ] `specs/DHT_STRESS_TEST_ANALYSIS_PLAN.md` — "auto-attest" → "mutual attest" in prose where cross-node
- [ ] `specs/CALENDAR_ACTIVE_MIRRORING_PLAN.md` — verify terminology (already uses "mutual attestation" correctly)
- [ ] `specs/FORETIAS_2_P2P_5_hardening.md` — "mutual-attest" prose check (already correct as mutual attestation)
- [ ] `specs/FORETIAS_2_P2P_6_probity_gossip.md` — "mutual-attest" prose check (already correct)
- [ ] `specs/FORETIAS_2_P2P_7_collision_detection.md` — "mutual-attest" prose check (already correct)
- [ ] `src/foretias/thin_client.py` — prose comment update where `auto_attest` appears in cross-node context

---

## Phase 6: Build & Test Verification

**Goal:** Confirm everything compiles and tests pass.

- [ ] `cargo build --workspace` — full build succeeds
- [ ] `cargo clippy --workspace` — no new warnings
- [ ] `cargo test --workspace` — all Rust tests pass (111+ tests)
- [ ] `cd p2p/core/build && ctest --output-on-failure` — all C11 tests pass
- [ ] `python -m pytest tests/ -v` — all Python tests pass
- [ ] Grep verification: `grep -r "AutoAttestConfig" p2p/ src/` — returns zero matches
- [ ] Grep verification: `grep -r "AutoAttestObserver" p2p/ src/` — returns zero matches
- [ ] Grep verification: `grep -r "\.auto_attest\." p2p/core-engine/src/ p2p/foretias-node/src/` — returns zero matches (cross-calendar context only)
- [ ] Grep verification: `grep -r "ma_blob" p2p/core-engine/src/` — returns zero matches
- [ ] Grep verification: confirm `auto_attestation_blob` and `build_auto_attestation` still exist unchanged

---

## Phase 7: Commit & Merge

- [ ] Commit with message: "Step: Terminology normalization — auto-attest vs mutual attest vs chronon attest\nopencode Qwen3.6-27B-AWQ-BF16-INT4"
- [ ] Verify all work is complete in worktree and committed to `feat/terminology-normalization`
- [ ] Merge `feat/terminology-normalization` to `alpha`
- [ ] Cleanup worktree
  - [ ] Check that `_PLAN.md` has all but Cleanup checkboxes completed
  - [ ] Remove worktree directory
  - [ ] This is the last checkbox to be checked in my `_PLAN.md`
