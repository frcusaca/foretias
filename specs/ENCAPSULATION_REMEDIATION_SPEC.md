# ENCAPSULATION_REMEDIATION_SPEC.md

**Spec: Encapsulation and Ownership Remediation**
**Status: PROPOSED**
**Date: 2026-06-07**
**Source: AGENTS.md § Encapsulation + codebase audit**

---

## 1. Overview

The codebase audit revealed that domain types have public fields that allow bypassing construction invariants. This spec defines the remediation: make fields private, add accessor methods where missing, and fix all direct field access sites.

### Principles (from AGENTS.md § Encapsulation)

- Fields are strictly private — callers go through methods
- A type's invariants are enforced by its own constructor/methods
- `&mut self` for in-place mutation; Typestate Pattern for type changes
- A type answers questions about itself via methods

---

## 2. Audit Results

### Tier 1: Security-Critical Domain Types

| Type | File | Public Fields | Accessors | Status |
|------|------|--------------|-----------|--------|
| `ChrononRecord` | `core-engine/foretias/tick.rs` | 10 | All 10 | ❌ Fields should be private |
| `ForetisRecord` | `core-engine/foretias/tick.rs` | 6 | All 6 | ❌ Fields should be private |
| `ExternalAttestationRecord` | `core-engine/foretias/external_attestation.rs` | 6 | **ZERO** | ❌ Need accessors + private fields |
| `StampedForetis` | `core-engine/foretias/tick.rs` | 3 | **ZERO** | ❌ Need accessors + private fields |
| `Calendar` | `core-engine/foretias/calendar.rs` | 4 | **ZERO** | ❌ `ticks` bypasses `append()` ordering |
| `Heartbeat` | `core-engine/collision/heartbeat.rs` | 5 | **ZERO** | ❌ Signed message, fields writable |
| `FamilyRecord` | `core-engine/foretias/family_record.rs` | 2 | Only `k()` | ❌ Validation bypass |

### Tier 2: Internal Types with Security Implications

| Type | File | Public Fields | Accessors | Status |
|------|------|--------------|-----------|--------|
| `SignatureEntry` | `core-engine/foretias/clean_auth.rs` | 4 | **ZERO** | ⚠️ Used in trust-boundary chains |
| `ProbityReportRecord` | `core-engine/probity/report.rs` | 8 | All 8 | ⚠️ Fields still writable |
| `EpochSnapshotRecord` | `core-engine/epoch/snapshot.rs` | 8 | All 8 | ⚠️ Fields still writable |
| `SoftwareCryptoServer` | `core-engine/crypto_server/software.rs` | 4 PQC keys | **ZERO** | ⚠️ Key replacement risk |

### Tier 3: Lower Risk

| Type | File | Public Fields | Accessors | Status |
|------|------|--------------|-----------|--------|
| `SealedBlob` | `core-engine/crypto_server/mod.rs` | 2 | **ZERO** | Low |
| `CryptoServerCapabilities` | `core-engine/crypto_server/mod.rs` | 7 | **ZERO** | Low |
| `PeerScore` | `core-engine/epoch/snapshot.rs` | 2 | **ZERO** | Low |
| `TopProbitySelector` | `core-engine/epoch/committee.rs` | 2 | **ZERO** | Low |
| `PeerAddr` | `core-engine/foretias/callbacks.rs` | 1 | **ZERO** | Low |

### Tier 4: Acceptable (No Change)

- `core/bindings.rs` FFI structs — must be `pub` for C interop
- Config structs — intentionally `pub` for serde
- Trust boundary wrappers — already correct (`inner` private, `signatures` `pub(crate)`)

---

## 3. Remediation Plan

### Phase 1: ChrononRecord + ForetisRecord (highest priority)

These already have accessor methods. The fix is:
1. Make all fields `pub(crate)` (or `pub(super)`) instead of `pub`
2. Fix all direct field access sites to use accessor methods
3. Keep builder pattern for construction

**Estimated impact:** ~100 direct field access sites across core-engine, foretias-server, foretias-client

### Phase 2: ExternalAttestationRecord + StampedForetis + Heartbeat + FamilyRecord

These have ZERO accessor methods. The fix is:
1. Add accessor methods (read-only references)
2. Make all fields `pub(crate)` (or `pub(super)`)
3. Fix all direct field access sites

**Estimated impact:** ~50 direct field access sites

### Phase 3: Calendar

The `ticks` field is the most dangerous — bypasses `Calendar::append()` ordering invariant. The fix is:
1. Make `ticks` private
2. Add accessor methods: `ticks()`, `tick_at(n)`, `latest_tick()`, `tick_count()`
3. Fix all direct `ticks` access sites

**Estimated impact:** ~30 direct field access sites

### Phase 4: SignatureEntry + ProbityReportRecord + EpochSnapshotRecord

These have accessor methods but fields are still writable. The fix is:
1. Make fields `pub(crate)` instead of `pub`
2. Fix direct access sites

### Phase 5: SoftwareCryptoServer + Lower Risk Types

These are lower priority. The fix is:
1. Add accessor methods where missing
2. Make fields `pub(crate)`

---

## 4. Scope

### In Scope

- Make domain type fields private (`pub(crate)` or `pub(super)`)
- Add accessor methods where missing
- Fix all direct field access sites to use accessor methods
- Verify `cargo test --workspace` passes after each phase

### Out of Scope

- Config structs (intentionally `pub` for serde)
- FFI bindings (intentionally `pub` for C interop)
- Trust boundary wrappers (already correct)
- Builder pattern changes (already correct)

---

## 5. Implementation TODO

### Phase 1: ChrononRecord + ForetisRecord
- [ ] Make ChrononRecord fields `pub(crate)`
- [ ] Make ForetisRecord fields `pub(crate)`
- [ ] Fix all direct field access sites in core-engine
- [ ] Fix all direct field access sites in foretias-server
- [ ] Fix all direct field access sites in foretias-client
- [ ] Verify tests pass

### Phase 2: ExternalAttestationRecord + StampedForetis + Heartbeat + FamilyRecord
- [ ] Add accessor methods to ExternalAttestationRecord
- [ ] Add accessor methods to StampedForetis
- [ ] Add accessor methods to Heartbeat
- [ ] Add accessor methods to FamilyRecord
- [ ] Make fields `pub(crate)`
- [ ] Fix all direct field access sites
- [ ] Verify tests pass

### Phase 3: Calendar
- [ ] Make Calendar fields `pub(crate)`
- [ ] Add accessor methods: `ticks()`, `tick_at()`, `latest_tick()`, `tick_count()`
- [ ] Fix all direct `ticks` access sites
- [ ] Verify tests pass

### Phase 4: SignatureEntry + ProbityReportRecord + EpochSnapshotRecord
- [ ] Make SignatureEntry fields `pub(crate)`
- [ ] Make ProbityReportRecord fields `pub(crate)`
- [ ] Make EpochSnapshotRecord fields `pub(crate)`
- [ ] Fix direct access sites
- [ ] Verify tests pass

### Phase 5: SoftwareCryptoServer + Lower Risk Types
- [ ] Add accessor methods to SoftwareCryptoServer
- [ ] Make fields `pub(crate)` where appropriate
- [ ] Verify tests pass
