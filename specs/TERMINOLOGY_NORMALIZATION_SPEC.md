# Spec: Terminology Normalization — Auto-Attestation, Mutual Attestation, Chrononchain

- [x] backburnered

**Scope:** Step (terminology consistency across code, specs, docs)
**Status:** Draft — ready for human review
**Pairs with:** `TERMINOLOGY_NORMALIZATION_PLAN.md`

---

## Problem Statement

The codebase and documentation contain inconsistent terminology for attestation concepts. The terms "auto-attestation", "mutual attestation", "attestation", and "chrononchain" overlap, are used interchangeably, or appear in the wrong context. This creates confusion about which mechanism is which, and makes it harder to reason about the protocol.

---

## Terminology Definitions (Canonical)

| Term | Code Name | Verb/Noun | Domain | Definition |
|------|-----------|-----------|--------|------------|
| **Auto-attestation** | `auto_attest` / `auto_attestation` | Verb: "auto-attest"; Noun: "auto-attestation" | **Chronomatter** (intra-node) | The mechanism by which a Time Being links consecutive chronons in its own Chrononchain. The old key signs the new key (forward_foretis), and the new key signs the old key (backward_foretis). Initiated by Chronomatter during each tick. Same TBID throughout. |
| **Mutual attestation** | `mutual_attest` / `mutual_attestation` | Verb: "mutually attest"; Noun: "mutual attestation" | **Calendar** (cross-node) | The mechanism by which two or more Calendars (different TBIDs) cross-stamp each other. Each Calendar stamps the other's TickRecord/ChrononChainRecord on the peer's `/stamp` endpoint, verifies the returned Foretis, and stores it as an `ExternalAttestation`. Initiated by Calendar, transported by Communerd. |
| **Chronon attestation** | `chronon_attest` / `chronon_attestation` | Verb: "chronon-attest"; Noun: "chronon attestation" | **Shared** | The generic mechanism of attesting between two chronons, regardless of whether they belong to the same or different Time Beings. Used in shared code paths where the distinction between auto and mutual doesn't apply (e.g., the blob construction in `tick.rs`). |
| **Attestation** | `attestation` | Verb: "attest"; Noun: "attestation" | **Stamp/Verify endpoints** | A simple signed Foretis produced by the `/stamp` endpoint. Used when a client (non-Time-Being) stamps content. No chain-linking semantics. |
| **Chrononchain** | `chrononchain` (lowercase in prose), `Chrononchain` (proper noun) | Noun only | **Documentation** | The append-only, tamper-evident chain of ChrononChainRecords, each linked to its predecessor by auto-attestation. |
| **Chronon chaining** | (prose only) | Noun phrase | **Documentation** | The act of linking chronons into a chain via auto-attestation. Replaces "tick chaining". |

---

## Rule Matrix

| Context | Who initiates | Code prefix | Prose term | Verb form |
|---------|--------------|-------------|------------|-----------|
| Self-linking (same TBID) | Chronomatter | `auto_attest` | "auto-attestation" | "auto-attest" |
| Cross-calendar (diff TBID) | Calendar | `mutual_attest` | "mutual attestation" | "mutually attest" |
| Shared blob construction | N/A | `chronon_attest` | "chronon attestation" | "chronon-attest" |
| Client stamp/verify | Client | (none) | "Fretias API calls" | "stamp", "verify", "attest"; Sometimes "Inquirer asks for verification of foretis and content." |

---

## Rename Map

### Rust Types, Structs, Traits

| Before | After | Reason |
|--------|-------|--------|
| `AutoAttestConfig` | `MutualAttestConfig` | Configures cross-calendar peer attestation, not self-linking |
| `AutoAttestObserver` | `MutualAttestObserver` | Observer trait for mutual attestation metrics |

### Rust Fields (config)

| Before | After | File(s) |
|--------|-------|---------|
| `auto_attest: AutoAttestConfig` | `mutual_attest: MutualAttestConfig` | `p2p.rs`, `time_family.rs` |
| `auto_attest_every_n: u64` | `every_n_chronons: u64` | `MutualAttestConfig` (field already named this in struct; the config-level alias `auto_attest_every_n` in `NodeConfig` is removed) |

### Rust Fields (state)

