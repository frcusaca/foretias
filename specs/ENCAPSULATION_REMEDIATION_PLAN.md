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

- [ ] Create worktree `git worktree add -b encapsulation-remediation ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [ ] Verify baseline: `cargo test --workspace` passes
- [ ] Run `cargo clippy --workspace --all-targets` — confirm zero warnings

### Wave 1: ChrononRecord Fields → `pub(crate)`

- [ ] Change all 10 ChrononRecord fields from `pub` to `pub(crate)` in `core-engine/src/foretias/tick.rs`
- [ ] Fix direct field access in `core-engine/src/chronomatter/mod.rs` (~10 sites)
- [ ] Fix direct field access in `core-engine/src/foretias/calendar.rs` (~10 sites)
- [ ] Fix direct field access in `core-engine/src/foretias/tick.rs` (~15 sites)
- [ ] Fix direct field access in `core-engine/src/foretias/clean_auth.rs` (~10 sites)
- [ ] Fix direct field access in `foretias-server/src/` (~20 sites)
- [ ] Fix direct field access in `foretias-client/src/` (~5 sites)
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "ChrononRecord: make fields pub(crate), fix access sites"

### Wave 2: ForetisRecord Fields → `pub(crate)`

- [ ] Change all 6 ForetisRecord fields from `pub` to `pub(crate)` in `core-engine/src/foretias/tick.rs`
- [ ] Fix direct field access in `core-engine/src/` (~10 sites)
- [ ] Fix direct field access in `foretias-server/src/` (~10 sites)
- [ ] Fix direct field access in `foretias-client/src/` (~5 sites)
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "ForetisRecord: make fields pub(crate), fix access sites"

### Wave 3: ExternalAttestationRecord + StampedForetis + Heartbeat + FamilyRecord

- [ ] Add accessor methods to ExternalAttestationRecord (6 fields)
- [ ] Add accessor methods to StampedForetis (3 fields)
- [ ] Add accessor methods to Heartbeat (5 fields)
- [ ] Add accessor methods to FamilyRecord (2 fields + matrix)
- [ ] Make ExternalAttestationRecord fields `pub(crate)`
- [ ] Make StampedForetis fields `pub(crate)`
- [ ] Make Heartbeat fields `pub(crate)`
- [ ] Make FamilyRecord fields `pub(crate)`
- [ ] Fix all direct field access sites
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "ExternalAttestationRecord, StampedForetis, Heartbeat, FamilyRecord: private fields + accessors"

### Wave 4: Calendar

- [ ] Add accessor methods to Calendar: `ticks()`, `tick_at(n)`, `latest_tick()`, `tick_count()`, `tbid()`, `tbn()`, `stamp_tbid()`
- [ ] Make Calendar fields `pub(crate)`
- [ ] Fix all direct `ticks` access sites in `core-engine/src/`
- [ ] Fix all direct `ticks` access sites in `foretias-server/src/`
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "Calendar: private fields + accessor methods"

### Wave 5: WorkerContext + PeerRegistrationRecord + MirrorState + LivenessCycleFlags

- [ ] Make WorkerContext fields `pub(crate)`, add accessor methods
- [ ] Make `signing_key` private, add `fn sign_with_calendar_key()` method
- [ ] Make `communerd` private, expose only `CommunerdetteLine` via accessor
- [ ] Make PeerRegistrationRecord fields `pub(crate)`, add validated constructor
- [ ] Make MirrorState fields private, add accessor methods with invariant enforcement
- [ ] Make LivenessCycleFlags fields private, add setter methods that log transitions
- [ ] Fix all direct field access sites
- [ ] Run `cargo test -p foretias-server --lib --bins`
- [ ] Commit: "WorkerContext, PeerRegistrationRecord, MirrorState, LivenessCycleFlags: private fields"

### Wave 6: SignatureEntry + ProbityReportRecord + EpochSnapshotRecord

- [ ] Make SignatureEntry fields `pub(crate)` in `core-engine/src/foretias/clean_auth.rs`
- [ ] Make ProbityReportRecord fields `pub(crate)` in `core-engine/src/probity/report.rs`
- [ ] Make EpochSnapshotRecord fields `pub(crate)` in `core-engine/src/epoch/snapshot.rs`
- [ ] Fix direct access sites
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "SignatureEntry, ProbityReportRecord, EpochSnapshotRecord: pub(crate) fields"

### Wave 6: SoftwareCryptoServer + Lower Risk Types

- [ ] Add accessor methods to SoftwareCryptoServer (4 PQC key fields)
- [ ] Make SoftwareCryptoServer fields `pub(crate)`
- [ ] Fix direct access sites
- [ ] Run `cargo test -p foretias-core -p foretias-client -p foretias-server --lib --bins`
- [ ] Commit: "SoftwareCryptoServer: private fields + accessors"

### Wave 7: Final Verification

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo fmt --check` — no formatting changes needed
- [ ] Merge to alpha
