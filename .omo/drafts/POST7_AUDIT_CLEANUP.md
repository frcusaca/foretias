# Draft: POST7_AUDIT_CLEANUP_SPEC

## Requirements (confirmed)
- Combine ALL code review findings into one spec: POST7_AUDIT_CLEANUP_SPEC.md
- Include unwrap audit items, F2/F3/F4 findings, Phase 8.2 blockers
- No other code review document should have unaddressed concerns
- Mark source specs/plans as complete or canceled after consolidation

## Sources of Findings
1. **Unwrap Audit** (`docs/security/unwrap-audit.md`) — 10 ranked items, 1 Critical, 2 Moderate
2. **F2 Code Quality Review** — Unused import (fixed), pre-existing dead code
3. **F3 Real Manual QA** — 8/8 scenarios pass, no issues
4. **F4 Scope Fidelity** — `.claude` contamination (fixed)
5. **Phase 8.2 Issues** (`.omo/notepads/.../issues.md`) — 5 blockers for Calendar DoAttestation
6. **Learnings** (`.omo/notepads/.../learnings.md`) — Patterns and insights

## Technical Decisions
- TBD: Which items are actionable vs informational
- TBD: Priority ordering
- TBD: Whether Phase 8.2 blockers belong here or stay deferred

## Decisions (confirmed)
- **Phase 8.2 blockers**: Keep as OPEN items in Group 7, HIGHER priority than cleanup
- **Mutex poison**: INCLUDE with full explanation (what, why, why parking_lot, downsides)
- **Dead code stubs**: INCLUDE with step-by-step description, individual todos in plan
- **Unwrap audit**: Document all 10 in spec, fix 3 actionables, confirm 7 informational with human

## Scope Boundaries
- INCLUDE: All 3 actionable unwrap fixes (P0: handlers.rs:579, P1: snapshot_signature.rs:68, P1: foretias.rs:284)
- INCLUDE: Mutex poison migration (~50 calls) with explanation
- INCLUDE: Dead code stub cleanup (28 warnings in communerdette.rs)
- INCLUDE: Document all 10 unwrap items, confirm 7 informational with human
- EXCLUDE: Test code unwraps (~494, acceptable)
- EXCLUDE: Build-time unwraps (build.rs)
- EXCLUDE: Phase 8.2 blockers (stay as Group 7 open items, higher priority)

## Plan Created
- `.omo/plans/POST7_AUDIT_CLEANUP.md` — Work plan with 7 tasks + Final Verification Wave
- Research agents launched (parking_lot, dead code, unwrap verification)
- Spec structure drafted (will be finalized after research completes)
