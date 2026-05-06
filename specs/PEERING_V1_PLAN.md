# Foretias — Peering v1 Plan (Auto-Port, Self-Registration, DHT Discovery)

**Version:** v0.5.238
**Status:** Draft — TBID index implemented, Phase 2 complete
**Prerequisites:** v0.4 DHT discovery (`FORETIAS_2_P2P_4_dht_discovery.md`)

---

## 1. Problem Statement

Currently, a Foretias node requires manual configuration of:
1. **P2P listen port** — `--p2p-listen /ip4/0.0.0.0/tcp/9901`
2. **Bootstrap peers** — `--dht-bootstrap /ip4/.../p2p/<PeerId>`
3. **Peer dials** — `--p2p-dial /ip4/.../tcp/9902/p2p/<PeerId>`

This makes multi-node testing painful and prevents zero-config deployment. A new node has no way to announce itself or discover peers without knowing their addresses ahead of time.

**Goal:** A node should start with only a list of known server IPs, automatically find a free port, register itself to the DHT, and discover peers for mutual attestation — all without manual multiaddr construction.

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                        New Node Startup                          │
│                                                                  │
│  1. Generate ed25519 keypair (PeerId)                            │
│  2. Pick random port from --p2p-port-range (default 9900..9999)  │
│  3. Bind swarm to 0.0.0.0:<port>                                │
│  4. Capture actual bound address from NewListenAddr              │
│  5. Dial first known server (from --known-servers)               │
│  6. PUT record to DHT:                                           │
│     key = "/foretias/<namespace>/peers/v1"                       │
│     value = { TBID, PeerId, multiaddr, chronon_ns }              │
│  7. Bootstrap DHT                                                │
│  8. GET record from DHT → discover peer list                     │
│  9. Dial discovered peers (up to --max-discovered-peers=13)      │
│ 10. Add to PeerPool → begin auto-attestation                     │
└──────────────────────────────────────────────────────────────────┘
```

---

## 3. Port Selection

### 3.1 Range Configuration

- **Default range:** `9900..9999` (100 ports)
- **CLI flag:** `--p2p-port-range 9900..9999`
- **Format:** `start..end` (inclusive start, exclusive end)
- **Config file key:** `p2p_port_range: [9900, 9999]`

### 3.2 Selection Algorithm

```rust
fn find_free_port(range: Range<u16>) -> Result<u16, NodeError> {
    let mut ports: Vec<u16> = range.collect();
    ports.shuffle(&mut thread_rng());
    for port in ports {
        if let Ok(listener) = TcpListener::bind(format!("0.0.0.0:{}", port)) {
            listener.set_nonblocking(true)?;
            return Ok(port);
        }
    }
    Err(NodeError::Internal("no free ports in range".into()))
}
```

- Shuffles the range to avoid contention when many nodes start simultaneously
- Uses non-blocking bind to avoid blocking on EADDRINUSE
- Returns `EADDRNOTAVAIL` if no port is free

### 3.3 Backward Compatibility

If `--p2p-listen` is explicitly provided, it takes precedence and port range is ignored. This maintains full backward compatibility.

### 3.4 Current Code References

- **Existing port 0 pattern:** `tests/integration.rs:16` — `TcpListener::bind("127.0.0.1:0")` then `local_addr().port()`
- **Swarm listen binding:** `swarm.rs:62` — `swarm.listen_on(listen)` takes `libp2p::Multiaddr`
- **Swarm constructor:** `swarm.rs:38` — `build_and_spawn_swarm(listen: libp2p::Multiaddr, ...)` — must accept `Option<Multiaddr>` for auto-port mode
- **Communerd enable_p2p:** `mod.rs:164` — constructs listen multiaddr from `--p2p-listen`; no fallback to port 0 currently

---

## 4. Self-Registration Protocol

### 4.1 Registration Record

After binding, the node publishes a Kademlia record:

```json
{
  "key": "/foretias/mainnet/peers/v1",
  "value": {
    "peer_id": "12D3KooW...",
    "tbid": "a1b2c3d4e5f6...",
    "multiaddr": "/ip4/192.168.1.5/tcp/9942/p2p/12D3KooW...",
    "json_rpc": "192.168.1.5:4001",
    "chronon_ns": 60000000000,
    "registered_at_ns": 1714876543000000000
  }
}
```

### 4.2 Record Key Scheme

```
/foretias/{namespace}/peers/v1
```

The namespace isolates testnets from mainnet. All nodes in the same namespace share the same DHT key for peer discovery.

### 4.3 Value Format

- **peer_id:** libp2p PeerId (Ed25519-derived)
- **tbid:** Time Being ID (16 bytes, hex-encoded)
- **multiaddr:** Full libp2p multiaddr with auto-discovered IP and port
- **json_rpc:** Host:port for direct JSON-RPC attestation (v0.2 transport)
- **chronon_ns:** Chronon period in nanoseconds (for attestation interval calculation)
- **registered_at_ns:** UNIX nanosecond timestamp of registration

### 4.4 Registration Flow

```rust
async fn register_self(
    swarm: &mut Swarm<ForetiasBehaviour>,
    namespace: &str,
    peer_id: PeerId,
    tbid: [u8; 16],
    multiaddr: Multiaddr,
    json_rpc: &str,
    chronon_ns: u64,
) -> Result<(), NodeError> {
    let key = kad::RecordKey::new(&format!("/foretias/{}/peers/v1", namespace));
    let value = PeerRecord {
        peer_id: peer_id.to_string(),
        tbid: hex::encode(tbid),
        multiaddr: multiaddr.to_string(),
        json_rpc: json_rpc.to_string(),
        chronon_ns,
        registered_at_ns: SystemTime::now().elapsed()?.as_nanos() as u64,
    };
    let record = kad::Record {
        key,
        value: serde_json::to_vec(&value)?,
        publisher: Some(peer_id),
        expires: None,
    };
    swarm.behaviour_mut().kad.put_record(record, kad::Quorum::One)?;
    Ok(())
}
```

### 4.5 Refresh Interval

Registration record is refreshed every 60 seconds via a background task to maintain DHT presence.

### 4.6 Current SwarmCommand Extensions Needed

**File:** `p2p/foretias-node/src/communerd/p2p/swarm.rs:27`

Current `SwarmCommand` enum:
```rust
pub enum SwarmCommand {
    Bootstrap,
    Provide { key: kad::RecordKey },
    GetProviders { key: kad::RecordKey },
    Dial { addr: libp2p::Multiaddr },
    EnterDormancy,
    PublishProbity { report: ProbityReport, namespace: String },
    PublishHeartbeat { heartbeat: Heartbeat, namespace: String },
}
```

**Add:**
```rust
PutRecord { key: kad::RecordKey, record: kad::Record },
GetRecord { key: kad::RecordKey },
```

---

## 5. Peer Discovery

### 5.1 Discovery Flow

```
GET record from DHT key "/foretias/{namespace}/peers/v1"
  → Returns list of PeerRecord values
  → Filter out own PeerId
  → Shuffle remaining peers
  → Take up to max_discovered_peers (default 13)
  → For each peer:
    a. Dial multiaddr
    b. On connection established, extract JSON-RPC address from identify info
    c. Add to PeerPool with PeerAddr { json_rpc, peer_id, last_seen_ns }
    d. Begin auto-attestation cycle
