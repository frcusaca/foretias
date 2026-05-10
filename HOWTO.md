# Foretias HOWTO

> Prove it works. Prove it survives. Prove it's trivial to use.

Foretias is a **decentralized time-integrity attestation service**. In plain terms: it cryptographically proves that a message existed at a specific tick in a specific calendar — and that calendar is replicated, verified, and signed by an entire network of peers.

This document shows you **three escalating demos** that prove foretias is:

1. **EASY** — start, stamp, verify in under 5 minutes
2. **PEERABLE** — hundreds of nodes, all auto-discovering and auto-attesting
3. **RESILIENT** — kill half the network and verification still works

---

## Prerequisites

See [README.md](README.md) for full installation and build instructions. In short, you need the `foretias` binary:

```bash
cd p2p && cargo build
```

The binary lives at `p2p/target/debug/foretias` (or `p2p/target/release/foretias` for a release build).

---

## Section 1: Quick Start — Single Server

> **Before you begin:** See [README.md](README.md) for full installation and build instructions. You need the `foretias` binary (`p2p/target/debug/foretias` or `p2p/target/release/foretias`).

This section takes **under 2 minutes**.

### Step 1: Start a server

Open a terminal and start a TimeFamilyServer:

```bash
p2p/target/debug/foretias serve --addr 127.0.0.1:4001 --chronon-ns 1000000000
```

You'll see output like:

```
Foretias TimeFamilyServer starting...
  Listen : 127.0.0.1:4001
  TBN    : tf-a1b2c3d4e5f6...
  TBID   : a1b2c3d4e5f6...
  Chronon: 1 second
```

The server is now ticking — creating new calendar entries every second. Leave this terminal running.

### Step 2: Stamp a message

Open a **second terminal** and stamp a message:

```bash
p2p/target/debug/foretias stamp -m "hello world" -s 127.0.0.1:4001 -o stamp.json
```

This connects to the server over an encrypted Noise_XX channel, submits your message, and receives a **Foretis** — a cryptographically signed attestation containing the message hash, tick number, signature, and the server's unique TBID (Time-Being ID).

The stamp is saved to `stamp.json`. Inspect it:

```bash
cat stamp.json
```

You'll see the tick number, content hash, Ed25519 signature, and the server's TBID — all in one self-contained JSON object.

### Step 3: Verify — Success case

Verify the stamp against the **same** server:

```bash
p2p/target/debug/foretias verify -m "hello world" -F stamp.json -s 127.0.0.1:4001
```

Expected output:

```json
{
  "method": "local",
  "valid": true
}
```

**That's it.** You just proved that the message `"hello world"` existed at a specific tick in a specific calendar, signed by a known key.

### Step 4: Verify — Failure case

Now try with the **wrong content**:

```bash
p2p/target/debug/foretias verify -m "tampered message" -F stamp.json -s 127.0.0.1:4001
```

Expected output:

```json
{
  "method": "local",
  "valid": false
}
```

The content hash doesn't match. The stamp is immutable. Foretias caught the tampering.

> **Quick start complete.** You've stamped and verified in under 2 minutes. Now let's scale.

---

## Section 2: Peering — Hundreds of Nodes

A single server is fine for local demos. But foretias is designed for **networks**. This section shows how to spin up hundreds of peers, all auto-discovering and auto-attesting each other.

### Step 1: Start the peer manager

Foretias ships with an interactive peer manager script:

```bash
python3 integration-tests/start_local_peers.py --peers 100
```

This spawns **100 foretias servers** on your local machine, each with:

- A unique identity (random Ed25519 keypair)
- A unique RPC port (5000–5099)
- A unique calendar
- Knowledge of all other peers (via `--known-servers`)

The script enters an interactive REPL:

```
foretias> help

Commands:
  status                  Show peer status table
  stats                   Show detailed per-peer stats (errors, warnings, ticks)
  sleep <seconds>         Pause for N seconds
  kill-random N|N%        Kill N random alive peers (or N% of alive)
  kill-tbid <hex-prefix>  Kill peers matching TBID prefix
  stamp <port> <message>  Stamp a message via the given peer port
  verify <port> <msg> <stamp.json>  Verify a stamp via the given peer port
  help                    Show this help
  shutdown / quit         Graceful shutdown and summary
```

### Step 2: Check the network

```
foretias> status
```

You'll see a table of all 100 peers with their TBIDs, ports, and tick counts:

