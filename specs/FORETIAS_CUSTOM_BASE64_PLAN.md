# Custom Base64 Display Encoding — Implementation Plan

**Paired with:** `FORETIAS_CUSTOM_BASE64_SPEC.md`
**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/FORETIAS_CUSTOM_BASE64_SPEC_${RANDOM}`
**Branch:** `feat/custom-base64-display`

## Worktree Lifecycle

- [ ] Create worktree `git worktree add -b feat/custom-base64-display ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory
... (implementation tasks below) ...
- [ ] Verify all work complete in ${FULL_WORKTREE_PATH} and committed to `feat/custom-base64-display`
- [ ] Merge `feat/custom-base64-display` to `alpha`
- [ ] Cleanup ${FULL_WORKTREE_PATH}:
  - [ ] Confirm all checkboxes above completed
  - [ ] `git worktree remove ${FULL_WORKTREE_PATH}`
  - [ ] This is the last checkbox in this plan

## Phase 1: C11 Encoding Functions

**Goal:** Encode/decode functions in C11 core with defensive boundary checks and full test coverage.

**Files:**
- `p2p/core/include/foretias_custom_base64.h` — public declarations
- `p2p/core/src/encoding_custom_base64.c` — implementation (BASE64_CUSTOM table, inverse lookup table, encode/decode)
- `p2p/core/include/foretias_core.h` — add `#include "foretias_custom_base64.h"`
- `p2p/core/tests/test_custom_base64.c` — test suite
- `p2p/core/tests/test_all.c` — register new test
- `p2p/core/tests/CMakeLists.txt` — add test source
- `p2p/core/CMakeLists.txt` — add implementation source

**Tasks:**
- [ ] Write `foretias_custom_base64.h` with declarations
- [ ] Write `encoding_custom_base64.c`:
  - BASE64_CUSTOM[64] lookup table
  - INVERSE_BASE64[128] decoded by Python script (auto-generated, NOT hand-written)
  - `foretias_custom_base64_encoded_len()` — formula: `(n/3)*4 + remainder`
  - `foretias_custom_base64_decoded_len()` — formula: `(n/4)*3 + remainder`
  - `foretias_custom_base64_encode()` — 3→4 with remainder 1→2, 2→3, no padding chars
  - `foretias_custom_base64_decode()` — 4→3 with remainder 2→1, 3→2, validates charset
- [ ] Write `test_custom_base64.c`:
  - Roundtrip for 0 bytes, 1 byte, 2 bytes, 3 bytes, up to 20 bytes
  - Roundtrip for 256 bytes (all values 0x00–0xFF)
  - Roundtrip for 1024 bytes
  - Roundtrip for all-zeros and all-0xFF
  - NULL input/output rejection
  - Invalid character rejection
  - Invalid length (`len % 4 == 1`) rejection
  - `encoded_len` helper correctness
  - Charset coverage (every encoded char is in `[a-zA-Z0-9|_]`)
- [ ] Update CMakeLists.txt (both root and tests)
- [ ] Update `foretias_core.h` to include new header
- [ ] Build: `cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build`
- [ ] Test: `cd p2p/core/build && ctest --output-on-failure` — ALL PASS
- [ ] Commit C11 changes

## Phase 2: Rust FFI Wrapper

**Goal:** Safe Rust wrappers over the C11 encoding functions.

**Files:**
- `p2p/core-engine/src/core/encoding.rs` — safe wrappers + unit tests
- `p2p/core-engine/src/core/mod.rs` — add `pub mod encoding;`

**Tasks:**
- [ ] Confirm bindgen picks up new declarations from `foretias_core.h`
- [ ] Write `encoding.rs`:
  - `custom_base64_encode(bytes: &[u8]) -> String` — allocs output buffer, calls FFI, returns owned String
  - `custom_base64_decode(s: &str) -> Result<Vec<u8>, CryptoError>` — allocs output buffer, calls FFI
  - Unit tests: roundtrip for empty, single byte, multi-byte, known vectors
