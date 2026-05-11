# CLEANUP_REPAIRS_PLAN.md

**Plan: Dead Code Removal, Deprecation Cleanup, Full Config Migration**
**Paired Spec: CLEANUP_REPAIRS_SPEC.md**
**Date: 2026-05-11**
**FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410**
**BRANCH_NAME=cleanup/repairs**

---

## Phase 0: Worktree Setup

- [ ] Create worktree `git worktree add -b cleanup/repairs /home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`
- [ ] `cd /home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`; reset current session work directory
- [ ] Verify baseline: `cargo build --workspace` and `cargo test --workspace` pass (from dad999d)

---

## Phase 1: Dead Code Removal (2 items)

### 1a. Remove `make_clock()` from tick.rs

- [ ] Remove `fn make_clock()` and its `#[allow(dead_code)]` from `p2p/core-engine/src/foretias/tick.rs` (~line 361)
- [ ] Verify no other code references `make_clock`
- [ ] `cargo test -p foretias-core -- tick` — confirm tests pass

### 1b. Narrow `#[allow(dead_code)]` on SoftwareCryptoServer

- [ ] Remove `#[allow(dead_code)]` from struct `SoftwareCryptoServer` declaration in `p2p/core-engine/src/crypto_server/software.rs` (~line 24)
- [ ] Add `#[allow(dead_code)]` only to `sphincs_sha2_256f_secret_key` and `mlkem_secret_key` fields
- [ ] `cargo build -p foretias-core` — verify zero dead_code warnings
- [ ] If other fields trigger warnings, investigate whether they're actually unused or false positives

---

## Phase 2: Remove Premature Deprecations (strip, don't migrate yet)

### 2a. Remove `#[deprecated]` from `NodeConfig`

- [ ] Remove `#[deprecated(since = "0.5.0", note = "Use TimeFamilyConfig instead")]` from `NodeConfig` struct in `p2p/core-engine/src/config/node.rs`
- [ ] Remove `#[allow(deprecated)]` from `p2p/core-engine/src/config/mod.rs` (re-export line)
- [ ] Remove `#[allow(deprecated)]` from `p2p/core-engine/src/config/time_family.rs` (use import + From impl)
- [ ] Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/main.rs` (module-level)
- [ ] Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/main.rs`
- [ ] Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/server/mod.rs` (module-level)
- [ ] Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/server/mod.rs`
- [ ] Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/communerd/mod.rs` (module-level)
- [ ] Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/communerd/mod.rs`
- [ ] Remove `#[allow(deprecated)]` from `p2p/foretias-python/src/lib.rs` (use import + From impl)

### 2b. Remove `#[deprecated]` from `auto_attestation_blob`

- [ ] Remove `#[deprecated(since = "0.9.0", ...)]` from `auto_attestation_blob` function in `p2p/core-engine/src/foretias/tick.rs`
- [ ] Remove `#[allow(deprecated)]` from `p2p/core-engine/src/foretias/mod.rs` (re-export line)
- [ ] Remove all `#[allow(deprecated)]` from test functions in `p2p/core-engine/src/foretias/tick.rs`
- [ ] Remove all `#[allow(deprecated)]` from test functions in `p2p/core-engine/src/foretias/calendar.rs`

### 2c. Verify clean baseline

- [ ] `cargo build --workspace` — zero warnings (no deprecation, no dead_code)
- [ ] `cargo test --workspace` — zero warnings, all tests pass
- [ ] **Checkpoint commit**: "Cleanup: remove premature deprecation markers and dead code allows"

---

## Phase 3: Full Config Migration (NodeConfig → TimeFamilyConfig)

### 3a. Refactor Communerd to accept sub-configs instead of NodeConfig

- [ ] Change `Communerd` struct field from `config: NodeConfig` to:
  ```rust
  auto_attest: AutoAttestConfig,
  collision: CollisionConfig,
  ```
- [ ] Change `Communerd::new(config: NodeConfig)` to:
  ```rust
  pub fn new(auto_attest: AutoAttestConfig, collision: CollisionConfig) -> Self
  ```
- [ ] Update `new()` internals to use `self.auto_attest` and `self.collision` instead of `self.config.xxx`
  - `request_timeout_secs` → `auto_attest.request_timeout_secs`
  - `peers` → `auto_attest.peers`
  - `collision.nonce_window` → `collision.nonce_window`
  - `collision.heartbeat_interval_secs` → `collision.heartbeat_interval_secs`
- [ ] Replace `pub fn config(&self) -> &NodeConfig` with:
  ```rust
  pub fn peers(&self) -> &[String] { &self.auto_attest.peers }
  pub fn auto_attest_config(&self) -> &AutoAttestConfig { &self.auto_attest }
  ```
- [ ] Update `Clone` impl to clone new fields
- [ ] Update test helper `make_config()` → construct sub-configs directly
- [ ] Remove `use foretias_core::config::NodeConfig` import (if no longer needed)
- [ ] `cargo build -p foretias-node` — verify

### 3b. Refactor TimeFamilyServer to accept sub-configs

- [ ] Change `fn new_with_config(..., config: Option<NodeConfig>)` to:
  ```rust
  fn new_with_config(
      ...,
      config: Option<(AutoAttestConfig, CollisionConfig)>,
  )
  ```
- [ ] Change `pub fn with_config(mut self, config: NodeConfig) -> Self` to:
  ```rust
  pub fn with_config(
      mut self,
      auto_attest: AutoAttestConfig,
      collision: CollisionConfig,
  ) -> Self
  ```
- [ ] Update `with_config` to pass sub-configs to `Communerd::new()`
- [ ] Remove `use foretias_core::config::NodeConfig` import
- [ ] `cargo build -p foretias-node` — verify

