# GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md

**Date:** 2026-05-22
**Application:** Foretias v0.3+
**Supersedes:** `TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_SPEC.md`
**Paired Plan:** `GENERIC_TRUST_BOUNDARY_WRAPPERS_PLAN.md`

---

## 1. Purpose

Replace the current set of 12 concrete wrapper structs (4 domain types multiplied by 3 wrapper stages) with a unified generic wrapper system. The compiler still enforces the three-stage type progression, but code is de-duplicated, and new domain types integrate without boilerplate.

### 1.1 The Problem with Concrete Wrappers

The current `clean_auth.rs` defines twelve separate structs:

```
UnprocessedChrononRecord,  CleanAuthenticatedChrononRecord,  ExternalizedChrononRecord
UnprocessedForetis,        CleanAuthenticatedForetis,        ExternalizedForetis
UnprocessedEpochSnapshot,  CleanAuthenticatedEpochSnapshot,  ExternalizedEpochSnapshot
UnprocessedProbityReport,  CleanAuthenticatedProbityReport,  ExternalizedProbityReport
```

Each triple repeats the same pattern: private `inner` field, `from_trusted()`, `inner()`, `into_inner()`, `externalize()`, `into_unprocessed()`. Adding a fifth domain type means writing three more structs, three more `TrustedInner` impls, and all the associated boilerplate. The repetition is error-prone and obscures the actual trust-boundary logic.

### 1.2 The Solution

This spec introduces three generic wrapper types:

```rust
pub struct Unprocessed<T> { inner: T }
pub struct CleanAuthenticated<T> { inner: T }
pub struct Externalized<T> { inner: T }
```

A single `TrustedInner<T>` trait provides `from_trusted()`, `inner()`, `into_inner()` with a generic implementation that covers all three wrappers. Delegation to domain-type methods (like `externalize()` and `into_unprocessed()`) uses explicit passthrough, so the compiler still catches missing implementations.

### 1.3 What This Gives You

**For the code writer:** Add a new domain type and wire up trust boundaries with a few impl blocks instead of hand-writing three structs. The generic wrappers handle construction guards, accessor methods, and trait compliance automatically.

**For the code reviewer:** Review one set of wrapper definitions instead of twelve. The trust-boundary discipline lives in the generic wrappers. Type-specific logic lives in the domain type's passthrough impls.

**For formal analysis:** A static analyzer verifies the discipline against three wrapper types instead of twelve. The permission table maps wrapper types against allowed locations, and the static analysis test cross-tabulates actual usage against the table.

## 2. Design Principles

### 2.1 Three Generic Wrappers, Not Twelve Concrete Ones

| Wrapper | Purpose | Construction |
|---------|---------|--------------|
| `Unprocessed<T>` | Parsed but not yet verified. Raw domain data from the wire or disk. | `Unprocessed::from_bytes()`, `Unprocessed::from_json_value()`, `Externalized<T>::into_unprocessed()` |
| `CleanAuthenticated<T>` | Due diligence complete. Authenticated to claimed TBID, sanitized, validated, normalized. Safe for in-process use. | `Unprocessed<T>::into_clean_authenticated()` (inbound gate), `CleanAuthenticated::from_trusted()` (local gate) |
| `Externalized<T>` | Wire and disk format. Stripped of all runtime context. Minimal fields only. | `CleanAuthenticated<T>::externalize()` |

The progression remains unidirectional:

```
Wire bytes → Unprocessed<T> → CleanAuthenticated<T> → Externalized<T> → Disk/Wire
                                ↑
                                └── (re-verification on load)
```

### 2.2 Private `inner: T` — Construction Guards

All three wrappers have a private `inner` field. No code outside `clean_auth.rs` constructs them directly.

```rust
pub struct Unprocessed<T> {
    inner: T,  // private
}

pub struct CleanAuthenticated<T> {
    inner: T,  // private
}

pub struct Externalized<T> {
    inner: T,  // private
}
```

The only paths to each type:

- **`Unprocessed<T>`**: `from_bytes()`, `from_json_value()`, `Externalized<U>::into_unprocessed()` (where U is the domain-specific Externalized form)
- **`CleanAuthenticated<T>`**: `from_trusted()`, `Unprocessed<T>::into_clean_authenticated()`
- **`Externalized<T>`**: `CleanAuthenticated<T>::externalize()`

No `as` cast, no `unsafe` transmute, no `Deref` that bypasses the gate.

### 2.3 `TrustedInner<T>` Trait — Generic Accessors

A single trait provides the three core accessor methods across all wrappers:

```rust
pub trait TrustedInner<T>: Sized {
    /// Trusted construction — only for locally-produced data.
    fn from_trusted(inner: T) -> Self;
    /// Read-only accessor.
    fn inner(&self) -> &T;
    /// Consume and return the inner value.
    fn into_inner(self) -> T;
}
```

Generic implementations:

```rust
impl<T> TrustedInner<T> for CleanAuthenticated<T> {
    fn from_trusted(inner: T) -> Self { Self { inner } }
    fn inner(&self) -> &T { &self.inner }
    fn into_inner(self) -> T { self.inner }
}

impl<T> TrustedInner<T> for Unprocessed<T> {
    fn from_trusted(inner: T) -> Self { Self { inner } }
    fn inner(&self) -> &T { &self.inner }
    fn into_inner(self) -> T { self.inner }
}
```

`Externalized<T>` does **not** implement `TrustedInner<T>` because externalized data is never constructed from a domain type directly. It only appears as the output of `CleanAuthenticated<T>::externalize()`.

### 2.4 Delegate Passthrough — Domain-Specific Methods

Methods that depend on domain-type semantics cannot be generic. These use explicit passthrough via domain-specific traits:

**For `CleanAuthenticated<T>`:**
```rust
impl<T: HasExternalize> CleanAuthenticated<T> {
    pub fn externalize(self) -> T::Externalized {
        T::externalize(self.into_inner())
    }
}
```

**For `Unprocessed<T>`:**
```rust
impl<T: HasVerify> Unprocessed<T> {
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        // ... domain-specific verification context ...
    ) -> Result<CleanAuthenticated<T>, CleanAuthError> {
        T::verify(self.into_inner(), crypto, /* ... */)
    }
}
```

Each domain type implements the relevant passthrough trait. The trait defines the method signature, and the impl provides the domain-specific logic. This keeps the generic wrappers lean while preserving the per-type verification and externalization logic.

### 2.5 Wire Format Preservation

Externalized forms must preserve the wire format of the concrete Externalized types they replace. The mapping from domain fields to externalized fields is explicit and documented per type. A change to the externalization logic for any type must be accompanied by a corresponding update to the `into_unprocessed()` reconstruction logic.

**Wire format preservation requirement:** If the concrete `ExternalizedChrononRecord` serializes to a specific JSON structure, the generic `Externalized<ChrononRecord>` must serialize to the same structure. Roundtrip compatibility is mandatory: `externalize().into_unprocessed()` must produce a domain type equivalent to the original, modulo documented lossy fields.

### 2.6 Module Location

All generic wrapper types and traits live in `p2p/core-engine/src/foretias/clean_auth.rs`. Domain-specific passthrough impls also live in this file, alongside the domain type definitions they reference.

## 3. Generic Wrapper Definitions

### 3.1 `Unprocessed<T>`

```rust
/// Data parsed from the wire or disk but not yet verified.
/// Do NOT trust this data.
pub struct Unprocessed<T> {
    inner: T,
}
```

**Generic methods:**
```rust
impl<T> Unprocessed<T> {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>
    where T: serde::Deserialize;

    /// Parse from a JSON-RPC Value. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError>
    where T: serde::Deserialize;
}
```

**Trait: `HasVerify` — inbound verification gate:**
```rust
pub trait HasVerify: Sized {
    /// Verify this unprocessed value and produce a CleanAuthenticated wrapper.
    fn verify(
        self,
        crypto: &dyn CryptoServer,
        ctx: &VerifyContext,
    ) -> Result<CleanAuthenticated<Self>, CleanAuthError>;
}
```

`Unprocessed<T>::into_clean_authenticated()` delegates to `T::verify()`. Each domain type provides its own verification logic.

