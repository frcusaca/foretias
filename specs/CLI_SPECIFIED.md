# Foretias CLI Specification

**Version:** v0.5.239
**Status:** Draft — all implementations (Rust, Python, Java) must conform
**Scope:** Complete CLI command, flag, and option specification for the Foretias TimeFamilyServer

---

## 1. Command Structure

```
foretias <command> [options] [arguments]
```

All implementations (Rust via clap, Python via argparse, Java via JCommander/CLI11) must support identical command names, short flags, long flags, defaults, and behaviors.

---

## 2. Global Options (None)

Currently no global options. All options are scoped to subcommands.

---

## 3. Subcommands

### 3.1 `serve` — Start the TimeFamilyServer

Starts a Foretias TimeFamily node that listens for JSON-RPC connections and optionally participates in the P2P network.

```
foretias serve [options]
```

#### 3.1.1 Address & Port Options

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-a` | `--addr` | `127.0.0.1:4001` | JSON-RPC listen address (host:port) |
| | `--p2p-listen` | *(none)* | libp2p listen multiaddr (e.g. `/ip4/0.0.0.0/tcp/9901`). If omitted and `--p2p-port-range` is set, auto-select port |
| | `--p2p-port-range` | `9900..9999` | Port range for auto-selection when `--p2p-listen` is omitted. Format: `start..end` (inclusive start, exclusive end) |

**Precedence:** `--p2p-listen` > `--p2p-port-range`. If both are given, `--p2p-listen` wins and port range is ignored.

**Auto-port algorithm:** When `--p2p-listen` is omitted:
1. Parse `--p2p-port-range` into `Range<u16>`
2. Shuffle the port range to avoid contention
3. Bind `TcpListener` to first available port in the shuffled range
4. Construct multiaddr `/ip4/0.0.0.0/tcp/<port>`
5. Pass to `build_and_spawn_swarm(Some(multiaddr), ...)`

#### 3.1.2 Chronon & Persistence Options

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-c` | `--chronon-ns` | `60000000000` | Chronon period in nanoseconds (60s default) |
| | `--persist-path` | *(none)* | Directory to persist calendar JSON files |
| | `--start-dormant` | `false` | Start in verify-only mode. Requires `--persist-path` |

#### 3.1.3 Peer & Attestation Options

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| | `--peer` | *(none, repeatable)* | Static peer address for mutual attestation (host:port). Can specify multiple times |
| | `--mutually-attest-every-chronons` | `1` | Mutual attestation frequency in chronons |
| | `--request-timeout-secs` | `5` | RPC request timeout in seconds |

#### 3.1.4 P2P & DHT Options

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| | `--p2p-dial` | *(none, repeatable)* | libp2p peer multiaddr to dial at startup. Can specify multiple times |
| | `--dht-namespace` | `mainnet` | DHT namespace for Kademlia protocol isolation |
| | `--dht-bootstrap` | *(none, repeatable)* | DHT bootstrap peer multiaddr. Can specify multiple times |
| `-k` | `--known-servers` | *(none, repeatable)* | Known server addresses for auto-registration. Format: `host:port`. Can specify multiple times |
| | `--max-discovered-peers` | `13` | Maximum number of peers to auto-discover from DHT |

**Known Servers Flow:**
When `--known-servers` is provided:
1. Resolve each `host:port` to a multiaddr (`/ip4/<host>/tcp/<port>` or `/dns/<host>/tcp/<port>`)
2. Dial all known servers via `SwarmCommand::Dial`
3. On first successful connection, bootstrap DHT
4. Self-register: `PUT /foretias/{namespace}/peers/v1` with `{ TBID, PeerId, multiaddr, json_rpc, chronon_ns }`
5. Discover peers: `GET /foretias/{namespace}/peers/v1` from DHT
6. Dial discovered peers (up to `--max-discovered-peers`)
7. Add discovered peers to PeerPool for mutual attestation

**Attestation Interval for Discovered Peers:**
```
interval = max(FORETIAS_MUTUAL_ATTESTATION_MINIMUM, 50 * (my_chronon_ns + peer_chronon_ns))
```
Where `FORETIAS_MUTUAL_ATTESTATION_MINIMUM` defaults to `30_000_000_000` (30 seconds) and can be overridden via environment variable.

#### 3.1.5 Serve Command Examples

