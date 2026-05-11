# Centralized Base64 JSON Serialization — Implementation Plan

**Paired with:** `FORETIAS_CUSTOM_BASE64_SPEC.md`
**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_${RANDOM}`
**Branch:** `feat/custom-base64-display`

**Design:** Two newtype wrappers (`FTByteVector`, `FTByteArray<N>`) with built-in serde. Applied at the field level via type change. No per-struct methods. No C11 code.

## Worktree Lifecycle

- [ ] Set `RANDOM_DIFF=${RANDOM}` once for this plan
- [ ] Create worktree `git worktree add -b feat/custom-base64-display /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_${RANDOM_DIFF}`
- [ ] `cd /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_${RANDOM_DIFF}`; reset current session work directory
- [ ] Revert any C11 changes from previous attempt (remove `foretias_custom_base64.h`, `encoding_custom_base64.c`, test files, CMake changes) if present
- [ ] Revert any `display.rs` from previous display-only attempt if present
... (implementation tasks below) ...
- [ ] Verify all work complete in worktree and committed to `feat/custom-base64-display`
- [ ] Merge `feat/custom-base64-display` to `alpha`
- [ ] Cleanup worktree:
  - [ ] Confirm all checkboxes above completed
  - [ ] `git worktree remove /home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_${RANDOM_DIFF}`
  - [ ] This is the last checkbox in this plan

---

## Phase 1: Wrapper Types & Encoding Module (core-engine)

**Goal:** Create `FTByteVector` and `FTByteArray<N>` with serde impls, and the centralized `encoding.rs` module.

**Files:**
- `p2p/core-engine/src/foretias/encoding.rs` — wrapper types + `to_json`/`from_json`/`to_json_pretty`
- `p2p/core-engine/src/foretias/mod.rs` — add `pub mod encoding;`

**Tasks:**
- [ ] Write `FTByteVector` with `Deref<Target=Vec<u8>>`, `DerefMut`, `From` conversions, `Serialize`, `Deserialize`
- [ ] Write `FTByteArray<const N: usize>` with `Deref<Target=[u8; N]>`, `Serialize`, `Deserialize`
- [ ] Write `to_json()`, `to_json_pretty()`, `from_json()` convenience functions
- [ ] Write unit tests for wrapper types (empty, single byte, all-zero, all-0xFF, max sig size, wrong-length rejection, invalid base64 rejection)
- [ ] Add `pub mod encoding;` to `foretias/mod.rs`
- [ ] Build: `cd p2p && cargo build -p foretias-core`
- [ ] Test: `cd p2p && cargo test -p foretias-core -- encoding`
- [ ] Commit

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
- [ ] Migrate `TickRecord` — change 5 field types, update `new()` constructor and all usages
- [ ] Migrate `Foretis` — change 2 field types, update `new()` constructor and `stamp()`/`verify()` functions
- [ ] Migrate `Tbid` — replace manual serde impl with `FTByteArray<96>` field. Update `raw_bytes()`, `from_raw()`, `from_bytes()`, `to_hex()` to work with wrapper
- [ ] Migrate `Heartbeat` — change 2 field types
- [ ] Migrate `EpochSnapshot` — change 2 field types
- [ ] Migrate `FrostMsg` — change 2 field types
- [ ] Migrate `SealedBlob` — change 2 field types
- [ ] Import `encoding::FTByteVector` and `encoding::FTByteArray` in each affected module
- [ ] Fix all compilation errors from type changes (Deref should handle most; some explicit conversions may be needed at boundaries where `Vec<u8>` is expected)
- [ ] Build: `cd p2p && cargo build -p foretias-core`
- [ ] Test: `cd p2p && cargo test -p foretias-core`
- [ ] Commit

---

## Phase 3: Comprehensive Serialization Test File

**Goal:** Write the dedicated `encoding_tests.rs` module per spec — covers every domain type + pathological cases.

**File:** `p2p/core-engine/src/foretias/encoding_tests.rs`

**Tasks:**
- [ ] Write wrapper type tests (15 tests — empty, small, large, max, all-zero, all-0xFF, Deref, wrong-length, invalid input)
- [ ] Write domain type serialization tests (30 tests — one format check + one roundtrip per struct)
- [ ] Write cross-language canonical output tests (4 tests — determinism, no int arrays, encoding::to_json matches direct serde)
- [ ] Write pathological case tests (6 tests — 100-tick calendar, empty calendar, deep nesting, max sig, etc.)
- [ ] Write negative tests (6 tests — invalid base64, wrong length, integer array input, truncated, etc.)
- [ ] Write helper functions (`assert_json_has_no_int_arrays`, `assert_field_is_base64_string`, `make_test_*`)
- [ ] Add `#[cfg(test)] mod encoding_tests;` to `foretias/mod.rs`
- [ ] Test: `cd p2p && cargo test -p foretias-core -- encoding_tests`
- [ ] Commit

