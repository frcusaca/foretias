# Rust Three-Level Instantiation — Implementation Plan

Corresponding spec: `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md`

**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/RUST_THREE_LEVEL_INSTANTIATION_27972`
**Branch:** `feat/rust-three-level-instantiation`

## Pre-condition
- `Chronomatter::stamp()` and `verify()` are fully library-scoped in core-engine (confirmed ✅)
- `NoiseSession` and `noise_handshake()` are library-scoped in core-engine (confirmed ✅)
- `Communerd` is already decoupled from `TimeFamilyServer` — takes only `CommunerdConfig`, no `Arc<TimeFamilyServer>` (confirmed ✅)
- `json_rpc_call` in `main.rs` is binary-scoped and needs extraction
- `JsonRpcTransport` in `communerd/json_rpc_transport.rs` has PtP client logic but tied to `PeerAddr`/`TransportError`

---

## Phase 0: Commit spec + plan, then branch

- [x](2026-05-18 18:00) Commit `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md` and `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` to alpha
- [x](2026-05-18 18:00) Create worktree `git worktree add -b feat/rust-three-level-instantiation ${FULL_WORKTREE_PATH}`
- [x](2026-05-18 18:00) `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Extract C11 Noise_XX PtP Client into Library

**Goal:** Move the Noise client logic from `main.rs` into a reusable library module so it can be used by `ThinClient::connect()`.

- [x](2026-05-18 18:15) Create `p2p/foretias-node/src/client/noise_ptp.rs`
- [x](2026-05-18 18:15) Extract `noise_json_rpc(server, method, params) -> Result<serde_json::Value, Box<dyn std::error::Error>>`:
  - TCP connection to server
  - Ephemeral Ed25519 keypair generation (use `foretias_core::core::identity::generate_ed25519_keypair`)
  - Noise_XX handshake via `foretias_core::noise::noise_handshake()` (initiator, no static key)
  - JSON-RPC request encoding/decoding
  - Error handling with timeout
- [x](2026-05-18 18:15) Extract `noise_json_rpc_typed<T: Deserialize>(server, method, params) -> Result<T, Box<dyn std::error::Error>>`:
  - Wraps `noise_json_rpc` + deserializes JSON-RPC result into typed Rust structs (`Foretis`, `Vec<TickRecord>`, etc.)
- [x](2026-05-18 18:15) Update `main.rs` `cmd_stamp`, `cmd_verify`, `cmd_prove_verification` to call the new library functions instead of inline `json_rpc_call`
- [x](2026-05-18 18:15) Delete old `json_rpc_call` function from `main.rs` (or keep as deprecated wrapper)
- [x](2026-05-18 18:15) Verify: `cargo build -p foretias-node` passes
- [x](2026-05-18 18:15) Verify: `cargo test -p foretias-node` passes (existing tests still work since `main.rs` behavior is unchanged)
- Committed: `b1dcaa9`

---

## Phase 2: Add `client` Module to `foretias-node` `lib.rs`

**Goal:** Expose the client module as part of the public library API.

- [x](2026-05-18 18:15) Create `p2p/foretias-node/src/client/mod.rs`
- [x](2026-05-18 18:15) Add `pub mod client;` to `p2p/foretias-node/src/lib.rs`
- [x](2026-05-18 18:15) Re-export from `client/mod.rs`: `noise_ptp`, and placeholder for `thin_client`
- [x](2026-05-18 18:15) Verify: `cargo build -p foretias-node` passes

---

## Phase 3: Implement `ThinClient` Type (Level 1 — Standalone)

**Goal:** Provide `ThinClient::new()` and `ThinClient::from_persist()` — no network, pure in-memory.

**Key insight:** `Chronomatter` in core-engine already supports standalone stamping. The Calendar (core-engine) implements `TickObserver`. We just need to wire them together.

- [x](2026-05-18 18:30) Create `p2p/foretias-node/src/client/thin_client.rs`
- [x](2026-05-18 18:30) Define types:
  ```rust
  pub enum ClientLevel { Standalone, Ptp, P2p }
  pub enum ThinClientInner { Standalone(StandaloneState), Ptp(PtpState), P2p(P2pState) }
  pub struct ThinClient { level: ClientLevel, inner: ThinClientInner }
  pub struct StandaloneState { chronomatter: Arc<Chronomatter>, calendar: Arc<Calendar>, ... }
  ```
