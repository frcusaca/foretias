//! Calendar component — task-driven orchestrator for one TimeFamily's chrononchain.
//!
//! # Priority Hierarchy (Invariant — Group 4b)
//!
//! Calendar's responsibilities are explicitly ordered. Resource allocation, task
//! scheduling, and back-pressure decisions must respect this ordering: a lower
//! priority must NEVER cause a higher priority to miss its deadline.
//!
//! | # | Priority    | Responsibility                                                  |
//! |---|-------------|-----------------------------------------------------------------|
//! | 1 | Critical    | Record every tick for the family's chronomatter                 |
//! | 2 | High        | Support local verify requests (look up ticks, validate chains)  |
//! | 3 | Medium-High | Mutual attestation with peers                                   |
//! | 4 | Medium      | Persist family's calendar in the P2P network (find mirrors)     |
//! | 5 | Low         | Mirror other calendars' ticks (starvable)                       |
//!
//! Priorities 4–5 are driven by the task queue (see `calendar::task_queue`) and
//! receive peer-pool change notifications via [`PeerChangeCallback`] from
//! Communerd. Priorities 1–3 are synchronous (Chronomatter ticks the calendar
//! directly) and cannot be starved by mirror/replication work.

use std::sync::Arc;

pub mod mirror;
pub mod task_queue;

use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::tick::CalendarLookup;
use foretias_core::foretias::{Calendar as CoreCalendar, ChrononRecord, types::{TickNumber, Tbid}};
use foretias_core::error::NodeError;
use parking_lot::RwLock;
use tracing::{debug, info};

pub use foretias_core::foretias::callbacks::PeerChangeCallback;
pub use mirror::{MirrorStore, compute_hash_sanity};
pub use task_queue::{
    CalendarTask, CalendarTaskSender, MirrorDispatcher, MirrorState, WorkerPool,
    DEFAULT_WORKER_COUNT, start_pool,
};

pub struct Calendar {
    inner: Arc<RwLock<CoreCalendar>>,
    tbn: String,
    /// Group 4b: low-priority work channel sender. `None` until
    /// `start_task_queue()` is called; once set, callers may enqueue
    /// `CalendarTask`s for the worker pool to process.
    task_tx: std::sync::Mutex<Option<CalendarTaskSender>>,
    /// Owned worker pool handles, kept here so Calendar drives the pool's
    /// lifetime. Phase 4b.6 will wire graceful shutdown through this field.
    worker_pool: std::sync::Mutex<Option<WorkerPool>>,
    /// Mirror-state handle returned by `start_default_pool_with_dispatcher`.
    /// Callers can set the local TBID hex, target/min mirror counts, and
    /// inspect the active mirror set. `None` until the queue is started.
    mirror_state: std::sync::Mutex<Option<Arc<MirrorState>>>,
}

impl Calendar {
    pub fn new(tbid: Tbid, tbn: &str) -> Self {
        info!(component = "calendar", tbid = %tbid.to_hex(), tbn = %tbn, "calendar initialized");
        Self {
            inner: Arc::new(RwLock::new(CoreCalendar::new(tbid, tbn))),
            tbn: tbn.to_string(),
            task_tx: std::sync::Mutex::new(None),
            worker_pool: std::sync::Mutex::new(None),
            mirror_state: std::sync::Mutex::new(None),
        }
    }

    pub fn from_persisted(path: &str) -> Result<Self, NodeError> {
        let cal = CoreCalendar::load(path)?;
        let tbid = cal.tbid();
        let tbn = cal.tbn.clone();
        info!(component = "calendar", tbid = %tbid.to_hex(), tbn = %tbn, "calendar loaded from persisted: {}", path);
        Ok(Self {
            inner: Arc::new(RwLock::new(cal)),
            tbn,
            task_tx: std::sync::Mutex::new(None),
            worker_pool: std::sync::Mutex::new(None),
            mirror_state: std::sync::Mutex::new(None),
        })
    }

    /// Start the Group 4b task queue in placeholder mode (no MirrorDispatcher).
    /// Useful for tests of the queue plumbing that don't exercise network
    /// behavior. Production callers should use `start_task_queue_with_dispatcher`.
    pub fn start_task_queue(&self) {
        let (tx, pool, state) = task_queue::start_default_pool(self.inner());
        // The mirror state carries the local TBID — populate it from the
        // calendar's current TBID so worker handlers can address themselves.
        let local_tbid = self.tbid().to_hex();
        *state.local_tbid_hex.write() = local_tbid;
        *self.task_tx.lock().unwrap() = Some(tx);
        *self.worker_pool.lock().unwrap() = Some(pool);
        *self.mirror_state.lock().unwrap() = Some(state);
        debug!(
            component = "calendar",
            worker_count = task_queue::DEFAULT_WORKER_COUNT,
            "calendar task queue started (placeholder mode)"
        );
    }

