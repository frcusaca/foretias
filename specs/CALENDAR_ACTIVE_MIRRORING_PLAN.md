# Calendar Active Mirroring — Implementation Plan

**Parent Major**: Calendar Replication (CALENDAR_REPLICATION_SPEC.md)
**Status**: DEPRECATED — superseded by `COMBINED_GROUP4_SPEC.md` §3 and `COMBINED_GROUP4_PLAN.md` Stream 4b (2026-05-22).
           All open tasks re-listed in COMBINED_GROUP4_PLAN.md. Do not update this file.
           **Further note (2026-05-26):** the successor (`COMBINED_GROUP4_*`) is itself now DEFERRED until Communerdette (Group 7) reaches feature completion AND a revised plan reflecting Communerdette / Group 6 integration is written. See `COMBINED_GROUP4_SPEC.md` and `COMBINED_GROUP4_PLAN.md` headers for the resumption preconditions.
**Date**: 2026-05-06
**Prereqs**: `query_community` returns peers (done), MirrorStore exists (done), mirror RPC handlers exist (done), `JobQueue<T>` abstraction (JOB_QUEUE_PLAN.md)

---

## READING ORDER

1. Re-read `CALENDAR_REPLICATION_SPEC.md` §2 (Terminology) and §4 (One-Way Mirror Protocol)
2. Re-read `foretias-node/src/calendar/mod.rs` (Calendar struct — entry point)
3. Re-read `foretias-node/src/calendar/mirror.rs` (MirrorStore — current storage)
4. Re-read `foretias-node/src/server/handlers.rs` (passive mirror handlers)
5. Read this document end to end before touching any code

---

## 1. CALENDAR RESPONSIBILITY HIERARCHY

The Calendar component has five responsibilities, listed in **descending priority**. This ordering governs resource allocation, error handling, and task scheduling:

| # | Priority | Responsibility | Description |
|---|----------|----------------|-------------|
| 1 | **Critical** | **Record for its family's chronomatter** | Store every tick produced by the local chronomatter. This is the Calendar's primary raison d'être. Never starved. |
| 2 | **High** | **Support chronomatter verify behavior** | Respond to verify requests from the local chronomatter (look up ticks by number, validate chains). Low latency required. |
| 3 | **Medium-High** | **Mutual attestation with other calendars** | Proactively initiate and manage cross-attestation stamp exchanges with peers. Attestations are stored locally as part of the calendar's own ticks. Triggered by tick count thresholds AND wall-clock intervals. |
| 4 | **Medium** | **Persist family's calendar in the P2P network** | Find and maintain willing mirrors so that the local calendar's ticks survive beyond this node. Actively seek out mirrors when capacity allows. |
| 5 | **Low** | **Mirror other calendars' ticks** | Store replicated copies of other calendars' data. Useful for redundancy and accelerated verification. Starvable — if resources are tight, mirror maintenance is the first thing to drop. |

**Implications**:
- Priority 1-2 are **always serviced**. They are the Calendar's core function.
- Priority 3 (mutual attestation) is the Calendar's **own job** — not delegated to Chronomatter. It runs on Calendar's task queue, triggered by both tick count and wall clock.
- Priority 4-5 are **P2P health tasks** — driven by Calendar's task queue based on Communerd peer callbacks. These share remaining capacity.
- A mirror failure at priority 4 enqueues a "find new mirror" task at priority 4.
- If the task queue is saturated with priority 3 work, priority 4-5 tasks may be deferred.

This hierarchy MUST be documented in the `Calendar` struct's module-level doc comment.

---

## 2. CALENDAR ARCHITECTURE

### 2.1 Calendar as Task-Driven Orchestrator

The Calendar owns its **own task queue and thread pool**. It does not run a simple `loop { sleep; poll }` pattern. Instead:

```
┌─────────────┐    peer callback      ┌────────────────────────┐
│  Communerd   │ ────────────────────▶│  Calendar              │
│  (peer pool) │                      │                        │
└─────────────┘                      │  ┌──────────────────┐  │
                                     │  │  Task Queue       │  │
                                     │  │  ┌──────────────┐ │  │
                                     │  │  │ ExpireMirror  │ │  │
                                     │  │  │ ExploreMirror │ │  │
                                     │  │  │ InitiateDump  │ │  │
                                     │  │  │ StartStream   │ │  │
                                     │  │  │ DoAttestation │ │  │
                                     │  │  │ FindNewMirror │ │  │
                                     │  │  └──────────────┘ │  │
                                     │  └──────────────────┘  │
                                     │                        │
                                     └────────────────────────┘
```

