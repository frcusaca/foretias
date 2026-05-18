# SCOPE_REDUCTION_SPEC.md

**Foretias — Scope Reduction: C11 Core + Rust Library Only**

**Purpose:** Narrow the active development scope to the C11 cryptographic core and the Rust library layer. Remove all language bindings (Python, Java) from the active scope. Preserve the three-level thin client instantiation model as a forward-looking specification for when bindings are reintroduced.

**This document is a SPEC only.** No implementation plan, no code changes.

---

## PART 1 — SCOPE REDUCTION

### 1.1 What Remains (Active Scope)

| Layer | Crate / Path | Status |
|-------|-------------|--------|
| **C11 Core** | `p2p/core/` | ✅ Active — verified cryptographic primitives (Ed25519, P-256, SHA-256, BLAKE3, Noise, Merkle, FROST, nullifier, RNG, memzero) |
| **core-engine** | `p2p/core-engine/` | ✅ Active — safe Rust wrappers over C11 FFI, CryptoServer trait, domain types (Foretis, TickRecord, Calendar), Chronomatter, config, collision, epoch, probity |
| **foretias-node** | `p2p/foretias-node/` | ✅ Active — server (`TimeFamilyServer`), CLI binary (`foretias`), communerd (libp2p P2P layer), calendar store, metrics |

These three layers constitute the **complete Foretias stack**. The Rust CLI (`foretias`) provides stamp, verify, serve, and prove-verification. The `foretias-node` library exposes all protocol logic through its public API. Any language binding is a thin wrapper over this stack — it adds zero protocol logic.

### 1.2 What Is Removed (Out of Active Scope)

| Binding | Crate / Path | Current State | Action |
|---------|-------------|---------------|--------|
| **Python PyO3 bindings** | `p2p/foretias-python/` | cdylib, 1211 lines `lib.rs`, PyO3 classes for PyForetis, PyTickRecord, PyCalendar, PyTimeFamily, PyTimeFamilyServer, PyCryptoServer, etc. | **Removed from active scope.** Crate may remain in the repository as reference, but is not built, tested, or maintained. |
| **Python shim package** | `src/foretias/` (top-level) | hatchling package, re-exports from `foretias_p2p`. Files: `__init__.py`, `cli.py`, `thin_client.py`, `_version.py` | **Removed from active scope.** Shim package is not built, tested, or maintained. |
| **Python tests** | `tests/` (top-level), `p2p/foretias-python/tests/` | `test_shim.py`, `test_cli_smoke.py`, Python binding tests | **Removed from active scope.** Not run in CI or build verification. |
| **Java JNI bindings** | `p2p/foretias-java/` | JNI crate (`foretias-jni`), Maven POM, build script | **Removed from active scope.** Crate may remain in the repository as reference, but is not built, tested, or maintained. |
| **Top-level pyproject.toml** | Root `pyproject.toml` | Configures hatchling build for shim + `foretias-p2p` dependency | **Removed from active scope.** Python packaging is not part of the build pipeline. |

### 1.3 Rationale

1. **Surface area reduction.** Language bindings multiply the attack surface (FFI boundaries, type conversion, serialization edge cases) without adding protocol logic.
2. **Build complexity elimination.** `maturin develop`, Python test runners, Java Maven builds — all require toolchain setup that distracts from core development.
3. **Single source of truth.** All protocol logic lives in Rust. The Rust CLI (`foretias`) exercises every code path. Bindings are mechanical delegation — they can be reintroduced later without risk of drift.
4. **Focus on P2P and protocol hardening.** The active development priorities are DHT, gossipsub, probity, epoch consensus, and collision detection — all live in `foretias-node` and `core-engine`.

### 1.4 Build & Verification (Reduced Scope)

**Build pipeline:**
```bash
# Step 1: Build C11 core
cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build

# Step 2: Build Rust workspace (core-engine + foretias-node only)
cd p2p && cargo build -p foretias-core -p foretias-node

# Step 3: Run tests
cd p2p/core/build && ctest --output-on-failure
cd p2p && cargo test -p foretias-core -p foretias-node
```

