# COMBINED_GROUP2_PLAN.md
# Workspace Hygiene Plan — Verification Only

**Date:** 2026-05-22
**Paired Spec:** `COMBINED_GROUP2_SPEC.md`
**Status:** ✅ COMPLETE — verification-only plan; all code changes already shipped

---

## Purpose

This plan exists for two reasons:
1. **Audit trail** — a record of how the Group 2 items were verified done.
2. **Per-agent pre-flight** — every agent working on Group 3/4/5 runs Phase A
   before starting their assigned work. If any check fails, the alpha branch
   itself is broken and must be fixed before any new work proceeds.

---

## Phase A — Per-Agent Pre-Flight Verification

**Run from worktree root before starting your assigned group's work.**

### A.1 Source hygiene

```bash
grep -rl "TickRecord" p2p/{core-engine,foretias-client,foretias-server}/src   # expect: 0
grep -rl "P2PConfig" p2p/                                                     # expect: 0
grep -rn "#\[allow(deprecated" p2p/                                           # expect: 0
grep -rn "#\[allow(dead_code" p2p/ | grep -v "core/bindings.rs\|types.rs"     # expect: 0 (bindgen excluded)
grep -rn "thread_rng" p2p/{core-engine,foretias-client,foretias-server}/src   # expect: 0
grep -rn "foretias-node" README.md HOWTO.md AGENTS.md                         # expect: 0
```

- [x] All six greps return zero matches → proceed
- [x] Any grep returns matches → STOP; escalate to coordinator

### A.2 Build is clean

```bash
cd p2p && cargo build --workspace 2>&1 | tee /tmp/build.log
grep -i "warning" /tmp/build.log | grep -v "unused_imports.*test"             # expect: 0 substantive warnings
```

- [x] Zero substantive warnings → proceed
- [x] Warnings present → STOP; escalate

### A.3 Tests pass

```bash
cd p2p && cargo test --workspace -- --skip e2e          # ~3 min
cd p2p/core && cmake --build build && ctest --output-on-failure  # ~1 min
```

- [x] Rust workspace tests: all pass
- [x] C11 tests: all pass
- [x] Any failure → STOP; escalate

### A.4 Static-analysis test passes

The trust-boundary discipline gate must pass:

```bash
cd p2p && cargo test -p foretias-core trust_boundary_type_usage
```

- [x] `trust_boundary_type_usage` test passes
- [x] Any failure → STOP; escalate (this means the type-enforced trust system
      has regressed and must be repaired first)

---

## Phase B — Group 2 Closure (Already Done — Audit Only)

These items were verified on 2026-05-22 against branch `alpha` at commit
80f0714. No changes needed; this section documents what was checked.

### B.1 Stale spec status updates

- [x] `CLEANUP_REPAIRS_SPEC.md` — status line updated to "COMPLETED"
      (verified 2026-05-22 by reviewer)
- [x] `CLEANUP_REPAIRS_PLAN.md` — superseded note added
      (verified 2026-05-22)
- [x] `CHRONONCHAIN_NAMING_SPEC.md` — stale per-file boxes closed
      (verified 2026-05-22; deferred doc-spec rename items marked [-])
- [x] `2026_05_18_TIDY_SPEC.md` — status updated to "Mostly Complete"
      with redirect to GROUP2
      (verified 2026-05-22)

### B.2 README.md / HOWTO.md / AGENTS.md alignment

- [x] `README.md` — no `foretias-node` references; line 181 uses `foretias-server`
- [x] `HOWTO.md` — no `foretias-node` references; build commands use `foretias-server`
- [x] `AGENTS.md` — three-crate architecture documented; transport table present
      (lines 265-271)

### B.3 Code-state grep matrix (snapshot, 2026-05-22)

| Pattern | Files searched | Hits | Status |
|---------|---------------|------|--------|
| `TickRecord` | source dirs (excl. target/) | 0 | ✅ |
| `P2PConfig` | p2p/ | 0 | ✅ |
| `NodeConfig` (outside def + bridge) | p2p/foretias-server/src | 0 | ✅ |
| `#[allow(deprecated)]` | p2p/ | 0 | ✅ |
| `#[allow(dead_code)]` (non-bindgen) | p2p/ | 0 | ✅ |
| `thread_rng()` | source dirs | 0 | ✅ |
| `foretias-node` (in docs) | README.md, HOWTO.md, AGENTS.md | 0 | ✅ |

---

## Completion Criteria

This plan is complete because:
1. Phase A pre-flight checks document the baseline that every other group depends on
2. Phase B confirms zero outstanding Group 2 code work

No commit is required from Group 2 itself. The plan is referenced by all other
groups during their pre-flight.
