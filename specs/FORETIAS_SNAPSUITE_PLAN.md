# FORETIAS_SNAPSUITE_PLAN.md

## FORETIAS_SNAPSUITE — Cryptographically Signed Snapshot Test Integration

Paired with `FORETIAS_SNAPSUITE_SPEC.md`.

---

- [x] Create worktree `git worktree add -b spec/snapsuite ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_4783`
      (2026-05-21 14:31)
- [x] `cd ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_4783`; reset current session work directory to be the full worktree path.
      (2026-05-21 14:31)

### Phase 1: Signature Module (core-engine)

- [x] Create `p2p/core-engine/src/snapshot_signature.rs`
      (2026-05-21 15:00)
- [x] Add `argon2 = "0.5"` to `[dev-dependencies]` in `p2p/core-engine/Cargo.toml`
      (2026-05-21 15:00)
- [x] Re-export in `p2p/core-engine/src/lib.rs`:
      (2026-05-21 15:00)

### Phase 2: Snapshot Suite (core-engine)

- [x] Create `p2p/core-engine/src/snapshot_suite.rs`
      (2026-05-21 15:00)
- [x] Run `cargo check -p foretias-core` to verify compilation
      (2026-05-21 15:00)

### Phase 3: Test Harness (foretias-server)

- [x] Add `insta` to `[dev-dependencies]` in `p2p/foretias-server/Cargo.toml`
      (2026-05-21 15:00)
- [x] Create `p2p/foretias-server/tests/snapshot_tests.rs`
      (2026-05-21 15:00)
- [x] Create `p2p/foretias-server/snapshot_tests/approved/` directory
      (2026-05-21 15:00)
- [x] Generate initial approved snapshot
      (2026-05-21 15:00)
- [x] Verify the generated `.snap` file contains a valid SIGNATURES footer
      (2026-05-21 15:00)
- [x] Run `cargo test -p foretias-server --test snapshot_tests` and confirm the test fails on re-run (non-deterministic output from C11 RNG)
      (2026-05-21 15:00)

### Phase 4: Verification & Cleanup

- [x] Run `cargo clippy -p foretias-core -p foretias-server` — clean on new code
      (2026-05-21 15:05)
- [x] Run `cargo test -p foretias-core -- snapshot_signature` — all signature tests pass (26 passed)
      (2026-05-21 15:05)
- [x] Run `cargo test -p foretias-core` — no regressions
      (2026-05-21 15:05)
- [x] Verify all work is complete and committed to spec/snapsuite (commit 4c3d98b)
      (2026-05-21 15:05)
- [x] Merge spec/snapsuite to alpha (fast-forward)
      (2026-05-21 15:05)
- [x] Cleanup:
  - [x] Remove worktree `git worktree remove --force ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_4783`
        (2026-05-21 15:06)
  - [x] Sanity check: `FORETIAS_SNAPSUITE_PLAN.md` has all but Cleanup checkboxes completed
        (2026-05-21 15:06)
  - [x] This is the last checkbox to be checked in this PLAN.md
        (2026-05-21 15:06)
