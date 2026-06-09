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

- [ ] Create worktree `git worktree add -b encapsulation-review-fixes ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [ ] Verify baseline: `cargo test --workspace` passes
- [ ] Run `cargo clippy --workspace --all-targets` — confirm zero warnings

### Wave 1: Fix 1 — Setters Clear Signature (HIGH)

- [ ] Update `PeerRegistrationRecord::set_multiaddr()` to clear signature
- [ ] Update `PeerRegistrationRecord::set_peer_id()` to clear signature
- [ ] Write test: `test_set_multiaddr_clears_signature` — set signature, call setter, verify empty
- [ ] Write test: `test_set_peer_id_clears_signature` — set signature, call setter, verify empty
- [ ] Write test: `test_set_multiaddr_then_resign` — set, clear, re-sign, verify valid
- [ ] Verify: `cargo test -p foretias-server --lib` passes
- [ ] Commit: "PeerRegistrationRecord: setters clear signature on canonical-payload mutation"

### Wave 2: Fix 2 — Internal Signing Flow (HIGH)

- [ ] Replace struct literal access in `communerd/mod.rs:1128-1143` with `set_signature()`
- [ ] Verify: `cargo test -p foretias-server --lib` passes
- [ ] Commit: "communerd: use set_signature() in signing flow"

### Wave 3: Fix 3 — Use is_genesis() (MEDIUM)

- [ ] Replace `*unproc.inner().chronon_number() == 1` with `unproc.inner().is_genesis()` in `handlers.rs:835`
- [ ] Replace same pattern in `handlers.rs:1311`
- [ ] Verify: `cargo test -p foretias-server --lib` passes
- [ ] Commit: "handlers: use is_genesis() for genesis checks"

### Wave 4: Fix 5 — Calendar Setter Validation (MEDIUM)

- [ ] Add `debug_assert!` to `Calendar::set_tbid()`
- [ ] Add `debug_assert!` to `Calendar::set_tbn()`
- [ ] Write test: `test_set_tbid_panics_if_already_set` (debug mode only)
- [ ] Write test: `test_set_tbn_panics_if_already_set` (debug mode only)
- [ ] Verify: `cargo test -p foretias-core --lib` passes
- [ ] Commit: "Calendar: add debug_assert to setters"

### Wave 5: Fix 6 — Binary Search in tick_at() (MEDIUM)

- [ ] Replace `iter().find()` with `binary_search_by()` in `Calendar::tick_at()`
- [ ] Write test: `test_tick_at_finds_existing` — insert ticks, verify tick_at returns correct one
- [ ] Write test: `test_tick_at_missing_returns_none` — verify None for non-existent chronon
- [ ] Write test: `test_tick_at_first_and_last` — verify boundary conditions
- [ ] Write test: `test_tick_at_out_of_order_insertion` — verify binary search works after append
- [ ] Verify: `cargo test -p foretias-core --lib` passes
- [ ] Commit: "Calendar: use binary search in tick_at()"

### Wave 6: Fix 7 — Atomic Ordering (MEDIUM)

- [ ] Change `Ordering::SeqCst` to `Ordering::Acquire` for loads in `LivenessCycleFlags`
- [ ] Change `Ordering::SeqCst` to `Ordering::Release` for stores in `LivenessCycleFlags`
- [ ] Add comment explaining why `Acquire`/`Release` is sufficient
- [ ] Verify: `cargo test -p foretias-server --lib` passes
- [ ] Commit: "LivenessCycleFlags: use Acquire/Release ordering"

### Wave 7: Fix 8 + Fix 9 — Cleanup (LOW)

- [ ] Remove unnecessary clone in `server/mod.rs:94` (`set_tbn(&tbn.clone())` → `set_tbn(&tbn)`)
- [ ] Add comment to `set_binding_rejected` explaining it's for test infrastructure
- [ ] Verify: `cargo clippy --workspace --all-targets` — zero warnings
- [ ] Commit: "Cleanup: remove unnecessary clone, document dead code"

### Wave 8: Final Verification

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo fmt --check` — no formatting changes needed
- [ ] Merge to alpha