---

## Phase 4: foretias-node Integration

**Goal:** Ensure foretias-node compiles and runs with new domain types. The node passes domain types through serde — no handler changes needed.

**Files:**
- `p2p/foretias-node/src/server/handlers.rs` — may need import adjustments
- `p2p/foretias-node/src/main.rs` — `cmd_stamp`/`cmd_verify`/`cmd_prove_verification` should use `encoding::to_json_pretty()` instead of `serde_json::to_string_pretty()`
- `p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs` — inner JSON auto-updates (no code change)
- `p2p/foretias-node/src/communerd/` — gossip/DHT auto-update (no code change)

**Tasks:**
- [ ] Update `main.rs` to import and use `foretias_core::foretias::encoding::to_json_pretty` for display output
- [ ] Build: `cd p2p && cargo build -p foretias-node`
- [ ] Fix any compilation errors from domain type boundary changes
- [ ] Test: `cd p2p && cargo test -p foretias-node`
- [ ] Commit

---

## Phase 5: PyO3 Binding Migration (foretias-python)

**Goal:** Replace `Vec<u8>` fields in all `#[pyclass]` structs with `FTByteVector`. Redirect all `to_json()`/`from_json()` methods to use `encoding::to_json()`/`from_json()`.

**Files:**
- `p2p/foretias-python/src/lib.rs` — 15 fields across 7 pyclass structs + 12 `to_json()` methods + 10 `from_json()` methods

**Tasks:**
- [ ] Change 15 `Vec<u8>` fields to `FTByteVector` across `PyForetis`, `PyTickRecord`, `PyCalendar`, `PyEpochSnapshot`, `PySealedBlob`, `PyHeartbeat`, `PyProbityReport`
- [ ] Update all 12 `to_json()` methods to call `foretias_core::foretias::encoding::to_json(self)` instead of `serde_json::to_string(self)`
- [ ] Update all 10 `from_json()` methods to call `foretias_core::foretias::encoding::from_json::<T>(s)` instead of `serde_json::from_str`
- [ ] Fix any PyO3-specific conversion issues (e.g., constructing `FTByteVector` from Python `bytes`)
- [ ] Build: `cd p2p && cargo build -p foretias-python`
- [ ] Commit

---

## Phase 6: Python Shim Integration

**Goal:** Ensure Python CLI produces and consumes base64 JSON output.

**Files:**
- `src/foretias/cli.py` — no code change needed if PyO3 bindings are correct (output flows from Rust)
- `src/foretias/thin_client.py` — verify `json.loads(foretis.to_json())` now produces base64 strings

**Tasks:**
- [ ] Verify `cli.py` stamp/verify output is base64 (no code change expected)
- [ ] Verify `thin_client.py` stamp/verify/calendar output is base64 (no code change expected)
- [ ] Build Python bindings: `cd p2p/foretias-python && maturin develop`
- [ ] Install shim: `pip install -e /home/hcbusy/webhash/foretias`
- [ ] Run Python tests: `python -m pytest tests/ -v`
- [ ] Fix any Python test failures from output format change
- [ ] Commit

---

## Phase 7: Verification

**Goal:** Full workspace validation.

**Tasks:**
- [ ] Rust tests: `cd p2p && cargo test --workspace` — ALL PASS
- [ ] Python bindings: `cd p2p/foretias-python && maturin develop`
- [ ] Python shim: `pip install -e /home/hcbusy/webhash/foretias`
- [ ] Python tests: `python -m pytest tests/ -v` — ALL PASS
- [ ] Manual smoke — stamp: `p2p/target/debug/foretias stamp -m "hello" --server 127.0.0.1:4001` — output contains base64 strings like `"signature": "dGVzdA..."`
- [ ] Manual smoke — verify: same server, verify output contains base64
- [ ] Manual smoke — calendar save: check `calendar.json` contains base64 strings, not int arrays
- [ ] Wire format check: intercept JSON-RPC response (e.g., with `nc` or `curl`) — body contains base64 strings
- [ ] C11 tests (unchanged): `cd p2p/core/build && ctest --output-on-failure`
- [ ] Commit verification pass

---

## Phase 8: Merge

**Tasks:**
- [ ] Verify all work committed to `feat/custom-base64-display`
- [ ] Merge `feat/custom-base64-display` to `alpha`
- [ ] Cleanup worktree (see top of this file)
