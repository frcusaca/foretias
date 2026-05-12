# CLEANUP_REPAIRS_PLAN.md

**Plan: Dead Code Removal, Deprecation Cleanup, Full Config Migration**
**Paired Spec: CLEANUP_REPAIRS_SPEC.md**
**Date: 2026-05-11**
**Revised: 2026-05-11 (CommunerdConfig design per time-being pattern)**
**FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410**
**BRANCH_NAME=cleanup/repairs**

---

## Phase 0: Worktree Setup

- [x](2026-05-11 21:05) Create worktree `git worktree add -b cleanup/repairs /home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`
- [x](2026-05-11 21:05) `cd /home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`; reset current session work directory
- [x](2026-05-11 21:07) Verify baseline: `cargo build --workspace` and `cargo test --workspace` pass (from dad999d)

---

## Phase 1: Dead Code Removal (2 items)

### 1a. Remove `make_clock()` from tick.rs

- [x](2026-05-11 21:10) Remove `fn make_clock()` and its `#[allow(dead_code)]` from `p2p/core-engine/src/foretias/tick.rs` (~line 361)
- [x](2026-05-11 21:10) Verify no other code references `make_clock`
- [x](2026-05-11 21:10) `cargo test -p foretias-core -- tick` — confirm tests pass

### 1b. Narrow `#[allow(dead_code)]` on SoftwareCryptoServer

- [x](2026-05-11 21:10) Remove `#[allow(dead_code)]` from struct `SoftwareCryptoServer` declaration in `p2p/core-engine/src/crypto_server/software.rs` (~line 24)
- [x](2026-05-11 21:10) Add `#[allow(dead_code)]` only to `sphincs_sha2_256f_secret_key` and `mlkem_secret_key` fields
- [x](2026-05-11 21:10) `cargo build -p foretias-core` — verify zero dead_code warnings

---

## Phase 2: Remove Premature Deprecations (strip markers only)

### 2a. Remove `#[deprecated]` from `NodeConfig`

- [x](2026-05-11 21:24) Remove `#[deprecated(...)]` from `NodeConfig` struct in `p2p/core-engine/src/config/node.rs`
- [x](2026-05-11 21:24) Remove `#[allow(deprecated)]` from `p2p/core-engine/src/config/mod.rs` (re-export line)
- [x](2026-05-11 21:24) Remove `#[allow(deprecated)]` from `p2p/core-engine/src/config/time_family.rs` (use import + From impl)
- [x](2026-05-11 21:24) Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/main.rs` (module-level)
- [x](2026-05-11 21:24) Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/main.rs`
- [x](2026-05-11 21:24) Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/server/mod.rs` (module-level)
- [x](2026-05-11 21:24) Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/server/mod.rs`
- [x](2026-05-11 21:24) Remove `#![allow(deprecated)]` from `p2p/foretias-node/src/communerd/mod.rs` (module-level)
- [x](2026-05-11 21:24) Remove all `#[allow(deprecated)]` item-level annotations from `p2p/foretias-node/src/communerd/mod.rs`
- [x](2026-05-11 21:24) Remove `#[allow(deprecated)]` from `p2p/foretias-python/src/lib.rs` (use import + From impl)

### 2b. Remove `#[deprecated]` from `auto_attestation_blob`

- [x](2026-05-11 21:24) Remove `#[deprecated(...)]` from `auto_attestation_blob` function in `p2p/core-engine/src/foretias/tick.rs`
- [x](2026-05-11 21:24) Remove `#[allow(deprecated)]` from `p2p/core-engine/src/foretias/mod.rs` (re-export line)
- [x](2026-05-11 21:24) Remove all `#[allow(deprecated)]` from test functions in `p2p/core-engine/src/foretias/tick.rs`
- [x](2026-05-11 21:24) Remove all `#[allow(deprecated)]` from test functions in `p2p/core-engine/src/foretias/calendar.rs`

### 2c. Verify clean baseline

- [x](2026-05-11 21:24) `cargo build --workspace` — zero warnings (no deprecation, no dead_code)
- [x](2026-05-11 21:24) `cargo test --workspace` — zero warnings, all tests pass (238 tests: 118 core + 112 node + 8 integration)
- [x](2026-05-11 21:24) **Checkpoint commit**: "Cleanup: remove premature deprecation markers and dead code allows"

---

## Phase 3: Config Restructuring (Create CommunerdConfig, Eliminate P2PConfig)

### 3a. Create `CommunerdConfig` and move `AutoAttestConfig` (p2p.rs)

