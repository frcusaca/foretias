# CLEANUP_REPAIRS_SPEC.md

**Spec: Dead Code Removal, Deprecation Cleanup, Full Config Migration**
**Status: DRAFT — Pending Human Approval**
**Date: 2026-05-11**

---

## Problem Statement

The workspace currently carries three categories of technical debt that produce no compiler warnings only because they're suppressed with `#[allow(...)]` attributes:

1. **Dead code** — 2 items suppressed with `#[allow(dead_code)]`. Code exists that serves no purpose.
2. **Premature deprecations** — `NodeConfig` (deprecated v0.5.0) and `auto_attestation_blob` (deprecated v0.9.0) were marked deprecated before callers migrated. 27 `#[allow(deprecated)]` attributes across 11 files mask ~100 warnings.
3. **Incomplete config migration** — `TimeFamilyConfig` is the intended replacement for `NodeConfig`. The `TimeFamilyConfig` struct, sub-configs, and `From<NodeConfig>` bridge exist. Production code (`TimeFamilyConfig::from_cli_and_file`) builds the new config but **never passes it to consumers**. `Communerd`, `TimeFamilyServer::with_config`, and Python bindings still accept `NodeConfig`.

The previous cleanup commit (`dad999d`) suppressed all these warnings rather than resolving them. This spec defines resolving them.

---

## Goal

After this spec is implemented:

- **Zero `#[allow(dead_code)]` attributes** — dead code is removed, not suppressed
- **Zero `#[allow(deprecated)]` attributes** — deprecations are either completed (callers migrated) or removed (deprecated items eliminated)
- **`NodeConfig` is fully deprecated and unused** — all callers use `TimeFamilyConfig` or its sub-configs
- **`auto_attestation_blob` is removed** — all tests use `auto_attestation_blob_with_count`
- **`cargo build --workspace` and `cargo test --workspace` produce zero warnings** without any allow attributes
- **All 240+ tests pass**

---

## Scope

### In Scope
| Component | Files | Action |
|-----------|-------|--------|
| C11 core | None | Not affected |
| foretias-core (core-engine) | `config/`, `foretias/`, `crypto_server/`, `noise.rs`, `integration_tests.rs`, `build.rs` | Dead code removal, auto_attestation_blob test migration, NodeConfig elimination |
| foretias-node | `server/`, `communerd/`, `main.rs`, `replication_logger.rs`, `tests/` | NodeConfig → TimeFamilyConfig migration, dead code removal |
| foretias-python | `lib.rs` | PyNodeConfig → PyTimeFamilyConfig migration |
| foretias-java | None | Not affected (uses separate JNI path) |

### Out of Scope
- Java bindings (`foretias-java`)
- C11 core library
- Python shim package (`src/foretias/`)
- Any behavioral changes to protocol, wire format, or storage
- Performance optimization

---

## Phase 1: Dead Code Removal

### 1a. `make_clock()` — `tick.rs:361`

**Current state**: Test helper `fn make_clock() -> Box<dyn Clock>` that constructs `Box::new(SystemClock)`. Not called by any test.

**Action**: Remove the function entirely. If a test needs it later, it can be added back.

**Validation**: `cargo test -p foretias-core` passes.

### 1b. `SoftwareCryptoServer` struct fields — `software.rs:24`

**Current state**: `#[allow(dead_code)]` on the entire struct because private fields (`curve`, `priv_key`, `peer_id`) are written at construction but never read externally.

**Analysis**:
- `curve: ForetiasCurve` — used internally by `sign()` and `seal()` trait methods
- `priv_key: PrivKeyHandle` — used internally by `sign()` and `seal()`
- `peer_id: ForetiasPeerID` — used internally by `tbid()` method

**Root cause**: The `#[allow(dead_code)]` is on the **struct declaration**, not specific fields. The fields ARE used — but only within trait method implementations on the same struct. The linter sees the fields as "never read from outside the impl block."

**Action**: Move `#[allow(dead_code)]` from the struct level to the specific fields that trigger the warning. This is `sphincs_sha2_256f_secret_key` and `mlkem_secret_key` (line 42, 44) — fields that are genuinely never read. For the other fields, the warning is a false positive from the struct-level annotation.

