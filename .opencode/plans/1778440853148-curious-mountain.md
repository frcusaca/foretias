# HOWTO v1 — User Documentation & Peer Manager Script

**Random differentiator:** `4827`
**Worktree path:** `/home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
**Branch:** `howto-peer-manager`

---

## Deliverables

1. **`integration-tests/start_local_peers.py`** — Interactive Python peer manager (stdlib-only)
2. **`HOWTO.md`** — 3-section user documentation proving ease-of-use and reliability

---

## Phase 1: `start_local_peers.py`

### Task 1.1 — Script skeleton + CLI args
- Create `integration-tests/start_local_peers.py`
- `argparse`: `--peers N` (default 50), `--port-base N` (default 5000), `--p2p-range`, `--chronon-ns` (default 1s), `--dht-namespace`, `--binary`, `--persist-dir`, `--stats-interval N` (default 10s, 0 disables)
- `find_foretias_binary()`: check `p2p/target/release/foretias`, fallback `debug/`

### Task 1.2 — Peer spawning
- `spawn_peers()`: batch-spawn subprocesses via `subprocess.Popen`, each with unique RPC port, persist dir, log file
- All peers get `--known-servers` pointing to every other peer (Rust skips self-dial)
- `parse_peer_startup(log_path)`: regex-extract TBID, TBN, listen addr, P2P info from startup output
- Per-peer tracking dict: index, tbid, tbn, rpc_port, pid, alive, persist_path, process, ticks, start_time

### Task 1.3 — Continual Stats Gathering Thread
A background Python thread continuously tails all peer logs, tracking errors, warnings, and tick counts. This thread is extensible — designed to be expanded later with richer metrics.

- Runs in a separate `threading.Thread` started after all peers spawn
- Polls each peer's log file every 3 seconds using a tail-file pattern (tracks file offset per peer, only reads new bytes)
- Per-peer counters: `errors`, `warnings`, `last_tick`, `last_log_pos`
- Prints a live summary to **stderr** (not interfering with REPL stdin) every `--stats-interval` seconds (default 10):

```
--- Live Stats (elapsed 45s) ---
  Alive: 50/50  |  Errors: 0  |  Warnings: 2
  Avg ticks/sec: 1.0  |  Highest tick: 47 (peer #12)
```

- Thread-safe: shared stats protected by a `threading.Lock`; REPL `status` command reads under the same lock
- Graceful shutdown via `threading.Event`; thread exits cleanly on `shutdown`
- New regex patterns/counters can be added later without architectural changes
- Add a `stats` REPL command to dump full per-peer breakdown

### Task 1.4 — Status monitoring
- `check_peer_alive(peer)`: `os.kill(pid, 0)`
- `parse_tick_count(log_path)`: scan log for latest tick
- `status()`: formatted table — index, TBID prefix, port, alive, ticks (also pulls from stats thread data)

### Task 1.4 — Kill commands
- `kill_random(n)`: `random.sample()` from alive peers, `process.terminate()`, mark dead
- Accept percentage syntax: `kill-random 50%` → kills floor(50% of alive)
- `kill_tbid(prefix)`: filter by TBID prefix match, kill all matches
- Edge cases: no alive peers, no matches, already-dead peers excluded

### Task 1.5 — Stamp/Verify convenience
- `stamp(port, message)`: runs `foretias stamp -m "..." -s 127.0.0.1:<port>`, prints command + output
- `verify(port, message, stamp_file)`: runs `foretias verify ...`, prints command + output

### Task 1.6 — REPL loop
- Prompt: `foretias> `
- Commands: `status`, `kill-random N|N%`, `kill-tbid <hex>`, `stamp <port> <msg>`, `verify <port> <msg> <file>`, `help`, `shutdown`, `quit`
- SIGINT/Ctrl+C handler: clean shutdown of all children
- Shutdown summary: peers started, killed, remaining, duration

### Task 1.7 — Test the script
- Build binary: `cd p2p && cargo build`
- Run with `--peers 5`, test all REPL commands end-to-end
- Verify status updates after kills, stamp/verify work against live peers

---

## Phase 2: `HOWTO.md`

### Task 2.1 — Write document
- Create `HOWTO.md` at repo root
- **Intro**: What foretias is, why it matters, what this doc proves
- **Section 1 — Quick Start**: Single server, stamp, verify success, verify failure (wrong content → `{"valid": false}`)
- **Section 2 — Peering**: Run `start_local_peers.py --peers 100`, stamp on one peer, verify on another peer
- **Section 3 — Resilience**: Same setup, `kill-random 50%`, verify still succeeds — proof of fault tolerance

### Task 2.2 — Verify-by-following
- Execute Section 1 commands literally from scratch
- Execute Sections 2-3 with the actual script
- Fix any discrepancies

---

## Phase 3: Finalize

- [ ] Create worktree `git worktree add -b howto-peer-manager /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
- [ ] `cd /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`; reset session work directory
- ... (implementation tasks above) ...
- [ ] Verify all work complete in worktree, committed to `howto-peer-manager`
- [ ] Merge `howto-peer-manager` to alpha

---

## Verification

End-to-end verification sequence:
1. `cd p2p && cargo build` — ensures binary exists
2. `python3 integration-tests/start_local_peers.py --peers 5` → enters REPL
3. Inside REPL: `status` → shows 5 alive peers
4. Inside REPL: `stamp 5000 "hello"` → returns Foretis JSON
5. Inside REPL: `verify 5001 "hello" stamp.json` → `{"valid": true}` across peers
6. Inside REPL: `kill-random 3` → 2 peers remain
7. Inside REPL: `verify 5001 "hello" stamp.json` → still `{"valid": true}`
8. Inside REPL: `shutdown` → clean exit with summary
9. Follow HOWTO.md Section 1 independently — all commands succeed
