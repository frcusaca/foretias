# Centralized Base64 JSON Serialization Specification

## Overview

Foretias serializes domain objects to JSON for three distinct purposes:
1. **CLI display** — human-readable output from `foretias stamp/verify` etc.
2. **Wire format** — JSON-RPC messages between nodes, GossipSub topics, DHT records
3. **Disk persistence** — calendar files, encrypted JSONL store

Currently, binary fields (`Vec<u8>`, `[u8; N]`) serialize as verbose JSON integer arrays (`[72, 181, 12, ...]`). This spec replaces that behavior with URL-safe base64 strings (`"ZGVzdA"`) across ALL serialization boundaries.

**Core Design Principle:**

> Jsonification of objects is a core standardized function of foretias. All language wrappers (PyO3, JNI, future bindings) MUST use foretias jsonification instead of custom or language-specific serialization libraries. This guarantees that output from a Python client, Java client, or Rust client is always formatted identically — a reliability requirement, not a cosmetic preference.

## What Changes

| Boundary | Before | After |
|---|---|---|
| In-memory domain types | `Vec<u8>`, `[u8; N]` | **UNCHANGED** — raw bytes in memory |
| CLI display output | `[128, 52, 2, ...]` | `"d42y...A"` |
| Wire format (JSON-RPC, gossip) | `[128, 52, 2, ...]` | `"d42y...A"` |
| Disk persistence (calendar JSON) | `[128, 52, 2, ...]` | `"d42y...A"` |
| PyO3 `to_json()` output | `[128, 52, 2, ...]` | `"d42y...A"` |
| Java JNI output | (N/A currently) | `"d42y...A"` |

## What Does NOT Change

- In-memory representation: `Vec<u8>`, `[u8; 32]`, `[u8; 16]` remain raw bytes behind wrapper Deref
- Wire protocol structure: still JSON-RPC over libp2p / TCP+Noise
- Encrypted JSONL outer pipeline: seal → CBOR → base64 (outer encoding unchanged)
- Internal crypto logic, verification, attestation algorithms

## Encoding

**Library:** Rust `base64` crate (already a dependency: `base64 = "0.21"`)
**Variant:** `URL_SAFE_NO_PAD` — alphabet `[A-Za-z0-9-_]`, no `=` padding
**Rationale:** URL-safe avoids `+`/`/` which are JSON-significant in some contexts. No padding reduces size slightly and avoids trailing whitespace issues.

## Backward Compatibility

**None.** This is a breaking change. All nodes must upgrade simultaneously. No migration of legacy integer-array files is required or supported.

---

## Architecture: Two Newtype Wrappers — Not Per-Type Methods

### The Design Decision

There are exactly **two newtype wrapper types**. Each carries its own `Serialize` / `Deserialize` impl. Every struct field that currently holds a raw byte array simply changes its declared type from `Vec<u8>` / `[u8; N]` to the corresponding wrapper. **No per-struct custom methods. No `#[serde(serialize_with)]` attributes.** The standard `#[derive(Serialize, Deserialize)]` on each struct automatically picks up the wrapper's impl.

```
Before:  pub struct Foretis { pub signature: Vec<u8>, ... }
After:   pub struct Foretis { pub signature: FTByteVector, ... }
```

The base64 encode/decode happens exactly once — inside the wrapper's serde impl. When serde walks a `Foretis`, it encounters `signature: FTByteVector`, calls `FTByteVector::serialize()`, and gets a base64 string. No other code knows base64 exists.

### Memory Transparency via Deref

Wrappers implement `Deref` / `DerefMut` so that existing logic works without change:

```rust
// FTByteVector derefs to Vec<u8>
let sig: FTByteVector = ...;
sig.len();        // works — Vec<u8>::len
sig.is_empty();   // works — Vec<u8>::is_empty
&sig[..];         // works — Vec<u8> slicing
sig.push(0xAB);   // works — Vec<u8>::push

// FTByteArray<N> derefs to [u8; N]
let hash: FTByteArray<32> = ...;
&hash[..];        // works — [u8; 32] slicing
hash[0];          // works — [u8; 32] indexing
```

### `FTByteVector` — variable-length byte arrays

