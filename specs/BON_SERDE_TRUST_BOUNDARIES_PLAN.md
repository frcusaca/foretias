# BON_SERDE_TRUST_BOUNDARIES_PLAN.md

**Plan: bon Builders + Permissive Serde for Trust Boundary Types**
**Paired Spec: BON_SERDE_TRUST_BOUNDARIES_SPEC.md**
**Status: PROPOSED**
**Date: 2026-06-04**
**BRANCH_NAME: bon-serde-trust-boundaries**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/BON_SERDE_####**

---

## TODOs

### Wave 0: Setup

- [ ] Create worktree `git worktree add -b bon-serde-trust-boundaries ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
- [ ] Verify baseline: `cargo test --workspace` passes (266 lib+bin tests)
- [ ] Add `bon = "3.9"` to `p2p/core-engine/Cargo.toml` and `p2p/foretias-server/Cargo.toml`
- [ ] Verify `cargo build --workspace` still compiles with new dependency

### Wave 1: ChrononRecord Shadow Type + TryFrom + Custom Deserialize

- [ ] Create `ChrononRecordUnchecked` struct in `p2p/core-engine/src/foretias/tick.rs` with all fields as `Option<T>` and matching `#[serde]` attributes
- [ ] Implement `TryFrom<ChrononRecordUnchecked> for ChrononRecord` with validation logic (matching existing `ChrononRecord::new()` invariants)
- [ ] Replace `#[derive(Deserialize)]` on `ChrononRecord` with custom `impl<'de> Deserialize<'de>` routing through `ChrononRecordUnchecked`
- [ ] Write test: `permissive_parsing_missing_optional_fields` — JSON with only required fields deserializes with defaults
- [ ] Write test: `missing_required_field_rejected` — missing `public_key` → clear error
- [ ] Write test: `zero_chronon_number_rejected` — `chronon_number = 0` → validation error
- [ ] Write test: `empty_json_rejected` — `{}` → error
- [ ] Write test: `tick_number_rename_works` — `tick_number` JSON key maps to `chronon_number`
- [ ] Write test: `serialize_uses_tick_number` — output uses `tick_number` not `chronon_number`
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "ChrononRecord: shadow type + TryFrom + custom Deserialize"

### Wave 2: Foretis Shadow Type + TryFrom + Custom Deserialize

- [ ] Create `ForetisUnchecked` struct with all fields as `Option<T>`
- [ ] Implement `TryFrom<ForetisUnchecked> for Foretis` with validation (chronon_number > 0)
- [ ] Replace `#[derive(Deserialize)]` on `Foretis` with custom impl
- [ ] Write test: `foretis_serde_roundtrip` — serialize → deserialize preserves all fields
- [ ] Write test: `foretis_missing_required_fields` — missing `content_hash` → error
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "Foretis: shadow type + TryFrom + custom Deserialize"

### Wave 3: ExternalAttestation Shadow Type + TryFrom + Custom Deserialize

- [ ] Create `ExternalAttestationUnchecked` struct with all fields as `Option<T>`, nesting `ForetisUnchecked` and `ChrononRecordUnchecked`
- [ ] Implement `TryFrom<ExternalAttestationUnchecked> for ExternalAttestation` with recursive validation
- [ ] Replace `#[derive(Deserialize)]` on `ExternalAttestation` with custom impl
- [ ] Write test: `external_attestation_permissive_parsing` — nested records with missing optional fields
- [ ] Write test: `external_attestation_missing_nested_required` — missing `public_key` in nested `attester_tick_record`
- [ ] Write test: `chronon_record_with_nested_external_attestations` — full round-trip with nested attestations
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "ExternalAttestation: shadow type + TryFrom + custom Deserialize"

### Wave 4: ChrononRecord bon Builder

- [ ] Add `#[derive(Builder)]` to `ChrononRecord` with `#[builder(finish_fn(vis = "", name = build_internal))]`
- [ ] Add `#[builder(default)]` for optional fields: `signature_algorithm`, `chronon_stamp_count`, `external_attestations`, `tb_version`, `tbid`
- [ ] Implement custom `build()` on `ChrononRecordBuilder<S: IsComplete>` returning `Result<ChrononRecord, NodeError>`
- [ ] Write test: `bon_builder_local_construction` — build with all required fields
- [ ] Write test: `bon_builder_defaults_applied` — optional fields get defaults
- [ ] Write test: `bon_builder_validation_zero_chronon` — build with chronon_number=0 → error
- [ ] Write test: `bon_builder_validation_empty_pubkey` — build with empty public_key → error
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "ChrononRecord: add bon builder with fallible build"

### Wave 5: Foretis + ExternalAttestation bon Builders

- [ ] Add `#[derive(Builder)]` to `Foretis` with fallible `build()`
- [ ] Add `#[derive(Builder)]` to `ExternalAttestation` with fallible `build()`
- [ ] Write test: `foretis_builder_validation` — build with chronon_number=0 → error
- [ ] Run `cargo test -p foretias-core` — all existing tests still pass
- [ ] Commit: "Foretis + ExternalAttestation: add bon builders"

### Wave 6: Config Struct Builders

**Function builders** (all params available at call site — use `#[builder] fn`):

- [ ] Convert `gossip_event_loop` to `#[bon::builder]` function builder, remove `GossipLoopConfig` struct
- [ ] Update call site at `communerd/mod.rs:657` to use `.events(events).cmd_tx(cmd_tx)...call().await`
- [ ] Convert `refresh_self_registration` to `#[bon::builder]` function builder, remove `RegistrationConfig` struct
- [ ] Update call site at `communerd/mod.rs:1123` to use builder chain
- [ ] Convert `cmd_serve` to `#[bon::builder]` function builder, remove `ServeConfig` struct
- [ ] Update call site at `main.rs:837` to use builder chain

**Struct builders** (built gradually — use `#[derive(Builder)]`):

- [ ] Add `#[derive(Builder)]` to `TimeFamilyCliConfig` with `#[builder(default)]` for optional fields
- [ ] Add `#[derive(Builder)]` to `VerifyConfig` with `#[builder(default)]` for optional fields
- [ ] Update call sites to use `StructName::builder().field(value).build()` syntax

- [ ] Run `cargo test --workspace` — all tests still pass
- [ ] Commit: "Config: bon function builders + struct builders"

### Wave 7: Migration — Replace Existing Constructors

- [ ] Find all `ChrononRecord::new(...)` call sites (grep for `ChrononRecord::new`)
- [ ] Replace each with `ChrononRecord::builder().field(value)...build()?`
- [ ] Find all `Foretis::new(...)` call sites
- [ ] Replace each with `Foretis::builder().field(value)...build()?`
- [ ] Remove the old `ChrononRecord::new()` and `Foretis::new()` methods (or mark deprecated)
- [ ] Run `cargo test --workspace` — all tests still pass
- [ ] Commit: "Migrate constructors to bon builders"

### Wave 8: Final Verification

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo fmt --check` — no formatting changes needed
- [ ] Verify wire format compatibility: serialize a `ChrononRecord`, deserialize with old code, serialize with new code, compare JSON
- [ ] Merge to alpha
