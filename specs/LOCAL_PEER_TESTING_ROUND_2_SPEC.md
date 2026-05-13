# LOCAL_PEER_TESTING_ROUND_2_SPEC.md

**Spec: Demo Peer Manager Fixes**
**Status: DRAFT**
**Date: 2026-05-11**

---

## Problem Statement

The local peer demo (`integration-tests/start_local_peers.py`) has four issues that prevent it from working out of the box:

### 1. Servers appear not to be ticking (tick count stuck at 0)

The `StatsGatherer` background thread polls server logs for tick advances using `TICK_RE = re.compile(r"Tick\s+#?(\d+)")`. This regex never matches because:

- The daemon tick loop logs at `debug!` level (`chronomatter/mod.rs:350`) which is filtered out by the default `info` log level
- Even if debug were enabled, the format `tick = N` doesn't match `Tick #N`

**Result**: All peers show `Ticks = 0` regardless of how long they've been running, making the demo appear broken.

**Fix**: Add an `info!` log line in `daemon_tick()` with format `Tick #<n>` to match the existing regex. Location: `p2p/core-engine/src/chronomatter/mod.rs` ~line 388.

### 2. Stamp command fails with "Connection refused"

The `stamp` command in the REPL invokes `foretias stamp -m <message> -s 127.0.0.1:<port>`. This fails because:

- The stamp command uses `subprocess.run()` (line 398) which spawns a NEW process
- This new process connects via Noise handshake (see `main.rs:599-605`) which requires the server to accept TCP connections
- The server DOES listen on the RPC port (`TimeFamilyServer::start_tcp` at `server/mod.rs:193`)
- The "Connection refused" error (code 111) indicates the server process either crashed or hasn't bound the port yet

**Root cause**: The 3-second startup wait (line 135) may be insufficient for all peers to bind their ports, especially with 50-100 peers. Additionally, any peer that crashes during startup will permanently refuse connections.

**Fix**: Add a port-probe loop before stamping that waits up to 10s for the server to accept connections.

### 3. Live stats output obscures the REPL prompt

The `StatsGatherer` thread prints to `sys.stderr` every 10 seconds. Python's `input()` also writes the prompt to `sys.stderr`. When the stats thread prints while the REPL is waiting for input, the output interleaves with the prompt, corrupting the display.

**Current behavior**:
```
--- Live Stats (elapsed 45s) ---
  Alive: 100/100  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)foretias> st
```

**Desired behavior**: After printing stats, redraw the prompt on a clean line:
```
--- Live Stats (elapsed 45s) ---
  Alive: 100/100  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)

foretias> st
```

**Fix**: Use ANSI escape sequences to clear the line after stats output and add a blank line before the next prompt. Since the stats thread and REPL share stderr, the simplest approach is to have the stats thread write a newline + carriage-return sequence that forces the cursor to a new line.

### 4. No peer connection distribution statistics

The demo spawns many peers but provides no visibility into how well the P2P mesh is connected. Each server should report how many peers it's tracking.

**Fix**: Add a `peers` command to the REPL that queries each alive peer for its peer count and prints min/max/avg/stdev statistics. Use the `get_peers` JSON-RPC method (or a new `status` method) to fetch peer counts.

---

## Design Decisions

### Tick Log Format

Add to `chronomatter/mod.rs`, `daemon_tick()`, after `notify_observer()`:
```rust
info!(tick = tick, "Tick #{}", tick);
```

This matches the existing `TICK_RE` regex in Python. No Python changes needed.

### Stamp Port Probe

In `cmd_stamp()`, before spawning the stamp process, probe the target port:
```python
def wait_for_server(addr, port, timeout=10):
    start = time.time()
    while time.time() - start < timeout:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(1)
            sock.connect((addr, port))
            sock.close()
            return True
        except (socket.error, OSError):
            time.sleep(0.5)
    return False
```

If the probe fails, print a warning and proceed (the stamp may still succeed if the server starts mid-probe).

### Prompt Redraw

Two approaches:
1. **Stats thread approach**: After printing stats, write `\n\r` to stderr to force cursor to a new clean line
2. **REPL approach**: After any command that takes time (sleep, stamp, verify), print the prompt on a fresh line

Chosen approach: **Stats thread**. Write the final stats line as `msg + "\n\n"` (two newlines) instead of `msg + "\n"`. The extra newline creates visual separation between the stats block and whatever follows.

### Peer Distribution Statistics

Query each alive peer via a simple TCP connection + JSON-RPC call to `get_calendar_slice` (or a new `status` endpoint) to get peer count. 

Simpler approach: Parse the log files for peer connection events. Look for `communerd: peer added` and `communerd: DHT-discovered peer added to pool` log lines.

Even simpler: Add a `status` JSON-RPC method that returns basic server info including peer count. This is the cleanest approach.

---

## Success Criteria

1. `status` command shows advancing tick counts after waiting 3+ chronon intervals
2. `stamp <port> <message>` succeeds against any alive peer
3. Stats output doesn't corrupt the prompt line
4. `peers` command shows connection distribution (min/max/avg/stdev of peer counts)
5. HOWTO.md updated with working examples
