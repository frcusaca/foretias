# HTTPS Transport — Implementation Plan

**Pairs with:** `FORETIAS_6_HTTPS_TRANSPORT_SPEC.md`

**Worktree path:** `FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/FORETIAS_6_HTTPS_TRANSPORT_4821`
**Branch:** `feat/foretias-6-https-transport`

---

## Step 0 — Terminology Normalization (PtP / P2P)

**Goal:** Replace all occurrences of "point-to-point" (and variants) with `PtP`; replace all occurrences of "peer-to-peer" (and variants) with `P2P`. Apply across specs, plans, code, comments, and root docs.

**Rules (from AGENTS.md + README.md):**
- `PtP` = point-to-point (direct channel between two parties). Never "point-to-point", "point to point", "point2point", or "PTP".
- `P2P` = peer-to-peer (network of distributed nodes). Never "peer-to-peer", "peer to peer", "peer2peer".
- **Prose:** Precisely `PtP` / `P2P`. No other capitalization.
- **Code:** Exact-case or all-lowercase: `PtPClient`, `P2PClient`, `make_ptp_connection`, `make_p2p_connection`, `ptp_client`. No `PtpClient`, `P2pClient`, `PTPCLIENT`, `P2PCLIENT`.

### 0.1 Root Documentation

- [x](2026-05-13 14:45) `AGENTS.md` — Add TERMINOLOGY SHORTHANDS section (table format)
- [x](2026-05-13 14:45) `README.md` — Add inline terminology note

### 0.2 Spec Files — PtP replacements

- [x](2026-05-13 14:45) `specs/CALENDAR_REPLICATION_SPEC.md` — 2 occurrences of "point-to-point" → `PtP`
- [x](2026-05-13 14:45) `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` — 1 occurrence of "point-to-point" → `PtP`
- [x](2026-05-13 14:45) `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — 1 occurrence of "point-to-point" → `PtP`
- [x](2026-05-13 14:45) `specs/FORETIAS_6_HTTPS_TRANSPORT_SPEC.md` — 1 occurrence of "point-to-point" → `PtP`
- [x](2026-05-13 14:45) `specs/CALENDAR_ACTIVE_MIRRORING_PLAN.md` — 1 occurrence of "point-to-point" → `PtP`
- [x](2026-05-13 14:45) `specs/questions.md` — 3 occurrences of "point-to-point" → `PtP`

### 0.3 Spec Files — P2P replacements

- [x](2026-05-13 14:45) `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` — 1 occurrence of "peer-to-peer" → `P2P`
- [x](2026-05-13 14:45) `specs/WHITEPAPER_SPEC.md` — 3 occurrences of "peer-to-peer" → `P2P`
- [x](2026-05-13 14:45) `specs/FORETIAS_0_OVERVIEW.md` — 1 occurrence of "peer-to-peer" → `P2P`
- [x](2026-05-13 14:45) `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — 1 occurrence of "peer-to-peer" → `P2P`
- [x](2026-05-13 14:45) `specs/LOCAL_PEER_TESTING_ROUND_2_SPEC.md` — 1 occurrence of "peer-to-peer" → `P2P`

### 0.4 Combined PtP + P2P

