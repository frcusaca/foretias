# DHT Stress Test & Jupyter Analysis — Implementation Plan

**Stage**: DHT Stress Test Hardening & Jupyter Analysis Notebook
**Parent Major**: DHT Peer Discovery (FORETIAS_2_P2P_4_dht_discovery.md)
**Status**: Plan — pending human review
**Date**: 2026-05-06
**Prereqs**: `dht_stress_test.sh` exists (done), `dht_stress_analysis.py` exists (done), DHT swarm works (done)

---

## READING ORDER

1. Re-read `FORETIAS_2_P2P_4_dht_discovery.md` §6 (Test Plan)
2. Re-read `foretias/p2p/dht_stress_test.sh` (stress test runner — 234 lines)
3. Re-read `foretias/p2p/dht_stress_analysis.py` (CLI analysis — 336 lines)
4. Re-read `foretias-node/src/communerd/mod.rs` (§289-327: heartbeat, §329-418: gossip loop)
5. Re-read `foretias-node/src/communerd/mod.rs` (§476-583: `register_and_discover()`)
6. Read this document end to end before touching any code

---

## 1. GOAL

The DHT stress test currently runs K seeds + N peers for a configurable runtime, collects logs, and produces a CLI summary with static plots. Several capabilities are incomplete:

1. **Timestamps are missing** from Rust tracing output → time-series plots are impossible (analysis script uses request order as proxy, line 290-291)
2. **Jupyter notebook does not exist** — `dht_stress_analysis.py:8` references `dht_stress_analysis.ipynb` but no such file exists
3. **TBID-based self-recognition is missing** — only IP/port self-dial skip exists
4. **`max_discovered_peers` is ignored** — `_max_peers: usize` parameter is unused
5. **No per-server distribution or activity-over-time plots** — only aggregate histograms
6. **No connection-graph / topology visualization**

**Deliverables**:
- Timestamped logging for all stress test nodes (Rust tracing)
- `dht_stress_analysis.ipynb` — Jupyter notebook with all analysis sections
- TBID self-recognition fix
- `max_discovered_peers` cap enforcement
- Random peer selection for attestation/mirroring

---

## 2. WHAT EXISTS TODAY

| Component | State |
|-----------|-------|
| `dht_stress_test.sh` | K seeds (configurable), N peers (default 100), runtime (default 300s), shuffled seeds per peer, self-dial skip by IP/port |
| `dht_stress_analysis.py` | CLI script — parses logs, prints summary, generates static PNG plots (histograms, boxplots). No time-series (no timestamps in logs). Mentions `.ipynb` on line 8 but none exists. |
| `register_and_discover()` | Accepts `_max_peers: usize`, ignores it. Self-recognition by IP/port only (line 498-503). |
| `PeerPool` | No cap on peer count. `add_peer()` accepts any peer. |
| Rust tracing | Uses `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!`. No explicit timestamps in format (tracing defaults may or may not include timestamps depending on subscriber config). |
| `main.rs` tracing init | Need to check — if using `tracing-subscriber` with `fmt().with_env_filter()`, timestamps depend on config. |

---

## 3. PHASE 1 — Timestamp Wiring (Prerequisite for Time-Series)

**Goal**: Ensure all Rust tracing output from stress test nodes includes parseable timestamps so the analysis notebook can produce time-series plots.

### Stage 1.1 — Audit Tracing Configuration

**Problem**: Unknown whether current tracing subscriber emits timestamps. If not, analysis script cannot do time-series.

**Task 1.1.1** — Check `main.rs` tracing initialization
- **File**: `foretias-node/src/main.rs`
- **Look for**: `tracing_subscriber::fmt::()` or `init_tracing()` calls
- **Check**: Is `with_timer()` or `with_env_filter()` configured? Does the format include timestamps?
- **Verification**: Run `foretias serve --help` and check `RUST_LOG` docs. Start a single node and inspect first 5 lines of log output for timestamp presence.

**Task 1.1.2** — Add timestamp formatting if missing
- **File**: `foretias-node/src/main.rs` (or dedicated `tracing_init.rs` module)
- **Change**: If timestamps are absent, configure tracing subscriber:
  ```rust
  tracing_subscriber::fmt()
      .with_env_filter(
          Directives::from_env_filter(
              env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
          ),
      )
      .with_timer(chrono::OffsetDateTime::UNIX_EPOCH) // or LocalTime/UtcTime
      .with_target(true)
      .init();
  ```
- **If `tracing-subscriber` is not a dependency**: Add to `Cargo.toml`:
  ```toml
  tracing-subscriber = { version = "0.3", features = ["env-filter", "time"] }
  chrono = "0.4"
  ```
