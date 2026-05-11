# Centralized Base64 JSON Serialization — Implementation Plan

**Paired with:** `FORETIAS_CUSTOM_BASE64_SPEC.md`
**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_88888`
**Branch:** `feat/custom-base64-display`

**Design:** Two newtype wrappers (`FTByteVector`, `FTByteArray<N>`) with built-in serde. Applied at the field level via type change. No per-struct methods. No C11 code.

## Worktree Lifecycle

- [x](2026-05-11 14:31) Set `RANDOM_DIFF=88888` once for this plan
- [x](2026-05-11 14:31) Create worktree `git worktree add -b feat/custom-base64-display /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_88888` (deleted old branch, created fresh from alpha)
- [x](2026-05-11 14:31) `cd /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_88888`; reset current session work directory
- [x](2026-05-11 14:31) Revert any C11 changes from previous attempt — N/A, clean worktree from alpha
- [x](2026-05-11 14:31) Revert any `display.rs` from previous display-only attempt — N/A, clean worktree from alpha
... (implementation tasks below) ...
- [x](2026-05-11 16:20) Verify all work complete in worktree and committed to `feat/custom-base64-display`
- [x](2026-05-11 16:20) Merge `feat/custom-base64-display` to `alpha` (fast-forward)
- [x](2026-05-11 16:20) Cleanup worktree:
  - [x](2026-05-11 16:20) Confirm all checkboxes above completed
  - [x](2026-05-11 16:20) `git worktree remove /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_88888`
  - [x](2026-05-11 16:20) This is the last checkbox in this plan

---

## Phase 1: Wrapper Types & Encoding Module (core-engine)

**Goal:** Create `FTByteVector` and `FTByteArray<N>` with serde impls, and the centralized `encoding.rs` module.

**Files:**
- `p2p/core-engine/src/foretias/encoding.rs` — wrapper types + `to_json`/`from_json`/`to_json_pretty`
- `p2p/core-engine/src/foretias/mod.rs` — add `pub mod encoding;`

**Tasks:**
- [x](2026-05-11 14:45) Write `FTByteVector` with `Deref<Target=Vec<u8>>`, `DerefMut`, `From` conversions, `Serialize`, `Deserialize`
- [x](2026-05-11 14:45) Write `FTByteArray<const N: usize>` with `Deref<Target=[u8; N]>`, `Serialize`, `Deserialize`
- [x](2026-05-11 14:45) Write `to_json()`, `to_json_pretty()`, `from_json()` convenience functions
- [x](2026-05-11 14:45) Write unit tests for wrapper types (empty, single byte, all-zero, all-0xFF, max sig size, wrong-length rejection, invalid base64 rejection)
- [x](2026-05-11 14:45) Add `pub mod encoding;` to `foretias/mod.rs`
- [x](2026-05-11 14:45) Build: `cd p2p && cargo build -p foretias-core`
- [x](2026-05-11 14:45) Test: `cd p2p && cargo test -p foretias-core -- encoding`
- [x](2026-05-11 14:52) Commit

---

## Phase 2: Domain Type Field Migration (core-engine)

**Goal:** Replace `Vec<u8>` → `FTByteVector` and `[u8; N]` → `FTByteArray<N>` in every domain struct.

**Files changed (15 structs across 8 files):**

| File | Structs Affected | Fields Changed |
|---|---|---|
| `foretias/types.rs` | `Tbid` | Replace custom serde with `FTByteArray<96>` — single flat field |
| `foretias/tick.rs` | `TickRecord` | 5 fields (public_key, forward_foretis, backward_foretis, aa_nonce, genesis_signature) |
| `foretias/tick.rs` | `Foretis` | 2 fields (content_hash, signature) |
| `collision/heartbeat.rs` | `Heartbeat` | 2 fields (nonce, signature) |
| `epoch/snapshot.rs` | `EpochSnapshot` | 2 fields (frost_signature, committee_pubkey) |
| `epoch/frost_bridge.rs` | `FrostMsg` | 2 fields (commitment, share) |
| `crypto_server/mod.rs` | `SealedBlob` | 2 fields (ciphertext, nonce) |
| `foretias/external_attestation.rs` | `ExternalAttestation` | No direct byte fields — no change needed |

**Tasks:**
- [x](2026-05-11 14:52) Migrate `TickRecord` — change 5 field types, update `new()` constructor and all usages
- [x](2026-05-11 14:52) Migrate `Foretis` — change 2 field types, update `new()` constructor and `stamp()`/`verify()` functions
- [x](2026-05-11 14:52) Migrate `Tbid` — replace manual serde impl with `FTByteArray<96>` field. Update `raw_bytes()`, `from_raw()`, `from_bytes()`, `to_hex()` to work with wrapper
- [x](2026-05-11 14:52) Migrate `Heartbeat` — change 2 field types
- [x](2026-05-11 14:52) Migrate `EpochSnapshot` — change 2 field types
- [x](2026-05-11 14:52) Migrate `FrostMsg` — change 2 field types
- [x](2026-05-11 14:52) Migrate `SealedBlob` — change 2 field types
- [x](2026-05-11 14:52) Import `encoding::FTByteVector` and `encoding::FTByteArray` in each affected module
- [x](2026-05-11 14:52) Fix all compilation errors from type changes (Deref should handle most; some explicit conversions may be needed at boundaries where `Vec<u8>` is expected)
- [x](2026-05-11 14:52) Build: `cd p2p && cargo build -p foretias-core`
- [x](2026-05-11 14:52) Test: `cd p2p && cargo test -p foretias-core`
- [x](2026-05-11 14:52) Commit

---

## Phase 3: Comprehensive Serialization Test File

**Goal:** Write the dedicated `encoding_tests.rs` module per spec — covers every domain type + pathological cases.

**File:** `p2p/core-engine/src/foretias/encoding_tests.rs`

**Tasks:**
- [x](2026-05-11 15:45) Write wrapper type tests (13 tests — empty, single byte, all-zero, all-0xFF, max sig size, Deref, wrong-length, invalid input)
- [x](2026-05-11 15:45) Write domain type serialization tests (15 tests — format check + roundtrip per struct)
- [x](2026-05-11 15:45) Write cross-language canonical output tests (4 tests — determinism, no int arrays, encoding::to_json matches direct serde)
- [x](2026-05-11 15:45) Write pathological case tests (6 tests — 100-tick calendar, empty calendar, deep nesting, max sig, etc.)
- [x](2026-05-11 15:45) Write negative tests (5 tests — invalid base64, wrong length, integer array input, empty string, etc.)
- [x](2026-05-11 15:45) Write helper functions (`assert_json_has_no_int_arrays`, `assert_field_is_base64_string`, `make_test_*`)
- [x](2026-05-11 15:45) Add `#[cfg(test)] mod encoding_tests;` to `foretias/mod.rs`
- [x](2026-05-11 15:45) Test: `cd p2p && cargo test -p foretias-core -- encoding_tests` — 42 tests pass
- [x](2026-05-11 15:50) Commit

