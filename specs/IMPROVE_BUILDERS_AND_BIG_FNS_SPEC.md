# IMPROVE_BUILDERS_AND_BIG_FNS_SPEC.md

**Spec: Improve Builders and Big Functions — bon Builders + Permissive Serde**
**Paired Plan: IMPROVE_BUILDERS_AND_BIG_FNS_PLAN.md**
**Status: PROPOSED**
**Date: 2026-06-04**
**Experiment: `experiments/bon-serde-poc/` (19/19 tests pass)**

---

## Overview

Integrate the `bon` builder crate for domain types and implement permissive wire parsing with strict validation for the trust boundary types. This replaces multi-parameter constructors with named builders and makes wire deserialization tolerant of missing optional fields.

### Motivation

1. **Large constructors are error-prone.** `ChrononRecord::new()` takes 7 positional params. `TimeFamilyCliConfig` has 15 fields. Builders with named methods eliminate positional confusion.

2. **Wire parsing should be permissive.** When a remote peer sends a `ChrononRecord` with missing optional fields (e.g., `tb_version`, `tbid`), the current code rejects it because serde requires all non-defaulted fields. Permissive parsing accepts missing optional fields and applies defaults during validation.

3. **Validation should happen once, at the trust boundary.** The `TryFrom<Unchecked> → Domain` pattern separates parsing (permissive) from validation (strict), making the trust boundary explicit and auditable.

### Architecture

```
Wire bytes
    ↓ serde_json::from_slice()
ChrononRecordUnchecked (all fields Option<T>, permissive)
    ↓ TryFrom<ChrononRecordUnchecked> for ChrononRecord (validation)
ChrononRecord (strict, all fields required/defaulted)
    ↓ wrapped in
UnverifiedSignatureEnvelope<ChrononRecord> (existing, unchanged)
    ↓ verify(crypto, ...)  — validates fields + signatures
CleanAuthenticated<ChrononRecord> (existing, unchanged)
    ↓ externalize()
Externalized<ChrononRecord> (existing, unchanged)
```

### What Changes

| Component | Before | After |
|-----------|--------|-------|
| `ChrononRecord` | `#[derive(Deserialize)]` — strict, fails if any field missing | Custom `Deserialize` — permissive, all fields optional, defaults applied |
| `Foretis` | `#[derive(Deserialize)]` — strict | Custom `Deserialize` — permissive |
| `ExternalAttestation` | `#[derive(Deserialize)]` — strict | Custom `Deserialize` — permissive |
| `ChrononRecord::new()` | 7 positional params | `ChrononRecord::builder().chronon_number(1).public_key(...).build()` |
| `Foretis::new()` | 6 positional params | `Foretis::builder().chronon_number(1).content_hash(...).build()` |
| `verify()` | Validates signatures only | Validates **fields + signatures** |
| Config structs | Multi-param constructors | `bon::Builder` derives |
| `UnverifiedSignatureEnvelope<T>` | No changes | **No changes** — name stays, `DontUse<T>` alias stays |
| `CleanAuthenticated<T>` | No changes | No changes |
| `Externalized<T>` | No changes | No changes |
| Passthrough | Unknown fields silently discarded | Unknown fields captured via `#[serde(flatten)]`, round-tripped |

### What Does NOT Change

- `UnverifiedSignatureEnvelope<T>` — stays as-is (same name, same `DontUse<T>` alias, same `from_bytes()` / `from_json_value()`)
- `CleanAuthenticated<T>` — stays as-is (private constructors, no serde)
- `Externalized<T>` — stays as-is (`Serialize, Deserialize` on the wrapper)
- The trust boundary flow — `UnverifiedSignatureEnvelope → CleanAuthenticated → Externalized`
- Wire format — JSON shape is identical (same field names, same types)
- Local construction — `Chronomatter` still uses `ChrononRecord::builder()...build()`

---

## Detailed Design

### 1. Permissive Deserialization with Passthrough

Each domain type gets a custom `Deserialize` that works with all-optional fields and preserves unknown fields. No new types — `UnverifiedSignatureEnvelope<T>` stays as-is.

The "unprocessed" state is simply `UnverifiedSignatureEnvelope<ChrononRecord>` where `ChrononRecord`'s `Deserialize` is permissive. `from_bytes()` works unchanged — the permissiveness comes from the inner type's `Deserialize` impl.