### 3c. Migrate main.rs (cmd_serve)

- [ ] Replace `NodeConfig` struct literal with sub-config extraction from `time_family_cfg`:
  ```rust
  let server = if !peers.is_empty() {
      let auto_attest = time_family_cfg.chronomatter.auto_attest.clone();
      let collision = time_family_cfg.p2p.collision.clone();
      Arc::new(server.with_config(auto_attest, collision))
  } else {
      Arc::new(server)
  };
  ```
- [ ] Update peer display code:
  ```rust
  if let Some(c) = server.communerd() {
      println!("  Peers  : {}", c.peers().join(", "));
      println!("  Auto Attest Every: {} chronons", c.auto_attest_config().every_n_chronons);
  }
  ```
- [ ] Remove `use foretias_core::config::NodeConfig` import from main.rs
- [ ] `cargo build -p foretias-node` — verify

### 3d. Migrate integration tests

- [ ] Update `test_two_nodes_auto_attest` in `p2p/foretias-node/tests/integration.rs`:
  - Replace `NodeConfig { ... }` with `(AutoAttestConfig { ... }, CollisionConfig::default())`
  - Update `server.with_config(config)` call
- [ ] Update `test_peer_unreachable_does_not_crash` in `p2p/foretias-node/tests/integration.rs`:
  - Same pattern
- [ ] Remove `use foretias_core::config::NodeConfig` imports
- [ ] `cargo test -p foretias-node` — verify

### 3e. Migrate foretias-python PyNodeConfig

- [ ] Change `PyNodeConfig` — keep Python-facing API flat (Option B from spec)
- [ ] Change `impl From<&NodeConfigInner> for PyNodeConfig` to `impl From<&TimeFamilyConfigInner> for PyNodeConfig`:
  - Map `listen_addr` → `p2p.listen_addr`
  - Map `version` → `version`
  - Map `calendar_path` → `calendars[0].persist_path`
  - Map `chronon_ns` → `chronomatter.chronon_ns`
  - Map `serialized` → (dropped, no equivalent)
  - Map `peers` → `chronomatter.auto_attest.peers`
  - Map `auto_attest_every_n` → `chronomatter.auto_attest.every_n_chronons`
  - Map `request_timeout_secs` → `chronomatter.auto_attest.request_timeout_secs`
  - Map `p2p_listen` → `p2p.p2p_listen`
  - Map `p2p_dial` → `p2p.p2p_dial`
  - Map `dht_namespace` → `p2p.dht.namespace`
  - Map `dht_bootstrap` → `p2p.dht.bootstrap`
  - Map `collision` → `p2p.collision`
- [ ] Update import: `use foretias_core::config::{TimeFamilyConfig as TimeFamilyConfigInner, CollisionConfig as CollisionConfigInner};`
- [ ] Remove `NodeConfig` import from foretias-python
- [ ] `cargo build -p foretias-python` — verify

### 3f. Verify NodeConfig is fully isolated

- [ ] Grep for `NodeConfig` across workspace — should only appear in:
  - `config/node.rs` (definition + impl + Default)
  - `config/time_family.rs` (From impl + use)
  - `config/mod.rs` (re-export)
  - **Nowhere else**
- [ ] `cargo build --workspace` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] **Checkpoint commit**: "Migration: replace NodeConfig runtime API with TimeFamilyConfig sub-configs"

---

## Phase 4: auto_attestation_blob Test Migration

### 4a. Migrate tick.rs tests

- [ ] Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in:
  - `verify_pair_valid_returns_true` (~line 487)
  - `verify_pair_tampered_returns_false` (~line 533)
  - `auto_attestation_blob_nonce_is_unique` (~line 590, 591)
  - `auto_attestation_blob_nonce_is_present` (~line 605)
- [ ] `cargo test -p foretias-core -- auto_attestation` — verify

### 4b. Migrate calendar.rs tests

- [ ] Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in:
  - `integrity_check_full_chain` (~line 316, 320)
  - `integrity_check_partial_range` (~line 368, 372)
- [ ] Update imports: `use crate::foretias::tick::auto_attestation_blob_with_count;`
- [ ] `cargo test -p foretias-core -- integrity_check` — verify

### 4c. Keep deprecated wrapper (no deprecation marker)

- [ ] `auto_attestation_blob` remains as a thin wrapper (no `#[deprecated]`, no warnings)
- [ ] Re-export in `foretias/mod.rs` remains (no `#[allow(deprecated)]` needed)

### 4d. Final verification

- [ ] `cargo build --workspace` — zero warnings
- [ ] `cargo test --workspace` — zero warnings, all tests pass
- [ ] Grep for `#\[allow(` across workspace — should be **zero** (except bindgen injection in build.rs)
- [ ] Grep for `deprecated` across workspace — should be **zero** (both markers removed)

---

## Phase 5: Merge & Cleanup

- [ ] Verify all work is complete in `/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410` and committed to `cleanup/repairs`
- [ ] Merge `cleanup/repairs` to alpha
  - [ ] `cd /home/hcbusy/webhash/foretias && git merge cleanup/repairs --no-ff -m "Major: Cleanup Repairs, Phase: Complete...opencode 1.14.39, Qwen3.6-27B-AWQ-BF16-INT4"`
- [ ] Final workspace verification:
  - [ ] `cargo build --workspace` — zero warnings
  - [ ] `cargo test --workspace` — all pass, zero warnings
- [ ] Cleanup `/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`
  - [ ] Check that _PLAN.md has all but Cleanup checkboxes completed
  - [ ] Remove "/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410" via `git worktree remove`
  - [ ] This is the last checkbox to be checked in my _PLAN.md
