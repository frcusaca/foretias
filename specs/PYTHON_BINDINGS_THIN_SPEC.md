# Python Bindings Thin Wrapper Specification

## Purpose

The Python bindings (`foretias-p2p` / `foretias-python`) must be a **zero-logic thin wrapper** around the Rust library. Python code performs **no** protocol work, **no** crypto, **no** network I/O, **no** serialization, and **no** server logic. Every operation delegates directly to the Rust implementation via PyO3 FFI.

## Invariants

1. **Python never speaks a network protocol.** No TCP sockets, no Noise handshakes, no JSON-RPC encoding/decoding, no HTTP. The Rust `foretias-node` server handles all network.
2. **Python never performs cryptography.** No signing, no verification, no hashing, no key generation. All crypto routes through `foretias-core` (Rust → C11).
3. **Python never implements server logic.** No request dispatch, no calendar management, no tick daemon. The Rust `TimeFamilyServer` handles everything.
4. **Python never re-implements types that already exist in Rust.** A `PyForetis` is a PyO3 wrapper around `ForetisInner`, not a Python dataclass with independent logic.
5. **A Python server speaks Noise protocol.** There is no raw JSON-RPC Python server. The `PyTimeFamilyServer.serve()` starts the Rust Noise TCP server.
6. **A Python client speaks Noise protocol.** `PyClient.stamp()` / `verify()` / `prove_verification()` use the Rust Noise client implementation.

## Architecture

```
Python Application
       │
       ▼
  PyO3 Bindings (foretias-python/src/lib.rs)
  ──────────────────────────────────────────
  PyTimeFamilyServer → foretias_node::TimeFamilyServer
  PyClient           → foretias_node noise client logic
  PyTimeFamily       → core-engine::Chronomatter
  PyCryptoServer     → core-engine::SoftwareCryptoServer
  PyForetis          → foretias_core::ForetisInner
  PyTickRecord       → foretias_core::TickRecord
  PyCalendar         → foretias_core::Calendar
       │
       ▼
  foretias-node (server, Noise protocol, JSON-RPC)
       │
       ▼
  foretias-core (Rust safe wrappers, domain logic)
       │
       ▼
  C11 core (libsodium, OpenSSL, liboqs)
```

## JSON Serialization API Exposure

Every Python wrapper type that corresponds to a Rust domain type must expose the Rust library's standard JSON serialization. The `.jsonify()` method **calls into the Rust `to_json()` function on the original Rust object**, not on any Python-side copy or intermediate representation. This ensures the JSON output from Python is **bit-identical** to the JSON output from Rust — same field ordering, same encoding (custom base64), same canonical form — because the computation is performed entirely within Rust on the authoritative data.

Each Python type provides:

| Method | Returns | Description |
|--------|---------|-------------|
| `.jsonify()` | `str` | Standard JSON string. Calls Rust `to_json()` on the original Rust object to compute JSON directly. |
| `.jsonify_pretty()` | `str` | Pretty-printed JSON string. Calls Rust `to_json_pretty()` on the original Rust object. |
| `.from_json(json_str)` | `Self` | Classmethod: reconstruct from standard JSON using Rust `from_json()`. |

This applies to: `PyForetis`, `PyTickRecord`, `PyCalendar`, `PyExternalAttestation`, `PyEpochSnapshot`, `PyPeerScore`, `PyHeartbeat`, `PyProbityReport`, `PySealedBlob`, `PyCalendarBlock`.

Python-side `json.dumps()` / `json.loads()` must **never** be used for these types. The `.jsonify()` method calls the Rust `to_json()` function (in `foretias-core/src/foretias/encoding.rs`) on the underlying Rust object, which implements the custom base64 encoding rules (byte arrays → base64 strings, no raw byte arrays in output).

```python
f = client.stamp("127.0.0.1:4001", b"hello")
j = f.jsonify()           # Rust computes JSON from original Rust object
f2 = PyForetis.from_json(j)  # Reconstruct from JSON
```

## What Python Bindings ARE Allowed to Do

