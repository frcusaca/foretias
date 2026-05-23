# COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md

**Date:** 2026-05-22
**Application:** Foretias v0.3+
**Supersedes:** `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md` (Take 2)
**Take 1:** `TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_SPEC.md`
**Paired Plan:** `COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_PLAN.md`

---

## 1. Purpose

Fix the incomplete Take 2 refactoring of `clean_auth.rs`. Take 2 defined the generic wrapper
types (`Unprocessed<T>`, `CleanAuthenticated<T>`, `Externalized<T>`) but then added concrete
newtype structs on top (`UnprocessedChrononRecord(pub Unprocessed<ChrononRecord>)` etc.),
which doubles the type count and requires a `delegate!` macro to pass through every method call.

This spec describes the correct end state. The paired plan describes how to get there.

---

## 2. The Problem with Take 2

Take 2 created this structure:

```
Unprocessed<T>                           (generic wrapper, correct)
UnprocessedChrononRecord                 (concrete newtype wrapping the generic, WRONG)
  └── pub Unprocessed<ChrononRecord>     (the `.0` field)

CleanAuthenticated<T>                    (generic wrapper, correct)
CleanAuthenticatedChrononRecord          (concrete newtype, WRONG)
  └── CleanAuthenticated<ChrononRecord>  (the `.0` field)
```

Consequences:
- Six types instead of three for each domain object (one generic + one concrete per stage)
- `delegate::delegate!` macro required for every field accessor because the newtype has no
  direct access to the inner fields
- `TrustedInner::<ChrononRecord>::inner(&self.0)` gymnastics everywhere
- New domain types require writing three new structs, not zero
- The `pub` on `.0` leaks the inner generic, weakening the encapsulation the design intended

---

## 3. Correct End State

### 3.1 No concrete newtype structs

`UnprocessedChrononRecord` must not be a `struct`. Callers write:
```rust
Unprocessed<ChrononRecord>       // instead of UnprocessedChrononRecord
CleanAuthenticated<ChrononRecord> // instead of CleanAuthenticatedChrononRecord
```

### 3.2 Type aliases (optional, for readability only)

If desired for call-site readability, type aliases may be introduced with a macro:
```rust
macro_rules! create_type_gated_classes {
    ($t:ident, $up:ident, $ca:ident) => {
        pub type $up = Unprocessed<$t>;
        pub type $ca = CleanAuthenticated<$t>;
    };
}

create_type_gated_classes!(ChrononRecord, UnprocessedChrononRecord, CleanAuthenticatedChrononRecord);
```

However, type aliases must NOT appear alongside the concrete newtypes they were previously. The
implementation should use the generic types directly (`Unprocessed<ChrononRecord>`) wherever
possible, with type aliases only at public API surfaces if needed.

### 3.3 Inherent methods on the generic types (no delegate needed)

Methods that were on the newtypes move directly onto `impl Unprocessed<ChrononRecord>` and
`impl CleanAuthenticated<ChrononRecord>`. Since `clean_auth.rs` defines the `Unprocessed<T>`
type, it can write inherent impls for any concrete T, accessing `self.inner` directly:

```rust
impl Unprocessed<ChrononRecord> {
    pub fn chronon_number(&self) -> &u64 { &self.inner.chronon_number }
    pub fn public_key(&self) -> &FTByteVector { &self.inner.public_key }
    // ... remaining field accessors
}
```

No `delegate!` macro, no `TrustedInner::<ChrononRecord>::inner(&self.0)` gymnastics.

### 3.4 Generic inherent methods (for any T)

Methods that apply to all domain types move to `impl<T> Unprocessed<T>` and
`impl<T> CleanAuthenticated<T>`:

```rust
impl<T> Unprocessed<T> {
    pub fn from_parsed(inner: T) -> Self { Self { inner } }
    pub fn inner(&self) -> &T { &self.inner }
    pub fn into_inner(self) -> T { self.inner }
}

impl<T: serde::de::DeserializeOwned> Unprocessed<T> {
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> { ... }
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError> { ... }
}

impl<T> CleanAuthenticated<T> {
    pub fn from_trusted(inner: T) -> Self { Self { inner } }
    pub fn inner(&self) -> &T { &self.inner }
    pub fn into_inner(self) -> T { self.inner }
}
```

Callers no longer need `use crate::foretias::clean_auth::TrustedInner` to access these.

### 3.5 `verify` method — inbound gate (single point of code review, outside the macro)