```bash
# Simple local server (JSON-RPC only)
foretias serve --addr 127.0.0.1:4001

# P2P node with auto-port, known server for discovery
foretias serve --addr 127.0.0.1:4001 --p2p-port-range 9900..9999 --known-servers 192.168.1.1:9901

# P2P node with explicit port and bootstrap
foretias serve --addr 127.0.0.1:4001 --p2p-listen /ip4/0.0.0.0/tcp/9901 --dht-bootstrap /ip4/127.0.0.1/tcp/9902/p2p/12D3KooW...

# Dormant (verify-only) node
foretias serve --addr 127.0.0.1:4001 --start-dormant --persist-path ~/.foretias/cal

# Auto-discovery with custom max peers
foretias serve -k 10.0.0.1:9901 -k 10.0.0.2:9902 --max-discovered-peers 25
```

#### 3.1.6 Startup Output

When P2P is enabled, the server MUST print:

```
Foretias TimeFamilyServer starting...
  Listen : 127.0.0.1:4001
  TBN    : <tbn>
  TBID   : <hex-encoded-tbid>
  Chronon: 1 minute
  P2P PeerId : <peer-id>
  P2P Listen : /ip4/0.0.0.0/tcp/9942
  Known Servers : 192.168.1.1:9901
```

---

### 3.2 `stamp` — Stamp Content

Stamp arbitrary content via a running TimeFamilyServer. Returns a Foretis (timestamped signature).

```
foretias stamp [options]
```

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-m` | `--message` | *(none)* | Message to stamp (inline string) |
| `-M` | `--message-file` | *(none)* | Read message from file |
| `-o` | `--stamp-output` | *(stdout)* | Write stamp output to file |
| `-s` | `--server` | `127.0.0.1:4001` | Server address |

**Mutual exclusivity:** `--message` and `--message-file` are mutually exclusive. Exactly one must be provided.

**Output:** JSON Foretis object to stdout or `--stamp-output` file.

---

### 3.3 `verify` — Verify Content Against Foretis

Server-side verification of a Foretis against content.

```
foretias verify [options]
```

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-m` | `--message` | *(none)* | Message to verify (inline string) |
| `-M` | `--message-file` | *(none)* | Read message from file |
| `-f` | `--foretis` | *(none)* | Foretis JSON inline |
| `-F` | `--foretis-file` | *(none)* | Read Foretis from file |
| `-o` | `--verify-output` | *(stdout)* | Write verify output to file |
| `-s` | `--server` | `127.0.0.1:4001` | Server address |

**Mutual exclusivity:**
- `--message` XOR `--message-file` (exactly one)
- `--foretis` XOR `--foretis-file` (exactly one)

**Output:** JSON verification result (`{ "valid": true/false, ... }`)

---

### 3.4 `prove-verification` — Local Proof of Verification

Fetch calendar slice from remote server and verify locally (no `/verify` call). This is a proof-of-verification artifact.

```
foretias prove-verification [options]
```

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-m` | `--message` | *(none)* | Message to verify |
| `-M` | `--message-file` | *(none)* | Read message from file |
| `-f` | `--foretis` | *(none)* | Foretis JSON inline |
| `-F` | `--foretis-file` | *(none)* | Read Foretis from file |
| `-o` | `--proof-output` | *(stdout)* | Write proof output to file |
| `-s` | `--server` | `127.0.0.1:4001` | Remote server address |

**Output:** JSON proof artifact with `verified_locally: true`, calendar records, and foretis.

---

### 3.5 `inspect-attestations` — Inspect Persisted Calendar

Load a persisted calendar JSON and re-verify each external attestation's signature and hash.

```
foretias inspect-attestations [options]
```

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-c` | `--calendar` | *(required)* | Path to calendar JSON file |

**Exit codes:**
- `0` — all attestations valid
- `1` — one or more attestations invalid

**Output:** Per-attestation status lines + summary:
```
tick=1 attester=abc123 attester_tick=1 sig=VALID
tick=2 attester=abc123 attester_tick=2 sig=VALID
--- Summary ---
Total attestations: 2
Valid:              2
Invalid:            0
```

---

### 3.6 `info` — Display Node Information

Display current node identity, configuration, and network state.

```
foretias info [options]
```

| Short | Long | Default | Description |
|-------|------|---------|-------------|
| `-s` | `--server` | `127.0.0.1:4001` | Server address |

**Output:**
```json
{
  "tbid": "a1b2c3...",
  "tbn": "TestNet",
  "current_tick": 42,
  "chronon_ns": 60000000000,
  "peer_id": "12D3KooW...",
  "p2p_listen": "/ip4/0.0.0.0/tcp/9942",
  "dht_namespace": "mainnet",
  "connected_peers": 5,
  "discovered_peers": 12
}
```

---

## 4. Configuration File

The CLI reads a JSON configuration file from `~/.config/foretias/foretias.settings.json` (or `$FORETIAS_CONFIG` env var).

CLI flags **override** config file values. Config file provides defaults.

