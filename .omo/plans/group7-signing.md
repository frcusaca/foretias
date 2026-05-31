# Group 7 — Signing/Time-Family Implementation (Phases 16→17→15→13)

## TL;DR

> **Quick Summary**: Implement canonical encoding (postcard), signature wrappers, RecordBase trait, gate enforcement, StrawmanSuite/TinmanSuite check suites, FamilyRecord/Family Cache, and FB gossip — on worktree `group7-signing` off alpha, merging each phase back when green.
>
> **Deliverables**:
> - postcard canonical encoding replacing ad-hoc byte concatenation
> - `Unprocessed<R>` → `UnverifiedSignatureEnvelope<R>` (alias `DontUse<R>`) rename across 14 files
> - `RecordBase` trait with `always_require_full_signature()` gate enforcement
> - Ordered `SignatureEntry` list on all trust-boundary wrappers
> - `Externalized<R>` builder with ordered signing
> - Foretis wire-break (sig_input v2 via postcard, `signature_algorithm` bump)
> - StrawmanSuite (AST gate lint) + TinmanSuite (rustdoc-JSON type-resolved checks)
> - `FamilyRecord` (k×k cross-signing matrix, `CleanFullyAuthenticated` gate)
> - Family Cache in Communerd
> - FB (Bruderschaft) gossip emission/reception
>
> **Estimated Effort**: Large (~4 phases, 40+ tasks, 1-2 weeks of agent time)
> **Parallel Execution**: YES — 5 waves, max 6 concurrent in Wave 1
> **Critical Path**: 16a → 16b → 17 → 15 → 13

---

## Context

### Original Request
Implement Group 7 (Communerdette) signing/Time-Family work: Phases 16→17→15→13 in build order, on isolated worktree `group7-signing` off alpha.

### Interview Summary
**Confirmed Choices**:
- Canonical serializer: **postcard** (not bincode) — deterministic, no_std, maintained
- Foretis wire-break: **signature_algorithm version bump** + clean cutover (no backward compat)
- Test strategy: **TDD for CHECK boxes** (negative tests first), tests-after for structural work, agent-executed QA throughout
- Split Phase 16 into **16a** (rename + postcard + RecordBase + wrappers) and **16b** (gate + Externalized builder + wire-break) per Metis

**Research Findings**:
- clean_auth.rs (925 lines) — current `Unprocessed<T>`, `CleanAuthenticated<T>`, `CleanFullyAuthenticated<T>`, `Externalized<T>`
- 14 files reference `Unprocessed` (6 production, 8 test)
- Current signing is ad-hoc: Foretis uses raw bytes + big-endian, ProbityReport uses length-prefixed + little-endian
- postcard/bincode NOT currently dependencies; serde_cbor exists (for encrypted storage, not signing)
- `sign_tbid_message` exists on Chronomatter; Calendar signing is Phase 8.4 (design-gated, deferred)

### Metis Review
**Key Findings (addressed)**:
- Split Phase 16 into 16a/16b → merge checkpoints to alpha
- postcard partitions from serde_cbor (canonical/signing vs encrypted storage)
- Phase 8.4 stub for Phase 13 local signing (trait stub, defer implementation)
- Phase 12 unit tests must pass before Phase 16 starts
- Only rename `Unprocessed`; CleanAuthenticated/CleanFullyAuthenticated stay as-is

---

## Work Objectives

### Core Objective
Standardize signing/canonical encoding across the trust boundary, add gate enforcement for full-signature requirements, and build the FamilyRecord/FB gossip infrastructure on top of the new type system.

### Concrete Deliverables
- postcard as canonical serializer for signing bytes
- `UnverifiedSignatureEnvelope<R>` (alias `DontUse<R>`) replacing `Unprocessed<R>`
- `RecordBase` trait with `always_require_full_signature()`
- `SignatureEntry` ordered list on wrappers
- Gate enforcement: fast-only rejected when full required
- `Externalized<R>` builder with ordered signing
- Foretis sig_input v2 (postcard, version bump)
- StrawmanSuite AST gate lint + TinmanSuite rustdoc-JSON checks
- FamilyRecord with k×k matrix, Family Cache in Communerd
- FB gossip emission/reception

### Definition of Done
- [ ] `cargo test --workspace` green on alpha after each phase merge
- [ ] `cargo test -p foretias-server --test toppoli -- --include-ignored` green
- [ ] Snapshot tests updated (`UPDATE_SNAPSHOT=1`)
- [ ] All CHECK boxes have negative tests

### Must Have
- postcard canonical encoding for all signing bytes
- `Unprocessed` → `UnverifiedSignatureEnvelope` rename (lsp_rename, all 14 files)
- `RecordBase::always_require_full_signature()` gate enforcement
- Ordered signature list on wrappers (no standalone `Signed<P>`)
- `Externalized<R>` builder enforcing terminal CommunerdEnvelope
- Foretis wire-break with `signature_algorithm` version bump
- StrawmanSuite gate lint (field + signatures + verify primitive)
- TinmanSuite type-resolved checks (aliases detected as violations)
- FamilyRecord k×k matrix with verify-at-construction
- Family Cache in Communerd
- FB gossip emission on FullyBound, reception verify+ingest

### Must NOT Have (Guardrails)
- MUST NOT: Panic on malformed input — always reject with distinct error variants
- MUST NOT: Touch `CleanAuthenticated` or `CleanFullyAuthenticated` naming
- MUST NOT: Remove `serde_cbor` — postcard partitions by use case (signing vs storage)
- MUST NOT: Complete Phase 12 tests as part of Phase 16
- MUST NOT: Design Phase 8.4 APIs — stub the interface, defer design
- MUST NOT: Start Phase 16 until Phase 12 unit tests pass
- MUST NOT: Commit intermediate (red) states to alpha

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (cargo test, toppoli harness, snapshot tests)
- **Automated tests**: TDD for CHECK boxes (negative tests first), tests-after for structural work
- **Framework**: cargo test (unit/integration), toppoli (multi-peer integration)
- **Agent-Executed QA**: ALWAYS — every task includes QA scenarios

### QA Policy
- **Rust library/module**: `cargo test -p foretias-core --lib`, `cargo test -p foretias-server --lib`
- **Snapshot tests**: `cargo test -p foretias-core --test trust_boundary_type_usage`
- **Toppoli integration**: `cargo test -p foretias-server --test toppoli -- --include-ignored`
- **Full workspace**: `cargo test --workspace`
- Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Phase 16a — Foundation Part A, 6 tasks):
├── Task 1: Add postcard dependency + basic round-trip test [quick]
├── Task 2: Rename Unprocessed → UnverifiedSignatureEnvelope (lsp_rename, 14 files) [quick]
├── Task 3: RecordBase trait + implement for ChrononRecord [quick]
├── Task 4: SignatureEntry struct + ordered signature-list model [quick]
├── Task 5: Migrate wrappers to carry signature lists [unspecified-high]
└── Task 6: Migrate canonical encoders (ProbityReport, PeerRegistrationRecord) to postcard [quick]

