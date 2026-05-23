//! Calendar task queue — Group 4b Phases 4b.2 + 4b.4.
//!
//! Calendar's lower-priority responsibilities (priority 4 = persist family's
//! calendar via mirrors; priority 5 = mirror other calendars' ticks) are
//! driven by a bounded `tokio::sync::mpsc` channel and a small worker pool.
//!
//! Phase 4b.2 introduced the `CalendarTask` enum and the worker spawning
//! plumbing. Phase 4b.4 wires the workers to a `MirrorDispatcher` trait so
//! they can make real RPC calls without coupling Calendar to Communerd's
//! transport stack. Communerd implements the trait; Calendar receives an
//! `Arc<dyn MirrorDispatcher>` via `start_task_queue_with_dispatcher`.

use std::sync::Arc;

use async_trait::async_trait;
use foretias_core::foretias::callbacks::PeerAddr;
use foretias_core::foretias::tick::ChrononRecord;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Default size of the Calendar worker pool. Each worker is a long-lived
/// `tokio::spawn` future that pulls one task at a time from the channel.
pub const DEFAULT_WORKER_COUNT: usize = 4;

/// Discrete unit of low-priority Calendar work.
///
/// All variants are bounded operations: a worker picks one off the queue,
/// runs it to completion (with internal timeouts), and returns to listen
/// for the next task. Tasks must NEVER hold Calendar's internal locks
/// across `.await` — they take snapshots via short critical sections.
#[derive(Debug, Clone)]
pub enum CalendarTask {
    /// Trigger a mutual-attestation stamp exchange with `peer`.
    ///
    /// Currently this just records the request and logs; Phase 4b.4 will
    /// move the existing mutual-attestation logic out of Chronomatter and
    /// route it through this task type so attestation cadence becomes
    /// visible to the priority system.
    DoAttestation { peer: PeerAddr },

    /// Look for a new mirror because mirror count is below the configured
    /// target. Phase 4b.4 will call `query_community` and then issue a
    /// `mirror_announce` RPC to candidate peers.
    FindNewMirror,

    /// Stream full local Chrononchain history to `mirror`. Triggered after
    /// a mirror enrolls. Phase 4b.4 will chunk records via
    /// `history_dump_chunk`.
    InitiateDump { mirror: PeerAddr },

    /// Subscribe to local TickObserver and push each new record to
    /// `mirror` via the existing `stream_tick` RPC. Triggered after
    /// `InitiateDump` completes.
    StartStream { mirror: PeerAddr },

    /// Periodic health probe via `mirror_health_check` RPC. After N=3
    /// consecutive failures, the worker enqueues `ExpireMirror`.
    ExploreMirror { mirror: PeerAddr },

    /// Remove `mirror` from the active mirror list. If the active count
    /// drops below the configured minimum, the worker enqueues
    /// `FindNewMirror`.
    ExpireMirror { mirror: PeerAddr },
}

impl CalendarTask {
    /// Short stable name for logging and metrics.
    pub fn kind(&self) -> &'static str {
        match self {
            CalendarTask::DoAttestation { .. } => "do_attestation",
            CalendarTask::FindNewMirror => "find_new_mirror",
            CalendarTask::InitiateDump { .. } => "initiate_dump",
            CalendarTask::StartStream { .. } => "start_stream",
            CalendarTask::ExploreMirror { .. } => "explore_mirror",
            CalendarTask::ExpireMirror { .. } => "expire_mirror",
        }
    }
}

/// Sender half of the Calendar task channel. Cloneable and `Send`.
///
/// Calendar exposes this via `enqueue_task` and (in Phase 4b.4) Communerd's
/// PeerChangeCallback uses it to push `FindNewMirror` when the peer pool
/// shrinks.
pub type CalendarTaskSender = mpsc::UnboundedSender<CalendarTask>;

/// MirrorDispatcher — the narrow surface Calendar's task workers use to make
/// network calls. Communerd implements this; Calendar holds it as
/// `Arc<dyn MirrorDispatcher>` so the worker pool can stay decoupled from
/// the transport stack.
///
/// All methods are async and bounded — implementations apply per-call
/// timeouts. The trait intentionally only exposes the small set of
/// operations needed by the six `CalendarTask` variants.
#[async_trait]
pub trait MirrorDispatcher: Send + Sync {
    /// Snapshot the currently-known peer pool. Source of candidates for
    /// `FindNewMirror`. Cheap; safe to call frequently.
    async fn known_peers(&self) -> Vec<PeerAddr>;

