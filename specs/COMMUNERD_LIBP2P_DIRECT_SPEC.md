# Communerd libp2p Direct Transport — Specification

**Prefix:** `COMMUNERD_LIBP2P_DIRECT`
**Pairs with:** `COMMUNERD_LIBP2P_DIRECT_PLAN.md`
**Status:** DEPRECATED — Phases 1–5 complete. Remaining work (libp2p unit tests; AGENTS.md transport
            table — already done) is tracked in `COMBINED_GROUP4_SPEC.md` Stream 4a. Do not update this file.

## Overview

Add a `libp2p request_response` transport to Communerd that sends JSON-RPC messages over **existing multiplexed libp2p connections** (yamux/mplex) to already-connected peers. The existing custom Noise_XX TCP transport (`JsonRpcTransport`) is retained for backward compatibility but libp2p direct becomes the preferred transport for peers reachable via libp2p.

## Problem

Currently, Communerd uses two independent communication paths:

| Transport | Encryption | Multiplexing | Purpose |
|-----------|-----------|-------------|---------|
| Noise_XX TCP (custom) | C11 `Noise_XX_25519_ChaChaPoly_SHA256` per connection | None — fresh TCP per call | Stamp, verify, calendar replication, ping |
| libp2p swarm | libp2p-noise (X25519) at transport layer | yamux multiplex | DHT, GossipSub (probity, heartbeat), identify, ping |

The libp2p swarm is already connected to peers, maintains keep-alive connections, and handles NAT traversal — but RPC calls (stamp, verify, calendar slice) open separate TCP connections with their own Noise handshake. This doubles connection overhead and misses NAT traversal entirely for RPC traffic.

## Design Invariants

1. **No deprecation.** `JsonRpcTransport` remains fully functional and tested.
2. **Multiplexed.** RPC over libp2p uses existing yamux/mplex streams — no new TCP sockets.
3. **Same wire format.** JSON-RPC 2.0 over both transports. No protocol translation.
4. **Fallback.** If libp2p direct fails for a peer, fall back to `JsonRpcTransport` (if `json_rpc` address is known).
5. **C11 deviation documented.** libp2p-noise is used for direct transport encryption, not C11 Noise_XX. This is acknowledged in logs and documentation.

## Transport Layer Comparison

| Property | Noise_XX TCP (custom) | libp2p Direct (new) |
|----------|----------------------|---------------------|
| Encryption | C11 Noise_XX + ChaCha20-Poly1305 | libp2p-noise (X25519 DH + ChaCha20-Poly1305) |
| Connection | Fresh TCP per RPC call | Multiplexed yamux stream over persistent TCP |
| Handshake | 3-message Noise_XX per connection | Done once at connection establishment |
| NAT traversal | None (requires reachable IP:port) | libp2p multiaddr (handles NAT via existing connection) |
| C11 compliance | ✅ C11 does all encryption | ❌ libp2p crate handles transport encryption |
| Protocol isolation | N/A | Namespace-scoped via stream protocol |

## Architecture

### New Module: `libp2p_transport.rs`

```
communerd/
├── transport.rs            # PeerTransport trait, TransportKind, select_transport
├── json_rpc_transport.rs   # Existing — unchanged
├── libp2p_transport.rs     # NEW — libp2p request_response transport
├── mod.rs                  # Communerd — holds both transports, selects at call time
├── peer_pool.rs            # PeerPool — liveness pings via active transport
└── p2p/
    ├── behaviour.rs        # ADD request_response behaviour
    ├── events.rs           # ADD StreamOpened/StreamClosed events if needed
    ├── swarm.rs            # ADD swarm commands for direct RPC
    └── ...
```

### PeerTransport Trait — Unchanged

The existing `PeerTransport` trait stays the same:

```rust
pub trait PeerTransport: Send + Sync {
    async fn stamp(&self, peer: &PeerAddr, content_hex: &str, echo: &str) -> Result<serde_json::Value, TransportError>;
    async fn route_stamp(&self, peer: &PeerAddr, target_tbid: &str, content_hex: &str, echo: &str) -> Result<serde_json::Value, TransportError>;
    async fn get_calendar_slice(&self, peer: &PeerAddr, tick_start: u64, count: u64) -> Result<Vec<TickRecord>, TransportError>;
    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
}
```

### New: `Libp2pTransport`

Holds a reference to the swarm's command channel and sends JSON-RPC via `request_response`. Receives replies on the same stream.

```rust
pub struct Libp2pTransport {
    cmd_tx: Arc<OnceLock<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>>,
    local_peer_id: Arc<OnceLock<libp2p::PeerId>>,
    timeout_secs: u64,
    namespace: Arc<std::sync::Mutex<String>>,
}
```

### Transport Selection Logic

`Communerd` holds **both** transports. At call time, it selects based on peer reachability:

```rust
enum ActiveTransport {
    JsonRpc(Arc<JsonRpcTransport>),
    Libp2p(Arc<Libp2pTransport>),
}

impl Communerd {
    fn select_transport(&self, peer: &PeerAddr) -> ActiveTransport {
        // If peer has a PeerId AND we have an active libp2p swarm,
        // prefer libp2p direct (multiplexed over existing connection).
        if peer.peer_id.is_some()
            && self.p2p_cmd_tx.get().is_some()
            && !peer.json_rpc.is_empty()
        {
            // Try libp2p first, fall back to JSON-RPC on failure
            ActiveTransport::Libp2p(self.libp2p_transport.clone())
        } else if !peer.json_rpc.is_empty() {
            ActiveTransport::JsonRpc(self.json_rpc_transport.clone())
        } else {
            // Peer only has PeerId, no RPC address — must use libp2p
            ActiveTransport::Libp2p(self.libp2p_transport.clone())
        }
    }
}
```

