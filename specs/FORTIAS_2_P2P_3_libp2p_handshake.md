# Fortias — P2P Sub-Spec 2: libp2p Handshake (v0.3)

**Milestone tag:** `v0.3-libp2p-handshake`
**Prereq:** `v0.2-direct-p2p-mutual-attestation` must be tagged.
**Next:** `FORTIAS_2_P2P_4_dht_discovery.md` (v0.4 — adds Kademlia and hybrid transport selection).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.2-direct-p2p-mutual-attestation` is tagged.
2. Read `FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md` end to end — the `PeerMessenger` trait (§5) and `Communerd` component are the seam this sub-spec extends.
3. Read this document end to end.
4. Confirm the identity-bridge approach (§4.4) is understood before touching `CryptoServer`.

---

## 1. GOAL

Add a **libp2p swarm** as a second transport inside the running `TimeFamily` process. The swarm uses **Noise XX** for the handshake and **yamux** for multiplexing over TCP. `identify` and `ping` behaviours run on every connection.

v0.2's direct JSON-RPC transport continues to operate unchanged. libp2p is a parallel, additional transport at this stage — no application traffic moves over it yet. The value delivered is: two `TimeFamily` processes can find and authenticate each other via libp2p Noise using identity material derived from their `CryptoServer`.

(@human — Noise XX is the right handshake for libp2p peers that have not
previously exchanged keys: both sides transmit their static public keys
during the handshake and each can verify the other's peer ID afterward.
This is the standard libp2p pattern. The v0.2 direct JSON-RPC channel
stays plain TCP — Noise only applies to libp2p connections, per the
design decision made during brainstorming.)

---

## 2. ACCEPTANCE DEMO

```bash
# Both servers already running with v0.2 mutual attestation
# Add p2p flags to each:

$ fortias serve --addr 127.0.0.1:4001 --peer 127.0.0.1:4002 \
    --p2p-listen /ip4/127.0.0.1/tcp/4101 \
    --chronon-ns 20000000000

$ fortias serve --addr 127.0.0.1:4002 --peer 127.0.0.1:4001 \
    --p2p-listen /ip4/127.0.0.1/tcp/4102 \
    --p2p-dial /ip4/127.0.0.1/tcp/4101/p2p/<PeerID-A>

# Expected logs on B within 10 s:
INFO swarm: connection established peer=<PeerID-A>
INFO identify: agent="fortias/0.3.0" peer=<PeerID-A>
INFO ping: rtt=312µs peer=<PeerID-A>
```

v0.2 mutual-attestation logs must still appear alongside the libp2p logs — both transports run concurrently.

---

## 3. WHAT v0.2 PROVIDES THAT THIS SPEC EXTENDS

| Symbol | Extension |
|---|---|
| `PeerMessenger` trait | v0.3 adds `Libp2pTransport` impl (unused for mutual-attest yet; used in v0.4) |
| `PeerAddr` struct | Gains `peer_id: Option<libp2p::PeerId>` field (may already be there per v0.2 spec) |
| `NetworkEvent` enum | New enum; swarm events surface here to `Communerd` |
| `Communerd` | Gains `swarm_handle: Option<SwarmHandle>` field for swarm event loop |

---

## 4. ARCHITECTURE

### 4.1 New files

```
fortias-node/src/
└── network/
    ├── mod.rs               re-exports; pub mod network; added to lib.rs
    ├── swarm.rs             build_and_spawn_swarm(), swarm event loop
    ├── behaviour.rs         FortiasBehaviour derive
    ├── identity_bridge.rs   CryptoServer → libp2p::Keypair
    └── events.rs            NetworkEvent enum
```

### 4.2 Cargo.toml additions

```toml
libp2p = { version = "0.55", default-features = false, features = [
    "tokio", "tcp", "noise", "yamux", "identify", "ping", "macros", "ed25519",
]}
multiaddr = "0.18"
```

### 4.3 `FortiasBehaviour`

```rust
// src/network/behaviour.rs
use libp2p::{identify, ping, swarm::NetworkBehaviour};

#[derive(NetworkBehaviour)]
pub struct FortiasBehaviour {
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
}

impl FortiasBehaviour {
    pub fn new(local_pub: libp2p::identity::PublicKey) -> Self {
        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("fortias/0.3.0".into(), local_pub.clone())
                    .with_agent_version(format!("fortias/{}", env!("CARGO_PKG_VERSION")))
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
        }
    }
}
```

(@human — protocol string `"fortias/0.3.0"` identifies the libp2p
protocol version, distinct from the cargo package version. Bump it only
on wire-incompatible changes, not on every patch.)

### 4.4 Identity bridge — `CryptoServer` → `libp2p::Keypair`

libp2p requires a `Keypair` to sign Noise handshake messages. For the v0.3 software backend, extract the Ed25519 seed from `SoftwareCryptoServer` directly. For future custom-plugin backends (v0.9+), a signing trampoline will be needed; that path returns `NodeError::Unsupported` in v0.3.

```rust
// src/network/identity_bridge.rs
pub fn libp2p_keypair_from(server: &Arc<dyn CryptoServer>) -> Result<Keypair, NodeError> {
    let sw = server.as_any().downcast_ref::<SoftwareCryptoServer>()
        .ok_or(NodeError::Unsupported("libp2p identity requires software CryptoServer in v0.3"))?;
    let seed: [u8; 32] = sw.export_ed25519_seed()?;
    let secret = ed25519::SecretKey::try_from_bytes(seed)
        .map_err(|e| NodeError::Internal(format!("{e}")))?;
    Ok(Keypair::from(ed25519::Keypair::from(secret)))
}
```

`CryptoServer` gains `fn as_any(&self) -> &dyn std::any::Any` (default returns `&()`; `SoftwareCryptoServer` overrides). `SoftwareCryptoServer` gains `pub(crate) fn export_ed25519_seed(&self) -> Result<[u8; 32], NodeError>`.

(@human — `export_ed25519_seed` is `pub(crate)` intentionally. It is the
only place in the codebase where a key material byte crosses the
CryptoServer boundary. Once v0.9+ enclave plugins exist this path
simply does not compile for those backends — the downcast fails and the
Unsupported error propagates at startup.)

### 4.5 Swarm construction and spawn

```rust
// src/network/swarm.rs
pub struct SwarmHandle {
    pub local_peer_id: libp2p::PeerId,
    pub events:        mpsc::UnboundedReceiver<NetworkEvent>,
    pub task:          tokio::task::JoinHandle<()>,
}

