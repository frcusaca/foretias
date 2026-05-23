# Spec: Rename Ticks to Chronons Across Code and Documentation

**Scope:** Step
**Status:** Completed — merged to alpha (2026-05-18, commit b75d10a)
**Superseded by:** `COMBINED_GROUP2_SPEC.md` (2026-05-22)
**Note:** Per-file checkboxes below were stale at merge time; confirmed done by grep (no TickRecord in source as of 2026-05-22).

---

## Decisions (locked)

| Decision | Choice |
|----------|--------|
| `TickRecord` rename | **`ChrononRecord`** — one entry in the Chrononchain |
| `tick` as a noun | Replaced everywhere with **`chronon`** |
| `tick` as a verb | **Kept** — "tick to the next chronon" is correct usage |
| `tick_number` → `chronon_number` | All references updated; properties (unique, monotonic, sequential, etc.) now describe **chronon number** |
| `forward_foretis` / `backward_foretis` | **Unchanged** — a Foretis is a stamp; these names are correct |
| `Calendar` | **Unchanged** — the Calendar does more than store Chrononchains |
| `calendar_store` module | **Unchanged** |
| JSON-RPC wire format | Requires **migration** — see below |

---

## Definitions (canonical)

| Term | Meaning |
|------|---------|
| **Chronon** | One time interval in a Time Being's life. Has its own Ed25519 keypair. Previously called a "tick" (noun). |
| **ChrononRecord** | The Rust type for one entry in the Chrononchain. Previously `TickRecord`. Contains `chronon_number`, `public_key`, `forward_foretis`, `backward_foretis`, `nonce`. |
| **Chrononchain** | The append-only, tamper-evident chain of all ChrononRecords, each linked to its predecessor by auto-attestation. |
| **Calendar** | The storage and serving layer that contains the Chrononchain. Kept as-is — Calendar is a broader concept. |
| **to tick** | The verb describing the transition from one chronon to the next. *"The Time Being ticked to chronon 42."* Kept as-is. |
| **Foretis / forward_foretis / backward_foretis** | A Foretis is a stamp — a signed attestation of content at a chronon. `forward_foretis` and `backward_foretis` are the auto-attestation stamps that link consecutive chronons. These names correctly describe what they are and are not changed. |
| **Chronon number** | The unique, monotonic, sequential identifier of each chronon within a Chrononchain. During a chronon tick, the chronon number increases. |

---

## Rename Map

### Rust types and fields

| Before | After |
|--------|-------|
| `TickRecord` | `ChrononRecord` |
| `tick_number` (field) | `chronon_number` |
| `TickRecord::tick_number` | `ChrononRecord::chronon_number` |

### Rust method and variable names

| Before | After |
|--------|-------|
| `cal_tick_start` (parameter) | `cal_chronon_start` |
| `tick_number` (local variable) | `chronon_number` |
| `foretis.tick_number` | `foretis.chronon_number` |

### Rust module and struct names — kept

| Name | Kept because |
|------|-------------|
| `Calendar` | Broader than Chrononchain storage |
| `calendar_store` | Module name is accurate |
| `CalendarBlock` | A block within the calendar store |
| `forward_foretis` | A Foretis is a stamp; name is correct |
| `backward_foretis` | A Foretis is a stamp; name is correct |

### Documentation and comments

| Before | After |
|--------|-------|
| "tick record" | "ChrononRecord" or "chronon record" |
| "the chain of ticks" | "the Chrononchain" |
| "tick N" | "chronon N" |
| "current tick" | "current chronon" |
| "tick interval" | "chronon interval" |
| "advance the tick" | "tick to the next chronon" (verb kept) |
| "tick number" | "chronon number" |
| "tick number is unique/monotonic/sequential" | "chronon number is unique/monotonic/sequential" |
| "during a tick, the tick number increases" | "during a chronon tick, the chronon number increases" |

---

## Chronon Number Properties (canonical replacement for "tick number" properties)

Wherever the codebase or specs previously stated that **tick number** has properties such as unique, monotonic, sequential, incrementing, or similar, those statements now refer to **chronon number**:

| Old statement | New statement |
|---------------|---------------|
| "Each tick number is unique within a calendar." | "Each chronon number is unique within a calendar." |
| "Tick numbers are monotonically increasing." | "Chronon numbers are monotonically increasing." |
| "Tick numbers are sequential starting from 1." | "Chronon numbers are sequential starting from 1." |
| "During a tick, the tick number increases." | "During a chronon tick, the chronon number increases." |
| "The tick number identifies a specific point in the Chrononchain." | "The chronon number identifies a specific point in the Chrononchain." |
| "Compare tick numbers to determine ordering." | "Compare chronon numbers to determine ordering." |

