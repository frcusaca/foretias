# POST7 Audit & Cleanup — Work Plan

## TL;DR

> **Quick Summary**: Consolidate all code review findings from Group 7 completion into a single spec (POST7_AUDIT_CLEANUP_SPEC.md) with paired implementation plan. Includes unwrap audit fixes, mutex poison migration, dead code stub cleanup, and pre-P2P review findings.
>
> **Deliverables**: POST7_AUDIT_CLEANUP_SPEC.md, POST7_AUDIT_CLEANUP_PLAN.md, source spec/plan cleanup
>
> **Estimated Effort**: Medium (spec creation + interview + plan generation)
> **Parallel Execution**: NO — sequential spec creation

---

## Context

### Original Request
User requested combining all code review suggestions and the recently completed unwrap audit into a single new spec called POST7_AUDIT_CLEANUP_SPEC. Include all things that need fixing, ask questions as we build the plan. Expect no other code review document open with unaddressed concerns. After completing the new combined spec/plan, mark those specs and plans complete or canceled.

### Interview Summary
**Key Discussions:**
- Phase 8.2 blockers: Keep as OPEN items in Group 7, HIGHER priority than cleanup
- Mutex poison: INCLUDE with full explanation (what, why, why parking_lot, downsides)
- Dead code stubs: INCLUDE with step-by-step description, individual todos in plan
- Unwrap audit: Document all 10 in spec, fix 3 actionables, confirm 7 informational with human

**Research Findings:**
- Unwrap audit: 10 items (1 Critical, 2 Moderate, 7 Low)
- F2/F3/F4: Minor findings (unused import fixed, contamination fixed)
- Phase 8.2: 5 blockers documented, 7-10h estimated effort
- Pre-P2P reviews: 1 actionable finding (test enshrines known bug)

---

## Work Objectives

### Core Objective
Create a comprehensive POST7_AUDIT_CLEANUP_SPEC.md that consolidates ALL code review findings, with a paired implementation plan.

### Concrete Deliverables
- `specs/POST7_AUDIT_CLEANUP_SPEC.md` — Combined specification
- `specs/POST7_AUDIT_CLEANUP_PLAN.md` — Paired implementation plan
- Updated `specs/INDEX.md` — New spec listed, source specs marked complete/canceled
- Source spec/plan files marked complete or canceled

### Definition of Done
- [ ] POST7_AUDIT_CLEANUP_SPEC.md created with all findings
- [ ] POST7_AUDIT_CLEANUP_PLAN.md created with actionable tasks
- [ ] INDEX.md updated to include new spec
- [ ] Source specs/plans marked complete or canceled
- [ ] Human has reviewed and approved the spec

### Must Have
- All 10 unwrap audit items documented
- 3 actionable fixes (P0/P1) included as tasks
- 7 informational items included with human confirmation steps
- Mutex poison migration with full explanation
- Dead code stub inventory with per-stub recommendations
- Pre-P2P review findings included

### Must NOT Have
- Phase 8.2 blockers (stay as Group 7 open items)
- Test code unwraps (~494, acceptable)
- Build-time unwraps (build.rs)
- Product design recommendations (separate work items)

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (cargo test --workspace)
- **Automated tests**: Tests after (spec/plan creation, then verification)
- **Framework**: cargo test
- **Agent-Executed QA**: Verify spec completeness, plan actionability

### QA Policy
Every task MUST include agent-executed QA scenarios.

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — research + spec creation):
├── Task 1: Research parking_lot vs std Mutex [librarian]
├── Task 2: Research dead code stubs [explore]
├── Task 3: Verify unwrap audit items [explore]
└── Task 4: Create POST7_AUDIT_CLEANUP_SPEC.md [writing]

Wave 2 (After Wave 1 — plan creation):
├── Task 5: Create POST7_AUDIT_CLEANUP_PLAN.md [writing]
├── Task 6: Update INDEX.md [quick]
└── Task 7: Mark source specs/plans complete/canceled [quick]

