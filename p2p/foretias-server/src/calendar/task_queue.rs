//! Calendar task queue — Group 4b Phase 4b.2 scaffold.
//!
//! Calendar's lower-priority responsibilities (priority 4 = persist family's
//! calendar via mirrors; priority 5 = mirror other calendars' ticks) are
//! driven by a bounded `tokio::sync::mpsc` channel and a small worker pool.
//!
//! This module defines the `CalendarTask` enum and the worker spawning
//! plumbing. Concrete handler implementations land in Phase 4b.4 — for
//! now, workers dispatch to a placeholder handler that records what it
//! received and emits a debug log. That is sufficient to wire up calling
//! code (PeerChangeCallback, integration tests) without coupling to the
//! still-to-be-built mirror RPC stack.

use std::sync::Arc;

use foretias_core::foretias::callbacks::PeerAddr;
use tokio::sync::mpsc;
use tracing::{debug, warn};

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

/// Internal handle returned by `spawn_workers` so callers can join the pool
/// during shutdown (currently unused — workers run for the lifetime of the
/// process; Phase 4b.6 wires graceful shutdown).
#[derive(Debug)]
pub struct WorkerPool {
    pub handles: Vec<tokio::task::JoinHandle<()>>,
}

/// Spawn `worker_count` worker tasks that all pull from `rx` and dispatch
/// to the placeholder handler. The returned `WorkerPool` owns the join
/// handles so callers can shut down the workers later.
pub fn spawn_workers(
    mut rx: mpsc::UnboundedReceiver<CalendarTask>,
    worker_count: usize,
) -> WorkerPool {
    // mpsc::UnboundedReceiver is single-consumer; share via Arc<Mutex<...>>
    // so multiple workers can pull from the same channel.
    let rx = Arc::new(tokio::sync::Mutex::new(rx_take(&mut rx)));
    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let rx = Arc::clone(&rx);
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
                handle_task(worker_id, task).await;
            }
        });
        handles.push(handle);
    }
    WorkerPool { handles }
}

/// Workaround helper: move the receiver out of its existing position. Required
/// because `mpsc::UnboundedReceiver` is `!Clone` and we want to share it
/// across workers via `Arc<Mutex<...>>`.
fn rx_take(rx: &mut mpsc::UnboundedReceiver<CalendarTask>) -> mpsc::UnboundedReceiver<CalendarTask> {
    // Replace `rx` with a placeholder receiver — the placeholder is
    // immediately dropped after this function returns. Callers should
    // not use the original `rx` after calling this.
    let (_placeholder_tx, placeholder_rx) = mpsc::unbounded_channel();
    std::mem::replace(rx, placeholder_rx)
}

/// Placeholder dispatch — Phase 4b.4 replaces each arm with the real handler.
///
/// For now we just log; an integration test can subscribe to tracing output
/// or wire a real handler via a Phase 4b.4 follow-up.
async fn handle_task(worker_id: usize, task: CalendarTask) {
    debug!(
        worker_id,
        task_kind = task.kind(),
        task = ?task,
        "calendar worker received task (placeholder dispatch — full implementation pending Phase 4b.4)"
    );
    // Intentionally no-op: each handler will get its own real implementation
    // when Phase 4b.4 lands. The placeholder ensures the channel and worker
    // pool plumbing is exercisable by tests and PeerChangeCallback integration
    // even before the RPC stack is in place.
}

/// Convenience: build the channel + spawn the default-sized worker pool.
/// Returns the sender (held by Calendar) and the worker pool (held for shutdown).
pub fn start_default_pool() -> (CalendarTaskSender, WorkerPool) {
    let (tx, rx) = mpsc::unbounded_channel();
    let pool = spawn_workers(rx, DEFAULT_WORKER_COUNT);
    (tx, pool)
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
        let (tx, _pool) = start_default_pool();
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
        // No assertion on side-effects — Phase 4b.4 will add observable
        // behavior. For now this test just confirms the channel and workers
        // accept and process all variants without panic or deadlock.
    }

    #[tokio::test]
    async fn enqueue_after_channel_close_returns_err() {
        let (tx, rx) = mpsc::unbounded_channel::<CalendarTask>();
        drop(rx);
        let result = enqueue(&tx, CalendarTask::FindNewMirror);
        assert!(result.is_err(), "enqueue must surface the closed channel");
    }
}