**Task Queue properties**:
- Backed by a bounded async channel (`tokio::sync::mpsc` or similar)
- Tasks are dequeued by a small worker pool (configurable, default 4 workers)
- Higher-priority tasks preempt lower-priority workers when the queue is full
- Each task has a timeout; timed-out tasks are re-enqueued with backoff

### 2.2 Communerd ↔ Calendar Peer Callback

Calendar registers a callback with Communerd. When Communerd's peer pool changes (new peers discovered, peers lost), it invokes the callback with the updated peer list.

```rust
pub trait PeerChangeCallback: Send + Sync {
    /// Called when the peer pool changes. `peers` is the current list.
    fn on_peers_changed(&self, peers: Vec<PeerAddr>);
}
```

**Calendar's implementation** of this callback enqueues exploration tasks:

1. For each **new peer** (not yet seen): enqueue `ExploreMirror(peer)` — probe whether this peer has a calendar worth mirroring and whether it would accept being a mirror for us.
2. For each **lost peer** that was actively mirroring or streamed to: enqueue `FindNewMirror` to replace the lost capacity.
3. For existing peers with active streams: no action (stream health is monitored separately).

### 2.3 Task Types

```rust
#[derive(Debug)]
pub enum CalendarTask {
    // Priority 3 — Mutual Attestation
    DoAttestation { peers: Vec<PeerAddr>, triggered_by: AttestationTrigger },

    // Priority 4 — Persist our calendar (find mirrors for us)
    ExploreMirror { peer: PeerAddr },
    RequestMirror { peer: PeerAddr },   // Ask peer to mirror our calendar

    // Priority 5 — Mirror other calendars
    InitiateDump { peer: PeerAddr, tbid: [u8; 16], up_to_tick: u64 },
    StartStream { peer: PeerAddr, tbid: [u8; 16], from_tick: u64 },

    // Recovery (inherits priority of the task it replaces)
    FindNewMirror { lost_peer_tbid: Option<[u8; 16]>, reason: String },
    RetryDump { peer: PeerAddr, tbid: [u8; 16], up_to_tick: u64, attempt: u32 },
}

#[derive(Debug)]
pub enum AttestationTrigger {
    TickCount { tick_number: u64, interval: u64 },
    WallClock { elapsed_s: u64, interval_s: u64 },
}
```

### 2.4 Task Execution Model

```
On task dequeue:
  1. Execute the task via Communerd (send RPC, receive response)
  2. On success:
     - DoAttestation → store attestation, schedule next trigger
     - ExploreMirror → if peer has calendar, enqueue RequestMirror
     - RequestMirror → if accepted, enqueue InitiateDump
     - InitiateDump → if completed, enqueue StartStream
     - StartStream → if accepted, stream is now active (no further task needed)
  3. On failure:
     - Mirror/stream failure → enqueue FindNewMirror
     - Dump failure → enqueue RetryDump (with backoff, max 3 attempts)
     - Attestation failure → re-enqueue DoAttestation with backoff
```

### 2.5 Mutual Attestation Scheduling

Mutual attestation is **Calendar's responsibility**, triggered by two independent conditions:

| Trigger | Condition | Default Interval |
|---------|-----------|-----------------|
| Tick count | Every N ticks produced by local chronomatter | 100 ticks |
| Wall clock | Every M seconds of real time | 600 seconds |

Whichever fires first enqueues a `DoAttestation` task. Both timers advance independently.

**Task 0.1** — Add attestation scheduling to Calendar
- Calendar tracks `last_attestation_tick` and `last_attestation_wallclock`
- On every new local tick (priority 1 recording path), check tick-count trigger
- A dedicated background beat checks wall-clock trigger
- Either trigger enqueues `DoAttestation { peers, triggered_by }` on the task queue
- **File**: `calendar/mod.rs` or `calendar/attestation.rs`
- **Verification**: Unit test — tick-count trigger fires at interval, wall-clock trigger fires independently

---

## 3. PERIOD STORAGE MODEL

### 3.1 Period Definition

Ticks are chunked into **Periods** — bounded, verifiable segments that are the atomic unit of transfer during mirroring.

A Period carries **two TBIDs**:
- **`chronomatter_tbid`** — The TBID of the chronomatter that generated these ticks (producer)
- **`calendar_tbid`** — The TBID of the calendar that originally recorded these ticks (source)