```json
{
  "listen_addr": "127.0.0.1:4001",
  "chronon_ns": 60000000000,
  "p2p_port_range": [9900, 9999],
  "known_servers": ["192.168.1.1:9901"],
  "max_discovered_peers": 13,
  "dht_namespace": "mainnet",
  "request_timeout_secs": 5,
  "mutual_attest_every_n": 1
}
```

---

## 5. Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FORETIAS_CONFIG` | `~/.config/foretias/foretias.settings.json` | Path to config file |
| `FORETIAS_MUTUAL_ATTESTATION_MINIMUM` | `30000000000` | Minimum attestation interval in nanoseconds (30s) |

---

## 6. Error Handling

All implementations must:
- Return exit code `0` on success
- Return exit code `1` on any error
- Print error messages to stderr
- Print usage/help on `--help` or missing required arguments

---

## 7. Cross-Language Conformance

Each language implementation (Rust, Python, Java) must:
1. Support all commands and flags listed above
2. Use identical short/long flag names
3. Use identical default values
4. Produce identical JSON output formats
5. Support the same configuration file format

### 7.1 Rust Implementation

| Detail | Value |
|--------|-------|
| Library | `clap` (derive mode) |
| Location | `p2p/foretias-node/src/main.rs` |
| Status | Partial — CLI flags exist but not all wired to `cmd_serve` |
| Current Flags | `--addr`, `--p2p-listen`, `--p2p-port-range`, `--p2p-dial`, `--known-servers`, `--max-discovered-peers`, `--dht-namespace`, `--dht-bootstrap`, `--mutually-attest-every-chronons`, `--request-timeout-secs`, `--persist-path`, `--start-dormant`, `--peer` |
| Missing Wiring | `cmd_serve()` signature (line 268) doesn't accept `p2p_port_range`, `known_servers`, `max_discovered_peers`. LSP error on line 637 confirms missing destructuring. |
| Self-Registration | Not implemented. `Communerd` needs `register_and_discover()` method. |
| Auto-Port Selection | Not implemented. `build_and_spawn_swarm()` takes `Multiaddr`, needs `Option<Multiaddr>`. |

### 7.2 Python Implementation

| Detail | Value |
|--------|-------|
| Library | `argparse` |
| Location | `p2p/foretias-python/src/cli.py` |
| Status | Not implemented — CLI file does not exist yet |
| PyO3 Bindings | `p2p/foretias-python/src/lib.rs` — existing bindings need extension for auto-port, known-servers, DHT registration |

### 7.3 Java Implementation

| Detail | Value |
|--------|-------|
| Library | `jcommander` or `picocli` |
| Location | `p2p/foretias-java/src/main/java/foretias/Cli.java` |
| Status | Not implemented — Java project does not exist yet |
| JNI Bridge | `p2p/core/include/foretias_core.h` + `p2p/core-engine/src/core/bindings.rs` — C FFI layer available for JNI wrapper |

---

## 8. Current Code State (Rust)

### 8.1 CLI Flags (main.rs)

**File:** `p2p/foretias-node/src/main.rs`

The `Serve` subcommand struct (lines 30-73) declares:
```rust
Serve {
    addr: String,                           // --addr, default "127.0.0.1:4001"
    chronon_ns: u64,                        // --chronon-ns, default 60_000_000_000
    persist_path: Option<String>,           // --persist-path
    start_dormant: bool,                    // --start-dormant
    peer: Vec<String>,                      // --peer (repeatable)
    mutually_attest_every_chronons: u64,    // --mutually-attest-every-chronons, default 1
    request_timeout_secs: u64,              // --request-timeout-secs, default 5
    p2p_listen: Option<String>,             // --p2p-listen
    p2p_port_range: String,                 // --p2p-port-range, default "9900..9999"
    p2p_dial: Vec<String>,                  // --p2p-dial (repeatable)
    known_servers: Vec<String>,             // --known-servers (repeatable, -k)
    dht_namespace: String,                  // --dht-namespace, default "mainnet"
    dht_bootstrap: Vec<String>,             // --dht-bootstrap (repeatable)
    max_discovered_peers: usize,            // --max-discovered-peers, default 13
}
```

**Problem:** `cmd_serve()` (line 268) does NOT accept `p2p_port_range`, `known_servers`, `max_discovered_peers`. The `main()` match arm (line 637) destructures only `p2p_listen, p2p_dial, dht_namespace, dht_bootstrap` — missing the three new fields. LSP confirms this error.

### 8.2 Swarm Architecture

**File:** `p2p/foretias-node/src/communerd/p2p/swarm.rs`