- [x](2026-05-13 14:45) `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — "All point-to-point and peer-to-peer communication" → "All PtP and P2P communication"
- [x](2026-05-13 14:45) `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` — "All point-to-point & peer-to-peer communication" → "All PtP & P2P communication"

### 0.5 Spec & Plan Prefix Consistency

- [x](2026-05-13 14:45) `FORETIAS_6_HTTPS_TRANSPORT_SPEC.md` — Fix `FORETIAS_3_HTTPS_TRANSPORT_PLAN.md` → `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md`
- [x](2026-05-13 14:45) `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` — Fix `feat/foretias-3-https-transport` → `feat/foretias-6-https-transport` (branch names)
- [x](2026-05-13 14:45) `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` — Fix `FORETIAS_3_HTTPS_TRANSPORT_PLAN.md` → `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` (Phase 10 references)

### 0.6 Verification

- [x](2026-05-13 14:45) Grep confirms zero remaining occurrences of `(?i)point.?to.?point` across specs/ and root docs (except rule definitions)
- [x](2026-05-13 14:45) Grep confirms zero remaining occurrences of `(?i)peer.?to.?peer` across specs/ and root docs (except rule definitions)
- [x](2026-05-13 14:45) Grep confirms zero remaining "FORETIAS_3_HTTPS" references in FORETIAS_6 files

---

## Prerequisites

- [ ] Verify no broken tests on alpha before starting
- [ ] `COMMUNERD_LIBP2P_DIRECT_PLAN.md` merged to alpha (libp2p direct transport baseline)

---

## Worktree Setup

- [ ] Create worktree `git worktree add -b feat/foretias-6-https-transport ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset current session work directory to be the full worktree path

---

## Phase 1 — PeerAddr Redesign & TransportKind Expansion

**Goal:** Redesign `PeerAddr` to support multi-transport URLs; expand `TransportKind` enum.

### 1.1 PeerAddr Redesign

- [ ] Redesign `PeerAddr` in `p2p/foretias-node/src/communerd/transport.rs`:
  - Replace `json_rpc: String` with `primary_url: String`
  - Add `fallback_urls: Vec<String>`
  - Add `url_scheme(&self) -> TransportKind` parser
  - Add `from_json_rpc(addr: &str) -> Self` migration constructor (interprets old `"host:port"` as `foretias://host:port`)
  - Implement `From<&str>` and `TryFrom<url::Url>`
- **Verification:** `cargo check` passes; unit test — parse all 4 URL schemes, fallback list preserved

### 1.2 TransportKind Expansion

- [ ] Expand `TransportKind` enum in `transport.rs`:
  ```rust
  pub enum TransportKind {
      TcpNoise,     // was: DirectJsonRpc
      TlsNoise,     // new
      HttpsNoise,   // new
      Libp2p,
  }
  ```
- [ ] Rename `DirectJsonRpc` → `TcpNoise` (workspace-wide)
- **Verification:** `cargo check` passes

### 1.3 Port Constants

- [ ] Add port constants to `p2p/core-engine/src/config.rs`:
  ```rust
  pub const DEFAULT_FORETIAS_PORT: u16 = 4001;
  pub const DEFAULT_FORETIAS_SSL_PORT: u16 = 4433;
  pub const DEFAULT_FORETIAS_HTTPS_PORT: u16 = 443;
  ```
- **Verification:** `cargo check` passes

---

## Phase 2 — HTTPS Server Routes (Axum)

**Goal:** Extend the existing Axum HTTP server with discovery, convenience POST, and WebSocket routes.

### 2.1 HTTPS Transport Config

- [ ] Add `HttpsTransportConfig` to `core-engine/src/config.rs`
- **Verification:** `cargo check` passes

### 2.2 Discovery Endpoint

- [ ] Add `GET /foretias` route in `server/mod.rs`:
  - Return JSON with version, tbid, tbn, tick_count, transport_kinds, endpoints, methods_whitelist
  - Read whitelist from config
- **Verification:** Unit test — GET returns expected JSON structure

### 2.3 Convenience POST Endpoints

- [ ] Add `POST /stamp` route:
  - Accept `{"content": "<hex>", "echo": "<str>"}`
  - Delegate to `handlers::handle_stamp()`
  - Return `{"foretis": {...}}`
- [ ] Add `POST /verify` route:
  - Accept `{"content": "<hex>", "foretis": {...}}`
  - Delegate to `handlers::handle_verify()`
  - Return `{"valid": true, "foretis": {...}}`
- **Verification:** Unit test — POST stamp returns valid Foretis; POST verify returns correct answer

### 2.4 CORS Middleware

- [ ] Add CORS middleware to the Axum router:
  - `Access-Control-Allow-Origin` from config (default: `*` for dev)
  - Handle OPTIONS preflight on POST routes
