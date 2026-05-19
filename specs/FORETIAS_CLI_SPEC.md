# Foretias CLI Specification

This document defines the canonical CLI interface for the Foretias Time Integrity Attestation Service.
All language implementations (Rust, Python, Java) must conform to this spec.

---

## 1. Commands

| Command | Description |
|---------|-------------|
| `serve` | Start the TimeFamilyServer |
| `stamp` | Stamp content via TimeFamilyServer |
| `verify` | Verify content against a Foretis (server-side) |
| `verify-with-proof` | Download chronon and verify locally with cryptographic proof |
| `inspect` | Inspect external attestations in a persisted calendar |

---

## 2. `serve` Command

Start the TimeFamilyServer. This is the primary daemon that runs a Foretias node.

```
foretias serve [OPTIONS]
```

### 2.1 Server Identity Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--addr <ADDR>` | `-a` | `127.0.0.1:4001` | JSON-RPC listen address (host:port) |
| `--chronon-ns <NS>` | `-c` | `60000000000` | Chronon period in nanoseconds (60s default) |
| `--persist-path <PATH>` | — | (none) | Persist calendar to this directory |
| `--start-dormant` | — | false | Start in dormant (verify-only) mode. Requires `--persist-path`. |

### 2.2 P2P / DHT Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--p2p-listen <MULTIADDR>` | — | (none) | libp2p listen multiaddr (e.g. `/ip4/0.0.0.0/tcp/9901`) |
| `--p2p-port-range <LOW..HIGH>` | — | (none) | Auto-select free port from range. Mutually exclusive with `--p2p-listen`. Default range: `9900..9999`. |
| `--p2p-dial <MULTIADDR>` | — | (none) | libp2p peer multiaddr to dial at startup. Repeatable. |
| `--known-server <IP:PORT>` | `-k` | (none) | Known server address for self-registration. Repeatable. Node dials known servers, registers its own address, and discovers peers via DHT. |
| `--dht-namespace <STRING>` | — | `mainnet` | DHT namespace for Kademlia protocol isolation |
| `--dht-bootstrap <MULTIADDR>` | — | (none) | DHT bootstrap peer multiaddr. Repeatable. Legacy mode — use `--known-server` for auto-discovery. |
| `--max-discovered-peers <N>` | — | `13` | Maximum number of peers to discover and attesta
