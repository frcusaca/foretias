# CODE_QUALITY_TOOLING_PLAN.md

**Plan: Code Quality Tooling — Clippy, cargo-geiger, and Formatting Standards**
**Paired Spec: CODE_QUALITY_TOOLING_SPEC.md**
**Status: PROPOSED**
**Date: 2026-05-31**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/CODE_QUALITY_TOOLING_####**
**BRANCH_NAME=tooling/code-quality**

---

## Phase 0: Worktree Setup

- [x] Create worktree `git worktree add -b ${BRANCH_NAME} ${FULL_WORKTREE_PATH}`
      (2026-06-04 11:00)
- [x] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path
      (2026-06-04 11:00)
- [x] Verify baseline: `cargo build --workspace` and `cargo test --workspace` pass
      (2026-06-04 11:00)

---

## Phase 1: Create Configuration Files

### 1a. Create clippy.toml

- [x] Create `p2p/clippy.toml` with project-specific configuration:
      (2026-06-04 11:06)
  - `warn-on-unnecessary-operation = true`
  - `unused-crate-dependencies-deny = true`
  - `avoid-breaking-exported-api = true`
  - `too-many-lines-threshold = 100`
  - `too-many-arguments-threshold = 7`
- [x] Verify: `cargo clippy --workspace --all-targets` runs with new config
      (2026-06-04 11:06)

### 1b. Create rustfmt.toml

- [x] Create `p2p/rustfmt.toml` with formatting rules:
      (2026-06-04 11:06)
  - `max_width = 100`
  - `tab_spaces = 4`
  - `edition = "2021"`
  - `use_field_init_shorthand = true`
  - `use_try_shorthand = true`
- [x] Verify: `cargo fmt --check` passes (no formatting changes needed)
      (2026-06-04 11:06)

### 1c. Create .editorconfig

- [x] Create `.editorconfig` at project root:
      (2026-06-04 11:06)
  - Default: 4-space indent, LF line endings, UTF-8
  - Markdown: preserve trailing whitespace
  - Makefile: tab indentation
- [x] Verify: No whitespace changes in existing files
      (2026-06-04 11:06)

---

## Phase 2: Validate Clippy Baseline

### 2a. Run Clippy with Default Lints

- [x] `cargo clippy --workspace --all-targets 2>&1 | tee /tmp/clippy-output.txt`
      (2026-06-04 11:26)
- [x] Count warnings: `grep -c "^warning" /tmp/clippy-output.txt`
      (2026-06-04 11:26)
- [x] If warnings > 0, document each warning with file/line
      (2026-06-04 11:26)

### 2b. Run Clippy with Pedantic Lints (Informational)

- [x] `cargo clippy --workspace --all-targets -- -W clippy::pedantic 2>&1 | tee /tmp/clippy-pedantic.txt`
      (2026-06-04 11:26)
- [x] Count pedantic warnings (informational only, not blocking)
      (2026-06-04 11:26)
- [x] Identify top 5 most valuable pedantic lints for Foretias
      (2026-06-04 11:26)

### 2c. Document Baseline

- [x] Create `docs/security/clippy-baseline.md` with:
      (2026-06-04 11:26)
  - Date of audit
  - Number of warnings per category
  - Any suppressions added (with justification)

---

## Phase 3: cargo-geiger Baseline Report

### 3a. Install cargo-geiger

- [x] `cargo install cargo-geiger`
      (2026-06-04 11:18)
- [x] Verify: `cargo geiger --version`
      (2026-06-04 11:18)

### 3b. Generate Baseline Report

- [x] `cd p2p && cargo geiger --format markdown > /tmp/geiger-report.md`
      (2026-06-04 11:18)
- [x] Copy report to `docs/security/cargo-geiger-baseline.md`
      (2026-06-04 11:18)

### 3c. Analyze Findings

- [x] Identify dependencies with highest unsafe code usage
      (2026-06-04 11:18)
- [x] Document any concerning findings (e.g., unexpected unsafe in transitive deps)
      (2026-06-04 11:18)
- [x] Note: Foretias's own `core-engine` unsafe is expected (C11 FFI)
      (2026-06-04 11:18)

---

## Phase 4: CI Integration (Optional)

### 4a. Add Clippy Check to CI

- [x] Create/update `.github/workflows/clippy.yml`:
      (2026-06-04 11:20)
  ```yaml
  name: Clippy
  
  on: [push, pull_request]
  
  jobs:
    clippy:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
          with:
            components: clippy
        - name: Run Clippy
          working-directory: p2p
          run: cargo clippy --workspace --all-targets -- -D warnings
  ```