Wave FINAL (After ALL tasks — review):
├── Task F1: Spec completeness audit [oracle]
└── Task F2: Plan actionability review [unspecified-high]
```

---

## TODOs

- [ ] 1. Research parking_lot vs std Mutex

  **What to do**:
  - Research mutex poisoning in std::sync::Mutex
  - Research how parking_lot::Mutex avoids poisoning
  - Research performance differences
  - Research API compatibility
  - Research downsides of parking_lot
  - Research why std::sync was used initially

  **References**:
  - Rust documentation for std::sync::Mutex
  - parking_lot crate documentation
  - Security best practices for concurrent code

  **Acceptance Criteria**:
  - [ ] Research document created with all 6 questions answered
  - [ ] Concrete technical details provided (not just "parking_lot is better")

  **QA Scenarios**:
  ```
  Scenario: Research completeness
    Tool: Read research document
    Steps:
      1. Verify all 6 questions answered
      2. Verify concrete technical details provided
      3. Verify no vague statements
    Expected: All questions answered with specifics
  ```

- [ ] 2. Research dead code stubs

  **What to do**:
  - Read communerdette.rs and identify all dead code warnings
  - For each stub: describe what it is, what phase created it, why it's dead, what to make it live
  - Provide recommendations: keep, mark with #[allow(dead_code)], or remove

  **References**:
  - `p2p/foretias-server/src/communerd/communerdette.rs`
  - Group 7 spec/plan for phase context

  **Acceptance Criteria**:
  - [ ] All 28 dead code warnings identified
  - [ ] Each stub has description, phase, reason, effort estimate, recommendation

  **QA Scenarios**:
  ```
  Scenario: Stub inventory completeness
    Tool: Read research document
    Steps:
      1. Count stubs identified
      2. Verify each has all required fields
      3. Verify recommendations are actionable
    Expected: 28 stubs documented with recommendations
  ```

- [ ] 3. Verify unwrap audit items

  **What to do**:
  - Read each of the 10 unwrap audit items
  - Verify current state (line numbers, code, context)
  - Assess if risk level has changed since audit

  **References**:
  - `docs/security/unwrap-audit.md`
  - Each source file listed in audit

  **Acceptance Criteria**:
  - [ ] All 10 items verified
  - [ ] Current line numbers confirmed
  - [ ] Risk levels assessed

  **QA Scenarios**:
  ```
  Scenario: Audit verification completeness
    Tool: Read verification document
    Steps:
      1. Verify all 10 items checked
      2. Verify line numbers match current code
      3. Verify risk assessments provided
    Expected: 10 items verified with current state
  ```

- [ ] 4. Create POST7_AUDIT_CLEANUP_SPEC.md

  **What to do**:
  - Create comprehensive spec combining all findings
  - Include all 10 unwrap items (3 fixes, 7 confirmations)
  - Include mutex poison migration with full explanation
  - Include dead code stub inventory
  - Include pre-P2P review findings
  - Ask human questions for unclear items

  **References**:
  - Research results from Tasks 1-3
  - `docs/security/unwrap-audit.md`
  - `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/issues.md`
  - `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/learnings.md`

  **Acceptance Criteria**:
  - [ ] Spec created at `specs/POST7_AUDIT_CLEANUP_SPEC.md`
  - [ ] All findings included
  - [ ] Human questions asked and answered
  - [ ] Spec reviewed and approved

  **QA Scenarios**:
  ```
  Scenario: Spec completeness
    Tool: Read spec file
    Steps:
      1. Verify all 10 unwrap items included
      2. Verify mutex poison section complete
      3. Verify dead code inventory included
      4. Verify pre-P2P findings included
    Expected: All findings documented
  ```

- [ ] 5. Create POST7_AUDIT_CLEANUP_PLAN.md

  **What to do**:
  - Create paired implementation plan
  - Include actionable tasks for each finding
  - Include human confirmation steps for informational items
  - Include verification strategy

  **References**:
  - POST7_AUDIT_CLEANUP_SPEC.md
  - Group 7 plan for task format reference

  **Acceptance Criteria**:
  - [ ] Plan created at `specs/POST7_AUDIT_CLEANUP_PLAN.md`
  - [ ] All actionable items have tasks
  - [ ] Human confirmation steps included
  - [ ] Verification strategy defined

  **QA Scenarios**:
  ```
  Scenario: Plan actionability
    Tool: Read plan file
    Steps:
      1. Verify all actionable items have tasks
      2. Verify human confirmation steps included
      3. Verify verification strategy defined
    Expected: Plan is actionable
  ```

- [ ] 6. Update INDEX.md

  **What to do**:
  - Add POST7_AUDIT_CLEANUP_SPEC to INDEX.md
  - Mark source specs/plans as complete or canceled
  - Update dependency graph

  **References**:
  - `specs/INDEX.md`
  - Source specs/plans to be marked

  **Acceptance Criteria**:
  - [ ] INDEX.md updated with new spec
  - [ ] Source specs/plans marked complete/canceled
  - [ ] Dependency graph updated

  **QA Scenarios**:
  ```
  Scenario: INDEX.md completeness
    Tool: Read INDEX.md
    Steps:
      1. Verify new spec listed
      2. Verify source specs marked
      3. Verify dependency graph updated
    Expected: INDEX.md is current
  ```

- [ ] 7. Mark source specs/plans complete/canceled

  **What to do**:
  - Review source specs/plans for unaddressed concerns
  - Mark as complete if all concerns addressed in POST7 spec
  - Mark as canceled if superseded
  - Document reasons for each marking

  **References**:
  - Source specs/plans:
    - `docs/security/unwrap-audit.md`
    - `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/issues.md`
    - `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/learnings.md`
    - Pre-P2P review documents

  **Acceptance Criteria**:
  - [ ] All source specs/plans reviewed
  - [ ] Appropriate markings applied
  - [ ] Reasons documented

  **QA Scenarios**:
  ```
  Scenario: Source spec cleanup
    Tool: Read source specs
    Steps:
      1. Verify all sources reviewed
      2. Verify markings applied
      3. Verify reasons documented
    Expected: Sources properly marked
  ```

---

## Final Verification Wave

- [ ] F1. **Spec Completeness Audit** — `oracle`
  Read POST7_AUDIT_CLEANUP_SPEC.md end-to-end. Verify all findings included, all questions answered, all sections complete.
  Output: `Sections [N/N] | Findings [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Plan Actionability Review** — `unspecified-high`
  Read POST7_AUDIT_CLEANUP_PLAN.md end-to-end. Verify all tasks actionable, all human steps included, all verification defined.
  Output: `Tasks [N/N] | Human Steps [N/N] | VERDICT: APPROVE/REJECT`

---

## Commit Strategy

- **1**: `spec: POST7 audit & cleanup — combine all review findings` — POST7_AUDIT_CLEANUP_SPEC.md, POST7_AUDIT_CLEANUP_PLAN.md, INDEX.md updates

---

## Success Criteria

### Verification Commands
```bash
# Verify spec exists
ls specs/POST7_AUDIT_CLEANUP_SPEC.md

# Verify plan exists
ls specs/POST7_AUDIT_CLEANUP_PLAN.md

# Verify INDEX.md updated
grep POST7 specs/INDEX.md
```

### Final Checklist
- [ ] POST7_AUDIT_CLEANUP_SPEC.md created
- [ ] POST7_AUDIT_CLEANUP_PLAN.md created
- [ ] INDEX.md updated
- [ ] Source specs/plans marked complete/canceled
- [ ] Human approved spec