These replacements apply to:
- Rust code (types, field names, variable names, doc comments)
- Spec documents (`foretias/specs/`)
- Design documents (`CAeHius/webhash/`)
- JSON-RPC wire format parameters (`cal_tick_start` → `cal_chronon_start`)

---

## JSON Wire Format Migration

`TickRecord` (now `ChrononRecord`) is serialized to JSON and transmitted over JSON-RPC
(`get_calendar_slice`) and stored in calendar files. Renaming the type and its
`tick_number` field affects the wire format.

**Migration approach:**

Use `#[serde(rename = "tick_number")]` on the new `chronon_number` field during
a transition period so that existing serialized calendars and in-flight JSON-RPC
responses remain readable. Remove the compat alias in a later cleanup step once
all nodes on the network have updated.

```rust
#[derive(Serialize, Deserialize)]
pub struct ChrononRecord {
    #[serde(rename = "tick_number")]   // compat — remove after migration
    pub chronon_number: u64,
    pub public_key: Vec<u8>,
    pub signature_algorithm: String,
    pub forward_foretis: Vec<u8>,
    pub backward_foretis: Vec<u8>,
    pub nonce: Vec<u8>,
}
```

The JSON-RPC method name `get_calendar_slice` and its parameter
`cal_tick_start` are also updated to `cal_chronon_start` in the spec and
server handler, with a compat alias accepting the old parameter name during
transition.

---

## Files to Update

### Core engine (`p2p/core-engine/src/`)

- [ ] `foretias/types.rs` — rename `TickRecord` → `ChrononRecord`;
      rename field `tick_number` → `chronon_number` with serde compat alias
- [ ] `foretias/tick.rs` — update all references; update function parameter
      names; update doc comments (tick number → chronon number throughout)
- [ ] `foretias/calendar.rs` — update type references and doc comments
- [ ] `foretias/mod.rs` — update re-exports
- [ ] `integration_tests.rs` — update test code

### Server (`p2p/foretias-server/src/`)

- [ ] `communerd/libp2p_transport.rs` — update `ChrononRecord` references
- [ ] `communerd/peer_pool.rs` — update `ChrononRecord` references
- [ ] `calendar_store/encrypted_jsonl.rs` — update `ChrononRecord` references;
      update `CalendarBlock.ticks` field type
- [ ] `calendar_store/lru.rs` — update references
- [ ] `server/handlers.rs` — update `cal_tick_start` parameter name to `cal_chronon_start`; add compat
      alias accepting old name; update doc comments (tick number → chronon number)
- [ ] `server/jsonrpc.rs` — update parameter name in JSON-RPC dispatch

### Client (`p2p/foretias-client/src/`)

- [ ] `foretias.rs` — update `TickRecord` → `ChrononRecord` references
- [ ] `calendar/mod.rs` — update references
- [ ] `calendar/mirror.rs` — update references

### Specs and documentation

- [ ] `specs/foretias-v1.md` — rename tick → chronon throughout; add
      ChrononRecord and Chrononchain to terminology table; keep
      forward_foretis / backward_foretis definitions as-is; replace all "tick number" properties with "chronon number"
- [ ] `specs/FORETIAS_0_OVERVIEW.md` — update references
- [ ] `specs/FORETIAS_1_MVP_SPEC.md` — update references
- [ ] `specs/FORETIAS_2_P2P_SPEC.md` — update references

---

## Acceptance Criteria

- [ ] `TickRecord` does not exist anywhere in the codebase.
- [ ] `ChrononRecord` is the canonical type name for one Chrononchain entry.
- [ ] `chronon_number` is the field name; serde compat alias for `tick_number`
      is present during transition.
- [ ] "tick number" (as a noun phrase describing the identifier) does not appear in any doc comment, spec, or user-facing string. Replaced by "chronon number" throughout.
- [ ] All statements about uniqueness, monotonicity, and sequentiality of tick numbers are updated to refer to chronon numbers.
- [ ] "tick" as a noun does not appear in any doc comment, spec, or user-facing string (only as a verb: "tick to the next chronon").
- [ ] `forward_foretis`, `backward_foretis`, `Calendar`, `calendar_store` are
      unchanged.
- [ ] All existing tests pass without modification (serde compat alias handles
      any serialized test fixtures that use `tick_number`).