- `SwarmHandle` (line 20): Has `local_peer_id`, `events`, `cmd_tx`, `task`. Missing `local_multiaddr`.
- `SwarmCommand` (line 27): Has `Bootstrap`, `Provide`, `GetProviders`, `Dial`, `EnterDormancy`, `PublishProbity`, `PublishHeartbeat`. Missing `PutRecord`, `GetRecord`.
- `build_and_spawn_swarm()` (line 38): Takes `listen: libp2p::Multiaddr`. Must accept `Option<libp2p::Multiaddr>` for auto-port mode.
- `swarm_loop` (line 91): Handles `NewListenAddr` at line 156 but only tracks `listener_id`, doesn't emit `NetworkEvent::ListenReady`.

### 8.3 Network Events

**File:** `p2p/foretias-node/src/communerd/p2p/events.rs`

Current events: `Connected`, `Disconnected`, `Identified`, `PingSuccess`, `DhtPeerDiscovered`, `DhtBootstrapComplete`, `GossipMessage`, `HeartbeatMessage`.

Missing: `ListenReady { multiaddr: Multiaddr }`, `RecordRetrieved { key: kad::RecordKey, records: Vec<kad::Record> }`.

### 8.4 Communerd Orchestrator

**File:** `p2p/foretias-node/src/communerd/mod.rs`

- `enable_p2p()` (line 164): Takes `listen: libp2p::Multiaddr` — must accept `Option<Multiaddr>`
- `bootstrap_dht()` (line 318): Dials bootstrap peers, sends `Bootstrap` command
- `publish_attest_willing()` (line 331): Sends `Provide` command for DHT discovery
- Missing: `register_and_discover()`, `refresh_registration()`

### 8.5 NodeConfig

**File:** `p2p/core-engine/src/config.rs`

Current fields (line 8-54): `listen_addr`, `version`, `calendar_path`, `chronon_ns`, `serialized`, `peers`, `mutually_attest_every_n`, `request_timeout_secs`, `p2p_listen`, `p2p_dial`, `dht_namespace`, `dht_bootstrap`, `collision`, `signature_algorithm`, `kem_algorithm`.

Missing: `p2p_port_range: Option<[u16; 2]>`, `known_servers: Vec<String>`, `max_discovered_peers: usize`.

---

## 9. Implementation Checklist

### Phase 2 — Rust DHT/P2P Core

- [ ] `swarm.rs`: Add `find_free_port(range: Range<u16>)` function
- [ ] `swarm.rs`: Change `build_and_spawn_swarm(listen: Multiaddr, ...)` to `Option<Multiaddr>`
- [ ] `swarm.rs`: Add `local_multiaddr: Arc<Mutex<Option<Multiaddr>>>` to `SwarmHandle`
- [ ] `swarm.rs`: Add `PutRecord`, `GetRecord` to `SwarmCommand`
- [ ] `swarm.rs`: Handle `PutRecord`/`GetRecord` in `swarm_loop`
- [ ] `swarm.rs`: Emit `ListenReady` from `NewListenAddr` event
- [ ] `events.rs`: Add `ListenReady`, `RecordRetrieved` variants
- [ ] `config.rs`: Add `p2p_port_range`, `known_servers`, `max_discovered_peers` to `NodeConfig`
- [ ] `mod.rs`: Add `register_and_discover()` method to `Communerd`
- [ ] `mod.rs`: Add `refresh_registration()` method to `Communerd`
- [ ] `mod.rs`: Change `enable_p2p()` to accept `Option<Multiaddr>`
- [ ] `main.rs`: Add `p2p_port_range`, `known_servers`, `max_discovered_peers` to `cmd_serve()` signature
- [ ] `main.rs`: Wire auto-port selection logic in `cmd_serve()`
- [ ] `main.rs`: Wire self-registration and peer discovery in `cmd_serve()`
- [ ] `main.rs`: Update `main()` match arm to pass new parameters

### Phase 3 — Peer Attestation

- [ ] Wire `DhtPeerDiscovered` into `PeerPool.add_peer()` in gossip loop
- [ ] Implement attestation interval formula in mutual attestation scheduler
- [ ] Print P2P startup info (actual port, TBID, PeerId, multiaddr)

### Phase 4 — Multi-Language

- [ ] Python CLI: Create `cli.py` matching spec
- [ ] Python PyO3: Extend bindings for auto-port, known-servers, DHT
- [ ] Java CLI: Create `Cli.java` matching spec
- [ ] Java JNI: Create `NativeBridge.java` for `foretias_core.h`

### Phase 5 — Integration Tests

- [ ] Rust 1-node + 100 peers test
- [ ] Rust ↔ Python cross-language test
- [ ] Rust ↔ Java cross-language test
- [ ] Python ↔ Java cross-language test

---

## 10. Version History

| Version | Date | Changes |
|---------|------|---------|
| v0.5.239 | 2026-05-05 | Initial spec: auto-port, known-servers, peer discovery, info command |
