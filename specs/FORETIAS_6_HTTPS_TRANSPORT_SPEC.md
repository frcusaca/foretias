# HTTPS Transport — Specification

**Prefix:** `FORETIAS_6_HTTPS_TRANSPORT`
**Pairs with:** `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md`
**Date:** 2026-05-13

---

## Step 0 — Terminology Normalization

Before any implementation, normalize PtP and P2P shorthands across the entire project:

- **PtP** = point-to-point (direct channel between two parties). Never write "point-to-point", "point to point", "point2point", or "PTP" (capital T means something else).
- **P2P** = peer-to-peer (network of distributed nodes). Never write "peer-to-peer", "peer to peer", or "peer2peer".
- **Prose:** Shorthand must be precisely capitalized `PtP` and `P2P`.
- **Code identifiers:** Exact-case or all-lowercase only. Examples: `PtPClient`, `P2PClient`, `make_ptp_connection`, `make_p2p_connection`, `ptp_client`. Forbidden: `PtpClient`, `P2pClient`, `PTPCLIENT`, `P2PCLIENT` (no mixed-case or all-caps).

This normalization applies to all specs, plans, code, comments, and documentation. See `AGENTS.md` TERMINOLOGY SHORTHANDS section and `README.md` for the authoritative rule.

**Files affected by this spec:**
- `AGENTS.md` — Added TERMINOLOGY SHORTHANDS section
- `README.md` — Added inline terminology note
- All spec/plan files — See `FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` Step 0 for exhaustive list

---

## 1. Overview

Add HTTPS-based transport to Communerd that sends JSON-RPC messages over standard HTTP(S) and WebSocket connections. This enables Foretias nodes to communicate through firewalls, reverse proxies, CDNs, and browser environments — mirroring Git's approach of supporting `git://`, `https://`, and `ssh://` transports for the same operations.

The HTTPS transport supports **two communication modes**:
1. **Blocking request/response** — standard JSON-RPC calls over POST or WebSocket (stamp, verify, ping, status)
2. **Callback-based push** — server-initiated delivery over persistent WebSocket sessions (history dump periods, stream ticks, mirror events)

Both modes coexist on the same connection when using WebSocket. HTTP POST is request/response only.

---

## 2. Problem

Currently, all PtP communication uses either:
- **TCP + Noise_XX** — requires open ports, fails behind NAT/firewalls, inaccessible from browsers
- **libp2p request_response** — requires libp2p stack on both ends, not browser-compatible

Calendar active mirroring (CALENDAR_ACTIVE_MIRRORING_PLAN) is the first production workload that needs HTTPS transport. It requires:
- **History Dump**: Mirror requests complete history, source pushes periods repeatedly, mirror acks
- **Stream**: Mirror requests ongoing ticks, source pushes ticks as they're produced, mirror acks periodically

Neither pattern works well with fire-and-forget HTTP POST. Both need a persistent, bidirectional channel with server-initiated pushes.

---

## 3. Design Invariants

1. **Same wire format.** JSON-RPC 2.0 over all transports. No protocol translation.
2. **Noise_XX identity binding over TLS.** The HTTPS transport layer uses TLS (or bare HTTPS for open endpoints). Application-layer identity is established via Noise_XX handshake frames exchanged over WebSocket binary frames. For open POST endpoints, identity is established through content signatures (as always).
3. **Configurable service whitelist.** Server configuration controls which JSON-RPC methods are exposed over which transport endpoints. Development mode whitelists all methods.
4. **Browser accessibility.** CORS headers, standard HTTPS ports, WebSocket upgrade — all standard web protocols.
5. **Dual communication modes.** Blocking (request → response) and callback (request → accept → push → ack) coexist on WebSocket sessions.
6. **No deprecation.** Existing TCP+Noise_XX and libp2p transports remain fully functional.

---

## 4. URL Addressing Model

All peer addresses use one of four URL schemes:

| Scheme | Description | Default Port | Encryption | Identity |
|--------|-------------|-------------|------------|----------|
| `foretias://host:port` | Custom TCP + Noise_XX | 4001 | C11 Noise_XX | Noise_XX handshake |
| `foretias+ssl://host:port` | Raw TLS + Noise_XX over streams | 4433 | TLS + C11 Noise_XX | Noise_XX over TLS stream |
| `foretias+https://host:port` | Standard HTTPS + WebSocket | 443 | TLS (transport) + Noise_XX (application, WS only) | Content sig (POST) / Noise_XX (WS) |
| libp2p multiaddr | libp2p request_response | varies | libp2p-noise | libp2p PeerId |

Port defaults are configurable via constants in `config.rs`:
```rust
pub const DEFAULT_FORETIAS_PORT: u16 = 4001;
pub const DEFAULT_FORETIAS_SSL_PORT: u16 = 4433;
pub const DEFAULT_FORETIAS_HTTPS_PORT: u16 = 443;
```

---

## 5. PeerAddr Redesign

The existing `PeerAddr` is insufficient for multi-transport addressing:

```rust
// Current
pub struct PeerAddr {
    pub json_rpc: String,
    pub peer_id: Option<libp2p::PeerId>,
    pub last_seen_ns: u64,
}
```

**Proposed redesign:**

```rust
pub struct PeerAddr {
    /// Primary URL for this peer (any supported scheme).
    pub primary_url: String,
    /// Alternative URLs for fallback (e.g., foretias+https if foretias fails).
    pub fallback_urls: Vec<String>,
    /// Optional libp2p PeerId (when available from DHT or identify).
    pub peer_id: Option<libp2p::PeerId>,
    /// Monotonic timestamp (nanoseconds) of last contact.
    pub last_seen_ns: u64,
}
```

A `PeerAddr` may have `primary_url = "foretias+https://node.example.com:443"` with `fallback_urls = ["foretias://node.example.com:4001"]`. Transport selection tries primary first, then falls back through the list.

**Migration path:** Existing code that sets `json_rpc: "host:port"` is interpreted as `foretias://host:port` during migration.

---

## 6. Transport Endpoint Routes

The HTTPS server exposes three categories of routes:

### 6.1 Discovery Endpoint

```
GET /foretias
```

Returns a JSON document describing available services:

```json
{
  "version": "0.3",
  "tbid": "<hex>",
  "tbn": "<name>",
  "tick_count": 12345,
  "transport_kinds": ["foretias", "foretias+https", "foretias+ssl"],
  "endpoints": {
    "jsonrpc": "/jsonrpc",
    "stamp": "/stamp",
    "verify": "/verify",
    "ws_session": "/foretias/ws/session"
  },
  "methods_whitelist": ["stamp", "verify", "ping", "status", "get_calendar_slice", "mirror_request", "mirror_accept", "ship_batch", "ship_ack", "stream_tick", "stream_ack", "mirror_mutual", "mirror_reconcile", "history_dump_request", "history_dump_ack", "stream_request", "stream_stop", "stream_end", "route_stamp", "integrity_check", "mirror_reconcile", "status", "get_peer_score", "collision_status", "get_latest_epoch", "verify_epoch_snapshot", "periods_query"]
}
```

### 6.2 Unified JSON-RPC Endpoint

```
POST /jsonrpc
```

Accepts any JSON-RPC 2.0 request. Same handler as existing Axum route (`process_request_from_value`). This is the **universal endpoint** — all 19+ methods are available here (subject to the whitelist).

**Open mode (no Noise_XX):** When accessed via plain HTTPS POST, identity is established through content signatures in the Foretis itself (as with all Foretias protocol). The TLS layer provides transport encryption only.

### 6.3 Convenience POST Endpoints

```
POST /stamp
```

Shortcut for stamping. Body:
```json
{ "content": "<hex>", "echo": "<string>" }
```

Response:
```json
{ "foretis": { ... } }
```

```
POST /verify
```

Shortcut for verification. Body:
```json
{ "content": "<hex>", "foretis": { ... } }
```