→ MERGE 16a TO ALPHA ←

Wave 2 (Phase 16b — Foundation Part B, 5 tasks):
├── Task 7: Gate enforcement (always_require_full_signature check) [unspecified-high]
├── Task 8: Externalized<R> builder with ordered signing [unspecified-high]
├── Task 9: Foretis wire-break (sig_input v2 via postcard) [deep]
├── Task 10: RecordBase implementations for remaining types [quick]
└── Task 11: Snapshot/trust-boundary test updates [quick]

→ MERGE 16b TO ALPHA ←

Wave 3 (Phase 17 — Lint Suites, 4 tasks):
├── Task 12: StrawmanSuite renames + check_gate_bodies() [quick]
├── Task 13: TinmanSuite (rustdoc JSON type-resolved checks) [deep]
├── Task 14: TinmanSuite snapshot integration + cross-tabulation [quick]
└── Task 15: StrawmanSuite known-bad test case + wire into snapshot test [quick]

→ MERGE 17 TO ALPHA ←

Wave 4 (Phase 15 — FamilyRecord + Family Cache, 5 tasks):
├── Task 16: FamilyRecord payload + verifying constructor [deep]
├── Task 17: FamilyRecord k×k matrix verification [deep]
├── Task 18: FamilyRecord gate enforcement (CleanFullyAuthenticated only) [unspecified-high]
├── Task 19: Family Cache in Communerd [unspecified-high]
└── Task 20: FamilyRecord DHT publication + cache lookup flow [deep]

→ MERGE 15 TO ALPHA ←

Wave 5 (Phase 13 — FB Gossip, 4 tasks):
├── Task 21: CommunerdetteHost API additions (sign + publish probity) [quick]
├── Task 22: FB emission (FullyBound established/lost) [unspecified-high]
├── Task 23: FB reception (verify+ingest, full-signature gate) [unspecified-high]
└── Task 24: Phase 8.4 trait stub + FB gossip integration tests [quick]

→ MERGE 13 TO ALPHA ←

Critical Path: 1 → 3 → 5 → 7 → 9 → 11 → 12 → 16 → 18 → 21 → 23
Parallel Speedup: ~60% faster than sequential
Max Concurrent: 6 (Wave 1)
```

### Dependency Matrix

| Tasks | Blocks | Blocked By |
|-------|--------|------------|
| 1-2 | 3-6 | None (Wave 1) |
| 3 | 4-5 | 1 |
| 4 | 5 | 3 |
| 5 | 7, 8 | 3, 4 |
| 6 | 11 | 1 |
| 7 | 9, 11 | 5 |
| 8 | 9, 11 | 5 |
| 9 | 11 | 7, 8 |
| 10 | 11 | 3 |
| 11 | 12-15 | 7, 8, 9, 10 (Wave 2 done) |
| 12 | 15 | 11 |
| 13 | 14 | None (parallel with 12) |
| 14 | 15 | 13 |
| 15 | 16-20 | 12, 14 (Wave 3 done) |
| 16 | 17-18 | 15 |
| 17 | 18 | 16 |
| 18 | 19 | 17 |
| 19 | 20 | 18 |
| 20 | 21-24 | 19 |
| 21 | 22 | 20 |
| 22 | 24 | 21 |
| 23 | 24 | 21 |
| 24 | F1-F4 | 22, 23 |

### Agent Dispatch Summary

- **Wave 1**: 6 tasks — T1-T4,T6 → `quick`, T5 → `unspecified-high`
- **Wave 2**: 5 tasks — T7,T8 → `unspecified-high`, T9 → `deep`, T10,T11 → `quick`
- **Wave 3**: 4 tasks — T12,T14,T15 → `quick`, T13 → `deep`
- **Wave 4**: 5 tasks — T16,T17,T20 → `deep`, T18,T19 → `unspecified-high`
- **Wave 5**: 4 tasks — T21,T24 → `quick`, T22,T23 → `unspecified-high`
- **FINAL**: 4 tasks — F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

### Wave 1 — Phase 16a (Foundation Part A)

- [x] 1. Add postcard dependency + basic round-trip test
      (2026-05-31 14:28)

   **What to do**:
  - Add `postcard = "1"` to `p2p/core-engine/Cargo.toml` [dev-dependencies first, then dependencies]
  - Derive `serde::Serialize` on `ChrononRecord` (if not already derived)
  - Write round-trip test: `postcard::to_allocvec(&record)` → `postcard::from_bytes()` → assert equality
  - Verify postcard produces deterministic output (two serializations of same input produce identical bytes)

  **Must NOT do**:
  - Do NOT replace serde_cbor — postcard is for signing canonical bytes only
  - Do NOT derive Serialize on types containing signatures (signatures live in wrappers)

  **Recommended Agent Profile**:
  - **Category**: `quick` — dependency addition + basic test, well-scoped
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 2, 6)
  - **Parallel Group**: Wave 1 (with Tasks 2-6)
  - **Blocks**: Tasks 3, 4, 5, 6, 11
  - **Blocked By**: None

  **References**:
  - `p2p/core-engine/Cargo.toml` — Add postcard dependency
  - `p2p/core-engine/src/foretias/tick.rs:ChrononRecord` — Target type for round-trip test
  - `p2p/core-engine/src/foretias/clean_auth.rs` — Current wrapper definitions
  - postcard docs: `https://postcard.cs01.com/` — API reference

  **Acceptance Criteria**:
  - [ ] `cargo check -p foretias-core` passes
  - [ ] Round-trip test passes: serialize → deserialize → assert equality
  - [ ] Determinism test passes: two serializations produce identical bytes

  **QA Scenarios**:
  ```
  Scenario: postcard round-trip for ChrononRecord
    Tool: Bash (cargo test)
    Preconditions: postcard added to Cargo.toml
    Steps:
      1. cd p2p && cargo test -p foretias-core postcard_round_trip -- --nocapture
      2. Assert: test passes, no compilation errors
    Expected Result: 1 test passed, 0 failed
    Evidence: .sisyphus/evidence/task-1-postcard-roundtrip.log

  Scenario: postcard determinism (same input → same bytes)
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core postcard_determinism -- --nocapture
    Expected Result: Two postcard::to_allocvec calls produce identical Vec<u8>
    Evidence: .sisyphus/evidence/task-1-postcard-determinism.log
  ```

  **Commit**: YES (groups with 16a)