**Cargo workspace (`p2p/Cargo.toml`):** The workspace members list is reduced to:
```toml
members = ["core-engine", "foretias-node"]
```

`foretias-python` and `foretias-java` are excluded from the workspace. They may exist as directories but are not compiled as part of `cargo build --workspace` or `cargo test --workspace`.

### 1.5 Reintroduction Contract

When language bindings are reintroduced, they MUST:
1. Be a **zero-logic thin wrapper** — no protocol work, no crypto, no network I/O, no serialization logic.
2. Implement the **three-level instantiation model** defined in Part 2.
3. Delegate every operation to the Rust library via FFI (PyO3 for Python, JNI for Java, etc.).
4. Pass the **three-tier test matrix** (functional, integration, end-to-end) defined in Part 2.

The specification in Part 2 is the reintroduction contract. A binding is complete when it satisfies all three instantiation levels and passes all associated tests.

---

## PART 2 — THREE-LEVEL THIN CLIENT INSTANTIATION MODEL

This section defines the thin client model that any language binding must implement. It is language-agnostic. The API examples use pseudocode for clarity; a concrete binding translates the constructor signatures and method calls to its host language idioms.

### 2.1 Overview

A thin client is a language-native wrapper that provides stamping, verification, and identity operations against a Foretias time family. It holds no protocol logic, performs no cryptography, and implements no server behavior — it delegates everything to the core Rust/C11 stack via bindings.

The thin client supports **three levels of instantiation**, each a strict superset of the previous:

| Level | Name | Capabilities | Network |
|-------|------|-------------|---------|
| 1 | **Standalone** | Stamp, verify, calendar (own in-memory calendar) | None |
| 2 | **PtP Networked** | Level 1 + stamp/verify/calendar-replication over direct PtP channels | Point-to-point (direct encrypted channels to known peers) |
| 3 | **P2P Full** | Level 2 + peer discovery, gossip, mutual attestation, reputation | Peer-to-peer mesh (DHT, gossipsub, peer pool) |

### 2.2 Thin Wrapper Invariants

These are absolute. No exceptions.

| # | Invariant |
|---|-----------|
| 1 | The thin client never opens network sockets |
| 2 | The thin client never performs cryptography (signing, verification, hashing, key derivation) |
| 3 | The thin client never implements protocol logic (Noise handshake, JSON-RPC encoding/decoding, wire formats) |
| 4 | The thin client never manages calendar state (append, integrity check, tick rotation, persistence) |
| 5 | The thin client never re-implements domain types (Foretis, TickRecord, Calendar) — it wraps or references them |
| 6 | The thin client never holds state not backed by the core stack |

**What the thin client MAY do:**
- Constructors that instantiate the core stack
- Delegation (call-through to core methods)
- Type conversion at the FFI/language boundary
- Human-readable representations (`__repr__`, `toString`)
- Error forwarding and translation
- Accessor methods for identity (public key, TBID, TBN)
- Typed result wrappers (return a `Foretis` object, not a dict or raw bytes)

### 2.3 Domain Types (read-only for thin client)

The thin client references these types from the core stack. It does not define them.

| Type | Description |
|------|-------------|
| **Foretis** | A cryptographically signed attestation: `(tick_number, content_hash, signature, signature_algorithm, tbid, echo, tbn, time_being_reference_time)` |
| **TickRecord** | A calendar entry linking consecutive ticks with forward/backward foretis, auto-attestation nonce, external attestations |
| **Calendar** | Append-only ordered collection of TickRecords, with `tbid` and `tbn` |
| **Tbid** | Time-being identity: 96-byte dual-key (Ed25519 public key \|\| PQC public key) |

### 2.4 Core Operations

Every level supports these. Higher levels add more.

#### 2.4.1 Stamp

Produce a Foretis attesting to the given content.

```
stamp(content: bytes, echo: str) -> Foretis
```

- The thin client passes `content` and `echo` to the core stamp operation.
- Returns a typed `Foretis` object (never a dict, never raw bytes).
- The caller can serialize the `Foretis` to JSON or wire format via its own `to_json()` method.

**Standalone:** Stamps against its own calendar (self-contained).
**PtP Networked:** Stamps against a remote server via PtP channel.
**P2P Full:** Stamps against a remote server; may route through P2P mesh if direct PtP unavailable.

