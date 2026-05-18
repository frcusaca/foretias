# RUST_THREE_LEVEL_INSTANTIATION_SPEC.md

**Foretias — Three-Level Rust Library Instantiation**

**Purpose:** Refactor `foretias-node` to expose a unified `ThinClient` API that supports three instantiation modes. This enables Rust programs (and future language bindings) to use the library directly without requiring a server daemon.

**This document is a SPEC only.** No implementation plan, no code changes.

---

## 1. OVERVIEW

### 1.1 Goal

Refactor `foretias-node` to expose a public library API that supports three instantiation modes. Currently, the three modes exist as:
- **Level 1 (Standalone):** Implicitly available through `core-engine` types (`Chronomatter`, `Calendar`) but no unified entry point
- **Level 2 (PtP Networked):** Only available through the Rust CLI (`main.rs`) — binary-scoped Noise client logic
- **Level 3 (P2P Full):** Only available through `TimeFamilyServer` with `Communerd` — server daemon with P2P mesh

The goal is to make all three modes available as a reusable library API: `ThinClient::new()`, `ThinClient::connect()`, `ThinClient::join()`.

### 1.2 Target Users

1. **Rust programs** that want to embed Foretias functionality without spawning a server process.
2. **Language bindings** (future) that need a single FFI target with three construction paths.
3. **Testing** — the three-tier test matrix (functional/integration/e2e) can be driven entirely from Rust.

### 1.3 Three Levels

| Level | Constructor | Capabilities | Noise Transport |
|-------|------------|--------------|-----------------|
| 1 | `ThinClient::new()` | Stamp, verify, calendar (own in-memory calendar) | None — no network |
| 2 | `ThinClient::connect()` | Level 1 + PtP connections to known peers | C11 `noise_xx.c` — custom dual-curve Noise_XX over raw TCP |
| 3 | `ThinClient::join()` | Level 2 + peer discovery, gossip, mutual attestation | `libp2p-noise` — standard libp2P transport encryption |

### 1.4 Two Noise Transports

There are **two independent Noise implementations** in the Foretias stack:

| Transport | Implementation | Used By | Level |
|-----------|---------------|---------|-------|
| **C11 Noise_XX** | `p2p/core/src/noise_xx.c` — custom dual-curve (Ed25519 + P-256) | Direct TCP PtP connections | Level 2 |
| **libp2p-noise** | `libp2p-noise` crate | libp2P swarm (DHT, gossipsub, request-response) | Level 3 |

**C11 Noise_XX** is our custom implementation. A client generates an ephemeral Ed25519 keypair, initiates a Noise_XX handshake over raw TCP, then sends JSON-RPC messages encrypted through the resulting session. This is what `json_rpc_call` in `main.rs` does today.

**libp2p-noise** is the standard libp2P transport encryption. It handles multiplexing (yamux/mplex), protocol negotiation (identify, ping, request-response, gossipsub, Kademlia). A Level 3 client uses this exclusively for mesh communication.

**Level 1 uses neither Noise transport.** It operates purely in-memory.

---

## 2. CURRENT STATE

### 2.1 What Exists Today

| Component | Location | Current Scope | Problem |
|-----------|----------|---------------|---------|
| `TimeFamilyServer` | `foretias-node/src/server/` | Server daemon — listens on port, handles JSON-RPC | Too heavy for Level 1 (opens port, runs daemon loop). Has stamp/verify logic but behind a server facade. |
| `Chronomatter` | `core-engine/src/chronomatter/` | Core stamp/tick/verify logic | Has the stamping logic but no network layer. No unified entry point — callers must wire Chronomatter + Calendar + CryptoServer manually. |
| `json_rpc_call` | `foretias-node/src/main.rs` | CLI Noise client — ephemeral keypair, Noise handshake, JSON-RPC over encrypted TCP | Binary-scoped. Not in `lib.rs`. Cannot be reused by library consumers. |
| `Communerd` | `foretias-node/src/communerd/` | P2P mesh orchestrator — DHT, gossipsub, peer pool, mutual attestation | Coupled to `TimeFamilyServer`. Assumes a server daemon is running. No client-only mode. |
| `CalendarStore` | `foretias-node/src/calendar_store/` | Encrypted JSONL persistence, LRU, calendar blocks | Scattered initialization — `TimeFamilyServer` sets it up. No standalone API. |