- [x] 2. Rename Unprocessed → UnverifiedSignatureEnvelope (lsp_rename, 14 files)
      (2026-05-31 14:28)

   **What to do**:
  - Use `lsp_rename` on `Unprocessed` in `p2p/core-engine/src/foretias/clean_auth.rs` to rename to `UnverifiedSignatureEnvelope`
  - Add type alias: `pub type DontUse<R> = UnverifiedSignatureEnvelope<R>;`
  - Update re-exports in `p2p/core-engine/src/foretias/mod.rs`
  - Verify all 14 files (6 production + 8 test) compile after rename
  - Update doc comments that reference "Unprocessed" to "UnverifiedSignatureEnvelope"

  **Must NOT do**:
  - Do NOT rename `CleanAuthenticated` or `CleanFullyAuthenticated`
  - Do NOT use manual find-replace — use lsp_rename for correctness
  - Do NOT change behavior — this is a pure rename

  **Recommended Agent Profile**:
  - **Category**: `quick` — mechanical rename with lsp_rename
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 1, 6)
  - **Parallel Group**: Wave 1 (with Tasks 1-6)
  - **Blocks**: Tasks 3-11 (all downstream references)
  - **Blocked By**: None

  **References**:
  - `p2p/core-engine/src/foretias/clean_auth.rs:39-79` — Unprocessed definition
  - `p2p/core-engine/src/foretias/mod.rs` — Re-exports
  - 14 call sites: clean_auth.rs, report.rs, handlers.rs, communerd/mod.rs, communerdette.rs, mod.rs, + 8 test files

  **Acceptance Criteria**:
  - [ ] `cargo check -p foretias-core` passes
  - [ ] `cargo check -p foretias-server` passes
  - [ ] `cargo check -p foretias-client` passes
  - [ ] All 14 files reference `UnverifiedSignatureEnvelope` (or `DontUse`)
  - [ ] `DontUse<R>` alias exists and compiles

  **QA Scenarios**:
  ```
  Scenario: All crates compile after rename
    Tool: Bash (cargo check)
    Steps:
      1. cd p2p && cargo check --workspace 2>&1
      2. Assert: exit code 0, no "Unprocessed" unresolved errors
    Expected Result: Clean compilation across workspace
    Evidence: .sisyphus/evidence/task-2-workspace-check.log

  Scenario: DontUse alias compiles and is usable
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core dont_use -- --nocapture
    Expected Result: Tests using DontUse alias compile and pass
    Evidence: .sisyphus/evidence/task-2-dontuse-alias.log
  ```

  **Commit**: YES (groups with 16a)

- [x] 3. RecordBase trait + implement for ChrononRecord
      (2026-05-31 15:02)

  **What to do**:
  - Define `RecordBase` trait in `p2p/core-engine/src/foretias/clean_auth.rs`:
    ```rust
    pub trait RecordBase: serde::Serialize {
        fn always_require_full_signature(&self) -> bool { false }
    }
    ```
  - Implement `RecordBase` for `ChrononRecord` (returns `false`)
  - Add `#[derive(serde::Serialize)]` to `ChrononRecord` if not already present
  - Write unit test: `assert!(!ChrononRecord::always_require_full_signature(&record))`

  **Must NOT do**:
  - Do NOT make RecordBase public outside core-engine (it's internal to the trust boundary)
  - Do NOT add methods beyond `always_require_full_signature()` — keep the surface narrow

  **Recommended Agent Profile**:
  - **Category**: `quick` — trait definition + single implementation
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 4, 6)
  - **Parallel Group**: Wave 1 (after Task 1 completes)
  - **Blocks**: Tasks 4, 5, 10
  - **Blocked By**: Task 1 (needs postcard in deps for Serialize derive)

  **References**:
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1985-2009` — RecordBase spec (§21.5)
  - `p2p/core-engine/src/foretias/tick.rs:ChrononRecord` — Target type
  - `p2p/core-engine/src/foretias/clean_auth.rs` — RecordBase goes here

  **Acceptance Criteria**:
  - [ ] RecordBase trait compiles with `always_require_full_signature()` method
  - [ ] ChrononRecord implements RecordBase and returns `false`
  - [ ] ChrononRecord derives serde::Serialize
  - [ ] Unit test passes: default always_require_full_signature is false

  **QA Scenarios**:
  ```
  Scenario: RecordBase trait compiles and ChrononRecord implements it
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core record_base -- --nocapture
    Expected Result: Test passes, RecordBase trait exists
    Evidence: .sisyphus/evidence/task-3-recordbase.log
  ```

  **Commit**: YES (groups with 16a)

- [ ] 4. SignatureEntry struct + ordered signature-list model

  **What to do**:
  - Define `SignatureEntry` struct:
    ```rust
    pub struct SignatureEntry {
        pub role:      SignerRole,
        pub tbid:      String,
        pub algorithm: SigAlgorithm,
        pub sig:       Vec<u8>,
    }
    pub enum SignerRole { Chronomatter, Calendar, Member, CommunerdEnvelope }
    pub enum SigAlgorithm { Ed25519, DualKey }
    ```
  - Derive `serde::Serialize` on `SignatureEntry` (for ordered signing rule)
  - Add `signatures: Vec<SignatureEntry>` field to `UnverifiedSignatureEnvelope<R>`
  - Add `signatures: Vec<SignatureEntry>` field to `CleanAuthenticated<R>` and `CleanFullyAuthenticated<R>`
  - Write tests: serialization round-trip for SignatureEntry, enum variant coverage

  **Must NOT do**:
  - Do NOT create a standalone `Signed<P>` type — signatures live in wrappers
  - Do NOT allow signatures in the payload `R` — R must be signature-free

  **Recommended Agent Profile**:
  - **Category**: `quick` — struct + enum definitions, well-scoped
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 6)
  - **Parallel Group**: Wave 1 (after Task 3)
  - **Blocks**: Task 5
  - **Blocked By**: Task 3 (needs RecordBase)

  **References**:
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1867-1899` — SignatureEntry spec (§21.2)
  - `p2p/core-engine/src/foretias/clean_auth.rs` — SignatureEntry goes here

  **Acceptance Criteria**:
  - [ ] SignatureEntry struct compiles with all fields
  - [ ] SignerRole enum has 4 variants
  - [ ] SigAlgorithm enum has 2 variants
  - [ ] SignatureEntry derives Serialize
  - [ ] UnverifiedSignatureEnvelope carries signatures field

  **QA Scenarios**:
  ```
  Scenario: SignatureEntry round-trip serialization
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core signature_entry -- --nocapture
    Expected Result: Serialize → deserialize produces identical SignatureEntry
    Evidence: .sisyphus/evidence/task-4-signature-entry.log
  ```

  **Commit**: YES (groups with 16a)

