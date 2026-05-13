# Foretias

**Free, Open-source Resilient Time Integrity Attestation Service**

**Terminology:** Prose shorthand is precisely `PtP` (point-to-point) and `P2P` (peer-to-peer). Code identifiers use exact-case or all-lowercase: `PtPClient`, `P2PClient`, `make_ptp_connection`, `make_p2p_connection`. No other capitalizations permitted.

Foretias makes digital timestamping backdate-proof by cryptographic construction. Each tick of a time being's calendar has its own Ed25519 keypair, and when a tick advances, the previous private key is disabled. A Foretis stamped at tick *n* cannot be forged from the past.

## Installation

```bash
# Build from source
cd p2p && cargo build --release
# Binary: p2p/target/release/foretias
```

## Quick Start

### Start a Server

```bash
foretias serve --addr 127.0.0.1:4001 --chronon-ns 60000000000
```

### Stamp a Message

```bash
foretias stamp --message "hello world" --server 127.0.0.1:4001
```

### Verify a Stamp

```bash
foretias verify --message "hello world" --foretis '{"tick_number":...}' --server 127.0.0.1:4001
```

## CLI Reference

### serve

Start a TimeFamilyServer with optional P2P peers and persistence.

```bash
# Basic server
foretias serve --addr 127.0.0.1:4001 --chronon-ns 60000000000

# With P2P peers and DHT discovery
foretias serve --addr 127.0.0.1:4001 \
  --known-servers 127.0.0.1:4002 \
  --dht-namespace mainnet \
  --p2p-listen /ip4/0.0.0.0/tcp/9901

# With persistence
foretias serve --addr 127.0.0.1:4001 --persist-path /tmp/cal

# Dormant (verify-only) mode
foretias serve --addr 127.0.0.1:4001 --start-dormant --persist-path /tmp/cal
```

| Flag | Default | Description |
|------|---------|-------------|
| `--addr`, `-a` | `127.0.0.1:4001` | Listen address |
| `--chronon-ns`, `-c` | `60000000000` | Chronon period in nanoseconds (60s) |
| `--persist-path` | — | Persist calendar to directory |
| `--start-dormant` | — | Start in verify-only mode (requires `--persist-path`) |
| `--peer` | — | Peer address for mutual attestation (repeatable) |
| `--mutually-attest-every-chronons` | `1` | Mutual attestation frequency in chronons |
| `--request-timeout-secs` | `5` | RPC request timeout in seconds |
| `--p2p-listen` | — | libp2p listen multiaddr (e.g. `/ip4/0.0.0.0/tcp/9901`) |
| `--p2p-port-range` | `9900..9999` | Port range for auto-selection when `--p2p-listen` omitted |
| `--p2p-dial` | — | libp2p peer multiaddr to dial (repeatable) |
| `--known-servers`, `-k` | — | Known server for DHT self-registration (repeatable) |
| `--dht-namespace` | `mainnet` | DHT namespace for Kademlia protocol isolation |
| `--dht-bootstrap` | — | DHT bootstrap peer multiaddr (repeatable, legacy) |
| `--max-discovered-peers` | `13` | Maximum peers to auto-discover from DHT |

### stamp

Stamp content against a running server.

```bash
# Inline message
foretias stamp --message "hello world" --server 127.0.0.1:4001

# From file, write output to file
foretias stamp --message-file myfile.txt --stamp-output stamp.json --server 127.0.0.1:4001
```

| Flag | Default | Description |
|------|---------|-------------|
| `--message`, `-m` | — | Message to stamp |
| `--message-file`, `-M` | — | Read message from file |
| `--stamp-output`, `-o` | stdout | Write stamp output to file |
| `--server`, `-s` | `127.0.0.1:4001` | Server address |

### verify

Verify content against a Foretis (server-side verification).

```bash
foretias verify --message "hello world" --foretis '{"tick_number":...}' --server 127.0.0.1:4001
```