```

### 5.2 Max Discovered Peers

- **Default:** 13
- **CLI flag:** `--max-discovered-peers 13`
- **Config key:** `max_discovered_peers: 13`
- **Purpose:** Bounds the number of peers a node auto-discovers to prevent unbounded connection growth

### 5.3 Peer Record Deduplication

If a peer is already in the PeerPool (by `json_rpc` address), the discovery skip it. This prevents duplicate attestation cycles.

### 5.4 Current PeerPool References

**File:** `p2p/foretias-node/src/communerd/peer_pool.rs`

Existing methods:
- `add_peer(&self, addr: PeerAddr)` — line 43
- `remove_peer(&self, addr: &PeerAddr)` — line 50
- `get_peers(&self) -> Vec<PeerAddr>` — line 55
- `start_liveness_pings(self)` — line 60

**File:** `p2p/foretias-node/src/communerd/dht_peer_source.rs`

Existing `DhtPeerSource`:
- `upsert(&self, peer_id: PeerId, addr: PeerAddr)` — inserts/updates discovered peer
- `remove(&self, peer_id: &PeerId)` — removes peer
- `update_json_rpc(&self, peer_id: &PeerId, json_rpc: String)` — updates RPC addr from identify
- `MAX_PEERS: usize = 256` — hard cap on DHT peer table

---

## 6. Known Servers

### 6.1 Configuration

- **CLI flag:** `--known-servers 192.168.1.1:9901` (repeatable)
- **Config key:** `known_servers: ["192.168.1.1:9901", "bootstrap.foretias.example:4101"]`
- **Format:** `host:port` — the host is the IP/DNS, the port is the p2p listen port

### 6.2 Resolution

Known servers are resolved into full multiaddrs:
```
"192.168.1.1:9901" → "/ip4/192.168.1.1/tcp/9901"
"bootstrap.example:4101" → "/dns/bootstrap.example/tcp/4101"
```

If the host is an IP literal, use `/ip4/` prefix. If it's a hostname, use `/dns/` prefix.

### 6.3 Dial Strategy

On startup, the node dials all known servers. The first successful connection triggers the bootstrap + registration flow. If all known servers are unreachable, the node logs a warning and continues without DHT participation.

---

## 7. Attestation Interval Formula

When a peer is discovered, the attestation interval is calculated as:

```
interval = max(MIN_INTERVAL, 50 * (my_chronon_ns + peer_chronon_ns))
```

Where:
- **MIN_INTERVAL:** `FORETIAS_MUTUAL_ATTESTATION_MINIMUM` environment variable or default `30_000_000_000` (30 seconds)
- **my_chronon_ns:** This node's chronon period in nanoseconds
- **peer_chronon_ns:** Discovered peer's chronon period from their registration record

This ensures:
- Fast chronon nodes attest frequently
- Slow chronon nodes don't overwhelm fast nodes
- The interval scales with both parties' tick rates

---

## 8. Code Changes Summary

### 8.1 `swarm.rs` — New SwarmCommands & ListenReady Event

**File:** `p2p/foretias-node/src/communerd/p2p/swarm.rs`

**Changes:**
1. Add `PutRecord` and `GetRecord` to `SwarmCommand` enum (line 27)
2. Handle `PutRecord` in `swarm_loop` command handler (line 103)
3. Handle `GetRecord` in `swarm_loop` command handler (line 103)
4. Capture `NewListenAddr` event's `multiaddr` and emit `NetworkEvent::ListenReady` (line 156)
5. Add `local_multiaddr: Arc<std::sync::Mutex<Option<Multiaddr>>>` to `SwarmHandle` (line 20)
6. Support `Option<Multiaddr>` in `build_and_spawn_swarm` signature (line 38)

```rust
pub struct SwarmHandle {
    pub local_peer_id: PeerId,
    pub local_multiaddr: Arc<std::sync::Mutex<Option<Multiaddr>>>,
    pub events: mpsc::UnboundedReceiver<NetworkEvent>,
    pub cmd_tx: mpsc::UnboundedSender<SwarmCommand>,
    pub task: tokio::task::JoinHandle<()>,
}