- [ ] 5. Migrate wrappers to carry signature lists

  **What to do**:
  - Update `UnverifiedSignatureEnvelope<R>`: add `signatures: Vec<SignatureEntry>`, update constructors
  - Update `CleanAuthenticated<R>`: add `signatures: Vec<SignatureEntry>`, update `into_clean_authenticated()` to carry verified signatures
  - Update `CleanFullyAuthenticated<R>`: add `signatures: Vec<SignatureEntry>`, update dual-key constructor
  - Update all specialized impls (ChrononRecord, Foretis, EpochSnapshot) to work with new wrapper shape
  - Ensure `R` is signature-free (no `signature` field on payload types — existing signatures move to wrapper)
  - Write migration tests: existing verify() paths still work with new signature field

  **Must NOT do**:
  - Do NOT change verification logic yet — that's Phase 16b
  - Do NOT remove existing signature fields from payloads yet — that's a separate migration step
  - Do NOT break existing tests — the signatures field starts as `Vec::new()` for existing paths

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — 925-line file migration, multiple specialized impls
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed — this is Rust refactoring, no domain-specific skills

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Tasks 3, 4)
  - **Parallel Group**: Wave 1 (sequential within wave)
  - **Blocks**: Tasks 7, 8
  - **Blocked By**: Tasks 3, 4

  **References**:
  - `p2p/core-engine/src/foretias/clean_auth.rs` — Full file (925 lines), all impl blocks
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1877-1899` — Wrapper states spec
  - `p2p/core-engine/src/foretias/tick.rs` — ChrononRecord, Foretis definitions

  **Acceptance Criteria**:
  - [ ] All wrapper types carry `signatures: Vec<SignatureEntry>`
  - [ ] `cargo check --workspace` passes
  - [ ] Existing tests compile (signatures field defaults to empty Vec)
  - [ ] `into_clean_authenticated()` carries signatures from UnverifiedSignatureEnvelope

  **QA Scenarios**:
  ```
  Scenario: All workspace crates compile with signature lists on wrappers
    Tool: Bash (cargo check)
    Steps:
      1. cd p2p && cargo check --workspace 2>&1
    Expected Result: Clean compilation, no errors
    Evidence: .sisyphus/evidence/task-5-workspace-check.log

  Scenario: Existing verify paths still compile (backward compat)
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core --lib -- clean_auth -- --nocapture
    Expected Result: Tests compile, existing paths work with empty signatures
    Evidence: .sisyphus/evidence/task-5-existing-tests.log
  ```

  **Commit**: YES (groups with 16a)

- [ ] 6. Migrate canonical encoders (ProbityReport, PeerRegistrationRecord) to postcard

  **What to do**:
  - Replace `ProbityReport::canonical()` with `postcard::to_allocvec(&self)` (signature-free payload)
  - Replace `PeerRegistrationRecord::canonical_payload()` with `postcard::to_allocvec(&self)` (signature-free payload)
  - Ensure both types derive `serde::Serialize`
  - Write tests: new postcard encoding vs old canonical encoding (document the difference — this is a signing change, not a wire change for these types)
  - Update `UnverifiedSignatureEnvelope<ProbityReport>::verify()` to use postcard bytes

  **Must NOT do**:
  - Do NOT change wire format — postcard is for signing bytes only
  - Do NOT migrate Foretis yet — that's Task 9 (wire-break, separate task)
  - Do NOT change serde_cbor usage for encrypted storage

  **Recommended Agent Profile**:
  - **Category**: `quick` — two types, well-defined migration
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 1-4, after Task 1)
  - **Parallel Group**: Wave 1
  - **Blocks**: Task 11
  - **Blocked By**: Task 1 (needs postcard in deps)

  **References**:
  - `p2p/core-engine/src/probity/report.rs:46-68` — ProbityReport::canonical()
  - `p2p/foretias-server/src/communerd/mod.rs` — PeerRegistrationRecord::canonical_payload()
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:2073-2078` — Migration notes

  **Acceptance Criteria**:
  - [ ] ProbityReport::canonical() uses postcard
  - [ ] PeerRegistrationRecord::canonical_payload() uses postcard
  - [ ] Both types derive Serialize
  - [ ] Existing verification paths still work (signature verification against new canonical bytes)

  **QA Scenarios**:
  ```
  Scenario: ProbityReport postcard canonical produces deterministic bytes
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-core probity_canonical -- --nocapture
    Expected Result: postcard::to_allocvec produces identical bytes for same input
    Evidence: .sisyphus/evidence/task-6-probity-canonical.log

  Scenario: PeerRegistrationRecord postcard canonical works
    Tool: Bash (cargo test)
    Steps:
      1. cd p2p && cargo test -p foretias-server peer_registration_canonical -- --nocapture
    Expected Result: canonical_payload uses postcard, verification passes
    Evidence: .sisyphus/evidence/task-6-peer-reg-canonical.log
  ```

  **Commit**: YES (groups with 16a)

### Wave 2 — Phase 16b (Foundation Part B)

- [ ] 7. Gate enforcement (always_require_full_signature check)

  **What to do**:
  - Implement the `DontUse<R>` → clean-state gate in `clean_auth.rs`:
    - When `R::always_require_full_signature() == true`: gate MUST produce `CleanFullyAuthenticated<R>` or reject
    - When `R::always_require_full_signature() == false`: `CleanAuthenticated<R>` is acceptable
    - Verify ALL signatures (no verify-last-only shortcut)
    - Reject unexpected signature count (cardinality is exactly two today)
  - Implement `verify_all_signatures()` method on `UnverifiedSignatureEnvelope<R>`
  - Write TDD negative tests FIRST:
    - Record with `always_require_full_signature() == true` + fast-only signatures → REJECTED
    - Record with wrong signature count → REJECTED
    - Record with invalid signature → REJECTED

  **Must NOT do**:
  - Do NOT panic on verification failure — return distinct error variants
  - Do NOT downgrade a full-required record to fast level
  - Do NOT skip signature verification (no shortcuts)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — core security gate logic, TDD required
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed — this is Rust implementation with defensive coding

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 8, 10)
  - **Parallel Group**: Wave 2 (after Wave 1 merge)
  - **Blocks**: Tasks 9, 11
  - **Blocked By**: Task 5 (needs signature lists on wrappers)

  **References**:
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:2000-2022` — Gate enforcement spec (§21.5)
  - `p2p/core-engine/src/foretias/clean_auth.rs` — Gate goes here
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1908-1918` — Verification state machine

  **Acceptance Criteria**:
  - [ ] Gate consults `always_require_full_signature()` before producing clean state
  - [ ] Record requiring full + fast-only signatures → REJECTED (negative test passes)
  - [ ] Record with wrong signature count → REJECTED (negative test passes)
  - [ ] Record with invalid signature → REJECTED (negative test passes)
  - [ ] Valid record with two correct signatures → CleanAuthenticated (positive test passes)

  **QA Scenarios**:
  ```
  Scenario: Record requiring full signature but carrying only fast → REJECTED
    Tool: Bash (cargo test — TDD negative test)
    Steps:
      1. Create test record with always_require_full_signature() == true
      2. Attach only fast (Ed25519) signatures
      3. Call gate → assert Err(never CleanAuthenticated)
    Expected Result: Gate returns specific rejection error, not panic
    Evidence: .sisyphus/evidence/task-7-full-required-fast-only.log

  Scenario: Valid record with two correct signatures → CleanAuthenticated
    Tool: Bash (cargo test — positive test)
    Steps:
      1. Create test record with valid Ed25519 signatures (inner + CommunerdEnvelope)
      2. Call gate → assert Ok(CleanAuthenticated)
    Expected Result: Gate returns CleanAuthenticated with verified signatures
    Evidence: .sisyphus/evidence/task-7-valid-two-sigs.log
  ```

  **Commit**: YES (groups with 16b)