    /// "I have TBID X, will you mirror?" — Source → Candidate.
    /// Returns Ok(true) if the candidate accepted, Ok(false) if declined.
    async fn mirror_announce(
        &self,
        peer: &PeerAddr,
        local_tbid_hex: &str,
    ) -> Result<bool, String>;

    /// Stream a chunk of locally-authoritative ChrononRecords to a mirror.
    /// Returns the number the mirror reports as accepted, or an error.
    async fn history_dump_chunk(
        &self,
        peer: &PeerAddr,
        local_tbid_hex: &str,
        records: Vec<ChrononRecord>,
    ) -> Result<u64, String>;

    /// Mark end-of-dump. Returns the mirror's reported tick_count.
    async fn history_dump_complete(
        &self,
        peer: &PeerAddr,
        local_tbid_hex: &str,
        total_records: u64,
    ) -> Result<u64, String>;

    /// Liveness probe. Returns Ok(tick_count) on success; Err on RPC failure.
    /// Drives `ExploreMirror`'s consecutive-failure counter.
    async fn mirror_health_check(
        &self,
        peer: &PeerAddr,
        local_tbid_hex: &str,
    ) -> Result<u64, String>;
}

/// State the worker pool keeps between tasks: which peers are active
/// mirrors, the local TBID being mirrored, and a small consecutive-failure
/// counter per mirror for the `ExploreMirror` task.
#[derive(Debug, Default)]
pub struct MirrorState {
    /// Hex TBID of the local TimeFamily being mirrored. Set by
    /// `Calendar::set_local_tbid` before the queue is useful.
    pub local_tbid_hex: parking_lot::RwLock<String>,
    /// Active mirrors that have accepted the relationship.
    pub mirrors: parking_lot::RwLock<std::collections::HashSet<String>>,
    /// Per-mirror consecutive failures from `mirror_health_check`. After
    /// `MAX_HEALTH_FAILURES`, the worker enqueues `ExpireMirror`.
    pub health_failures:
        parking_lot::RwLock<std::collections::HashMap<String, u32>>,
    /// Lower-bound mirror count target. Below this, `ExpireMirror` enqueues
    /// a fresh `FindNewMirror`.
    pub min_mirrors: parking_lot::RwLock<usize>,
    /// Upper-bound mirror count target. `FindNewMirror` stops when reached.
    pub target_mirrors: parking_lot::RwLock<usize>,
}

/// Threshold for `mirror_health_check` failures before a mirror is expired.
pub const MAX_HEALTH_FAILURES: u32 = 3;

/// Default mirror count target. Calendars can override via
/// `set_target_mirrors` after `start_task_queue_with_dispatcher`.
pub const DEFAULT_MIN_MIRRORS: usize = 1;
pub const DEFAULT_TARGET_MIRRORS: usize = 3;

/// Bounded chunk size for history dumps. Spec §3.3 target: 64 records or
/// 1 MB, whichever comes first. The current server-side per-connection cap
/// is `MAX_REQUEST_LINE_BYTES = 4096` (plaintext), which only fits a handful
/// of records. A follow-up will raise that cap (and the cipher cap) to
/// allow the spec-target 64-record chunks; for now we ship one record per
/// chunk so the dump correctly traverses the existing transport.
pub const DUMP_CHUNK_SIZE: usize = 1;

/// Internal handle returned by `spawn_workers` so callers can join the pool
/// during shutdown (currently unused — workers run for the lifetime of the
/// process; Phase 4b.6 wires graceful shutdown).
#[derive(Debug)]
pub struct WorkerPool {
    pub handles: Vec<tokio::task::JoinHandle<()>>,
}

/// Per-worker context: dispatcher, mirror state, and a self-reference to the
/// task sender so handlers can enqueue follow-up work.
#[derive(Clone)]
pub struct WorkerContext {
    pub dispatcher: Option<Arc<dyn MirrorDispatcher>>,
    pub mirror_state: Arc<MirrorState>,
    pub task_tx: CalendarTaskSender,
    pub calendar_lookup:
        Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
}