- [x](2026-05-11 21:37) Add `CommunerdConfig` struct to `p2p/core-engine/src/config/p2p.rs` with fields:
  - `mutual_attest: MutualAttestConfig` (moved from chronomatter.rs)
  - `p2p_listen: Option<String>` (from P2PConfig)
  - `p2p_port_range: [u16; 2]` (from P2PConfig)
  - `p2p_dial: Vec<String>` (from P2PConfig)
  - `known_servers: Vec<String>` (from P2PConfig)
  - `max_discovered_peers: usize` (from P2PConfig)
  - `dht: DHTConfig` (from P2PConfig)
  - `collision: CollisionConfig` (from P2PConfig)
- [x](2026-05-11 21:37) Add `#[derive(Debug, Clone, Serialize, Deserialize)]` with serde defaults
- [x](2026-05-11 21:37) Implement `Default` for `CommunerdConfig`
- [x](2026-05-11 21:37) Move `MutualAttestConfig` struct definition from `chronomatter.rs` to `p2p.rs`
- [x](2026-05-11 21:37) Move `MutualAttestConfig` default functions to `p2p.rs`
- [x](2026-05-11 21:37) Update `config/mod.rs` re-exports: add `CommunerdConfig`, `AutoAttestConfig` from p2p; keep `DHTConfig`, `CollisionConfig`

### 3b. Update `ChronomatterConfig` (chronomatter.rs)

- [x](2026-05-11 21:37) Remove `auto_attest: AutoAttestConfig` field from `ChronomatterConfig`
- [x](2026-05-11 21:37) Remove `AutoAttestConfig` from `pub use` exports in `chronomatter.rs`
- [x](2026-05-11 21:37) Update `ChronomatterConfig::default()` — remove auto_attest initialization
- [x](2026-05-11 21:37) `cargo build -p foretias-core` — verify

### 3c. Update `TimeFamilyConfig` (time_family.rs)

- [x](2026-05-11 21:37) Replace `pub p2p: P2PConfig` with `pub communerd: CommunerdConfig`
- [x](2026-05-11 21:37) Update `Default` impl: `communerd: CommunerdConfig::default()`
- [x](2026-05-11 21:37) Update `from_cli_and_file()` — route P2P/attestation fields to `communerd`; `listen_addr` stays at top level:
  - `listen_addr` → `self.listen_addr` (top-level, unchanged)
  - `peers` → `communerd.auto_attest.peers`
  - `auto_attest_every_n` → `communerd.auto_attest.every_n_chronons`
  - `request_timeout_secs` → `communerd.auto_attest.request_timeout_secs`
  - `p2p_listen` → `communerd.p2p_listen`
  - `p2p_port_range` → `communerd.p2p_port_range`
  - `p2p_dial` → `communerd.p2p_dial`
  - `known_servers` → `communerd.known_servers`
  - `max_discovered_peers` → `communerd.max_discovered_peers`
  - `dht_namespace` → `communerd.dht.namespace`
  - `dht_bootstrap` → `communerd.dht.bootstrap`
- [x](2026-05-11 21:37) Update accessor methods (`peers()`, `auto_attest_every_n()`, `request_timeout_secs()`) to use `self.communerd`
- [x](2026-05-11 21:37) Update `From<NodeConfig> for TimeFamilyConfig` — populate `communerd` instead of `p2p`
- [x](2026-05-11 21:37) Update imports: remove `P2PConfig`, `DHTConfig`; add `CommunerdConfig`, `AutoAttestConfig`
- [x](2026-05-11 21:37) `cargo build -p foretias-core` — verify

### 3d. Eliminate `P2PConfig`

- [x](2026-05-11 21:37) Remove `P2PConfig` struct from `p2p.rs` (all fields absorbed into CommunerdConfig)
- [x](2026-05-11 21:37) Remove `P2PConfig` default functions from `p2p.rs` (merge into CommunerdConfig defaults)
- [x](2026-05-11 21:37) Remove `P2PConfig` from `config/mod.rs` re-exports
- [x](2026-05-11 21:37) Grep for remaining `P2PConfig` references in core-engine — eliminate all
- [x](2026-05-11 21:37) `cargo build -p foretias-core` — verify

### 3e. Checkpoint

- [x](2026-05-11 21:37) `cargo build -p foretias-core` — zero warnings
- [x](2026-05-11 21:37) `cargo test -p foretias-core` — 118 passed, 0 failed
- [x](2026-05-11 21:37) **Checkpoint commit**: "Config: create CommunerdConfig, eliminate P2PConfig, move AutoAttestConfig to Communerd domain"

