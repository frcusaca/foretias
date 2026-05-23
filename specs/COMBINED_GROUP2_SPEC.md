# COMBINED_GROUP2_SPEC.md
# Workspace Hygiene & Master Coordination

**Date:** 2026-05-22 (reviewed and verified against alpha at 80f0714)
**Status:** ✅ ALL CODE COMPLETE — this group also serves as the master coordination
            document for Groups 3, 4, 5
**Paired Plan:** `COMBINED_GROUP2_PLAN.md`
**Supersedes:**
- `CLEANUP_REPAIRS_SPEC.md` / `CLEANUP_REPAIRS_PLAN.md` (code merged 2026-05-12)
- `CHRONONCHAIN_NAMING_SPEC.md` (code merged 2026-05-18, commit b75d10a)
- `2026_05_18_TIDY_SPEC.md` (all tasks now verified done)

---

## 1. Group 2 Status — All Items Verified Done

A grep of the current `alpha` branch confirms the following:

| Item | Verification command | Result |
|------|---------------------|--------|
| No `TickRecord` in source | `grep -rl TickRecord p2p/{core-engine,foretias-client,foretias-server}/src` | 0 matches |
| No `P2PConfig` in source | `grep -r P2PConfig p2p/` | 0 matches |
| No `NodeConfig` outside its definition | `grep -rn NodeConfig p2p/foretias-server/src/` | 0 matches |
| No `#[allow(deprecated)]` in workspace | `grep -r "#\[allow(deprecated" p2p/` | 0 matches |
| No `#[allow(dead_code)]` outside bindgen | same pattern | 0 (only bindgen-generated) |
| `README.md` uses `foretias-server` | `grep foretias-node README.md` | 0 matches |
| `HOWTO.md` uses `foretias-server` | same | 0 matches |
| `AGENTS.md` has transport table | `grep "Peer Transport Comparison" AGENTS.md` | line 265 |
| `cargo build --workspace` | n/a | zero warnings |
| `cargo test --workspace` | n/a | all pass |

Group 2 requires **no further code or documentation changes**. The Plan exists only
to confirm these checks pass on each agent's branch before work starts.

---

## 2. Master Coordination for Groups 3, 4, 5

This section defines how independent agents work on Groups 3, 4, 5 in parallel
without stepping on each other.

### 2.1 Goals (in priority order, per user direction)

1. **Correctness** — every change strengthens the temporal attestation guarantee
2. **Trustworthiness** — no silent failures, no false success returns, signed evidence
3. **Robustness** — no panics on reachable inputs, no unzeroized secrets, defensive at boundaries
4. **Safety** — no nonce reuse, no unsafe `Send` lies, no unsigned DHT records
5. **Comprehensibility** — clear types, named invariants, single-responsibility modules

Anything that conflicts with these takes precedence over speed or scope.

### 2.2 Phase Ordering (between groups)

Three execution phases minimize merge conflicts:

```
Phase 1 (parallel, ~3-5 days each):
  ├── G3-B    NoiseSession `unsafe Send` removal       branch: g3-b-noise-send
  ├── G3-F2   signing_tbid.rs Zeroizing wrapping       branch: g3-f-secretbytes
  ├── G3-E    Unwrap sweep (auditing pass)             branch: g3-e-unwrap-sweep
  ├── G4-A    libp2p direct transport unit tests       branch: g4-a-libp2p-tests
  ├── G5-B    crash-recovery test fix                  branch: g5-b-crash-recovery
  └── G5-D    AGENTS.md gap-fill                       branch: g5-d-agents-rules

Phase 2 (after Phase 1 merges, ~5-7 days):
  └── G5-A    Clock injection sweep + DI fix           branch: g5-a-clock-inject
                ↑ broad surface; do alone

Phase 3 (after Phase 2 merges, parallel, ~5-10 days each):
  ├── G3-D3   DHT PeerRegistrationRecord sign+verify    branch: g3-d-dht-sign
  ├── G4-B    Calendar Active Mirroring                branch: g4-b-cal-mirror
  ├── G4-C    Calendar Proof of Storage                branch: g4-c-cal-proof
  └── G5-C    JSON-RPC auth (after human decision)     branch: g5-c-rpc-auth
```

### 2.3 Why this ordering

- **Phase 1** items each touch a single, non-overlapping file or area. Multiple agents
  can work simultaneously without conflict.
- **Phase 2** (Clock injection) touches ~17 sites across many modules. Doing it
  alone avoids merge conflicts with everything else.
- **Phase 3** items each rely on Clock injection being landed (timestamp generation
  goes through `Clock` not `SystemTime::now()`). G3-D3 and G4-B both modify
  `communerd/mod.rs` but in non-overlapping regions; merge order is alphabetical
  by branch name.

### 2.4 Worktree convention

Every agent uses:
```bash
git worktree add -b <branch-name> ${HOME}/tmp/foretias-worktrees/<branch-name>-$(date +%s)
```

Branch names must match §2.2 exactly so the master coordinator can track them.

### 2.5 Pre-flight check (every agent runs this first)

```bash
cd <worktree>
cd p2p && cargo test --workspace                    # must pass
cd ../p2p/core && cmake --build build && ctest      # must pass
```

If any test fails at pre-flight, **stop and escalate** — do not start work on
a broken baseline (AGENTS.md Development Rules).

### 2.6 Common commit message format

```
Major: <group-name>, Phase: <phase> — <one-line description>
Claude Code 2.1.119 (Claude Code); claude-opus-4-7 (or whichever model the agent uses)
```

### 2.7 Cross-group dependencies — Escalation table

If an agent encounters one of these, they must NOT proceed unilaterally:

| Situation | Escalation |
|-----------|------------|
| A G3 fix breaks an existing test in another module | Stop; ping master coordinator; do not "fix" the other test |
| The `Clock` trait shape needs to change | Block on G5-A landing first |
| A new wire-format breaking change is needed | Block; this is out of scope for these groups |
| FROST epoch handler needs real implementation | Out of scope; document only |

---

## 3. Verification Commands (used by all agents)

```bash
# Source hygiene (must remain clean)
grep -rl "TickRecord" p2p/{core-engine,foretias-client,foretias-server}/src   # → 0
grep -rl "P2PConfig" p2p/                                                     # → 0
grep -rn "#\[allow(deprecated" p2p/                                           # → 0
grep -rn "thread_rng" p2p/{core-engine,foretias-client,foretias-server}/src   # → 0

# Build/test invariants
cd p2p && cargo build --workspace 2>&1 | grep -i warning  # → 0
cd p2p && cargo test --workspace                          # → all pass
cd p2p/core && cmake --build build && ctest --output-on-failure   # → all pass
```

---

## 4. Out of Scope for Groups 2-5

- FROST epoch consensus (future major)
- New cryptographic algorithms beyond what exists
- Wire format breaking changes
- New language bindings (Python/Java)
- Production deployment infrastructure
- Performance optimization beyond what tests already cover
- AGENTS.md style refactors (only filling specific identified gaps)

If an agent thinks they need any of the above, stop and escalate.
