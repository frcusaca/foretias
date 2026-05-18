# Scope Reduction — Implementation Plan

Corresponding spec: `SCOPE_REDUCTION_SPEC.md`

**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/SCOPE_REDUCTION_12121`
**Branch:** `feat/scope-reduction`

---

## Phase 0: Commit specs + plan, then branch

- [x](2026-05-18 07:50) Commit `SCOPE_REDUCTION_SPEC.md` and `SCOPE_REDUCTION_PLAN.md` to alpha
- [x](2026-05-18 07:50) Commit `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md` and `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` to alpha
- [x](2026-05-18 08:10) Create worktree `git worktree add -b feat/scope-reduction ${FULL_WORKTREE_PATH}`
- [x](2026-05-18 08:10) `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Remove Python/Java from Cargo workspace

- [x](2026-05-18 08:15) Edit `p2p/Cargo.toml`: remove `foretias-python` and `foretias-java` from workspace members
- [x](2026-05-18 08:30) Verify workspace still compiles: `cd p2p && cargo build --workspace`
- [x](2026-05-18 08:35) Verify workspace tests pass: `cd p2p && cargo test --workspace`

---

## Phase 2: Update build scripts and documentation

- [x](2026-05-18 08:40) Update `AGENTS.md` build/test commands: remove Python test references, remove Java build steps
- [x](2026-05-18 08:40) Update any top-level Makefile or build scripts that reference `foretias-python` or `foretias-java`
- [x](2026-05-18 08:40) Update `README.md` if it references Python/Java bindings

---

## Phase 3: Verify reduced scope

- [x](2026-05-18 09:15) `cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build` — C11 builds
- [x](2026-05-18 09:15) `cd p2p/core/build && ctest --output-on-failure` — C11 tests pass
- [x](2026-05-18 09:20) `cd p2p && cargo build -p foretias-core -p foretias-node` — Rust builds
- [x](2026-05-18 09:25) `cd p2p && cargo test -p foretias-core -p foretias-node` — Rust tests pass (actual: 170 unit + 8 integration = 290 total)
- [x](2026-05-18 09:25) Verify `foretias-python` and `foretias-java` directories still exist but are not built
- [x](2026-05-18 09:25) Verify `cargo build --workspace` does NOT build `foretias-python` or `foretias-java`

---

## Phase 4: Cleanup

- [x](2026-05-18 09:30) Verify all work is complete in `${FULL_WORKTREE_PATH}` and committed to `feat/scope-reduction`
- [x](2026-05-18 09:30) Merge `feat/scope-reduction` to alpha
- [x](2026-05-18 09:30) Cleanup `${FULL_WORKTREE_PATH}`
- [x](2026-05-18 09:30) Check that `SCOPE_REDUCTION_PLAN.md` has all but Cleanup checkboxes completed
- [x](2026-05-18 09:30) This is the last checkbox to be checked in `SCOPE_REDUCTION_PLAN.md`