- **Verification:** Unit test — OPTIONS returns correct headers; POST from cross-origin succeeds

### 2.5 Method Whitelist Enforcement

- [ ] Add `HttpsMethodFilter` wrapper around `process_request_from_value`:
  - Check method against whitelist before dispatching
  - Return `METHOD_NOT_FOUND` for non-whitelisted methods
  - In dev mode (`whitelist_all: true`), pass through all methods
- **Verification:** Unit test — whitelisted method passes; non-whitelisted returns error; dev mode passes all

---

## Phase 3 — WebSocket Session with Noise_XX Handshake

**Goal:** Implement the WebSocket endpoint `/foretias/ws/session` with Noise_XX handshake over binary frames, then encrypted JSON-RPC over text frames.

### 3.1 WebSocket Route

- [ ] Add `GET /foretias/ws/session` route in `server/mod.rs`:
  - Accept WebSocket upgrade (standard `Sec-WebSocket-*` headers)
  - Spawn `handle_websocket_session()` task
- **Verification:** `cargo check` passes

### 3.2 Noise_XX Handshake over WebSocket

- [ ] Implement `websocket_noise_handshake()` in new file `server/ws_noise.rs`:
  - Read 3 binary frames from WebSocket (Noise_XX handshake messages)
  - Use existing C11 Noise_XX handshake logic (`noise::noise_handshake`) but adapted for WebSocket frame I/O instead of TCP stream
  - After handshake: `Split()` → `(cipher_tx, cipher_rx)`
  - Return `(cipher_tx, cipher_rx)` for use in subsequent frame encryption
- **Verification:** Unit test — handshake completes, cipher states can encrypt/decrypt test payloads

### 3.3 WebSocket Frame Protocol

- [ ] Implement `WebSocketFrameHandler` in `server/ws_handler.rs`:
  - Read text frames → decrypt with `cipher_rx` → parse as JSON-RPC 2.0
  - Read binary frames → treat as handshake (only during initial phase)
  - Write text frames → encrypt with `cipher_tx` → send as WebSocket text frame
  - Dispatch to `process_request_from_value()` for blocking requests (with `"id"`)
  - Queue notifications (without `"id"`) for callback push
- **Verification:** Unit test — encrypted request decrypted, dispatched, encrypted response sent

### 3.4 Dual-Mode Dispatch (Blocking + Callback)

- [ ] Implement the dual-mode dispatcher in `server/ws_handler.rs`:
  - **Blocking:** `"id"` present → call handler → respond with matching `"id"`
  - **Callback trigger:** `"id"` present, method is `history_dump_request` or `stream_request` → respond with `"id"` → spawn push task
  - **Callback push:** `"id"` absent → notification from push task, no response expected
  - **Callback ack:** `"id"` absent, method is `history_dump_ack`, `stream_ack`, `stream_stop` → forward to handler
- **Verification:** Unit test — all four dispatch paths work correctly

---

## Phase 4 — HttpsTransport (Client Side, Blocking)

**Goal:** Implement `HttpsTransport: PeerTransport` for HTTP POST to `foretias+https://` URLs.

### 4.1 Create `https_transport.rs`

- [ ] Create `p2p/foretias-node/src/communerd/https_transport.rs`:
  - Struct: `HttpsTransport { timeouts, tls_config }`
  - `new(config: &HttpsTransportConfig) -> Self`
- **Verification:** `cargo check` passes

### 4.2 Blocking PeerTransport Methods

- [ ] Implement `PeerTransport` trait for `HttpsTransport`:
  - `stamp()` → POST `https://{host}:{port}/jsonrpc` with JSON-RPC body
  - `route_stamp()` → same pattern
  - `get_calendar_slice()` → same pattern, deserialize `Vec<TickRecord>`
  - `ping()` → same pattern, ignore result
- [ ] Use `reqwest` (or `hyper`) for HTTPS client with configurable TLS
- **Verification:** Unit test with mock server — each method sends correct request, parses response

### 4.3 Timeout Configuration