- **Verification**: Start a node, verify log lines contain ISO 8601 or epoch timestamps. Example expected format:
  ```
  2026-05-06T12:34:56.789Z  INFO foretias::communerd: DHT-discovered peer added to pool peer=12D3KooW...
  ```

**Task 1.1.3** — Emit structured stress-test events with timestamps
- **File**: `communerd/mod.rs`, `server/handlers.rs`
- **Key events to tag with explicit tracing** (many already have `tracing::info!`/`warn!`, verify they cover these):
  - `stamps_total` (every stamp handled) — already in `handlers.rs:62` via metrics, add `tracing::info!(event="stamp_handled", tick=?...)`
  - `verify_handled` (every verify with result) — add `tracing::info!(event="verify_handled", valid=..., method=...)`
  - `peer_discovered` — already in `mod.rs:380, 414`
  - `self_dial_skipped` — already in `mod.rs:499`
  - `mirror_request_sent` — (new, for active mirroring)
  - `mirror_catchup_complete` — (new, for active mirroring)
  - `heartbeat_sent` — add `tracing::debug!(event="heartbeat_sent")` in `heartbeat_broadcast_loop`
  - `attestation_sent` — add `tracing::info!(event="attestation_sent", peer=...)` in auto-attest path
  - `connection_established` — already in swarm event handling
  - `connection_dropped` — already in swarm event handling

**Task 1.1.4** — Add `--log-file` flag to `foretias serve` for stress tests
- **File**: `main.rs` (serve subcommand)
- **Change**: Add optional `--log-file <path>` to write structured JSON logs (for machine parsing) alongside stderr tracing (for human readability)
- **Implementation**: Use `tracing-subscriber`'s layer stacking:
  ```rust
  let file_layer = tracing_subscriber::fmt::layer()
      .json()
      .with_file(false)
      .with_line(false)
      .with_writer(move || file_handle.clone());
  tracing_subscriber::registry()
      .with(fmt_layer_for_stderr)
      .with(file_layer)
      .init();
  ```
- **Verification**: Run with `--log-file /tmp/test.json`, verify output is line-delimited JSON with timestamps

---

## 4. PHASE 2 — DHT Peer Discovery Fixes

**Goal**: Fix TODOs discovered during stress testing that affect correctness.

### Stage 2.1 — TBID-Based Self-Recognition

**Problem**: `register_and_discover()` skips self-dial by IP/port match only. If a node discovers its own TBID record in the DHT (which it will, since it publishes its own record), it will add itself to the peer pool.

**Task 2.1.1** — Add TBID self-recognition in gossip_event_loop
- **File**: `communerd/mod.rs:367-382` (RecordRetrieved handler)
- **Change**:
  ```rust
  NetworkEvent::RecordRetrieved { key, records } => {
      if (&*key.to_vec()).ends_with(b"/peers/v1") {
          let my_tbid_hex = hex::encode(*my_tbid); // need to thread tbid into this closure
          for record in &records {
              if let Ok(peer_record) = serde_json::from_slice::<PeerRegistrationRecord>(&record.value) {
                  // TBID-based self-recognition
                  if peer_record.tbid == my_tbid_hex {
                      tracing::debug!(peer = %peer_record.peer_id, "skipping self (matches own TBID)");
                      continue;
                  }
                  // ... rest of existing logic
              }
          }
      }
      // ... also apply to TBID index records
  }
  ```
- **Also apply** in `register_and_discover()` `Step 5` (discover peers) — when GET record returns results, filter out own TBID before adding to pool.
- **Verification**:
  - Unit test: gossip loop receives record with own TBID → peer pool unchanged
  - Stress test: after 30s, each node's peer count < total nodes (self excluded)

### Stage 2.2 — max_discovered_peers Cap

**Problem**: `register_and_discover(_max_peers: usize)` accepts a cap but ignores it. `PeerPool` grows unbounded.

**Task 2.2.1** — Add `max_peers` to `PeerPool`
- **File**: `communerd/peer_pool.rs`
- **Changes**:
  1. Add `max_peers: usize` field to `PeerPool` struct
  2. Update `new()` to accept 4th parameter: `max_peers: usize`
  3. In `add_peer()`, guard:
     ```rust
     pub async fn add_peer(&self, addr: PeerAddr) {
         let mut peers = self.peers.write().await;
         if peers.iter().any(|p| p.json_rpc == addr.json_rpc) {
             return; // already known
         }
         if peers.len() >= self.max_peers {
             tracing::debug!(max_peers = self.max_peers, "peer pool full, rejecting new peer");
             return;
         }
         peers.push(addr);
     }
     ```
- **Verification**: Unit test — add 101 peers to pool with max=100 → pool size is 100