- [ ] Add module to `core/mod.rs`
- [ ] Build: `cd p2p && cargo build -p foretias-core`
- [ ] Test: `cd p2p && cargo test -p foretias-core -- encoding`
- [ ] Commit Rust wrapper changes

## Phase 3: Display Formatter

**Goal:** Post-process a `serde_json::Value` tree, converting integer arrays to base64 strings.

**Files:**
- `p2p/core-engine/src/foretias/display.rs` — formatter + unit tests
- `p2p/core-engine/src/foretias/mod.rs` — add `pub mod display;`

**Algorithm:**
1. Walk the `serde_json::Value` recursively
2. For `Value::Array`, check if ALL elements are `Value::Number` in range `[0, 255]`
3. If yes, replace the array with `Value::String(custom_base64_encode(collected_bytes))`
4. If no, recurse into each element
5. For other value types, recurse into children (objects, nested arrays)
6. Return `serde_json::to_string_pretty(&transformed)`

**Edge cases:**
- Empty arrays `[]` — pass through (not binary)
- Arrays with mixed types `[1, "hello"]` — recurse, don't convert
- Arrays with numbers > 255 `[300]` — recurse, don't convert
- Arrays with negative numbers — recurse, don't convert
- Nested arrays `[ [1,2], [3,4] ]` — each inner array converted independently

**Tasks:**
- [ ] Write `display.rs` with `format_json_for_display()`
- [ ] Write unit tests covering: simple binary arrays, nested structures, empty arrays, mixed arrays, numbers > 255
- [ ] Add module to `foretias/mod.rs`
- [ ] Build: `cd p2p && cargo build -p foretias-core`
- [ ] Test: `cd p2p && cargo test -p foretias-core -- display`
- [ ] Commit display formatter changes

## Phase 4: CLI Integration

**Goal:** Replace pretty-print calls with display formatter in all user-facing output paths.

### 4a. Rust CLI
- [ ] `p2p/foretias-node/src/main.rs` — in `cmd_stamp`, `cmd_verify`, `cmd_prove_verification`: replace `serde_json::to_string_pretty(&result)` with `foretias_core::foretias::display::format_json_for_display(&result)`

### 4b. PyO3 Bindings
- [ ] `p2p/foretias-python/src/lib.rs` — add `to_json_display() -> String` method to `PyForetis`, `PyCalendar`, `PyTickRecord`. Each calls `format_json_for_display(serde_json::to_value(self)?)`

### 4c. Python CLI
- [ ] `src/foretias/cli.py` — add `format_binary_fields(obj)` helper. Two approaches (pick one):
  - Option A: Use PyO3 `to_json_display()` and parse result
  - Option B: Implement the transformation purely in Python (import custom base64 encode from a small Python module)
- [ ] Apply formatter before every `json.dumps(j, indent=2)` call in stamp/verify/integrity commands

### Build & Test
- [ ] `cd p2p && cargo build -p foretias-node && cargo build -p foretias-python`
- [ ] `cd p2p && cargo test --workspace`
- [ ] Commit CLI integration changes

## Phase 5: Verification

- [ ] C11 tests: `cd p2p/core/build && ctest --output-on-failure` — ALL PASS
- [ ] Rust tests: `cd p2p && cargo test --workspace` — ALL PASS
- [ ] Python bindings: `cd p2p/foretias-python && maturin develop`
- [ ] Python shim: `pip install -e /home/hcbusy/webhash/foretias`
- [ ] Python tests: `python -m pytest tests/ -v` — ALL PASS
- [ ] Manual smoke: `p2p/target/debug/foretias stamp -m "hello" --server 127.0.0.1:4001` — `tbid`/`signature`/`content_hash` display as base64 strings (e.g., `"tbid": "xITh..."`)
- [ ] Wire format verification: capture JSON-RPC response — internal fields still use integer arrays

## Phase 6: Merge

- [ ] Verify all work committed to `feat/custom-base64-display`
- [ ] Merge to `alpha`
- [ ] Cleanup worktree (see top of this file)
