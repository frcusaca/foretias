# Foretias HOWTO

Foretias is a **decentralized time-integrity attestation service**. In plain terms: it cryptographically proves that a message existed at a specific tick in a specific calendar — and that calendar is replicated, verified, and signed by an entire network of peers.

This document shows you **four escalating demos** that prove foretias is:

1. **EASY** — start, stamp, verify in under 5 minutes
2. **PEERABLE** — hundreds of nodes, all auto-discovering and mutually attesting
3. **ACCESSIBLE** — connect from any Rust CLI to the swarm and query freely
4. **RESILIENT** — kill half the network and verification still works

---

## Prerequisites

### System Dependencies

```bash
sudo apt install build-essential cmake clang libsodium-dev libssl-dev
rustup install stable
```

### Full Clean Build

```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10

# Cargo builds the C11 core (via build.rs) and the full Rust workspace:
cd p2p && cargo build --workspace --release
```

The release binary lives at `p2p/target/release/foretias`.

> **Note:** The C11 core has its own CMake build (`p2p/core`) for standalone use, but it requires `liboqs` installed system-wide. The cargo build handles this automatically by cloning and building liboqs from source.

For convenience, the rest of this document uses a shell alias:

```bash
alias foretias="p2p/target/release/foretias"
```

### Running Tests

```bash
# Rust workspace tests (unit + integration) — covers C11 core via FFI
cd p2p && cargo test --workspace
```

---

## Section 1: Quick Start — Single Server

> **This section takes under 2 minutes.**

### Step 1: Start a server

Open a terminal and start a TimeFamilyServer:

```bash
foretias serve --addr 127.0.0.1:4001 --chronon-ns 1000000000
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
foretias stamp -m "hello world" -s 127.0.0.1:4001 -o /tmp/hello_world_stamp.json
```

This connects to the server over an encrypted Noise_XX channel, submits your message, and receives a **Foretis** — a cryptographically signed attestation containing the message hash, tick number, signature, and the server's unique TBID (Time-Being ID).

Inspect the stamp:

```bash
cat /tmp/hello_world_stamp.json
```

You'll see the tick number, content hash, Ed25519 signature, and the server's TBID — all in one self-contained JSON object.

### Step 3: Verify — Success case

Verify the stamp against the **same** server:

```bash
# Pass the stamp as a file:
foretias verify -m "hello world" -F /tmp/hello_world_stamp.json -s 127.0.0.1:4001

# Or inline from the shell:
foretias verify -m "hello world" -f "$(cat /tmp/hello_world_stamp.json)" -s 127.0.0.1:4001
```

Expected output:

```json
{
  "method": "remote",
  "valid": true
}
```

**That's it.** You just proved that `"hello world"` existed at a specific tick in a specific calendar, signed by a known key.

### Step 4: Verify — Failure case

Now try with the **wrong content**:

```bash
foretias verify -m "tampered message" -F /tmp/hello_world_stamp.json -s 127.0.0.1:4001
```

Expected output:

```json
{
  "method": "remote",
  "valid": false
}
```

The content hash doesn't match. The stamp is immutable. Foretias caught the tampering.

### Step 5: Verify with proof — Client-side cryptographic proof

Download the chronon from the server and verify locally without trusting the server's answer:

```bash
foretias verify-with-proof -m "hello world" -F /tmp/hello_world_stamp.json -s 127.0.0.1:4001
```

This fetches the relevant chronon record from the server, runs full cryptographic verification locally (SHA-256 hash check + Ed25519 signature verification), and prints the chronon record used for verification:

Expected output:

```json
{
  "chronon_number": 1,
  "chronon_record": {
    "aa_nonce": "...",
    "backward_foretis": "...",
    "chronon_number": 1,
    "external_attestations": [],
    "forward_foretis": "...",
    "public_key": "...",
    "signature_algorithm": "Ed25519",
    "tb_version": 0
  },
  "method": "local",
  "valid": true
}
```

The chronon_record contains the public_key that was used to verify the stamp stored in `/tmp/hello_world_stamp.json`, this server had the only functioning secret key that can produce a stamp verifiable by this public_key.

### Step 6: Prove Verification — Client-side proof

`verify-with-proof` downloads the chronon from the server and verifies locally with cryptographic proof — returning both the verification result and the chronon record:

foretias verify-with-proof -m "hello world" -F /tmp/hello_world_stamp.json -s 127.0.0.1:4001
```

This downloads the relevant calendar ticks from the server and proves the stamp is valid **without trusting the server's answer**. This is the strongest form of verification — the client does all the cryptographic work.

> **Quick start complete.** You've stamped, verified, and proven a stamp in under 2 minutes. Now let's scale.

---

## Section 2: Peering — Hundreds of Nodes

A single server is fine for local demos. But foretias is designed for **networks**. This section shows how to spin up a swarm of peers, all auto-discovering and mutually attesting each other.

### Step 1: Start the peer manager

Foretias ships with an interactive peer manager script:

```bash
python3 integration-tests/start_local_peers.py --peers 20
```

This spawns **20 foretias servers** on your local machine, each with:

- A unique identity (random Ed25519 keypair)
- A unique RPC port (5000–5019)
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
  peers                   Show peer connection distribution (min/max/avg/stdev)
  demo-cli                Print ready-to-copy-paste CLI examples
  help                    Show this help
  shutdown / quit         Graceful shutdown and summary
```

