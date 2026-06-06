# IMPROVE_BUILDERS_AND_BIG_FNS_PLAN.md

**Plan: Improve Builders and Big Functions**
**Paired Spec: IMPROVE_BUILDERS_AND_BIG_FNS_SPEC.md**
**Status: PROPOSED**
**Date: 2026-06-05**
**BRANCH_NAME: improve-builders-and-big-fns**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/IMPROVE_BUILDERS_####**

---

## TODOs

### Wave 0: Setup

- [ ] Create worktree `git worktree add -b improve-builders-and-big-fns ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [ ] Verify baseline: `cargo test --workspace` passes (266 lib+bin tests)
- [ ] Add `bon = "3.9"` to `p2p/core-engine/Cargo.toml` and `p2p/foretias-server/Cargo.toml`
- [ ] Verify `cargo build --workspace` still compiles with new dependency

### Wave 1: ChrononRecord Builder

- [x] Add `#[derive(Builder)]` to `ChrononRecord` with `#[builder(finish_fn(vis = "", name = build_internal))]`
      (2026-06-06 00:00)
- [x] Add `#[builder(default)]` for optional fields: `signature_algorithm`, `chronon_stamp_count`, `external_attestations`, `tb_version`, `tbid`
      (2026-06-06 00:00)
- [x] Implement custom `build()` on `ChrononRecordBuilder<S: IsComplete>` returning `Result<ChrononRecord, NodeError>`
      (2026-06-06 00:00)
- [x] Write test: `bon_builder_local_construction` — build with all required fields
      (2026-06-06 00:00)
- [x] Write test: `bon_builder_defaults_applied` — optional fields get defaults
      (2026-06-06 00:00)
- [x] Write test: `bon_builder_validation_zero_chronon` — build with chronon_number=0 → error
      (2026-06-06 00:00)
- [x] Write test: `bon_builder_validation_empty_pubkey` — build with empty public_key → error
      (2026-06-06 00:00)
- [x] Verify: `ChrononRecord` still derives `Deserialize` (strict, no changes)
      (2026-06-06 00:00)
- [x] Run `cargo test -p foretias-core` — all existing tests still pass (283 tests)
      (2026-06-06 00:00)
- [x] Commit: "ChrononRecord: add bon builder with fallible build"
      (2026-06-06 00:00)

### Wave 2: Chronomatter Uses Builder

- [ ] Replace struct literal at `chronomatter/mod.rs:310` with `ChrononRecord::builder()...build()?`
- [ ] Replace struct literal at `chronomatter/mod.rs:335` with `ChrononRecord::builder()...build()?`
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "Chronomatter: use ChrononRecord builder instead of struct literal"

### Wave 3: Foretis + ExternalAttestation Builders

- [ ] Rename `Foretis` → `ForetisRecord` in `tick.rs` and all call sites
- [ ] Rename `ExternalAttestation` → `ExternalAttestationRecord` in `external_attestation.rs` and all call sites
- [ ] Add `#[derive(Builder)]` to `ForetisRecord` with fallible `build()`
- [ ] Add `#[derive(Builder)]` to `ExternalAttestationRecord` with fallible `build()`
- [ ] Write test: `foretis_builder_validation` — build with chronon_number=0 → error
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "ForetisRecord + ExternalAttestationRecord: rename + add bon builders"

### Wave 4: BaseRecord Rename + Remaining Types

- [ ] Rename `RecordBase` → `BaseRecord` in `clean_auth.rs` and all call sites
- [ ] Rename `EpochSnapshot` → `EpochSnapshotRecord` in `epoch/snapshot.rs` and all call sites
- [ ] Rename `ProbityReport` → `ProbityReportRecord` in `probity/report.rs` and all call sites
- [ ] Implement `BaseRecord` for `ForetisRecord` and `ExternalAttestationRecord`
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "BaseRecord rename + implement for all Record types"

### Wave 5: Function Builders

- [ ] Convert `gossip_event_loop` to `#[bon::builder]` function builder, remove `GossipLoopConfig` struct
- [ ] Update call site at `communerd/mod.rs` to use builder chain
- [ ] Convert `refresh_self_registration` to `#[bon::builder]` function builder, remove `RegistrationConfig` struct
- [ ] Update call site at `communerd/mod.rs` to use builder chain
- [ ] Convert `cmd_serve` to `#[bon::builder]` function builder, remove `ServeConfig` struct
- [ ] Update call site at `main.rs` to use builder chain
- [ ] Run `cargo test --workspace` — all tests still pass
- [ ] Commit: "Config: bon function builders for large functions"

### Wave 6: Struct Builders for Config Types

- [ ] Add `#[derive(Builder)]` to `TimeFamilyCliConfig` with `#[builder(default)]` for optional fields
- [ ] Add `#[derive(Builder)]` to `VerifyConfig` with `#[builder(default)]` for optional fields
- [ ] Update call sites to use `StructName::builder().field(value).build()` syntax
- [ ] Run `cargo test --workspace` — all tests still pass
- [ ] Commit: "Config structs: add bon struct builders"

### Wave 7: Migration — Replace Existing Constructors

- [ ] Find all `ChrononRecord::new(...)` call sites (grep for `ChrononRecord::new`)
- [ ] Replace each with `ChrononRecord::builder().field(value)...build()?`
- [ ] Find all `ForetisRecord::new(...)` call sites
- [ ] Replace each with `ForetisRecord::builder().field(value)...build()?`
- [ ] Remove the old `ChrononRecord::new()` and `ForetisRecord::new()` methods (or mark deprecated)
- [ ] Run `cargo test --workspace` — all tests still pass
- [ ] Commit: "Migrate constructors to bon builders"

### Wave 8: Final Verification

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo fmt --check` — no formatting changes needed
- [ ] Merge to alpha