### Fallback Chain

When `Libp2pTransport` fails (peer not connected, stream timeout, dial fails):

1. If `PeerAddr.json_rpc` is non-empty → retry via `JsonRpcTransport`
2. If `PeerAddr.json_rpc` is empty → return error (no fallback possible)

This ensures peers discovered via DHT that have a `json_rpc` address in their registration record can still be reached even if the libp2p connection drops.

## Wire Protocol

### Stream Protocol

```
/foretias/{namespace}/rpc/1.0.0
```

Namespace-scoped to prevent cross-namespace interference. Matches the DHT protocol naming convention (`/foretias/kad/{namespace}/1.0.0`).

### Framing

libp2p `request_response` handles framing. Payload is raw JSON-RPC 2.0 JSON bytes (same as `JsonRpcTransport`). No length prefix needed — libp2p handles stream framing.

### Request/Response

Identical to `JsonRpcTransport`:

```json
{"jsonrpc":"2.0","method":"stamp","params":{"content":"...","echo":"..."},"id":1}
```

```json
{"jsonrpc":"2.0","result":{"tick_number":1,"content_hash":"...","signature":"...","tbid":"...","echo":"...","tbn":"...","time_being_reference_time":...},"id":1}
```

## ForetiasBehaviour Changes

Add `request_response` to the composite behaviour:

```rust
#[derive(NetworkBehaviour)]
pub struct ForetiasBehaviour {
    pub identify:          identify::Behaviour,
    pub ping:              ping::Behaviour,
    pub kad:               kad::Behaviour<kad::store::MemoryStore>,
    pub gossip:            gossipsub::Behaviour,
    pub request_response:  request_response::Behaviour<ForetiasRpcProtocol>,
}
```

`ForetiasRpcProtocol` implements `request_response::Protocol` with the namespace-scoped protocol string and JSON serialization.

## SwarmCommand Extensions

```rust
pub enum SwarmCommand {
    // ... existing ...
    /// Send a JSON-RPC request over libp2p request_response to a peer.
    /// Returns response via oneshot channel.
    RequestResponse {
        peer_id: PeerId,
        request: serde_json::Value,
        reply: tokio::sync::oneshot::Sender<Result<serde_json::Value, TransportError>>,
    },
}
```

## Logging — C11 Deviation

Every log message from `Libp2pTransport` includes a transport tag:

```
transport=libp2p-direct encryption=libp2p-noise (not C11) peer=...
```

At startup:

```
WARN component=communerd "libp2p direct transport uses libp2p-noise for encryption, NOT C11 Noise_XX. C11 deviation acknowledged."
```

In `TransportKind`:

```rust
pub enum TransportKind {
    /// Direct TCP JSON-RPC with C11 Noise_XX encryption.
    DirectJsonRpc,
    /// libp2p multiplexed streams with libp2p-noise encryption (C11 deviation).
    Libp2p,
}
```

## Behaviour Changes in Communerd::new()

```rust
impl Communerd {
    pub fn new(config: NodeConfig) -> Self {
        let json_rpc_transport: Arc<dyn PeerTransport> = Arc::new(JsonRpcTransport::new(...));
        let libp2p_transport: Arc<Libp2pTransport> = Arc::new(Libp2pTransport::new(...));
        // ... rest of construction
    }
}
```

## Behaviour Changes in Communerd::stamp_peer()

```rust
pub async fn stamp_peer(&self, peer: &PeerAddr, content_hex: &str, echo: &str)
    -> Result<Foretis, TransportError>
{
    // Try selected transport (libp2p preferred if peer has PeerId)
    let result = self.send_via_transport(peer, |t| t.stamp(peer, content_hex, echo)).await;
    // ... deserialize and return
}
```

## PeerPool Liveness

`PeerPool::start_liveness_pings` currently uses `self.transport.ping()`. It continues to use the selected transport — no change needed. The ping method works over both transports.

## Security Considerations

- **libp2p-noise** uses X25519 for DH and ChaCha20-Poly1305 for AEAD — same primitives as our C11 Noise_XX, but different protocol (libp2p's Noise implementation vs our custom `Noise_XX_static`).
- **Stream authentication** is guaranteed by libp2p's transport layer — only authenticated peers can open streams.
- **Namespace isolation** prevents cross-namespace RPC injection.
- **C11 deviation** is an acknowledged tradeoff: convenience and multiplexing in exchange for using libp2p's transport encryption instead of C11.

## Testing

### Unit Tests
- `Libp2pTransport` sends and receives JSON-RPC over request_response
- Fallback: libp2p failure → JSON-RPC retry
- Peer without `json_rpc` but with `PeerId` → libp2p only

### Integration Tests
- Two swarms connect → RPC over multiplexed stream succeeds
- Swarm A stamps via Swarm B over libp2p direct (end-to-end)
- Fallback: disconnect libp2p → RPC falls back to JSON-RPC

### Existing Tests
- All `JsonRpcTransport` tests remain unchanged
- C11 Noise_XX tests remain unchanged

## Out of Scope

- Replacing `JsonRpcTransport` entirely (retained for compatibility)
- Adding request_response to C11 core
- NAT hole punching (libp2p already handles this via relay, not our concern)
- TLS or additional transport encryption layers
