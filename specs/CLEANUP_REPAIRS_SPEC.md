# CLEANUP_REPAIRS_SPEC.md

**Spec: Dead Code Removal, Deprecation Cleanup, Full Config Migration**
**Status: DRAFT — Pending Human Approval**
**Date: 2026-05-11**
**Revised: 2026-05-11 (CommunerdConfig design per time-being pattern)**

---

## Problem Statement

The workspace currently carries three categories of technical debt that produce no compiler warnings only because they're suppressed with `#[allow(...)]` attributes:

1. **Dead code** — 2 items suppressed with `#[allow(dead_code)]`. Code exists that serves no purpose.
2. **Premature deprecations** — `NodeConfig` (deprecated v0.5.0) and `auto_attestation_blob` (deprecated v0.9.0) were marked deprecated before callers migrated. 27 `#[allow(deprecated)]` attributes across 11 files mask ~100 warnings.
3. **Incomplete config migration** — `TimeFamilyConfig` is the intended replacement for `NodeConfig`. The `TimeFamilyConfig` struct, sub-configs, and `From<NodeConfig>` bridge exist. Production code (`TimeFamilyConfig::from_cli_and_file`) builds the new config but **never passes it to consumers**. `Communerd`, `TimeFamilyServer::with_config`, and Python bindings still accept `NodeConfig`.

Additionally, the current config hierarchy violates the established **one-config-per-time-being** pattern:
- `AutoAttestConfig` lives inside `ChronomatterConfig` but governs Communerd behavior
- `P2PConfig` is a standalone struct but Communerd has no `CommunerdConfig`
- `listen_addr` lives in `P2PConfig` but is a server-level concern

The previous cleanup commit (`dad999d`) suppressed all these warnings rather than resolving them. This spec defines resolving them.

---

## Goal

After this spec is implemented:

- **Zero `#[allow(dead_code)]` attributes** — dead code is removed, not suppressed
- **Zero `#[allow(deprecated)]` attributes** — deprecations are either completed (callers migrated) or removed (deprecated items eliminated)
- **`NodeConfig` is fully deprecated and unused** — all callers use `TimeFamilyConfig` or its sub-configs
- **`auto_attestation_blob` is removed** — all tests use `auto_attestation_blob_with_count`
- **Each time being has exactly one config struct** — `ChronomatterConfig`, `CommunerdConfig`, `CalendarConfig`
- **`cargo build --workspace` and `cargo test --workspace` produce zero warnings** without any allow attributes
- **All 240+ tests pass**

---

## Scope

### In Scope
| Component | Files | Action |
|-----------|-------|--------|
| C11 core | None | Not affected |
| foretias-core (core-engine) | `config/`, `foretias/`, `crypto_server/`, `noise.rs`, `integration_tests.rs`, `build.rs` | Dead code removal, config restructuring, auto_attestation_blob test migration, NodeConfig elimination |
| foretias-node | `server/`, `communerd/`, `main.rs`, `replication_logger.rs`, `tests/` | NodeConfig → CommunerdConfig migration, dead code removal |
| foretias-python | `lib.rs` | PyNodeConfig → PyTimeFamilyConfig migration |
| foretias-java | None | Not affected (uses separate JNI path) |

### Out of Scope
- Java bindings (`foretias-java`)
- C11 core library
- Python shim package (`src/foretias/`)
- Any behavioral changes to protocol, wire format, or storage
- Performance optimization

---

## Config Hierarchy Design

### Established Pattern

Each time being has **one** config struct. Sub-configs may exist for separation of concerns within that struct:

```
ChronomatterConfig          (ticking engine)
├── chronon_ns, tbn, dormant
├── signature_algorithm, kem_algorithm
├── key_rotation: KeyRotationConfig
└── (no auto_attest — that's Communerd's domain)

CommunerdConfig             (P2P networking)
├── auto_attest: AutoAttestConfig
│   ├── peers
│   ├── every_n_chronons
│   └── request_timeout_secs
├── p2p_listen, p2p_port_range
├── p2p_dial
├── known_servers, max_discovered_peers
├── dht: DHTConfig
└── collision: CollisionConfig

CalendarConfig              (persistence)
├── persist_path
└── encryption: EncryptionConfig
```

