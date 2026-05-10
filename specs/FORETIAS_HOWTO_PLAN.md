# FORETIAS HOWTO & PEER MANAGER IMPLEMENTATION PLAN

**Random differentiator:** `4827`
**Worktree path:** `/home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
**Branch:** `howto-peer-manager`

---

## Phase 1: start_local_peers.py

- [x](2026-05-10 13:44) Create worktree `git worktree add -b howto-peer-manager /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
- [x](2026-05-10 13:11) `cd /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`; reset session work directory

### Task 1.1: Script skeleton + argument parsing
- [x](2026-05-10 13:44) Create `integration-tests/start_local_peers.py`
- [x](2026-05-10 13:44) Implement `argparse` block with all CLI args
- [x](2026-05-10 13:44) Implement `find_foretias_binary()` — check release then debug

### Task 1.2: Peer spawning logic
- [x](2026-05-10 13:44) Implement `spawn_peers()` function with batch spawning
- [x](2026-05-10 13:44) Implement `parse_peer_startup(log_path)` — regex-parse TBID, TBN, listen addr
- [x](2026-05-10 13:44) Implement peer tracking dict/list

### Task 1.3: Continual Stats Gathering Thread
- [x](2026-05-10 13:44) Implement background stats thread with tail-file pattern
- [x](2026-05-10 13:44) Per-peer counters: errors, warnings, last_tick, last_log_pos
- [x](2026-05-10 13:44) Periodic stderr summary every --stats-interval seconds
- [x](2026-05-10 13:44) Thread-safe shared stats via threading.Lock
- [x](2026-05-10 13:44) Graceful shutdown via threading.Event

### Task 1.4: Status monitoring
- [x](2026-05-10 13:44) Implement `check_peer_alive(peer)`
- [x](2026-05-10 13:44) Implement `status()` — formatted table output

### Task 1.5: Kill commands
- [x](2026-05-10 13:44) Implement `kill_random(n)` and percentage syntax
- [x](2026-05-10 13:44) Implement `kill_tbid(prefix)`
- [x](2026-05-10 13:44) Handle edge cases

### Task 1.6: Stamp/Verify convenience commands
- [x](2026-05-10 13:44) Implement `stamp(port, message)`
- [x](2026-05-10 13:44) Implement `verify(port, message, stamp_file)`

### Task 1.7: REPL loop
- [x](2026-05-10 13:44) Implement REPL with all commands
- [x](2026-05-10 13:44) SIGINT/Ctrl+C handler
- [x](2026-05-10 13:44) Shutdown summary

### Task 1.8: Test the script
- [x](2026-05-10 13:44) Build foretias binary if needed
- [x](2026-05-10 13:44) Run with `--peers 5`, test all commands
- [x](2026-05-10 13:44) Verify stats thread prints correctly
- [x](2026-05-10 13:44) Fix any issues

---

## Phase 2: HOWTO.md

### Task 2.1: Write HOWTO.md
- [x](2026-05-10 13:44) Create `HOWTO.md` at repo root
- [x](2026-05-10 13:44) Write introduction
- [x](2026-05-10 13:44) Section 1: Quick Start
- [x](2026-05-10 13:44) Section 2: Peering
- [x](2026-05-10 13:44) Section 3: Resilience

### Task 2.2: Verify-by-following
- [x](2026-05-10 13:44) Follow all sections literally
- [x](2026-05-10 13:44) Fix discrepancies

---

## Phase 3: Finalize

- [x](2026-05-10 13:44) Verify all work complete, committed to `howto-peer-manager`
- [x](2026-05-10 13:44) Merge `howto-peer-manager` to alpha

- [x](2026-05-10 13:44) Cleanup `/home/hcbusy/tmp/foretias-worktrees/HOWTO_4827` after successful merge