The name carries the `FT` prefix to prevent collisions across all binding languages (Rust, Python, Java, C). The base64 encoding is an implementation detail of the serde impl — it is not advertised in the type name.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FTByteVector {
    inner: Vec<u8>,
}

impl std::ops::Deref for FTByteVector {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target { &self.inner }
}

impl std::ops::DerefMut for FTByteVector {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.inner }
}

impl From<Vec<u8>> for FTByteVector { fn from(v: Vec<u8>) -> Self { Self { inner: v } } }
impl From<FTByteVector> for Vec<u8> { fn from(w: FTByteVector) -> Self { w.inner } }

impl Serialize for FTByteVector {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(&self.inner))
    }
}

impl<'de> Deserialize<'de> for FTByteVector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        let decoded = URL_SAFE_NO_PAD.decode(&s).map_err(serde::de::Error::custom)?;
        Ok(FTByteVector { inner: decoded })
    }
}
```

### `FTByteArray<const N: usize>` — fixed-length byte arrays

Same `FT` prefix rationale. Generic over length `N`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FTByteArray<const N: usize> {
    inner: [u8; N],
}

impl<const N: usize> std::ops::Deref for FTByteArray<N> {
    type Target = [u8; N];
    fn deref(&self) -> &Self::Target { &self.inner }
}

impl<const N: usize> Serialize for FTByteArray<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(&self.inner))
    }
}

impl<'de, const N: usize> Deserialize<'de> for FTByteArray<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let decoded = URL_SAFE_NO_PAD.decode(&s).map_err(serde::de::Error::custom)?;
        let arr: [u8; N] = decoded.try_into().map_err(serde::de::Error::custom)?;
        Ok(FTByteArray { inner: arr })
    }
}
```

### Summary: Where Base64 Logic Lives

| Layer | Knows about base64? | Responsibility |
|---|---|---|
| `FTByteVector` / `FTByteArray` | **YES** — encode/decode in serde impl | Only two types, one location each |
| Domain structs (`Foretis`, `TickRecord`, etc.) | **NO** — just declare field types as wrappers | `#[derive(Serialize, Deserialize)]` |
| JSON-RPC handlers | **NO** — call `to_value` / `from_value` | Pass domain types through serde |
| PyO3 bindings | **NO** — call `encoding::to_json()` | Delegate to foretias jsonification |
| Java JNI | **NO** — call `encoding::to_json()` | Delegate to foretias jsonification |
| Python shim | **NO** — uses PyO3 output directly | Trust Rust output |

---

## Centralized Jsonification API

A single module `foretias/encoding.rs` provides:

```rust
// Re-export wrapper types
pub use ft_byte_vector::FTByteVector;
pub use ft_byte_array::FTByteArray;

// Canonical JSON serialization for any foretias domain type
pub fn to_json<T: serde::Serialize>(val: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(val)
}

pub fn to_json_pretty<T: serde::Serialize>(val: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(val)
}

// Canonical JSON deserialization
pub fn from_json<T: serde::Deserialize<'static>>(s: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(s)
}
```

### Binding Rules

| Binding | Rule |
|---|---|
| PyO3 `to_json()` | MUST call `foretias_core::foretias::encoding::to_json(self)` — NOT `serde_json::to_string(self)` directly |
| PyO3 `from_json()` | MUST call `foretias_core::foretias::encoding::from_json::<T>(s)` — NOT `serde_json::from_str` directly |
| Java JNI `*ToJson()` | MUST call `foretias_core::foretias::encoding::to_json()` — NOT construct JSON in Java |
| Java JNI `*FromJson()` | MUST call `foretias_core::foretias::encoding::from_json()` — NOT parse JSON in Java |
| Rust CLI | SHOULD call `encoding::to_json_pretty()` for display output |

This ensures any future change to the jsonification logic (encoding variant, pretty-print format, field ordering) propagates to all bindings automatically.

---

## Field Mapping

Every `Vec<u8>` field in domain structs becomes `FTByteVector`. Every `[u8; N]` field becomes `FTByteArray<N>`.

### Domain Type Changes (core-engine)

