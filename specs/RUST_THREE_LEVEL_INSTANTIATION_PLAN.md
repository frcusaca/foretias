# Rust Three-Level Instantiation — Implementation Plan

Corresponding spec: `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md`

**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/RUST_THREE_LEVEL_INSTANTIATION_${RANDOM}`
**Branch:** `feat/rust-three-level-instantiation`

## Pre-condition
- `Chronomatter::stamp()` and `verify()` are fully library-scoped in core-engine (confirmed ✅)
- `NoiseSession` and `noise_handshake()` are library-scoped in core-engine (confirmed ✅)
- `Communerd` is already decoupled from `TimeFamilyServer` — takes only `CommunerdConfig`, no `Arc<TimeFamilyServer>` (confirmed ✅)
- `json_rpc_call` in `main.rs` is binary-scoped and needs extraction
- `JsonRpcTransport` in `communerd/json_rpc_transport.rs` has PtP client logic but tied to `PeerAddr`/`TransportError`

---

## Phase 0: Commit spec + plan, then branch

- [ ] Commit `RUST_THREE_LEVEL_INSTANTIATION_SPEC.md` and `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` to alpha
- [ ] Create worktree `git worktree add -b feat/rust-three-level-instantiation ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path.

---

## Phase 1: Extract C11 Noise_XX PtP Client into Library

**Goal:** Move the Noise client logic from `main.rs` into a reusable library module so it can be used by `ThinClient::connect()`.

- [ ] Create `p2p/foretias-node/src/client/noise_ptp.rs`
- [ ] Extract `noise_json_rpc(server, method, params) -> Result<serde_json::Value, Box<dyn std::error::Error>>`:
  - TCP connection to server
  - Ephemeral Ed25519 keypair generation (use `foretias_core::core::identity::generate_ed25519_keypair`)
  - Noise_XX handshake via `foretias_core::noise::noise_handshake()` (initiator, no static key)
  - JSON-RPC request encoding/decoding
  - Error handling with timeout
- [ ] Extract `noise_json_rpc_typed<T: Deserialize>(server, method, params) -> Result<T, Box<dyn std::error::Error>>`:
  - Wraps `noise_json_rpc` + deserializes JSON-RPC result into typed Rust structs (`Foretis`, `Vec<TickRecord>`, etc.)
- [ ] Update `main.rs` `cmd_stamp`, `cmd_verify`, `cmd_prove_verification` to call the new library functions instead of inline `json_rpc_call`
- [ ] Delete old `json_rpc_call` function from `main.rs` (or keep as deprecated wrapper)
- [ ] Verify: `cargo build -p foretias-node` passes
- [ ] Verify: `cargo test -p foretias-node` passes (existing tests still work since `main.rs` behavior is unchanged)

---

## Phase 2: Add `client` Module to `foretias-node` `lib.rs`

**Goal:** Expose the client module as part of the public library API.

- [ ] Create `p2p/foretias-node/src/client/mod.rs`
- [ ] Add `pub mod client;` to `p2p/foretias-node/src/lib.rs`
- [ ] Re-export from `client/mod.rs`: `noise_ptp`, and placeholder for `thin_client`
- [ ] Verify: `cargo build -p foretias-node` passes

---

## Phase 3: Implement `ThinClient` Type (Level 1 — Standalone)

**Goal:** Provide `ThinClient::new()` and `ThinClient::from_persist()` — no network, pure in-memory.

**Key insight:** `Chronomatter` in core-engine already supports standalone stamping. The Calendar (core-engine) implements `TickObserver`. We just need to wire them together.

- [ ] Create `p2p/foretias-node/src/client/thin_client.rs`
- [ ] Define types:
  ```rust
  pub enum ClientLevel { Standalone, Ptp, P2p }
  pub enum ThinClientInner { Standalone(StandaloneState), Ptp(PtpState), P2p(P2pState) }
  pub struct ThinClient { level: ClientLevel, inner: ThinClientInner }
  pub struct StandaloneState { chronomatter: Arc<Chronomatter>, calendar: Arc<Calendar>, ... }
  ```
- [ ] Implement `ThinClient::new(tbn: String, persist_path: Option<PathBuf>) -> Result<Self, ThinClientError>`:
  - Create `CryptoServer` via `new_software()`
  - Create `Calendar` (core-engine) or `Calendar` (foretias-node wrapper with `RwLock`)
  - Calendar implements `TickObserver` → pass to `Chronomatter::new(chronon_ns, Arc::new(calendar))`
  - Store in `StandaloneState`
- [ ] Implement `ThinClient::from_persist(path: PathBuf) -> Result<Self, ThinClientError>`:
  - Load calendar from path
  - Create dormant `Chronomatter` via `Chronomatter::from_calendar()`
  - `is_dormant` flag set to true
- [ ] Implement common operations for Level 1:
  - `stamp(content, echo)` → `self.inner.chronomatter().stamp()`
  - `verify(content, foretis)` → `self.inner.chronomatter().verify(foretis, content, calendar)`
  - `calendar_slice(start, count)` → `self.inner.calendar().ticks[start..start+count]`
  - `public_key()`, `tbid()`, `tbn()`, `status()`
- [ ] Verify: `cargo build -p foretias-node` passes

---

## Phase 4: Implement `ThinClient::connect()` (Level 2 — PtP Networked)

**Goal:** Provide PtP client that uses C11 Noise_XX for direct encrypted connections.

- [ ] Define `PtpState { standalone: StandaloneState, connections: Vec<PtpConnection> }`
- [ ] Implement `ThinClient::connect(tbn: String, peer_addrs: Vec<String>) -> Result<Self, ThinClientError>`:
  - Create standalone state (same as Level 1)
  - For each `peer_addr`, create a `PtpConnection` (stores address, no persistent connection — connect on demand)
