# BON_SERDE_TRUST_BOUNDARIES_SPEC.md

**Spec: bon Builders + Permissive Serde for Trust Boundary Types**
**Paired Plan: BON_SERDE_TRUST_BOUNDARIES_PLAN.md**
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
UnverifiedSignatureEnvelope<ChrononRecord> (existing trust boundary)
    ↓ verify(crypto, ...)
CleanAuthenticated<ChrononRecord> (existing, unchanged)
    ↓ externalize()
Externalized<ChrononRecord> (existing, unchanged)
```

### What Changes

| Component | Before | After |
|-----------|--------|-------|
| `ChrononRecord` | `#[derive(Deserialize)]` on domain type | Custom `Deserialize` impl routing through `ChrononRecordUnchecked` |
| `Foretis` | `#[derive(Deserialize)]` on domain type | Custom `Deserialize` impl routing through `ForetisUnchecked` |
| `ExternalAttestation` | `#[derive(Deserialize)]` on domain type | Custom `Deserialize` impl routing through `ExternalAttestationUnchecked` |
| `ChrononRecord::new()` | 7 positional params | `ChrononRecord::builder().chronon_number(1).public_key(...).build()` |
| `Foretis::new()` | 6 positional params | `Foretis::builder().chronon_number(1).content_hash(...).build()` |
| Config structs | Multi-param constructors | `bon::Builder` derives |
| `UnverifiedSignatureEnvelope<T>` | No changes | No changes |
| `CleanAuthenticated<T>` | No changes | No changes |
| `Externalized<T>` | No changes | No changes |

### What Does NOT Change

- `UnverifiedSignatureEnvelope<T>` — stays as-is (manual `from_bytes()` / `from_json_value()`)
- `CleanAuthenticated<T>` — stays as-is (private constructors, no serde)
- `Externalized<T>` — stays as-is (`Serialize, Deserialize` on the wrapper)
- The trust boundary flow — `Unverified → CleanAuthenticated → Externalized`
- Wire format — JSON shape is identical (same field names, same types)

---

## Detailed Design

### 1. Shadow Types (Permissive Wire Parsing)

For each domain type, create a shadow type with all fields as `Option<T>`:

```rust
// In tick.rs (or a new tick_unchecked.rs module)
struct ChrononRecordUnchecked {
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
    external_attestations: Option<Vec<ExternalAttestationUnchecked>>,
    #[serde(default)]
    tb_version: Option<u32>,
    #[serde(default)]
    tbid: Option<Tbid>,
}
```

The `ChrononRecordUnchecked` type is **private** to the module — it never escapes the deserialization boundary.

### 2. TryFrom Validation

```rust
impl TryFrom<ChrononRecordUnchecked> for ChrononRecord {
    type Error = ValidationError;

    fn try_from(raw: ChrononRecordUnchecked) -> Result<Self, Self::Error> {
        let chronon_number = raw.chronon_number.ok_or(missing("chronon_number"))?;
        if chronon_number == 0 {
            return Err(ValidationError("chronon_number must be > 0".into()));
        }
        let public_key = raw.public_key.ok_or(missing("public_key"))?;
        if public_key.is_empty() {
            return Err(ValidationError("public_key must not be empty".into()));
        }
        Ok(Self {
            chronon_number,
            public_key,
            signature_algorithm: raw.signature_algorithm.unwrap_or_else(|| "Ed25519".to_string()),
            forward_foretis: raw.forward_foretis.ok_or(missing("forward_foretis"))?,
            backward_foretis: raw.backward_foretis.ok_or(missing("backward_foretis"))?,
            aa_nonce: raw.aa_nonce.ok_or(missing("aa_nonce"))?,
            chronon_stamp_count: raw.chronon_stamp_count.unwrap_or(0),
            external_attestations: raw.external_attestations
                .map(|v| v.into_iter().map(ExternalAttestation::try_from).collect())
                .transpose()?
                .unwrap_or_default(),
            tb_version: raw.tb_version.unwrap_or(0),
            tbid: raw.tbid.unwrap_or_default(),
        })
    }
}
```

### 3. Custom Deserialize Impl

Replace `#[derive(Deserialize)]` on the domain type with a manual impl:

```rust
impl<'de> Deserialize<'de> for ChrononRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let unchecked = ChrononRecordUnchecked::deserialize(deserializer)?;
        ChrononRecord::try_from(unchecked).map_err(serde::de::Error::custom)
    }
}
```

This is a **drop-in replacement** — the JSON shape is identical. The only difference is that missing optional fields get defaults instead of errors.

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

The 5 large config structs get `#[derive(Builder)]` without the shadow type pattern (they don't cross trust boundaries):

| Struct | Fields | File |
|--------|--------|------|
| `GossipLoopConfig` | 11 | `communerd/mod.rs` |
| `RegistrationConfig` | 9 | `communerd/mod.rs` |
| `TimeFamilyCliConfig` | 15 | `config/time_family.rs` |
| `ServeConfig` | 14 | `main.rs` |
| `VerifyConfig` | 8 | `main.rs` |

These get `#[derive(Builder)]` with `#[builder(default)]` for optional fields and `#[allow(clippy::too_many_arguments)]` removed from their constructors.

---

## Scope

### In Scope

- `ChrononRecord` — shadow type + TryFrom + custom Deserialize + bon builder
- `Foretis` — shadow type + TryFrom + custom Deserialize + bon builder
- `ExternalAttestation` — shadow type + TryFrom + custom Deserialize + bon builder
- `GossipLoopConfig` — bon builder
- `RegistrationConfig` — bon builder
- `TimeFamilyCliConfig` — bon builder
- `ServeConfig` — bon builder
- `VerifyConfig` — bon builder
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