- [ ] Per-method timeout mapping:
  - stamp/verify/ping: 5s
  - get_calendar_slice: 30s
  - mirror operations: 60s
- **Verification:** Unit test — request exceeding timeout returns `TransportError::Timeout`

---

## Phase 5 — HttpsWebSocketTransport (Client Side, Callback)

**Goal:** Implement `HttpsWebSocketTransport` with callback-capable methods for history dump and stream sessions.

### 5.1 Create `https_ws_transport.rs`

- [ ] Create `p2p/foretias-node/src/communerd/https_ws_transport.rs`:
  - Struct: `HttpsWebSocketTransport { inner: HttpsTransport, noise_static_priv, noise_static_pub }`
  - Connect to `wss://{host}:{port}/foretias/ws/session`
  - Perform Noise_XX handshake (3 binary frames)
  - Return ready session
- **Verification:** Unit test — connects, handshake completes, session ready

### 5.2 HistoryDumpSession

- [ ] Implement `HistoryDumpSession` struct and methods:
  - `start_history_dump()` → send blocking `history_dump_request`, receive `history_dump_start` response
  - `recv_period()` → read text frame, decrypt, parse as `history_dump_period` notification
  - `send_ack()` → encrypt and send `history_dump_ack` notification
  - Detect `history_dump_end` frame → return `HistoryDumpFrame::End`
- **Verification:** Unit test with mock server — request → receive 2 periods → receive end → send ack

### 5.3 StreamSession

- [ ] Implement `StreamSession` struct and methods:
  - `start_stream()` → send blocking `stream_request`, receive `stream_start` response
  - `recv_tick()` → read text frame, decrypt, parse as `stream_tick` notification
  - `send_ack()` → encrypt and send `stream_ack` notification
  - `stop()` → encrypt and send `stream_stop` notification
  - Detect `stream_end` frame → return error or special Ok variant
- **Verification:** Unit test with mock server — request → receive 5 ticks → ack → stop

### 5.4 Session Timeout and Backpressure

- [ ] Add session-level timeout to both `HistoryDumpSession` and `StreamSession`:
  - No activity for `ws_session_timeout_secs` → close session, return timeout error
- [ ] Stream backpressure:
  - Buffer at most `stream_buffer_size` (100) received ticks
  - Buffer full + `stream_stall_timeout_s` (60s) → close session
- **Verification:** Unit test — stale session times out; slow consumer triggers stall

---

## Phase 6 — Server-Side WebSocket Callback Handlers

**Goal:** Implement server-side handlers for incoming callback requests (history dump, stream) on WebSocket.

### 6.1 handle_websocket_session Entry Point

- [ ] Implement `TimeFamilyServer::handle_websocket_session()` in `server/mod.rs`:
  - Accept WebSocket stream
  - Run Noise_XX handshake
  - Enter frame loop: read → decrypt → dispatch → encrypt → write
- **Verification:** Unit test — full session lifecycle

### 6.2 History Dump Server Handler

- [ ] Implement `handle_history_dump_request_server()` in `server/handlers.rs`:
  - On receiving `history_dump_request` (with `"id"`) on WebSocket:
    1. Look up period index for `(chronomatter_tbid, calendar_tbid)`
    2. Respond with `history_dump_start` (matching `"id"`)
    3. Push each period as `history_dump_period` notification (no `"id"`)
    4. Push `history_dump_end` (no `"id"`)
    5. Wait for `history_dump_ack` from mirror
  - **Critical:** Must push frames through the same WebSocket session (not a new connection)
- **Verification:** Unit test with mock calendar — 2500 ticks → 3 periods pushed correctly

### 6.3 Stream Server Handler

- [ ] Implement `handle_stream_request_server()` in `server/handlers.rs`:
  - On receiving `stream_request` (with `"id"`) on WebSocket:
    1. Respond with `stream_start` (matching `"id"`)
    2. Spawn background task: poll Chronomatter for new ticks → push `stream_tick` notifications
    3. Handle incoming `stream_ack` (update backpressure state)
    4. Handle incoming `stream_stop` (terminate push task)
  - **Critical:** The push task must write to the same WebSocket session
