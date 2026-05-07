# JobQueue — Shared Async Task Dispatcher

**Status**: Plan — pending human review
**Date**: 2026-05-06
**Consumers**: Communerd (peer liveness), Chronomatter (ticking), Calendar (mirroring, attestation)

---

## 1. PROBLEM

Three components each need an independent async work loop with their own task types:

| Component | What it does with its queue | Task types |
|-----------|---------------------------|------------|
| **Communerd** | Peer liveness pings, DHT gossip processing, peer addition/removal | LivenessPing, ProcessGossipRecord, AddPeer, RemovePeer |
| **Chronomatter** | Periodic ticking, attestation production, epoch transitions | Tick, ProduceAttestation, EpochTransition |
| **Calendar** | Mirroring lifecycle, mutual attestation, peer exploration | ExploreMirror, InitiateDump, StartStream, DoAttestation, FindNewMirror |

Each component instantiates its own `JobQueue` with its own task enum. The abstraction is shared; the task types are not.

---

## 2. DESIGN

### 2.1 Core Abstraction

```rust
pub struct JobQueue<T: Job> {
    sender: mpsc::Sender<TaskEntry<T>>,
    workers: Vec<Option<tokio::task::JoinHandle<()>>>,
    config: JobQueueConfig,
}

pub trait Job: Send + Sync + 'static {
    /// Numeric priority. Lower number = higher priority.
    /// When the queue is full, lowest priority (highest number) is dropped first.
    fn priority(&self) -> u8;

    /// Async execution of the job. The executor passes a handle that allows
    /// the job to re-enqueue itself or spawn sibling jobs.
    fn execute(&self, executor: &JobExecutor<T>) -> impl std::future::Future<Output = JobResult> + Send;

    /// Maximum wall-clock time for this job. On timeout, the job is aborted.
    fn timeout(&self) -> Duration { Duration::from_secs(30) }
}

pub enum JobResult {
    Ok,
    Retry(Backoff),   // Re-enqueue with backoff
    Fatal(String),    // Do not retry, log reason
}

#[derive(Debug, Clone)]
pub enum Backoff {
    Immediate,
    Fixed(Duration),
    Exponential { base: Duration, max: Duration },
}
```

### 2.2 Configuration

```rust
pub struct JobQueueConfig {
    /// Number of concurrent worker tasks (tokio::spawn per worker)
    pub workers: usize,            // default: 4

    /// Maximum depth of the internal channel. When full, lowest-priority
    /// job is dropped to make room for the incoming job.
    pub max_queue_depth: usize,    // default: 64

    /// Default timeout for jobs that don't specify one
    pub default_timeout: Duration, // default: 30s

    /// Maximum retry attempts before giving up on a job
    pub max_retries: u32,          // default: 3
}
```

### 2.3 Instantiation Pattern

Each component creates its own queue in its constructor or init method:

```rust
// Communerd
impl Communerd {
    pub fn new(config: CommunerdConfig, ...) -> Self {
        let job_queue = JobQueue::new(JobQueueConfig {
            workers: config.job_queue_workers,
            max_queue_depth: config.job_queue_max_depth,
            ..Default::default()
        });
        // Communerd enqueues CommunerdJob variants
    }
}

// Chronomatter
impl Chronomatter {
    pub fn new(config: ChronomatterConfig, ...) -> Self {
        let job_queue = JobQueue::new(JobQueueConfig {
            workers: config.job_queue_workers,
            max_queue_depth: config.job_queue_max_depth,
            ..Default::default()
        });
        // Chronomatter enqueues ChronomatterJob variants
    }
}

// Calendar
impl Calendar {
    pub fn new(config: CalendarConfig, ...) -> Self {
        let job_queue = JobQueue::new(JobQueueConfig {
            workers: config.job_queue_workers,
            max_queue_depth: config.job_queue_max_depth,
            ..Default::default()
        });
        // Calendar enqueues CalendarJob variants
    }
}
```

### 2.4 Enqueueing

```rust
impl<T: Job> JobQueue<T> {
    /// Enqueue a job. If the queue is full and the new job's priority is
    /// higher (lower number) than the lowest-priority job in the queue,
    /// the lowest-priority job is evicted.
    pub fn enqueue(&self, job: T) -> Result<(), EnqueueError> {
        // ...
    }
}

pub enum EnqueueError {
    QueueFull,           // New job is lowest priority and queue is full
    ShuttingDown,        // Queue is being torn down
}
```