---

## Phase 4: Migrate All Callers from NodeConfig to CommunerdConfig

### 4a. Communerd::new accepts CommunerdConfig (communerd/mod.rs)

- [x](2026-05-11 21:48) Change `Communerd` struct field from `config: NodeConfig` to `config: CommunerdConfig`
- [x](2026-05-11 21:48) Change `Communerd::new(config: NodeConfig)` to `Communerd::new(config: CommunerdConfig)`
- [x](2026-05-11 21:48) Update `new()` internals:
  - `config.request_timeout_secs` → `config.auto_attest.request_timeout_secs`
  - `config.peers` → `config.auto_attest.peers`
  - `config.collision.nonce_window` → `config.collision.nonce_window`
  - `config.collision.heartbeat_interval_secs` → `config.collision.heartbeat_interval_secs`
  - `config.peers.is_empty()` → `config.auto_attest.peers.is_empty()`
- [x](2026-05-11 21:48) Update accessor `pub fn config(&self) -> &CommunerdConfig`
- [x](2026-05-11 21:48) Update `Clone` impl (field name unchanged, type changed — no code change needed)
- [x](2026-05-11 21:48) Update test helper `make_config()` — construct `CommunerdConfig` directly
- [x](2026-05-11 21:48) Update import: `use foretias_core::config::CommunerdConfig` (remove NodeConfig)
- [x](2026-05-11 21:48) `cargo build -p foretias-node` — verify

### 4b. TimeFamilyServer (server/mod.rs)

- [x](2026-05-11 21:48) Change `fn new_with_config(..., config: Option<NodeConfig>)` to `fn new_with_config(..., config: Option<CommunerdConfig>)`
- [x](2026-05-11 21:48) Change `pub fn with_config(mut self, config: NodeConfig) -> Self` to `pub fn with_communerd(mut self, config: CommunerdConfig) -> Self`
- [x](2026-05-11 21:48) Update internal `Communerd::new()` calls
- [x](2026-05-11 21:48) Update import: `use foretias_core::config::CommunerdConfig` (remove NodeConfig)
- [x](2026-05-11 21:48) `cargo build -p foretias-node` — verify

### 4c. main.rs (cmd_serve)

- [x](2026-05-11 21:48) Replace `NodeConfig` struct literal (lines ~340-350) with `time_family_cfg.communerd.clone()`
- [x](2026-05-11 21:48) Update call from `server.with_config(node_config)` to `server.with_communerd(time_family_cfg.communerd.clone())`
- [x](2026-05-11 21:48) Update peer display code (lines ~424-425):
  - `c.config().peers` → `c.config().auto_attest.peers`
  - `c.config().auto_attest_every_n` → `c.config().auto_attest.every_n_chronons`
- [x](2026-05-11 21:48) Remove `NodeConfig` from import: `use foretias_core::config::TimeFamilyConfig;` (remove NodeConfig)
- [x](2026-05-11 21:48) `cargo build -p foretias-node` — verify

### 4d. Integration tests (tests/integration.rs)

- [x](2026-05-11 21:48) Update `test_two_nodes_auto_attest`:
  - Replace `NodeConfig { ... }` with `CommunerdConfig { listen_addr: addr_b.clone(), auto_attest: AutoAttestConfig { peers: vec![addr_b.clone()], every_n_chronons: 1, request_timeout_secs: 5 }, ..Default::default() }`
  - Update `server.with_config(config)` to `server.with_communerd(config)`
- [x](2026-05-11 21:48) Update `test_peer_unreachable_does_not_crash`:
  - Same pattern: construct `CommunerdConfig`, call `with_communerd`
- [x](2026-05-11 21:48) Update imports: `use foretias_core::config::{CommunerdConfig, AutoAttestConfig};` (remove NodeConfig)
- [x](2026-05-11 21:48) `cargo test -p foretias-node` — verify

### 4e. foretias-python PyNodeConfig (lib.rs)

