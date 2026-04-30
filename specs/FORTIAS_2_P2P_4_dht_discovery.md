# Fortias — P2P Sub-Spec 3: DHT Peer Discovery & Hybrid Transport (v0.4)

**Milestone tag:** `v0.4-dht-discovery`
**Prereq:** `v0.3-libp2p-handshake` must be tagged.
**Next:** `FORTIAS_2_P2P_5_hardening.md` (v0.5 — threat model, observability,
encrypted persistence, formal-verification PoC).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.3-libp2p-handshake` is tagged.
2. Re-read `FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md` §5
   (`PeerMessenger` trait, `PeerAddr`, `Communerd`) — this sub-spec
   adds a `DhtPeerSource` that slots into the existing scheduler without
   modifying it.
3. Re-read `FORTIAS_2_P2P_3_libp2p_handshake.md` §4.5–§4.7
   (`SwarmHandle`, `NetworkEvent`, swarm wiring) — this sub-spec extends
   `FortiasBehaviour` with Kademlia.
4. Read this document end to end before touching any code.

---

## 1. GOAL

Replace the static peer list from v0.2 with **Kademlia DHT peer discovery**:
nodes announce themselves as willing cross-attest partners and find other
willing nodes without prior configuration.

In parallel, introduce **hybrid transport selection**: direct JSON-RPC
("close friends" — peers already known and trusted via prior attestation
history) for low-latency mutual attestation; libp2p streams for general
network traffic (gossip, probity — used from v0.6 onward).

(@human — "close friends" is not a cryptographic concept here; it just
means a peer whose `PeerAddr.json_rpc` address is known. In v0.4 that
knowledge comes from the identify event: once a peer's agent info arrives
via libp2p, we can read its announced JSON-RPC address and add it to the
direct-call pool. Peers that never announce a JSON-RPC address are reached
only via libp2p streams. The split matters for latency: a direct TCP
JSON-RPC call is faster than routing a request through the libp2p
request-response layer.)

Three-node acceptance test: node A and node C both boot with only node B
as a bootstrap peer; A discovers C via B's DHT, dials it, and successfully
mutual-attests — all without A knowing C's address at startup.

---

## 2. WHAT v0.3 PROVIDES THAT THIS SPEC EXTENDS

| Symbol | Extension |
|---|---|
| `FortiasBehaviour` | Gains `kad: kad::Behaviour<kad::store::MemoryStore>` |
| `NetworkEvent` | Gains `DhtPeerDiscovered { peer_id, addresses }` |
| `PeerSource` trait (v0.2) | v0.4 ships `DhtPeerSource` impl |
| `PeerAddr` struct (v0.2) | Gains `json_rpc: Option<String>` (was `String`; now optional since not all libp2p peers expose JSON-RPC) |
| `identify::Info` (v0.3) | Parsed for a custom `fortias-jsonrpc-addr` user agent field (see §4.3) |

---

## 3. ACCEPTANCE DEMO

```bash
# Node B — bootstrap only (no cross-attest peers configured)
$ fortias serve --addr 127.0.0.1:4002 \
    --p2p-listen /ip4/127.0.0.1/tcp/4102 \
    --chronon-ns 20000000000

# Node A — knows only B
$ fortias serve --addr 127.0.0.1:4001 \
    --p2p-listen /ip4/127.0.0.1/tcp/4101 \
    --dht-bootstrap /ip4/127.0.0.1/tcp/4102/p2p/<PeerID-B> \
    --chronon-ns 20000000000

# Node C — knows only B
$ fortias serve --addr 127.0.0.1:4003 \
    --p2p-listen /ip4/127.0.0.1/tcp/4103 \
    --dht-bootstrap /ip4/127.0.0.1/tcp/4102/p2p/<PeerID-B> \
    --chronon-ns 20000000000
```

Expected within 60 s:
- A's swarm logs: `dht: discovered peer=<PeerID-C>`
- A's mutual-attest logs: `stored external attestation from=<C tbid>`
- C's calendar: external attestation from A.

Static `--peer` flags are absent — A and C find each other purely via DHT.

(@human — the 60 s window is generous; on a LAN the discovery typically
completes in under 5 s. In CI use a tighter timeout of 15 s.)

---

## 4. ARCHITECTURE

### 4.1 Kademlia in `FortiasBehaviour`

```rust
// src/network/behaviour.rs  (extended from v0.3)
use libp2p::{identify, ping, kad};
use libp2p::swarm::NetworkBehaviour;