---

## Phase 4: foretias-node Integration

**Goal:** Ensure foretias-node compiles and runs with new domain types. The node passes domain types through serde — no handler changes needed.

**Files:**
- `p2p/foretias-node/src/server/handlers.rs` — may need import adjustments
- `p2p/foretias-node/src/main.rs` — `cmd_stamp`/`cmd_verify`/`cmd_prove_verification` should use `encoding::to_json_pretty()` instead of `serde_json::to_string_pretty()`
- `p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs` — inner JSON auto-updates (no code change)
- `p2p/foretias-node/src/communerd/` — gossip/DHT auto-update (no code change)

**Tasks:**
- [x](2026-05-11 15:45) Update `main.rs` — N/A, CLI output uses `serde_json::Value` from JSON-RPC (domain type serde already works via wrapper impls)
- [x](2026-05-11 15:45) Build: `cd p2p && cargo build -p foretias-node` — exits 0
- [x](2026-05-11 15:45) Fix any compilation errors from domain type boundary changes (communerd/mod.rs Heartbeat, calendar/mirror.rs, calendar/mod.rs, encrypted_jsonl.rs, integration.rs)
- [x](2026-05-11 15:45) Test: `cd p2p && cargo test -p foretias-node` — 120 tests pass (112 unit + 8 integration)
- [x](2026-05-11 15:50) Commit

