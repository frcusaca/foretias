# FORETIAS HOWTO & PEER MANAGER SPEC

## Overview

Two deliverables to demonstrate Foretias ease-of-use and network resilience:

1. **HOWTO.md** — User-facing documentation proving foretias is easy and reliable
2. **start_local_peers.py** — Interactive Python script for managing dozens of local peers

---

## Deliverable 1: HOWTO.md

### Purpose
HOWTO.md is **promotional documentation** designed to persuade casual perusers that foretias is:

1. **Easy to use** — a user can start a server, stamp a message, and verify it in under 5 minutes with zero configuration beyond building the binary
2. **Safe** — tamper detection is demonstrated explicitly: a single-byte change to a stamped message causes verification to fail
3. **Reliable** — even after killing 50% of a 100-node network, previously-stamped attestations remain verifiable on surviving peers

HOWTO.md is not a technical manual. It is a guided demonstration that combines live CLI commands with narrative framing to build confidence. The actual HOWTO.md document implements this specification by providing copy-paste runnable commands and expected outputs.

### Design Goals
- Target audience: engineers who have never used foretias
- Tone: enthusiastic, confidence-building, not dry technical manual
- Commands must be copy-paste runnable
- Each section should be completable in under 5 minutes
- Each section must include both a success case and a failure case (e.g., verification with correct vs. tampered content)

### Location
`/home/hcbusy/webhash/foretias/HOWTO.md`

### Section 1: Quick Start (Single Server)
- Start one server: `foretias serve --addr 127.0.0.1:4001 --chronon-ns 1000000000`
- Stamp a message from another terminal: `foretias stamp -m "hello world" -s 127.0.0.1:4001`
- Verify with correct content → `{"valid": true}`
- Verify with wrong content → `{"valid": false}`
- All commands use the Rust binary `foretias` (not Python)

### Section 2: Peering (Many Servers)
- Point user to `integration-tests/start_local_peers.py`
- User runs script to start N peers (default 50)
- Shows `foretias stamp` against any peer's listen address
- Shows `foretias verify` against a **different** peer — proof of mutual attestation replication

### Section 3: Resilience (Kill 50%)
- Same setup as Section 2
- User types `kill-random 50%` at the script prompt
- Demonstrates verification STILL succeeds against remaining peers
- Proves network resilience to massive node failure

### Section Requirements
- Target audience: engineers who have never used foretias
- Tone: enthusiastic, confidence-building, not dry technical manual
- Commands must be copy-paste runnable
- Each section should be completable in <5 minutes

---

## Deliverable 2: start_local_peers.py

### Purpose
Interactive peer manager that spawns, monitors, and controls dozens of foretias server processes.

### Location
`/home/hcbusy/webhash/foretias/integration-tests/start_local_peers.py`

### Architecture

#### Configuration
- `--peers N` — number of peers to spawn (default: 50)
- `--port-base N` — starting RPC port (default: 5000)
- `--p2p-range start..end` — P2P port range (default: 9900..9999)
- `--chronon-ns N` — tick interval in nanoseconds (default: 1000000000 = 1s)
- `--dht-namespace NAME` — DHT namespace (default: "howto-demo")
- `--binary PATH` — path to foretias binary (auto-detect debug/release)
- `--persist-dir DIR` — base directory for peer persist paths (default: temp dir)
- `--stats-interval N` — seconds between live stat prints (default: 10; 0 disables)

#### Peer Spawning
- Each peer gets:
  - Unique RPC port (port_base + index)
  - Unique persist directory (`persist_dir/peer_<index>`)
  - Unique log file for output capture
- All peers know all other peers via `--known-servers` flags (Rust code skips self-dial)
- Peers start in batches (e.g., 10 at a time) with short delays for orderly startup
- Script captures startup output to extract: TBID, TBN, listen address, P2P multiaddr, PeerId

#### Peer Tracking (internal data structure)
Each peer tracked as:
```python
{
    "index": int,
    "tbid": str,           # hex string
    "tbn": str,            # time-being name
    "rpc_port": int,       # RPC listen port
    "p2p_port": int,       # P2P port (if captured)
    "pid": int,            # process PID
    "alive": bool,         # whether process is still running
    "persist_path": str,   # persist directory path
    "chronon_ns": int,     # chronon period
    "ticks": int,          # last known tick count (from log parsing)
    "start_time": float,   # time.time() at spawn
    "process": subprocess.Popen,
}
```