When a Period is replicated, **both TBIDs are preserved**. A mirrored period still identifies both its original producer and its original source calendar.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Period {
    /// TBID of the chronomatter that produced these ticks
    pub chronomatter_tbid: [u8; 16],

    /// TBID of the calendar that originally recorded these ticks
    pub calendar_tbid: [u8; 16],

    /// 0-based sequential index within (chronomatter_tbid, calendar_tbid) pair
    pub period_index: u64,

    /// Inclusive first tick number in this period
    pub start_tick: u64,

    /// Inclusive last tick number in this period
    pub end_tick: u64,

    /// Number of ticks in this period (end_tick - start_tick + 1)
    pub tick_count: u64,

    /// SHA-256 checksum of concatenated tick numbers in this period
    pub checksum: String,

    /// Expandable metadata — reserved for future fields:
    /// - combined public key (for signature verification)
    /// - attestation references
    /// - compression format
    pub metadata: HashMap<String, String>,
}
```

**Period size**: Configurable, default 1000 ticks. Period `n` covers ticks `[n * period_size .. (n+1) * period_size)`. The last period may be partial.

### 3.2 Period Index

Each (chronomatter_tbid, calendar_tbid) pair maintains an ordered list of Periods.

```rust
#[derive(Debug, Clone)]
pub struct PeriodIndex {
    pub chronomatter_tbid: [u8; 16],
    pub calendar_tbid: [u8; 16],
    pub periods: Vec<Period>,
}
```

**Invariants**:
- Periods are ordered by `period_index` (ascending)
- Periods are contiguous: `periods[n].end_tick + 1 == periods[n+1].start_tick`
- No gaps, no overlaps

### 3.3 Storage Organization

Calendar storage is partitioned by the **chronomatter TBID** of the producer:

```
storage/
  calendar/
    local/                          // Own chronomatter's ticks (priority 1)
    mirrors/
      {chronomatter_tbid_hex}/      // Grouped by who produced the ticks
        {calendar_tbid_hex}/        // Grouped by which calendar originally recorded them
          period_0.jsonl
          period_1.jsonl
          ...
          period_index.json        // PeriodIndex for this pair
          tick_count               // Total tick count