    /// Start the Group 4b task queue with a `MirrorDispatcher`. This is the
    /// production wiring: TimeFamilyServer passes Communerd as the
    /// dispatcher so worker handlers can make real RPC calls. Returns the
    /// `MirrorState` Arc so the caller can adjust target/min mirror counts
    /// or read the active mirror set for diagnostics.
    pub fn start_task_queue_with_dispatcher(
        &self,
        dispatcher: Arc<dyn MirrorDispatcher>,
    ) -> Arc<MirrorState> {
        let (tx, pool, state) =
            task_queue::start_default_pool_with_dispatcher(self.inner(), Some(dispatcher));
        let local_tbid = self.tbid().to_hex();
        *state.local_tbid_hex.write() = local_tbid;
        *self.task_tx.lock().unwrap() = Some(tx);
        *self.worker_pool.lock().unwrap() = Some(pool);
        let returned = Arc::clone(&state);
        *self.mirror_state.lock().unwrap() = Some(state);
        info!(
            component = "calendar",
            worker_count = task_queue::DEFAULT_WORKER_COUNT,
            "calendar task queue started with mirror dispatcher"
        );
        returned
    }

    /// Read-only access to the MirrorState (if the queue has been started).
    pub fn mirror_state(&self) -> Option<Arc<MirrorState>> {
        self.mirror_state.lock().unwrap().clone()
    }

    /// Enqueue a calendar task. Returns `Err` if the task queue hasn't been
    /// started (callers should call `start_task_queue` once at construction
    /// time) or if the channel was closed.
    pub fn enqueue_task(&self, task: CalendarTask) -> Result<(), CalendarTask> {
        let guard = self.task_tx.lock().unwrap();
        match guard.as_ref() {
            Some(tx) => task_queue::enqueue(tx, task),
            None => {
                debug!(
                    component = "calendar",
                    task_kind = task.kind(),
                    "task queue not started; dropping enqueue"
                );
                Err(task)
            }
        }
    }

    /// True if the task queue has been started.
    pub fn task_queue_started(&self) -> bool {
        self.task_tx.lock().unwrap().is_some()
    }