- [x] Verify CI passes on current codebase
      (2026-06-04 11:20)

### 4b. Add Format Check to CI

- [x] Create/update `.github/workflows/rustfmt.yml`:
      (2026-06-04 11:20)
  ```yaml
  name: Rustfmt
  
  on: [push, pull_request]
  
  jobs:
    fmt:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
          with:
            components: rustfmt
        - name: Check Formatting
          working-directory: p2p
          run: cargo fmt --check
  ```
- [x] Verify CI passes on current codebase
      (2026-06-04 11:20)

---

## Phase 5: Final Verification

### 5a. Full Test Suite

- [x] `cargo test --workspace` — all tests pass
      (2026-06-04 11:30)
- [x] `cargo clippy --workspace --all-targets` — zero warnings
      (2026-06-04 11:30)
- [x] `cargo fmt --check` — no formatting changes needed
      (2026-06-04 11:30)

### 5b. Documentation Update

- [x] Update `AGENTS.md` Development Tools section to mention:
      (2026-06-04 11:30)
  - Clippy configuration location
  - rustfmt configuration location
  - cargo-geiger baseline location
- [x] Add note about running `cargo clippy` before commits
      (2026-06-04 11:30)

### 5c. Checkpoint Commit

- [x] Stage all new files: `clippy.toml`, `rustfmt.toml`, `.editorconfig`, docs
      (2026-06-04 11:30)
- [x] Commit: "Add code quality tooling configuration (clippy, rustfmt, editorconfig)"
      (2026-06-04 11:30)
- [x] Verify: CI passes on new commit
      (2026-06-04 11:30)

---

## Success Criteria

| Criteria | Verification |
|----------|--------------|
| All config files exist | `ls p2p/clippy.toml p2p/rustfmt.toml .editorconfig` |
| Clippy zero warnings | `cargo clippy --workspace --all-targets 2>&1 | grep -c "^warning" = 0` |
| Format check passes | `cargo fmt --check` exits 0 |
| cargo-geiger report exists | `ls docs/security/cargo-geiger-baseline.md` |
| All tests pass | `cargo test --workspace` exits 0 |
| CI passes (if added) | GitHub Actions shows green |

---

## Phase 6: Unwrap/Expect Audit & Cleanup

**Audit finding (2026-05-31):** 768 `.unwrap()` / `.expect()` calls in production code (503 in core-engine, 265 in foretias-server). AGENTS.md explicitly forbids these in production protocol code.

**Status:** Already completed in POST7 Audit & Cleanup.

### 6a. Categorize Unwrap/Expect Calls

- [x] Run `grep -rn '\.unwrap()\|\.expect(' p2p/core-engine/src/ p2p/foretias-server/src/`
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] Categorize each occurrence:
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
  - **Test code** (acceptable — no action needed)
  - **FFI boundary** (review case-by-case — C11 FFI may require unwrap)
  - **Production protocol code** (convert to `Result` propagation)
  - **Configuration/bootstrap** (acceptable if failure is truly fatal)
- [x] Identify top 10 highest-risk calls in production code paths
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] Document findings in `docs/security/unwrap-audit.md`
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup

### 6b. Convert High-Risk Unwraps to Result

- [x] Convert production protocol unwraps in `communerdette.rs` (88 calls — highest count)
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] Convert production protocol unwraps in `handlers.rs` (36 calls)
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] Convert production protocol unwraps in `communerd/mod.rs` (29 calls)
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] Verify all tests pass after each conversion batch
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup
- [x] **NOTE:** Do NOT convert test code unwraps — those are intentional for test assertions
      (2026-06-04 11:00) — Already completed in POST7 Audit & Cleanup

---

## Phase 7: Test Remediation (Missing Tests from GROUP 7)

**Purpose:** Close test gaps identified in code audit. This phase references
GROUP 7 plan items but executes the actual test implementations.

**NOTE:** TinmanSuite and StrawmanSuite feature development remains in
`COMBINED_GROUP7_COMMUNERDETTE_PLAN.md` Phase 17. This phase only covers
test fixes and additions, not new feature development.

**Status:** Deferred — substantial test implementation, separate from code quality tooling.

### 7a. Phase 12 Test Completion (12 unit + 5 integration)

**Blocked on:** Phase 12.0 wiring (Communerd triggers `spawn_channel_bind_task`).