#[derive(NetworkBehaviour)]
pub struct FortiasBehaviour {
    pub identify: identify::Behaviour,
    pub ping:     ping::Behaviour,
    pub kad:      kad::Behaviour<kad::store::MemoryStore>,  // NEW
}

impl FortiasBehaviour {
    pub fn new(local_pub: libp2p::identity::PublicKey, namespace: &str) -> Self {
        let local_peer_id = local_pub.to_peer_id();
        let kad_config = kad::Config::new(
            kad::PROTOCOL_NAME  // overridden to private namespace below
        );
        // Private namespace: derive a protocol name from a shared namespace secret.
        // For v0.4 this is a hardcoded string; v0.5 hardening moves it to config.
        let protocol = libp2p::StreamProtocol::try_from_owned(
            format!("/fortias/kad/{}/1.0.0", namespace)
        ).expect("valid protocol string");
        let mut kad_cfg = kad::Config::new(protocol);
        kad_cfg.set_query_timeout(std::time::Duration::from_secs(30));

        let store = kad::store::MemoryStore::new(local_peer_id);
        Self {
            identify: identify::Behaviour::new(
                identify::Config::new("fortias/0.4.0".into(), local_pub.clone())
                    .with_agent_version(format!("fortias/{}", env!("CARGO_PKG_VERSION")))
            ),
            ping: ping::Behaviour::new(ping::Config::new()),
            kad:  kad::Behaviour::with_config(local_peer_id, store, kad_cfg),
        }
    }
}
```

### 4.2 Bootstrap flow

On startup, after the swarm is built:

```rust
for bootstrap_addr in config.dht_bootstrap_addrs {
    let peer_id = extract_peer_id_from_multiaddr(&bootstrap_addr)?;
    swarm.behaviour_mut().kad.add_address(&peer_id, strip_peer_id(&bootstrap_addr));
}
// Kick off the initial bootstrap query
swarm.behaviour_mut().kad.bootstrap()?;
```

After the swarm reaches `OutboundQueryProgressed { result: BootstrapOk, .. }`, the DHT
is seeded. Subsequent peer discovery happens via provider records (§4.3).

### 4.3 Provider record — "willing to cross-attest"

Cross-attest-willing nodes publish a Kademlia provider record under a
deterministic key derived from the shared namespace:

```rust
const CROSS_ATTEST_KEY_SUFFIX: &str = "/fortias/cross-attest-willing/v1";

fn cross_attest_provider_key(namespace: &str) -> kad::RecordKey {
    kad::RecordKey::new(&format!("{}{}", namespace, CROSS_ATTEST_KEY_SUFFIX))
}
```

On `kad.bootstrap()` completing successfully, the node calls:

```rust
swarm.behaviour_mut().kad.start_providing(cross_attest_provider_key(&namespace))?;
```

To discover peers, the node calls:

```rust
swarm.behaviour_mut().kad.get_providers(cross_attest_provider_key(&namespace));
```

This query fires once at startup (after bootstrap) and periodically every
`dht_reprovide_interval` (default: 20 min). Results arrive via
`kad::Event::OutboundQueryProgressed { result: GetProvidersOk { .. }, .. }`.

### 4.4 Announcing the JSON-RPC address

To enable "close friend" direct TCP connections, each node announces its
JSON-RPC address in the `identify` agent version string:

```rust
// Format: "fortias/<version> rpc=<host:port>"
// Example: "fortias/0.4.0 rpc=198.51.100.7:4001"
let agent = format!("fortias/{} rpc={}", env!("CARGO_PKG_VERSION"), json_rpc_addr);
identify::Config::new(...).with_agent_version(agent)
```

The `Communerd` component parses this on every `NetworkEvent::Identified`
event and populates `PeerAddr.json_rpc` for peers that announce an address.

(@human — embedding the RPC address in the agent string is a minimal
approach that avoids adding a custom identify extension for v0.4. v0.5
hardening may replace this with a signed custom field if the simple string
proves fragile. For now the invariant is: trust the address announced
over Noise-authenticated identify, but treat it as advisory — the
actual RPC connection is still a plain TCP call subject to v0.2's
verification chain.)

### 4.5 `NetworkEvent` additions

```rust
pub enum NetworkEvent {
    Connected    { peer_id: PeerId },
    Disconnected { peer_id: PeerId },
    Identified   { peer_id: PeerId, info: identify::Info },
    // NEW in v0.4:
    DhtPeerDiscovered { peer_id: PeerId, addresses: Vec<Multiaddr> },
    DhtBootstrapComplete,
}
```

`Communerd` handles `DhtPeerDiscovered` by adding the peer to its
known-peers table with a `PeerAddr` (libp2p multiaddr populated;
`json_rpc` populated only if an `Identified` event has already been seen
for this peer).

### 4.6 `DhtPeerSource` — implements v0.2's `PeerSource` trait

```rust
// src/communerd/dht_peer_source.rs
pub struct DhtPeerSource {
    peers: Arc<RwLock<HashMap<PeerId, PeerAddr>>>,
}