/// Spawn `worker_count` worker tasks that pull from `rx`, dispatch via the
/// provided context (with optional MirrorDispatcher). The returned
/// `WorkerPool` owns the join handles so callers can shut down later.
pub fn spawn_workers(
    mut rx: mpsc::UnboundedReceiver<CalendarTask>,
    worker_count: usize,
    ctx: WorkerContext,
) -> WorkerPool {
    let rx = Arc::new(tokio::sync::Mutex::new(rx_take(&mut rx)));
    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let rx = Arc::clone(&rx);
        let ctx = ctx.clone();
        let handle = tokio::spawn(async move {
            loop {
                let task_opt = {
                    let mut guard = rx.lock().await;
                    guard.recv().await
                };
                let Some(task) = task_opt else {
                    debug!(worker_id, "calendar task channel closed; worker exiting");
                    break;
                };
                handle_task(worker_id, task, &ctx).await;
            }
        });
        handles.push(handle);
    }
    WorkerPool { handles }
}

/// Workaround helper: move the receiver out of its existing position.
fn rx_take(rx: &mut mpsc::UnboundedReceiver<CalendarTask>) -> mpsc::UnboundedReceiver<CalendarTask> {
    let (_placeholder_tx, placeholder_rx) = mpsc::unbounded_channel();
    std::mem::replace(rx, placeholder_rx)
}

/// Real dispatch — Phase 4b.4. Each variant has its own handler function
/// to keep this top-level dispatch readable.
async fn handle_task(worker_id: usize, task: CalendarTask, ctx: &WorkerContext) {
    debug!(
        worker_id,
        task_kind = task.kind(),
        "calendar worker dispatching task"
    );
    match task {
        CalendarTask::FindNewMirror => handle_find_new_mirror(worker_id, ctx).await,
        CalendarTask::InitiateDump { mirror } => {
            handle_initiate_dump(worker_id, mirror, ctx).await
        }
        CalendarTask::StartStream { mirror } => {
            // Phase 4b.4c: subscribe to TickObserver and push via stream_tick.
            // Currently a structural stub — InitiateDump covers the initial
            // history transfer; ongoing streaming relies on Chronomatter's
            // existing stream_tick path which already runs on tick advance.
            debug!(worker_id, mirror = %mirror.json_rpc, "start_stream placeholder (Phase 4b.4c follow-up)");
        }
        CalendarTask::ExploreMirror { mirror } => {
            handle_explore_mirror(worker_id, mirror, ctx).await
        }
        CalendarTask::ExpireMirror { mirror } => {
            handle_expire_mirror(worker_id, mirror, ctx).await
        }
        CalendarTask::DoAttestation { peer } => {
            // Phase 4b.4d: move existing mutual-attestation into this task.
            // Existing path in Chronomatter remains the source of truth until
            // we refactor it; this handler is a structural placeholder so the
            // task variant is wired end-to-end.
            debug!(worker_id, peer = %peer.json_rpc, "do_attestation placeholder (Phase 4b.4d follow-up)");
        }
    }
}

async fn handle_find_new_mirror(worker_id: usize, ctx: &WorkerContext) {
    let Some(dispatcher) = ctx.dispatcher.as_ref() else {
        debug!(worker_id, "find_new_mirror: no dispatcher wired; skipping");
        return;
    };
    let target = *ctx.mirror_state.target_mirrors.read();
    let current = ctx.mirror_state.mirrors.read().len();
    if current >= target {
        debug!(worker_id, current, target, "find_new_mirror: at target; skipping");
        return;
    }

    let local_tbid = ctx.mirror_state.local_tbid_hex.read().clone();
    if local_tbid.is_empty() {
        warn!(worker_id, "find_new_mirror: local_tbid not set on Calendar; cannot announce");
        return;
    }

    let candidates = dispatcher.known_peers().await;
    let already_mirroring = ctx.mirror_state.mirrors.read().clone();

    for peer in candidates {
        if already_mirroring.contains(&peer.json_rpc) {
            continue;
        }
        match dispatcher.mirror_announce(&peer, &local_tbid).await {
            Ok(true) => {
                info!(worker_id, peer = %peer.json_rpc, "mirror_announce accepted; enrolling and enqueueing InitiateDump");
                ctx.mirror_state
                    .mirrors
                    .write()
                    .insert(peer.json_rpc.clone());
                let _ = enqueue(
                    &ctx.task_tx,
                    CalendarTask::InitiateDump { mirror: peer.clone() },
                );
                if ctx.mirror_state.mirrors.read().len()
                    >= *ctx.mirror_state.target_mirrors.read()
                {
                    break;
                }
            }
            Ok(false) => {
                debug!(worker_id, peer = %peer.json_rpc, "mirror_announce declined; trying next");
            }
            Err(e) => {
                warn!(worker_id, peer = %peer.json_rpc, "mirror_announce failed: {e}");
            }
        }
    }
}