### TimeFamilyConfig (Top-Level)

```rust
pub struct TimeFamilyConfig {
    pub version: String,
    pub listen_addr: String,
    pub chronomatter: ChronomatterConfig,
    pub calendars: Vec<CalendarConfig>,
    pub communerd: CommunerdConfig,
    pub logging: LoggingConfig,
}
```

`listen_addr` lives at the `TimeFamilyConfig` level (not inside `CommunerdConfig`). The TimeFamily passes it to the Communerd on startup. The Communerd may override it with `p2p_listen` when libp2p multiaddr is specified explicitly, but the HTTP/JSON-RPC listen address is a server-level concern owned by the TimeFamily.

### What Changes from Current Design

| Current Location | New Location | Reason |
|-----------------|--------------|--------|
| `ChronomatterConfig.auto_attest` | `CommunerdConfig.auto_attest` | Auto-attestation is a P2P/Communerd concern, not a ticking concern |
| `P2PConfig` (entire struct) | Absorbed into `CommunerdConfig` | `P2PConfig` was an intermediate abstraction, not a time being |
| `P2PConfig.listen_addr` | `TimeFamilyConfig.listen_addr` | Server listen address is a TimeFamily-level concern, not P2P-specific |
| `TimeFamilyConfig.p2p` | `TimeFamilyConfig.communerd` | Rename to match the time being it configures |

### CommunerdConfig Definition

```rust
pub struct CommunerdConfig {
    /// Auto-attestation configuration.
    pub auto_attest: AutoAttestConfig,

    /// libp2p listen address as a multiaddr string.
    pub p2p_listen: Option<String>,

    /// Port range for auto-selection when --p2p-listen is omitted.
    pub p2p_port_range: [u16; 2],

    /// libp2p peers to dial at startup.
    pub p2p_dial: Vec<String>,

    /// Known server addresses for auto-registration.
    pub known_servers: Vec<String>,

    /// Maximum number of peers to auto-discover from DHT.
    pub max_discovered_peers: usize,

    /// DHT configuration.
    pub dht: DHTConfig,

    /// Collision detection configuration.
    pub collision: CollisionConfig,
}
```

### Signature Changes

```rust
// Before:
Communerd::new(config: NodeConfig)
TimeFamilyServer::with_config(self, config: NodeConfig)

// After:
Communerd::new(config: CommunerdConfig)
TimeFamilyServer::with_communerd(self, communerd_config: CommunerdConfig)
```

---

## Phase 1: Dead Code Removal

### 1a. `make_clock()` — `tick.rs:361`

**Current state**: Test helper `fn make_clock() -> Box<dyn Clock>` that constructs `Box::new(SystemClock)`. Not called by any test.

**Action**: Remove the function entirely. If a test needs it later, it can be added back.

**Validation**: `cargo test -p foretias-core` passes.

### 1b. `SoftwareCryptoServer` struct fields — `software.rs:24`

**Current state**: `#[allow(dead_code)]` on the entire struct because private fields are written at construction but never read externally.

**Action**: Move `#[allow(dead_code)]` from the struct level to the specific fields that trigger the warning (`sphincs_sha2_256f_secret_key`, `mlkem_secret_key`). Verify remaining fields don't trigger warnings.

**Validation**: `cargo build -p foretias-core` produces zero dead_code warnings.

---

## Phase 2: Remove Premature Deprecations

### 2a. Remove `#[deprecated]` from `NodeConfig`

**Current state**: `#[deprecated(since = "0.5.0", note = "Use TimeFamilyConfig instead")]` on `NodeConfig` struct. 27 `#[allow(deprecated)]` attributes across the workspace suppress the resulting warnings.