#### 2.4.2 Verify

Verify that a Foretis attests to the given content.

```
verify(content: bytes, foretis: Foretis) -> bool
```

- Returns `true` if the Foretis is valid for the content, `false` otherwise.
- The thin client may raise on malformed Foretis (invalid structure), but not on cryptographic mismatch (that's a `false` result).

**Standalone:** Verifies against its own calendar.
**PtP Networked:** Verifies against the remote server's calendar.
**P2P Full:** Verifies against remote server; may fetch calendar slice from discovered peers.

#### 2.4.3 Prove Verification

Client-side proof: fetch a calendar slice from a remote time being and verify the Foretis locally.

```
prove_verification(content: bytes, foretis: Foretis) -> VerificationReport
```

- The thin client delegates to the core's prove-verification logic.
- Returns a structured report (not a boolean) with: verified status, calendar slice used, tick range, any anomalies.

#### 2.4.4 Calendar Slice

Fetch a range of tick records.

```
calendar_slice(start: u64, count: u64) -> [TickRecord]
```

**Standalone:** Returns from its own in-memory calendar.
**PtP Networked / P2P Full:** Fetches from the connected server (or discovered peers).

#### 2.4.5 Identity Accessors

```
public_key() -> str    # hex-encoded Ed25519 public key
tbid()     -> str      # hex-encoded TBID (96 bytes)
tbn()      -> str      # human-readable time-being name
status()   -> dict     # tick count, peer count, dormant flag, etc.
```

### 2.5 Level 1 — Standalone

**Use Case:** An embedded application needs self-contained timestamping with no network dependency. Think: a logging daemon that stamps its own entries, an IoT device that records sensor data with cryptographic timestamps, or a unit test that needs foretis attestations without spinning up a server.

**Constructor:**
```python
from foretias import ThinClient

# Fresh in-memory time family
client = ThinClient(tbn="my-device")

# With optional disk persistence
client = ThinClient(tbn="my-device", persist_path="/var/lib/foretias/my-device")
```

**Typical Session:**
```python
from foretias import ThinClient

# --- Instantiate ---
client = ThinClient(tbn="audit-logger")

# --- Identity (available immediately) ---
pub_key = client.public_key()   # "a1b2c3..." (hex)
tbid    = client.tbid()         # 192 hex chars (96 bytes)
tbn     = client.tbn()          # "audit-logger"

# --- Stamp content ---
foretis = client.stamp("user logged in at 2026-05-17T10:30:00Z", echo="auth-event-1")
# foretis is a typed Foretis object, NOT a dict

# Inspect the foretis
print(foretis.tick_number)              # 1
print(foretis.content_hash.hex())       # sha256 of the message
print(foretis.signature_algorithm)      # "Ed25519" (or whatever the CryptoServer uses)
print(foretis.time_being_reference_time) # "UE+..." (nanosecond wall clock)

# Serialize for storage/transit
json_str = foretis.to_json()

# --- Stamp again (tick advances) ---
foretis2 = client.stamp("file uploaded: report.pdf", echo="storage-event-1")
assert foretis2.tick_number == foretis.tick_number + 1

# --- Verify a stamp ---
assert client.verify("user logged in at 2026-05-17T10:30:00Z", foretis) is True
assert client.verify("tampered message", foretis) is False

# --- Deserialize and verify ---
restored = Foretis.from_json(json_str)
assert client.verify("user logged in at 2026-05-17T10:30:00Z", restored) is True

# --- Calendar ---
records = client.calendar_slice(0, 10)  # [TickRecord, TickRecord, ...]
assert len(records) == 2

# --- Status ---
status = client.status()
assert status["tick_count"] == 2
assert status["dormant"] is False
assert status["peer_count"] == 0  # no network
```

**Dormant Reload:**
```python
# Persist and reload later (verify-only)
client = ThinClient(tbn="audit-logger", persist_path="/var/lib/foretias/audit-logger")
client.stamp("some event", echo="evt-1")
client.stamp("another event", echo="evt-2")

# Later: reload in dormant mode (verify-only, no private key)
dormant = ThinClient.from_persist("/var/lib/foretias/audit-logger")
assert dormant.status()["dormant"] is True

# Can verify old stamps
assert dormant.verify("some event", old_foretis) is True

# Cannot stamp
try:
    dormant.stamp("new event")
except DormantError:
    pass  # expected
```

### 2.6 Level 2 — PtP Networked

**Use Case:** An application needs to stamp against a known, trusted server over an encrypted direct channel. Think: a client application connecting to a specific Foretias node for timestamping, a backup system that replicates calendar state from a primary server, or a service that needs to verify attestations against a remote authority.

**Constructor:**
```python
from foretias import ThinClient

# Single peer
client = ThinClient.connect(
    tbn="my-client",
    peer_addr="127.0.0.1:4001"
)

# Multiple peers (selectable by target)
client = ThinClient.connect(
    tbn="my-client",
    peer_addrs=["127.0.0.1:4001", "127.0.0.1:4002", "10.0.0.5:4001"]
)
```

**Typical Session:**
```python
from foretias import ThinClient

# --- Instantiate (connects to known server) ---
client = ThinClient.connect(
    tbn="data-pipeline",
    peer_addr="192.168.1.100:4001"
)

# --- Stamp against the remote server ---
foretis = client.stamp("batch-2026-05-17 processed", echo="pipeline-run-42")
# foretis was stamped by the remote server's tick, not our own

# The tick_number comes from the server's calendar
print(foretis.tick_number)        # e.g. 847 (server's current tick)
print(foretis.tbn)                # the server's TBN, not ours

# --- Verify against the remote server ---
assert client.verify("batch-2026-05-17 processed", foretis) is True

# --- Prove verification (client-side proof) ---
report = client.prove_verification("batch-2026-05-17 processed", foretis)
# report: VerificationReport
assert report.verified is True
assert report.tick_range.start <= foretis.tick_number
assert report.tick_range.end   >= foretis.tick_number
print(report.calendar_slice)    # the fetched tick records used for proof

# --- Calendar slice from remote ---
records = client.calendar_slice(840, 10)
# Returns 10 tick records starting from tick 840 on the server

# --- Multi-peer: select target ---
client = ThinClient.connect(
    tbn="multi-client",
    peer_addrs=["primary:4001", "secondary:4001", "tertiary:4001"]
)

# Stamp on a specific peer
foretis = client.stamp("important data", echo="evt-1", target="primary:4001")

# Verify against a different peer (cross-verification)
assert client.verify("important data", foretis, target="secondary:4001") is True

# --- Status ---
status = client.status()
assert status["peer_count"] == 1  # or 3 for multi-peer
assert status["dormant"] is False
```

**Calendar Replication:**
```python
# Pull server's calendar into local cache
client = ThinClient.connect(tbn="replicator", peer_addr="server:4001")

# Pull full calendar
all_records = client.calendar_slice(0, count=None)

# Pull incremental (last 100 ticks)
latest = client.status()["tick_count"]
new_records = client.calendar_slice(latest - 100, 100)
```

### 2.7 Level 3 — P2P Full

**Use Case:** An application needs to participate in the full Foretias mesh: discover peers, stamp through the network, verify through distributed lookup, and maintain mutual attestations. Think: a full Foretias node operator, a dApp that needs decentralized timestamping, or a service that contributes to network integrity through mutual attestation.

**Constructor:**
```python
from foretias import ThinClient

client = ThinClient.join(
    tbn="my-node",
    known_peers=["seed1.mainnet.foretias.net:4001", "seed2.mainnet.foretias.net:4001"],
    dht_namespace="mainnet",
    persist_path="/var/lib/foretias/my-node"
)
```

**Typical Session:**
```python
from foretias import ThinClient
import time

# --- Instantiate (joins P2P mesh) ---
client = ThinClient.join(
    tbn="my-node",
    known_peers=["seed1.example.net:4001", "seed2.example.net:4001"],
    dht_namespace="mainnet"
)

# Wait for mesh convergence (DHT registration, peer discovery)
client.wait_ready(timeout=30)

# --- Self-stamp (local tick) ---
foretis = client.stamp("node operational", echo="heartbeat")
# Stamped against our own calendar

# --- Stamp via P2P routing (target another node by TBID) ---
target_tbid = "a1b2c3..."  # some peer's TBID discovered via DHT
foretis = client.stamp("cross-stamped data", echo="cross-evt", target=target_tbid)
# Routed through the mesh to the target peer

# --- Verify (distributed lookup: local → peers → DHT) ---
assert client.verify("cross-stamped data", foretis) is True
# The client searches: local calendar → connected peers → DHT-discovered peers

# --- Prove verification (mesh-aware) ---
report = client.prove_verification("cross-stamped data", foretis)
# Fetches calendar slice from the best-reputation peer

# --- Discover peers ---
peers = client.peers()
# [{tbid: "...", reputation: 0.95, last_seen: "..."}, ...]

# --- Status ---
status = client.status()
assert status["peer_count"] > 0        # discovered peers
assert status["dht_connected"] is True
assert status["gossipsub_subscribed"] is True

# --- Mutual attestation (automatic, configured at join) ---
# The client periodically stamps peers' tick records and shares via gossip.
# This is handled by the core stack, not the application code.

# --- Graceful shutdown ---
client.close()
# Tears down PtP channels, unsubscribes from gossipsub, deregisters from DHT
```

**P2P Recovery:**
```python
# Crash recovery: persist path preserves identity and calendar
client = ThinClient.join(
    tbn="my-node",
    known_peers=["seed1.example.net:4001"],
    dht_namespace="mainnet",
    persist_path="/var/lib/foretias/my-node"
)
# Identity restored from disk, calendar restored, re-joins mesh
# Previous stamps are still verifiable
```

---

## 5. TESTING STRATEGY

Three test tiers, matching the three instantiation levels. Each tier is independently passable and independently meaningful.

### 5.1 Functional Tests (Level 1 — Standalone)

**Purpose:** Verify the thin client works in isolation. No network, no persistence (unless testing the persistence path specifically).

**Test Environment:** Pure in-memory. No sockets, no disk (except persistence-specific tests).

**Tests:**

| # | Test | Assertion |
|---|------|-----------|
| F1 | Stamp produces valid Foretis | `stamp("hello")` returns a `Foretis` with correct `content_hash`, `tick_number`, `echo`, `tbn` |
| F2 | Stamp-verify roundtrip | `verify("hello", stamp("hello"))` returns `true` |
| F3 | Wrong content fails verification | `verify("wrong", stamp("hello"))` returns `false` |
| F4 | Empty content stamps | `stamp("")` succeeds; roundtrip verifies |
| F5 | Multiple stamps produce distinct Foretis | Two `stamp("hello")` calls produce different `tick_number` values |
| F6 | Tick monotonicity | `tick_number` increases with each stamp |
| F7 | Calendar reflects stamps | `calendar_slice(0, 10)` returns all stamped records in order |
| F8 | Identity accessors return valid data | `public_key()`, `tbid()`, `tbn()` return non-empty, well-formed values |
| F9 | Persistence roundtrip | Stamp → persist → reload from persisted path → verify original stamps |
| F10 | Dormant mode is verify-only | `from_persist(path)` rejects `stamp()` with an error |
| F11 | Foretis serialization roundtrip | `Foretis.to_json()` → parse → reconstruct → `Foretis` fields match |
| F12 | Content hash correctness | `Foretis.content_hash` equals `SHA-256(content)` |

**Runner:** Unit test framework (no async, no fixtures needed). Fast — sub-second total.

### 5.2 Integration Tests (Level 2 — PtP Networked)

**Purpose:** Verify the thin client communicates correctly over a direct PtP channel with a real server.

**Test Environment:** In-process server + thin client. Real TCP socket between them, but same process. No external dependencies.

**Fixture (Python):**
```python
import pytest
from foretias import ThinClient, TimeFamilyServer

@pytest.fixture
def p2p_pair():
    """Spawn a real server + PtP thin client, yield, then tear down."""
    server = TimeFamilyServer(addr="127.0.0.1:0", chronon_ns=100_000_000)
    server.start()
    addr = server.address()  # ephemeral port
    client = ThinClient.connect(tbn="test-client", peer_addr=addr)
    yield server, client
    client.close()
    server.stop()
```

**Tests:**

| # | Test | Assertion |
|---|------|-----------|
| I1 | Remote stamp succeeds | `client.stamp("hello")` returns a Foretis from the server's tick |
| I2 | Remote stamp-verify roundtrip | `client.verify("hello", client.stamp("hello"))` returns `true` |
| I3 | Remote stamp, local verify | Stamp on server, verify on a fresh `ThinClient.connect` to the same server |
| I4 | Wrong content fails remote verification | `client.verify("wrong", client.stamp("hello"))` returns `false` |
| I5 | Calendar slice from remote | `client.calendar_slice(0, 5)` returns server's tick records |
| I6 | Calendar replication | Stamp on server → client pulls calendar slice → client's local calendar matches |
| I7 | Prove verification (remote) | `client.prove_verification("hello", foretis)` fetches calendar slice, verifies locally |
| I8 | Server restart recovery | Stop server → restart with persist → client reconnects → verifies old stamps |
| I9 | Unreachable server | Disconnect server → `client.stamp()` raises (timeout, not silent failure) |
| I10 | Multiple peers | Two servers → client connects to both → can target specific peer for stamp/verify |
| I11 | Status reflects remote state | `client.status()` returns correct tick count, peer info |
| I12 | PtP encryption (sanity) | Wire traffic is encrypted (not plaintext JSON) — verify via packet capture or protocol inspection |

**Runner:** Async test framework. Each test creates its own server/client pair. Total: ~10-30 seconds.

### 5.3 End-to-End Tests (Level 3 — P2P Full)

**Purpose:** Verify the thin client operates correctly in a mesh network with discovery, gossip, and peer reputation.

**Test Environment:** Multiple servers in P2P mode + thin client that joins the mesh. Real networking, DHT, gossipsub.

**Fixture (Python):**
```python
import pytest
import time
from foretias import ThinClient, TimeFamilyServer

@pytest.fixture
def p2p_mesh():
    """Spawn two P2P servers + a thin client, yield, then tear down."""
    server_a = TimeFamilyServer(addr="127.0.0.1:0", chronon_ns=100_000_000,
                                dht_namespace="testnet", p2p_listen=True)
    server_b = TimeFamilyServer(addr="127.0.0.1:0", chronon_ns=100_000_000,
                                dht_namespace="testnet", p2p_listen=True,
                                known_servers=[server_a.p2p_address()])
    server_a.start(); server_b.start()
    time.sleep(2)  # DHT settlement

    client = ThinClient.join(
        tbn="mesh-client",
        known_peers=[server_a.address()],
        dht_namespace="testnet"
    )
    client.wait_ready(timeout=15)
    yield server_a, server_b, client
    client.close()
    server_a.stop(); server_b.stop()
```

**Tests:**

| # | Test | Assertion |
|---|------|-----------|
| E1 | Client discovers peers | After `join()`, `client.status()` shows discovered peers from DHT |
| E2 | Stamp via P2P routing | `client.stamp("hello", target=server_a_tbid)` routes through mesh, succeeds |
| E3 | Verify via P2P lookup | `client.verify("hello", foretis)` finds the attestation via DHT peer lookup |
| E4 | Gossip propagation | Server A stamps → server B receives via gossipsub (within timeout) |
| E5 | Mutual attestation | Client's stamp is attested by a peer within the configured interval |
| E6 | Calendar replication via mesh | Client's local calendar converges with server calendars after gossip |
| E7 | Peer reputation affects routing | Lower-reputation peer is deprioritized for calendar fetch |
| E8 | Prove verification via mesh | `client.prove_verification()` fetches calendar slice from best peer, verifies |
| E9 | Network partition tolerance | Disconnect one peer → client falls back to remaining peer |
| E10 | Client leaves and rejoins | Disconnect client → reconnect → calendar replicates, previous stamps still verifiable |
| E11 | Cross-language stamp/verify | (If applicable) Stamp via Rust client, verify via Python thin client (or vice versa) |
| E12 | Full lifecycle | Stamp → persist → server restart → client reconnect → verify → prove verification |

**Runner:** Async test framework with real networking. Requires port management and settlement delays. Total: ~30-120 seconds.

### 5.4 Test Execution Matrix

| Tier | Level | Runner | Isolation | Duration | Dependencies |
|------|-------|--------|-----------|----------|-------------|
| Functional | Standalone | Unit tests | Pure in-memory | <1s | Core stack only |
| Integration | PtP Networked | Async tests | In-process server | ~30s | Core stack + TCP |
| E2E | P2P Full | Async tests | Multi-node mesh | ~120s | Core stack + TCP + DHT + Gossip |

**Golden rule:** Functional tests must pass before Integration tests are meaningful. Integration tests must pass before E2E tests are meaningful. But each tier is independently runnable — E2E failures don't block functional test results.

---

## 6. CONTENT ENCODING

The thin client API uses **raw bytes** for content (not hex strings, not base64).

```
stamp(content: bytes, echo: str)  # NOT hex-encoded
verify(content: bytes, foretis: Foretis)  # NOT hex-encoded
```

Any language binding that receives strings from user code must encode to bytes before delegation. The canonical encoding is UTF-8. Binary content is passed as-is.

The JSON serialization of `Foretis` uses hex encoding for binary fields (`content_hash`, `signature`, `tbid`) — but this is the `Foretis` type's responsibility, not the thin client's.

---

## 7. ERROR SEMANTICS

| Error | Level | Behavior |
|-------|-------|----------|
| Malformed Foretis (invalid structure) | All | Raise exception (not a `false` return) |
| Cryptographic mismatch (bad signature, wrong content) | All | Return `false` (not an exception) |
| Network unreachable | PtP, P2P | Raise exception with timeout context |
| Dormant mode stamp attempt | All | Raise exception (verify-only) |
| Invalid persist path | Standalone | Raise exception at construction/reload |
| Peer not found in mesh | P2P | Raise exception (DHT lookup failed) |

---

## 8. WHAT WE DISCARDED (lessons from prior implementation)

| What was | Why discarded |
|----------|---------------|
| `ForetiasClient` wrapping `PyTimeFamily` (lightweight, no key rotation) | Wrong type — loses per-tick key disablement, persistence, dormant mode |
| JSON-string roundtripping for Foretis (`to_json()` → `json.loads()` → dict) | Lossy, fragile. Return typed objects. |
| `dict` return from `stamp()` | Callers can't distinguish a Foretis dict from an arbitrary dict. Return typed `Foretis`. |
| Zero tests for the thin client path | Unacceptable. Three tiers, mandatory. |
| Not exported from the main module | `from foretias.thin_client import ForetiasClient` is undocumented. Export from top level. |
| Content encoding inconsistency (`utf-8` vs locale-dependent) | Canonical encoding is UTF-8 for strings, raw bytes for binary. |
| Signature algorithm hack in stamp (`"Ed25519"` forced) | The algorithm label must come from the CryptoServer, not be hardcoded. |
| Python implementing JSON-RPC parsing or Noise protocol | Violates invariant #3. Core stack handles protocol. |

---

## 9. BINDING IMPLEMENTATION NOTES

This spec is language-agnostic. The following notes guide implementation but are not part of the spec contract.

### PyO3 (Python) Binding
- The thin client wraps a Rust struct (`ThinClientInner`) that holds the appropriate level's state.
- `Standalone` level: wraps `TimeFamilyServer` (ephemeral or persistent).
- `PtP` level: wraps `TimeFamilyServer` + PtP client connections.
- `P2P` level: wraps `TimeFamilyServer` + Communerd (libp2p swarm).
- All PyO3 methods are blocking by default (run Tokio runtime internally) with async variants available (`astamp`, `averify`, etc.).

### JNI (Java) Binding
- Mirror the same three constructors. Java's type system benefits from a `ThinClient<T>` generic or three concrete classes.

### General
- The thin client is a **single class** with three construction paths. Do not create separate classes per level — the level is a construction-time concern, not a type-system concern.
- The `Foretis` return type must have a `to_json()` method for serialization. The deserialization counterpart is `Foretis.from_json(json_str)`.

---

# END OF SPEC