| Struct | Field | Old Type | New Type |
|---|---|---|---|
| `TickRecord` | `public_key` | `Vec<u8>` | `FTByteVector` |
| `TickRecord` | `forward_foretis` | `Vec<u8>` | `FTByteVector` |
| `TickRecord` | `backward_foretis` | `Vec<u8>` | `FTByteVector` |
| `TickRecord` | `aa_nonce` | `[u8; 16]` | `FTByteArray<16>` |
| `TickRecord` | `genesis_signature` | `Vec<u8>` | `FTByteVector` |
| `Foretis` | `content_hash` | `[u8; 32]` | `FTByteArray<32>` |
| `Foretis` | `signature` | `Vec<u8>` | `FTByteVector` |
| `Tbid` | (custom serde) | manual impl | `FTByteArray<96>` — replace custom impl |
| `Heartbeat` | `nonce` | `[u8; 16]` | `FTByteArray<16>` |
| `Heartbeat` | `signature` | `Vec<u8>` | `FTByteVector` |
| `EpochSnapshot` | `frost_signature` | `Vec<u8>` | `FTByteVector` |
| `EpochSnapshot` | `committee_pubkey` | `Vec<u8>` | `FTByteVector` |
| `FrostMsg` | `commitment` | `Vec<u8>` | `FTByteVector` |
| `FrostMsg` | `share` | `Vec<u8>` | `FTByteVector` |
| `SealedBlob` | `ciphertext` | `Vec<u8>` | `FTByteVector` |
| `SealedBlob` | `nonce` | `Vec<u8>` | `FTByteVector` |

### PyO3 Binding Changes (foretias-python)

| Struct | Field | Old Type | New Type |
|---|---|---|---|
| `PyForetis` | `content_hash` | `Vec<u8>` | `FTByteVector` |
| `PyForetis` | `signature` | `Vec<u8>` | `FTByteVector` |
| `PyForetis` | `tbid` | `Vec<u8>` | `FTByteVector` |
| `PyTickRecord` | `public_key` | `Vec<u8>` | `FTByteVector` |
| `PyTickRecord` | `forward_foretis` | `Vec<u8>` | `FTByteVector` |
| `PyTickRecord` | `backward_foretis` | `Vec<u8>` | `FTByteVector` |
| `PyTickRecord` | `aa_nonce` | `Vec<u8>` | `FTByteVector` |
| `PyCalendar` | `tbid` | `Vec<u8>` | `FTByteVector` |
| `PyEpochSnapshot` | `frost_signature` | `Vec<u8>` | `FTByteVector` |
| `PyEpochSnapshot` | `committee_pubkey` | `Vec<u8>` | `FTByteVector` |
| `PySealedBlob` | `nonce` | `Vec<u8>` | `FTByteVector` |
| `PySealedBlob` | `ciphertext` | `Vec<u8>` | `FTByteVector` |
| `PyHeartbeat` | `nonce` | `Vec<u8>` | `FTByteVector` |
| `PyHeartbeat` | `signature` | `Vec<u8>` | `FTByteVector` |
| `PyProbityReport` | `signature` | `Vec<u8>` | `FTByteVector` |

**Total: 31 fields across 15 structs.**

---

## Wire Format Impact Assessment

**The wire format change requires ZERO additional work beyond the domain type field changes.**

The wire format IS the domain type serialization. Every JSON-RPC handler, GossipSub message, and DHT record uses `serde_json::to_vec(&domain_type)` or `serde_json::from_slice::<DomainType>(bytes)`. When the domain type's serde impl changes from integer arrays to base64 strings (via the wrapper types), the wire format changes automatically. No handler code needs modification.

| Wire Path | Before | After | Additional Work |
|---|---|---|---|
| JSON-RPC `handle_stamp` | `to_value(&Foretis)` → int arrays | `to_value(&Foretis)` → base64 strings | **Zero** |
| JSON-RPC `handle_get_calendar_slice` | `to_value(&Vec<TickRecord>)` | base64 strings | **Zero** |
| JSON-RPC `handle_verify` | `from_value::<Foretis>` | base64 decode | **Zero** |
| GossipSub (probity reports) | `to_vec(&report)` | base64 strings | **Zero** |
| GossipSub (heartbeats) | `to_vec(&hb)` | base64 strings | **Zero** |
| Communerd request/response | `to_vec`/`from_slice` | base64 strings | **Zero** |
| DHT records | `to_vec(&PeerRegistrationRecord)` | No byte fields — unchanged | **Zero** |
| Epoch handler | `to_string(&EpochSnapshot)` | base64 strings | **Zero** |
| Calendar `save()` | `to_string_pretty(self)` | base64 strings | **Zero** |
| Encrypted JSONL | `to_vec(&CalendarBlock)` → seal → CBOR → base64 | Inner JSON uses base64, outer pipeline unchanged | **Zero** |

