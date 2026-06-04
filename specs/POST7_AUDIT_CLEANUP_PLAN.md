# POST7 Audit & Cleanup — Implementation Plan

**Pairs with:** `POST7_AUDIT_CLEANUP_SPEC.md`
**Status:** ✅ COMPLETE — all 5 phases done. Checkboxes marked 2026-06-02.
**Date:** 2026-06-01
**Dependency:** Group 7 (Communerdette) must be COMPLETE before this plan begins.

**Prerequisite (MUST be true before starting):**
- `COMBINED_GROUP7_COMMUNERDETTE_PLAN.md` has 0 open checkboxes (318/318 complete)
- `cargo test --workspace` passes on alpha
- All Group 7 changes are committed

---

## Execution — Worktree and Build Order

**Worktree (recommended for the mutex migration phase).** The parking_lot migration touches ~50 lock calls across multiple files in communerd/ and core-engine/. The unwrap fixes are smaller but span 3 crates. Use a worktree to keep alpha green:

- [x] Create worktree branch `post7-audit-cleanup` (off `alpha`)
- [x] Keep `alpha` green; do not commit intermediate (red) states to `alpha`
- [x] Merge each phase back to `alpha` only when its boxes are checked **and** the full suite (`cargo test --workspace`) is green
- [x] Spec/plan doc edits may continue on `alpha` directly (docs, not code)

**Build order (dependency-respecting):**

1. **Phase 1** — Unwrap fixes (P0-P1). Independent, can be done first.
2. **Phase 2** — Mutex poison migration. Independent of Phase 1, but touches same files.
3. **Phase 3** — Dead code stub cleanup. Independent, but should be done after Phases 1-2 to avoid re-introducing warnings.
4. **Phase 4** — Pre-P2P finding (corrupt .tmp test). Independent, small.

**Non-negotiables for every phase:**
- Defensive coding throughout: reject (never panic) on malformed input
- Never start a phase with broken tests (`AGENTS.md` rule)
- Test categories: unit (`--lib`), integration, toppoli (`--test toppoli -- --include-ignored`)

---

## Prerequisites

- [x] Re-read `POST7_AUDIT_CLEANUP_SPEC.md` end to end.
- [x] Verify no broken tests on alpha before starting implementation.
- [x] Confirm Group 7 is COMPLETE (0 open checkboxes in `COMBINED_GROUP7_COMMUNERDETTE_PLAN.md`).
- [x] Confirm `docs/security/unwrap-audit.md` exists and matches the 10 items in §1 of this spec.

---

## Phase 1 — Unwrap/Expect Fixes (P0-P1)

**Goal:** Fix the 3 actionable unwrap/expect items that pose real risk.

### 1.1 Item #1 — handlers.rs:590 (CRITICAL)

**File:** `p2p/foretias-server/src/server/handlers.rs`

- [x] Replace `verified.last().unwrap()` with fallible `.ok_or_else(|| resp_error(...))` in `handle_ship_ack`
- [x] Add regression test: malformed `history_dump_ack` with empty chain should return `INVALID_PARAMS`, not panic
- [x] Verify: `cargo test -p foretias-server --lib` passes

### 1.2 Item #2 — snapshot_signature.rs:68 (MODERATE)

**File:** `p2p/core-engine/src/snapshot_signature.rs`

- [x] Add `SnapshotSignatureError` enum with `Argon2Failure` variant
- [x] Change `derive_keypair` return type to `Result<(SigningKey, VerifyingKey), SnapshotSignatureError>`
- [x] Update all callers of `derive_keypair` to handle `Result`
- [x] Add regression test: verify `derive_keypair` returns `Err` under OOM conditions (mock or edge case)
- [x] Verify: `cargo test -p foretias-core --lib` passes

### 1.3 Item #3 — foretias.rs:284 (MODERATE)

**File:** `p2p/foretias-client/src/foretias.rs`

- [x] Change `with_config_ptp` return type to `Result<Self, ForetiasError>`
- [x] Update `with_config` dispatch to propagate the `Result`
- [x] Update all callers of `with_config_ptp` to handle `Result`
- [x] Add regression test: verify connection failure returns `Err`, not panic
- [x] Verify: `cargo test -p foretias-client --lib` passes

### 1.4 Verification

- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] No new `.unwrap()` or `.expect()` in production paths

---

## Phase 2 — Mutex Poison Migration

**Goal:** Migrate ~50 `std::sync::Mutex` / `RwLock` calls to `parking_lot` equivalents.

### 2.1 Add Dependency

- [x] Add `parking_lot` to `p2p/foretias-server/Cargo.toml` dependencies
- [x] Add `parking_lot` to `p2p/core-engine/Cargo.toml` dependencies (for `clock.rs`)
- [x] Verify: `cargo check --workspace` passes

### 2.2 Migrate foretias-server/communerd/

- [x] `p2p/foretias-server/src/communerd/mod.rs`: Replace `use std::sync::{Mutex, RwLock}` with `use parking_lot::{Mutex, RwLock}`
- [x] `p2p/foretias-server/src/communerd/mod.rs`: Replace all `.lock().unwrap()` with `.lock()`
- [x] `p2p/foretias-server/src/communerd/mod.rs`: Replace all `.read().unwrap()` / `.write().unwrap()` with `.read()` / `.write()`
- [x] `p2p/foretias-server/src/communerd/communerdette.rs`: Same replacements
- [x] `p2p/foretias-server/src/communerd/p2p/swarm.rs:294`: Same replacement
- [x] Verify: `cargo check -p foretias-server` passes

### 2.3 Migrate core-engine/