**Action**: Remove the `#[deprecated]` attribute. The migration will be done properly in Phase 3, AFTER which the deprecation can be re-applied meaningfully.

### 2b. Remove `#[deprecated]` from `auto_attestation_blob`

**Current state**: `#[deprecated(since = "0.9.0", note = "Use auto_attestation_blob_with_count instead...")]` on `auto_attestation_blob` function.

**Action**: Remove the `#[deprecated]` attribute. Phase 4 migrates all test callers to `_with_count`.

---

## Phase 3: Config Restructuring (Create CommunerdConfig)

### 3a. Create `CommunerdConfig` in `p2p.rs`

- [ ] Create new `CommunerdConfig` struct in `p2p/core-engine/src/config/p2p.rs`
- [ ] Merge `P2PConfig` fields into `CommunerdConfig` (absorb, don't nest) — EXCEPT `listen_addr` which goes to `TimeFamilyConfig`
- [ ] Add `auto_attest: AutoAttestConfig` field (moved from ChronomatterConfig)
- [ ] Implement `Default` for `CommunerdConfig`
- [ ] Add `Serialize, Deserialize` derives with serde defaults

### 3b. Move `AutoAttestConfig` from ChronomatterConfig to CommunerdConfig

- [ ] Move `AutoAttestConfig` struct definition from `chronomatter.rs` to `p2p.rs`
- [ ] Remove `auto_attest: AutoAttestConfig` field from `ChronomatterConfig`
- [ ] Remove `AutoAttestConfig` from `pub use` exports in `chronomatter.rs`
- [ ] Add `AutoAttestConfig` to `pub use` exports in `p2p.rs`
- [ ] Update all imports of `AutoAttestConfig` across the workspace

### 3c. Update `TimeFamilyConfig`

- [ ] Replace `pub p2p: P2PConfig` with `pub communerd: CommunerdConfig`
- [ ] Ensure `pub listen_addr: String` remains at top level (move from `P2PConfig` if needed)
- [ ] Update `Default` impl
- [ ] Update `from_cli_and_file()` — route P2P fields to `communerd` instead of `p2p`; `listen_addr` stays at top level
- [ ] Update accessor methods (`peers()`, `auto_attest_every_n()`, `request_timeout_secs()`)

### 3d. Update `From<NodeConfig> for TimeFamilyConfig`

- [ ] Update field mapping to populate `communerd` instead of `p2p`
- [ ] Move auto_attest fields into `communerd.auto_attest`

### 3e. Eliminate `P2PConfig`

- [ ] After all fields are merged into `CommunerdConfig`, remove `P2PConfig` struct
- [ ] Update `config/mod.rs` re-exports
- [ ] Update any remaining imports

### 3f. Update `ChronomatterConfig`

- [ ] Remove `auto_attest` field (moved to CommunerdConfig)
- [ ] Remove `AutoAttestConfig` from module imports
- [ ] Update `Default` impl

---

## Phase 4: Migrate Callers to CommunerdConfig

### 4a. Communerd::new accepts CommunerdConfig

- [ ] Change `Communerd` struct field from `config: NodeConfig` to `config: CommunerdConfig`
- [ ] Change `Communerd::new(config: NodeConfig)` to `Communerd::new(config: CommunerdConfig)`
- [ ] Update `new()` internals to use `self.config.auto_attest` and `self.config.collision`
- [ ] Update accessor `config(&self) -> &CommunerdConfig`
- [ ] Update `Clone` impl
- [ ] Update test helper `make_config()` to construct `CommunerdConfig`

### 4b. TimeFamilyServer::with_communerd

- [ ] Change `with_config(mut self, config: NodeConfig)` to `with_communerd(mut self, config: CommunerdConfig)`
- [ ] Change `new_with_config(..., config: Option<NodeConfig>)` to `new_with_config(..., config: Option<CommunerdConfig>)`
- [ ] Update internal `Communerd::new()` calls

### 4c. main.rs (cmd_serve)

- [ ] Replace `NodeConfig` struct literal with `CommunerdConfig` construction from `time_family_cfg.communerd`
- [ ] Update `server.with_communerd(time_family_cfg.communerd.clone())` call
- [ ] Update peer display code to use `communerd.config().auto_attest.peers`
- [ ] Remove `use foretias_core::config::NodeConfig` import

### 4d. Integration tests

- [ ] Update `test_two_nodes_auto_attest` — construct `CommunerdConfig` directly
- [ ] Update `test_peer_unreachable_does_not_crash` — construct `CommunerdConfig` directly
- [ ] Remove `use foretias_core::config::NodeConfig` imports

### 4e. PyNodeConfig → PyTimeFamilyConfig (Python bindings)

- [ ] Change `PyNodeConfig` internal source from `NodeConfigInner` to `TimeFamilyConfigInner`
- [ ] Update `From<&TimeFamilyConfigInner> for PyNodeConfig` mapping:
  - `listen_addr` → `c.listen_addr` (top-level, not inside communerd)
  - `peers` → `communerd.auto_attest.peers`
  - `auto_attest_every_n` → `communerd.auto_attest.every_n_chronons`
  - `request_timeout_secs` → `communerd.auto_attest.request_timeout_secs`
  - `p2p_listen` → `communerd.p2p_listen`
  - `p2p_dial` → `communerd.p2p_dial`
  - `dht_namespace` → `communerd.dht.namespace`
  - `dht_bootstrap` → `communerd.dht.bootstrap`
  - `collision` → `communerd.collision`
- [ ] Update imports

### 4f. Verify NodeConfig is fully isolated

- [ ] Grep for `NodeConfig` across workspace — should only appear in:
  - `config/node.rs` (definition + impl + Default)
  - `config/time_family.rs` (From impl + use)
  - `config/mod.rs` (re-export)
  - **Nowhere else**
- [ ] `cargo build --workspace` — zero warnings
- [ ] `cargo test --workspace` — all tests pass

---

## Phase 5: auto_attestation_blob Test Migration

### 5a. Migrate tick.rs tests

- [ ] Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in all test functions
- [ ] `cargo test -p foretias-core -- auto_attestation` — verify

### 5b. Migrate calendar.rs tests

- [ ] Replace `auto_attestation_blob(...)` with `auto_attestation_blob_with_count(..., 0)` in all test functions
- [ ] Update imports
- [ ] `cargo test -p foretias-core -- integrity_check` — verify

### 5c. Keep deprecated wrapper (no deprecation marker)

- [ ] `auto_attestation_blob` remains as a thin wrapper (no `#[deprecated]`, no warnings)

---

## Phase 6: Final Verification

- [ ] `cargo build --workspace` — zero warnings
- [ ] `cargo test --workspace` — zero warnings, all tests pass
- [ ] Grep for `#\[allow(` across workspace — zero matches (except bindgen in build.rs)
- [ ] Grep for `deprecated` across workspace — zero matches
- [ ] Verify each time being has exactly one config: `ChronomatterConfig`, `CommunerdConfig`, `CalendarConfig`

---

## Invariants Preserved

- No behavioral changes to stamp, verify, or P2P protocols
- No wire format changes
- No storage format changes
- All existing tests continue to pass (with updated call signatures)
- `NodeConfig` JSON deserialization still works (backward compat)

## Success Criteria

- `cargo build --workspace` — zero warnings, zero `#[allow(...)]` attributes added
- `cargo test --workspace` — zero warnings, all 240+ tests pass
- `grep -r '#\[allow(' p2p/` — returns zero matches (or only pre-existing bindgen allows in build.rs)
- `NodeConfig` used only in `config/node.rs` and `config/time_family.rs` (backward compat), nowhere else
- `auto_attestation_blob` (wrapper) exists but produces zero warnings
- `P2PConfig` eliminated, `CommunerdConfig` created
- Each time being has exactly one config struct