- **Verification:** Unit test — 10 ticks produced, 10 delivered; stop signal terminates cleanly

### 6.4 WebSocket Frame Writer (Shared Access)

- [ ] Design the shared writer pattern:
  - The WebSocket session has ONE writer but potentially TWO concurrent senders (response dispatcher + push task)
  - Use `tokio::sync::mpsc::channel` as a frame queue: both senders push encrypted frames into the channel, a single writer task drains and sends
  - Frame queue capacity: 256 (covers backpressure buffer + responses)
- **Verification:** Unit test — concurrent response + push frames sent without race conditions

---

## Phase 7 — Communerd Integration (Transport Selection + Fallback)

**Goal:** Wire `HttpsTransport` and `HttpsWebSocketTransport` into Communerd's transport selection and fallback chain.

### 7.1 Add Transports to Communerd

- [ ] Add `https_transport: Arc<HttpsTransport>` and `https_ws_transport: Arc<HttpsWebSocketTransport>` to `Communerd` struct
- [ ] Initialize in `Communerd::new()` from `HttpsTransportConfig`
- [ ] Clone both in `Communerd::clone()`
- **Verification:** `cargo check` passes

### 7.2 Transport Selection for Blocking Operations

- [ ] Refactor `Communerd::stamp_peer()`, `route_stamp()`, `get_calendar_slice()`, `ping()`:
  - Try transport in priority order: libp2p → TcpNoise → HttpsTransport (POST) → TlsNoise
  - Log which transport succeeded: `transport=https-post` or `transport=libp2p-direct`
- **Verification:** Unit test — each operation tries correct fallback chain

### 7.3 Transport Selection for Callback Operations

- [ ] Add new methods to `Communerd`:
  ```rust
  async fn start_history_dump(&self, peer: &PeerAddr, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16], up_to_tick: u64) -> Result<HistoryDumpSession, TransportError>;
  async fn start_stream(&self, peer: &PeerAddr, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16], from_tick: u64) -> Result<StreamSession, TransportError>;
  ```
  - Try `HttpsWebSocketTransport` first (for `foretias+https://` URLs)
  - Fall back to TcpNoise-based sessions if available (future: TCP + Noise_XX + callback protocol)
- **Verification:** Unit test — history dump starts over WebSocket; stream starts over WebSocket

### 7.4 DHT Registration with HTTPS Capabilities

- [ ] When peer registers in DHT, include `foretias+https` endpoint and `supports_websocket: true` in registration record
- **Verification:** Unit test — DHT lookup returns HTTPS endpoint with websocket capability flag

---

## <<< MID-PROJECT INSERTION POINT (Already Satisfied) >>>

**Status:** `COMMUNERD_LIBP2P_DIRECT_PLAN.md` was already executed and merged to alpha (commit 140c431, 2026-05-10). This insertion point remains documented for correctness — it shows where the libp2p direct work plugs into the HTTPS transport plan's phase ordering.

**Action at this point:** Rebase this worktree branch on alpha to pick up the libp2p direct code, then continue with Phase 8.

**Cross-file consistency notes:**

| Issue | HTTPS Transport Plan | libp2p Direct Plan | Resolution |
|-------|---------------------|-------------------|------------|
| `PeerAddr.json_rpc` → `primary_url` | Phase 1.1 redesigns PeerAddr | References `peer.json_rpc` throughout | Phase 1.1 adds `from_json_rpc()` migration constructor; workspace-wide rename in same commit |
| `TransportKind::DirectJsonRpc` → `TcpNoise` | Phase 1.2 renames enum variant | References `DirectJsonRpc` | Phase 1.2 renames all occurrences workspace-wide including libp2p direct code |
| libp2p transport in Communerd | Phase 7.1 adds HTTPS transports | Phase 4.1 adds `Libp2pTransport` | Both phases add their respective fields; no conflict (different struct fields) |
| `process_request_from_value` dispatch | Phase 6 adds history_dump/stream handlers | Uses `CommunerdRpcHandler` trait for libp2p inbound | WebSocket dispatch uses `process_request_from_value` directly; libp2p inbound uses `CommunerdRpcHandler` trait — both route to same handlers, no conflict |