| Flag | Default | Description |
|------|---------|-------------|
| `--message`, `-m` | — | Message to verify |
| `--message-file`, `-M` | — | Read message from file |
| `--foretis`, `-f` | — | Foretis JSON inline |
| `--foretis-file`, `-F` | — | Read Foretis from file |
| `--verify-output`, `-o` | stdout | Write verify output to file |
| `--server`, `-s` | `127.0.0.1:4001` | Server address |

### prove-verification

Fetch a calendar slice from a remote TimeBeing and verify locally (client-side proof).

```bash
foretias prove-verification --message "hello world" --foretis '{"tick_number":...}' --server 127.0.0.1:4001
```

| Flag | Default | Description |
|------|---------|-------------|
| `--message`, `-m` | — | Message to verify |
| `--message-file`, `-M` | — | Read message from file |
| `--foretis`, `-f` | — | Foretis JSON inline |
| `--foretis-file`, `-F` | — | Read Foretis from file |
| `--proof-output`, `-o` | stdout | Write proof output to file |
| `--server`, `-s` | `127.0.0.1:4001` | Remote TimeBeing server address |

### inspect-attestations

Offline verification of external attestations in a persisted calendar. Re-runs signature, hash, and echo verification on every `ExternalAttestation`. Exits 0 if all valid, 1 if any invalid.

```bash
foretias inspect-attestations --calendar /tmp/cal/calendar.json
```

| Flag | Description |
|------|-------------|
| `--calendar`, `-c` | Path to calendar JSON file |

## Architecture

Foretias decomposes into three time beings coordinated by an orchestrator:

| Component | Role |
|-----------|------|
| **Chronomatter** (*Chronos fidelius authenticus*) | Autonomous ticking, stamping, verification |
| **Calendar** (*Chronos fidelius grapha*) | Calendar data, persistence, mutual attestation scheduling |
| **Communerd** (*Chronos fidelius locutus*) | All P2P communication — DHT, gossipsub, peer discovery |

Intra-family communication uses direct method calls. Only Communerd communicates with extra-family peers.

The `TimeFamilyServer` holds `Arc<Chronomatter>`, `Arc<Calendar>`, and `Option<Arc<Communerd>>`, exposing JSON-RPC and HTTP endpoints for stamp, verify, and calendar queries.

## Tech Stack

- **C11 core** — Verified cryptographic primitives (Ed25519 via libsodium, SHA-256, BLAKE3, Noise protocol, Merkle trees, FROST)
- **foretias-core** — Rust safe wrappers over C11 FFI (bindgen), domain types, CryptoServer trait
- **foretias-node** — Server, CLI binary, Communerd (libp2p), calendar store, metrics
- **foretias-python** — PyO3 Python bindings (produces `foretias-p2p` pip package)
- **foretias-java** — JNI bindings for Java

## Current Features

- **v0.1** — Local server with JSON-RPC stamp/verify (C11 core + Rust + PyO3)
- **v0.2** — P2P mutual attestation between statically configured peers
- **v0.3** — DHT peer discovery via Kademlia with private namespace

## Development

### Build

```bash
# C11 core
cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build

# Rust workspace
cd p2p && cargo build --workspace

# Python bindings
cd p2p/foretias-python && maturin develop

# Python shim package
pip install -e .
```

### Test

```bash
# C11 core tests
cd p2p/core/build && ctest --output-on-failure

# Rust workspace tests
cd p2p && cargo test --workspace

# Python shim tests
python -m pytest tests/ -v
```

The project uses the `alpha` branch as the center of development.

## Appendix

The classification for time beings belongs to this branch of the **Artificialia** domain.

- Family: **Chronosidae**
- Subfamily: **Chronosinae**
- Tribe: **Chronosini**
- Subtribe: **Chronosina**
- Genus: **Chronos**
- Species: **Chronos fidelius**
- Subspecies:
  - Chronomatter: **Chronos fidelius authenticus**
  - Calendar: **Chronos fidelius grapha**
  - Time Family: **Chronos fidelius adunatrix**
  - Communerd: **Chronos fidelius locutus**
  - Inquirer: TBD

## License

This project is licensed under the [BSD 3-Clause Clear License](LICENSE).