No trait is needed. Each domain type gets a `verify` method written directly on
`impl Unprocessed<T>`. Because `Unprocessed<T>` is defined in `core-engine`, inherent impls for
any concrete `T` that is also defined in `core-engine` can live right alongside the generic
definition. Each impl is its own explicit, auditable block.

Each type's `verify` takes exactly the parameters that type's verification requires — no shared
`VerifyContext` enum and no generic passthrough trait:

```rust
// Inbound gate for ChrononRecord.
// Written outside the macro, in clean_auth.rs or a companion file.
impl Unprocessed<ChrononRecord> {
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        prev: Option<&CleanAuthenticated<ChrononRecord>>,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError> {
        // All auth, sanity, and range checks for ChrononRecord here.
        // prev = None means genesis (tick 1).
        todo!()
    }
}

// Inbound gate for Foretis.
impl Unprocessed<Foretis> {
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        record: &CleanAuthenticated<ChrononRecord>,
        content: &[u8],
    ) -> Result<CleanAuthenticated<Foretis>, CleanAuthError> {
        todo!()
    }
}

// Inbound gate for EpochSnapshot.
impl Unprocessed<EpochSnapshot> {
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        committee_pubkeys: &[SignatureBytes],
        threshold: usize,
    ) -> Result<CleanAuthenticated<EpochSnapshot>, CleanAuthError> {
        todo!()
    }
}
```

The existing ergonomic helpers (`into_clean_authenticated`, `into_clean_authenticated_genesis`)
may remain as thin wrappers that call `verify`, or be removed if callers are updated directly.

### 3.6 `externalize` method — outbound gate (single point of code review, outside the macro)

No trait is needed. Each domain type gets an `externalize` method written directly on
`impl CleanAuthenticated<T>`. Written outside the macro, one explicit impl per type:

```rust
// Outbound gate for ChrononRecord.
impl CleanAuthenticated<ChrononRecord> {
    pub fn externalize(self) -> ExternalizedChrononRecord {
        // Strip to minimal persistent/wire form here.
        todo!()
    }
}

// Outbound gate for Foretis.
impl CleanAuthenticated<Foretis> {
    pub fn externalize(self) -> ExternalizedForetis {
        todo!()
    }
}

// Outbound gate for EpochSnapshot.
impl CleanAuthenticated<EpochSnapshot> {
    pub fn externalize(self) -> ExternalizedEpochSnapshot {
        todo!()
    }
}
```

Reconstruction from externalized form (for re-verification) is a free function or an associated
function on the `Externalized*` type itself, also written outside the macro:

```rust
impl ExternalizedChrononRecord {
    pub fn reconstruct(self) -> Result<Unprocessed<ChrononRecord>, ParseError> { todo!() }
}

---

## 4. The create_type_gated_classes! Macro

The macro creates type aliases and is the only location where alias names are registered:

```rust
macro_rules! create_type_gated_classes {
    ($t:ident, $up:ident, $ca:ident) => {
        pub type $up = Unprocessed<$t>;
        pub type $ca = CleanAuthenticated<$t>;
    };
}
```

The verification and externalization logic is intentionally **outside the macro**. Each
`HasVerify` and `HasExternalize` impl is its own explicit, auditable code block. The macro
registers only names, not logic.

To add a new domain type `Foo`:
1. Call `create_type_gated_classes!(Foo, UnprocessedFoo, CleanAuthenticatedFoo)`
2. Write `impl HasVerify for Foo { fn verify(...) { /* inbound gate */ } }`
3. Write `impl HasExternalize for Foo { type Externalized = ExternalizedFoo; ... }`
4. Write `impl Unprocessed<Foo>` field accessors
5. Write `impl CleanAuthenticated<Foo>` field accessors

---

## 5. ProbityReport — Moved to core-engine

`ProbityReport` is moved from `foretias-server` to `core-engine` as part of this refactoring.
This eliminates the only coherence exception: once the type is in `core-engine`, the inherent
impls `impl Unprocessed<ProbityReport>` and `impl CleanAuthenticated<ProbityReport>` can live
there alongside the other domain types, following exactly the same pattern.

**What moves to `core-engine`:**
- The `ProbityReport` struct definition (fields only)
- `ExternalizedProbityReport` struct
- `impl Unprocessed<ProbityReport> { pub fn verify(...) }`
- `impl CleanAuthenticated<ProbityReport> { pub fn externalize(...) }`
- Field accessors on both wrappers

**What stays in `foretias-server`:**
- Gossip handler (`gossip_handler.rs`)
- Probity store (`ProbityStore`)
- Aggregator and any server-specific business logic
- All of the above continue to import `Unprocessed<ProbityReport>` and
  `CleanAuthenticated<ProbityReport>` from `foretias-core` — no concrete newtypes needed

**No circular dependency:** `ProbityReport` verification only requires `&dyn CryptoServer`,
which is already defined in `core-engine`. Nothing from `foretias-server` is needed.

**Concrete newtypes eliminated:**
```rust
// DELETED from foretias-server/src/probity/clean_auth.rs:
pub struct UnprocessedProbityReport(pub Unprocessed<ProbityReport>);
pub struct CleanAuthenticatedProbityReport(CleanAuthenticated<ProbityReport>);
```

After this change there are **zero** coherence exceptions in the workspace.

---

## 6. Static Analysis Table — Updated Format

The `trust_boundary_type_usage.rs` snapshot test output format must change. Instead of one row
per (file, inner-type, context) triple, it must produce **one row per function-signature position**:

"""markdown
OUTPUT:
```markdown
| file          | struct_or_impl | method | valence         |  inner_type    | Unprocessed | CleanAuthenticated | Externalized | Comments   |
| path/to/lib.rs| ExampleStrut   | test   | return          |  ChrononRecord | Y           | N                  | N            | human says |
| path/to/lib.rs| ExampleStrut   | test   | param:input_one |  ChrononRecord | Y           | Y                  | N            | WHAT??? NO!|
```
"""

Where:
- `struct_or_impl` — enclosing struct or impl type name
- `method` — function name
- `valence` — `return`, `param:formal_parameter_name_1`, `param:formal_parameter_name_2`, 
- `inner_type` — the `T` in `Unprocessed<T>` / `CleanAuthenticated<T>` / `Externalized<T>`
- `Unprocessed` / `CleanAuthenticated` / `Externalized` — boolean (`Y` / `-`) columns
- 'comment' in case human wants to type something.

The syn visitor must be updated to:
1. Visit function signatures specifically (not just any type occurrence)
2. For each `FnArg::Typed` parameter, record role as `param_N`
3. For the `ReturnType`, record role as `return`
4. Only emit rows for positions that contain one of the three wrapper types

This format allows static review of which functions cross the trust boundary at each parameter
position and whether the types are correct (e.g., a function returning `CleanAuthenticated<T>`
that takes `Unprocessed<T>` is an inbound gate and should be in a known location).

The snapshot should be cryptographically signed as per usual for snapshot_suite.

---

## 7. Verification Checklist (pre-merge)

- [ ] No `struct UnprocessedXxx` or `struct CleanAuthenticatedXxx` in `core-engine/src/foretias/clean_auth.rs`
- [ ] `Unprocessed<T>` has generic inherent methods (`from_parsed`, `inner`, `into_inner`, `from_bytes`, `from_json_value`)
- [ ] `CleanAuthenticated<T>` has generic inherent methods (`from_trusted`, `inner`, `into_inner`)
- [ ] No `HasVerify` trait — instead, `impl Unprocessed<ChrononRecord>`, `impl Unprocessed<Foretis>`, `impl Unprocessed<EpochSnapshot>` each have a concrete `verify(...)` method outside the macro
- [ ] No `HasExternalize` trait — instead, `impl CleanAuthenticated<ChrononRecord>`, `impl CleanAuthenticated<Foretis>`, `impl CleanAuthenticated<EpochSnapshot>` each have a concrete `externalize(...)` method outside the macro
- [ ] No `VerifyContext` enum — each `verify` takes the exact parameters its type requires
- [ ] `create_type_gated_classes!` macro generates type aliases only (no logic)
- [ ] `delegate::delegate!` removed from `core-engine/src/foretias/clean_auth.rs`
- [ ] `delegate` removed from `core-engine/Cargo.toml` dependencies (if not used elsewhere)
- [ ] All existing call sites in `foretias-server`, `foretias-client`, `chronomatter` compile unchanged
- [ ] `e2e_verification.rs` test uses `Unprocessed::<ChrononRecord>::from_parsed(record)` not the tuple constructor
- [ ] Trust boundary snapshot test updated to per-signature-position row format
- [ ] `ProbityReport` moved to `core-engine`; `UnprocessedProbityReport` and `CleanAuthenticatedProbityReport` concrete newtypes deleted from `foretias-server`
- [ ] `cargo test --workspace` passes with zero test failures