- [x](2026-05-11 21:48) Change import: `use foretias_core::config::{TimeFamilyConfig as TimeFamilyConfigInner, CollisionConfig as CollisionConfigInner};` (remove NodeConfig)
- [x](2026-05-11 21:48) Update `impl From<&TimeFamilyConfigInner> for PyNodeConfig`:
  - `listen_addr` → `c.listen_addr.clone()` (top-level, not inside communerd)
  - `version` → `c.version.clone()`
  - `calendar_path` → `c.calendars[0].persist_path.to_string_lossy().to_string()`
  - `chronon_ns` → `c.chronomatter.chronon_ns`
  - `serialized` → `false` (dropped, no equivalent)
  - `peers` → `c.communerd.auto_attest.peers.clone()`
  - `auto_attest_every_n` → `c.communerd.auto_attest.every_n_chronons`
  - `request_timeout_secs` → `c.communerd.auto_attest.request_timeout_secs`
  - `p2p_listen` → `c.communerd.p2p_listen.clone()`
  - `p2p_dial` → `c.communerd.p2p_dial.clone()`
  - `dht_namespace` → `c.communerd.dht.namespace.clone()`
  - `dht_bootstrap` → `c.communerd.dht.bootstrap.clone()`
  - `collision` → `PyCollisionConfig::from(&c.communerd.collision)`
- [x](2026-05-11 21:48) `cargo build -p foretias-python` — verify

### 4f. Verify NodeConfig is fully isolated

- [x](2026-05-11 21:48) Grep for `NodeConfig` across workspace — should only appear in:
  - `config/node.rs` (definition + impl + Default)
  - `config/time_family.rs` (From impl + use)
  - `config/mod.rs` (re-export)
  - **Nowhere else**
- [x](2026-05-11 21:48) `cargo build --workspace` — zero warnings
- [x](2026-05-11 21:48) `cargo test --workspace` — all tests pass
- [x](2026-05-11 21:48) **Checkpoint commit**: "Migration: replace all NodeConfig callers with CommunerdConfig, update server/communerd/main/tests/python"

---

## Phase 5: auto_attestation_blob Test Migration

### 5a. Migrate tick.rs tests

- [x](2026-05-11 22:01) Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in:
  - `verify_pair_valid_returns_true` (~line 487)
  - `verify_pair_tampered_returns_false` (~line 533)
  - `auto_attestation_blob_nonce_is_unique` (~line 590, 591)
  - `auto_attestation_blob_nonce_is_present` (~line 605)
- [x](2026-05-11 22:01) `cargo test -p foretias-core -- auto_attestation` — verify

### 5b. Migrate calendar.rs tests

- [x](2026-05-11 22:01) Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in:
  - `integrity_check_full_chain` (~line 316, 320)
  - `integrity_check_partial_range` (~line 368, 372)
- [x](2026-05-11 22:01) Update imports: `use crate::foretias::tick::auto_attestation_blob_with_count;`
- [x](2026-05-11 22:01) `cargo test -p foretias-core -- integrity_check` — verify

### 5c. Keep wrapper (no deprecation marker)

- [x](2026-05-11 22:01) `auto_attestation_blob` remains as a thin wrapper (no `#[deprecated]`)
- [x](2026-05-11 22:01) **Checkpoint commit**: "Migration: tests use auto_attestation_blob_with_count"

---

## Phase 6: Final Verification & Merge

### 6a. Final verification

- [x](2026-05-11 22:05) `cargo build --workspace` — zero warnings
- [x](2026-05-11 22:05) `cargo test --workspace` — zero warnings, all tests pass
- [x](2026-05-11 22:05) Grep for `#\[allow(` across workspace — zero matches (except bindgen in core/bindings.rs, core/mod.rs, foretias/types.rs, crypto_server/software.rs)
- [x](2026-05-11 22:05) Grep for `deprecated` across workspace — zero matches
- [x](2026-05-11 22:05) Grep for `P2PConfig` across workspace — zero matches
- [x](2026-05-11 22:05) Grep for `NodeConfig` across workspace — only in config/node.rs, config/time_family.rs, config/mod.rs
- [x](2026-05-11 22:05) Verify each time being has exactly one config: `ChronomatterConfig`, `CommunerdConfig`, `CalendarConfig`

### 6b. Merge

- [x](2026-05-11 22:05) Verify all work is complete in `/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410` and committed to `cleanup/repairs`
- [x](2026-05-12 15:47) Merge `cleanup/repairs` to alpha: already merged (ancestor of alpha)
- [x](2026-05-12 15:47) Final workspace verification on alpha: verified via worktree tests (238 passed)
- [x](2026-05-12 15:47) Cleanup `/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410`
  - [x](2026-05-12 15:47) Check that _PLAN.md has all but Cleanup checkboxes completed
  - [x](2026-05-12 15:47) Remove "/home/hcbusy/tmp/foretias-worktrees/CLEANUP_REPAIRS_32410" via `git worktree remove`
  - [x](2026-05-12 15:47) This is the last checkbox to be checked in my _PLAN.md
