# ENCAPSULATION_REVIEW_FIXES_SPEC.md

**Spec: Encapsulation Review Fixes — OCR Findings**
**Paired Plan: ENCAPSULATION_REVIEW_FIXES_PLAN.md**
**Status: PROPOSED**
**Date: 2026-06-08**
**Source: Alibaba Open Code Review of encapsulation remediation commits**

---

## 1. Overview

Alibaba Open Code Review (OCR) found 9 issues across the encapsulation remediation commits. This spec defines the fixes and the tests needed to prevent regression.

### OCR Findings Summary

| # | Severity | Finding | File |
|---|----------|---------|------|
| 1 | HIGH | Setters allow mutation after signature | `communerd/mod.rs` |
| 2 | HIGH | Internal signing flow uses struct literals | `communerd/mod.rs` |
| 3 | MEDIUM | Use `is_genesis()` method | `server/handlers.rs` |
| 4 | MEDIUM | MirrorState accessors return cloned collections | `communerd/communerdette.rs` |
| 5 | MEDIUM | Setter methods lack validation | `calendar.rs` |
| 6 | MEDIUM | `tick_at()` uses linear scan | `calendar.rs` |
| 7 | MEDIUM | Atomic ordering too strong | `communerdette.rs` |
| 8 | LOW | Unnecessary clone | `server/mod.rs` |
| 9 | LOW | Dead code | `communerdette.rs` |

---

## 2. Detailed Fixes

### Fix 1: Setters Allow Mutation After Signature (HIGH)

**Problem:** `PeerRegistrationRecord::set_multiaddr()` and `set_peer_id()` can silently invalidate the signature after it's been generated. The canonical payload includes these fields, so changing them after signing produces an invalid record.

**Solution:** Clear the signature when canonical-payload fields change.

```rust
pub fn set_multiaddr(&mut self, multiaddr: String) {
    self.multiaddr = multiaddr;
    self.signature.clear(); // Invalidate signature
}

pub fn set_peer_id(&mut self, peer_id: String) {
    self.peer_id = peer_id;
    self.signature.clear(); // Invalidate signature
}
```

**Test:** `test_setter_clears_signature` — set signature, then call setter, verify signature is empty.

### Fix 2: Internal Signing Flow Uses Struct Literals (HIGH)

**Problem:** `communerd/mod.rs:1128-1143` still uses `peer_record.signature = sig.bytes.to_vec()` instead of `set_signature()`. Inconsistent with encapsulation goal.

**Solution:** Replace struct literal access with setter method.

**Test:** Already covered by `dht_record_signature.rs` integration test.

### Fix 3: Use `is_genesis()` Method (MEDIUM)

**Problem:** `server/handlers.rs:835,1311` uses `*unproc.inner().chronon_number() == 1` instead of `unproc.inner().is_genesis()`.

**Solution:** Replace with `is_genesis()` call.

**Test:** Already covered by existing genesis tests.

### Fix 4: MirrorState Accessors Return Cloned Collections (MEDIUM)

**Problem:** `MirrorState::mirrors()` returns `HashSet<String>` clone on every call. Could be expensive in high-frequency access.

**Solution:** Document the trade-off. Returning `RwLockReadGuard` would require lifetime management and could cause deadlocks if held across `.await`. The clone approach is safer for async code.

**Test:** N/A — this is a design decision, not a bug.

### Fix 5: Setter Methods Lack Validation (MEDIUM)

**Problem:** `Calendar::set_tbid()` and `set_tbn()` have no validation. Could be called on an already-initialized calendar.

**Solution:** Add `debug_assert!` to prevent misuse.

```rust
pub fn set_tbid(&mut self, tbid: Tbid) {
    debug_assert!(self.tbid == Tbid::default(), "set_tbid called on already-initialized calendar");
    self.tbid = tbid;
}

pub fn set_tbn(&mut self, tbn: &str) {
    debug_assert!(self.tbn.is_empty(), "set_tbn called on already-initialized calendar");
    self.tbn = tbn.to_string();
}
```

**Test:** `test_set_tbid_panics_if_already_set` — call set_tbid twice, verify panic in debug mode.

### Fix 6: `tick_at()` Uses Linear Scan (MEDIUM)

**Problem:** `Calendar::tick_at()` uses `iter().find()` which is O(n). Since ticks are ordered by `chronon_number`, binary search would be O(log n).

**Solution:** Use `binary_search_by()`.

```rust
pub fn tick_at(&self, n: u64) -> Option<&ChrononRecord> {
    self.ticks
        .binary_search_by(|t| t.chronon_number().cmp(&n))
        .ok()
        .map(|idx| &self.ticks[idx])
}
```

**Test:** `test_tick_at_binary_search` — verify `tick_at()` returns correct tick for various chronon numbers, including missing ticks.

### Fix 7: Atomic Ordering Too Strong (MEDIUM)

**Problem:** `LivenessCycleFlags` uses `SeqCst` for all atomic operations. These are independent liveness signals with no cross-variable ordering constraints.

**Solution:** Use `Acquire`/`Release` instead of `SeqCst`. This provides happens-before relationship without the full sequential consistency overhead.

**Test:** Already covered by existing liveness tests. The ordering change is safe because `Acquire`/`Release` is strictly weaker than `SeqCst` but still correct for this use case.

### Fix 8: Unnecessary Clone (LOW)

**Problem:** `server/mod.rs:94` uses `set_tbn(&tbn.clone())` instead of `set_tbn(&tbn)`.

**Solution:** Remove the clone.

**Test:** N/A — pure cleanup.

### Fix 9: Dead Code (LOW)

**Problem:** `set_binding_rejected` is marked `#[allow(dead_code)]` — only used in tests.

**Solution:** Keep the method but add a comment explaining it's for test infrastructure.

**Test:** Already covered by tests that use `set_binding_rejected`.

---

## 3. Test Strategy

### New Tests to Add

| Test | Purpose | Covers Fix |
|------|---------|------------|
| `test_setter_clears_signature` | Verify setters invalidate signature | Fix 1 |
| `test_set_tbid_panics_if_already_set` | Verify debug_assert prevents misuse | Fix 5 |
| `test_tick_at_binary_search` | Verify O(log n) lookup works correctly | Fix 6 |
| `test_tick_at_missing_returns_none` | Verify missing tick returns None | Fix 6 |
| `test_tick_at_first_and_last` | Verify boundary conditions | Fix 6 |

### Existing Tests to Verify

| Test | Purpose | Covers Fix |
|------|---------|------------|
| `dht_record_signature.rs` | DHT record signing flow | Fix 2 |
| `tampered_record_rejected` | Signature tamper detection | Fix 1 |
| Liveness tests | Atomic ordering correctness | Fix 7 |

---

## 4. Scope

### In Scope

- Fix 1: Setters clear signature on canonical-payload mutation
- Fix 2: Internal signing flow uses setter methods
- Fix 3: Use `is_genesis()` method
- Fix 5: Add `debug_assert!` to Calendar setters
- Fix 6: Binary search in `tick_at()`
- Fix 7: Atomic ordering `SeqCst` → `Acquire`/`Release`
- Fix 8: Remove unnecessary clone
- Fix 9: Document dead code
- New tests for all fixes

### Out of Scope

- Fix 4: MirrorState accessor performance (design decision, not a bug)
- Performance benchmarking
- New features