### 2.2 Current `main.rs` Client Logic

The stamp/verify/prove-verification CLI subcommands in `main.rs` follow this pattern:
1. Generate ephemeral Ed25519 keypair
2. Connect to server via TCP
3. Perform Noise_XX handshake (using C11 `noise_xx.c`)
4. Send JSON-RPC request (encrypted)
5. Decrypt response, return result

This logic is approximately 200 lines of code in `main.rs` — Noise handshake setup, TCP connection, JSON-RPC encoding/decoding, error handling. It works but is not library-scoped.

---

## 3. REQUIRED STATE

### 3.1 Single Type, Three Constructors

The Rust library exposes a single `ThinClient` type:

```rust
pub enum ClientLevel {
    Standalone,
    Ptp,
    P2p,
}

pub struct ThinClient {
    level: ClientLevel,
    inner: ThinClientInner,
}
```

**Level 1 — Standalone:**
```rust
impl ThinClient {
    pub fn new(tbn: String, persist_path: Option<PathBuf>) -> Result<Self, ThinClientError>;
    pub fn from_persist(path: PathBuf) -> Result<Self, ThinClientError>;  // dormant reload
}
```

**Level 2 — PtP Networked:**
```rust
impl ThinClient {
    pub fn connect(tbn: String, peer_addrs: Vec<PeerAddr>) -> Result<Self, ThinClientError>;
}
```

**Level 3 — P2P Full:**
```rust
impl ThinClient {
    pub async fn join(config: P2pJoinConfig) -> Result<Self, ThinClientError>;
}

pub struct P2pJoinConfig {
    pub tbn: String,
    pub known_peers: Vec<String>,
    pub dht_namespace: String,
    pub persist_path: Option<PathBuf>,
}
```

### 3.2 Common Operations (All Levels)

All three levels expose the same operation surface:

```rust
impl ThinClient {
    pub async fn stamp(&self, content: &[u8], echo: String) -> Result<Foretis, ThinClientError>;
    pub async fn verify(&self, content: &[u8], foretis: &Foretis) -> Result<bool, ThinClientError>;
    pub async fn prove_verification(&self, content: &[u8], foretis: &Foretis) -> Result<VerificationReport, ThinClientError>;
    pub async fn calendar_slice(&self, start: u64, count: u64) -> Result<Vec<TickRecord>, ThinClientError>;

    // Identity accessors
    pub fn public_key(&self) -> String;
    pub fn tbid(&self) -> String;
    pub fn tbn(&self) -> String;
    pub fn status(&self) -> ThinClientStatus;

    // Shutdown (Level 3 only)
    pub async fn close(self);
}
```

### 3.3 Level-Specific Behavior

**Level 1 (Standalone):**
- Wraps a non-listening time family (Chronomatter + Calendar + CryptoServer)
- `stamp()` increments local tick, signs, appends to local calendar
- `verify()` checks against local calendar
- `status().peer_count` is always 0
- `from_persist()` loads dormant calendar (verify-only)

**Level 2 (PtP Networked):**
- Wraps Level 1 + C11 Noise_XX client connections
- `stamp()` sends content to remote server via encrypted PtP channel
- `verify()` checks against remote server's calendar
- `connect()` establishes one or more PtP channels
- Multi-peer: target specific peers for stamp/verify operations
- `status().peer_count` reflects connected peers

**Level 3 (P2P Full):**
- Wraps Level 2 + Communerd (libp2P swarm, client-only mode)
- `stamp()` can stamp locally OR route through mesh
- `verify()` performs distributed lookup: local → connected peers → DHT-discovered
- `join()` joins mesh via DHT + gossipsub
- `close()` tears down swarm, unsubscribes from gossipsub, deregisters from DHT
- `status().dht_connected` and `status().gossipsub_subscribed` are true

---

## 4. REFACTORING IMPLICATIONS

### 4.1 Extract C11 Noise_XX PtP Client into Library

**Current:** `json_rpc_call` in `main.rs` — binary-scoped.
**Required:** `foretias-node/src/client/noise_ptp.rs` — library-scoped module.

The new module provides:
```rust
pub mod noise_ptp {
    /// Perform a single PtP request over C11 Noise_XX encrypted TCP.
    pub async fn noise_json_rpc(
        server: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>>;
}
```