**Rollup concern:** All connected nodes must upgrade simultaneously. No mixed-format communication is possible. This is a coordinated cluster upgrade, not a gradual rollout.

---

## Dedicated Serialization Test File

A new test module `p2p/core-engine/src/foretias/encoding_tests.rs` provides comprehensive coverage of serialization behavior across every domain type and pathological edge case. This file is the single source of truth for verifying that the base64 encoding contract holds everywhere.

### Module Structure

```
p2p/core-engine/src/foretias/encoding_tests.rs  (≈500 lines)
├── Wrapper type tests
│   ├── FTByteVector roundtrip (empty, small, large, max)
│   ├── FTByteArray<16> roundtrip
│   ├── FTByteArray<32> roundtrip
│   ├── FTByteArray<96> roundtrip
│   ├── Deref transparency tests
│   └── Invalid input rejection tests
├── Domain type serialization tests (one per struct)
│   ├── Foretis
│   ├── TickRecord
│   ├── Tbid
│   ├── Calendar
│   ├── Heartbeat
│   ├── EpochSnapshot
│   ├── FrostMsg
│   ├── SealedBlob
│   ├── ExternalAttestation
│   └── ProbityReport
├── Cross-language canonical output tests
│   ├── to_json() == to_json_pretty() (unformatted)
│   ├── encoding::to_json produces identical output across clones
│   └── JSON contains no integer arrays for binary fields
├── Pathological cases
│   ├── All-zero byte arrays
│   ├── All-0xFF byte arrays
│   ├── 49,856 byte SLH-DSA signature (max real size)
│   ├── 7,856 byte SPHINCS+ signature
│   ├── Nested structures (Calendar with 100 ticks)
│   ├── Deep nesting (ExternalAttestation with nested Foretis and TickRecord)
│   └── Empty calendar (0 ticks)
├── Wire format simulation tests
│   ├── to_json → from_json roundtrip for each domain type
│   ├── Large calendar serialization (memory pressure)
│   └── JSON-RPC response envelope simulation
└── Negative tests
    ├── Invalid base64 strings must fail deserialization
    ├── Wrong-length base64 for fixed arrays must fail
    ├── Integer array input must fail (no backward compat)
    └── Truncated base64 must fail
```

### Test Categories in Detail

#### 1. Wrapper Type Tests

| Test | Purpose |
|---|---|
| `base64vec_empty_roundtrip` | Empty `Vec<u8>` encodes to `""`, decodes back |
| `base64vec_single_byte_roundtrip` | One byte `[0xAB]` roundtrips |
| `base64vec_all_zeroes_32` | 32 zero bytes — checks no special handling |
| `base64vec_all_0xff_64` | 64 `0xFF` bytes — checks high-bit handling |
| `base64vec_slh_dsa_sig_size` | 49,856 bytes — max real signature size |
| `base64vec_output_is_string_not_array` | Verify JSON output is `"dGV...="` not `[100,101,...]` |
| `base64array16_roundtrip` | `[u8; 16]` wrapper roundtrip |
| `base64array32_roundtrip` | `[u8; 32]` wrapper roundtrip |
| `base64array96_roundtrip` | `[u8; 96]` wrapper roundtrip |
| `base64array_wrong_length_rejected` | Decoding a base64 string of wrong length into `FTByteArray<32>` must error |
| `base64vec_deref_read` | `Deref<Target=Vec<u8>>` works for reads |
| `base64vec_deref_mut_write` | `DerefMut` works for mutations |
| `base64array_deref_read` | `Deref<Target=[u8; N]>` works for reads |

#### 2. Domain Type Serialization Tests

Each domain type gets at least two tests: one that verifies output format, one that verifies roundtrip.