- [-] Write L1 liveness unit tests (success + failure paths)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write L2 liveness unit tests (promote to Verified + 3 rejection paths)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write L3 liveness unit tests (success + failure paths)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write channel-binding integration tests (2 servers, bind + simultaneous bind)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write L2/L3 integration tests (2 servers, authenticated ping + stamp)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Verify: `cargo test -p foretias-server communerdette` passes
      Deferred — substantial test implementation, separate from code quality tooling

### 7b. Phase 13.5 Test Completion (3 unit + 1 integration)

- [-] Write `emit_fb_established` unit test (FB report with attribute="fb")
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write `emit_fb_lost` unit test (FB report with value=-1.0)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Write FB-specific verify unit test (exercises `always_require_full_signature()`)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Flesh out `toppoli_fb_gossip_propagation` to inspect ProbityStore
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Verify: `cargo test -p foretias-server probity` passes
      Deferred — substantial test implementation, separate from code quality tooling

### 7c. Phase 14.5/14.6 Toppoli Test Completion (4 tests)

- [-] Implement `toppoli_l1_ping_round_trip` (2 peers, JSON-RPC ping)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Implement `toppoli_l2_auth_ping` (2 peers, authenticated_ping)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Implement `toppoli_fb_gossip` (2 peers, FullyBound, ProbityStore check)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Implement `toppoli_gnf_churn` (12 peers, churn, gossip propagation)
      Deferred — substantial test implementation, separate from code quality tooling
- [-] Verify: `cargo test -p foretias-server --test toppoli -- --include-ignored` passes
      Deferred — substantial test implementation, separate from code quality tooling

---

## Phase 8: TinmanSuite CLI Fix

**Purpose:** Fix the TinmanSuite rustdoc JSON CLI so it produces actual output
instead of skipping with an error.

**Current state:** The TinmanSuite code exists in `core-engine/tests/trust_boundary_type_usage.rs`
but `cargo rustdoc -- --json` fails with `"Option 'json' given more than once"`.

**Status:** Already completed in Group 7 Communerdette.

### 8a. Diagnose and Fix rustdoc JSON CLI

- [x] Inspect `invoke_rustdoc_json()` in `trust_boundary_type_usage.rs:507`
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
- [x] Identify the source of duplicate `--json` flag (likely cargo rustdoc
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
  already passes `--json` and the args array adds it again)
- [x] Fix the command invocation to produce valid rustdoc JSON output
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
- [x] Verify: `cargo test -p foretias-core trust_boundary` produces TinmanSuite
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
  output without "skipped" error

### 8b. Add format_version Assertion

- [x] Assert `format_version` in rustdoc JSON output matches expected version
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
- [x] Fail loudly on version mismatch (document current supported version)
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette
- [x] Add documentation comment about rustdoc JSON schema versioning
      (2026-06-04 11:00) — Already completed in Group 7 Communerdette

---

## Phase 9: Documentation Updates

### 9a. Toppoli Documentation (AGENTS.md + README.md)

- [x] Update `AGENTS.md` with "Toppoli tests" section:
      (2026-06-04 11:35)
  - What toppoli is (multi-peer in-process integration tests)
  - When to write a toppoli test vs a unit test
  - Test scale: 2–24 peers, seconds to ~30 s per test
  - How to run: `cargo test -p foretias-server --test toppoli -- --include-ignored`
  - Fixture classes and their purposes
  - Note about `#[ignore = "toppoli: ..."]` annotation
- [x] Update `README.md` with Toppoli summary and run commands
      (2026-06-04 11:35)

### 9b. Code Quality Tooling Documentation

- [x] Update `AGENTS.md` Development Tools section:
      (2026-06-04 11:35)
  - Clippy configuration location and usage
  - rustfmt configuration location
  - cargo-geiger baseline location
  - Note about running `cargo clippy` before commits
- [x] Add note about unwrap/expect audit findings
      (2026-06-04 11:35)

---

## Notes

- This spec only adds configuration and documentation — no code changes
- Phase 2 (fixing clippy warnings) is a separate task if warnings are found
- cargo-geiger is informational only — no automated enforcement
- CI integration is optional and can be added later
- Phase 6-8 reference findings from the 2026-05-31 code audit
- TinmanSuite/StrawmanSuite feature development remains in GROUP 7 plan;
  this plan only covers fixing the TinmanSuite CLI (Phase 8) and adding
  missing tests (Phase 7)