```

**Key insight**: The first-level partition is by chronomatter TBID. This means if three different calendars all mirror ticks from the same chronomatter, the ticks are grouped together under that chronomatter's TBID, but sub-partitioned by which calendar originally recorded them. When querying for a dump, the mirror asks for `(chronomatter_tbid, calendar_tbid)` to get the specific lineage.

### 3.4 Period Finalization

A period is **finalized** when it contains `period_size` ticks. The current/active period is **open** (not yet finalized).

- `hash_sanity` / `checksum` is computed at finalization time
- An open period's checksum is recomputed on demand (for queries and partial dumps)
- Metadata is empty at creation, populated later (e.g., combined public key added post-facto)

### 3.5 Period Configuration

```rust
pub struct PeriodConfig {
    pub period_size: usize,  // default: 1000
}
```

### 3.6 Period Operations

```rust
pub trait PeriodOps: Send + Sync {
    /// Finalize a period when tick count crosses boundary.
    fn maybe_finalize_period(&mut self, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16]) -> Option<Period>;

    /// Get period index for a (chronomatter, calendar) pair.
    fn get_period_index(&self, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16]) -> Option<&PeriodIndex>;

    /// List all period indices.
    fn list_period_indices(&self) -> Vec<&PeriodIndex>;

    /// Get ticks within a specific period.
    fn get_period_ticks(&self, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16], period_index: u64) -> Option<Vec<TickRecord>>;

    /// Total tick count for a (chronomatter, calendar) pair.
    fn tick_count_for_pair(&self, chronomatter_tbid: &[u8; 16], calendar_tbid: &[u8; 16]) -> u64;

    /// List all known chronomatter TBIDs.
    fn list_chronomatter_tbids(&self) -> Vec<[u8; 16]>;

    /// List all calendar TBIDs for a given chronomatter TBID.
    fn list_calendar_tbids_for(&self, chronomatter_tbid: &[u8; 16]) -> Vec<[u8; 16]>;
}
```

---

## 4. PHASE 1 — Periods

**Goal**: Chunk ticks into Periods with dual-TBID metadata. Implement and test before any transfer protocol.

### 4.1 Tasks

**Task 1.1** — Add `PeriodConfig` to `NodeConfig`
- **File**: `core-engine/src/config.rs`
- **Verification**: `cargo check` passes

**Task 1.2** — Define `Period`, `PeriodIndex` types with dual TBIDs and expandable metadata
- **File**: `calendar/period.rs` (new file)
- **Verification**: `cargo check` passes

**Task 1.3** — Implement `PeriodOps` trait — finalization, queries, tick retrieval
- **File**: `calendar/period_ops.rs` (new file)
- Wire `maybe_finalize_period()` into the tick insertion path (both local and mirrored)
- **Verification**: Unit test — insert 2500 ticks for one pair, expect 2 finalized + 1 open (500)

**Task 1.4** — Implement disk-backed MirrorStore with dual-TBID partitioning
- **File**: `calendar/mirror.rs`
- MirrorStore persists all data to disk under the layout from §3.3:
  - `period_N.jsonl` — one `TickRecord` per line, within `(chronomatter_tbid, calendar_tbid)` partition
  - `period_index.json` — serialized `PeriodIndex` for the partition
  - `tick_count` — file containing the u64 tick count for the partition
- On `Calendar::new()`: scan the `mirrors/` directory tree, reconstruct `PeriodIndex` for each partition from existing files, load tick counts
- All writes are append-only to the current open period's JSONL file. Finalized periods are never modified.
- `period_index.json` is written on every finalization and on `start()` for the open period
- **Verification**: Unit test — insert ticks, drop in-memory state, reload from disk, verify all periods and counts match

**Task 1.4.5** — MirrorStore persistence tests
- **Verification**: Integration test — start calendar, insert 2500 ticks across 3 TBID pairs, stop calendar, restart, verify all data is present with correct period boundaries and tick counts

**Task 1.5** — Add `periods_query` RPC handler
- **File**: `server/handlers.rs`
- **New RPC**:
  ```json
  {
    "method": "periods_query",
    "params": {
      "chronomatter_tbid": "<hex or null for all>",
      "calendar_tbid": "<hex or null for all>"
    }
  }
  ```
- **Response**: Array of `PeriodIndex` objects matching the filter.
- **Verification**: Unit test — 3 pairs, query all, query by chronomatter, query by pair

### 4.2 Phase 1 Acceptance Criteria

- [ ] Periods created automatically as ticks are inserted (local and mirrored)
- [ ] Period index queryable by chronomatter TBID, calendar TBID, or both
- [ ] `periods_query` RPC returns correct dual-TBID metadata
- [ ] Tick count per (chronomatter, calendar) pair is accurate
- [ ] Period metadata field is present and expandable
- [ ] `cargo test` passes

---

## 5. PHASE 2 — History Dump Connection

**Goal**: A one-time connection that transfers complete history up to a tick (inclusive), then closes.

**Independently implementable and testable. Does NOT depend on Phase 3.**

### 5.1 Concept

The mirror calendar asks: **"Give me all ticks for (chronomatter_tbid, calendar_tbid) up to tick N."**
The source ships periods in order, one at a time, up to the requested tick.

One connection. One request. One transfer. Done.

### 5.2 RPC Protocol

```
Mirror → Source: history_dump_request { chronomatter_tbid, calendar_tbid, up_to_tick }
Source → Mirror: history_dump_start { period_count, total_ticks }
Source → Mirror: history_dump_period { Period, tick_records: [TickRecord] }  (repeated)
Source → Mirror: history_dump_end { combined_checksum }
Mirror → Source: history_dump_ack { status: "ok" | "error" }
```

**Flow**:
1. Mirror sends `history_dump_request` with both TBIDs and `up_to_tick`
2. Source looks up the period index for the pair, determines which periods cover `[0..up_to_tick]`
3. Source sends `history_dump_start`
4. Source ships each period via `history_dump_period` (the full Period struct + tick records)
5. For the final partial period, only ship ticks up to `up_to_tick`
6. Source sends `history_dump_end` with combined checksum (SHA-256 of all tick numbers in range)
7. Mirror verifies, inserts ticks under the original TBID pair, replies with `history_dump_ack`

### 5.3 Tasks

**Task 2.1** — Add `history_dump_request` transport method
- **File**: `transport.rs`
- **Verification**: `cargo check` passes

**Task 2.2** — Implement `handle_history_dump_request` on source side
- **File**: `server/handlers.rs`
- Ships periods in order, respects `up_to_tick`, computes combined checksum
- **Verification**: Unit test — mock calendar with 2500 ticks, request up_to=1500 → 2 periods shipped

**Task 2.3** — Implement `initiate_history_dump` on Calendar side (called by task queue)
- **File**: `calendar/mod.rs` — new method that delegates to Communerd
- **Verification**: Unit test with mock transport — ticks inserted with original TBID pair preserved

**Task 2.4** — Add `history_dump_ack` handler on source side
- **File**: `server/handlers.rs`
- Log ack result; on error, signal Calendar to re-enqueue RetryDump

### 5.4 Phase 2 Acceptance Criteria

- [ ] Dump ships periods in order with dual-TBID metadata preserved
- [ ] `up_to_tick` correctly truncates the final partial period
- [ ] Mirror inserts ticks under the original (chronomatter_tbid, calendar_tbid) pair
- [ ] Checksum verification works (match and mismatch)
- [ ] Connection closes cleanly
- [ ] `cargo test` passes

---

## 6. PHASE 3 — Stream Connection

**Goal**: An ongoing connection delivering ticks from a tick (inclusive) onward.

**Independently implementable and testable. Does NOT depend on Phase 2.**

### 6.1 Concept

The mirror calendar asks: **"Send me ticks for (chronomatter_tbid, calendar_tbid) from tick N onward."**
The source pushes new ticks as they are produced.

One connection. Ongoing delivery. Explicit stop.

### 6.2 RPC Protocol

```
Mirror → Source: stream_request { chronomatter_tbid, calendar_tbid, from_tick }
Source → Mirror: stream_start { from_tick }
Source → Mirror: stream_tick { tick_record: TickRecord }  (repeated)
Mirror → Source: stream_ack { tick_number }
Mirror → Source: stream_stop { reason }  (optional)
Source → Mirror: stream_end { reason }  (source-initiated stop)
```

### 6.3 Stream Registration

Two approaches:

| Approach | Pros | Cons |
|----------|------|------|
| **A — Callback on tick production** | Immediate delivery | More wiring |
| **B — Polling loop** | Simpler | Latency proportional to poll interval |

**Recommendation**: Approach B initially. Refactor to A if latency matters.

### 6.4 Backpressure

- Source buffers at most `stream_buffer_size` ticks (default 100)
- Buffer full → `WARN:stream_buffer_full`
- Buffer full > `stream_stall_timeout_s` (default 60s) → terminate with `stream_end { reason: "mirror_too_slow" }`

### 6.5 Tasks

**Task 3.1** — Add `stream_request` transport method
- **File**: `transport.rs`
- **Verification**: `cargo check` passes

**Task 3.2** — Implement `handle_stream_request` on source side
- **File**: `server/handlers.rs`
- Polling loop, push new ticks, handle backpressure
- **Verification**: Unit test — source produces 10 ticks, all delivered

**Task 3.3** — Implement `initiate_stream` on Calendar side (called by task queue)
- **File**: `calendar/mod.rs`
- Returns `StreamHandle` (allows Calendar to stop stream later)
- On stream termination (any reason), enqueue `FindNewMirror` or `RetryDump` as appropriate
- **Verification**: Unit test with mock transport

**Task 3.4** — Backpressure + stall timeout
- **File**: `server/handlers.rs`
- **Verification**: Unit test — slow mirror → stall → clean termination

### 6.6 Phase 3 Acceptance Criteria

- [ ] Stream delivers ticks as produced, with dual-TBID preservation
- [ ] `from_tick` is inclusive
- [ ] Backpressure: buffer full → stall → timeout → clean stop
- [ ] Either side can stop cleanly
- [ ] Stream termination triggers Calendar task re-enqueue
- [ ] `cargo test` passes

---

## 7. PHASE 4 — CALENDAR TASK QUEUE & PEER CALLBACK

**Goal**: Wire the Calendar's task queue, Communerd peer callback, and mutual attestation scheduling. This is the orchestration phase that ties Phases 1-3 into a living system.

### 7.1 JobQueue Instance

Calendar uses the shared `JobQueue<CalendarJob>` defined in **JOB_QUEUE_PLAN.md** (§2.3, Task 6).

**File**: `foretias-node/src/calendar/mod.rs`

```rust
impl Calendar {
    pub fn new(config: CalendarConfig, ...) -> Self {
        let job_queue = JobQueue::new(JobQueueConfig {
            workers: config.job_queue_workers,
            max_queue_depth: config.job_queue_max_depth,
            ..Default::default()
        });
        // ...
    }
}
```

Calendar defines its own `CalendarJob` enum (listed in §2.3 above) implementing the `Job` trait. Each variant's `priority()` returns its responsibility level (0 = critical, 4 = low).

### 7.2 Communerd Peer Callback Registration

**File**: `communerd/mod.rs`

```rust
impl Communerd {
    pub fn register_peer_callback(&mut self, callback: Arc<dyn PeerChangeCallback>) {
        self.peer_callback = Some(callback);
    }
}
```

When `PeerPool` changes (peer added, peer removed), `Communerd` calls the callback with the full peer list.

**Task 4.1** — Add `register_peer_callback` to Communerd
- Wire callback invocation into `PeerPool::add_peer()` and `PeerPool::remove_peer()`
- **Verification**: Unit test — callback fires on peer add/remove

### 7.3 Calendar's Peer Callback Implementation

**File**: `calendar/mod.rs`

```rust
impl PeerChangeCallback for Calendar {
    fn on_peers_changed(&self, peers: Vec<PeerAddr>) {
        let current_peers = self.tracked_peers.lock();
        let new_peers = peers.iter()
            .filter(|p| !current_peers.contains(*p))
            .cloned()
            .collect::<Vec<_>>();
        let lost_peers = current_peers.iter()
            .filter(|p| !peers.contains(*p))
            .cloned()
            .collect::<Vec<_>>();

        // New peers → explore for mirroring (priority 4 & 5)
        for peer in new_peers {
            self.job_queue.enqueue(CalendarJob::ExploreMirror { peer });
        }

        // Lost peers → find replacements if they were mirrors
        for peer in lost_peers {
            if self.was_mirroring(&peer) {
                self.job_queue.enqueue(CalendarJob::FindNewMirror {
                    lost_peer_tbid: self.resolve_peer_tbid(&peer),
                    reason: "peer_lost".into(),
                });
            }
        }

        *current_peers = peers;
    }
}
```

**Task 4.2** — Implement Calendar's `PeerChangeCallback`
- **Verification**: Unit test — peer added → ExploreMirror enqueued; peer lost → FindNewMirror enqueued

### 7.4 Mirroring Lifecycle via Task Queue

The sequence of tasks for establishing a mirror:

```
ExploreMirror(peer)
  → peer has a calendar?
    → yes → enqueue RequestMirror(peer)
    → no → discard