Response:
```json
{ "valid": true, "foretis": { ... } }
```

**These are HTTPS-only** — no Noise_XX layer, identity established through content.

### 6.4 WebSocket Session Endpoint

```
GET /foretias/ws/session
```

Upgrades to a WebSocket connection. Supports both blocking and callback communication modes.

**Noise_XX Handshake over WebSocket:** After the WebSocket is established, the first frames are a 3-message Noise_XX handshake exchanged as binary frames:

```
Client → Server: Noise_XX handshake message 1 (ephemeral public key)          [binary frame]
Server → Client: Noise_XX handshake message 2 (ephemeral + encrypted static)  [binary frame]
Client → Server: Noise_XX handshake message 3 (encrypted static)              [binary frame]
```

After `Split()` → two `CipherState` instances. All subsequent frames are encrypted Noise_XX payloads.

**Frame protocol after handshake:**

| Frame Type | Content | Direction | Mode |
|------------|---------|-----------|------|
| Text frame | JSON-RPC request | Client → Server | Blocking |
| Text frame | JSON-RPC response | Server → Client | Blocking |
| Text frame | JSON-RPC notification (push) | Server → Client | Callback |
| Text frame | JSON-RPC notification (push) | Client → Server | Callback |

Text frames carry JSON-RPC 2.0. The distinction between blocking and callback is determined by the presence of an `"id"` field:
- `"id"` present → blocking (expect response with same `"id"`)
- `"id"` absent → notification (callback push, no response expected)

For callback-based workflows (history dump, stream), the initial request IS blocking (has `"id"`), but the subsequent pushes are notifications (no `"id"`).

---

## 7. Communication Modes

### 7.1 Blocking Mode (Request/Response)

Standard JSON-RPC 2.0 with request `"id"` and matching response `"id"`.

**HTTP POST:**
```
Client → Server: POST /jsonrpc  {"jsonrpc":"2.0", "method":"stamp", "params":{...}, "id":1}
Server → Client: 200 OK         {"jsonrpc":"2.0", "result":{...}, "id":1}
```

**WebSocket (post-handshake):**
```
Client → Server: text frame {"jsonrpc":"2.0", "method":"ping", "params":{}, "id":2}
Server → Client: text frame {"jsonrpc":"2.0", "result":null, "id":2}
```

**Timeout:** Configurable per-method, default 5s for stamp/verify, 30s for calendar slice, 60s for mirror operations.

### 7.2 Callback Mode (Push)

The client initiates a blocking request. The server accepts with a response. Then the server pushes notifications (JSON-RPC without `"id"`) for the duration of the operation. The client responds with acknowledgments (notifications or requests with `"id"`, depending on the protocol).

**History Dump (CALENDAR_ACTIVE_MIRRORING Phase 2):**

```
Mirror → Source (blocking, id=1):
  {"jsonrpc":"2.0", "method":"history_dump_request",
   "params":{"chronomatter_tbid":"...", "calendar_tbid":"...", "up_to_tick":1500},
   "id":1}

Source → Mirror (response, id=1):
  {"jsonrpc":"2.0",
   "result":{"period_count":2, "total_ticks":1500},
   "id":1}

Source → Mirror (push, no id):
  {"jsonrpc":"2.0", "method":"history_dump_period",
   "params":{"period":{...}, "tick_records":[...]}}

Source → Mirror (push, no id):
  {"jsonrpc":"2.0", "method":"history_dump_period",
   "params":{"period":{...}, "tick_records":[...]}}

Source → Mirror (push, no id):
  {"jsonrpc":"2.0", "method":"history_dump_end",
   "params":{"combined_checksum":"..."}}

Mirror → Source (ack, no id):
  {"jsonrpc":"2.0", "method":"history_dump_ack",
   "params":{"status":"ok"}}
```

**Stream (CALENDAR_ACTIVE_MIRRORING Phase 3):**