---

## Phase 8 — Server HTTPS Start Integration

**Goal:** Wire the HTTPS server (Axum with all routes) into `TimeFamilyServer::start()`.

### 8.1 start_https Method

- [ ] Add `TimeFamilyServer::start_https()` (similar to `start_http()`):
  - Bind to `https_addr` from config
  - Apply TLS if `tls_cert_path` and `tls_key_path` are set (use `tokio-rustls` or `axum-server`)
  - Mount all routes: `/foretias`, `/jsonrpc`, `/stamp`, `/verify`, `/foretias/ws/session`
  - Return `JoinHandle<()>`
- **Verification:** Server starts, routes accessible

### 8.2 start() Integration

- [ ] In `TimeFamilyServer::start()` (or `start_daemon_arc`):
  - Call `start_https()` if `HttpsTransportConfig` is present
  - Call existing `start_tcp()` for TCP+Noise_XX
  - Both listeners run concurrently
- **Verification:** Server starts with both TCP and HTTPS listeners

### 8.3 CLI Flags

- [ ] Add CLI flags in `main.rs`:
  - `--https-addr` — HTTPS listen address
  - `--tls-cert` — TLS certificate path
  - `--tls-key` — TLS key path
  - `--whitelist-all` — Development mode (whitelist all methods)
- **Verification:** Binary accepts new flags, server starts correctly

---

## Phase 9 — Tests

### 9.1 Unit Tests

- [ ] `HttpsTransport` — POST stamp/verify/ping returns correct responses
- [ ] `HttpsWebSocketTransport` — WebSocket connect + Noise_XX handshake succeeds
- [ ] `HistoryDumpSession` — receive 3 period pushes + end, send ack
- [ ] `StreamSession` — receive 10 tick pushes, ack periodically, stop cleanly
- [ ] WebSocket server — handles concurrent blocking + callback on same session
- [ ] CORS middleware — OPTIONS returns headers, cross-origin POST succeeds
- [ ] Method whitelist — whitelisted passes, non-whitelisted rejected, dev mode passes all
- [ ] PeerAddr URL parsing — all 4 schemes, fallback list preserved
- [ ] Discovery endpoint — returns correct JSON structure

### 9.2 Integration Tests

- [ ] Two servers, mirror requests history dump over HTTPS WebSocket, source pushes all periods
- [ ] Two servers, mirror starts stream over HTTPS WebSocket, source pushes ticks as produced
- [ ] Fallback chain: HTTPS POST times out → TCP+Noise_XX succeeds
- [ ] Browser fetch to `/stamp` returns valid Foretis
- [ ] WebSocket session timeout — stale session closes after timeout
- [ ] Stream backpressure — slow mirror triggers stall timeout

### 9.3 Existing Tests Must Pass

- [ ] `cargo test --workspace` — all tests pass
- [ ] C11 tests (`ctest`) — all pass
- [ ] Python tests (`pytest`) — all pass

---

## Phase 10 — Final Verification & Merge

- [ ] Verify all work is complete in `${FULL_WORKTREE_PATH}` and committed to `feat/foretias-6-https-transport`
- [ ] Run `cargo clippy --workspace` — no new warnings
- [ ] Merge `feat/foretias-6-https-transport` to alpha
- [ ] Cleanup `${FULL_WORKTREE_PATH}`
- [ ] Update `COMMUNERD_LIBP2P_DIRECT_PLAN.md` — verify no conflicts post-merge
- [ ] Check that `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` has all but Cleanup checkboxes completed
- [ ] Remove `${FULL_WORKTREE_PATH}` worktree reference
- [ ] This is the last checkbox to be checked in `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md`