#[async_trait::async_trait]
impl PeerSource for DhtPeerSource {
    async fn list(&self) -> Vec<PeerAddr> {
        self.peers.read().values().cloned().collect()
    }
}

impl DhtPeerSource {
    /// Called by Communerd on DhtPeerDiscovered / Identified events.
    pub fn upsert(&self, peer_id: PeerId, addr: PeerAddr) {
        self.peers.write().insert(peer_id, addr);
    }
    pub fn remove(&self, peer_id: &PeerId) {
        self.peers.write().remove(peer_id);
    }
}
```

The `Calendar`'s mutual-attest scheduler calls `peer_source.list()` exactly as it did with `StaticPeerSource` in v0.2. The scheduler is unchanged.

### 4.7 Hybrid transport selection

When `Communerd` executes a mutual-attest request, it selects the transport:

```rust
fn select_transport(peer: &PeerAddr) -> TransportKind {
    if peer.json_rpc.is_some() {
        TransportKind::DirectJsonRpc   // "close friend" — low latency
    } else {
        TransportKind::Libp2p          // general — used for v0.6+ gossip traffic too
    }
}
```

`TransportKind::Libp2p` for mutual-attest is reserved for v0.4 — the
infrastructure is wired in this sub-spec, but libp2p-stream-based `/stamp`
calls are not exercised in the v0.4 demo (all three test nodes announce a
JSON-RPC address, so direct TCP is used). The `Libp2p` arm returns
`TransportError::Unsupported` in v0.4 to keep scope clean; v0.5 lifts that.

(@human — the point of wiring the selection logic now rather than in v0.5
is that adding it later would require touching the mutual-attest scheduler.
Defining the two-arm branch in v0.4, even with one arm unimplemented, keeps
the future diff contained to a single file.)

---

## 5. CONFIGURATION

```json
{
  "network": {
    "p2p_listen":            "/ip4/0.0.0.0/tcp/4101",
    "dht_namespace":         "mainnet",
    "dht_bootstrap":         ["/ip4/bootstrap.fortias.example/tcp/4101/p2p/12D3KooW..."],
    "dht_reprovide_interval_secs": 1200
  }
}
```

CLI: `--dht-bootstrap <multiaddr>` (repeatable). `--dht-namespace <string>` (default `"mainnet"`).

The `--peer` flag from v0.2 continues to work as a static override. If both
`--peer` and `--dht-bootstrap` are given, both sources are active; the
`Communerd` deduplicates by `PeerId`.

---

## 6. TEST PLAN

- `kad_provider_key_deterministic` — same namespace always produces same key bytes.
- `dht_peer_source_upsert_remove` — upsert populates list(); remove removes it.
- `hybrid_transport_selects_direct_when_rpc_addr_known`
- `hybrid_transport_selects_libp2p_when_rpc_addr_absent` — returns `Unsupported` in v0.4.
- Integration: `three_nodes_discover_and_attest` — A and C, bootstrapped only via B, produce external attestations in each other's calendars within 15 s.
- All v0.1, v0.2, v0.3 tests pass.

---

## 7. NON-GOALS FOR v0.4

- ❌ GossipSub — v0.6.
- ❌ Probity scoring of discovered peers — v0.6.
- ❌ libp2p-stream mutual-attest (the `Libp2p` transport arm is wired but returns Unsupported).
- ❌ NAT traversal (AutoNAT, relay, hole-punching) — deferred.
- ❌ Encrypted calendar persistence — v0.5.

---

## 8. MILESTONE CHECKLIST

```
[ ] v0.4.1  Add kad feature to libp2p dependency; FortiasBehaviour gains kad field.
[ ] v0.4.2  Bootstrap flow; provider record publish + query; NetworkEvent additions.
[ ] v0.4.3  DhtPeerSource implements PeerSource; Communerd handles DHT events.
[ ] v0.4.4  JSON-RPC address announcement in identify agent string; parser.
[ ] v0.4.5  Hybrid transport selection (Libp2p arm returns Unsupported).
[ ] v0.4.6  three_nodes_discover_and_attest integration test passes.
[ ] v0.4.7  All prior tests pass.
[ ] v0.4.8  TAG: v0.4-dht-discovery.
```

---
# END OF SUB-SPEC 3 (v0.4)
