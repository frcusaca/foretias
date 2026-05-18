# Scope Reduction — Implementation Plan

Corresponding spec: `SCOPE_REDUCTION_SPEC.md`

**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/SCOPE_REDUCTION_12121`
**Branch:** `feat/scope-reduction`

---

## Phase 0: Commit specs + plan, then branch

- [ ] Commit `SCOPE_REDUCTION_SPEC.md` and `SCOPE_REDUCTION_PLAN.md` to alpha
- [ ] Commit `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md` and `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` to alpha
- [ ] Create worktree `git worktree add -b feat/scope-reduction ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Remove Python/Java from Cargo workspace

- [ ] Edit `p2p/Cargo.toml`: remove `foretias-python` and `foretias-java` from workspace members
- [ ] Verify workspace still compiles: `cd p2p && cargo build --workspace`
- [ ] Verify workspace tests pass: `cd p2p && cargo test --workspace`

---

## Phase 2: Update build scripts and documentation

- [ ] Update `AGENTS.md` build/test commands: remove Python test references, remove Java build steps
- [ ] Update any top-level Makefile or build scripts that reference `foretias-python` or `foretias-java`
- [ ] Update `README.md` if it references Python/Java bindings

---

## Phase 3: Verify reduced scope

- [ ] `cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build` — C11 builds
- [ ] `cd p2p/core/build && ctest --output-on-failure` — C11 tests pass
- [ ] `cd p2p && cargo build -p foretias-core -p foretias-node` — Rust builds
- [ ] `cd p2p && cargo test -p foretias-core -p foretias-node` — Rust tests pass (expected: 104 unit + 16 integration)
- [ ] Verify `foretias-python` and `foretias-java` directories still exist but are not built
- [ ] Verify `cargo build --workspace` does NOT build `foretias-python` or `foretias-java`

---

## Phase 4: Cleanup

- [ ] Verify all work is complete in `${FULL_WORKTREE_PATH}` and committed to `feat/scope-reduction`
- [ ] Merge `feat/scope-reduction` to alpha
- [ ] Cleanup `${FULL_WORKTREE_PATH}`
- [ ] Check that `SCOPE_REDUCTION_PLAN.md` has all but Cleanup checkboxes completed
- [ ] This is the last checkbox to be checked in `SCOPE_REDUCTION_PLAN.md`