```rust
use std::collections::HashMap;

/// ChrononRecord — strict domain type.
/// NO #[derive(Deserialize)] — custom Deserialize impl below.
/// Keeps builder for local construction by Chronomatter.
#[derive(Debug, Clone, Serialize, Builder)]
pub struct ChrononRecord {
    #[serde(rename = "tick_number")]
    pub chronon_number: u64,
    pub public_key: FTByteVector,
    pub signature_algorithm: String,
    pub forward_foretis: FTByteVector,
    pub backward_foretis: FTByteVector,
    pub aa_nonce: FTByteArray<16>,
    pub chronon_stamp_count: u64,
    pub external_attestations: Vec<ExternalAttestation>,
    pub tb_version: u32,
    pub tbid: Tbid,
    /// Unknown fields from the wire, preserved for round-trip passthrough.
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub passthrough: HashMap<String, serde_json::Value>,
}

/// Private all-optional intermediate for deserialization.
/// This is the "unprocessed" wire state — all fields Optional, unknowns captured.
#[derive(Deserialize)]
struct ChrononRecordRaw {
    #[serde(rename = "tick_number")]
    chronon_number: Option<u64>,
    public_key: Option<FTByteVector>,
    #[serde(default)]
    signature_algorithm: Option<String>,
    forward_foretis: Option<FTByteVector>,
    backward_foretis: Option<FTByteVector>,
    aa_nonce: Option<FTByteArray<16>>,
    #[serde(default)]
    chronon_stamp_count: Option<u64>,
    #[serde(default)]
    external_attestations: Option<Vec<ExternalAttestationRaw>>,
    #[serde(default)]
    tb_version: Option<u32>,
    #[serde(default)]
    tbid: Option<Tbid>,
    /// Catch-all for unknown fields.
    #[serde(flatten)]
    passthrough: Option<HashMap<String, serde_json::Value>>,
}

/// Custom Deserialize — permissive parsing, all fields optional.
impl<'de> Deserialize<'de> for ChrononRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = ChrononRecordRaw::deserialize(deserializer)?;
        // Validation happens here — field presence, defaults, passthrough
        // Signature verification happens later in verify()
        Ok(Self {
            chronon_number: raw.chronon_number.unwrap_or(0),
            public_key: raw.public_key.unwrap_or_default(),
            signature_algorithm: raw.signature_algorithm.unwrap_or_else(|| "Ed25519".into()),
            forward_foretis: raw.forward_foretis.unwrap_or_default(),
            backward_foretis: raw.backward_foretis.unwrap_or_default(),
            aa_nonce: raw.aa_nonce.unwrap_or([0u8; 16]),
            chronon_stamp_count: raw.chronon_stamp_count.unwrap_or(0),
            external_attestations: raw.external_attestations
                .map(|v| v.into_iter().map(|r| r.into()).collect())
                .unwrap_or_default(),
            tb_version: raw.tb_version.unwrap_or(0),
            tbid: raw.tbid.unwrap_or_default(),
            passthrough: raw.passthrough.unwrap_or_default(),
        })
    }
}
```

**Why this works:**
- `UnverifiedSignatureEnvelope::from_bytes(bytes)` calls `serde_json::from_slice::<ChrononRecord>(bytes)` — which now uses the custom `Deserialize`
- `#[serde(flatten)]` captures any unknown JSON fields
- Missing known fields → `None` → defaults applied
- Unknown fields → carried through in `passthrough` → re-serialized on output
- The `ChrononRecordRaw` type is **private** — never escapes the module

**Recursive nesting:** `ExternalAttestationRaw` follows the same pattern, capturing unknowns in its own `passthrough` field.

### 2. UnverifiedSignatureEnvelope Stays

```rust
// Existing type — NO changes needed
pub struct UnverifiedSignatureEnvelope<T> {
    inner: T,
    pub(crate) signatures: Vec<SignatureEntry>,
}
pub type DontUse<T> = UnverifiedSignatureEnvelope<T>;
```

`from_bytes()` works because `T: DeserializeOwned` — and `ChrononRecord` now implements `Deserialize` (custom impl above).

### 3. Verify: Fields + Signatures + Builder