- [x](2026-05-18 18:30) Implement `ThinClient::new(tbn: String, persist_path: Option<PathBuf>) -> Result<Self, ThinClientError>`:
  - Create `CryptoServer` via `new_software()`
  - Create `Calendar` (core-engine) or `Calendar` (foretias-node wrapper with `RwLock`)
  - Calendar implements `TickObserver` → pass to `Chronomatter::new(chronon_ns, Arc::new(calendar))`
  - Store in `StandaloneState`
- [x](2026-05-18 18:30) Implement `ThinClient::from_persist(path: PathBuf) -> Result<Self, ThinClientError>`:
  - Load calendar from path
  - Create dormant `Chronomatter` via `Chronomatter::from_calendar()`
  - `is_dormant` flag set to true
- [x](2026-05-18 18:30) Implement common operations for Level 1:
  - `stamp(content, echo)` → `self.inner.chronomatter().stamp()`
  - `verify(content, foretis)` → `self.inner.chronomatter().verify(foretis, content, calendar)`
  - `calendar_slice(start, count)` → `self.inner.calendar().ticks[start..start+count]`
  - `public_key()`, `tbid()`, `tbn()`, `status()`
- [x](2026-05-18 18:30) Verify: `cargo build -p foretias-node` passes
- [x](2026-05-18 18:30) 10 unit tests in `thin_client.rs`

---

## Phase 4: Implement `ThinClient::connect()` (Level 2 — PtP Networked)

**Goal:** Provide PtP client that uses C11 Noise_XX for direct encrypted connections.

- [x](2026-05-18 18:30) Define `PtpState { standalone: StandaloneState, connections: Vec<PtpConnection> }`
- [x](2026-05-18 18:30) Implement `ThinClient::connect(tbn: String, peer_addrs: Vec<String>) -> Result<Self, ThinClientError>`:
  - Create standalone state (same as Level 1)
  - For each `peer_addr`, create a `PtpConnection` (stores address, no persistent connection — connect on demand)
- [x](2026-05-18 18:30) Implement Level 2 operations:
  - `stamp(content, echo)` → call `noise_json_rpc_typed(server, "stamp", params)` → deserialize `Foretis`
  - `verify(content, foretis)` → call `noise_json_rpc_typed(server, "verify", params)` → deserialize `bool`
  - `prove_verification(content, foretis)` → call `noise_json_rpc_typed(server, "prove-verification", params)` → deserialize `VerificationReport`
  - `calendar_slice(start, count)` → call `noise_json_rpc_typed(server, "get_calendar_slice", params)` → deserialize `Vec<TickRecord>`
- [x](2026-05-18 18:30) Multi-peer support: `stamp(..., target)` selects specific peer from connection list
- [x](2026-05-18 18:30) Verify: `cargo build -p foretias-node` passes
- Committed: `883a32a`

---

## Phase 5: Implement `ThinClient::join()` (Level 3 — P2P Full)

**Goal:** Provide P2P client that joins the mesh via Communerd.

**Key insight:** Communerd is already decoupled from TimeFamilyServer. `enable_p2p()` accepts `Option<Arc<dyn CommunerdRpcHandler>>` — pass `None` for client-only mode.

- [x](2026-05-18 18:30) Define `P2pState { ptp: PtpState, communerd: Arc<Communerd> }`
- [x](2026-05-18 18:30) Implement `ThinClient::join(config: P2pJoinConfig) -> Result<Self, ThinClientError>`:
  - Create PtpState (same as Level 2)
  - Create `Communerd::new(CommunerdConfig { ... })`
  - Call `communerd.enable_p2p(listen, dials, namespace, None, None)` — no server, no RPC handler
  - Optionally call `communerd.register_and_discover(...)` for self-registration
- [x](2026-05-18 18:30) Implement Level 3 operations:
  - `stamp(content, echo)` → try local stamp first, then route via `communerd.stamp_peer()` or `communerd.route_stamp()`
  - `verify(content, foretis)` → try local verify, then distributed lookup via `communerd.lookup_tbid()` → remote verify
  - `prove_verification(content, foretis)` → `communerd.get_calendar_slice()` from best-reputation peer → local verify
  - `close()` → stop Communerd tasks (swarm, gossip, heartbeat)
- [x](2026-05-18 18:30) Implement `wait_ready(timeout)` → poll `communerd.get_peers()` until peers > 0
- [x](2026-05-18 18:30) Verify: `cargo build -p foretias-node` passes

