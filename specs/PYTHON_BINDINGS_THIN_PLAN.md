# Python Bindings Thin Wrapper — Implementation Plan

Corresponding spec: `PYTHON_BINDINGS_THIN_SPEC.md`

## Pre-condition
- `foretias-python` depends on `foretias-node` which contains both the Noise server (`TimeFamilyServer::start()`) and Noise client (`json_rpc_call` in `main.rs`).
- The client-side `json_rpc_call` is currently in `main.rs` (binary crate), not in `lib.rs`. It must be refactored into the library crate for reuse by the Python bindings.
- Some Python wrapper types already have `.to_json()` methods, but not all types have the full `.jsonify()` / `.jsonify_pretty()` / `.from_json()` surface.
- Current PyO3 wrappers (e.g., `PyForetis`) store fields as raw `Vec<u8>` Python properties via `#[pyo3(get)]`. Per spec, byte fields must not be exposed as raw bytes; instead, accessor methods (e.g., `content_hash_hex()`) convert on demand. Additionally, `.jsonify()` must call Rust `to_json()` on the original Rust object, which requires the wrapper to hold the Rust object rather than converting fields eagerly.

---

## Phase 0: Commit spec + plan, then branch

- [ ] Write `PYTHON_BINDINGS_THIN_SPEC.md` and `PYTHON_BINDINGS_THIN_PLAN.md`
- [ ] Commit spec and plan to alpha with message indicating tests are broken and remaining work will be done on a branch
- [ ] Create worktree `git worktree add -b feat/python-bindings-thin ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Expose JSON serialization API on all Python types

- [ ] Audit all PyO3 wrapper types for `.jsonify()`, `.jsonify_pretty()`, and `.from_json()` methods:
  `PyForetis`, `PyTickRecord`, `PyCalendar`, `PyExternalAttestation`, `PyEpochSnapshot`, `PyPeerScore`, `PyHeartbeat`, `PyProbityReport`, `PySealedBlob`, `PyCalendarBlock`.
- [ ] For each type missing `.jsonify()`: add method that calls `foretias_core::foretias::encoding::to_json(&self.inner)` and returns `PyResult<String>`.
- [ ] For each type missing `.jsonify_pretty()`: add method that calls `foretias_core::foretias::encoding::to_json_pretty(&self.inner)`.
- [ ] For each type missing `.from_json()`: add `#[classmethod]` that calls `foretias_core::foretias::encoding::from_json(json_str)` and wraps result.
- [ ] Verify: Python `f.jsonify()` produces identical output to Rust `to_json(&foretis)` for same data.
- [ ] `cargo build -p foretias-python` passes.

## Phase 2: Refactor Noise client into library

- [ ] Move `json_rpc_call`, `generate_ed25519_keypair` usage, and `fetch_calendar_slice` from `foretias-node/src/main.rs` into `foretias-node/src/client/mod.rs` (new module).
- [ ] The new `foretias_node::client::noise_json_rpc(server, method, params)` function returns `Result<serde_json::Value, Box<dyn std::error::Error>>`.
- [ ] Update `main.rs` to call the new library function instead of inline code.
- [ ] `cargo test --workspace` passes.

## Phase 3: Add PyTimeFamilyServer.serve()

- [ ] Add `serve()` method to `PyTimeFamilyServer` in `foretias-python/src/lib.rs`.
  - Creates a threaded Tokio runtime.
  - Moves `self` into an `Arc<TimeFamilyServer>`.
  - Calls `server.start()` to spawn the Noise TCP listener.
  - Blocks via `runtime.block_on(futures::future::pending())` so Python `serve()` is a blocking call.
- [ ] `cargo build -p foretias-python` passes.

## Phase 4: Add PyClient

- [ ] Add `PyClient` pyclass to `foretias-python/src/lib.rs` with methods:
  - `stamp(server_addr: str, content: bytes, echo: str) -> PyForetis`
  - `verify(server_addr: str, content: bytes, foretis: PyForetis) -> bool`
  - `verify_json(server_addr: str, content: bytes, foretis_json: str) -> bool`
  - `prove_verification(server_addr: str, content: bytes, foretis: PyForetis) -> dict`
  - `get_calendar_slice(server_addr: str, tick_start: int, count: int) -> list[PyTickRecord]`
- [ ] Each method creates a single-shot Tokio runtime, generates ephemeral keypair, performs Noise handshake, sends JSON-RPC, returns translated Python types.
- [ ] Register `PyClient` in `foretias_p2p` pymodule.
- [ ] `cargo build -p foretias-python` passes.

## Phase 5: Repair all Python tests

- [ ] Delete `foretias-python/tests/python/py_server.py` (violates invariant: Python speaks network protocol).
- [ ] Rewrite `test_cross_language.py`:
  - Replace `python_server` fixture with `PyTimeFamilyServer` (calls `serve()` in a background thread).
  - Replace Python-side RPC calls with `PyClient` calls.
  - Restore the full 16-combo parametric matrix: all combos now speak Noise protocol.
- [ ] Update `test_bindings.py`:
  - Add tests for `PyClient` (stamp/verify roundtrip via Noise).
  - Add tests for `.jsonify()` / `.jsonify_pretty()` / `.from_json()` on all wrapper types.
  - Verify `.jsonify()` output matches Rust `to_json()` output.
- [ ] `maturin develop` to rebuild bindings.
- [ ] `python -m pytest tests/python/ -v` — ALL Python binding tests pass.
- [ ] `python -m pytest tests/ -v` — top-level shim tests pass.

## Phase 6: Verify and cleanup

- [ ] `cargo test --workspace` — all Rust tests pass.
- [ ] `cargo clippy --workspace` — no errors.
- [ ] `cd p2p/core/build && ctest` — C11 tests pass.
- [ ] Verify no socket imports remain in `foretias-python/tests/python/` (grep for `import socket`, `socket.`).
- [ ] Verify no `json.dumps()` or `json.loads()` on domain types in test code (use `.jsonify()` / `.from_json()` instead).
- [ ] Verify all work is complete in ${FULL_WORKTREE_PATH} and committed to feat/python-bindings-thin
- [ ] Merge feat/python-bindings-thin to alpha
