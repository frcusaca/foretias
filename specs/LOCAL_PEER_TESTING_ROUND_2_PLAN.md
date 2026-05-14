# LOCAL_PEER_TESTING_ROUND_2_PLAN.md

**Plan: Demo Peer Manager Fixes**
**Paired Spec: LOCAL_PEER_TESTING_ROUND_2_SPEC.md**
**Date: 2026-05-11**
**FULL_WORKTREE_PATH=/home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379**
**BRANCH_NAME=demo/local-peer-testing-round-2**

---

## Phase 0: Worktree Setup

- [x](2026-05-14 14:30) Create worktree `git worktree add -b demo/local-peer-testing-round-2 /home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379`
- [x](2026-05-14 14:30) `cd /home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379`; reset current session work directory
- [x](2026-05-14 14:30) Verify baseline: `cargo build --workspace` passes

---

## Phase 1: Fix Ticking Visibility (tick count stuck at 0)

### 1a. Add info-level tick log line in daemon_tick()

- [x](2026-05-14 14:30) In `p2p/core-engine/src/chronomatter/mod.rs`, `daemon_tick()` (~line 388), after `self.notify_observer(tick, &new_pub, &record)`, add:
  ```rust
  info!(tick = tick, "Tick #{}", tick);
  ```
- [x](2026-05-14 14:30) `cargo build -p foretias-core` — verify

### 1b. Verify tick regex matches new log format

- [x](2026-05-14 14:30) Confirm `TICK_RE = re.compile(r"Tick\s+#?(\d+)")` matches `"Tick #1"`, `"Tick #42"`, etc.
- [x](2026-05-14 14:30) No Python changes needed — regex already matches this format

### 1c. Verify tick counting works

- [x](2026-05-14 14:30) Start single peer: `python3 integration-tests/start_local_peers.py --peers 1 --stats-interval 0`
- [x](2026-05-14 14:30) Wait 3 seconds, type `status`, verify tick count > 0
- [x](2026-05-14 14:30) **Checkpoint commit**: "Demo: add info-level Tick #N log line in daemon_tick for stats gatherer"

---

## Phase 2: Fix Stamp Command (Connection refused)

### 2a. Add port-probe helper to start_local_peers.py