### 3.2 `CleanAuthenticated<T>`

```rust
/// Due diligence complete. Authenticated, sanitized, validated, normalized.
/// Safe for in-process use.
///
/// **Private fields** — zero external construction.
pub struct CleanAuthenticated<T> {
    inner: T,
}
```

**Trait: `HasExternalize` — outbound serialization gate:**
```rust
pub trait HasExternalize: Sized {
    /// The Externalized form for this domain type.
    type Externalized: serde::Serialize + serde::Deserialize + Send + Sync;

    /// Strip to minimal persistent form. No runtime context leaks.
    fn externalize(self) -> Self::Externalized;

    /// Reconstruct as Unprocessed for re-verification on load.
    fn reconstruct(self) -> Result<Unprocessed<Self>, ParseError>
    where Self: Sized;
}
```

`CleanAuthenticated<T>::externalize()` delegates to `T::externalize()`. `Externalized<U>::into_unprocessed()` delegates to `U::reconstruct()`.

### 3.3 `Externalized<T>`

```rust
/// Wire and disk format. Stripped of all runtime context.
/// Minimal fields only.
pub struct Externalized<T> {
    inner: T,
}
```

`Externalized<T>` has public fields on the inner type (which is the domain-specific externalized struct). This allows serde serialization without wrapping. The `Externalized` wrapper provides the trust-boundary marker: data at this point is wire/disk format, not domain logic.

**Note:** In the generic design, `Externalized<T>` wraps the domain-specific externalized struct (e.g., `ExternalizedChrononRecord` becomes the inner type of `Externalized<ExternalizedChrononRecord>`). Alternatively, the design can collapse `Externalized<T>` to be the domain-specific struct itself, with the type system enforcing that it only appears at trust boundaries. The spec leaves this design decision to the implementation plan.

## 4. Domain Types and Passthrough Specifications

Four domain types use the generic wrappers. Each has explicit field listings and passthrough method specifications.

### 4.1 ChrononRecord (10 fields)

**Domain type:** `p2p/core-engine/src/foretias/tick.rs:ChrononRecord`

| # | Field | Type | Description |
|---|-------|------|-------------|
| 1 | `chronon_number` | `u64` | Monotonically increasing chronon index |
| 2 | `public_key` | `FTByteVector` | Public key active at this chronon |
| 3 | `signature_algorithm` | `String` | Algorithm identifier for this chronon's key |
| 4 | `forward_foretis` | `FTByteVector` | Serialized Foretis attesting forward |
| 5 | `backward_foretis` | `FTByteVector` | Serialized Foretis attesting backward |
| 6 | `aa_nonce` | `FTByteArray<16>` | Auto-attestation nonce (16 bytes) |
| 7 | `chronon_stamp_count` | `u64` | User-initiated stamps during this chronon |
| 8 | `external_attestations` | `Vec<ExternalAttestation>` | External attestations from other Time Families |
| 9 | `tb_version` | `u32` | TBID protocol version |
| 10 | `tbid` | `Tbid` | Time Being ID |

**Externalized form fields (10 fields):**

| # | Field | Type | Notes |
|---|-------|------|-------|
| 1 | `chronon_number` | `u64` | Direct passthrough |
| 2 | `public_key` | `[u8; 32]` | Normalized to 32-byte array (Ed25519 fixed size) |
| 3 | `signature_algorithm` | `String` | Direct passthrough |
| 4 | `forward_foretis` | `Vec<u8>` | Direct passthrough |
| 5 | `backward_foretis` | `Vec<u8>` | Direct passthrough |
| 6 | `aa_nonce` | `[u8; 16]` | Direct passthrough |
| 7 | `stamps_per_tick` | `u64` | Maps from `chronon_stamp_count` |
| 8 | `external_attestations` | `Vec<ExternalizedAttestation>` | Each ExternalAttestation externalized |
| 9 | `tb_version` | `u8` | Cast from `u32` |
| 10 | `tbid` | `Vec<u8>` | Raw bytes from Tbid |