RequestMirror(peer)
  → peer accepts?
    → yes → enqueue InitiateDump(peer, tbid, tick_count)
    → no → discard

InitiateDump(peer, tbid, up_to_tick)
  → dump succeeds?
    → yes → enqueue StartStream(peer, tbid, up_to_tick + 1)
    → no (attempt < 3) → enqueue RetryDump(peer, tbid, up_to_tick, attempt+1)
    → no (attempt >= 3) → log ERROR, enqueue FindNewMirror

StartStream(peer, tbid, from_tick)
  → stream starts?
    → yes → stream active, monitored by separate health check
    → no → enqueue FindNewMirror

FindNewMirror(reason)
  → request new peer list from Communerd
  → enqueue ExploreMirror for each unexplored peer
```

**Task 4.3** — Implement the full task execution chain
- Each task type's handler in `calendar/task_executor.rs`
- **Verification**: Integration test — 2 servers, full lifecycle from peer discovery to active stream

### 7.5 Mutual Attestation Integration

Mutual attestation runs entirely within Calendar's task queue:

```
Tick produced (priority 1 path):
  → check: (tick_number - last_attestation_tick) >= attestation_tick_interval?
  → yes → enqueue DoAttestation { triggered_by: TickCount }

Wall clock heartbeat (§7.6, spawned in start()):
  → check: (now - last_attestation_wallclock) >= attestation_clock_interval?
  → yes → enqueue DoAttestation { triggered_by: WallClock }