The `verify()` method now validates field presence and invariants BEFORE checking signatures. Uses the builder to construct `ChrononRecord` from validated fields:

```rust
impl UnverifiedSignatureEnvelope<ChrononRecord> {
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        prev: Option<&CleanAuthenticated<ChrononRecord>>,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError> {
        let record = &self.inner;

        // 1. Validate field presence (reject zero-values from permissive deserialization)
        if record.chronon_number == 0 {
            return Err(CleanAuthError::InvalidField("chronon_number must be > 0"));
        }
        if record.public_key.is_empty() {
            return Err(CleanAuthError::InvalidField("public_key must not be empty"));
        }

        // 2. Chain verification (existing logic)
        if let Some(prev) = prev {
            verify_pair(crypto, &prev.inner().tbid.to_hex(), prev.inner(), record)
                .map_err(|e| CleanAuthError::Crypto(e))?;
        }

        // 3. Signature verification (existing logic)
        // ...

        // 4. Produce CleanAuthenticated
        Ok(CleanAuthenticated {
            inner: self.inner,  // ChrononRecord already constructed via Deserialize
            signatures: self.signatures,
        })
    }
}
```

**Note:** The builder is used by Chronomatter for local construction. For wire deserialization, the builder is used implicitly through the custom `Deserialize` impl (which constructs `ChrononRecord` with defaults). The `verify()` method validates the already-constructed record.