**Passthrough methods:**
- `ChrononRecord::verify()` — calls `verify_pair()` for non-genesis, structural checks for genesis. Returns `CleanAuthenticated<ChrononRecord>`.
- `ChrononRecord::externalize()` — maps 10 domain fields to 10 externalized fields (see table above).
- `ExternalizedChrononRecord::reconstruct()` — maps 10 externalized fields back to 10 domain fields, producing `Unprocessed<ChrononRecord>`.

### 4.2 Foretis (8 fields)

**Domain type:** `p2p/core-engine/src/foretias/tick.rs:Foretis`

| # | Field | Type | Description |
|---|-------|------|-------------|
| 1 | `chronon_number` | `u64` | Chronon number at attestation creation |
| 2 | `content_hash` | `FTByteArray<32>` | SHA-256 hash of attested content |
| 3 | `signature` | `FTByteVector` | Ed25519 (or PQC) signature |
| 4 | `signature_algorithm` | `String` | Algorithm identifier |
| 5 | `tbid` | `Tbid` | TimeBeing identifier |
| 6 | `echo` | `String` | Echo string (chronon identifier) |
| 7 | `tbn` | `String` | TimeBeing name |
| 8 | `time_being_reference_time` | `String` | Wall-clock time at stamping |

**Externalized form fields (7 fields):**

| # | Field | Type | Notes |
|---|-------|------|-------|
| 1 | `chronon_number` | `u64` | Direct passthrough |
| 2 | `content_hash` | `[u8; 32]` | Direct passthrough |
| 3 | `signature` | `Vec<u8>` | Direct passthrough |
| 4 | `signature_algorithm` | `String` | Direct passthrough |
| 5 | `tbid` | `Vec<u8>` | Raw bytes from Tbid |
| 6 | `echo` | `Option<String>` | Empty string becomes `None` |
| 7 | `tbn` | `u64` | Length of tbn string (lossy — full string not preserved) |

**Note:** `time_being_reference_time` is intentionally excluded from the externalized form. It is a runtime context field, not needed for reconstruction and re-verification. The field is lossy in roundtrip.

**Passthrough methods:**
- `Foretis::verify()` — rebuilds sig input from `tbid || chronon_number || content`, verifies hash, checks algorithm and chronon number, verifies signature. Returns `CleanAuthenticated<Foretis>`.
- `Foretis::externalize()` — maps 8 domain fields to 7 externalized fields. `time_being_reference_time` is dropped.
- `ExternalizedForetis::reconstruct()` — maps 7 externalized fields back to 8 domain fields. `time_being_reference_time` reconstructed as empty string. `tbn` reconstructed as empty string (lossy).

### 4.3 EpochSnapshot (8 fields)

**Domain type:** `p2p/core-engine/src/epoch/snapshot.rs:EpochSnapshot`

| # | Field | Type | Description |
|---|-------|------|-------------|
| 1 | `epoch_number` | `u64` | Epoch identifier |
| 2 | `epoch_start_ns` | `u64` | Epoch start time (nanoseconds) |
| 3 | `epoch_end_ns` | `u64` | Epoch end time (nanoseconds) |
| 4 | `peer_scores` | `Vec<PeerScore>` | Peer scores at freeze time |
| 5 | `committee` | `Vec<String>` | Committee member PeerIds (hex) |
| 6 | `threshold` | `u32` | Signing threshold k (k-of-n) |
| 7 | `frost_signature` | `FTByteVector` | Aggregate FROST-Ed25519 signature |
| 8 | `committee_pubkey` | `FTByteVector` | FROST group public key |

**Externalized form fields (6 fields):**

| # | Field | Type | Notes |
|---|-------|------|-------|
| 1 | `epoch_number` | `u64` | Direct passthrough |
| 2 | `peer_scores` | `Vec<(Vec<u8>, i32)>` | PeerId bytes and score (type change: String → Vec<u8>, f32 → i32) |
| 3 | `committee` | `Vec<Vec<u8>>` | TBID bytes (type change: String → Vec<u8>) |
| 4 | `threshold` | `usize` | Cast from `u32` |
| 5 | `frost_signature` | `Vec<u8>` | Direct passthrough |
| 6 | `committee_pubkey` | `Vec<u8>` | Direct passthrough |