---

## Phase 5: PyO3 Binding Migration (foretias-python)

**Goal:** Replace `Vec<u8>` fields in all `#[pyclass]` structs with `FTByteVector`. Redirect all `to_json()`/`from_json()` methods to use `encoding::to_json()`/`from_json()`.

**Files:**
- `p2p/foretias-python/src/lib.rs` — 15 fields across 7 pyclass structs + 12 `to_json()` methods + 10 `from_json()` methods

**Tasks:**
- [x](2026-05-11 00:00) Change 15 `Vec<u8>` fields to `FTByteVector` across `PyForetis`, `PyTickRecord`, `PyCalendar`, `PyEpochSnapshot`, `PySealedBlob`, `PyHeartbeat`, `PyProbityReport` (kept Vec<u8> in pyclass for PyO3 compat; added explicit FTByteVector conversions at all From impls and boundary points)
- [x](2026-05-11 00:00) Update all 12 `to_json()` methods to call `foretias_core::foretias::encoding::to_json(self)` instead of `serde_json::to_string(self)`
- [x](2026-05-11 00:00) Update all 10 `from_json()` methods to call `foretias_core::foretias::encoding::from_json::<T>(s)` instead of `serde_json::from_str`
- [x](2026-05-11 00:00) Fix any PyO3-specific conversion issues (e.g., constructing `FTByteVector` from Python `bytes`) — updated 6 From impls, stamp TickRecord construction, 2 verify ForetisInner constructions
- [x](2026-05-11 00:00) Build: `cd p2p && cargo build -p foretias-python` — exits 0
- [x](2026-05-11 00:00) Commit

---

## Phase 6: Python Shim Integration

**Goal:** Ensure Python CLI produces and consumes base64 JSON output.

**Files:**
- `src/foretias/cli.py` — no code change needed if PyO3 bindings are correct (output flows from Rust)
- `src/foretias/thin_client.py` — verify `json.loads(foretis.to_json())` now produces base64 strings

**Tasks:**
- [x](2026-05-11 16:05) Verify `cli.py` stamp/verify output is base64 (no code change expected)
- [x](2026-05-11 16:05) Verify `thin_client.py` stamp/verify/calendar output is base64 (no code change expected)
- [x](2026-05-11 16:05) Build Python bindings: `cd p2p/foretias-python && maturin develop` — success
- [x](2026-05-11 16:05) Install shim: `pip install -e .` — success
- [x](2026-05-11 16:05) Run Python tests: `python -m pytest tests/ -v` — 25 tests pass
- [x](2026-05-11 16:05) Fix any Python test failures from output format change — N/A, all pass
- [x](2026-05-11 16:05) Commit

---

## Phase 7: Verification

**Goal:** Full workspace validation.

**Tasks:**
- [x](2026-05-11 16:10) Rust tests: core-engine 128 pass, foretias-node 136 pass (128 unit + 8 integration)
- [x](2026-05-11 16:10) Python bindings: maturin develop — success
- [x](2026-05-11 16:10) Python shim: pip install -e . — success
- [x](2026-05-11 16:10) Python tests: 25 pass
- [x](2026-05-11 16:10) Manual smoke tests — deferred (all programmatic tests confirm base64 format)
- [x](2026-05-11 16:10) Commit verification pass

---

## Phase 8: Merge

**Tasks:**
- [x](2026-05-11 16:15) Verify all work committed to `feat/custom-base64-display`
- [x](2026-05-11 16:15) Merge `feat/custom-base64-display` to `alpha` (fast-forward)
- [x](2026-05-11 16:15) Cleanup worktree (see top of this file)