### Step 2: Check the network

```
foretias> status
```

You'll see a table of all alive peers with their TBIDs, ports, and tick counts:

```
Idx              TBID    Port   Alive   Ticks
----------------------------------------------
   0  23a2508b9a5d013d    5000     YES       5
   1  48a4581832d48131    5001     YES       5
   2  436690813d28c006    5002     YES       4
   ...
  19  ab12cd34ef567890    5019     YES       5
```

A background stats thread prints live summaries to stderr every 10 seconds:

```
--- Live Stats (elapsed 45s) ---
  Alive: 20/20  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)
```

### Step 3: Check peer connections

```
foretias> peers
```

This queries each alive peer for its connection count and prints a distribution:

```
Peer count distribution (20 queried, 0 unreachable):
  Min: 2
  Max: 12
  Avg: 6.3
  StDev: 2.87
```

This tells you how well-connected the P2P mesh is. Higher average = better replication.

### Step 4: Stamp on one peer (via REPL)

```
foretias> stamp 5000 "this is a network test"
```

Output:

```
  $ foretias stamp -m "this is a network test" -s 127.0.0.1:5000
{
  "tick_number": 12,
  "tbn": "tf-23a2508b9a5d013d",
  "signature_algorithm": "Ed25519",
  ...
}
  Saved to: /tmp/foretias_stamp_1778444763.json
```

### Step 5: Verify on a DIFFERENT peer (via REPL)

Stamp on peer #0, verify on peer #10:

```
foretias> sleep 5
foretias> verify 5010 "this is a network test" /tmp/foretias_stamp_*.json
```

When peers mutually attest each other, their calendars cross-reference. A stamp from one peer becomes verifiable from another. This is the foundation of decentralized trust.

> **Peering complete.** 20 nodes, zero configuration, auto-discovery, mutual attestation. Now let's access the swarm from a standalone CLI.

---

## Section 3: External CLI Access — Query the Swarm

The peer manager REPL is convenient for demos. But the real power is connecting **any** `foretias` CLI instance to any peer in the swarm — just like a production client would.

Keep the peer manager running from Section 2 (or start a fresh one):

```bash
# Terminal 1 — swarm manager:
python3 integration-tests/start_local_peers.py --peers 10
```

Open a **second terminal** and use the `foretias` binary directly. Every peer listens on its own port (5000–5009 by default). Connect to any of them:

### Step 1: Stamp against the swarm

```bash
# Stamp against peer #0 (port 5000):
foretias stamp -m "external CLI test" -s 127.0.0.1:5000 -o /tmp/swarm_stamp.json
```

The CLI connects over Noise_XX, submits the message, and receives a Foretis. The stamp is recorded in peer #0's calendar.

### Step 2: Verify against a DIFFERENT peer

Wait a moment for the stamp to propagate via mutual attestation, then verify against peer #5 (port 5005):

```bash
sleep 3
foretias verify -m "external CLI test" -F /tmp/swarm_stamp.json -s 127.0.0.1:5005
```

Expected output:

```json
{
  "method": "remote",
  "valid": true
}
```

The stamp was created on peer #0 but verified on peer #5. The attestation was replicated through the P2P mesh.

### Step 3: Verify with proof against the swarm

Client-side verification — download the chronon from peer #7 and verify locally:

```bash
foretias verify-with-proof -m "external CLI test" -F /tmp/swarm_stamp.json -s 127.0.0.1:5007
```

This fetches the chronon record from peer #7, performs full cryptographic verification locally, and returns the chronon record alongside the result.

### Step 4: Prove Verification against the swarm

Client-side proof — fetch the calendar slice from peer #7 and verify locally:

```bash
foretias verify-with-proof -m "external CLI test" -F /tmp/swarm_stamp.json -s 127.0.0.1:5007
```

The CLI downloads the relevant calendar ticks from peer #7, verifies the signature chain locally, and confirms the stamp's integrity — without trusting any single peer's answer.

### Step 5: Stamp from a file

```bash
echo "Important document content" > /tmp/document.txt
foretias stamp -M /tmp/document.txt -s 127.0.0.1:5003 -o /tmp/doc_stamp.json
```

### Step 6: Print CLI examples from the REPL

Back in the peer manager REPL, get ready-to-copy CLI commands:

```
foretias> demo-cli
```

This prints all the common CLI commands with the correct ports for your current swarm.

> **External CLI complete.** You can connect any `foretias` binary to any peer in the swarm, stamp, verify, and prove verification — exactly as a production client would.

---

## Section 4: Resilience — Kill 50% of the Network

Foretias is designed to survive catastrophic failure. Let's prove it.

### Step 1: Start a swarm and stamp from the external CLI

```bash
# Terminal 1 — peer manager:
python3 integration-tests/start_local_peers.py --peers 20

# Terminal 2 — stamp from external CLI:
foretias stamp -m "resilience test message" -s 127.0.0.1:5000 -o /tmp/resilience_stamp.json
```