#### Continual Stats Gathering Thread
A background Python thread runs alongside the REPL, continuously monitoring all peer logs for activity and errors. This thread is extensible — initially it only tracks error counts and prints periodic summaries, but will be expanded later for richer metrics.

**Thread behavior:**
- Runs in a separate `threading.Thread` started after all peers are spawned
- Polls each peer's log file every 3 seconds using a tail-file pattern (tracks file offset per peer, reads only new bytes since last poll)
- Tracks per-peer and aggregate statistics:
  - **Error count**: lines matching error patterns (case-insensitive: "error", "ERROR", "panic", "FAILED")
  - **Warning count**: lines matching warning patterns ("warning", "WARN")
  - **Tick count**: latest tick number observed per peer
- Prints a summary to **stderr** (so as not to interfere with REPL stdin) every `--stats-interval` seconds (default 10):

```
--- Live Stats (elapsed 45s) ---
  Alive: 50/50  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)
```

After a kill event, the summary reflects the new state:

```
--- Live Stats (elapsed 120s) ---
  Alive: 25/50  |  Errors: 3  |  Warnings: 5
  Avg ticks/sec: 1.0  |  Highest tick: 118 (peer #7)
```

**Implementation details:**
- Uses `tail-file` pattern: track file position per peer, only read new bytes since last poll
- Thread-safe: uses a `threading.Lock` when updating shared stats; REPL reads stats under the same lock
- Graceful shutdown: set an `threading.Event` flag; thread exits cleanly on `shutdown`
- Stats data structure:

```python
stats = {
    "start_time": float,
    "per_peer": {
        index: {
            "errors": int,
            "warnings": int,
            "last_tick": int,
            "last_log_pos": int,   # file offset for tailing
        }
    },
    "aggregate_errors": int,
    "aggregate_warnings": int,
}
```

**Extensibility hooks:**
- The polling loop is structured so additional metrics (e.g., stamp counts, attestation counts, peer connection events) can be added by adding new regex patterns and counters
- A future `stats` REPL command can dump the full per-peer breakdown

#### Interactive REPL Commands

| Command | Description |
|---------|-------------|
| `status` | Table: index, TBID (short), RPC port, alive?, ticks |
| `stats` | Full per-peer stats breakdown (errors, warnings, ticks) |
| `kill-random N` | Kill N random alive peers. Accepts `N%` for percentage of alive peers. |
| `kill-tbid <hex>` | Kill specific peer by TBID prefix match |
| `stamp <port> <message>` | Run `foretias stamp -m "<message>" -s 127.0.0.1:<port>`, print result |
| `verify <port> <message> <stamp.json>` | Run `foretias verify -m "<message>" -F <stamp.json> -s 127.0.0.1:<port>`, print result |
| `help` | Print all commands |
| `shutdown` | Graceful shutdown of all remaining peers, print summary |
| `quit` | Same as shutdown |

#### Output Capture & Parsing
- Each peer's stdout/stderr captured to a log file
- On `status`, parse log files for tick count (look for "Tick #" patterns)
- Parse startup output for TBID, TBN, P2P info
- The stats thread also tails these same log files for continuous monitoring

#### Shutdown Summary
```
=== Session Summary ===
Peers started:    50
Peers killed:     25
Peers remaining:  25
Stamps made:      3
Verifications:    2
Duration:         12m 34s
```

### Dependencies
- Python 3.8+ (subprocess, argparse, re, random, time, json, threading)
- No external pip dependencies — stdlib only
- Requires `foretias` binary built (debug or release)

### Behavioral Requirements
- Script exits cleanly on Ctrl+C (SIGINT), killing all children and stopping the stats thread
- `kill-random` with percentage rounds down
- `stamp`/`verify` commands print the full CLI command being executed for transparency
- If a peer process has already died, `status` reflects it without errors
- `kill-tbid` with no match prints error; with multiple matches kills all matches
- Stats thread prints to stderr only, never interferes with REPL input/output

### Edge Cases
- Peer fails to start: log the failure, continue with remaining peers
- Peer dies during session: mark as dead in status, don't count for kill-random
- stamp/verify against dead peer: print the error naturally from CLI
- If no foretias binary found: print instructions to build, exit
- Stats thread handles missing log files gracefully (peer hasn't written yet)
