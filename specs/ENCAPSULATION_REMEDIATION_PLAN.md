# ENCAPSULATION_REMEDIATION_PLAN.md

**Plan: Encapsulation and Ownership Remediation**
**Paired Spec: ENCAPSULATION_REMEDIATION_SPEC.md**
**Status: PROPOSED**
**Date: 2026-06-07**
**BRANCH_NAME: encapsulation-remediation**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/ENCAPSULATION_####**

---

## TODOs

### Wave 0: Setup

- [x] Create worktree `git worktree add -b encapsulation-remediation ${FULL_WORKTREE_PATH}`
- [x] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [x] Verify baseline: `cargo test --workspace` passes
- [x] Run `cargo clippy --workspace --all-targets` — confirm zero warnings

### Wave 1: ChrononRecord Fields → `pub(crate)`

- [x] Change all 10 ChrononRecord fields from `pub` to `pub(crate)` in `core-engine/src/foretias/tick.rs`
- [x] Fix direct field access in `core-engine/src/chronomatter/mod.rs` (~10 sites)
- [x] Fix direct field access in `core-engine/src/foretias/calendar.rs` (~10 sites)
- [x] Fix direct field access in `core-engine/src/foretias/tick.rs` (~15 sites)
- [x] Fix direct field access in `core-engine/src/foretias/clean_auth.rs` (~10 sites)
- [x] Fix direct field access in `foretias-server/src/` (~20 sites)
- [x] Fix direct field access in `foretias-client/src/` (~5 sites)
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "ChrononRecord: make fields pub(crate), fix access sites"

### Wave 2: ForetisRecord Fields → `pub(crate)`

- [x] Change all 6 ForetisRecord fields from `pub` to `pub(crate)` in `core-engine/src/foretias/tick.rs`
- [x] Fix direct field access in `core-engine/src/` (~10 sites)
- [x] Fix direct field access in `foretias-server/src/` (~10 sites)
- [x] Fix direct field access in `foretias-client/src/` (~5 sites)
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "ForetisRecord: make fields pub(crate), fix access sites"

### Wave 3: ExternalAttestationRecord + StampedForetis + Heartbeat + FamilyRecord

- [x] Add accessor methods to ExternalAttestationRecord (6 fields)
- [x] Add accessor methods to StampedForetis (3 fields)
- [x] Add accessor methods to Heartbeat (5 fields)
- [x] Add accessor methods to FamilyRecord (2 fields + matrix)
- [x] Make ExternalAttestationRecord fields `pub(crate)`
- [x] Make StampedForetis fields `pub(crate)`
- [x] Make Heartbeat fields `pub(crate)`
- [x] Make FamilyRecord fields `pub(crate)`
- [x] Fix all direct field access sites
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "ExternalAttestationRecord, StampedForetis, Heartbeat, FamilyRecord: private fields + accessors"

### Wave 4: Calendar

- [x] Add accessor methods to Calendar: `ticks()`, `tick_at(n)`, `latest_tick()`, `tick_count()`, `tbid()`, `tbn()`, `stamp_tbid()`
- [x] Make Calendar fields `pub(crate)`
- [x] Fix all direct `ticks` access sites in `core-engine/src/`
- [x] Fix all direct `ticks` access sites in `foretias-server/src/`
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "Calendar: private fields + accessor methods"

### Wave 5: WorkerContext + PeerRegistrationRecord + MirrorState + LivenessCycleFlags

- [x] Make WorkerContext fields `pub(crate)`, add accessor methods
- [x] Make `signing_key` private, add `fn sign_with_calendar_key()` method
- [x] Make `communerd` private, expose only `CommunerdetteLine` via accessor
- [x] Make PeerRegistrationRecord fields `pub(crate)`, add validated constructor
- [x] Make MirrorState fields private, add accessor methods with invariant enforcement
- [x] Make LivenessCycleFlags fields private, add setter methods that log transitions
- [x] Fix all direct field access sites
- [x] Run `cargo test -p foretias-server --lib --bins`
- [x] Commit: "WorkerContext, PeerRegistrationRecord, MirrorState, LivenessCycleFlags: private fields"

### Wave 6: SignatureEntry + ProbityReportRecord + EpochSnapshotRecord

- [x] Make SignatureEntry fields `pub(crate)` in `core-engine/src/foretias/clean_auth.rs`
- [x] Make ProbityReportRecord fields `pub(crate)` in `core-engine/src/probity/report.rs`
- [x] Make EpochSnapshotRecord fields `pub(crate)` in `core-engine/src/epoch/snapshot.rs`
- [x] Fix direct access sites
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "SignatureEntry, ProbityReportRecord, EpochSnapshotRecord: pub(crate) fields"

### Wave 6: SoftwareCryptoServer + Lower Risk Types

- [x] Add accessor methods to SoftwareCryptoServer (4 PQC key fields)
- [x] Make SoftwareCryptoServer fields `pub(crate)`
- [x] Fix direct access sites
- [x] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [x] Commit: "SoftwareCryptoServer: private fields + accessors"

### Wave 7: Final Verification

- [x] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [x] `cargo test --workspace` — all tests pass
- [x] `cargo fmt --check` — no formatting changes needed
- [x] Merge to alpha