| Before | After | File(s) |
|--------|-------|---------|
| `auto_attest_observer: Option<Arc<dyn AutoAttestObserver>>` | `mutual_attest_observer: Option<Arc<dyn MutualAttestObserver>>` | `chronomatter/mod.rs` |

### Rust Methods

| Before | After | File(s) |
|--------|-------|---------|
| `set_auto_attest_observer()` | `set_mutual_attest_observer()` | `chronomatter/mod.rs` |
| `on_auto_attest_sent()` | `on_mutual_attest_sent()` | `callbacks.rs` |
| `on_auto_attest_ok()` | `on_mutual_attest_ok()` | `callbacks.rs` |
| `on_auto_attest_failed()` | `on_mutual_attest_failed()` | `callbacks.rs` |
| `peers()` | `mutual_attest_peers()` | `time_family.rs` |
| `auto_attest_every_n()` | `mutual_attest_every_n()` | `time_family.rs` |
| `request_timeout_secs()` | `mutual_attest_request_timeout_secs()` | `time_family.rs` |

### Rust Metrics (NodeMetrics)

| Before | After | File(s) |
|--------|-------|---------|
| `auto_attest_sent` | `mutual_attest_sent` | `metrics.rs` |
| `auto_attest_ok` | `mutual_attest_ok` | `metrics.rs` |
| `auto_attest_failed` | `mutual_attest_failed` | `metrics.rs` |

### Rust Variables

| Before | After | File(s) |
|--------|-------|---------|
| `ma_blob` | `attest_blob` | `tick.rs`, `calendar.rs`, `chronomatter/mod.rs` |

**Rationale for `attest_blob`:** The blob construction code in `tick.rs::auto_attestation_blob_with_count()` is shared between auto-attestation (Chronomatter) and mutual attestation (Calendar) — both call the same function. The variable name `ma_blob` is ambiguous. Since `stamp`/`verify` carry their own domain prefix and `auto_attest`/`mutual_attest` distinguish initiator, the blob construction uses the neutral term `attest_blob`.

### CLI Flags

| Before | After |
|--------|-------|
| `--auto-attest-every-chronons` | `--mutually-attest-every-chronons` |

**Rationale:** The adverb form "mutually-attest" parallels the existing "auto-attest" (both are adverb+verb). The noun form "mutual attestation" is used in prose, comments, and type names.

### CLI Struct Fields

| Before | After | File(s) |
|--------|-------|---------|
| `auto_attest_every_chronons: u64` | `mutually_attest_every_chronons: u64` | `main.rs` |

### Python Bindings

| Before | After | File(s) |
|--------|-------|---------|
| `auto_attest_every_n: u64` | `mutually_attest_every_n: u64` | `foretias-python/src/lib.rs` |

### Documentation Prose

| Before | After | Context |
|--------|-------|---------|
| "tick chaining" | "chronon chaining" | All prose |
| "self-transition" | "existing chrononchain" | Where referring to key rotation within the chain |
| "auto-attesting" (cross-node) | "mutually attesting" | README, HOWTO, specs |
| "auto-attestation" (cross-node) | "mutual attestation" | README, HOWTO, specs |
| "mutual auto-attestation" | "auto-attestation" | CHRONONCHAIN_NAMING_SPEC — the chain is linked by auto-attestation, not "mutual auto-attestation" |
| "mutual-attest" (adjective in flag names) | "mutually-attest" | CLI flag descriptions use adverb form |

### Unchanged (Correct Already)

| Term | Reason |
|------|--------|
| `auto_attestation_blob` / `auto_attestation_blob_with_count` | Function is in `tick.rs` — Chronomatter domain, self-linking only |
| `build_auto_attestation` | Method in Chronomatter — constructs self-linking attestations |
| `forward_foretis` / `backward_foretis` | These are stamp names; already correct |
| `stamp` / `verify` | Simple attestation verbs for stamp/verify endpoints |
| `ExternalAttestation` | Type name for cross-node attestations stored in calendar — the type itself is correct |
| `ChrononChainRecord` | Renamed from TickRecord per CHRONONCHAIN_NAMING_SPEC — already correct |
| `auto_attestation` (in foretias-v1.md § on self-linking) | Used correctly for Chronomatter self-linking |

---

## File Inventory