This uses the C11 `noise_xx.c` state machine for direct TCP PtP encryption. The `libp2p-noise` transport is unaffected — it lives in `communerd` and is used only by Level 3.

**What moves from `main.rs` to `lib.rs`:**
- Ephemeral keypair generation
- TCP connection
- Noise_XX handshake (C11)
- JSON-RPC encoding/decoding
- Error handling

**What stays in `main.rs`:**
- CLI argument parsing
- File I/O (read message, write output)
- Config file loading
- Logging setup

### 4.2 Decouple Communerd from Server Daemon

**Current:** `Communerd::new()` takes `Arc<TimeFamilyServer>`. Assumes a server is listening.
**Required:** `Communerd` supports a client-only mode.

Changes:
- `TimeFamilyServer` reference becomes `Option<Arc<TimeFamilyServer>>`
- When `None`, Communerd operates in client-only mode:
  - DHT registration still works (peer discovery)
  - Gossipsub still works (probity gossip, heartbeat)
  - Request-response still works (sending to peers)
  - No server listener, no incoming connections
- New constructor: `Communerd::client_only(config)` — no server, just mesh participation

### 4.3 Unified ThinClient Type

**New module:** `foretias-node/src/client/mod.rs`

```rust
pub mod client {
    pub mod noise_ptp;   // Level 2 transport
    pub mod thin_client; // Unified type
    
    pub use thin_client::{ThinClient, ClientLevel, ThinClientError, ThinClientStatus, P2pJoinConfig};
}
```

The `ThinClientInner` enum holds the three mode-specific states:
```rust
enum ThinClientInner {
    Standalone(StandaloneState),
    Ptp(PtpState),
    P2p(P2pState),
}
```

Each variant holds the appropriate components:
- **StandaloneState:** Chronomatter, Calendar, CryptoServer
- **PtpState:** StandaloneState + Vec<PtpConnection> (C11 Noise_XX clients)
- **P2pState:** PtpState + Communerd (client-only mode)

### 4.4 Tokio Runtime Management

The `ThinClient` must support both async and sync callers:
- Level 1 (Standalone) operations can be synchronous
- Level 2 and 3 operations are inherently async

Options:
1. **Internal runtime:** `ThinClient` creates its own Tokio runtime. Methods like `stamp()` internally call `runtime.block_on(async { ... })`. Simplest for bindings but ties the client to a specific runtime model.
2. **External runtime:** `ThinClient` accepts a `&tokio::runtime::Runtime` reference. Callers manage the runtime. More flexible but requires callers to set up Tokio.
3. **Hybrid:** Level 1 methods are sync; Level 2/3 methods are async. Bindings bridge the gap.

**Recommended:** Option 3 — hybrid. Level 1 is sync (no network needed). Level 2/3 are async. Bindings that need sync wrappers can use `runtime.block_on()` themselves.

### 4.5 Persist Path Handling

**Current:** Persistence is scattered across `TimeFamilyServer` (calendar store), `Chronomatter` (tick state), and `CalendarStore` (encrypted JSONL).

**Required:** `ThinClient` consolidates persistence:
- `new(tbn, Some(persist_path))` — creates fresh time family with persistence
- `from_persist(path)` — loads dormant calendar
- All three levels support persistence
- On shutdown, calendar is flushed to disk

---

## 5. WHAT DOES NOT CHANGE

- **C11 core:** No changes. `noise_xx.c` remains the PtP Noise implementation.
- **core-engine:** No changes. `Chronomatter`, `Calendar`, `CryptoServer` remain as-is.
- **foretias-node server:** `TimeFamilyServer` remains unchanged. The server daemon continues to work as before.
- **foretias-node CLI:** `main.rs` continues to work as before. The CLI will eventually call the new `ThinClient` API for stamp/verify commands, but this is a follow-up.
- **Domain types:** `Foretis`, `TickRecord`, `Calendar`, `ProbityReport` — unchanged.

---

## 6. TESTING STRATEGY

### 6.1 Functional Tests (Level 1 — Standalone)

**Test Environment:** Pure in-memory. No sockets, no disk.