pub enum SwarmCommand {
    Bootstrap,
    Provide { key: kad::RecordKey },
    GetProviders { key: kad::RecordKey },
    PutRecord { key: kad::RecordKey, record: kad::Record },
    GetRecord { key: kad::RecordKey },
    Dial { addr: libp2p::Multiaddr },
    EnterDormancy,
    PublishProbity { report: ProbityReport, namespace: String },
    PublishHeartbeat { heartbeat: Heartbeat, namespace: String },
}

pub async fn build_and_spawn_swarm(
    listen: Option<libp2p::Multiaddr>,
    dials: Vec<libp2p::Multiaddr>,
    namespace: &str,
    json_rpc_addr: Option<&str>,
) -> Result<SwarmHandle, NodeError>
```

### 8.2 `events.rs` — New NetworkEvents

**File:** `p2p/foretias-node/src/communerd/p2p/events.rs`

```rust
pub enum NetworkEvent {
    Connected { peer_id: PeerId },
    Disconnected { peer_id: PeerId },
    Identified { peer_id: PeerId, info: identify::Info },
    PingSuccess { peer_id: PeerId, rtt: std::time::Duration },
    DhtPeerDiscovered { peer_id: PeerId, addresses: Vec<Multiaddr> },
    DhtBootstrapComplete,
    GossipMessage { data: Vec<u8>, source: PeerId },
    HeartbeatMessage { data: Vec<u8>, source: PeerId },
    ListenReady { multiaddr: Multiaddr },
    RecordRetrieved { key: kad::RecordKey, records: Vec<kad::Record> },
}
```

### 8.3 `mod.rs` (Communerd) — New Methods

**File:** `p2p/foretias-node/src/communerd/mod.rs`

```rust
impl Communerd {
    pub async fn enable_p2p(
        &self,
        listen: libp2p::Multiaddr,
        dials: Vec<libp2p::Multiaddr>,
        namespace: &str,
        json_rpc_addr: Option<&str>,
    ) -> Result<(), NodeError>
    // ^^^ CURRENT — must accept Option<Multiaddr> for listen

