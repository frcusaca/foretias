# FORETIAS HOWTO & PEER MANAGER IMPLEMENTATION PLAN

**Random differentiator:** `4827`
**Worktree path:** `/home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
**Branch:** `howto-peer-manager`

---

## Phase 1: start_local_peers.py

- [ ] Create worktree `git worktree add -b howto-peer-manager /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`
- [x](2026-05-10 13:11) `cd /home/hcbusy/tmp/foretias-worktrees/HOWTO_4827`; reset session work directory

### Task 1.1: Script skeleton + argument parsing
- [ ] Create `integration-tests/start_local_peers.py`
- [ ] Implement `argparse` block with all CLI args
- [ ] Implement `find_foretias_binary()` — check release then debug

### Task 1.2: Peer spawning logic
- [ ] Implement `spawn_peers()` function with batch spawning
- [ ] Implement `parse_peer_startup(log_path)` — regex-parse TBID, TBN, listen addr
- [ ] Implement peer tracking dict/list

### Task 1.3: Continual Stats Gathering Thread
- [ ] Implement background stats thread with tail-file pattern
- [ ] Per-peer counters: errors, warnings, last_tick, last_log_pos
- [ ] Periodic stderr summary every --stats-interval seconds
- [ ] Thread-safe shared stats via threading.Lock
- [ ] Graceful shutdown via threading.Event

### Task 1.4: Status monitoring
- [ ] Implement `check_peer_alive(peer)`
- [ ] Implement `status()` — formatted table output

### Task 1.5: Kill commands
- [ ] Implement `kill_random(n)` and percentage syntax
- [ ] Implement `kill_tbid(prefix)`
- [ ] Handle edge cases

### Task 1.6: Stamp/Verify convenience commands
- [ ] Implement `stamp(port, message)`
- [ ] Implement `verify(port, message, stamp_file)`

### Task 1.7: REPL loop
- [ ] Implement REPL with all commands
- [ ] SIGINT/Ctrl+C handler
- [ ] Shutdown summary

### Task 1.8: Test the script
- [ ] Build foretias binary if needed
- [ ] Run with `--peers 5`, test all commands
- [ ] Verify stats thread prints correctly
- [ ] Fix any issues

---

## Phase 2: HOWTO.md

### Task 2.1: Write HOWTO.md
- [ ] Create `HOWTO.md` at repo root
- [ ] Write introduction
- [ ] Section 1: Quick Start
- [ ] Section 2: Peering
- [ ] Section 3: Resilience

### Task 2.2: Verify-by-following
- [ ] Follow all sections literally
- [ ] Fix discrepancies

---

## Phase 3: Finalize

- [ ] Verify all work complete, committed to `howto-peer-manager`
- [ ] Merge `howto-peer-manager` to alpha