pub async fn build_and_spawn_swarm(
    server:  Arc<dyn CryptoServer>,
    listen:  Multiaddr,
    dials:   Vec<Multiaddr>,
) -> Result<SwarmHandle, NodeError> {
    let keypair = libp2p_keypair_from(&server)?;
    let local_peer_id = keypair.public().to_peer_id();

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default().nodelay(true), noise::Config::new, yamux::Config::default)
        .map_err(|e| NodeError::Internal(format!("{e}")))?
        .with_behaviour(|key| FortiasBehaviour::new(key.public()))
        .map_err(|e| NodeError::Internal(format!("{e}")))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    swarm.listen_on(listen)?;
    for ma in dials { let _ = swarm.dial(ma); }

    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(swarm_loop(swarm, tx));
    Ok(SwarmHandle { local_peer_id, events: rx, task })
}
```

The swarm event loop emits `NetworkEvent::Connected`, `NetworkEvent::Disconnected`, and `NetworkEvent::Identified` into the channel. `Communerd` consumes these to build its peer table — useful in v0.4 when DHT populates peers dynamically.

### 4.6 `NetworkEvent`

```rust
// src/network/events.rs
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Connected    { peer_id: PeerId },
    Disconnected { peer_id: PeerId },
    Identified   { peer_id: PeerId, info: identify::Info },
    // Extended in v0.4: DhtPeerDiscovered, in v0.6: GossipMessage
}
```

### 4.7 Wiring into `TimeFamily`

```rust
// src/time_family/mod.rs  (additions)
impl Communerd {
    pub async fn enable_p2p(
        self: &Arc<Self>,
        listen: Multiaddr,
        dials:  Vec<Multiaddr>,
    ) -> Result<(), NodeError> {
        let handle = build_and_spawn_swarm(
            Arc::clone(&self.crypto), listen, dials
        ).await?;
        self.swarm_handle = Some(handle);
        Ok(())
    }
}
```

If `--p2p-listen` is absent, `enable_p2p` is never called and libp2p is completely inactive.

---

## 5. CONFIGURATION

```json
{
  "network": {
    "p2p_listen":                  "/ip4/0.0.0.0/tcp/4101",
    "p2p_dial":                    ["/ip4/127.0.0.1/tcp/4102/p2p/12D3KooW..."],
    "idle_connection_timeout_secs": 60
  }
}
```

CLI: `--p2p-listen <multiaddr>`, `--p2p-dial <multiaddr>` (repeatable).

---

## 6. ERROR HANDLING

| Condition | Behaviour |
|---|---|
| `p2p-listen` malformed | Exit at startup with descriptive error |
| Bind fails (port in use) | Exit at startup |
| Dial fails | Log WARN; continue — reconnection is the swarm's job |
| Non-software CryptoServer downcast fails | `NodeError::Unsupported`; process exits with message |
| Three consecutive ping failures | Connection closed; `Disconnected` event emitted |

---

## 7. TEST PLAN

- `identity_bridge_roundtrip` — software CryptoServer → libp2p Keypair → sign/verify roundtrip.
- `swarm_listen_only` — build swarm, no dials; `NewListenAddr` event arrives within 1 s.
- `two_swarms_connect_and_identify` — two in-process swarms on ephemeral ports; both see `Connected` + `Identified` within 5 s.
- v0.2 mutual-attest tests still pass (libp2p flags absent → v0.2 behaviour unchanged).

Demo script `scripts/demo_v0_3_libp2p.sh` adds `--p2p-listen`/`--p2p-dial` to the v0.2 demo and greps for `"connection established"` in the logs.

---

## 8. NON-GOALS FOR v0.3

- ❌ DHT / Kademlia — v0.4.
- ❌ GossipSub — v0.6.
- ❌ Using libp2p for mutual-attest traffic — still direct JSON-RPC; v0.4 adds hybrid selection.
- ❌ AutoNAT, Relay, Hole-punching — deferred; demo runs on LAN/localhost.
- ❌ QUIC transport — TCP only for now.

---

## 9. MILESTONE CHECKLIST

```
[ ] v0.3.1  Add libp2p to Cargo.toml; cargo build succeeds.
[ ] v0.3.2  identity_bridge.rs + CryptoServer::as_any + export_ed25519_seed.
[ ] v0.3.3  FortiasBehaviour, swarm.rs, events.rs.
[ ] v0.3.4  Wire enable_p2p into TimeFamily; CLI flags parse.
[ ] v0.3.5  Integration test two_swarms_connect_and_identify passes.
[ ] v0.3.6  Demo script passes.
[ ] v0.3.7  All v0.1 and v0.2 tests pass.
[ ] v0.3.8  TAG: v0.3-libp2p-handshake.
```

---
# END OF SUB-SPEC 2 (v0.3)