DoAttestation job execution:
  → select N peers from Communerd peer pool (shuffled)
  → initiate stamp/verify exchange with each
  → store attestations as part of local calendar
  → update last_attestation_tick and/or last_attestation_wallclock
```

**Task 4.4** — Wire attestation triggers into the tick recording path
- **File**: `calendar/mod.rs` or `calendar/attestation.rs`
- Tick-count trigger: check on every new local tick in the priority 1 recording path
- Wall-clock trigger: handled by the heartbeat loop in `start()` (see §7.6)
- **Verification**: Unit test — tick trigger fires at interval, clock heartbeat fires independently

### 7.6 Calendar `start()` — Workers + Heartbeat

Calendar follows the standard component lifecycle from JOB_QUEUE_PLAN.md §2.8.

**File**: `calendar/mod.rs`

```rust
impl Calendar {
    /// Spawn JobQueue workers and the attestation wall-clock heartbeat.
    pub fn start(&self) {
        self.job_queue.start();

        // Wall-clock attestation heartbeat
        let interval = Duration::from_secs(self.config.attestation_clock_interval_s);
        let jq = self.job_queue.clone();
        let last_attest = self.last_attestation_wallclock.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                jq.enqueue(CalendarJob::DoAttestation {
                    peers: vec![], // will be filled at execute time from Communerd
                    triggered_by: AttestationTrigger::WallClock,
                }).ok();
            }
        });
    }
}
```

**Task 4.5** — Wire Calendar `start()` into TimeFamilyServer
- **File**: `server/mod.rs`
- After creating Calendar and Communerd:
  ```rust
  calendar.start();
  communerd.register_peer_callback(Arc::clone(&calendar) as Arc<dyn PeerChangeCallback>);
  ```
- **Verification**: Server starts, workers spawn, heartbeat fires, callback fires on peer change

---

## 8. CONFIGURATION

```rust
pub struct CalendarConfig {
    // Job queue (shared abstraction from JOB_QUEUE_PLAN.md)
    pub job_queue_workers: usize,     // default: 4
    pub job_queue_max_depth: usize,   // default: 64