### Code Files (require changes)

| File | Changes |
|------|---------|
| `p2p/core-engine/src/config/p2p.rs` | `AutoAttestConfig` → `MutualAttestConfig`, `auto_attest` → `mutual_attest` |
| `p2p/core-engine/src/config/node.rs` | Remove `auto_attest_every_n` field (moved to communerd) |
| `p2p/core-engine/src/config/time_family.rs` | `auto_attest` → `mutual_attest`, accessor renames |
| `p2p/core-engine/src/foretias/callbacks.rs` | `AutoAttestObserver` → `MutualAttestObserver`, method renames |
| `p2p/core-engine/src/foretias/tick.rs` | `ma_blob` → `attest_blob` |
| `p2p/core-engine/src/foretias/calendar.rs` | `ma_blob` → `attest_blob` |
| `p2p/core-engine/src/chronomatter/mod.rs` | `auto_attest_observer` → `mutual_attest_observer`, method rename |
| `p2p/foretias-node/src/main.rs` | CLI flag rename, struct field rename |
| `p2p/foretias-node/src/metrics.rs` | Counter renames, JSON key renames |
| `p2p/foretias-node/src/server/mod.rs` | `AutoAttestObserver` → `MutualAttestObserver` |
| `p2p/foretias-node/src/communerd/mod.rs` | `auto_attest` → `mutual_attest` config access |
| `p2p/foretias-python/src/lib.rs` | `auto_attest_every_n` → `mutual_attest_every_n` |
| `p2p/foretias-node/tests/integration.rs` | `AutoAttestConfig` → `MutualAttestConfig`, field access renames |

### Spec/Doc Files (require changes)

| File | Changes |
|------|---------|
| `README.md` | "auto-attestation" → "mutual attestation" (peer flag), CLI flag rename |
| `HOWTO.md` | "auto-attesting" → "mutually attesting" (cross-node references) |
| `AGENTS.md` | "auto_attest_every_chronons" → "mutual_attest_every_chronons" |
| `specs/CLI_SPECIFIED.md` | Flag rename, description updates |
| `specs/FORETIAS_0_OVERVIEW.md` | "auto-attestation" context check |
| `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` | Prose normalization |
| `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` | Verify terminology note is correct |
| `specs/CHRONONCHAIN_NAMING_SPEC.md` | "mutual auto-attestation" → "auto-attestation" |
| `specs/CLI_SPECIFIED.md` | Flag rename |
| `specs/CONFIG_HIERARCHY_PLAN.md` | Config struct name references |
| `specs/CLEANUP_REPAIRS_SPEC.md` | Config struct references |
| `specs/CLEANUP_REPAIRS_PLAN.md` | Config struct references |
| `specs/FORETIAS_HOWTO_SPEC.md` | "auto-attestation" → "mutual attestation" where cross-node |
| `specs/CALENDAR_ACTIVE_MIRRORING_PLAN.md` | Verify terminology (already uses "mutual attestation") |
| `specs/foretias-v1.md` | "self-transition" → "existing chrononchain" (single occurrence) |

---

## Acceptance Criteria

- [ ] `AutoAttestConfig` does not exist anywhere in the codebase (replaced by `MutualAttestConfig`)
- [ ] `AutoAttestObserver` does not exist anywhere in the codebase (replaced by `MutualAttestObserver`)
- [ ] `auto_attest` (as a Rust identifier for cross-calendar attestation) does not exist — replaced by `mutual_attest`
- [ ] `auto_attestation_blob`, `build_auto_attestation` remain unchanged (Chronomatter self-linking)
- [ ] CLI flag `--auto-attest-every-chronons` replaced by `--mutual-attest-every-chronons`
- [ ] `ma_blob` replaced by `attest_blob` in `tick.rs`, `calendar.rs`, `chronomatter/mod.rs`
- [ ] "tick chaining" replaced by "chronon chaining" in all prose
- [ ] "self-transition" replaced by "existing chrononchain" where referring to key rotation within chain
- [ ] "auto-attestation" in README/HOWTO (for cross-node) replaced by "mutual attestation"
- [ ] All Rust tests pass (`cargo test --workspace`)
- [ ] All C11 tests pass (`ctest`)
- [ ] All Python tests pass (`pytest`)
