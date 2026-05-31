# CODE_QUALITY_TOOLING_PLAN.md

**Plan: Code Quality Tooling — Clippy, cargo-geiger, and Formatting Standards**
**Paired Spec: CODE_QUALITY_TOOLING_SPEC.md**
**Status: PROPOSED**
**Date: 2026-05-31**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/CODE_QUALITY_TOOLING_####**
**BRANCH_NAME=tooling/code-quality**

---

## Phase 0: Worktree Setup

- [ ] Create worktree `git worktree add -b ${BRANCH_NAME} ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path
- [ ] Verify baseline: `cargo build --workspace` and `cargo test --workspace` pass

---

## Phase 1: Create Configuration Files

### 1a. Create clippy.toml

- [ ] Create `p2p/clippy.toml` with project-specific configuration:
  - `warn-on-unnecessary-operation = true`
  - `unused-crate-dependencies-deny = true`
  - `avoid-breaking-exported-api = true`
  - `too-many-lines-threshold = 100`
  - `too-many-arguments-threshold = 7`
- [ ] Verify: `cargo clippy --workspace --all-targets` runs with new config

### 1b. Create rustfmt.toml

- [ ] Create `p2p/rustfmt.toml` with formatting rules:
  - `max_width = 100`
  - `tab_spaces = 4`
  - `edition = "2021"`
  - `use_field_init_shorthand = true`
  - `use_try_shorthand = true`
- [ ] Verify: `cargo fmt --check` passes (no formatting changes needed)

### 1c. Create .editorconfig

- [ ] Create `.editorconfig` at project root:
  - Default: 4-space indent, LF line endings, UTF-8
  - Markdown: preserve trailing whitespace
  - Makefile: tab indentation
- [ ] Verify: No whitespace changes in existing files

---

## Phase 2: Validate Clippy Baseline

### 2a. Run Clippy with Default Lints

- [ ] `cargo clippy --workspace --all-targets 2>&1 | tee /tmp/clippy-output.txt`
- [ ] Count warnings: `grep -c "^warning" /tmp/clippy-output.txt`
- [ ] If warnings > 0, document each warning with file/line

### 2b. Run Clippy with Pedantic Lints (Informational)

- [ ] `cargo clippy --workspace --all-targets -- -W clippy::pedantic 2>&1 | tee /tmp/clippy-pedantic.txt`
- [ ] Count pedantic warnings (informational only, not blocking)
- [ ] Identify top 5 most valuable pedantic lints for Foretias

### 2c. Document Baseline

- [ ] Create `docs/security/clippy-baseline.md` with:
  - Date of audit
  - Number of warnings per category
  - Any suppressions added (with justification)

---

## Phase 3: cargo-geiger Baseline Report

### 3a. Install cargo-geiger

- [ ] `cargo install cargo-geiger`
- [ ] Verify: `cargo geiger --version`

### 3b. Generate Baseline Report

- [ ] `cd p2p && cargo geiger --format markdown > /tmp/geiger-report.md`
- [ ] Copy report to `docs/security/cargo-geiger-baseline.md`

### 3c. Analyze Findings

- [ ] Identify dependencies with highest unsafe code usage
- [ ] Document any concerning findings (e.g., unexpected unsafe in transitive deps)
- [ ] Note: Foretias's own `core-engine` unsafe is expected (C11 FFI)

---

## Phase 4: CI Integration (Optional)

### 4a. Add Clippy Check to CI

- [ ] Create/update `.github/workflows/clippy.yml`:
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
- [ ] Verify CI passes on current codebase

### 4b. Add Format Check to CI

- [ ] Create/update `.github/workflows/rustfmt.yml`:
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
- [ ] Verify CI passes on current codebase

---

## Phase 5: Final Verification

### 5a. Full Test Suite

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy --workspace --all-targets` — zero warnings
- [ ] `cargo fmt --check` — no formatting changes needed

### 5b. Documentation Update

- [ ] Update `AGENTS.md` Development Tools section to mention:
  - Clippy configuration location
  - rustfmt configuration location
  - cargo-geiger baseline location
- [ ] Add note about running `cargo clippy` before commits

### 5c. Checkpoint Commit

- [ ] Stage all new files: `clippy.toml`, `rustfmt.toml`, `.editorconfig`, docs
- [ ] Commit: "Add code quality tooling configuration (clippy, rustfmt, editorconfig)"
- [ ] Verify: CI passes on new commit

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

## Notes

- This spec only adds configuration and documentation — no code changes
- Phase 2 (fixing clippy warnings) is a separate task if warnings are found
- cargo-geiger is informational only — no automated enforcement
- CI integration is optional and can be added later