**Note:** `epoch_start_ns` and `epoch_end_ns` are excluded from the externalized form. These are runtime context fields, not needed for FROST verification. Lossy in roundtrip.

**Passthrough methods:**
- `EpochSnapshot::verify()` — currently returns `CleanAuthError::NotYetImplemented`. When FROST ships, verifies aggregate signature against committee public keys. Returns `CleanAuthenticated<EpochSnapshot>`.
- `EpochSnapshot::externalize()` — maps 8 domain fields to 6 externalized fields. `epoch_start_ns` and `epoch_end_ns` are dropped.
- `ExternalizedEpochSnapshot::reconstruct()` — maps 6 externalized fields back to 8 domain fields. `epoch_start_ns` and `epoch_end_ns` reconstructed as 0.

### 4.4 ProbityReport (7 fields)

**Domain type:** `p2p/foretias-server/src/probity/report.rs:ProbityReport`

| # | Field | Type | Description |
|---|-------|------|-------------|
| 1 | `subject` | `String` | PeerId (hex) of reported peer |
| 2 | `reporter` | `String` | PeerId (hex) of reporting peer |
| 3 | `attribute` | `String` | Opaque attribute name |
| 4 | `value` | `f32` | Signed magnitude |
| 5 | `timestamp_ns` | `u64` | Observation time |
| 6 | `signature` | `Vec<u8>` | Ed25519 or P-256 signature |
| 7 | `curve` | `u8` | 1 = Ed25519, 2 = P-256 |

**Externalized form fields (6 fields):**

| # | Field | Type | Notes |
|---|-------|------|-------|
| 1 | `reporter` | `Vec<u8>` | Reporter TBID bytes |
| 2 | `subject` | `Vec<u8>` | Subject TBID bytes |
| 3 | `target` | `Vec<u8>` | Target TBID bytes (maps from `subject` in current naming) |
| 4 | `score` | `i32` | Cast from `value` (f32 → i32) |
| 5 | `timestamp_ns` | `u64` | Direct passthrough |
| 6 | `signature` | `Vec<u8>` | Direct passthrough |

**Note:** `curve` is excluded from the externalized form. The externalized form uses Ed25519 signatures exclusively for gossip transmission. If multi-curve support is needed in the future, the externalized form gains a `curve` field. The `attribute` field is also excluded from the externalized form, as gossip payloads carry only the essential attestation data.

**Passthrough methods:**
- `ProbityReport::verify()` — rebuilds canonical signing bytes, verifies Ed25519 or P-256 signature against reporter's public key, rejects if signature length is invalid. Returns `CleanAuthenticated<ProbityReport>`.
- `ProbityReport::externalize()` — maps 7 domain fields to 6 externalized fields. `curve` and `attribute` are dropped.
- `ExternalizedProbityReport::reconstruct()` — maps 6 externalized fields back to 7 domain fields. `curve` reconstructed as 1 (Ed25519 default). `attribute` reconstructed as empty string.

## 5. Passthrough Trait Design

### 5.1 Trait Definitions

```rust
/// Marker trait for domain types that can be verified from Unprocessed to CleanAuthenticated.
pub trait HasVerify: Sized + serde::Deserialize {
    /// Verify this unprocessed domain value.
    ///
    /// The `ctx` parameter provides domain-specific verification context
    /// (e.g., predecessor record for ChrononRecord, calendar record for Foretis).
    fn verify(
        self,
        crypto: &dyn CryptoServer,
        ctx: &VerifyContext,
    ) -> Result<CleanAuthenticated<Self>, CleanAuthError>;
}

/// Marker trait for domain types that can be externalized to wire/disk form.
pub trait HasExternalize: Sized {
    /// The externalized (wire/disk) form for this domain type.
    type Externalized: serde::Serialize + serde::Deserialize + Send + Sync;

    /// Strip to minimal persistent form.
    fn externalize(self) -> Self::Externalized;

    /// Reconstruct from externalized form for re-verification.
    fn reconstruct(ext: Self::Externalized) -> Result<Unprocessed<Self>, ParseError>;
}
```