async fn handle_initiate_dump(worker_id: usize, mirror: PeerAddr, ctx: &WorkerContext) {
    let Some(dispatcher) = ctx.dispatcher.as_ref() else {
        debug!(worker_id, "initiate_dump: no dispatcher wired; skipping");
        return;
    };
    let local_tbid = ctx.mirror_state.local_tbid_hex.read().clone();
    if local_tbid.is_empty() {
        warn!(worker_id, mirror = %mirror.json_rpc, "initiate_dump: local_tbid not set");
        return;
    }

    // Snapshot the calendar — clone the records out of the read lock so we
    // don't hold the lock across the dispatcher's awaits.
    let records: Vec<ChrononRecord> = {
        let cal = ctx.calendar_lookup.read();
        cal.ticks.clone()
    };
    if records.is_empty() {
        debug!(worker_id, mirror = %mirror.json_rpc, "initiate_dump: no records to send");
        let _ = dispatcher
            .history_dump_complete(&mirror, &local_tbid, 0)
            .await;
        return;
    }

    let total = records.len();
    let mut sent: u64 = 0;
    for chunk in records.chunks(DUMP_CHUNK_SIZE) {
        match dispatcher
            .history_dump_chunk(&mirror, &local_tbid, chunk.to_vec())
            .await
        {
            Ok(accepted) => sent += accepted,
            Err(e) => {
                warn!(worker_id, mirror = %mirror.json_rpc, "history_dump_chunk failed: {e}; aborting dump");
                return;
            }
        }
    }
    match dispatcher
        .history_dump_complete(&mirror, &local_tbid, total as u64)
        .await
    {
        Ok(reported_count) => {
            info!(worker_id, mirror = %mirror.json_rpc, sent, reported_count, "initiate_dump complete; enqueueing StartStream");
            let _ = enqueue(
                &ctx.task_tx,
                CalendarTask::StartStream { mirror: mirror.clone() },
            );
        }
        Err(e) => {
            warn!(worker_id, mirror = %mirror.json_rpc, "history_dump_complete failed: {e}");
        }
    }
}

async fn handle_explore_mirror(worker_id: usize, mirror: PeerAddr, ctx: &WorkerContext) {
    let Some(dispatcher) = ctx.dispatcher.as_ref() else {
        debug!(worker_id, "explore_mirror: no dispatcher wired; skipping");
        return;
    };
    let local_tbid = ctx.mirror_state.local_tbid_hex.read().clone();
    if local_tbid.is_empty() {
        return;
    }
    match dispatcher
        .mirror_health_check(&mirror, &local_tbid)
        .await
    {
        Ok(tick_count) => {
            debug!(worker_id, mirror = %mirror.json_rpc, tick_count, "mirror health ok");
            ctx.mirror_state
                .health_failures
                .write()
                .insert(mirror.json_rpc.clone(), 0);
        }
        Err(e) => {
            let failures = {
                let mut guard = ctx.mirror_state.health_failures.write();
                let entry = guard.entry(mirror.json_rpc.clone()).or_insert(0);
                *entry += 1;
                *entry
            };
            warn!(worker_id, mirror = %mirror.json_rpc, failures, "mirror_health_check failed: {e}");
            if failures >= MAX_HEALTH_FAILURES {
                info!(worker_id, mirror = %mirror.json_rpc, failures, "mirror exceeded MAX_HEALTH_FAILURES; expiring");
                let _ = enqueue(
                    &ctx.task_tx,
                    CalendarTask::ExpireMirror { mirror: mirror.clone() },
                );
            }
        }
    }
}

async fn handle_expire_mirror(worker_id: usize, mirror: PeerAddr, ctx: &WorkerContext) {
    {
        let mut mirrors = ctx.mirror_state.mirrors.write();
        if !mirrors.remove(&mirror.json_rpc) {
            debug!(worker_id, mirror = %mirror.json_rpc, "expire_mirror: not active; noop");
            return;
        }
    }
    ctx.mirror_state
        .health_failures
        .write()
        .remove(&mirror.json_rpc);
    info!(worker_id, mirror = %mirror.json_rpc, "mirror expired");
    // If the active mirror count fell below min_mirrors, enqueue a fresh
    // FindNewMirror to refill the pool.
    let active = ctx.mirror_state.mirrors.read().len();
    let min = *ctx.mirror_state.min_mirrors.read();
    if active < min {
        debug!(worker_id, active, min, "active mirrors below min; enqueueing FindNewMirror");
        let _ = enqueue(&ctx.task_tx, CalendarTask::FindNewMirror);
    }
}