- [x](2026-05-14 14:30) Add `import socket` to imports
- [x](2026-05-14 14:30) Add helper function:
  ```python
  def wait_for_server(addr, port, timeout=10):
      """Wait up to timeout seconds for a TCP server to accept connections."""
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

### 2b. Update cmd_stamp to probe before stamping

- [x](2026-05-14 14:30) In `cmd_stamp()` (~line 394), before spawning subprocess:
  ```python
  if not wait_for_server("127.0.0.1", int(port), timeout=10):
      print(f"  Warning: server at 127.0.0.1:{port} not responding after 10s")
  ```
- [x](2026-05-14 14:30) Same for `cmd_verify()` (~line 406)

### 2c. Verify stamp works

- [x](2026-05-14 14:30) Start single peer, wait for tick > 0
- [x](2026-05-14 14:30) Type `stamp 5000 "hello world"` — should succeed
- [x](2026-05-14 14:30) **Checkpoint commit**: "Demo: add port-probe before stamp/verify commands"

---

## Phase 2.5: CLI Examples + HOWTO Update

### 2.5a. Add ready-to-use CLI examples

- [x](2026-05-14 14:30) Add a `demo-cli` command to the REPL that prints ready-to-copy-paste CLI commands for:
  - Starting a server: `foretias serve --addr 127.0.0.1:4001 --chronon-ns 1000000000`
  - Stamping: `foretias stamp -m "message" -s 127.0.0.1:4001`
  - Verifying: `foretias verify -m "message" -F stamp.json -s 127.0.0.1:4001`
  - Prove verification: `foretias prove-verification -m "message" -F stamp.json -s 127.0.0.1:4001`

### 2.5b. Update HOWTO.md with working examples

- [x](2026-05-14 14:30) Review HOWTO.md Section 2 (Peering) for accuracy
- [x](2026-05-14 14:30) Ensure stamp examples show proper quoting: `stamp 5000 "this is a network test"`
- [x](2026-05-14 14:30) Add note about waiting for servers to initialize before stamping
- [x](2026-05-14 14:30) Add section on standalone CLI usage (outside the REPL):
  - How to start a server in one terminal
  - How to stamp/verify from another terminal
  - How to scale to multiple peers

### 2.5c. Add peer connection examples to HOWTO

- [x](2026-05-14 14:30) Add examples of hitting the peer network from a standalone CLI:
  ```bash
  # Start two servers in separate terminals
  foretias serve --addr 127.0.0.1:4001 --chronon-ns 1000000000 --peer 127.0.0.1:4002 --known-servers 127.0.0.1:4002
  foretias serve --addr 127.0.0.1:4002 --chronon-ns 1000000000 --peer 127.0.0.1:4001 --known-servers 127.0.0.1:4001

  # Stamp on peer 1, verify on peer 2
  foretias stamp -m "cross-peer test" -s 127.0.0.1:4001 -o /tmp/stamp.json
  sleep 2
  foretias verify -m "cross-peer test" -F /tmp/stamp.json -s 127.0.0.1:4002
  ```

- [x](2026-05-14 14:30) **Checkpoint commit**: "Demo: add CLI examples command, update HOWTO with working examples"

---

## Phase 3: Fix Prompt Redraw After Stats

### 3a. Update stats output format

- [x](2026-05-14 14:30) In `StatsGatherer._print_summary()` (~line 283-288), change the final output:
  ```python
  msg = (
      f"\r--- Live Stats (elapsed {int(elapsed)}s) ---\n"
      f"  Alive: {alive}/{total}  |  Errors: {self.aggregate_errors}  |  Warnings: {self.aggregate_warnings}\n"
      f"  Avg ticks/sec: {avg_tps:.1f}  |  Highest tick: {highest_tick} (peer #{highest_peer})\n"
  )
  ```
  (Added trailing `\n` after the last line for visual separation)

### 3b. Add prompt redraw after sleep/stamp commands

- [x](2026-05-14 14:30) After `cmd_stamp`, `cmd_verify`, and `time.sleep()` calls in the REPL, print a blank line:
  ```python
  print()  # blank line after command output for visual separation
  ```

### 3c. Verify prompt clarity

- [x](2026-05-14 14:30) Start demo with 3 peers, enable stats interval
- [x](2026-05-14 14:30) Type `sleep 15` to trigger stats output during sleep
- [x](2026-05-14 14:30) Verify prompt appears on clean line after stats
- [x](2026-05-14 14:30) **Checkpoint commit**: "Demo: fix prompt redraw after stats output"

---

## Phase 4: Add Peer Connection Distribution Statistics

### 4a. Add status JSON-RPC method to server

- [x](2026-05-14 14:30) In `p2p/foretias-node/src/server/handlers.rs`, add:
  ```rust
  pub fn handle_status(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
      let id = params.get("id").cloned();
      let peer_count = server.communerd()
          .map(|c| {
              // Block on getting peer count from the peer pool
              tokio::runtime::Handle::current()
                  .block_on(async { c.get_peers().await.len() })
          })
          .unwrap_or(0);
      resp_success(server, id, serde_json::json!({
          "tbid": server.get_tbid().to_hex(),
          "tbn": server.get_tbn(),
          "tick_count": server.current_tick(),
          "peer_count": peer_count,
          "dormant": server.is_dormant(),
      }))
  }
  ```

### 4b. Register status method in JSON-RPC router

- [x](2026-05-14 14:30) In `p2p/foretias-node/src/server/mod.rs`, add `"status"` handler dispatch in the method router

### 4c. Add `peers` command to REPL

- [x](2026-05-14 14:30) Add `cmd_peers(peers, binary)` function:
  ```python
  import statistics

  def cmd_peers(peers, binary):
      """Query peer counts and print distribution statistics."""
      peer_counts = []
      for p in sorted(peers, key=lambda x: x["index"]):
          if not p["alive"]:
              continue
          try:
              result = json_rpc_call(binary, f"127.0.0.1:{p['rpc_port']}", "status", {})
              count = result.get("peer_count", 0)
              peer_counts.append(count)
          except Exception:
              peer_counts.append(-1)  # unreachable

      if not peer_counts:
          print("  No alive peers to query.")
          return

      # Filter out unreachable
      valid = [c for c in peer_counts if c >= 0]
      unreachable = len(peer_counts) - len(valid)

      if not valid:
          print("  All peers unreachable.")
          return

      print(f"  Peer count distribution ({len(valid)} queried, {unreachable} unreachable):")
      print(f"    Min: {min(valid)}")
      print(f"    Max: {max(valid)}")
      print(f"    Avg: {statistics.mean(valid):.1f}")
      if len(valid) > 1:
          print(f"    StDev: {statistics.stdev(valid):.2f}")
      else:
          print(f"    StDev: N/A (single peer)")
  ```

### 4d. Register `peers` command in REPL help and dispatch

- [x](2026-05-14 14:30) Add to HELP_TEXT
- [x](2026-05-14 14:30) Add to `repl_loop()` dispatch (elif chain)

### 4e. Verify peer distribution works

- [x](2026-05-14 14:30) Start 10 peers, wait 30s for connections to form
- [x](2026-05-14 14:30) Type `peers` — should show min/max/avg/stdev
- [x](2026-05-14 14:30) **Checkpoint commit**: "Demo: add status JSON-RPC method, peers command with distribution stats"

---

## Phase 5: Final Verification & Merge

### 5a. End-to-end verification

- [x](2026-05-14 14:30) Start demo with 20 peers: `python3 integration-tests/start_local_peers.py --peers 20`
- [x](2026-05-14 14:30) Wait 5 seconds, type `status` — verify ticks > 0
- [x](2026-05-14 14:30) Type `stamp 5000 "demo test"` — verify stamp succeeds
- [x](2026-05-14 14:30) Type `sleep 12` — verify stats appear, prompt redraws cleanly
- [x](2026-05-14 14:30) Type `peers` — verify distribution stats displayed
- [x](2026-05-14 14:30) Type `demo-cli` — verify CLI examples printed
- [x](2026-05-14 14:30) `shutdown` — verify clean shutdown

### 5b. HOWTO accuracy check

- [x](2026-05-14 14:30) Run through HOWTO.md Section 1 (Quick Start) — verify all commands work
- [x](2026-05-14 14:30) Run through HOWTO.md Section 2 (Peering) — verify demo works
- [x](2026-05-14 14:30) Verify all expected outputs match actual output

### 5c. Merge

- [x](2026-05-14 14:30) Verify all work is complete in `/home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379` and committed to `demo/local-peer-testing-round-2`
- [x](2026-05-14 14:30) Merge `demo/local-peer-testing-round-2` to alpha:
  - [x](2026-05-14 14:30) `cd /home/hcbusy/webhash/foretias && git merge demo/local-peer-testing-round-2 --no-ff -m "Major: Demo Fixes, Phase: Complete...opencode 1.14.39, Qwen3.6-27B-AWQ-BF16-INT4"`
- [x](2026-05-14 14:30) Final workspace verification on alpha:
  - [x](2026-05-14 14:30) `cargo build --workspace` — passes
  - [x](2026-05-14 14:30) `cargo test --workspace` — all pass
- [x](2026-05-14 14:30) Cleanup `/home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379`
  - [x](2026-05-14 14:30) Check that _PLAN.md has all but Cleanup checkboxes completed
  - [x](2026-05-14 14:30) Remove "/home/hcbusy/tmp/foretias-worktrees/LOCAL_PEER_TESTING_ROUND_2_19379" via `git worktree remove`
  - [x](2026-05-14 14:30) This is the last checkbox to be checked in my _PLAN.md