```
Mirror → Source (blocking, id=2):
  {"jsonrpc":"2.0", "method":"stream_request",
   "params":{"chronomatter_tbid":"...", "calendar_tbid":"...", "from_tick":1501},
   "id":2}

Source → Mirror (response, id=2):
  {"jsonrpc":"2.0", "result":{"from_tick":1501}, "id":2}

Source → Mirror (push, every new tick, no id):
  {"jsonrpc":"2.0", "method":"stream_tick",
   "params":{"tick_record":{...}}}

Mirror → Source (ack every N ticks, no id):
  {"jsonrpc":"2.0", "method":"stream_ack",
   "params":{"tick_number":1550}}
```

**Key property:** The WebSocket session remains open for the entire duration. For stream mode, this can be minutes, hours, or indefinitely until either side sends a stop signal.

### 7.3 Mode Selection

| Operation | Mode | Transport |
|-----------|------|-----------|
| stamp | Blocking | HTTP POST or WebSocket |
| verify | Blocking | HTTP POST or WebSocket |
| ping | Blocking | HTTP POST or WebSocket |
| status | Blocking | HTTP POST or WebSocket |
| get_calendar_slice | Blocking | HTTP POST or WebSocket |
| route_stamp | Blocking | HTTP POST or WebSocket |
| history_dump_request → history_dump_ack | Callback | WebSocket only |
| stream_request → stream_stop/end | Callback | WebSocket only |
| mirror_request | Blocking | HTTP POST or WebSocket |
| mirror_accept / ship_batch / ship_ack (legacy) | Blocking | HTTP POST or WebSocket |
| mirror_mutual | Blocking | HTTP POST or WebSocket |
| mirror_reconcile | Blocking | HTTP POST or WebSocket |
| periods_query | Blocking | HTTP POST or WebSocket |

---

## 8. HTTPS Transport Implementation

### 8.1 `HttpsTransport: PeerTransport`

Implements the `PeerTransport` trait for `foretias+https://` URLs:

```rust
pub struct HttpsTransport {
    /// Timeout configuration per method category.
    timeouts: TransportTimeouts,
    /// TLS configuration (cert verification, etc.)
    tls_config: Option<reqwest::TLSVersion>,
}

#[async_trait]
impl PeerTransport for HttpsTransport {
    async fn stamp(&self, peer: &PeerAddr, content_hex: &str, echo: &str) -> Result<serde_json::Value, TransportError>;
    async fn route_stamp(&self, peer: &PeerAddr, target_tbid: &str, content_hex: &str, echo: &str) -> Result<serde_json::Value, TransportError>;
    async fn get_calendar_slice(&self, peer: &PeerAddr, tick_start: u64, count: u64) -> Result<Vec<TickRecord>, TransportError>;
    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
}
```

**Blocking methods** use HTTP POST to `https://{host}:{port}/jsonrpc`.

### 8.2 `HttpsWebSocketTransport: PeerTransport`

Extends `PeerTransport` with callback-capable methods for the WebSocket session:

```rust
pub struct HttpsWebSocketTransport {
    inner: HttpsTransport,
    /// Noise_XX static keypair for WebSocket handshake.
    noise_static_priv: [u8; 32],
    noise_static_pub: [u8; 32],
}

impl HttpsWebSocketTransport {
    /// Start a history dump session. Returns a HistoryDumpSession for receiving pushed periods.
    async fn start_history_dump(
        &self,
        peer: &PeerAddr,
        chronomatter_tbid: &[u8; 16],
        calendar_tbid: &[u8; 16],
        up_to_tick: u64,
    ) -> Result<HistoryDumpSession, TransportError>;

    /// Start a stream session. Returns a StreamSession for receiving pushed ticks.
    async fn start_stream(
        &self,
        peer: &PeerAddr,
        chronomatter_tbid: &[u8; 16],
        calendar_tbid: &[u8; 16],
        from_tick: u64,
    ) -> Result<StreamSession, TransportError>;
}

/// Ongoing history dump session. Implements a receive stream for period pushes.
pub struct HistoryDumpSession {
    ws: WebSocketStream<TlsStream<TcpStream>>,
    cipher_rx: noise::CipherState,
    cipher_tx: noise::CipherState,
}

impl HistoryDumpSession {
    /// Receive the next period push from the source.
    async fn recv_period(&mut self) -> Result<HistoryDumpFrame, TransportError>;

    /// Send acknowledgment when dump is complete.
    async fn send_ack(&mut self, status: &str) -> Result<(), TransportError>;
}

pub enum HistoryDumpFrame {
    Period { period: Period, tick_records: Vec<TickRecord> },
    End { combined_checksum: String },
}

/// Ongoing stream session. Implements a receive stream for tick pushes.
pub struct StreamSession {
    ws: WebSocketStream<TlsStream<TcpStream>>,
    cipher_rx: noise::CipherState,
    cipher_tx: noise::CipherState,
}

impl StreamSession {
    /// Receive the next tick push from the source.
    async fn recv_tick(&mut self) -> Result<TickRecord, TransportError>;

    /// Send acknowledgment for received ticks.
    async fn send_ack(&mut self, tick_number: u64) -> Result<(), TransportError>;

    /// Stop the stream (mirror-initiated).
    async fn stop(&mut self, reason: &str) -> Result<(), TransportError>;
}
```

### 8.3 Server-Side WebSocket Handler

The server must handle incoming WebSocket connections from mirrors:

```rust
impl TimeFamilyServer {
    /// Handle an incoming WebSocket session.
    /// After Noise_XX handshake, dispatch incoming JSON-RPC frames to handlers.
    /// Supports both blocking (request+id → response+id) and callback (request without id → push without id).
    async fn handle_websocket_session(
        &self,
        ws_stream: WebSocketStream<TlsStream<TcpStream>>,
    ) -> Result<(), NodeError>;
}
```

When the server receives a `history_dump_request` with `"id"` on WebSocket:
1. Respond with `history_dump_start` (matching `"id"`)
2. Push periods as notifications (no `"id"`)
3. Push `history_dump_end` (no `"id"`)
4. Wait for `history_dump_ack` from mirror

When the server receives a `stream_request` with `"id"`:
1. Respond with `stream_start` (matching `"id"`)
2. Spawn a background task that pushes `stream_tick` notifications as ticks are produced
3. Wait for `stream_ack` or `stream_stop` from mirror

---

## 9. Transport Kind Enum Expansion

```rust
pub enum TransportKind {
    /// Direct TCP JSON-RPC with C11 Noise_XX encryption.
    TcpNoise,
    /// Raw TLS stream with C11 Noise_XX over streams.
    TlsNoise,
    /// HTTPS (HTTP POST + WebSocket) with Noise_XX over WebSocket.
    HttpsNoise,
    /// libp2p multiplexed streams with libp2p-noise encryption (C11 deviation).
    Libp2p,
}
```

---

## 10. Transport Selection and Fallback Chain

When `Communerd` needs to contact a peer, it tries transports in priority order:

1. **libp2p direct** — if peer has `peer_id` AND libp2p swarm is active AND peer is connected
2. **foretias+https (WebSocket)** — if `primary_url` or `fallback_urls` contains `foretias+https://` AND operation requires callback mode
3. **foretias (TCP+Noise_XX)** — if `primary_url` or `fallback_urls` contains `foretias://`
4. **foretias+https (HTTP POST)** — if `primary_url` or `fallback_urls` contains `foretias+https://` AND operation is blocking-only
5. **foretias+ssl** — if `primary_url` or `fallback_urls` contains `foretias+ssl://`

For blocking operations (stamp, verify, ping), HTTP POST is sufficient. For callback operations (history dump, stream), WebSocket is required.

---

## 11. DHT Registration and Capability Discovery

When a peer registers in the DHT, it announces its capabilities:

```json
{
  "tbid": "<hex>",
  "endpoints": {
    "foretias": "host:4001",
    "foretias+https": "host:443",
    "foretias+ssl": "host:4433"
  },
  "supports_websocket": true,
  "methods_whitelist": ["stamp", "verify", "history_dump_request", "stream_request", ...]
}
```