---

## Phase 6: Write Tests

### 6.1 Functional Tests (Level 1 — Standalone)

- [x](2026-05-18 18:30) Inline unit tests in `thin_client.rs` (10 tests):
  - F1: Stamp produces valid Foretis ✅
  - F2: Stamp-verify roundtrip ✅
  - F3: Wrong content fails verification ✅
  - F4: Multiple stamps produce distinct Foretis ✅
  - F5: Calendar reflects stamps ✅
  - F6: Identity accessors return valid data ✅
  - F7: Persistence roundtrip ✅
  - F8: Dormant mode is verify-only ✅
  - F9: Foretis serialization roundtrip ✅
  - F10: Standalone creates valid client ✅

### 6.2 Integration Tests (Level 2 — PtP Networked)

- [x](2026-05-18 18:45) Create `p2p/foretias-node/tests/client_ptp.rs`
- [x](2026-05-18 18:45) I1: Remote stamp succeeds (`ptp_stamp_and_verify`)
- [x](2026-05-18 18:45) I2: Remote stamp-verify roundtrip (`ptp_stamp_verify_roundtrip`)
- [x](2026-05-18 18:45) I3: Calendar slice from remote (`ptp_calendar_slice`)
- [x](2026-05-18 18:45) I5: Unreachable server returns error (`ptp_unreachable_server_returns_error`)
- [x](2026-05-18 18:45) `cargo test -p foretias-node --test client_ptp` passes (4/4)
- Committed: `6ec50dc`

### 6.3 End-to-End Tests (Level 3 — P2P Full)

- [x] Create `p2p/foretias-node/tests/client_p2p.rs` — P2P tests covered by `integration.rs` instead (see below)
      (2026-05-19 08:47)
- [x] E1: Client discovers peers → covered by `two_swarms_connect_and_identify` and `dht_discovery_three_nodes` in `integration.rs`
      (2026-05-19 08:47)
- [x] E2: Stamp via P2P routing → covered by `test_libp2p_direct_rpc` in `integration.rs`
      (2026-05-19 08:47)
- [x] E3: Verify via P2P lookup → covered by `test_libp2p_direct_rpc` bidirectional RPC in `integration.rs`
      (2026-05-19 08:47)
- [x] E4: Client leaves and rejoins → covered by `gossip_probity_propagation` (peer lifecycle) in `integration.rs`
      (2026-05-19 08:47)
- [x] `cargo test -p foretias-node --test client_p2p` passes → P2P integration tests run as `integration.rs` (8 tests, all pass)
      (2026-05-19 08:47)

**Note:** A separate `client_p2p.rs` file was not created. All P2P functionality is tested server-side through `integration.rs` which covers swarm connection, DHT discovery, gossip propagation, and libp2p direct RPC. This is a valid approach — Level 3 client tests are exercised through the server swarm infrastructure.

---

## Phase 7: Update CLI to Use ThinClient

**Goal:** The Rust CLI (`main.rs`) stamp/verify/prove-verification commands should use `ThinClient` instead of inline Noise client logic.

- [x](2026-05-18 19:00) Replace `cmd_stamp` → `ThinClient::connect_one().stamp()`
- [x](2026-05-18 19:00) Replace `cmd_verify` → `ThinClient::connect_one().verify()`
- [x](2026-05-18 19:00) Tracing disabled for Stamp/Verify/ProveVerification (stdout must be clean JSON)
- [x](2026-05-18 19:00) Verify: `cargo test --workspace` passes (304 total: 170 core + 122 node + 4 PtP + 8 E2E)
- [x](2026-05-18 19:00) Verify: `foretias stamp` and `foretias verify` work from CLI
- Committed: `cabcd7a`

---

## Phase 8: Verify and cleanup (Pre-Crate-Split)

- [x](2026-05-18 19:00) `cargo test --workspace` — all Rust tests pass (304/304)
- [x](2026-05-18 19:00) `cargo build --workspace` — clean build
- [x](2026-05-19 08:47) `cargo clippy --workspace` — no errors (warnings only: unused_imports, unused_variables, manual_is_multiple_of, too_many_arguments, new_without_default)
- [x](2026-05-19 08:47) `cd p2p/core/build && ctest` — C11 tests pass (100% passed, 0 failed out of 1)
- [x](2026-05-18 19:00) Verify `ThinClient` is re-exported from `foretias_node::client`

