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

- [x] Create worktree `git worktree add -b improve-builders-and-big-fns ${FULL_WORKTREE_PATH}`
      (2026-06-06 14:32)
- [x] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
      (2026-06-06 14:32)
- [x] Verify baseline: `cargo test --workspace` passes (266 lib+bin tests)
      (2026-06-06 14:32)
- [x] Add `bon = "3.9"` to `p2p/core-engine/Cargo.toml` and `p2p/foretias-server/Cargo.toml`
      (2026-06-06 14:32)
- [x] Verify `cargo build --workspace` still compiles with new dependency
      (2026-06-06 14:32)

### Wave 1: ChrononRecord Builder

- [x] Add `#[derive(Builder)]` to `ChrononRecord` with `#[builder(finish_fn(vis = "", name = build_internal))]`
      (2026-06-06 14:40)
- [x] Add `#[builder(default)]` for optional fields: `signature_algorithm`, `chronon_stamp_count`, `external_attestations`, `tb_version`, `tbid`
      (2026-06-06 14:40)
- [x] Implement custom `build()` on `ChrononRecordBuilder<S: IsComplete>` returning `Result<ChrononRecord, NodeError>`
      (2026-06-06 14:40)
- [x] Write test: `bon_builder_local_construction` — build with all required fields
      (2026-06-06 14:40)
- [x] Write test: `bon_builder_defaults_applied` — optional fields get defaults
      (2026-06-06 14:40)
- [x] Write test: `bon_builder_validation_zero_chronon` — build with chronon_number=0 → error
      (2026-06-06 14:40)
- [x] Write test: `bon_builder_validation_empty_pubkey` — build with empty public_key → error
      (2026-06-06 14:40)
- [x] Verify: `ChrononRecord` still derives `Deserialize` (strict, no changes)
      (2026-06-06 14:40)
- [x] Run `cargo test -p foretias-core` — all existing tests still pass
      (2026-06-06 14:40)
- [x] Commit: "ChrononRecord: add bon builder with fallible build"
      (2026-06-06 14:40)

### Wave 2: Chronomatter Uses Builder

- [x] Replace struct literal at `chronomatter/mod.rs:310` with `ChrononRecord::builder()...build()?`
      (2026-06-06 14:48)
- [x] Replace struct literal at `chronomatter/mod.rs:335` with `ChrononRecord::builder()...build()?`
      (2026-06-06 14:48)
- [x] Run `cargo test -p foretias-core` — all existing tests still pass
      (2026-06-06 14:48)
- [x] Commit: "Chronomatter: use ChrononRecord builder instead of struct literal"
      (2026-06-06 14:48)

### Wave 3: Foretis + ExternalAttestation Builders

- [x] Rename `Foretis` → `ForetisRecord` in `tick.rs` and all call sites
      (2026-06-06 15:05)
- [x] Rename `ExternalAttestation` → `ExternalAttestationRecord` in `external_attestation.rs` and all call sites
      (2026-06-06 15:05)
- [x] Add `#[derive(Builder)]` to `ForetisRecord` with fallible `build()`
      (2026-06-06 15:05)
- [x] Add `#[derive(Builder)]` to `ExternalAttestationRecord` with fallible `build()`
      (2026-06-06 15:05)
- [x] Write test: `foretis_builder_validation` — build with chronon_number=0 → error
      (2026-06-06 15:05)
- [x] Run `cargo test -p foretias-core` — all existing tests still pass
      (2026-06-06 15:05)
- [x] Commit: "ForetisRecord + ExternalAttestationRecord: rename + add bon builders"
      (2026-06-06 15:05)

### Wave 4: BaseRecord Rename + Remaining Types

- [x] Rename `RecordBase` → `BaseRecord` in `clean_auth.rs` and all call sites
      (2026-06-06 15:25)
- [x] Rename `EpochSnapshot` → `EpochSnapshotRecord` in `epoch/snapshot.rs` and all call sites
      (2026-06-06 15:25)
- [x] Rename `ProbityReport` → `ProbityReportRecord` in `probity/report.rs` and all call sites
      (2026-06-06 15:25)
- [x] Implement `BaseRecord` for `ForetisRecord` and `ExternalAttestationRecord`
      (2026-06-06 15:25)
- [x] Run `cargo test -p foretias-core` — all existing tests still pass
      (2026-06-06 15:25)
- [x] Commit: "BaseRecord rename + implement for all Record types"
      (2026-06-06 15:25)

### Wave 5: Function Builders

- [x] Convert `gossip_event_loop` to `#[bon::builder]` function builder, remove `GossipLoopConfig` struct
      (2026-06-06 15:50) [REVISED: Added #[derive(Builder)] to GossipLoopConfig instead - async fn builder incompatible]
- [x] Update call site at `communerd/mod.rs` to use builder chain
      (2026-06-06 15:50)
- [x] Convert `refresh_self_registration` to `#[bon::builder]` function builder, remove `RegistrationConfig` struct
      (2026-06-06 15:50) [REVISED: Added #[derive(Builder)] to RegistrationConfig, call site uses builder]
- [x] Update call site at `communerd/mod.rs` to use builder chain
      (2026-06-06 15:50)
- [x] Convert `cmd_serve` to `#[bon::builder]` function builder, remove `ServeConfig` struct
      (2026-06-06 15:50) [REVISED: Added #[derive(Builder)] to ServeConfig + VerifyConfig]
- [x] Update call site at `main.rs` to use builder chain
      (2026-06-06 15:50)
- [x] Run `cargo test --workspace` — all tests still pass
      (2026-06-06 15:50)
- [x] Commit: "Config: bon function builders for large functions"
      (2026-06-06 15:50)

### Wave 6: Struct Builders for Config Types

- [x] Add `#[derive(Builder)]` to `TimeFamilyCliConfig` with `#[builder(default)]` for optional fields
      (2026-06-06 16:05)
- [x] Add `#[derive(Builder)]` to `VerifyConfig` with `#[builder(default)]` for optional fields
      (2026-06-06 16:05) [Done in Wave 5 revised]
- [x] Update call sites to use `StructName::builder().field(value).build()` syntax
      (2026-06-06 16:05)
- [x] Run `cargo test --workspace` — all tests still pass
      (2026-06-06 16:05)
- [x] Commit: "Config structs: add bon struct builders"
      (2026-06-06 16:05)

### Wave 7: Migration — Replace Existing Constructors

- [x] Find all `ChrononRecord::new(...)` call sites (grep for `ChrononRecord::new`)
      (2026-06-06 16:15)
- [x] Replace each with `ChrononRecord::builder().field(value)...build()?`
      (2026-06-06 16:15)
- [x] Find all `ForetisRecord::new(...)` call sites
      (2026-06-06 16:15)
- [x] Replace each with `ForetisRecord::builder().field(value)...build()?`
      (2026-06-06 16:15)
- [x] Remove the old `ChrononRecord::new()` and `ForetisRecord::new()` methods (or mark deprecated)
      (2026-06-06 16:15)
- [x] Run `cargo test --workspace` — all tests still pass
      (2026-06-06 16:15)
- [x] Commit: "Migrate constructors to bon builders"
      (2026-06-06 16:15)

### Wave 8: Final Verification

- [x] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
      (2026-06-06 16:30)
- [x] `cargo test --workspace` — all tests pass
      (2026-06-06 16:30)
- [x] `cargo fmt --check` — no formatting changes needed
      (2026-06-06 16:30)
- [x] Merge to alpha
      (2026-06-06 16:30)