    // Periods
    pub period_config: PeriodConfig,

    // Mirroring (priority 4-5)
    pub max_active_mirrors: usize,        // default: 8
    pub max_outbound_mirrors: usize,      // default: 4 (how many peers we ask to mirror us)

    // Mutual attestation (priority 3)
    pub attestation_tick_interval: u64,   // default: 100
    pub attestation_clock_interval_s: u64, // default: 600
    pub attestation_max_peers_per_round: usize, // default: 4
}
```

---

## 9. TEST PLAN

### Phase 1 — Periods

| Test | Type | Verification |
|------|------|------|
| `period_finalizes_at_size` | Unit | 1000 ticks → 1 finalized period |
| `partial_last_period` | Unit | 1500 ticks → 1 full + 1 partial (500) |
| `dual_tbid_preserved` | Unit | Period has correct chronomatter_tbid AND calendar_tbid |
| `period_index_contiguous` | Unit | No gaps or overlaps |
| `tick_count_per_pair_accurate` | Unit | 3 chronomatter × 2 calendar = 6 correct counts |
| `periods_query_all` | Unit | Null filters return all indices |
| `periods_query_by_pair` | Unit | Specific pair returns only matching index |
| `period_metadata_expandable` | Unit | Metadata field accepts arbitrary key-value pairs |

### Phase 2 — History Dump

| Test | Type | Verification |
|------|------|------|
| `dump_single_period` | Unit (mock) | 1000 ticks in 1 period, TBIDs preserved |
| `dump_multiple_periods` | Unit (mock) | 2500 ticks in 3 periods |
| `dump_respects_up_to_tick` | Unit (mock) | up_to=1500 → only ticks 0-1500 |
| `dump_checksum_match` | Unit | Mirror's checksum matches source |
| `dump_checksum_mismatch_aborts` | Unit | Bad checksum → rejected |
| `dump_end_to_end` | Integration | 2 servers, dump completes |

### Phase 3 — Stream

| Test | Type | Verification |
|------|------|------|
| `stream_delivers_ticks` | Unit (mock) | Source produces, mirror receives |
| `stream_from_tick_inclusive` | Unit | from_tick=N delivers N onward |
| `stream_backpressure` | Unit | Slow mirror → buffer full → stall |
| `stream_stall_timeout` | Unit | 60s stall → clean termination |
| `stream_stop_by_mirror` | Unit | Mirror stops → source halts |
| `stream_termination_reenqueues_task` | Unit | Dead stream → FindNewMirror on queue |
| `stream_end_to_end` | Integration | 2 servers, stream delivers |

### Phase 4 — Task Queue & Orchestration

| Test | Type | Verification |
|------|------|------|
| `task_queue_executes_tasks` | Unit | Enqueue → execute → complete |
| `task_queue_retries_on_failure` | Unit | Failed task re-enqueued with backoff |
| `task_queue_drops_low_priority_when_full` | Unit | Queue full → priority 5 dropped first |
| `peer_callback_enqueues_explore` | Unit | New peer → ExploreMirror enqueued |
| `peer_callback_enqueues_find_mirror_on_loss` | Unit | Lost peer → FindNewMirror enqueued |
| `attestation_tick_trigger` | Unit | Every N ticks → DoAttestation enqueued |
| `attestation_clock_trigger` | Unit | Every M seconds → DoAttestation enqueued |
| `full_mirror_lifecycle` | Integration | Discover → explore → dump → stream |
| `mirror_failure_searches_new` | Integration | Stream dies → FindNewMirror → new mirror found |
| `priority_ordering_respected` | Integration | Priority 1-2 never starved by priority 4-5 |

---

## 10. NEXT STEPS (after this iteration)

These are deliberately deferred but planned:

| Feature | Description |
|---------|-------------|
| Reconciliation loop | Periodic hash sanity checks per CALENDAR_REPLICATION_SPEC.md §5.3 — compare mirror state with source on a schedule |
| Encrypted mirror storage | Encrypt period files on disk so mirrored data is not readable without the Calendar's key |
| Route stamp for mirror data | Use route stamps rather than direct JSON-RPC for mirror data integrity |
| Gossip-based mirror propagation | Multi-hop mirror distribution beyond PtP |
| Combined public key in Period metadata | Populate the `metadata` field on Period with the combined public key for signature verification chains |
| MirrorStore compaction | Merge small period files, reclaim space from pruned mirrors |
| Mirror consistency guarantees | CRDT-style or version-vector based conflict resolution for divergent mirror histories |
| SPHINCS+ TBID identity (SLH-DSA-256s/f) | Replace random TBID (`[u8; 16]`) with SPHINCS+ public key from **level 5 SLH-DSA-256s/f** parameter set. Public key is **64 bytes** (PK.seed \|\| PK.root, per FIPS 205 §9.1). TBID becomes a verifiable Calendar identity — every Period and tick is bound to the Calendar's SPHINCS+ key. Key generation uses `liboqs` (already in dependency chain via `core-engine`). Tbid type changes from `[u8; 16]` to `[u8; 64]`. Ripples through Foretis, TickRecord, DHT keys, mirror storage, bindings. |
| Sporadic SPHINCS+ tick signing | Sign ticks with the Time Being's SPHINCS+ identity key — NOT every tick (SPHINCS+ signatures are ~7.8 KB), but periodically (e.g., every finalized period, or every N ticks). The signature covers the Period's checksum + metadata. Allows mirrors to verify origin authenticity without trusting the stream. Un-signed ticks rely on Period checksum + chain-of-trust via forward/backward foretis. |
| Signed mirror agreement | The mirror relationship itself becomes a signed agreement: both parties sign a `MirrorAgreement { source_tbid, mirror_tbid, period_id, start_tick, agreed_terms }` using their SPHINCS+ keys. Provides non-repudiation — either party can prove the mirror was established and under what terms. |
| `cancel_mirror` RPC (courtesy notice) | Mirror sends `cancel_mirror { tbid, reason, last_tick_mirrored }` to source as a courtesy notification that it has stopped mirroring. Source enqueues `FindNewMirror`. Not a teardown contract — just a courtesy so the source doesn't have to discover the loss through reconciliation. Wire format: `{"method": "cancel_mirror", "params": {"tbid": "<hex>", "reason": "...", "last_tick_mirrored": N}}`. |

---

## 11. MILESTONE CHECKLIST

```
Phase 1 — Periods
[ ] M1  PeriodConfig added to NodeConfig
[ ] M2  Period + PeriodIndex with dual TBIDs + expandable metadata
[ ] M3  PeriodOps trait (finalization, queries, retrieval)
[ ] M4  MirrorStore reorganized for dual-TBID partitioning
[ ] M5  periods_query RPC handler
[ ] M6  Phase 1 tests pass