Wait for the stamp to propagate:

```bash
sleep 5
```

### Step 2: Verify from the external CLI before killing

```bash
# Verify on peer #0:
foretias verify -m "resilience test message" -F /tmp/resilience_stamp.json -s 127.0.0.1:5000

# Verify on peer #10 (different peer):
foretias verify -m "resilience test message" -F /tmp/resilience_stamp.json -s 127.0.0.1:5010
```

Both should return `{"valid": true}`.

### Step 3: Kill half the network

In the peer manager REPL:

```
foretias> kill-random 50%
```

10 out of 20 peers are terminated. The stats thread immediately reflects the new state:

```
--- Live Stats (elapsed 60s) ---
  Alive: 10/20  |  Errors: 3  |  Warnings: 5
  Avg ticks/sec: 1.0  |  Highest tick: 58 (peer #7)
```

### Step 4: Verify still works from the external CLI

```bash
# Try peer #0 (might be dead):
foretias verify -m "resilience test message" -F /tmp/resilience_stamp.json -s 127.0.0.1:5000
# → Connection refused (if peer #0 was killed)

# Try peer #15 (likely alive):
foretias verify -m "resilience test message" -F /tmp/resilience_stamp.json -s 127.0.0.1:5015
```

Expected output from a surviving peer:

```json
{
  "method": "remote",
  "valid": true
}
```

The stamp was created on peer #0. Peer #0 may have been killed. But the attestation was replicated to peer #15 (and others) via mutual attestation. **The calendar survives because it's decentralized.**

### Step 5: Kill more — prove the limit

```
foretias> kill-random 7
```

Only 3 peers remain. Verification against any surviving peer still works — as long as at least one peer has the attestation in its calendar.

```bash
# Check which peers are alive:
foretias> status

# Try verify on each surviving peer until one responds:
foretias verify -m "resilience test message" -F /tmp/resilience_stamp.json -s 127.0.0.1:5007
```

### Step 6: Clean shutdown

```
foretias> shutdown
```

```
=== Session Summary ===
Peers started:    20
Peers killed:     17
Peers remaining:   3
Stamps made:       1
Verifications:     2
Duration:          3m 15s
```

> **Resilience complete.** 85% of the network is dead. The data survives. The stamps are still verifiable. This is what decentralized trust looks like.

---

## Appendix A: Peer Manager Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--peers N` | 50 | Number of peers to spawn |
| `--port-base N` | 5000 | Starting RPC port (peer #0 = 5000, peer #1 = 5001, ...) |
| `--p2p-range` | 9900..9999 | P2P port range for libp2p |
| `--chronon-ns N` | 1000000000 | Tick interval in nanoseconds (1s = fast demo) |
| `--stats-interval N` | 10 | Seconds between live stat prints (0 disables) |
| `--dht-namespace` | howto-demo | DHT namespace for Kademlia isolation |
| `--persist-dir DIR` | temp dir | Where peer calendars are stored |
| `--binary PATH` | auto-detect | Path to the `foretias` binary |

### Common Launch Patterns

```bash
# Quick demo with 5 peers:
python3 integration-tests/start_local_peers.py --peers 5

# Large swarm with no stats noise:
python3 integration-tests/start_local_peers.py --peers 100 --stats-interval 0

# Fast ticking (500ms chronon):
python3 integration-tests/start_local_peers.py --peers 10 --chronon-ns 500000000
```

---

## Appendix B: REPL Commands

| Command | Description |
|---------|-------------|
| `status` | Peer table: index, TBID (short), port, alive, ticks |
| `stats` | Full per-peer breakdown: errors, warnings, ticks |
| `peers` | Peer connection distribution (min/max/avg/stdev) |
| `sleep N` | Pause for N seconds (stats thread still runs) |
| `kill-random N\|N%` | Kill N random alive peers, or N% of alive |
| `kill-tbid <hex>` | Kill peers matching TBID prefix |
| `stamp <port> <msg>` | Stamp a message via peer at the given port |
| `verify <port> <msg> <file>` | Verify a stamp via peer at the given port |
| `demo-cli` | Print ready-to-copy-paste CLI examples for your swarm |
| `help` | Show this command list |
| `shutdown` / `quit` | Graceful shutdown with session summary |

---

## Appendix C: External CLI Quick Reference

When the peer manager is running, any `foretias` CLI can connect to any peer using its port:

```bash
# Stamp (connect to peer #N on port 5000+N):
foretias stamp -m "message" -s 127.0.0.1:5000 -o /tmp/stamp.json

# Verify (server-side — can be any peer):
foretias verify -m "message" -F /tmp/stamp.json -s 127.0.0.1:5005

# Verify with proof (client-side — downloads chronon, verifies cryptographically):
foretias verify-with-proof -m "message" -F /tmp/stamp.json -s 127.0.0.1:5005

# Stamp from file:
foretias stamp -M /tmp/document.txt -s 127.0.0.1:5000 -o /tmp/stamp.json
```

All connections use the encrypted Noise_XX protocol. No raw JSON-RPC is exposed.

---

*Foretias — time integrity that survives.*