| Test | Struct | Verifies |
|---|---|---|
| `foretis_serializes_to_base64_strings` | `Foretis` | JSON keys `content_hash`, `signature` are base64 strings |
| `foretis_roundtrip` | `Foretis` | `to_json → from_json` preserves all fields |
| `tick_record_serializes_to_base64_strings` | `TickRecord` | All 5 binary fields are base64 strings |
| `tick_record_roundtrip` | `TickRecord` | Full roundtrip |
| `tbid_serializes_to_single_base64_string` | `Tbid` | 96-byte TBID is ONE base64 string, not two |
| `tbid_roundtrip` | `Tbid` | Ed25519 + SLH-DSA components preserved |
| `calendar_serializes_ticks_as_base64` | `Calendar` | Nested `Vec<TickRecord>` all use base64 |
| `calendar_roundtrip` | `Calendar` | Full calendar with multiple ticks |
| `heartbeat_serializes_to_base64_strings` | `Heartbeat` | `nonce`, `signature` are base64 strings |
| `heartbeat_roundtrip` | `Heartbeat` | Full roundtrip |
| `epoch_snapshot_roundtrip` | `EpochSnapshot` | `frost_signature`, `committee_pubkey` roundtrip |
| `frost_msg_roundtrip` | `FrostMsg` | `commitment`, `share` roundtrip |
| `sealed_blob_roundtrip` | `SealedBlob` | `ciphertext`, `nonce` roundtrip |
| `external_attestation_roundtrip` | `ExternalAttestation` | Deep nesting of Foretis + TickRecord |
| `probity_report_roundtrip` | `ProbityReport` | `signature` roundtrip |

#### 3. Cross-Language Canonical Output Tests

| Test | Purpose |
|---|---|
| `to_json_and_from_json_are_inverses` | For each domain type, prove `from_json(to_json(x)) == x` |
| `canonical_output_is_deterministic` | Serializing the same object twice produces identical strings |
| `no_integer_arrays_in_any_output` | Parse serialized JSON and assert no `Value::Array` of numbers exists at any depth |
| `encoding_to_json_equals_direct_serde` | `encoding::to_json(&x)` produces same output as `serde_json::to_string(&x)` (no hidden divergence) |

#### 4. Pathological Cases

| Test | Scenario |
|---|---|
| `calendar_100_ticks_serialization` | Calendar with 100 tick records — memory pressure test |
| `calendar_0_ticks_serialization` | Empty calendar — boundary condition |
| `deep_nesting_external_attestation` | ExternalAttestation containing Foretis containing Tbid — 3 levels of binary wrappers |
| `all_zero_calendars` | All binary fields are zero bytes — no false positives |
| `max_slh_dsa_signature_in_tick_record` | TickRecord with a 49,856 byte genesis signature |
| `mimic_spincs_signature_size` | SealedBlob with 7,856 byte ciphertext |

#### 5. Negative Tests (Must Fail)

| Test | Invalid Input | Expected Behavior |
|---|---|---|
| `deserialize_invalid_base64_fails` | `"!!not-valid-base64!!"` | Deserialization error |
| `deserialize_wrong_length_for_array32_fails` | base64 of 16 bytes into `FTByteArray<32>` | Length mismatch error |
| `deserialize_integer_array_fails` | `"[1, 2, 3]"` into `FTByteVector` | Type error — no backward compat |
| `deserialize_truncated_base64_fails` | `"ZGVzd"` (truncated) | Decode error |
| `deserialize_empty_string_for_array32_fails` | `""` into `FTByteArray<32>` | Length mismatch error |
| `deserialize_padding_characters_rejected` | `"ZGVzZA=="` (padded) | Decodes via URL_SAFE_NO_PAD tolerance OR fails — must be explicit |

### Helper Functions

```rust
fn assert_json_has_no_int_arrays(value: &serde_json::Value) {
    // Recursively assert that no Value::Array contains only Value::Number elements
    // This is the key invariant: binary data NEVER appears as [1,2,3,...]
}

fn assert_field_is_base64_string(json_str: &str, field: &str) {
    // Parse JSON, find field, assert it's a String matching base64 charset
}

fn make_test_foretis() -> Foretis { /* deterministic test data */ }
fn make_test_tick_record(tick: u64) -> TickRecord { /* deterministic test data */ }
fn make_test_calendar(num_ticks: u64) -> Calendar { /* deterministic test data */ }
```