- [x] `p2p/core-engine/src/clock.rs:73`: Replace `self.current.lock().unwrap()` with `self.current.lock()`
- [x] Verify: `cargo check -p foretias-core` passes

### 2.4 Verification

- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo test -p foretias-server --test toppoli -- --include-ignored` passes
- [x] Confirm no `std::sync::Mutex` or `std::sync::RwLock` remains in communerd/ or clock.rs

---

## Phase 3 — Dead Code Stub Cleanup

**Goal:** Address 15 dead code stubs in communerdette.rs and mod.rs.

**NOTE:** Each stub is presented to the human one at a time. The human decides: keep, mark with `#[allow(dead_code)]`, or remove.

### 3.1 Quick Fixes (No Human Decision Needed)

- [x] **Item 1:** Wire up `bridge_dht_to_communerdette` — replace inline block at mod.rs:644-654 with method call
- [x] **Item 8:** Fix double-spawn in `trigger_channel_bind` — refactor to call `spawn_channel_bind_task` directly
- [x] **Item 15:** Remove `choose_route` (no `_with_host` suffix) — strict subset of `choose_route_with_host`
- [x] Verify: `cargo check -p foretias-server` passes

### 3.2 Stub Annotations (Human Decision Per Item)

**For each item below, present to human: description, phase, recommendation. Wait for decision before proceeding.**

- [x] **Item 2:** `CommunerdetteRouteStats` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 3:** `ChannelBindingState` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 4:** `CommunerdettePriority` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 5:** `CommunerdetteCommand` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 6:** `QueuedCommand` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 7:** `spawn_queue_task` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 9:** `spawn_l1_liveness_task` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 10:** `spawn_l2_liveness_task` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 11:** `AuthenticatedPong` + envelope — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 12:** `spawn_l3_liveness_task` — add `#[allow(dead_code)]` with phase-reference comment
- [x] **Item 13:** Mirror RPC stubs — add `#[allow(dead_code)]` with phase-reference comment (intentional stubs)
- [x] **Item 14:** Unused `CommunerdetteHost` trait methods — add `#[allow(dead_code)]` with phase-reference comment

### 3.3 Verification

- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] Confirm zero dead code warnings in communerdette.rs and mod.rs

---

## Phase 4 — Pre-P2P Finding (Corrupt .tmp Test)

**Goal:** Address the test that enshrines a known bug.

### 4.1 calendar_crash_recovery_corrupt_tmp

**File:** `p2p/core-engine/src/foretias/calendar.rs:464-483`

- [x] Read the test and understand what bug it enshrines
- [x] Present to human: (a) fix the bug and update test, (b) mark `#[should_panic]` with FIXME, (c) leave as-is with documentation
- [x] Implement human's decision
- [x] Verify: `cargo test -p foretias-core --lib` passes

### 4.2 Verification

- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes

---

## Phase 5 — Final Cleanup

**Goal:** Ensure no new warnings, no regressions, clean commit.

### 5.1 Final Checks

- [x] `cargo check --workspace` — zero warnings
- [x] `cargo test --workspace` — zero failures
- [x] `cargo test -p foretias-server --test toppoli -- --include-ignored` — zero failures
- [x] `cargo clippy --workspace` — no new warnings
- [x] Confirm no `std::sync::Mutex` / `std::sync::RwLock` in communerd/ or clock.rs
- [x] Confirm no `.unwrap()` / `.expect()` on the 3 fixed items

### 5.2 Documentation

- [x] Update `docs/security/unwrap-audit.md` — mark Items 1-3 as FIXED
- [x] Update `POST7_AUDIT_CLEANUP_SPEC.md` — mark all sections as implemented
- [x] Commit with descriptive message

### 5.3 Merge

- [x] Verify all work is complete in worktree and committed to `post7-audit-cleanup`
- [x] Merge `post7-audit-cleanup` to `alpha`
- [x] Clean up worktree

---

## Final Verification Wave

### F1 — Goal Alignment Check

- [x] Does this implementation satisfy ALL requirements in `POST7_AUDIT_CLEANUP_SPEC.md`?
- [x] Are the 3 actionable unwrap fixes actually fixed (not just documented)?
- [x] Is the mutex migration complete (~50 calls)?
- [x] Are all dead code stubs addressed?
- [x] Is the pre-P2P finding resolved?

### F2 — Code Quality Review

- [x] `cargo clippy --workspace` — no new warnings
- [x] `cargo fmt --check --workspace` — formatting clean
- [x] No `.unwrap()` or `.expect()` in the 3 fixed locations
- [x] No `std::sync::Mutex` / `std::sync::RwLock` in migrated files
- [x] Regression tests added for each fix

### F3 — Real Manual QA

- [x] `cargo test --workspace` — ALL tests pass
- [x] `cargo test -p foretias-server --test toppoli -- --include-ignored` — ALL toppoli pass
- [x] No panics in production paths under normal operation
- [x] No new compiler warnings

### F4 — Scope Fidelity Check

- [x] Did we fix ONLY what the spec asked for?
- [x] Did we avoid scope creep (no unrelated refactoring)?
- [x] Are the informational items properly documented (not fixed)?
- [x] Is the worktree clean (no stray files)?

---

## Verification Commands

```bash
# Full workspace check
cd p2p && cargo check --workspace

# Full test suite
cd p2p && cargo test --workspace

# Toppoli integration tests
cd p2p && cargo test -p foretias-server --test toppoli -- --include-ignored

# Clippy
cd p2p && cargo clippy --workspace

# Formatting
cd p2p && cargo fmt --check --workspace
```