Phase 2 — History Dump
[ ] M7  history_dump_request transport method
[ ] M8  handle_history_dump_request (source)
[ ] M9  initiate_history_dump (Calendar, called by task queue)
[ ] M10 Checksum verification (match + mismatch)
[ ] M11 Phase 2 tests pass

Phase 3 — Stream
[ ] M12 stream_request transport method
[ ] M13 handle_stream_request (source)
[ ] M14 initiate_stream (Calendar, called by task queue)
[ ] M15 Backpressure + stall timeout
[ ] M16 Stream termination → task re-enqueue
[ ] M17 Phase 3 tests pass

Phase 4 — Orchestration (uses shared JobQueue from JOB_QUEUE_PLAN.md)
[ ] M18 Communerd peer callback registration (register_peer_callback)
[ ] M19 CalendarJob enum implementing Job trait (in calendar/mod.rs)
[ ] M20 Calendar's PeerChangeCallback (enqueue CalendarJob on peer changes)
[ ] M21 Task execution chain (Explore → Request → Dump → Stream)
[ ] M22 Mutual attestation scheduling (tick count + wall clock triggers)
[ ] M23 Calendar + JobQueue wired into TimeFamilyServer.start_daemon_arc()
[ ] M24 Integration tests pass
[ ] M25 TAG: v0.5-active-mirroring
```