- [ ] Implement Level 2 operations:
  - `stamp(content, echo)` → call `noise_json_rpc_typed(server, "stamp", params)` → deserialize `Foretis`
  - `verify(content, foretis)` → call `noise_json_rpc_typed(server, "verify", params)` → deserialize `bool`
  - `prove_verification(content, foretis)` → call `noise_json_rpc_typed(server, "prove-verification", params)` → deserialize `VerificationReport`
  - `calendar_slice(start, count)` → call `noise_json_rpc_typed(server, "get_calendar_slice", params)` → deserialize `Vec<TickRecord>`
- [ ] Multi-peer support: `stamp(..., target)` selects specific peer from connection list
- [ ] Verify: `cargo build -p foretias-node` passes

---

## Phase 5: Implement `ThinClient::join()` (Level 3 — P2P Full)

**Goal:** Provide P2P client that joins the mesh via Communerd.

**Key insight:** Communerd is already decoupled from TimeFamilyServer. `enable_p2p()` accepts `Option<Arc<dyn CommunerdRpcHandler>>` — pass `None` for client-only mode.

- [ ] Define `P2pState { ptp: PtpState, communerd: Arc<Communerd> }`
- [ ] Implement `ThinClient::join(config: P2pJoinConfig) -> Result<Self, ThinClientError>`:
  - Create PtpState (same as Level 2)
  - Create `Communerd::new(CommunerdConfig { ... })`
  - Call `communerd.enable_p2p(listen, dials, namespace, None, None)` — no server, no RPC handler
  - Optionally call `communerd.register_and_discover(...)` for self-registration
- [ ] Implement Level 3 operations:
  - `stamp(content, echo)` → try local stamp first, then route via `communerd.stamp_peer()` or `communerd.route_stamp()`
  - `verify(content, foretis)` → try local verify, then distributed lookup via `communerd.lookup_tbid()` → remote verify
  - `prove_verification(content, foretis)` → `communerd.get_calendar_slice()` from best-reputation peer → local verify
  - `close()` → stop Communerd tasks (swarm, gossip, heartbeat)
- [ ] Implement `wait_ready(timeout)` → poll `communerd.get_peers()` until peers > 0
- [ ] Verify: `cargo build -p foretias-node` passes

---

## Phase 6: Write Tests

### 6.1 Functional Tests (Level 1 — Standalone)

- [ ] Create `p2p/foretias-node/tests/client_standalone.rs`
- [ ] F1: Stamp produces valid Foretis
- [ ] F2: Stamp-verify roundtrip
- [ ] F3: Wrong content fails verification
- [ ] F4: Multiple stamps produce distinct Foretis (different tick_number)
- [ ] F5: Tick monotonicity
- [ ] F6: Calendar reflects stamps
- [ ] F7: Identity accessors return valid data
- [ ] F8: Persistence roundtrip (stamp → persist → from_persist → verify)
- [ ] F9: Dormant mode is verify-only (from_persist stamp returns DormantError)
- [ ] F10: Foretis serialization roundtrip
- [ ] `cargo test -p foretias-node --test client_standalone` passes

### 6.2 Integration Tests (Level 2 — PtP Networked)

- [ ] Create `p2p/foretias-node/tests/client_ptp.rs`
- [ ] I1: Remote stamp succeeds (spawn server in background, client connects, stamps)
- [ ] I2: Remote stamp-verify roundtrip
- [ ] I3: Calendar slice from remote
- [ ] I4: Prove verification (remote)
- [ ] I5: Unreachable server returns error (not silent failure)
- [ ] I6: Multiple peers (two servers, client connects to both)
- [ ] `cargo test -p foretias-node --test client_ptp` passes

### 6.3 End-to-End Tests (Level 3 — P2P Full)

- [ ] Create `p2p/foretias-node/tests/client_p2p.rs`
- [ ] E1: Client discovers peers (join mesh, wait_ready, assert peer_count > 0)
- [ ] E2: Stamp via P2P routing
- [ ] E3: Verify via P2P lookup
- [ ] E4: Client leaves and rejoins (close → rejoin → stamps still verifiable)
- [ ] `cargo test -p foretias-node --test client_p2p` passes

---

## Phase 7: Update CLI to Use ThinClient

**Goal:** The Rust CLI (`main.rs`) stamp/verify/prove-verification commands should use `ThinClient` instead of inline Noise client logic.

- [ ] Replace `cmd_stamp` → `ThinClient::connect().stamp()`
- [ ] Replace `cmd_verify` → `ThinClient::connect().verify()`
- [ ] Replace `cmd_prove_verification` → `ThinClient::connect().prove_verification()`
- [ ] Verify: `cargo test --workspace` passes (existing integration tests still pass)
- [ ] Verify: `foretias stamp` and `foretias verify` work from CLI

---

## Phase 8: Verify and cleanup

- [ ] `cargo test --workspace` — all Rust tests pass
- [ ] `cargo clippy --workspace` — no errors
- [ ] `cd p2p/core/build && ctest` — C11 tests pass
- [ ] Verify `ThinClient` is re-exported from `foretias_node::client`
- [ ] Verify all work is complete in `${FULL_WORKTREE_PATH}` and committed to `feat/rust-three-level-instantiation`
- [ ] Merge `feat/rust-three-level-instantiation` to alpha
- [ ] Cleanup `${FULL_WORKTREE_PATH}`
- [ ] Check that `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md` has all but Cleanup checkboxes completed
- [ ] This is the last checkbox to be checked in `RUST_THREE_LEVEL_INSTANTIATION_PLAN.md`

---

# END OF PLAN