- [ ] 8. Externalized<R> builder with ordered signing

  **What to do**:
  - Implement `Externalized<R>::builder_from(src)` accepting raw `R` or `CleanAuthenticated<R>`
  - Implement `add_signature(role, key)` that appends entry signing `postcard(payload) ‖ postcard(&signatures_so_far)`
  - Implement `build()` that validates terminal entry is `CommunerdEnvelope` and returns `Externalized<R>`
  - Write tests:
    - Builder with zero signatures → REJECTED
    - Builder without terminal CommunerdEnvelope → REJECTED
    - Builder with correct ordered signatures → Externalized (positive test)
    - Builder preserves payload fields, drops inbound signatures

  **Must NOT do**:
  - Do NOT allow constructing Externalized without a builder
  - Do NOT allow out-of-order signing
  - Do NOT carry inbound signatures into outbound record

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high` — builder pattern with ordered signing enforcement
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 7, 10)
  - **Parallel Group**: Wave 2 (after Wave 1 merge)
  - **Blocks**: Tasks 9, 11
  - **Blocked By**: Task 5 (needs signature lists on wrappers)

  **References**:
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:2026-2062` — Externalized builder spec (§21.6)
  - `p2p/core-engine/src/foretias/clean_auth.rs` — Builder goes here

  **Acceptance Criteria**:
  - [ ] builder_from() accepts raw R or CleanAuthenticated<R>
  - [ ] add_signature() enforces ordered signing rule
  - [ ] build() rejects unless last entry is CommunerdEnvelope
  - [ ] Builder with zero signatures → REJECTED (negative test)
  - [ ] Builder with correct signatures → Externalized (positive test)

  **QA Scenarios**:
  ```
  Scenario: Builder with zero signatures → REJECTED
    Tool: Bash (cargo test — TDD negative test)
    Steps:
      1. Create builder from raw payload
      2. Call build() without add_signature
      3. Assert Err
    Expected Result: Specific error about missing terminal CommunerdEnvelope
    Evidence: .sisyphus/evidence/task-8-zero-sigs-rejected.log

  Scenario: Builder with ordered signatures → Externalized
    Tool: Bash (cargo test — positive test)
    Steps:
      1. Create builder, add Chronomatter signature, add CommunerdEnvelope signature
      2. Call build() → assert Ok(Externalized)
    Expected Result: Externalized with correct signature chain
    Evidence: .sisyphus/evidence/task-8-ordered-sigs-ok.log
  ```

  **Commit**: YES (groups with 16b)