### 5.2 Generic Wrapper Passthrough

```rust
impl<T: HasVerify> Unprocessed<T> {
    /// Verify and produce a CleanAuthenticated wrapper.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        ctx: &VerifyContext,
    ) -> Result<CleanAuthenticated<T>, CleanAuthError> {
        T::verify(self.into_inner(), crypto, ctx)
    }
}

impl<T: HasExternalize> CleanAuthenticated<T> {
    /// Strip to minimal persistent form. No runtime context leaks.
    pub fn externalize(self) -> T::Externalized {
        T::externalize(self.into_inner())
    }
}

impl<T: HasExternalize> T::Externalized {
    /// Reconstruct as Unprocessed for re-verification on load.
    pub fn into_unprocessed(self) -> Result<Unprocessed<T>, ParseError> {
        T::reconstruct(self)
    }
}
```

### 5.3 Verification Context

Domain-specific verification requires different context:

```rust
pub enum VerifyContext {
    /// ChrononRecord verification: predecessor record required.
    ChrononRecord {
        prev: CleanAuthenticated<ChrononRecord>,
        is_genesis: bool,
    },
    /// Foretis verification: calendar record and content required.
    Foretis {
        record: CleanAuthenticated<ChrononRecord>,
        content: Vec<u8>,
    },
    /// EpochSnapshot verification: committee public keys and threshold.
    EpochSnapshot {
        committee_pubkeys: Vec<SignatureBytes>,
        threshold: usize,
    },
    /// ProbityReport verification: reporter's public key.
    ProbityReport {
        reporter_pub_key: [u8; 32],
    },
}
```

### 5.4 Passthrough Update Discipline

When a domain type's verification or externalization logic needs to change, the corresponding passthrough impl must be updated. **Passthrough updates require corresponding validation improvements.** For example, if `ChrononRecord` gains a new field that affects verification, the `HasVerify` impl must be updated to handle the new field. If the externalization logic needs to include a new field, both `externalize()` and `reconstruct()` must be updated symmetrically.

## 6. Wire Format Preservation

### 6.1 Preservation Requirement

The wire format of externalized types must remain compatible with the concrete Externalized types they replace. This means:

1. **Field names must match.** If `ExternalizedChrononRecord` has a field named `stamps_per_tick`, the externalized JSON must still use that name.
2. **Field types must match.** A `u64` stays a `u64`. A `[u8; 32]` stays a `[u8; 32]`.
3. **Serialization order must match.** For formats where field order matters (like binary serialization), the order is preserved.
4. **Default values must match.** Fields with `#[serde(default)]` retain their default behavior.

### 6.2 Roundtrip Testing

Each domain type must have a roundtrip test:

```rust
fn test_roundtrip<T: HasExternalize + PartialEq + Debug>()
where
    T::Externalized: Into<Unprocessed<T>>,
{
    let original = make_valid::<T>();
    let ca = CleanAuthenticated::from_trusted(original.clone());
    let ext = ca.externalize();
    let reconstructed = ext.into_unprocessed().unwrap();
    // Compare non-lossy fields
    assert_fields_match(&original, reconstructed.inner());
}
```

### 6.3 Lossy Fields Documentation

When a field is intentionally lossy in the externalization/reconstruction cycle, the `HasExternalize` impl must document which fields are lossy and why. The field listing tables in §4 document these cases:

| Type | Lossy Field | Reason |
|------|-------------|--------|
| Foretis | `time_being_reference_time` | Runtime context, not needed for verification |
| Foretis | `tbn` | Only length preserved in externalized form |
| EpochSnapshot | `epoch_start_ns` | Runtime context, not needed for FROST verification |
| EpochSnapshot | `epoch_end_ns` | Runtime context, not needed for FROST verification |
| ProbityReport | `curve` | Externalized form defaults to Ed25519 |
| ProbityReport | `attribute` | Not included in gossip wire format |

## 7. Static Analysis Test

### 7.1 Purpose

A compile-time static analysis test verifies that the trust-boundary discipline is enforced across the codebase. The test uses a `syn`-based visitor to scan all Rust source files and cross-tabulate type usage against the permission table.