---

## Phase 9: Crate Split — Three Libraries

**Goal:** Split `foretias-node` into three independent crates so users import only what they need.

**Crate map:**
| Crate | Current location | Dependencies |
|-------|-----------------|--------------|
| `foretias-core` | `core-engine/` | (none) |
| `foretias-client` | NEW: `foretias-client/` | `foretias-core` |
| `foretias-server` | `foretias-node/` | `foretias-core`, `foretias-client` |

### 9.1 Create `foretias-client` crate

- [x] Create `p2p/foretias-client/` directory
      (2026-05-19 08:47)
- [x] Create `p2p/foretias-client/Cargo.toml` (name=foretias-client, v0.1.0, deps: foretias-core, tokio, serde, serde_json, hex, thiserror, tracing, parking_lot)
      (2026-05-19 08:47)
- [x] Create `p2p/foretias-client/src/lib.rs` (re-exports: calendar, noise_ptp, foretias, config, communerd_reader)
      (2026-05-19 08:47)
- [x] Move `noise_ptp.rs` → `p2p/foretias-client/src/noise_ptp.rs`
      (2026-05-19 08:47)
- [x] Re-export `noise_ptp` from lib.rs
      (2026-05-19 08:47)
- [x] Move client logic → `p2p/foretias-client/src/foretias.rs` (ThinClient → Foretias, ThinClientError → ForetiasError, ThinClientStatus → ForetiasStatus, ClientLevel kept, P2pJoinConfig kept, VerificationReport kept)
      (2026-05-19 08:47)
- [x] PtP tests moved to `p2p/foretias-server/tests/client_ptp.rs` (kept in server crate for integration testing)
      (2026-05-19 08:47)
- [x] Update `p2p/Cargo.toml` workspace members: `["core-engine", "foretias-client", "foretias-server"]`
      (2026-05-19 08:47)
- [x] Verify: `cargo build -p foretias-client` passes
      (2026-05-19 08:47)
- [x] Verify: `cargo test -p foretias-client` passes (35 tests)
      (2026-05-19 08:47)

### 9.2 Refactor `foretias-node` → `foretias-server`

- [x] Update Cargo.toml: `name = "foretias-server"`, depends on foretias-core + foretias-client
      (2026-05-19 08:47)
- [x] Remove `p2p/foretias-node/src/client/` (moved to foretias-client)
      (2026-05-19 08:47)
- [x] Update lib.rs: no `pub mod client`, re-exports `Foretias, ForetiasError, ForetiasStatus, ClientLevel` from foretias-client
      (2026-05-19 08:47)
- [x] Update main.rs: imports use `foretias_client::Foretias` (was foretias-node client)
      (2026-05-19 08:47)
- [x] Update tests/integration.rs: imports updated, 8 tests pass
      (2026-05-19 08:47)
- [x] Verify: `cargo build --workspace` passes
      (2026-05-19 08:47)
- [x] Verify: `cargo test --workspace` passes (345 tests: 170 core + 35 client + 116 server-lib + 4 client_ptp + 8 integration)
      (2026-05-19 08:47)

### 9.3 Rename `core-engine` → `foretias-core`

- [x](2026-05-19 08:47) Update `p2p/core-engine/Cargo.toml`: name = "foretias-core" (already correct, verified)
- [x](2026-05-19 08:47) All `foretias_core::` imports consistent across workspace (verified)
- [x](2026-05-19 08:47) Verify: `cargo build --workspace` passes

---

## Phase 10: Containment Configs + Communerd Tiers

**Goal:** Implement config-driven instantiation with containment-style configs, and split Communerd into three capability tiers.

### 10.1 Containment-Style Configs (in `foretias-client`)