- [ ] 9. Foretis wire-break (sig_input v2 via postcard)

  **What to do**:
  - Define signature-free `Foretis` payload (remove `signature`/`signature_algorithm` fields from Foretis struct)
  - Implement `RecordBase` for `Foretis` (returns `false`)
  - Replace `sig_input` construction (`tbid || chronon_be8 || content`) with `postcard::to_allocvec(&foretis_payload)`
  - Add `signature_algorithm: u32` field with version bump (v2 = postcard encoding)
  - Update `Foretis::stamp()` to use postcard canonical bytes
  - Update `Foretis::verify()` to use postcard canonical bytes
  - Update `UnverifiedSignatureEnvelope<Foretis>::verify()` to use postcard bytes
  - Write tests:
    - New v2 sig_input produces correct postcard bytes
    - Old v1 sig_input format is rejected (or handled via version check)

  **Must NOT do**:
  - Do NOT maintain backward compatibility for v1 stamps (clean cutover)
  - Do NOT change the wire format for Foretis JSON — only the signing bytes change
  - Do NOT put signatures on the Foretis payload — they go in the wrapper

  **Recommended Agent Profile**:
  - **Category**: `deep` — wire-breaking change, affects core stamping protocol
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed — this is deep protocol work

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Tasks 7, 8)
  - **Parallel Group**: Wave 2 (after gate + builder)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 7, 8

  **References**:
  - `p2p/core-engine/src/foretias/tick.rs:176-238` — Foretis stamp() and verify()
  - `p2p/core-engine/src/foretias/clean_auth.rs:444-507` — Unprocessed<Foretis> impl
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:2076-2080` — Foretis wire-break notes

  **Acceptance Criteria**:
  - [ ] Foretis payload is signature-free (no signature fields)
  - [ ] sig_input uses `postcard::to_allocvec(&payload)`
  - [ ] `signature_algorithm` field carries version number (v2)
  - [ ] stamp() and verify() use new canonical bytes
  - [ ] stamp → verify round-trip passes with v2 encoding

  **QA Scenarios**:
  ```
  Scenario: Foretis v2 stamp-verify round-trip
    Tool: Bash (cargo test)
    Steps:
      1. Create Foretis with v2 sig_input (postcard)
      2. Sign with test key
      3. Verify → assert Ok
    Expected Result: Round-trip passes with postcard canonical bytes
    Evidence: .sisyphus/evidence/task-9-v2-roundtrip.log

  Scenario: Foretis v2 signature differs from v1 (wire-break confirmed)
    Tool: Bash (cargo test)
    Steps:
      1. Compute v1 sig_input (old BE+concat)
      2. Compute v2 sig_input (postcard)
      3. Assert: v1_bytes != v2_bytes
    Expected Result: Bytes differ, confirming wire break
    Evidence: .sisyphus/evidence/task-9-v1-vs-v2.log
  ```

  **Commit**: YES (groups with 16b)

- [ ] 10. RecordBase implementations for remaining types

  **What to do**:
  - Implement `RecordBase` for `ProbityReport` (returns `false` for ordinary reports; `true` for FB/GNF — use attribute field to decide)
  - Implement `RecordBase` for `EpochSnapshot` (returns `false`)
  - Derive `serde::Serialize` on any types that don't already have it
  - Write tests: each type's always_require_full_signature() returns correct value

  **Must NOT do**:
  - Do NOT add RecordBase to types that aren't part of the trust boundary
  - Do NOT implement FamilyRecord yet — that's Phase 15

  **Recommended Agent Profile**:
  - **Category**: `quick` — trait implementations, well-scoped
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 7, 8)
  - **Parallel Group**: Wave 2
  - **Blocks**: Task 11
  - **Blocked By**: Task 3 (needs RecordBase trait)

  **References**:
  - `p2p/core-engine/src/probity/report.rs` — ProbityReport definition
  - `p2p/core-engine/src/foretias/tick.rs` — EpochSnapshot definition
  - `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:2004-2009` — always_require_full_signature table

  **Acceptance Criteria**:
  - [ ] ProbityRecord::always_require_full_signature() returns correct value per type
  - [ ] EpochSnapshot implements RecordBase
  - [ ] All types derive Serialize

  **QA Scenarios**:
  ```
  Scenario: ProbityReport FB/GNF returns true for always_require_full_signature
    Tool: Bash (cargo test)
    Steps:
      1. Create ProbityReport with attribute="fb"
      2. Assert: always_require_full_signature() == true
      3. Create ProbityReport with attribute="ordinary"
      4. Assert: always_require_full_signature() == false
    Expected Result: Conditional return based on attribute
    Evidence: .sisyphus/evidence/task-10-probity-condition.log
  ```

  **Commit**: YES (groups with 16b)

- [ ] 11. Snapshot/trust-boundary test updates

  **What to do**:
  - Update `trust_boundary_type_usage.rs` snapshot for renamed wrapper types
  - Update `crypto_callsite_snapshot.rs` for any new crypto call sites
  - Run `UPDATE_SNAPSHOT=1` to regenerate snapshots
  - Verify snapshots are consistent (no unexpected diffs)

  **Must NOT do**:
  - Do NOT add new call sites unintentionally
  - Do NOT ignore snapshot diffs — investigate each one

  **Recommended Agent Profile**:
  - **Category**: `quick` — test maintenance
  - **Skills**: []
  - **Skills Evaluated but Omitted**: None needed

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on all Wave 2 tasks)
  - **Parallel Group**: Wave 2 (final task)
  - **Blocks**: Tasks 12-15 (Wave 3)
  - **Blocked By**: Tasks 6, 7, 8, 9, 10

  **References**:
  - `p2p/core-engine/tests/trust_boundary_type_usage.rs` — Trust boundary snapshot test
  - `p2p/foretias-server/tests/crypto_callsite_snapshot.rs` — Crypto callsite snapshot

  **Acceptance Criteria**:
  - [ ] `UPDATE_SNAPSHOT=1 cargo test -p foretias-core --test trust_boundary_type_usage` passes
  - [ ] `UPDATE_SNAPSHOT=1 cargo test -p foretias-server --test crypto_callsite_snapshot` passes
  - [ ] Snapshots are consistent (no spurious diffs)

  **QA Scenarios**:
  ```
  Scenario: Trust boundary snapshot passes
    Tool: Bash (cargo test)
    Steps:
      1. UPDATE_SNAPSHOT=1 cargo test -p foretias-core --test trust_boundary_type_usage -- --nocapture
    Expected Result: Snapshot regenerated, test passes
    Evidence: .sisyphus/evidence/task-11-trust-snapshot.log
  ```

  **Commit**: YES (groups with 16b)

---

### Wave 3 — Phase 17 (Lint Suites)

- [ ] 12. StrawmanSuite renames + check_gate_bodies()

  **What to do**:
  - Rename existing functions: `collect_all_type_usages` → `collect_ast_usages`, `verify_trust_boundary_invariants` → `verify_ast_invariants`, `format_usage_table` → `format_ast_table`
  - Rename snapshot section header to `=== StrawmanSuite (AST / syn) ===`
  - Implement `check_gate_bodies()`: for every gate fn (DontUse param → Clean* return), require body references all three: `always_require_full_signature`, signature data (`signatures`/`matrix`), verify primitive (`verify`/`verify_with`/`tbid_verify`)
  - Maintain `VERIFY_PRIMITIVES` set; honor `// gate-strawman-exempt: <reason>` opt-out marker

  **Must NOT do**: Do NOT cross-call StrawmanSuite↔TinmanSuite. Do NOT allow exemption without reason.

  **Recommended Agent Profile**: `quick` — renames + lint implementation. **Parallel**: Wave 3 (with Task 13). **Blocks**: 15. **Blocked By**: 11.

  **References**: `specs/TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md:336-402` (§9), `p2p/core-engine/tests/trust_boundary_type_usage.rs`

  **Acceptance Criteria**:
  - [ ] All renames applied, check_gate_bodies() compiles and runs
  - [ ] Lint catches missing references in gate functions

  **QA Scenarios**:
  ```
  Scenario: check_gate_bodies compiles and runs
    Tool: Bash (cargo test)
    Steps: 1. cd p2p && cargo test -p foretias-core check_gate_bodies -- --nocapture
    Expected Result: Test passes
    Evidence: .sisyphus/evidence/task-12-gate-bodies.log
  ```

  **Commit**: YES (groups with 17)

- [ ] 13. TinmanSuite (rustdoc JSON type-resolved checks)

  **What to do**:
  - Add dev-dependency `rustdoc-types` (pinned); implement: `invoke_rustdoc_json()`, `extract_fn_signatures()`, `resolve_type()`, `collect_semantic_usages()`
  - Implement `verify_semantic_invariants()`: structural rules + flag Resolved (alias/newtype) occurrences as violations
  - Write tests: rustdoc JSON version check, type resolution for known wrappers

  **Must NOT do**: Do NOT silently accept format_version mismatches.

  **Recommended Agent Profile**: `deep` — new analysis layer, rustdoc JSON parsing. **Parallel**: Wave 3 (with Task 12). **Blocks**: 14. **Blocked By**: 11.

  **References**: `specs/TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md:93-268` (§§5-7), `specs/TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md:393-411`

  **Acceptance Criteria**:
  - [ ] rustdoc-types added, invoke_rustdoc_json() produces valid JSON
  - [ ] collect_semantic_usages() finds wrapper occurrences, verify_semantic_invariants() flags Resolved

  **QA Scenarios**:
  ```
  Scenario: TinmanSuite finds existing wrapper occurrences
    Tool: Bash (cargo test)
    Steps: 1. cd p2p && cargo test -p foretias-core tinman -- --nocapture
    Expected Result: Semantic occurrences found and reported
    Evidence: .sisyphus/evidence/task-13-tinman.log
  ```

  **Commit**: YES (groups with 17)