**Task 2.2.2** — Wire `max_peers` through the call chain
- **File**: `communerd/mod.rs`
- **Changes**:
  1. `register_and_discover()`: rename `_max_peers` → `max_peers`
  2. Pass `max_peers` to `PeerPool::new()` — but `PeerPool` is created in `Communerd::new()` at line 101. Need to either:
     - Option A: Add `resize(max_peers)` method to PeerPool called after construction
     - Option B: Accept `max_peers` in `Communerd::new()` and pass to `PeerPool::new()`
  3. **Recommendation**: Option B. Add `max_peers: usize` to `NodeConfig` (default 256). Pass through to `PeerPool::new()` in `Communerd::new()`.
- **File**: `core-engine/src/config.rs` — add `pub max_discovered_peers: usize` to `NodeConfig` with default 256
- **Verification**: `cargo check` passes

### Stage 2.3 — Random Peer Selection

**Problem**: Auto-attestation and mirror selection should distribute load randomly across available peers.

**Task 2.3.1** — Add shuffle to auto-attest peer selection in Chronomatter
- **File**: `core-engine/src/chronomatter.rs`
- **Check**: Find the auto-attest loop (likely inside `daemon_tick()` or a dedicated method). Does it select peers sequentially or randomly?
- **If sequential**: Add Fisher-Yates shuffle before selection:
  ```rust
  let mut peers = messenger.query_community(CommunityQuery::KnownPeers)?;
  // Shuffle to distribute attestation load
  peers.as_mut_slice().shuffle(&mut rand::thread_rng());
  // Pick first N from shuffled list
  ```
- **Verification**: Over 100 ticks, each peer receives roughly equal attestations

---

## 5. PHASE 3 — Jupyter Analysis Notebook

**Goal**: Replace CLI analysis script with a rich Jupyter notebook that reads stress test logs and produces interactive visualizations.

### Stage 3.1 — Notebook Shell

**Task 3.1.1** — Create `dht_stress_analysis.ipynb`
- **File**: `foretias/p2p/dht_stress_analysis.ipynb`
- **Structure** (cells):
  1. **Markdown**: Title, description, how to run
  2. **Code**: Installation and Imports (`pandas`, `numpy`, `matplotlib`, `seaborn`, `plotly`, `networkx`)
  3. **Code**: Configuration (log directory path, output directory)
  4. **Code**: Log parsing (adapted from `dht_stress_analysis.py`, enhanced with timestamp parsing)
  5. **Code**: Summary statistics table
  6. **Plot**: Stamps per node (histogram)
  7. **Plot**: Verifies per node (histogram)
  8. **Plot**: Chronon distribution
  9. **Plot**: Seeds vs Peers stamp boxplot
  10. **Plot**: Peers discovered per node (histogram)
  11. **Plot**: Correctness bar (valid vs invalid)
  12. **Plot**: **NEW** — Network activity over time (time-series, requires timestamps)
  13. **Plot**: **NEW** — Stamps over time (per-server stacked area)
  14. **Plot**: **NEW** — Verifies over time (per-server stacked area)
  15. **Plot**: **NEW** — Peer discovery timeline (cumulative peers discovered vs time)
  16. **Plot**: **NEW** — Connection graph (nodes as vertices, attestation pairs as edges)
  17. **Plot**: **NEW** — Error timeline (errors over time)
  17. **Plot**: **NEW** — Calculate and visualize network metrics: density, average_shortest_path, degree_centrality, etc.
  18. **Code**: Per-server detail table (interactive DataFrame)
  19. **Code**: Export summary to CSV

**Task 3.1.2** — Enhance log parser for timestamps
- **Cell**: Adapt `parse_log()` from `dht_stress_analysis.py`
- **Enhancements**:
  1. Parse timestamp from each log line (ISO 8601 or epoch, depending on tracing format)
  2. Store `timestamps: list[datetime]` alongside each event
  3. Build `events: list[dict]` with `{ts, event_type, details}`
  4. New event types: `stamp`, `verify`, `peer_discovered`, `self_skipped`, `heartbeat`, `attestation`, `error`, `mirror_request`, `mirror_catchup`, `connection`, `disconnection`
- **Verification**: Parse a sample log file, verify timestamps are extracted correctly

**Task 3.1.3** — Time-series plots
- **Cells**: Use `pandas.resample()` to bucket events by time window
- **Plots**:
  - **Activity over time**: Count of all events per 10-second bucket, stacked by event type
  - **Stamps over time**: Per-server stamp counts over time (line chart with one line per server, or stacked area)
  - **Verifies over time**: Same pattern
  - **Peer discovery timeline**: Cumulative count of peers discovered vs elapsed time
  - **Error timeline**: Errors per time bucket (bar chart or heatmap)
- **Library**: `matplotlib` for static plots, optionally `plotly` for interactive
- **Verification**: Render notebook, verify time-series plots show expected patterns (ramp-up at start, steady state, shutdown)