- **Constructors:** Call `RustType::new(...)` and return a PyO3 wrapper.
- **Delegation:** Forward method calls to the underlying Rust object with direct parameter mapping.
- **Conversion:** Translate between Python and Rust types at the FFI boundary (e.g., `&[u8]` ↔ `bytes`, `String` ↔ `str`, `Option<T>` ↔ `T | None`).
- **Representation:** Implement `__repr__` / `__str__` using Rust data.
- **Error forwarding:** Convert Rust `Result` errors into Python exceptions.
- **Accessor methods:** Provide on-demand accessor methods that compute derived representations of internal fields (e.g., `content_hash_hex()` returns a hex string, `signature_b64()` returns base64, `tbid_hex()` returns the 192-char hex TBID). Python does not operate on the raw innards of these objects; everything needed should have been provided by the Rust library through accessor methods.
- **Dataclass translation:** Small translation code to map Rust return values into Python dataclasses or native Python types (lists, dicts, ints) at the FFI boundary. This is thin glue, not business logic.

## What Python Bindings MUST NOT Do

- **Open sockets** or accept TCP connections.
- **Implement Noise protocol** or any encryption handshake.
- **Parse or serialize JSON-RPC** requests/responses.
- **Handle server lifecycle** (listen, accept, spawn threads, daemon loops).
- **Perform cryptographic operations** (sign, verify, hash, keygen).
- **Manage calendar state** (append ticks, integrity checks, persistence).
- **Re-implement logic** that exists in Rust (e.g., a Python `stamp()` function that does work instead of calling `self.server.stamp()`).
- **Hold independent state** not backed by a Rust object.

## Required API — PyTimeFamilyServer.serve()

The `PyTimeFamilyServer` must expose a `serve()` method that starts the Rust Noise TCP server. The Python caller gets a blocking call (runs a Tokio runtime internally) or a background task handle.

```python
from foretias_p2p import PyTimeFamilyServer

server = PyTimeFamilyServer(listen_addr="127.0.0.1:4001")
server.serve()  # blocks, runs Rust Noise TCP server
```

Implementation: `serve()` calls `Arc::new(self).start()` which spawns the Tokio TCP listener with Noise handshake handling. A threaded Tokio runtime is created inside the PyO3 binding to run the async `start()` loop.

## Required API — PyClient

A new `PyClient` class that performs stamp/verify/prove-verification over Noise protocol using the Rust client implementation. No Python code touches sockets.

```python
from foretias_p2p import PyClient

client = PyClient()
foretis = client.stamp("127.0.0.1:4001", b"hello world")
valid = client.verify("127.0.0.1:4001", b"hello world", foretis)
proof = client.prove_verification("127.0.0.1:4001", b"hello world", foretis)
```

Implementation: Each method creates a threaded Tokio runtime, generates an ephemeral Ed25519 keypair, performs the Noise handshake as initiator, sends the JSON-RPC request encrypted, and decrypts the response. The logic mirrors the existing `json_rpc_call` function in `foretias-node/src/main.rs`, refactored into a reusable crate-level function.

## py_server.py Removal

The `py_server.py` test helper implements a raw TCP JSON-RPC server in Python. This violates invariant #1 (Python speaks a network protocol). It must be deleted after `PyTimeFamilyServer.serve()` and `PyClient` are implemented.

## Test Implications

After implementation, the cross-language test matrix includes only Noise protocol:

| Server | Client | Protocol | Valid? |
|--------|--------|----------|--------|
| Rust `foretias serve` | Rust CLI | Noise | Yes |
| Python `PyTimeFamilyServer.serve()` | Rust CLI | Noise | Yes |
| Rust `foretias serve` | Python `PyClient` | Noise | Yes |
| Python `PyTimeFamilyServer.serve()` | Python `PyClient` | Noise | Yes |
| Python `py_server.py` | Any client | Raw JSON-RPC | **Deleted** |

The full 16-combo matrix becomes testable because all endpoints speak the same protocol.

## Verification

This spec is satisfied when:
1. `PyTimeFamilyServer` exposes a `serve()` method that starts the Rust Noise TCP server.
2. A `PyClient` class exists that performs stamp/verify/prove-verification over Noise protocol using the Rust client implementation.
3. `py_server.py` is deleted.
4. All cross-language tests pass using only Noise protocol.
5. A `grep` of `foretias-python/tests/python/` finds zero socket imports and zero manual JSON-RPC encoding.