    pub fn inner(&self) -> Arc<RwLock<CoreCalendar>> {
        self.inner.clone()
    }

    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let cal = self.inner.read();
        let tick_count = cal.ticks.len();
        cal.save(path)?;
        info!(component = "calendar", tbid = %cal.tbid().to_hex(), tick_count, "calendar saved to: {}", path);
        Ok(())
    }

    /// Save the calendar wrapped in `PersistedCalendar` format (with metadata header).
    /// This is an additional save format alongside the raw JSON `save()` method.
    pub fn save_as_persisted(&self, path: &str) -> Result<(), NodeError> {
        use foretias_core::config::{PersistedCalendar, CalendarMetadata, CalendarConfig};

        let cal = self.inner.read();
        let persisted = PersistedCalendar {
            config: CalendarMetadata {
                tbid: cal.tbid().to_hex(),
                tbn: cal.tbn().to_string(),
                stamp_tbid: cal.tbid().to_hex(),
                persisted_by: env!("CARGO_PKG_VERSION").to_string(),
                calendar_config: CalendarConfig::default(),
            },
            ticks: cal.ticks.iter()
                .map(|t| serde_json::to_value(t).unwrap_or_default())
                .collect(),
        };
        let json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| NodeError::Internal(format!("failed to serialize persisted calendar: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| NodeError::Internal(format!("failed to write persisted calendar: {}", e)))?;
        Ok(())
    }
}

impl TickObserver for Calendar {
    fn on_tick_advance(&self, chronon_number: TickNumber, _public_key: &[u8], tick_record: &ChrononRecord) {
        let mut cal = self.inner.write();
        if let Err(e) = cal.append(tick_record.clone()) {
            tracing::error!(component = "calendar", tbid = %cal.tbid().to_hex(), tick = chronon_number.0, "calendar: on_tick_advance failed: {}", e);
        } else {
            debug!(component = "calendar", tbid = %cal.tbid().to_hex(), tick = chronon_number.0, tick_count = cal.ticks.len(), "calendar: heartbeat");
        }
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, chronon_number: u64, count: usize) -> Result<Vec<ChrononRecord>, NodeError> {
        self.inner.read().get(chronon_number, count)
    }

    fn latest(&self) -> Option<u64> {
        self.inner.read().latest()
    }

    fn tbid(&self) -> Tbid {
        self.inner.read().tbid()
    }

    fn tbn(&self) -> &str {
        &self.tbn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick(chronon_number: u64) -> ChrononRecord {
        ChrononRecord {
            chronon_number,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        }
    }

    #[test]
    fn tick_observer_appends_record() {
        let cal = Calendar::new(Tbid::from_raw([0x01; 96]), "observer-test");
        let tick = make_tick(1);

        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &tick);

        assert_eq!(cal.latest(), Some(1));
        let records = cal.get(1, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].chronon_number, 1);
    }

    #[test]
    fn tick_observer_rejects_duplicate() {
        let cal = Calendar::new(Tbid::from_raw([0x02; 96]), "dup-test");
        let tick = make_tick(5);

        cal.on_tick_advance(TickNumber(5), &[0u8; 32], &tick);
        cal.on_tick_advance(TickNumber(5), &[0u8; 32], &tick);

        assert_eq!(cal.latest(), Some(5));
        let records = cal.get(5, 10).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn calendar_lookup_get_returns_ticks() {
        let cal = Calendar::new(Tbid::from_raw([0x03; 96]), "lookup-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(TickNumber(3), &[0u8; 32], &make_tick(3));

        let records = cal.get(2, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].chronon_number, 2);
        assert_eq!(records[1].chronon_number, 3);
    }

    #[test]
    fn calendar_lookup_get_respects_count() {
        let cal = Calendar::new(Tbid::from_raw([0x04; 96]), "count-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(TickNumber(3), &[0u8; 32], &make_tick(3));

        let records = cal.get(1, 2).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn calendar_lookup_latest_on_empty() {
        let cal = Calendar::new(Tbid::from_raw([0x05; 96]), "empty-test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn calendar_lookup_tbid() {
        let tbid = Tbid::from_raw([0xAB; 96]);
        let cal = Calendar::new(tbid, "tbid-test");
        assert_eq!(cal.tbid(), tbid);
    }

    #[test]
    fn calendar_lookup_tbn() {
        let cal = Calendar::new(Tbid::from_raw([0x06; 96]), "my-name");
        assert_eq!(cal.tbn(), "my-name");
    }

    #[test]
    fn calendar_save_and_load() {
        let cal = Calendar::new(Tbid::from_raw([0x07; 96]), "persist-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));

        let path = "/tmp/foretias-test-calendar.json";
        cal.save(path).unwrap();

        let loaded = Calendar::from_persisted(path).unwrap();
        assert_eq!(loaded.latest(), Some(2));
        assert_eq!(loaded.tbn(), "persist-test");
        let records = loaded.get(1, 10).unwrap();
        assert_eq!(records.len(), 2);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn inner_returns_arc() {
        let cal = Calendar::new(Tbid::from_raw([0x08; 96]), "arc-test");
        let inner = cal.inner();
        assert_eq!(inner.read().ticks.len(), 0);
    }

    // ── Group 4b: task queue scaffolding ────────────────────────────────────

    #[test]
    fn task_queue_starts_in_inactive_state() {
        let cal = Calendar::new(Tbid::from_raw([0x09; 96]), "queue-test");
        assert!(!cal.task_queue_started());
    }

    #[test]
    fn enqueue_before_start_returns_err() {
        let cal = Calendar::new(Tbid::from_raw([0x0A; 96]), "queue-test");
        let result = cal.enqueue_task(CalendarTask::FindNewMirror);
        assert!(
            result.is_err(),
            "enqueue must fail before start_task_queue is called"
        );
    }

    #[tokio::test]
    async fn task_queue_accepts_all_variants_after_start() {
        use foretias_core::foretias::callbacks::PeerAddr;
        let cal = Calendar::new(Tbid::from_raw([0x0B; 96]), "queue-test");
        cal.start_task_queue();
        assert!(cal.task_queue_started());

        let peer = PeerAddr {
            json_rpc: "127.0.0.1:6001".to_string(),
        };
        let variants = vec![
            CalendarTask::DoChrononAttestation { target_tbid: "tbid-abc".into() },
            CalendarTask::DoEpochAttestation { target_tbid: "tbid-abc".into() },
            CalendarTask::FindNewMirror,
            CalendarTask::InitiateDump { mirror: peer.clone() },
            CalendarTask::StartStream { mirror: peer.clone() },
            CalendarTask::ExploreMirror { mirror: peer.clone() },
            CalendarTask::ExpireMirror { mirror: peer.clone() },
        ];
        for task in variants {
            cal.enqueue_task(task).expect("enqueue after start succeeds");
        }
        // Give the worker pool a tick to drain placeholder dispatches.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