```
Idx              TBID    Port   Alive   Ticks
----------------------------------------------
   0  23a2508b9a5d013d    5000     YES       5
   1  48a4581832d48131    5001     YES       5
   2  436690813d28c006    5002     YES       4
   ...
  99  ab12cd34ef567890    5099     YES       5
```

The stats thread runs in the background, printing live summaries to stderr every 10 seconds:

```
--- Live Stats (elapsed 45s) ---
  Alive: 100/100  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)
```

### Step 3: Stamp on one peer

```
foretias> stamp 5000 "this is a network test"
```

Output:

```
  $ foretias stamp -m this is a network test -s 127.0.0.1:5000
{
  "tick_number": 12,
  "tbn": "tf-23a2508b9a5d013d",
  "signature_algorithm": "Ed25519",
  ...
}
  Saved to: /tmp/foretias_stamp_1778444763.json
```

### Step 4: Verify on a DIFFERENT peer

This is where foretias earns its stripes. Stamp on peer #0, verify on peer #50:

```
foretias> sleep 5
foretias> verify 5050 "this is a network test" /tmp/foretias_stamp_*.json
```

When peers auto-attest each other, their calendars cross-reference. A stamp from one peer becomes verifiable from another. This is the foundation of decentralized trust.

> **Peering complete.** 100 nodes, zero configuration, auto-discovery, auto-attestation. Try scaling to 200 with `--peers 200`.

---

## Section 3: Resilience — Kill 50% of the Network

Foretias is designed to survive catastrophic failure. Let's prove it.

### Step 1: Start the same network

```bash
python3 integration-tests/start_local_peers.py --peers 50
```

### Step 2: Stamp a message

```
foretias> stamp 5000 "resilience test message"
foretias> sleep 5
```

Wait for the stamp to propagate via auto-attestation.

### Step 3: Kill half the network

```
foretias> kill-random 50%
```

The script randomly terminates 25 out of 50 peers. The stats thread immediately reflects the new state:

```
--- Live Stats (elapsed 60s) ---
  Alive: 25/50  |  Errors: 3  |  Warnings: 5
  Avg ticks/sec: 1.0  |  Highest tick: 58 (peer #7)
```

### Step 4: Verify still works

```
foretias> status
foretias> verify 5020 "resilience test message" /tmp/foretias_stamp_*.json
```

Verification against a **surviving** peer succeeds:

```json
{
  "method": "local",
  "valid": true
}
```

The stamp was created on peer #0. Peer #0 may have been killed. But the attestation was replicated to peer #20 (and others) via auto-attestation. **The calendar survives because it's decentralized.**

### Step 5: Kill more — prove the limit

Try killing 80%:

```
foretias> kill-random 30
```

Only 15 peers remain. Verification against any surviving peer still works — as long as at least one peer has the attestation in its calendar.

### Step 6: Clean shutdown

```
foretias> shutdown
```

```
=== Session Summary ===
Peers started:    50
Peers killed:     45
Peers remaining:   5
Stamps made:       1
Verifications:     2
Duration:          2m 15s
```

> **Resilience complete.** 50% (or 90%) of the network is dead. The data survives. The stamps are still verifiable. This is what decentralized trust looks like.

---

## Appendix: Peer Manager Reference

| Flag | Default | Description |
|------|---------|-------------|
| `--peers N` | 50 | Number of peers to spawn |
| `--port-base N` | 5000 | Starting RPC port |
| `--p2p-range` | 9900..9999 | P2P port range |
| `--chronon-ns N` | 1000000000 | Tick interval (1s = fast demo) |
| `--stats-interval N` | 10 | Seconds between stat prints (0 disables) |
| `--dht-namespace` | howto-demo | DHT namespace |
| `--persist-dir DIR` | temp dir | Where peer calendars are stored |

### REPL Commands

| Command | Description |
|---------|-------------|
| `status` | Peer table: index, TBID, port, alive, ticks |
| `stats` | Full per-peer breakdown: errors, warnings, ticks |
| `sleep N` | Pause for N seconds |
| `kill-random N\|N%` | Kill N random peers, or N% of alive |
| `kill-tbid <hex>` | Kill peers matching TBID prefix |
| `stamp <port> <msg>` | Stamp via peer at given port |
| `verify <port> <msg> <file>` | Verify stamp via peer at given port |
| `shutdown` / `quit` | Graceful shutdown with summary |

---

*Foretias — time integrity that survives.*