### 7.2 Permission Table (Snapshot)

The permission table defines which wrapper types may appear in which locations:

| Wrapper Type | Communerd Inbound | Communerd Outbound | Intra-Family | Domain Logic | Wire/Disk |
|-------------|-------------------|-------------------|--------------|--------------|-----------|
| `Unprocessed<T>` | ✅ (handler params) | ❌ | ❌ | ❌ | ❌ |
| `CleanAuthenticated<T>` | ✅ (handler output) | ✅ (input) | ✅ | ✅ | ❌ |
| `Externalized<T>` | ✅ (deserialize target) | ✅ (serialize source) | ❌ | ❌ | ✅ |

### 7.3 Static Analysis Implementation

```rust
// Test: trust_boundary_static_analysis
// Uses syn::visit::Visit to scan all .rs files in the workspace.
// For each type usage found, cross-tabulate against the permission table.
// Report violations as test failures.

#[test]
fn trust_boundary_static_analysis() {
    let workspace_files = glob::glob("p2p/**/*.rs").unwrap().collect::<Vec<_>>();
    let mut violations = Vec::new();

    for file_path in &workspace_files {
        let source = std::fs::read_to_string(file_path).unwrap();
        let ast = syn::parse_file(&source).unwrap();

        let mut visitor = TrustBoundaryVisitor::new();
        visitor.visit_file(&ast);

        for usage in visitor.usages() {
            if !permission_table.allows(usage.wrapper_type, usage.location) {
                violations.push(Violation {
                    file: file_path.to_string(),
                    line: usage.line,
                    wrapper: usage.wrapper_type,
                    location: usage.location,
                    expected: permission_table.allowed_for(usage.location),
                });
            }
        }
    }

    assert!(violations.is_empty(),
        "Trust boundary violations found:\n{}",
        violations.iter().map(|v| format!("  {}:{} — {} in {:?}",
            v.file, v.line, v.wrapper, v.location)).join("\n"));
}
```

### 7.4 What the Visitor Checks

The `syn`-based visitor identifies:

1. **Type occurrences** — `Unprocessed<T>`, `CleanAuthenticated<T>`, `Externalized<T>` in function signatures, struct fields, type annotations.
2. **Module context** — which module/file the occurrence is in (communerd, server, calendar, core-engine, etc.).
3. **Usage context** — function parameter, return type, struct field, local variable, generic type parameter.

The visitor does **not** check raw bytes or strings. It operates on the AST level, checking Rust type annotations.

### 7.5 Permission Table as Snapshot

The permission table is maintained as a test snapshot. When the table is updated (e.g., a new allowed location is added), the test must be updated to reflect the new rules. This prevents accidental relaxation of trust boundaries through silent test updates.

### 7.6 Out of Scope for Static Analysis

**Permission table refinements** (fine-grained file/struct/regex markers) are out of scope for this spec. The static analysis test uses module-level location categories. Future work may refine the table to file-level, struct-level, or regex-based markers for more granular enforcement. This refinement is explicitly excluded from the current spec.

## 8. Error Types

Error types remain shared across the generic system:

```rust
pub enum CleanAuthError {
    Parse(ParseError),
    InvalidSignature,
    AlgorithmMismatch,
    ChainBreak,
    ReplayDetected,
    UnknownPeer,
    NotYetImplemented,
    InvalidLength(String),
    Crypto(NodeError),
}

pub enum ParseError {
    InvalidJson(serde_json::Error),
    TruncatedBytes,
    InvalidLength(String),
}
```

These are unchanged from the concrete wrapper spec.

## 9. Handler Refactoring Rules

### 9.1 General Pattern

Every handler that accepts inbound data follows the same pattern:

```rust
// 1. Parse to Unprocessed (fails on malformed input)
let unprocessed = Unprocessed::from_bytes(&raw)?;

// 2. Clean & authenticate (fails on cryptographic failure)
let ca = unprocessed.into_clean_authenticated(&self.crypto, &ctx)?;

// 3. Consume clean-authenticated data (compiler guarantees this is safe)
self.store.insert(ca.inner())?;
```

