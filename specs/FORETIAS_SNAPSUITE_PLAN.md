# FORETIAS_SNAPSUITE_PLAN.md

## FORETIAS_SNAPSUITE — Cryptographically Signed Snapshot Test Integration

Paired with `FORETIAS_SNAPSUITE_SPEC.md`.

---

- [x] Create worktree `git worktree add -b spec/snapsuite ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_4783`
      (2026-05-21 14:31)
- [x] `cd ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_4783`; reset current session work directory to be the full worktree path.
      (2026-05-21 14:31)

### Phase 1: Signature Module (core-engine)

- [ ] Create `p2p/core-engine/src/snapshot_signature.rs`
  - Port `derive_keypair`, `sign_content`, `verify_signature` from foolish-rust
  - Change salt to `"foretias:snapsuite-sig:v1"`
  - Port `canonicalize_block`, `sign_snapshot`, `verify_snapshot`, `parse_snapshot_footer`
  - Rename `foolish_sig` → `input_sig`, `hs_sig` → `result_sig` throughout
  - Port all unit tests from foolish-rust `signature.rs`
  - Run `cargo test -p foretias-core -- snapshot_signature` to verify

- [ ] Add `argon2 = "0.5"` to `[dev-dependencies]` in `p2p/core-engine/Cargo.toml`

- [ ] Re-export in `p2p/core-engine/src/lib.rs`:
  ```rust
  pub mod snapshot_signature;
  pub mod snapshot_suite;
  ```

### Phase 2: Snapshot Suite (core-engine)

- [ ] Create `p2p/core-engine/src/snapshot_suite.rs`
  - Adapt `SnapshotSuite` — remove `input_dir`, keep `approved_dir`
  - Adapt `Evaluator` trait — `evaluate(&self, test_name: &str) -> Result<String, String>`
  - Remove file-based discovery (`discover()`, `input_names()`)
  - Keep `get_missing_snapshots()` / `get_missing_inputs()` for approved-side hygiene
  - Remove `evaluate_all()` (no file-based parallel evaluation needed)
  - Add `compare(&self, test_name: &str, actual: &str) -> Result<(), TestFailure>` — compare against approved snapshot
  - Keep `SnapshotSuiteError` and `TestFailure` enums

- [ ] Run `cargo check -p foretias-core` to verify compilation

### Phase 3: Test Harness (foretias-server)

- [ ] Add `insta` to `[dev-dependencies]` in `p2p/foretias-server/Cargo.toml`

- [ ] Create `p2p/foretias-server/tests/snapshot_tests.rs`
  - Import `SnapshotSuite`, `Evaluator` from `foretias_core`
  - Define `ServerStampEvaluator` — wraps `TimeFamilyServer` stamp/verify operations
  - Implement first test: `server_stamp_verify_fixed_seed`
    - Create server with short chronon period
    - Stamp "hello world"
    - Verify the stamp
    - Format output (INPUT + RESULT + COMMENTS blocks)
    - Sign with `sign_snapshot("", input, result, comments)`
    - Compare against approved snapshot via `suite.compare()`
    - Mark `#[ignore]` with clear documentation

- [ ] Create `p2p/foretias-server/snapshot_tests/approved/` directory

- [ ] Generate initial approved snapshot:
  ```bash
  cd p2p && INSTA_UPDATE=always cargo test -p foretias-server --test snapshot_tests -- server_stamp_verify_fixed_seed --ignored
  ```

- [ ] Verify the generated `.snap` file contains a valid SIGNATURES footer
  - Write a quick verification test that parses the footer and verifies all three progressive signatures

- [ ] Run `cargo test -p foretias-server --test snapshot_tests` and confirm the test fails on re-run (non-deterministic output from C11 RNG)

### Phase 4: Verification & Cleanup

- [ ] Run `cargo clippy -p foretias-core -p foretias-server` — clean on new code
- [ ] Run `cargo test -p foretias-core -- snapshot_signature` — all signature tests pass
- [ ] Run `cargo test -p foretias-core` — no regressions
- [ ] Verify all work is complete in ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_19283 and committed to spec/snapsuite
- [ ] Merge spec/snapsuite to alpha
- [ ] Cleanup:
  - [ ] Remove worktree `git worktree remove ${HOME}/tmp/foretias-worktrees/FORETIAS_SNAPSUITE_19283`
  - [ ] Sanity check: `FORETIAS_SNAPSUITE_PLAN.md` has all but Cleanup checkboxes completed
  - [ ] This is the last checkbox to be checked in this PLAN.md