- [ ] 14. TinmanSuite snapshot integration + cross-tabulation

  **What to do**: Add `=== TinmanSuite ===` and `=== TinmanSuite Cross-Tabulation ===` sections to snapshot; implement `merge_into_table()` and `format_merged_table()`; wire `UPDATE_SNAPSHOT=1`.

  **Recommended Agent Profile**: `quick`. **Parallel**: Wave 3 (after Task 13). **Blocks**: 15. **Blocked By**: 13.

  **References**: `specs/TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md:271-326` (§§7-8)

  **Acceptance Criteria**: [ ] Snapshot has all three sections. [ ] UPDATE_SNAPSHOT=1 regenerates.

  **QA Scenarios**:
  ```
  Scenario: Full snapshot test passes with both suites
    Tool: Bash (cargo test)
    Steps: 1. UPDATE_SNAPSHOT=1 cargo test -p foretias-core --test trust_boundary_type_usage
    Expected Result: All sections present
    Evidence: .sisyphus/evidence/task-14-snapshot.log
  ```

  **Commit**: YES (groups with 17)

- [ ] 15. StrawmanSuite known-bad test case + wire into snapshot test

  **What to do**: Create known-bad gate fn, wire check_gate_bodies() into test_trust_boundary_snapshot, verify caught, add exemption.

  **Recommended Agent Profile**: `quick`. **Parallel**: Wave 3 (final). **Blocks**: 16-20. **Blocked By**: 12, 14.

  **Acceptance Criteria**: [ ] check_gate_bodies() called from snapshot test. [ ] Known-bad caught. [ ] Test passes after exemption.

  **QA Scenarios**:
  ```
  Scenario: Known-bad gate caught by lint
    Tool: Bash (cargo test)
    Steps: 1. Create known-bad gate, 2. Run → assert failure, 3. Add exemption → assert pass
    Evidence: .sisyphus/evidence/task-15-known-bad.log
  ```

  **Commit**: YES (groups with 17)

### Wave 4 — Phase 15 (FamilyRecord + Family Cache)

- [ ] 16. FamilyRecord payload + verifying constructor

  **What to do**: Define `FamilyRecord` (signature-free) per §19.4; `try_new()` enforcing invariants; `MAX_FAMILY_MEMBERS` cap (DoS guard); `RecordBase` returning `true`.

  **Must NOT do**: Do NOT panic on malformed input — return FamilyError.

  **Recommended Agent Profile**: `deep`. **Parallel**: Wave 4 (with 17). **Blocks**: 17, 18. **Blocked By**: 15.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1550-1600` (§19.4), `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1176-1198`

  **Acceptance Criteria**:
  - [ ] try_new() rejects empty/duplicate members, bad matrix dims, oversized k
  - [ ] MAX_FAMILY_MEMBERS enforced, always_require_full_signature() == true

  **QA Scenarios**:
  ```
  Scenario: Malformed FamilyRecords rejected (TDD negative)
    Tool: Bash (cargo test)
    Steps: 1. Empty members → Err, 2. Duplicate → Err, 3. Wrong dims → Err, 4. Oversized → Err
    Expected Result: Distinct FamilyError per case, no panic
    Evidence: .sisyphus/evidence/task-16-malformed-family.log
  ```

  **Commit**: YES (groups with 15)

- [ ] 17. FamilyRecord k×k matrix verification

  **What to do**: Verify every `matrix[i][j]` is member i's dual-key (Ed25519 ‖ SLH-DSA) signature over member j's TBID. Reject on any failure.

  **Recommended Agent Profile**: `deep`. **Parallel**: Wave 4 (after 16). **Blocks**: 18. **Blocked By**: 16.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1784-1790`

  **Acceptance Criteria**: [ ] All k² verified. [ ] Single wrong signature → rejected.

  **QA Scenarios**:
  ```
  Scenario: k×k matrix — single wrong signature rejected
    Tool: Bash (cargo test)
    Steps: 1. Valid k=2, 2. Tamper matrix[0][1], 3. Verify → Err
    Evidence: .sisyphus/evidence/task-17-matrix-tamper.log
  ```

  **Commit**: YES (groups with 15)

- [ ] 18. FamilyRecord gate enforcement (CleanFullyAuthenticated only)

  **What to do**: Gate for `UnverifiedSignatureEnvelope<FamilyRecord>` → `CleanFullyAuthenticated<FamilyRecord>`. Verify full dual-key envelope AND k×k matrix. Never produce CleanAuthenticated.

  **Recommended Agent Profile**: `unspecified-high`. **Parallel**: Wave 4 (after 17). **Blocks**: 19. **Blocked By**: 17.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1769-1790` (§19.10), `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1189-1191`

  **Acceptance Criteria**:
  - [ ] Gate produces CleanFullyAuthenticated or rejects
  - [ ] Fast-only → rejected (negative test)

  **QA Scenarios**:
  ```
  Scenario: FamilyRecord fast-only → REJECTED (TDD negative)
    Tool: Bash (cargo test)
    Steps: 1. UnverifiedSignatureEnvelope<FamilyRecord> with fast Ed25519, 2. Gate → Err
    Evidence: .sisyphus/evidence/task-18-family-fast-only.log
  ```

  **Commit**: YES (groups with 15)

- [ ] 19. Family Cache in Communerd

  **What to do**: Implement Family Cache: `communerd_tbid → CleanFullyAuthenticated<FamilyRecord>`, reverse pointers, `family_cache_lookup(tbid)`.

  **Recommended Agent Profile**: `unspecified-high`. **Parallel**: Wave 4 (with 20). **Blocks**: 20. **Blocked By**: 18.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1618-1640` (§19.6)

  **Acceptance Criteria**: [ ] Cache stores CleanFullyAuthenticated. [ ] Lookup returns family. [ ] Cache miss → None.

  **QA Scenarios**:
  ```
  Scenario: Family Cache hit for known TBID
    Tool: Bash (cargo test)
    Steps: 1. Insert record, 2. Lookup communerd_tbid → Some, 3. Lookup member_tbid → Some
    Evidence: .sisyphus/evidence/task-19-cache-hit.log
  ```

  **Commit**: YES (groups with 15)