| # | Test | Assertion |
|---|------|-----------|
| F1 | Stamp produces valid Foretis | `stamp("hello")` returns correct `content_hash`, `tick_number`, `echo`, `tbn` |
| F2 | Stamp-verify roundtrip | `verify("hello", stamp("hello"))` returns `true` |
| F3 | Wrong content fails verification | `verify("wrong", stamp("hello"))` returns `false` |
| F4 | Multiple stamps produce distinct Foretis | Two `stamp("hello")` calls produce different `tick_number` values |
| F5 | Tick monotonicity | `tick_number` increases with each stamp |
| F6 | Calendar reflects stamps | `calendar_slice(0, 10)` returns all stamped records in order |
| F7 | Identity accessors return valid data | `public_key()`, `tbid()`, `tbn()` return well-formed values |
| F8 | Persistence roundtrip | Stamp → persist → `from_persist()` → verify original stamps |
| F9 | Dormant mode is verify-only | `from_persist(path).stamp()` returns `DormantError` |
| F10 | Foretis serialization roundtrip | `to_json()` → `from_json()` → fields match |

**Location:** `foretias-node/src/client/thin_client.rs` (inline unit tests) + `foretias-node/tests/client_standalone.rs` (integration tests).

### 6.2 Integration Tests (Level 2 — PtP Networked)

**Test Environment:** In-process server + PtP client. Real TCP socket, C11 Noise_XX encryption.

| # | Test | Assertion |
|---|------|-----------|
| I1 | Remote stamp succeeds | `client.stamp("hello")` returns Foretis from server's tick |
| I2 | Remote stamp-verify roundtrip | `client.verify("hello", client.stamp("hello"))` returns `true` |
| I3 | Remote stamp, local verify | Stamp on server, verify on fresh `connect()` to same server |
| I4 | Calendar slice from remote | `client.calendar_slice(0, 5)` returns server's tick records |
| I5 | Prove verification (remote) | `client.prove_verification()` fetches calendar slice, verifies locally |
| I6 | Server restart recovery | Stop server → restart → client reconnects → verifies old stamps |
| I7 | Unreachable server | `client.stamp()` returns error (timeout, not silent failure) |
| I8 | Multiple peers | Two servers → client connects to both → can target specific peer |
| I9 | PtP encryption sanity | Wire traffic is encrypted (not plaintext JSON) |

**Location:** `foretias-node/tests/client_ptp.rs`

### 6.3 End-to-End Tests (Level 3 — P2P Full)

**Test Environment:** Multiple servers in P2P mode + thin client that joins the mesh.

| # | Test | Assertion |
|---|------|-----------|
| E1 | Client discovers peers | After `join()`, `client.status()` shows discovered peers |
| E2 | Stamp via P2P routing | `client.stamp("hello", target=...)` routes through mesh |
| E3 | Verify via P2P lookup | `client.verify()` finds attestation via DHT peer lookup |
| E4 | Gossip propagation | Server A stamps → server B receives via gossipsub |
| E5 | Prove verification via mesh | `client.prove_verification()` fetches from best-reputation peer |
| E6 | Client leaves and rejoins | Disconnect → reconnect → calendar replicates, stamps verifiable |

**Location:** `foretias-node/tests/client_p2p.rs`

### 6.4 Test Execution Matrix

| Tier | Level | Runner | Isolation | Duration |
|------|-------|--------|-----------|----------|
| Functional | Standalone | Unit + integration | Pure in-memory | <1s |
| Integration | PtP Networked | Integration tests | In-process server | ~30s |
| E2E | P2P Full | Integration tests | Multi-node mesh | ~120s |

**Golden rule:** Functional tests must pass before Integration tests are meaningful. Integration tests must pass before E2E tests are meaningful.

---

## 7. RELATIONSHIP TO SCOPE REDUCTION

This spec is a **companion** to `SCOPE_REDUCTION_SPEC.md`. The scope reduction removes Python/Java bindings. This spec defines the Rust library refactoring that makes the library usable in three modes — for Rust programs AND future language bindings.

The scope reduction spec preserves the language-agnostic thin client model (invariants, operations, testing strategy). This spec focuses on the Rust implementation details.

When language bindings are reintroduced, they will:
1. Confirm this Rust spec is implemented (or implement it if not)
2. Write thin FFI wrappers around the `ThinClient` type
3. Pass the three-tier test matrix

---

# END OF SPEC