### 2.5 Worker Loop

Each worker is a `tokio::task::JoinHandle` spawned on the current runtime:

```
loop {
    select! {
        Some(entry) = receiver.recv() => {
            let deadline = Instant::now() + entry.job.timeout();
            select! {
                result = entry.job.execute(&executor) => {
                    match result {
                        JobResult::Ok => { /* done */ }
                        JobResult::Retry(backoff) => {
                            sleep(backoff.delay());
                            executor.reenqueue(entry.job);
                        }
                        JobResult::Fatal(reason) => {
                            tracing::error!(%reason, "job failed fatally");
                        }
                    }
                }
                _ = sleep_until(deadline) => {
                    tracing::warn!("job timed out");
                    // Re-enqueue with backoff if retries remain
                }
            }
        }
        else => break,  // sender dropped, shut down
    }
}
```

### 2.6 JobExecutor — Re-enqueue Handle

Jobs need to be able to enqueue sibling jobs (e.g., an `ExploreMirror` job that discovers a calendar enqueues a `RequestMirror` job). The executor provides this:

```rust
pub struct JobExecutor<T: Job> {
    sender: mpsc::Sender<TaskEntry<T>>,
    max_retries: u32,
}

impl<T: Job> JobExecutor<T> {
    pub fn enqueue(&self, job: T) -> Result<(), EnqueueError> { ... }
    pub fn remaining_retries(&self, original_job_id: Uuid) -> u32 { ... }
}
```

### 2.7 Start / Stop Lifecycle

```rust
impl<T: Job> JobQueue<T> {
    /// Spawn worker tasks on the current tokio runtime.
    /// Must be called before enqueueing jobs.
    pub fn start(&mut self) {
        for _ in 0..self.config.workers {
            self.workers.push(Some(tokio::spawn(worker_loop(
                self.sender.clone(),
                self.config.clone(),
            ))));
        }
    }

    /// Drop all senders, join all workers, clean up.
    pub async fn stop(&mut self) {
        drop(self.sender);
        for handle in &mut self.workers {
            if let Some(h) = handle.take() {
                h.await.ok();
            }
        }
    }
}
```

### 2.8 Heartbeat / Periodic Drivers

The `JobQueue` worker loop is **reactive** — it processes jobs that are enqueued. It does not generate work on its own. Each component must spawn its own **heartbeat loops** that periodically enqueue monitoring jobs.

The pattern is identical for every component: a `tokio::spawn` with a `tokio::time::interval` loop that enqueues into the component's JobQueue.

```rust
// Generic pattern — every component follows this:
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_s));
    loop {
        interval.tick().await;
        job_queue.enqueue(JobVariant::MonitoringTask).ok();
    }
});
```

No additional abstraction is needed. This is idiomatic Tokio.

**Heartbeat summary per component:**

| Component | Heartbeat | Interval | Job Enqueued |
|-----------|-----------|----------|------|
| **Chronomatter** | Tick heartbeat | Configurable (e.g. 10s) | `ChronomatterJob::Tick` |
| **Communerd** | Peer liveness | ~30s | `CommunerdJob::LivenessPing { peer }` per peer |
| **Communerd** | DHT gossip refresh | ~120s | `CommunerdJob::ProcessGossipRecord` |
| **Calendar** | Attestation wall-clock | ~600s | `CalendarJob::DoAttestation { WallClock }` |

Calendar's **tick-count** attestation trigger is event-driven (fires on each new tick recorded), not a heartbeat. It enqueues directly in the recording path.

Each component spawns its heartbeat loops in its `start()` method. The `start()` contract:

```rust
pub trait TimeBeingComponent {
    /// Instantiate the JobQueue, spawn workers, spawn heartbeat loops.
    /// After this call, the component is fully operational.
    fn start(&self);

    /// Graceful shutdown — stop heartbeats, drain queue, join workers.
    fn stop(&mut self);
}
```

Chronomatter's `start()` spawns the tick heartbeat and its JobQueue workers.
Communerd's `start()` spawns the liveness heartbeat, gossip heartbeat, and its JobQueue workers.
Calendar's `start()` spawns the attestation clock heartbeat and its JobQueue workers.

**Heartbeat lifecycle**: Heartbeats are tied to the component's lifetime. `stop()` cancels heartbeat handles first, then drains the queue, then joins workers.

### 2.9 Location

---

## 3. TASKS