    pub async fn register_and_discover(
        &self,
        known_servers: Vec<String>,
        namespace: &str,
        tbid: [u8; 16],
        chronon_ns: u64,
        json_rpc_addr: &str,
        max_peers: usize,
    ) -> Result<(), NodeError>;

    pub async fn refresh_registration(&self, namespace: &str) -> Result<(), NodeError>;
}
```

### 8.4 `config.rs` (NodeConfig) — New Fields

**File:** `p2p/core-engine/src/config.rs`

```rust
pub struct NodeConfig {
    // ... existing fields ...
    pub p2p_listen: Option<String>,
    pub p2p_dial: Vec<String>,
    pub dht_namespace: String,
    pub dht_bootstrap: Vec<String>,
    // NEW FIELDS:
    pub p2p_port_range: Option<[u16; 2]>,
    pub known_servers: Vec<String>,
    pub max_discovered_peers: usize,
}
```

Current `NodeConfig` defaults (line 84-103):
- `p2p_listen: None`
- `p2p_dial: Vec::new()`
- `dht_namespace: "mainnet".to_string()`
- `dht_bootstrap: Vec::new()`

### 8.5 `main.rs` — CLI Flag Wiring

**File:** `p2p/foretias-node/src/main.rs`

Current `Serve` subcommand (line 30-73) already has:
- `p2p_listen: Option<String>` (line 54)
- `p2p_port_range: String` with default `9900..9999` (line 57)
- `known_servers: Vec<String>` (line 63)
- `max_discovered_peers: usize` with default 13 (line 72)

**Missing from `cmd_serve` function (line 268-372):**
- `p2p_port_range`, `known_servers`, `max_discovered_peers` are NOT passed to `cmd_serve`
- Current `cmd_serve` signature (line 268) doesn't accept these parameters
- Current `cmd_serve` only calls `enable_p2p` when `p2p_listen` is `Some` (line 319)
- No self-registration or peer discovery logic exists

**Changes needed:**
1. Add `p2p_port_range`, `known_servers`, `max_discovered_peers` to `cmd_serve` signature
2. Parse port range string into `Range<u16>`
3. When `p2p_listen` is `None` and `known_servers` is not empty:
   a. Find free port in range
   b. Construct multiaddr `/ip4/0.0.0.0/tcp/<port>`
   c. Call `enable_p2p(Some(listen), ...)`
   d. Wait for `ListenReady` event
   e. Call `register_and_discover(known_servers, ...)`
4. Update `main()` match arm (line 637) to pass new parameters

### 8.6 `main.rs` — Current cmd_serve signature vs needed

**Current (line 268):**
```rust
async fn cmd_serve(
    addr: String,
    chronon_ns: u64,
    persist_path: Option<String>,
    start_dormant: bool,
    peers: Vec<String>,
    auto_attest_every_chronons: u64,
    request_timeout_secs: u64,
    p2p_listen: Option<String>,
    p2p_dial: Vec<String>,
    dht_namespace: String,
    dht_bootstrap: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>>
```

**Needed:**
```rust
async fn cmd_serve(
    addr: String,
    chronon_ns: u64,
    persist_path: Option<String>,
    start_dormant: bool,
    peers: Vec<String>,
    auto_attest_every_chronons: u64,
    request_timeout_secs: u64,
    p2p_listen: Option<String>,
    p2p_port_range: String,
    p2p_dial: Vec<String>,
    known_servers: Vec<String>,
    dht_namespace: String,
    dht_bootstrap: Vec<String>,
    max_discovered_peers: usize,
) -> Result<(), Box<dyn std::error::Error>>
```

---

## 9. Language Bindings Inventory

### 9.1 Rust (Core)

| Component | File | Notes |
|-----------|------|-------|
| CLI binary | `p2p/foretias-node/src/main.rs` | clap-based, all subcommands |
| Communerd | `p2p/foretias-node/src/communerd/mod.rs` | P2P orchestrator |
| Swarm | `p2p/foretias-node/src/communerd/p2p/swarm.rs` | libp2p swarm construction |
| Events | `p2p/foretias-node/src/communerd/p2p/events.rs` | NetworkEvent enum |
| Behaviour | `p2p/foretias-node/src/communerd/p2p/behaviour.rs` | ForetiasBehaviour (identify, ping, kad, gossipsub) |
| PeerPool | `p2p/foretias-node/src/communerd/peer_pool.rs` | PeerAddr management, liveness pings |
| DhtPeerSource | `p2p/foretias-node/src/communerd/dht_peer_source.rs` | DHT-discovered peer table |
| Config | `p2p/core-engine/src/config.rs` | NodeConfig struct |
| Server | `p2p/foretias-node/src/server/mod.rs` | TimeFamilyServer, get_tbid() → [u8; 16] |
| Gossip | `p2p/foretias-node/src/communerd/p2p/gossip.rs` | publish_probity_report(), publish_heartbeat() |

### 9.2 C (Native FFI)

| Component | File | Notes |
|-----------|------|-------|
| C Header | `p2p/core/include/foretias_core.h` | FFI header for foreign languages |
| Bindings | `p2p/core-engine/src/core/bindings.rs` | rust-bindgen auto-generated bindings |

Java CLI uses JNI to this FFI layer.

### 9.3 Python (PyO3/maturin)

| Component | File | Notes |
|-----------|------|-------|
| PyO3 Bindings | `p2p/foretias-python/src/lib.rs` | Python module bindings |
| Python CLI | `p2p/foretias-python/src/cli.py` | argparse-based CLI (to be created) |

### 9.4 Java (Planned)

| Component | File | Notes |
|-----------|------|-------|
| Java CLI | `p2p/foretias-java/src/main/java/foretias/Cli.java` | jcommander/picocli CLI (to be created) |
| JNI Bridge | `p2p/foretias-java/src/main/java/foretias/NativeBridge.java` | JNI wrapper for foretias_core.h |

---

## 10. Implementation Phase Breakdown

### Phase 1 — CLI Spec (COMPLETE)

- [x] `foretias/specs/CLI_SPECIFIED.md` — Complete CLI spec covering all commands/flags
- [x] `foretias/specs/PEERING_V1_PLAN.md` — This document

### Phase 2 — Rust DHT/P2P Core (PENDING)

- [ ] Implement `find_free_port()` with range shuffle in `swarm.rs`
- [ ] Modify `build_and_spawn_swarm()` to accept `Option<Multiaddr>`
- [ ] Add `local_multiaddr` field to `SwarmHandle`
- [ ] Add `PutRecord` and `GetRecord` to `SwarmCommand`
- [ ] Handle `PutRecord`/`GetRecord` in `swarm_loop`
- [ ] Emit `ListenReady` event from `NewListenAddr` handler
- [ ] Emit `RecordRetrieved` event from `GetRecord` query result
- [ ] Add `p2p_port_range`, `known_servers`, `max_discovered_peers` to `NodeConfig`
- [ ] Add `register_and_discover()` method to `Communerd`
- [ ] Wire `cmd_serve` to accept and use new parameters
- [ ] Wire `main()` match arm to pass new parameters

### Phase 3 — Peer Attestation (PENDING)

- [ ] Wire `DhtPeerDiscovered` events into `PeerPool.add_peer()` in gossip loop
- [ ] Implement attestation interval formula in auto-attestation scheduler
- [ ] Print server startup info with P2P details (actual port, TBID, PeerId, multiaddr)

### Phase 4 — Multi-Language Expansion (PENDING)

- [ ] Update Python CLI (`foretias-python/src/cli.py`) to match CLI spec
- [ ] Update PyO3 bindings to expose auto-port and known-servers
- [ ] Ensure Python TimeFamily can register to DHT and discover peers
- [ ] Create Java CLI with jcommander/picocli matching CLI spec
- [ ] Create JNI bridge to `foretias_core.h`

### Phase 5 — Integration Tests (PENDING)

- [ ] Rust: 1 node + 100 peers, verify DHT discovery and attestation
- [ ] Rust ↔ Python: Cross-language DHT discovery
- [ ] Rust ↔ Java: Cross-language DHT discovery
- [ ] Python ↔ Java: Cross-language DHT discovery

---

## 11. Integration Test Plan

### 11.1 Rust-Only Test

```
1. Start node A with --known-servers (no p2p-listen)
2. Verify A prints its auto-selected port
3. Start 100 nodes B1..B100 with --known-servers A's address
4. Each Bi discovers A and registers
5. Verify A discovers at least 13 peers within 30 seconds
6. Verify mutual attestation occurs between A and discovered peers
```

### 11.2 Cross-Language Tests

- **Rust ↔ Python:** Rust node auto-discovers Python node via DHT
- **Rust ↔ Java:** Rust node auto-discovers Java node via DHT
- **Python ↔ Java:** Python node auto-discovers Java node via DHT

---

## 12. Out of Scope

- NAT traversal / hole-punching (deferred, see `FORETIAS_2_P2P_3_IMPLEMENTATION_PLAN.md` §8.4)
- QUIC transport (TCP only for now)
- Relay / AutoNAT (deferred)
- Dynamic peer removal (peers stay in pool until manually removed or liveness ping fails 3×)

---

## 13. References

- `FORETIAS_2_P2P_4_dht_discovery.md` — DHT provider model (v0.4)
- `FORETIAS_2_P2P_3_IMPLEMENTATION_PLAN.md` — Port strategy, NAT discussion (§8.1–8.4)
- `P2P_DESIGN_DISCUSSION.md` — BitTorrent/Bitcoin bootstrap lessons, hardcoded bootstrap peers
- `CLI_SPECIFIED.md` — CLI specification (all implementations must conform)

---

## 14. DHT TBID Index

### 14.1 Purpose
The DHT TBID index enables cross-node verification: when a Time Being needs to verify a Foretis from an unknown TBID, it queries the DHT for the TBID owner's address, fetches the relevant calendar slice, and verifies locally.

### 14.2 Key Scheme
```
/foretias/{namespace}/tbid/{hex_tbid}/v1
```
The value is a `PeerRegistrationRecord` (same struct as peer registration):
```json
{
  "peer_id": "...",
  "tbid": "a1b2c3d4...",
  "multiaddr": "/ip4/.../tcp/9942",
  "json_rpc": "192.168.1.5:4001",
  "chronon_ns": 60000000000,
  "registered_at_ns": 1714876543000000000
}
```

### 14.3 Publishing
Each node publishes TWO DHT records:
1. `/foretias/{ns}/peers/v1` — for peer discovery (existing)
2. `/foretias/{ns}/tbid/{hex_tbid}/v1` — for TBID index lookup (new)

Both records are refreshed every 60 seconds by the same background task.

### 14.4 Lookup Flow
```
handle_verify(cross_node: true)
  → local calendar miss
  → communerd.lookup_tbid(tbid_hex, namespace)
    → check tbid_index cache first
    → if miss, DHT GetRecord for /foretias/{ns}/tbid/{hex}/v1
    → oneshot channel (5s timeout) for async resolution
    → gossip loop caches RecordRetrieved results in tbid_index
  → get_calendar_slice(owner, tick_number, 1)
  → crypto.verify_with(pk, alg, sig_input, signature)
```

### 14.5 Oneshot Channel Pattern
```rust
pending_lookups: Arc<Mutex<HashMap<RecordKey, oneshot::Sender<Option<PeerRegistrationRecord>>>>>
```
The gossip loop resolves pending lookups when `RecordRetrieved` events arrive. The caller awaits with a 5-second timeout.

### 14.6 Gossip Loop TBID Caching
The gossip loop handles `/tbid/` records by:
1. Parsing as `PeerRegistrationRecord`
2. Inserting into `tbid_index: RwLock<HashMap<String, PeerRegistrationRecord>>`
3. Resolving any pending oneshot channel for that key
