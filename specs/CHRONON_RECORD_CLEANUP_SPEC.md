# ChrononRecord Cleanup Fixes

## Overview

Three targeted cleanup changes to `ChrononRecord`:

| Fix | Type | Impact |
|-----|------|--------|
| Rename `stamps_per_tick` → `chronon_stamp_count` | Naming | Pure rename, no semantics change |
| Add `is_genesis()` method | API | Read-only helper, no data change |
| Restructure `genesis_signature` | Wire format | Structural change — embeds genesis sig into forward/backward foretis |

---

## Fix 1: Rename `stamps_per_tick` → `chronon_stamp_count`

**Rationale:** The field counts stamps within a chronon. "Tick" is an alias for chronon in this codebase, but `chronon_stamp_count` is more precise and consistent with the domain terminology.

**Scope:** Rename everywhere:
- `ChrononRecord::stamps_per_tick` → `ChrononRecord::chronon_stamp_count` (tick.rs)
- `auto_attestation_blob_with_count()` parameter rename (tick.rs)
- `Chronomatter::stamps_per_tick` → `Chronomatter::chronon_stamp_count` (chronomatter/mod.rs)
- All test fixtures, calendar construction sites (mirror.rs, calendar_store, etc.)
- `serde(rename = "chronon_stamp_count")` to preserve wire format compatibility OR break compat and rename the JSON key too

**Decision needed:** Preserve the legacy JSON key `stamps_per_tick` via `#[serde(rename = "stamps_per_tick")]` for backward compatibility, or break compat and rename to `chronon_stamp_count` everywhere?

---

## Fix 2: Add `ChrononRecord::is_genesis()`

**Signature:**

```rust
impl ChrononRecord {
    /// Returns true if this record is the genesis tick (chronon 1).
    pub fn is_genesis(&self) -> bool {
        self.chronon_number == 1
    }
}
```

**Rationale:** Centralizes the genesis check. Currently callers do `tick.chronon_number == 1` inline in multiple places (tick.rs:203, chronomatter/mod.rs:424, verify_genesis_signature). A dedicated method is more readable and future-proofs against convention changes.

---

## Fix 3: Restructure `genesis_signature` into forward/backward foretis

**Current state:** `genesis_signature` is a standalone `FTByteVector` field on `ChrononRecord`. It's only populated for tick 1. The forward and backward foretis blobs at tick 1 do NOT contain the genesis signature.

**Proposed state:** The genesis signature is embedded into BOTH `forward_foretis` and `backward_foretis` at tick 1. The standalone `genesis_signature` field is removed.

**Wire format change (tick 1 only):**

Current forward foretis blob layout:
```
tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk ‖ stamps_per_tick ‖ nonce
```

Proposed forward foretis blob layout (when at genesis):
```
tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk ‖ genesis_sig ‖ stamps_per_tick ‖ nonce
```

Similarly for backward foretis.

**Questions to resolve:**
1. Does the genesis signature go into BOTH forward and backward, or only one?
2. Is `genesis_sig` a variable-length prefix (needs length encoding) or fixed-length (96 bytes for dual-key TBID v1)?
3. How does `verify()` handle this — does it strip the genesis signature from the foretis blob before verification, or is the signature verified separately?
4. What happens to `verify_genesis_signature()` — does it become part of the standard foretis verification path?
5. Migration: existing tick 1 records on disk won't have genesis signatures in their foretis blobs. Do we need a migration path?

**Scope impact:**
- `tick.rs` — blob construction, verification, `ChrononRecord` struct
- `chronomatter/mod.rs` — genesis tick creation
- `verify_genesis_signature()` — may be absorbed into standard verify path
- Wire format — backward incompatible for genesis ticks

---

## Implementation Order

1. Fix 1 (rename) — no behavioral change
2. Fix 2 (is_genesis) — no behavioral change
3. Fix 3 (genesis_signature restructuring) — wire format change, done last