**Task 1** — Implement `JobQueue<T>` core
- File: `core-engine/src/job_queue.rs`
- Types: `JobQueue`, `JobQueueConfig`, `TaskEntry`, `JobExecutor`, `EnqueueError`
- Trait: `Job` with `priority()`, `execute()`, `timeout()`
- Enum: `JobResult`, `Backoff`
- Verification: `cargo check` passes

**Task 2** — Implement worker loop with timeout and retry
- File: `core-engine/src/job_queue.rs`
- `start()` spawns workers, `stop()` joins them
- Timeout handling, backoff re-enqueue
- Verification: Unit test — job that returns `Retry` is re-enqueued with backoff

**Task 3** — Implement priority-based eviction on enqueue
- File: `core-engine/src/job_queue.rs`
- When queue is full and incoming job has higher priority, evict lowest-priority job
- Verification: Unit test — fill queue with priority-5 jobs, enqueue priority-3 → priority-5 evicted

**Task 4** — Wire JobQueue into Communerd
- File: `communerd/mod.rs`
- Define `CommunerdJob` enum implementing `Job`
- Create `JobQueue<CommunerdJob>` in `Communerd::new()`
- Call `job_queue.start()` on startup
- Verification: Communerd starts, enqueues a dummy job, it executes

**Task 5** — Wire JobQueue into Chronomatter
- File: `core-engine/src/chronomatter/mod.rs`
- Define `ChronomatterJob` enum implementing `Job`
- Create `JobQueue<ChronomatterJob>` in `Chronomatter::new()`
- Verification: Chronomatter starts, ticking job executes

**Task 6** — Wire JobQueue into Calendar
- File: `foretias-node/src/calendar/mod.rs`
- Define `CalendarJob` enum implementing `Job`
- Create `JobQueue<CalendarJob>` in `Calendar::new()`
- Verification: Calendar starts, enqueues a dummy job, it executes

**Task 7** — Heartbeat for Chronomatter (tick loop)
- File: `core-engine/src/chronomatter/mod.rs`
- In `start()`: `tokio::spawn` a `tokio::interval` loop that enqueues `ChronomatterJob::Tick`
- Interval configurable via `ChronomatterConfig.tick_interval_s`
- In `stop()`: cancel the heartbeat handle before draining the queue
- Verification: Unit test — `Tick` jobs appear at regular intervals

**Task 8** — Heartbeat for Communerd (liveness + gossip)
- File: `communerd/mod.rs`
- In `start()`: spawn two interval loops:
  1. Liveness ping (~30s): enqueues `LivenessPing` for each peer in the pool
  2. Gossip refresh (~120s): enqueues `ProcessGossipRecord`
- In `stop()`: cancel both heartbeat handles
- Verification: Unit test — liveness pings fire periodically, gossip refresh fires at longer interval

**Task 9** — Heartbeat for Calendar (attestation wall-clock)
- File: `foretias-node/src/calendar/mod.rs`
- In `start()`: spawn an interval loop (~600s) that enqueues `CalendarJob::DoAttestation { WallClock }`
- Interval configurable via `CalendarConfig.attestation_clock_interval_s`
- In `stop()`: cancel the heartbeat handle
- Verification: Unit test — attestation clock job fires at the configured interval

---

## 4. TEST PLAN

| Test | Type | Verification |
|------|------|------|
| `job_queue_executes_jobs` | Unit | Enqueued job runs to completion |
| `job_queue_retry_with_backoff` | Unit | `Retry(Fixed)` re-enqueues after delay |
| `job_queue_timeout_aborts` | Unit | Long-running job hits timeout |
| `job_queue_priority_eviction` | Unit | High-priority job evicts low-priority when full |
| `job_queue_worker_count` | Unit | N workers → N jobs run concurrently |
| `job_queue_graceful_stop` | Unit | `stop()` drains queue, joins workers |
| `communerd_uses_job_queue` | Unit | Communerd enqueues and executes `CommunerdJob` |
| `chronomatter_uses_job_queue` | Unit | Chronomatter enqueues and executes `ChronomatterJob` |
| `calendar_uses_job_queue` | Unit | Calendar enqueues and executes `CalendarJob` |

---

## 5. NON-GOALS

- ❌ Persistent job storage — in-memory only
- ❌ Cross-queue communication — each queue is isolated (components coordinate via shared state, not cross-queue messaging)
- ❌ Distributed job scheduling — single-node only
- ❌ Job dependency graphs — jobs may enqueue siblings but there is no DAG tracking