**Specific fix**:
1. Remove `#[allow(dead_code)]` from the struct (line 24)
2. Add `#[allow(dead_code)]` only to `sphincs_sha2_256f_secret_key` and `mlkem_secret_key`
3. Verify the remaining fields don't actually trigger warnings (they shouldn't — they're used in trait impls)

**Validation**: `cargo build -p foretias-core` produces zero dead_code warnings.

---

## Phase 2: Remove Premature Deprecations

### 2a. Remove `#[deprecated]` from `NodeConfig`

**Current state**: `#[deprecated(since = "0.5.0", note = "Use TimeFamilyConfig instead")]` on `NodeConfig` struct (node.rs:10). 27 `#[allow(deprecated)]` attributes across the workspace suppress the resulting warnings.

**Action**: Remove the `#[deprecated]` attribute. The migration will be done properly in Phase 3, AFTER which the deprecation can be re-applied meaningfully.

**Files affected**:
- `core-engine/src/config/node.rs` — remove deprecation annotation
- `core-engine/src/config/mod.rs` — remove `#[allow(deprecated)]` from re-export
- `core-engine/src/config/time_family.rs` — remove `#[allow(deprecated)]` from use and From impl
- `foretias-node/src/main.rs` — remove `#![allow(deprecated)]` and all item-level allows
- `foretias-node/src/server/mod.rs` — remove `#![allow(deprecated)]` and all item-level allows
- `foretias-node/src/communerd/mod.rs` — remove `#![allow(deprecated)]` and all item-level allows
- `foretias-python/src/lib.rs` — remove `#[allow(deprecated)]` from use and From impl

### 2b. Remove `#[deprecated]` from `auto_attestation_blob`

**Current state**: `#[deprecated(since = "0.9.0", note = "Use auto_attestation_blob_with_count instead...")]` on `auto_attestation_blob` function (tick.rs:253).

**Action**: Remove the `#[deprecated]` attribute. Phase 3 migrates all test callers to `_with_count`, AFTER which the deprecated wrapper can be removed entirely (not just un-deprecated).

**Files affected**:
- `core-engine/src/foretias/tick.rs` — remove deprecation annotation, remove all `#[allow(deprecated)]` from tests
- `core-engine/src/foretias/mod.rs` — remove `#[allow(deprecated)]` from re-export
- `core-engine/src/foretias/calendar.rs` — remove all `#[allow(deprecated)]` from tests

---

## Phase 3: Full Config Migration (NodeConfig → TimeFamilyConfig)

### 3a. Design: What Communerd Actually Needs

`Communerd` stores `config: NodeConfig` but only accesses 4 fields:

| Field Used | Source in TimeFamilyConfig |
|-----------|---------------------------|
| `request_timeout_secs` | `chronomatter.auto_attest.request_timeout_secs` |
| `peers` | `chronomatter.auto_attest.peers` |
| `collision.nonce_window` | `p2p.collision.nonce_window` |
| `collision.heartbeat_interval_secs` | `p2p.collision.heartbeat_interval_secs` |

**Approach**: Instead of passing the entire `NodeConfig` or `TimeFamilyConfig` to `Communerd`, pass only the sub-configs it needs. This is more focused and avoids coupling Communerd to config hierarchy changes.

**New `Communerd::new` signature**:
```rust
pub fn new(
    auto_attest: AutoAttestConfig,
    collision: CollisionConfig,
) -> Self
```

**New `Communerd` struct field**:
```rust
auto_attest: AutoAttestConfig,
collision: CollisionConfig,
// (replaces config: NodeConfig)
```

**New accessor** (replaces `config() -> &NodeConfig`):
```rust
pub fn peers(&self) -> &[String] { &self.auto_attest.peers }
pub fn auto_attest_config(&self) -> &AutoAttestConfig { &self.auto_attest }
```

### 3b. Design: What TimeFamilyServer Needs

`TimeFamilyServer::new_with_config` and `with_config` accept `Option<NodeConfig>` / `NodeConfig` solely to construct a `Communerd`. After Phase 3a, these methods accept the sub-configs directly:

```rust
pub fn with_config(
    mut self,
    auto_attest: AutoAttestConfig,
    collision: CollisionConfig,
) -> Self
```

### 3c. main.rs Migration

`cmd_serve` currently:
1. Builds `TimeFamilyConfig` from CLI (line 308) — **already correct**
2. Builds `NodeConfig` struct literal for peers (line 342) — **needs migration**
3. Calls `server.with_config(node_config)` — **needs migration**
4. Reads `communerd.config().peers` for display (line 428) — **needs migration**

After migration:
1. `TimeFamilyConfig::from_cli_and_file(...)` already built
2. Extract `auto_attest` and `collision` sub-configs from `time_family_cfg`
3. Call `server.with_config(time_family_cfg.chronomatter.auto_attest, time_family_cfg.p2p.collision)`
4. Read `communerd.peers()` and `communerd.auto_attest_config()` for display

### 3d. PyNodeConfig → PyTimeFamilyConfig (Python bindings)

`PyNodeConfig` mirrors `NodeConfig`'s flat structure. Options:

**Option A — Rename + Restructure**: Create `PyTimeFamilyConfig` with nested sub-config classes (`PyAutoAttestConfig`, `PyP2PConfig`, etc.) matching `TimeFamilyConfig`. Breaking change for Python consumers.

**Option B — Keep PyNodeConfig, change internal source**: `PyNodeConfig` remains the Python-facing type (flat, simple API), but internally it converts from `TimeFamilyConfig` sub-configs rather than `NodeConfig`. No breaking change for Python consumers.

**Recommendation: Option B**. The Python binding is a convenience layer — keeping the flat API avoids breaking external users. The `From<&NodeConfigInner>` impl is replaced with a constructor that accepts sub-configs and assembles the flat struct.

### 3e. Test Migration

Integration tests in `foretias-node/tests/integration.rs` and `communerd/mod.rs` test helper construct `NodeConfig` literals. Replace with sub-config construction:

```rust
// Before:
let config = NodeConfig { listen_addr: addr, peers: vec![...], ..Default::default() };
server.with_config(config)

// After:
let auto_attest = AutoAttestConfig { peers: vec![...], every_n_chronons: 1, request_timeout_secs: 5 };
let collision = CollisionConfig::default();
server.with_config(auto_attest, collision)
```

### 3f. Remove NodeConfig After Migration

Once all callers migrate:

1. `NodeConfig` struct can remain (for JSON deserialization / backward compat loading) but is no longer passed around
2. `From<NodeConfig> for TimeFamilyConfig` remains (for config file migration)
3. `NodeConfig::load()` and `NodeConfig::default_path()` remain (for legacy config files)
4. Everything else (struct literals, parameter passing) is eliminated

**We do NOT delete `NodeConfig` entirely** — it serves as a backward-compat deserialization target for legacy config files. But it no longer flows through the runtime API.

---

## Phase 4: auto_attestation_blob Removal

All test code calls the deprecated `auto_attestation_blob`. Production code (`chronomatter/mod.rs`) already uses `auto_attestation_blob_with_count`.

### Migration

Each test call:
```rust
auto_attestation_blob(&tbid_str, a_tick, &a_pk, b_tick, &b_pk)
```

Becomes:
```rust
auto_attestation_blob_with_count(&tbid_str, a_tick, &a_pk, b_tick, &b_pk, 0)
```

### Files Affected
- `core-engine/src/foretias/tick.rs` — 5 test calls (lines 487, 533, 590, 591, 605)
- `core-engine/src/foretias/calendar.rs` — 6 test calls (lines 316, 320, 368, 372 + 2 imports)

### After Migration

The deprecated `auto_attestation_blob` wrapper can be:
- **Option A**: Kept as a thin wrapper (no `#[deprecated]`, no warnings) — simplest, zero risk
- **Option B**: Removed entirely — cleaner but risks breaking external callers

**Recommendation: Option A**. The wrapper is 3 lines, maintains backward compatibility, and costs nothing to keep.

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
- `auto_attestation_blob` (deprecated wrapper) exists but produces zero warnings