Same pattern for `Foretis` and `ExternalAttestation` — each gets a private `*Raw` intermediate and a `passthrough` field:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Foretis {
    pub chronon_number: u64,
    pub content_hash: FTByteArray<32>,
    pub tbid: Tbid,
    pub echo: String,
    pub tbn: String,
    pub time_being_reference_time: String,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub passthrough: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalAttestation {
    pub attester_tbid: String,
    pub foretis: Foretis,
    pub signature: FTByteVector,
    pub signature_algorithm: String,
    pub attester_tick_record: ChrononRecord,
    pub received_at_ns: u64,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub passthrough: HashMap<String, serde_json::Value>,
}
```

**Wire compatibility:** Existing JSON without unknown fields deserializes fine (passthrough is empty). New fields from future versions are preserved and round-tripped.

### 4. bon Builders

Add `#[derive(Builder)]` to domain types for local construction:

```rust
#[derive(Debug, Clone, Serialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ChrononRecord {
    #[serde(rename = "tick_number")]
    pub chronon_number: u64,
    pub public_key: FTByteVector,
    #[builder(default = "Ed25519".to_string())]
    pub signature_algorithm: String,
    // ...
}

impl<S: chronon_record_builder::IsComplete> ChrononRecordBuilder<S> {
    pub fn build(self) -> Result<ChrononRecord, NodeError> {
        let record = self.build_internal();
        if record.chronon_number == 0 {
            return Err(NodeError::InvalidInput("chronon_number must be > 0".into()));
        }
        // ...
        Ok(record)
    }
}
```

**Compile-time enforcement:** Non-`Option` fields are required by default in bon. Omitting a required field produces a compile error.

**Default values:** `#[builder(default)]` for optional fields, `#[builder(default = "Ed25519".to_string())]` for fields with specific defaults.

**Fallible build:** Custom `build()` method on the builder returns `Result<T, NodeError>`.

### 5. Config Struct Builders

The 5 large config structs get bon builders. The choice between **function builder** and **struct builder** depends on how the config is constructed:

#### bon Function Builder (`#[builder] fn`) — Use When All Params Available at Call Site

When all values are available atomically at the call site, use a **function builder**. The builder is ephemeral — consumed by `.call()`. No intermediate struct needed.

```rust
// Before: struct + multi-param constructor
async fn gossip_event_loop(config: GossipLoopConfig) { ... }
Self::gossip_event_loop(GossipLoopConfig::new(a, b, c, d, e, f, g, h, i, j, k)).await;

// After: bon function builder
#[bon::builder]
async fn gossip_event_loop(
    events: UnboundedReceiver<NetworkEvent>,
    cmd_tx: Option<UnboundedSender<SwarmCommand>>,
    probity_store: Arc<ProbityStore>,
    // ... all 11 params
) { ... }

Self::gossip_event_loop()
    .events(events)
    .cmd_tx(cmd_tx)
    .probity_store(probity_store)
    // ...
    .call()
    .await;
```

**Candidates (all constructed atomically at the call site):**

| Function | Fields | File | Current Call Site |
|----------|--------|------|-------------------|
| `gossip_event_loop` | 11 | `communerd/mod.rs:657` | All from `self.something.clone()` |
| `refresh_self_registration` | 9 | `communerd/mod.rs:1123` | All from captured variables |
| `cmd_serve` | 14 | `main.rs:837` | All from CLI parsing |

#### bon Struct Builder (`#[derive(Builder)]`) — Use When Config Built Gradually

When a config struct is built up **gradually over time** — each piece set as it becomes available, then finally passed to a function — use a **struct builder**. The struct acts as an accumulator.

```rust
// Config built gradually: piece by piece as each becomes available
let mut builder = MyConfig::builder()
    .name(available_name)
    .timeout(available_timeout);

// Later, when more pieces arrive:
if let Some(path) = available_path {
    builder = builder.persist_path(path);
}

// Finally, call:
let config = builder.build()?;
do_work(config);
```

**Candidates (built gradually, not atomically):**

| Struct | Fields | File | Reason for struct builder |
|--------|--------|------|---------------------------|
| `TimeFamilyCliConfig` | 15 | `config/time_family.rs` | May be built from config file + CLI overrides |
| `VerifyConfig` | 8 | `main.rs` | May be built from multiple input sources |

#### Decision Rule

| Pattern | Use | Example |
|---------|-----|---------|
| All params at call site | `#[builder] fn` | `gossip_event_loop`, `cmd_serve` |
| Built gradually, passed later | `#[derive(Builder)]` on struct | `TimeFamilyCliConfig` |
| Struct already exists and is passed around | Keep struct, add `#[derive(Builder)]` | `VerifyConfig` |

---

## Scope

### In Scope

- `ChrononRecord` — shadow type + TryFrom + custom Deserialize + bon builder
- `Foretis` — shadow type + TryFrom + custom Deserialize + bon builder
- `ExternalAttestation` — shadow type + TryFrom + custom Deserialize + bon builder
- `GossipLoopConfig` — bon function builder (`#[builder] fn gossip_event_loop`)
- `RegistrationConfig` — bon function builder (`#[builder] fn refresh_self_registration`)
- `ServeConfig` — bon function builder (`#[builder] fn cmd_serve`)
- `TimeFamilyCliConfig` — bon struct builder (`#[derive(Builder)]`)
- `VerifyConfig` — bon struct builder (`#[derive(Builder)]`)
- `bon = "3.9"` dependency added to `foretias-core` and `foretias-server`

### Out of Scope

- `UnverifiedSignatureEnvelope<T>` — no changes
- `CleanAuthenticated<T>` — no changes
- `Externalized<T>` — no changes
- `EpochSnapshot` — can follow the same pattern later
- `FamilyRecord` — can follow the same pattern later
- Config file deserialization (TOML/YAML) — only JSON for now

---

## Risks

| Risk | Mitigation |
|------|------------|
| bon compile time increase | bon ~2x typed-builder, but only affects the 3 domain types + 5 config structs |
| Shadow type maintenance | Each domain field change requires updating both the domain type and the unchecked type — document this in the module |
| `TryFrom` error type | `ValidationError` implements `serde::de::Error` so errors map cleanly to serde errors |
| Wire format compatibility | JSON shape is identical — no wire format change |

---

## Experiment Verification

The experiment at `experiments/bon-serde-poc/` verifies all 19 test cases:

1. Permissive parsing (missing optional fields → defaults)
2. Missing required field → clear error
3. Zero chronon_number → validation error
4. bon builder local construction
5. bon builder defaults applied
6. bon builder validation (zero chronon, empty pubkey)
7. Foretis serde round-trip
8. ChrononRecord serde round-trip
9. Externalized wrapper round-trip
10. Nested ExternalAttestation permissive parsing
11. Nested ExternalAttestation missing required
12. ChrononRecord with nested external attestations
13. Tick number rename works
14. Serialize uses tick_number (not chronon_number)
15. Empty JSON → rejected
16. Partial JSON → rejected
17. Foretis missing required fields → error
18. Foretis builder validation
19. Compile-time enforcement (missing builder field)

**Result: 19/19 pass. Pattern is viable.**