### 9.2 Handler Type Mapping

| Handler | New Unprocessed Type | Verification Context |
|---------|---------------------|---------------------|
| `handle_ship_ack` | `Unprocessed<ChrononRecord>` | `VerifyContext::ChrononRecord { prev, is_genesis }` |
| `handle_stream_tick` | `Unprocessed<ChrononRecord>` | `VerifyContext::ChrononRecord { prev, is_genesis: false }` |
| `handle_verify` (local) | `Unprocessed<Foretis>` | `VerifyContext::Foretis { record, content }` |
| `cross_node_verify` | `Unprocessed<ChrononRecord>` | `VerifyContext::ChrononRecord { prev, is_genesis: false }` |
| `handle_verify_epoch_snapshot` | `Unprocessed<EpochSnapshot>` | `VerifyContext::EpochSnapshot { committee_pubkeys, threshold }` |
| `gossip_handler::handle_gossip_message` | `Unprocessed<ProbityReport>` | `VerifyContext::ProbityReport { reporter_pub_key }` |
| `stamp_peer` / `route_stamp` | `Unprocessed<Foretis>` | `VerifyContext::Foretis { record, content }` |
| `get_calendar_slice` (remote) | `Vec<Unprocessed<ChrononRecord>>` | Batch verify: chain each against predecessor |

## 10. Migration from Concrete Wrappers

### 10.1 Migration Steps

1. Define generic wrapper structs (`Unprocessed<T>`, `CleanAuthenticated<T>`, `Externalized<T>`) in `clean_auth.rs`.
2. Define `TrustedInner<T>` trait with generic impls.
3. Define `HasVerify` and `HasExternalize` traits.
4. Implement `HasVerify` and `HasExternalize` for each of the four domain types.
5. Update handler code to use generic wrappers.
6. Remove concrete wrapper structs.
7. Add static analysis test.
8. Verify `cargo test --workspace` passes.

### 10.2 Compatibility

During migration, both concrete and generic wrappers may coexist briefly. Type aliases can bridge the gap:

```rust
// Temporary compatibility aliases — removed after migration completes
type UnprocessedChrononRecord = Unprocessed<ChrononRecord>;
type CleanAuthenticatedChrononRecord = CleanAuthenticated<ChrononRecord>;
type ExternalizedChrononRecord = Externalized<ExternalizedChrononRecordData>;
```

These aliases are removed once all handler code is updated.

## 11. Out of Scope

- **Permission table refinements** — fine-grained file/struct/regex markers for static analysis. This is future work. The current static analysis uses module-level categories.
- **Secret material handling** — covered by `SECRET_TYPE_DISCIPLINE_SPEC.md` / `HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md`
- **FFI length validation** — covered by `FFI_LENGTH_VALIDATION_SPEC.md`
- **Clock injection** — covered by `CLOCK_INJECTION_SPEC.md`
- **FROST implementation** — covered by `FROST_IMPLEMENTATION_SPEC.md`
- **Canonical encoding** — covered by `CANONICAL_ENCODING_SPEC.md`
- **Provenance metadata** — belongs at the storage layer, not the type layer
- **Domain type definition changes** — this spec does not change domain type field definitions. The field listings in §4 are documentation of existing types.

## 12. Verification Checklist

Before this spec is considered implemented:

- [ ] Three generic wrapper structs defined in `clean_auth.rs`
- [ ] `TrustedInner<T>` trait with generic impls for `CleanAuthenticated<T>` and `Unprocessed<T>`
- [ ] `HasVerify` trait implemented for all four domain types
- [ ] `HasExternalize` trait implemented for all four domain types
- [ ] All handler code updated to use generic wrappers
- [ ] Concrete wrapper structs removed (or aliased during migration)
- [ ] Wire format preservation verified via roundtrip tests
- [ ] Static analysis test passes (syn-based visitor, cross-tabulation against permission table)
- [ ] Permission table snapshot maintained as test fixture
- [ ] `cargo test --workspace` passes
- [ ] No regression in existing tests
- [ ] No `unwrap()` in verification paths