- [ ] 20. FamilyRecord DHT publication + cache lookup flow

  **What to do**: Connection flow: `/tbid` lookup → dial → fetch FamilyRecord → verify (full) → cache → start Communerdette. DHT publication.

  **Recommended Agent Profile**: `deep`. **Parallel**: Wave 4 (final). **Blocks**: 21-24. **Blocked By**: 18, 19.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md:1640-1667` (§19.3)

  **Acceptance Criteria**: [ ] Flow: lookup → fetch → verify → cache. [ ] DHT publish/lookup round-trip.

  **QA Scenarios**:
  ```
  Scenario: Connection flow end-to-end
    Tool: Bash (cargo test)
    Steps: 1. Publish to DHT, 2. Lookup → fetch → verify → cache, 3. Assert cached
    Evidence: .sisyphus/evidence/task-20-connection-flow.log
  ```

  **Commit**: YES (groups with 15)

### Wave 5 — Phase 13 (FB Gossip)

- [ ] 21. CommunerdetteHost API additions (sign + publish probity)

  **What to do**: Add `host_sign_probity_report()` and `host_publish_probity_report()` to CommunerdetteHost. Implement sign (canonical + Calendar signing stub) + publish (gossip). Update MockHost.

  **Must NOT do**: Do NOT implement full Calendar signing — trait stub only (Phase 8.4 deferred).

  **Recommended Agent Profile**: `quick`. **Parallel**: Wave 5 (with 22, 23). **Blocks**: 22, 23. **Blocked By**: 20.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1003-1024` (§13.1)

  **Acceptance Criteria**: [ ] API compiles. [ ] Sign returns signed report. [ ] Publish forwards to gossip.

  **QA Scenarios**:
  ```
  Scenario: API compiles
    Tool: Bash (cargo check)
    Steps: 1. cd p2p && cargo check -p foretias-server
    Evidence: .sisyphus/evidence/task-21-api-check.log
  ```

  **Commit**: YES (groups with 13)

- [ ] 22. FB emission (FullyBound established/lost)

  **What to do**: On FullyBound: emit ProbityReport attribute="fb", value=1.0. On lost: value=-1.0. Add `local_calendar_tbid` to executor. Failure → WARN log, no abort.

  **Recommended Agent Profile**: `unspecified-high`. **Parallel**: Wave 5 (after 21). **Blocks**: 24. **Blocked By**: 21.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1026-1058` (§§13.2-13.3)

  **Acceptance Criteria**: [ ] FB established: correct fields. [ ] FB lost: value=-1.0. [ ] Failure → WARN.

  **QA Scenarios**:
  ```
  Scenario: FB report built correctly
    Tool: Bash (cargo test)
    Steps: 1. Simulate FullyBound, 2. Assert attribute="fb", value=1.0, correct TBIDs
    Evidence: .sisyphus/evidence/task-22-fb-report.log
  ```

  **Commit**: YES (groups with 13)

- [ ] 23. FB reception (verify+ingest, full-signature gate)

  **What to do**: Gossip handler: parse → UnverifiedSignatureEnvelope → verify → CleanFullyAuthenticated → ingest (fb/gnf). Error → TRACE, drop. **CHECK: FB/GNF requires full → CleanFullyAuthenticated**.

  **Must NOT do**: Do NOT ingest without full verification. Do NOT panic.

  **Recommended Agent Profile**: `unspecified-high`. **Parallel**: Wave 5 (after 21). **Blocks**: 24. **Blocked By**: 21.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1059-1077` (§13.4)

  **Acceptance Criteria**:
  - [ ] FB → CleanFullyAuthenticated → ingest
  - [ ] Fast-only → rejected (negative test)
  - [ ] Malformed → dropped at TRACE

  **QA Scenarios**:
  ```
  Scenario: FB fast-only → rejected (TDD negative)
    Tool: Bash (cargo test)
    Steps: 1. FB with fast Ed25519, 2. Gossip → assert rejection, not ingested
    Evidence: .sisyphus/evidence/task-23-fb-fast-only-rejected.log
  ```

  **Commit**: YES (groups with 13)

- [ ] 24. Phase 8.4 trait stub + FB gossip integration tests

  **What to do**: Trait stub for Calendar signing. Integration test (toppoli): 2 servers, A FullyBound with B, B's ProbityStore contains FB from A. Unit tests: emit_fb_established, emit_fb_lost, verify.

  **Recommended Agent Profile**: `quick`. **Parallel**: Wave 5 (final). **Blocks**: F1-F4. **Blocked By**: 22, 23.

  **References**: `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md:1079-1093` (§13.5)

  **Acceptance Criteria**: [ ] Stub compiles. [ ] Unit tests pass. [ ] Integration test: FB propagates.

  **QA Scenarios**:
  ```
  Scenario: FB propagates between two servers (toppoli)
    Tool: Bash (cargo test --test toppoli)
    Steps: 1. 2 peers, 2. A FullyBound with B, 3. B ProbityStore has FB from A (5s)
    Evidence: .sisyphus/evidence/task-24-fb-toppoli.log
  ```

  **Commit**: YES (groups with 13)

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Present results to user, get explicit "okay".
> **Do NOT auto-proceed after verification. Wait for user's explicit approval.**

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read plan end-to-end. For each "Must Have": verify implementation exists. For each "Must NOT Have": search for forbidden patterns. Check evidence files exist. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo check --workspace` + `cargo test --workspace`. Review changed files for: `as any`/`@ts-ignore`, empty catches, console.log, unused imports. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Tests [N pass/N fail] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high`
  Execute EVERY QA scenario from EVERY task — follow exact steps, capture evidence. Test cross-task integration. Test edge cases: empty state, invalid input. Save to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff. Verify 1:1 — everything in spec built, nothing beyond spec built. Check "Must NOT do" compliance. Detect cross-task contamination.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | VERDICT`

---

## Commit Strategy

- **16a**: `feat(core): Group 7 Phase 16a — postcard canonical encoding, UnverifiedSignatureEnvelope rename, RecordBase trait, signature wrappers`
- **16b**: `feat(core): Group 7 Phase 16b — gate enforcement, Externalized builder, Foretis sig_input v2`
- **17**: `test(core): Group 7 Phase 17 — StrawmanSuite + TinmanSuite trust-boundary check suites`
- **15**: `feat(server): Group 7 Phase 15 — FamilyRecord + Family Cache`
- **13**: `feat(server): Group 7 Phase 13 — FB gossip emission/reception`

---

## Success Criteria

### Verification Commands
```bash
# After each phase merge to alpha:
export CMAKE_BUILD_PARALLEL_LEVEL=10
cd p2p && cargo test --workspace
cd p2p && cargo test -p foretias-server --test toppoli -- --include-ignored

# Snapshot update (when intentionally adding new call sites):
UPDATE_SNAPSHOT=1 cargo test -p foretias-core --test trust_boundary_type_usage
```

### Final Checklist
- [ ] All "Must Have" present
- [ ] All "Must NOT Have" absent
- [ ] All CHECK boxes have negative tests
- [ ] All phases merged to alpha with green tests
- [ ] Worktree cleaned up (`git worktree remove`)