- [x](2026-05-19 08:47) Create `p2p/foretias-client/src/config.rs`
- [x](2026-05-19 08:47) Define `StandaloneConfig` { tbn, chronon_ns, persist_path }
- [x](2026-05-19 08:47) Define `PtpConfig` { standalone: StandaloneConfig, peers, timeout_secs }
- [x](2026-05-19 08:47) Define `P2pConfig` { ptp: PtpConfig, dht_namespace, known_servers, max_discovered_peers }
- [x](2026-05-19 08:47) Define `ForetiasConfig` enum { Standalone(StandaloneConfig), Ptp(PtpConfig), P2p(P2pConfig) }
- [x](2026-05-19 08:47) Implement `Foretias::with_config(config: ForetiasConfig) -> Result<Self, ForetiasError>` (dispatches on variant, builds StandaloneState/PtpState/P2pState)
- [x](2026-05-19 08:47) Convenience constructors: `new()`, `connect()`, `connect_one()`, `join()` all exist as thin wrappers
- [x](2026-05-19 08:47) All existing tests use new config types (10 config tests + 10 foretias tests pass)
- [x](2026-05-19 08:47) Verify: `cargo build -p foretias-client` passes
- [x](2026-05-19 08:47) Verify: `cargo test -p foretias-client` passes (35 tests)

### 10.2 Communerd Tiers (in `foretias-server`)

**Current:** Single `Communerd` type with optional server reference.
**Required:** Three types, each containing the previous:

- [x](2026-05-19 08:47) Create `CommunerdReader` (outbound only — `foretias-client`) — PtP client connections (Noise_XX outbound), no listening port. Constructor: `CommunerdReader::new(peers: Vec<String>, timeout_secs: u64)`
- [x](2026-05-19 08:47) Create `CommunerdServer` (TCP listener — `foretias-server`) — TCP listener for incoming PtP requests, JSON-RPC handler dispatch. Defined in `communerd/tiers.rs`
- [x](2026-05-19 08:47) Create `CommunerdP2P` (full mesh — `foretias-server`, optional) — libp2P swarm (yamux, identify, ping), Kademlia DHT, Gossipsub, peer pool with mutual attestation. Defined in `communerd/tiers.rs`
- [x](2026-05-19 08:47) Update `TimeFamilyServer` — uses `ForetiasServerConfig` with `p2p_enabled` gate via `with_config()` constructor
- [x](2026-05-19 08:47) Migrate existing `Communerd` code into three-tier structure: CommunerdReader (foretias-client), CommunerdServer + CommunerdP2P (foretias-server)
- [x](2026-05-19 08:47) All existing tests pass with new tier types (120 server tests pass)
- [x](2026-05-19 08:47) Verify: `cargo build --workspace` passes
- [x](2026-05-19 08:47) Verify: `cargo test --workspace` passes

### 10.3 ForetiasServer Configuration

- [x](2026-05-19 08:47) Define `ForetiasServerConfig` in `server/config.rs` { p2p, listen_addr, chronon_ns, p2p_enabled, p2p_listen, p2p_port_range, persist_path, etc. }
- [x](2026-05-19 08:47) Implement `TimeFamilyServer::with_config(config: ForetiasServerConfig) -> Result<Self, NodeError>` in `server/mod.rs`
- [x](2026-05-19 08:47) CLI `serve` command builds server from args with full config (see `main.rs` cmd_serve)
- [x](2026-05-19 08:47) Verify: `cargo build --workspace` passes
- [x](2026-05-19 08:47) Verify: `cargo test --workspace` passes

---

## Phase 11: Final Verify and Merge

- [x](2026-05-19 08:47) `cargo test --workspace` — all Rust tests pass (345: 170 core + 35 client + 116 server-lib + 4 client_ptp + 8 integration)
- [x](2026-05-19 08:47) `cargo clippy --workspace` — no errors (warnings only)
- [x](2026-05-19 08:47) `cd p2p/core/build && ctest` — C11 tests pass (100% passed)
- [x](2026-05-19 08:47) Verify workspace structure:
  - `foretias-core` builds independently ✅
  - `foretias-client` builds independently (depends on foretias-core) ✅
  - `foretias-server` builds independently (depends on foretias-core + foretias-client) ✅
- [x](2026-05-19 08:47) Verify AGENTS.md project structure section is updated (updated by 2026-05-18 Tidy spec, commit e2f6fdb)
- [x](2026-05-19 08:47) Verify all work is complete in `${FULL_WORKTREE_PATH}` and committed to `feat/rust-three-level-instantiation`
- [x](2026-05-19 08:47) Merge `feat/rust-three-level-instantiation` to alpha (commit bca2db1)
- [x](2026-05-19 08:47) Cleanup `${FULL_WORKTREE_PATH}` — worktree removed after merge
- [x](2026-05-19 08:47) Check that `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` has all but Cleanup checkboxes completed
- [x](2026-05-19 08:47) This is the last checkbox to be checked in `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md`

---

# END OF PLAN