A mirror peer discovering a source via DHT checks `supports_websocket` and `methods_whitelist` before choosing transport. If the source supports WebSocket and the mirror needs callback mode, it uses `foretias+https://` with WebSocket.

---

## 12. Browser Accessibility

The `foretias+https://` transport must work from a browser:

- **CORS headers** on all routes: `Access-Control-Allow-Origin: *` (configurable)
- **CORS preflight** on POST routes: respond to OPTIONS with appropriate headers
- **WebSocket upgrade** at `/foretias/ws/session` — standard `Sec-WebSocket-*` headers
- **No custom protocols** in URLs that browsers can't handle — browser clients use standard `https://` URLs

Browser client API:
```javascript
// Blocking (stamp)
fetch('https://node.example.com/stamp', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ content: '...', echo: '...' })
}).then(r => r.json());

// Callback (stream) — requires WebSocket
const ws = new WebSocket('wss://node.example.com/foretias/ws/session');
// After Noise_XX handshake (binary frames), send JSON-RPC
```

---

## 13. Security Considerations

| Concern | Mitigation |
|---------|-----------|
| Transport encryption | TLS 1.2+ for HTTPS/SSL endpoints. C11 Noise_XX for TCP and WebSocket. |
| Application-layer identity | Noise_XX handshake binds identities over WebSocket. Content signatures for open POST. |
| Service whitelist | Server config controls which methods are exposed over which endpoints. Dev mode = all. |
| Replay protection | Noise_XX cipher state provides per-message authentication. No stateless replay over encrypted channels. |
| DoS on WebSocket | Configurable session timeout, max frames per second, max payload size. |
| Browser CORS | Configurable `Access-Control-Allow-Origin` — restrict to known domains in production. |

---

## 14. Configuration

New config section in `config.rs`:

```rust
pub struct HttpsTransportConfig {
    /// HTTPS listen address (e.g., "0.0.0.0:443").
    pub https_addr: String,
    /// TLS certificate path (for HTTPS server).
    pub tls_cert_path: Option<String>,
    /// TLS key path (for HTTPS server).
    pub tls_key_path: Option<String>,
    /// Methods exposed over open HTTP POST endpoints (no Noise_XX).
    pub post_methods_whitelist: Vec<String>,
    /// Methods exposed over WebSocket (with Noise_XX).
    pub ws_methods_whitelist: Vec<String>,
    /// Whitelist all methods (development mode).
    pub whitelist_all: bool,
    /// CORS allowed origins (empty = allow all).
    pub cors_origins: Vec<String>,
    /// WebSocket session timeout (seconds). 0 = no timeout.
    pub ws_session_timeout_secs: u64,
    /// Max payload size per WebSocket frame (bytes).
    pub ws_max_frame_bytes: usize,
}
```

---

## 15. Testing

### 15.1 Unit Tests
- `HttpsTransport` sends JSON-RPC via POST, receives response
- `HttpsWebSocketTransport` establishes Noise_XX handshake over WebSocket
- `HistoryDumpSession` receives period pushes, sends ack
- `StreamSession` receives tick pushes, sends ack
- WebSocket server handles concurrent blocking + callback on same session

### 15.2 Integration Tests
- Two servers, mirror requests history dump over HTTPS WebSocket, source pushes periods
- Two servers, mirror starts stream over HTTPS WebSocket, source pushes ticks as produced
- Fallback: HTTPS POST fails → TCP+Noise_XX retry
- Browser fetch to `/stamp` returns valid Foretis

### 15.3 Existing Tests
- All `JsonRpcTransport` tests remain unchanged
- All `Libp2pTransport` tests remain unchanged
- C11 tests remain unchanged

---

## 16. Out of Scope

- Replacing any existing transport (all are retained)
- `foretias+ssl://` implementation (separate spec, lower priority)
- Client-language binding changes (Python, Java)
- NAT hole punching or relay support
- mTLS or certificate-based authentication (content signatures suffice)