**Task 3.1.4** — Connection graph / topology visualization
- **Cell**: Build network graph from log data
  - Nodes: Each server (identified by TBID or peer_id)
  - Edges: Attestation pairs (directed: A→B means A attested B)
  - Edge weight: Number of attestations between pair
  - Edge color: Connection type (seed-seed, seed-peer, peer-peer)
- **Library**: `networkx` for graph construction, `matplotlib` or `pyvis` for visualization
- **Layout**: Force-directed (Fruchterman-Reingold) or circular
- **Verification**: Render graph, verify 3 seeds are connected to each other, peers connect to seeds and to each other

**Task 3.1.5** — Keep CLI script working
- **File**: `dht_stress_analysis.py`
- **Change**: Update to import shared parsing logic from a common module, or leave as-is (it's useful for quick CLI checks)
- **Recommendation**: Leave as-is. The notebook is the primary analysis tool; the CLI script is a quick smoke test. No changes needed.

---

## 6. PHASE 4 — Stress Test Script Enhancements

**Goal**: Make `dht_stress_test.sh` produce cleaner output for the notebook to consume.

### Stage 4.1 — Emit structured JSON logs alongside tracing

**Task 4.1.1** — Add `--log-file` to stress test invocation
- **File**: `dht_stress_test.sh`
- **Change**: In seed and peer invocation, add `--log-file "$PERSIST/stress.json"` (once the flag is implemented in Phase 1.1.4)
- **Verification**: After stress test, `$LOG_DIR/seed_0/stress.json` exists and is valid line-delimited JSON

### Stage 4.2 — Capture per-node startup metadata

**Task 4.2.1** — Write a `node_info.json` file per node
- **File**: `dht_stress_test.sh`
- **Change**: After spawning each node, write a metadata file:
  ```bash
  cat > "$PERSIST/node_info.json" <<EOF
  {
    "name": "seed_$s",
    "role": "seed",
    "rpc_port": $PORT,
    "chronon_ns": $CHRONON_NS,
    "known_servers": [$(printf '"%s",' "${SEED_ADDRS[@]}" | sed 's/,$//')]
  }
  EOF
  ```
- **Purpose**: Notebook reads these to know which nodes are seeds, their chronon values, and known server lists — without parsing log lines.
- **Verification**: `cat $LOG_DIR/seed_0/node_info.json | python3 -m json.tool` succeeds

---

## 7. TEST PLAN

| Test | Type | Verification |
|------|------|------|
| `tracing_emits_timestamps` | Manual | Start node, verify log lines have parseable timestamps |
| `structured_json_log_output` | Manual | Start with `--log-file`, verify JSON output |
| `tbid_self_recognition` | Unit | gossip loop skips own TBID record |
| `peer_pool_cap_enforced` | Unit | Pool rejects beyond max |
| `random_peer_selection_distributes_load` | Integration | 10 nodes, 100 ticks, stddev of attestations < 20% of mean |
| `notebook_runs_end_to_end` | Manual | Open `dht_stress_analysis.ipynb`, run all cells, all plots render |
| `time_series_plots_have_data` | Manual | Activity-over-time plot shows non-zero data |
| `connection_graph_renders` | Manual | Graph shows all N nodes with edges |
| `stress_test_with_all_features` | Integration | Run `dht_stress_test.sh --n-seeds 3 --n-peers 10 --runtime 60`, verify no crashes, notebook produces plots |

---

## 8. NON-GOALS

- ❌ Real-time monitoring dashboard — offline analysis only
- ❌ Distributed tracing (Jaeger/Zipkin) — single-node logs are sufficient
- ❌ Automated test harness integration (GitHub Actions) — manual execution for now
- ❌ Performance benchmarking — stress test measures correctness, not throughput
- ❌ Cross-platform stress test (Windows/macOS) — Linux only

---

## 9. MILESTONE CHECKLIST

```
[ ] M1  Audit tracing config, add timestamps if missing
[ ] M2  Add structured tracing events for stress-test-relevant operations
[ ] M3  Add --log-file flag for structured JSON output
[ ] M4  TBID-based self-recognition in gossip_event_loop
[ ] M5  max_discovered_peers cap in PeerPool
[ ] M6  Random peer selection for auto-attestation
[ ] M7  Create dht_stress_analysis.ipynb (shell + imports + config)
[ ] M8  Enhanced log parser with timestamp extraction
[ ] M9  Time-series plots (activity, stamps, verifies, discovery, errors)
[ ] M10 Connection graph / topology visualization
[ ] M11 Stress test script enhancements (--log-file, node_info.json)
[ ] M12 Notebook runs end-to-end on sample stress test data
[ ] M13 Full stress test run (3 seeds, 100 peers, 300s) produces complete analysis
[ ] M14 TAG: v0.5-dht-stress-analysis
```
