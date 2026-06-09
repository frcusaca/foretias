# ENCAPSULATION_REVIEW_FIXES_PLAN.md

**Plan: Encapsulation Review Fixes — OCR Findings**
**Paired Spec: ENCAPSULATION_REVIEW_FIXES_SPEC.md**
**Status: PROPOSED**
**Date: 2026-06-08**
**BRANCH_NAME: encapsulation-review-fixes**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/ENCAPSULATION_REVIEW_####**

---

## TODOs

### Wave 0: Setup

- [x] Create worktree `git worktree add -b encapsulation-review-fixes ${FULL_WORKTREE_PATH}`
- [x] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [x] Verify baseline: `cargo test --workspace` passes
- [x] Run `cargo clippy --workspace --all-targets` — confirm zero warnings

### Wave 1: Fix 1 — Setters Clear Signature (HIGH)

- [x] Update `PeerRegistrationRecord::set_multiaddr()` to clear signature
- [x] Update `PeerRegistrationRecord::set_peer_id()` to clear signature
- [x] Write test: `test_set_multiaddr_clears_signature` — set signature, call setter, verify empty
- [x] Write test: `test_set_peer_id_clears_signature` — set signature, call setter, verify empty
- [x] Write test: `test_set_multiaddr_then_resign` — set, clear, re-sign, verify valid
- [x] Verify: `cargo test -p foretias-server --lib` passes
- [x] Commit: "PeerRegistrationRecord: setters clear signature on canonical-payload mutation"

### Wave 2: Fix 2 — Internal Signing Flow (HIGH)

- [x] Replace struct literal access in `communerd/mod.rs:1128-1143` with `set_signature()`
- [x] Verify: `cargo test -p foretias-server --lib` passes
- [x] Commit: "communerd: use set_signature() in signing flow"

### Wave 3: Fix 3 — Use is_genesis() (MEDIUM)

- [x] Replace `*unproc.inner().chronon_number() == 1` with `unproc.inner().is_genesis()` in `handlers.rs:835`
- [x] Replace same pattern in `handlers.rs:1311`
- [x] Verify: `cargo test -p foretias-server --lib` passes
- [x] Commit: "handlers: use is_genesis() for genesis checks"

### Wave 4: Fix 5 — Calendar Setter Validation (MEDIUM)

- [x] Add `debug_assert!` to `Calendar::set_tbid()`
- [x] Add `debug_assert!` to `Calendar::set_tbn()`
- [x] Write test: `test_set_tbid_panics_if_already_set` (debug mode only)
- [x] Write test: `test_set_tbn_panics_if_already_set` (debug mode only)
- [x] Verify: `cargo test -p foretias-core --lib` passes
- [x] Commit: "Calendar: add debug_assert to setters"

### Wave 5: Fix 6 — Binary Search in tick_at() (MEDIUM)

- [x] Replace `iter().find()` with `binary_search_by()` in `Calendar::tick_at()`
- [x] Write test: `test_tick_at_finds_existing` — insert ticks, verify tick_at returns correct one
- [x] Write test: `test_tick_at_missing_returns_none` — verify None for non-existent chronon
- [x] Write test: `test_tick_at_first_and_last` — verify boundary conditions
- [x] Write test: `test_tick_at_out_of_order_insertion` — verify binary search works after append
- [x] Verify: `cargo test -p foretias-core --lib` passes
- [x] Commit: "Calendar: use binary search in tick_at()"

### Wave 6: Fix 7 — Atomic Ordering (MEDIUM)

- [x] Change `Ordering::SeqCst` to `Ordering::Acquire` for loads in `LivenessCycleFlags`
- [x] Change `Ordering::SeqCst` to `Ordering::Release` for stores in `LivenessCycleFlags`
- [x] Add comment explaining why `Acquire`/`Release` is sufficient
- [x] Verify: `cargo test -p foretias-server --lib` passes
- [x] Commit: "LivenessCycleFlags: use Acquire/Release ordering"

### Wave 7: Fix 8 + Fix 9 — Cleanup (LOW)

- [x] Remove unnecessary clone in `server/mod.rs:94` (`set_tbn(&tbn.clone())` → `set_tbn(&tbn)`)
- [x] Add comment to `set_binding_rejected` explaining it's for test infrastructure
- [x] Verify: `cargo clippy --workspace --all-targets` — zero warnings
- [x] Commit: "Cleanup: remove unnecessary clone, document dead code"

### Wave 8: Final Verification

- [x] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [x] `cargo test --workspace` — all tests pass
- [x] `cargo fmt --check` — no formatting changes needed
- [x] Merge to alpha