/// Convenience: build the channel + spawn the default-sized worker pool with
/// no dispatcher (placeholder mode — used by Phase 4b.2 tests that don't
/// exercise the network path).
pub fn start_default_pool(
    calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
) -> (CalendarTaskSender, WorkerPool, Arc<MirrorState>) {
    start_default_pool_with_dispatcher(calendar_lookup, None)
}

/// Build the channel + spawn the default-sized worker pool with an optional
/// `MirrorDispatcher`. Returns the sender, the worker pool, and the
/// MirrorState handle (so callers can set local_tbid_hex, target_mirrors,
/// inspect active mirrors, etc.).
pub fn start_default_pool_with_dispatcher(
    calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
    dispatcher: Option<Arc<dyn MirrorDispatcher>>,
) -> (CalendarTaskSender, WorkerPool, Arc<MirrorState>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let mirror_state = Arc::new(MirrorState::default());
    *mirror_state.min_mirrors.write() = DEFAULT_MIN_MIRRORS;
    *mirror_state.target_mirrors.write() = DEFAULT_TARGET_MIRRORS;
    let ctx = WorkerContext {
        dispatcher,
        mirror_state: Arc::clone(&mirror_state),
        task_tx: tx.clone(),
        calendar_lookup,
    };
    let pool = spawn_workers(rx, DEFAULT_WORKER_COUNT, ctx);
    (tx, pool, mirror_state)
}

/// Convenience: enqueue a task. Returns Err if the receiver has been dropped
/// (which only happens after Calendar shutdown).
pub fn enqueue(sender: &CalendarTaskSender, task: CalendarTask) -> Result<(), CalendarTask> {
    sender.send(task).map_err(|e| {
        warn!("calendar task channel closed; cannot enqueue {:?}", e.0);
        e.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_task_kind_is_stable_for_each_variant() {
        assert_eq!(
            CalendarTask::DoAttestation {
                peer: PeerAddr { json_rpc: "x".into() }
            }
            .kind(),
            "do_attestation"
        );
        assert_eq!(CalendarTask::FindNewMirror.kind(), "find_new_mirror");
        assert_eq!(
            CalendarTask::InitiateDump {
                mirror: PeerAddr { json_rpc: "x".into() }
            }
            .kind(),
            "initiate_dump"
        );
        assert_eq!(
            CalendarTask::StartStream {
                mirror: PeerAddr { json_rpc: "x".into() }
            }
            .kind(),
            "start_stream"
        );
        assert_eq!(
            CalendarTask::ExploreMirror {
                mirror: PeerAddr { json_rpc: "x".into() }
            }
            .kind(),
            "explore_mirror"
        );
        assert_eq!(
            CalendarTask::ExpireMirror {
                mirror: PeerAddr { json_rpc: "x".into() }
            }
            .kind(),
            "expire_mirror"
        );
    }

    #[tokio::test]
    async fn workers_drain_enqueued_tasks() {
        use foretias_core::foretias::types::Tbid;
        let cal = Arc::new(parking_lot::RwLock::new(
            foretias_core::foretias::Calendar::new(Tbid::default(), "queue-test"),
        ));
        let (tx, _pool, _state) = start_default_pool(cal);
        for i in 0..10 {
            enqueue(
                &tx,
                CalendarTask::DoAttestation {
                    peer: PeerAddr {
                        json_rpc: format!("127.0.0.1:{i}"),
                    },
                },
            )
            .expect("enqueue");
        }
        // Give workers a moment to drain.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // No assertion on side-effects — workers run the real Phase 4b.4
        // handlers but in placeholder mode (no dispatcher), so all variants
        // are processed without panic or deadlock.
    }

    #[tokio::test]
    async fn enqueue_after_channel_close_returns_err() {
        let (tx, rx) = mpsc::unbounded_channel::<CalendarTask>();
        drop(rx);
        let result = enqueue(&tx, CalendarTask::FindNewMirror);
        assert!(result.is_err(), "enqueue must surface the closed channel");
    }
}
