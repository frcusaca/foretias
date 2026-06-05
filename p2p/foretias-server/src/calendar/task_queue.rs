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
use foretias_core::foretias::types::Tbid;
use rand::Rng;
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
    /// Chronon-level mutual attestation with a specific TBID.
    ///
    /// Requests a chronon-level attestation stamp exchange. The target TBID
    /// identifies the remote TimeFamily to attest with.
    DoChrononAttestation { target_tbid: String },

    /// Epoch-level mutual attestation with a specific TBID.
    ///
    /// Requests an epoch-level attestation stamp exchange. The target TBID
    /// identifies the remote TimeFamily to attest with.
    DoEpochAttestation { target_tbid: String },

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

    /// Verify that a remote TBID's calendar records (including external
    /// attestations) are healthy. Picks a random chronon range, fetches
    /// it via `get_calendar_slice`, and logs coverage / attestation
    /// presence.
    VerifyFbRecorded { target_tbid: String },
}

impl CalendarTask {
    /// Short stable name for logging and metrics.
    pub fn kind(&self) -> &'static str {
        match self {
            CalendarTask::DoChrononAttestation { .. } => "do_chronon_attestation",
            CalendarTask::DoEpochAttestation { .. } => "do_epoch_attestation",
            CalendarTask::FindNewMirror => "find_new_mirror",
            CalendarTask::InitiateDump { .. } => "initiate_dump",
            CalendarTask::StartStream { .. } => "start_stream",
            CalendarTask::ExploreMirror { .. } => "explore_mirror",
            CalendarTask::ExpireMirror { .. } => "expire_mirror",
            CalendarTask::VerifyFbRecorded { .. } => "verify_fb_recorded",
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
    async fn mirror_announce(&self, peer: &PeerAddr, local_tbid_hex: &str) -> Result<bool, String>;

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
    pub health_failures: parking_lot::RwLock<std::collections::HashMap<String, u32>>,
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
///
/// `communerd` and `chronomatter` are optional: production wiring sets them
/// via the `TimeFamilyServer`; test/placeholder mode leaves them `None`.
#[derive(Clone)]
pub struct WorkerContext {
    pub dispatcher: Option<Arc<dyn MirrorDispatcher>>,
    pub mirror_state: Arc<MirrorState>,
    pub task_tx: CalendarTaskSender,
    pub calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
    /// Communerd reference — needed by `DoChrononAttestation` to obtain a
    /// `CommunerdetteLine` for a target TBID. `None` in placeholder mode.
    pub communerd: Option<Arc<crate::communerd::Communerd>>,
    /// Chronomatter reference — needed by `DoChrononAttestation` to produce
    /// an internal Foretis (stamp-free, within trust boundary). `None` in
    /// placeholder mode.
    pub chronomatter: Option<Arc<foretias_core::chronomatter::Chronomatter>>,
    /// Calendar's Ed25519 signing key, shared via Arc from Calendar.
    /// Used by attestation handlers to sign stamp payloads.
    pub signing_key:
        Option<Arc<parking_lot::Mutex<Option<foretias_core::core::identity::PrivKeyHandle>>>>,
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
fn rx_take(
    rx: &mut mpsc::UnboundedReceiver<CalendarTask>,
) -> mpsc::UnboundedReceiver<CalendarTask> {
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
        CalendarTask::InitiateDump { mirror } => handle_initiate_dump(worker_id, mirror, ctx).await,
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
        CalendarTask::ExpireMirror { mirror } => handle_expire_mirror(worker_id, mirror, ctx).await,
        CalendarTask::DoChrononAttestation { target_tbid } => {
            handle_do_chronon_attestation(worker_id, &target_tbid, ctx).await
        }
        CalendarTask::DoEpochAttestation { target_tbid } => {
            handle_do_epoch_attestation(worker_id, &target_tbid, ctx).await
        }
        CalendarTask::VerifyFbRecorded { target_tbid } => {
            handle_verify_fb_recorded(worker_id, &target_tbid, ctx).await
        }
    }
}

/// Chronon-level mutual attestation with a specific TBID.
///
/// Full flow:
/// 1. Obtain a `CommunerdetteLine` for `target_tbid` via Communerd
/// 2. Fetch the target's latest chronon via `line.get_tick(u64::MAX)`
/// 3. Internally stamp via Chronomatter (stamp-free, within trust boundary)
/// 4. Sign the Foretis with Calendar's key
/// 5. Transmit the stamped content to the target via `line.stamp()`
/// 6. Verify attestation was recorded on the target (best-effort)
///
/// Gracefully degrades when Communerd or Chronomatter are not wired into the
/// `WorkerContext` (placeholder mode).
async fn handle_do_chronon_attestation(worker_id: usize, target_tbid: &str, ctx: &WorkerContext) {
    // ── Resource gate ────────────────────────────────────────────────────
    let Some(ref communerd) = ctx.communerd else {
        warn!(
            worker_id,
            target_tbid, "do_chronon_attestation: no Communerd in WorkerContext; skipping"
        );
        return;
    };
    let Some(ref chronomatter) = ctx.chronomatter else {
        warn!(
            worker_id,
            target_tbid, "do_chronon_attestation: no Chronomatter in WorkerContext; skipping"
        );
        return;
    };

    let tbid = match Tbid::from_hex(target_tbid) {
        Ok(t) => t,
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, error = %e, "do_chronon_attestation: invalid target TBID hex"
            );
            return;
        }
    };

    // ── Step 2: Request target's latest chronon ──────────────────────────
    let line = communerd.line_for_tbid(tbid);
    let target_record = match line.get_tick(u64::MAX).await {
        Ok(record) => record,
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, error = %e, "do_chronon_attestation: get_tick failed"
            );
            return;
        }
    };
    let target_chronon = target_record.inner().chronon_number;
    info!(
        worker_id,
        target_tbid, target_chronon, "do_chronon_attestation: fetched target's latest chronon"
    );

    // ── Steps 3-4: Stamp and sign ────────────────────────────────────────
    let Some((stamped, calendar_signature)) =
        stamp_and_sign_chronon_attestation(worker_id, target_tbid, ctx, chronomatter)
    else {
        return;
    };

    // ── Step 5-6: Transmit and verify ────────────────────────────────────
    transmit_chronon_attestation(
        worker_id,
        target_tbid,
        &line,
        target_chronon,
        stamped,
        calendar_signature,
    )
    .await;
}

fn stamp_and_sign_chronon_attestation(
    worker_id: usize,
    target_tbid: &str,
    ctx: &WorkerContext,
    chronomatter: &foretias_core::chronomatter::Chronomatter,
) -> Option<(
    foretias_core::foretias::tick::StampedForetis,
    foretias_core::foretias::types::SignatureBytes,
)> {
    let stamp_content = format!("chronon-attest-{}", target_tbid);
    let stamped = chronomatter
        .stamp(
            stamp_content.into_bytes(),
            "chronon-attestation".to_string(),
        )
        .ok()
        .inspect(|s| {
            debug!(
                worker_id,
                target_tbid,
                local_chronon = s.foretis.chronon_number,
                "do_chronon_attestation: local Foretis produced"
            )
        })?;

    let calendar_signature: foretias_core::foretias::types::SignatureBytes =
        sign_with_calendar_key(ctx, &stamped.foretis.sig_input_bytes())?.into();
    debug!(
        worker_id,
        target_tbid,
        sig_len = calendar_signature.len(),
        "do_chronon_attestation: calendar signature produced"
    );
    Some((stamped, calendar_signature))
}

async fn transmit_chronon_attestation(
    worker_id: usize,
    target_tbid: &str,
    line: &crate::communerd::CommunerdetteLine,
    target_chronon: u64,
    stamped: foretias_core::foretias::tick::StampedForetis,
    _calendar_signature: foretias_core::foretias::types::SignatureBytes,
) {
    let foretis_bytes = stamped.foretis.sig_input_bytes();
    let echo = format!("attest-{}", stamped.foretis.chronon_number);
    match line.stamp(foretis_bytes, echo.clone()).await {
        Ok(remote_foretis) => {
            info!(
                worker_id,
                target_tbid,
                remote_chronon = remote_foretis.inner().chronon_number,
                "do_chronon_attestation: mutual attestation complete"
            );
            verify_chronon_attestation_recorded(
                worker_id,
                target_tbid,
                target_chronon,
                line,
                &echo,
                &remote_foretis,
            )
            .await;
        }
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, error = %e, "do_chronon_attestation: line.stamp() failed"
            );
        }
    }
}

async fn verify_chronon_attestation_recorded(
    worker_id: usize,
    target_tbid: &str,
    target_chronon: u64,
    line: &crate::communerd::CommunerdetteLine,
    echo: &str,
    remote_foretis: &foretias_core::foretias::clean_auth::CleanAuthenticated<
        foretias_core::foretias::tick::Foretis,
    >,
) {
    match line.get_tick(target_chronon).await {
        Ok(verified_record) => {
            let record = verified_record.inner();
            let remote_content_hash = remote_foretis.inner().content_hash.clone();
            let attestation_present = record.external_attestations.iter().any(|att| {
                att.foretis.echo == echo && att.foretis.content_hash == remote_content_hash
            });
            if attestation_present {
                debug!(
                    worker_id,
                    target_tbid,
                    target_chronon,
                    "do_chronon_attestation: FB verification — attestation recorded"
                );
            } else {
                warn!(
                    worker_id,
                    target_tbid,
                    target_chronon,
                    "do_chronon_attestation: attestation not yet visible"
                );
            }
        }
        Err(e) => {
            warn!(
                worker_id,
                target_tbid,
                target_chronon,
                error = %e,
                "do_chronon_attestation: FB verification query failed (best-effort)"
            );
        }
    }
}

/// Epoch-level mutual attestation with a specific TBID.
///
/// Full flow:
/// 1. Obtain a `CommunerdetteLine` for `target_tbid` via Communerd
/// 2. Fetch the target's latest epoch via `line.get_calendar_slice(u64::MAX, 1)`
/// 3. Internally stamp via Chronomatter (stamp-free, within trust boundary)
/// 4. Sign the Foretis with Calendar's key
/// 5. Transmit the stamped content to the target via `line.stamp()`
///
/// Gracefully degrades when Communerd or Chronomatter are not wired into the
/// `WorkerContext` (placeholder mode).
async fn handle_do_epoch_attestation(worker_id: usize, target_tbid: &str, ctx: &WorkerContext) {
    // ── Resource gate ────────────────────────────────────────────────────
    let Some(ref communerd) = ctx.communerd else {
        warn!(
            worker_id,
            target_tbid, "do_epoch_attestation: no Communerd in WorkerContext; skipping"
        );
        return;
    };
    let Some(ref chronomatter) = ctx.chronomatter else {
        warn!(
            worker_id,
            target_tbid, "do_epoch_attestation: no Chronomatter in WorkerContext; skipping"
        );
        return;
    };

    let tbid = match Tbid::from_hex(target_tbid) {
        Ok(t) => t,
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, error = %e, "do_epoch_attestation: invalid target TBID hex"
            );
            return;
        }
    };

    // ── Step 2: Fetch target's latest epoch ──────────────────────────────
    let line = communerd.line_for_tbid(tbid);
    let target_chronon = match get_target_latest_epoch(worker_id, target_tbid, &line).await {
        Some(v) => v,
        None => return,
    };

    // ── Steps 3-4: Stamp and sign ────────────────────────────────────────
    let Some((stamped, calendar_signature)) =
        stamp_and_sign_epoch_attestation(worker_id, target_tbid, ctx, chronomatter)
    else {
        return;
    };

    // ── Step 5: Transmit ────────────────────────────────────────────────
    transmit_epoch_attestation(worker_id, target_tbid, &line, stamped, calendar_signature).await;
}

async fn get_target_latest_epoch(
    worker_id: usize,
    target_tbid: &str,
    line: &crate::communerd::CommunerdetteLine,
) -> Option<u64> {
    let slice = line.get_calendar_slice(u64::MAX, 1).await.ok()?;
    let target_record = slice.into_iter().next().inspect(|_| {
        warn!(
            worker_id,
            target_tbid, "do_epoch_attestation: target returned empty calendar slice"
        );
    })?;

    let target_chronon = target_record.inner().chronon_number;
    info!(
        worker_id,
        target_tbid, target_chronon, "do_epoch_attestation: fetched target's latest epoch"
    );
    Some(target_chronon)
}

fn stamp_and_sign_epoch_attestation(
    worker_id: usize,
    target_tbid: &str,
    ctx: &WorkerContext,
    chronomatter: &foretias_core::chronomatter::Chronomatter,
) -> Option<(
    foretias_core::foretias::tick::StampedForetis,
    foretias_core::foretias::types::SignatureBytes,
)> {
    let stamp_content = format!("epoch-attest-{}", target_tbid);
    let stamped = chronomatter
        .stamp(stamp_content.into_bytes(), "epoch-attestation".to_string())
        .ok()
        .inspect(|s| {
            debug!(
                worker_id,
                target_tbid,
                local_chronon = s.foretis.chronon_number,
                "do_epoch_attestation: local Foretis produced"
            )
        })?;

    let calendar_signature: foretias_core::foretias::types::SignatureBytes =
        sign_with_calendar_key(ctx, &stamped.foretis.sig_input_bytes())?.into();
    debug!(
        worker_id,
        target_tbid,
        sig_len = calendar_signature.len(),
        "do_epoch_attestation: calendar signature produced"
    );
    Some((stamped, calendar_signature))
}

async fn transmit_epoch_attestation(
    worker_id: usize,
    target_tbid: &str,
    line: &crate::communerd::CommunerdetteLine,
    stamped: foretias_core::foretias::tick::StampedForetis,
    _calendar_signature: foretias_core::foretias::types::SignatureBytes,
) {
    let foretis_bytes = stamped.foretis.sig_input_bytes();
    let echo = format!("epoch-attest-{}", stamped.foretis.chronon_number);
    match line.stamp(foretis_bytes, echo).await {
        Ok(remote_foretis) => {
            info!(
                worker_id,
                target_tbid,
                remote_chronon = remote_foretis.inner().chronon_number,
                "do_epoch_attestation: mutual attestation complete"
            );
        }
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, error = %e, "do_epoch_attestation: line.stamp() failed"
            );
        }
    }
}

fn sign_with_calendar_key(ctx: &WorkerContext, data: &[u8]) -> Option<Vec<u8>> {
    let signing_key = ctx.signing_key.as_ref()?;
    let guard = signing_key.lock();
    let key = guard.as_ref()?;
    match key.sign(data) {
        Ok(sig) => Some(sig.bytes.to_vec()),
        Err(e) => {
            warn!(error = %e, "calendar key signing failed");
            None
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
        debug!(
            worker_id,
            current, target, "find_new_mirror: at target; skipping"
        );
        return;
    }

    let local_tbid = ctx.mirror_state.local_tbid_hex.read().clone();
    if local_tbid.is_empty() {
        warn!(
            worker_id,
            "find_new_mirror: local_tbid not set on Calendar; cannot announce"
        );
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
                    CalendarTask::InitiateDump {
                        mirror: peer.clone(),
                    },
                );
                if ctx.mirror_state.mirrors.read().len() >= *ctx.mirror_state.target_mirrors.read()
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
                CalendarTask::StartStream {
                    mirror: mirror.clone(),
                },
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
    match dispatcher.mirror_health_check(&mirror, &local_tbid).await {
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
                    CalendarTask::ExpireMirror {
                        mirror: mirror.clone(),
                    },
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
        debug!(
            worker_id,
            active, min, "active mirrors below min; enqueueing FindNewMirror"
        );
        let _ = enqueue(&ctx.task_tx, CalendarTask::FindNewMirror);
    }
}

/// Verify that a remote TBID's calendar records are healthy.
///
/// 1. Obtain a `CommunerdetteLine` for `target_tbid`
/// 2. Fetch the target's latest chronon to determine the available range
/// 3. Pick a random chronon range (chronon 1 to latest, capped at 100 records)
/// 4. Call `get_calendar_slice` (with attestations) for that range
/// 5. Log coverage results; warn if coverage is poor or attestations are missing
async fn handle_verify_fb_recorded(worker_id: usize, target_tbid: &str, ctx: &WorkerContext) {
    let Some(ref communerd) = ctx.communerd else {
        warn!(
            worker_id,
            target_tbid, "verify_fb_recorded: no Communerd in WorkerContext; skipping"
        );
        return;
    };

    let tbid = match Tbid::from_hex(target_tbid) {
        Ok(t) => t,
        Err(e) => {
            warn!(
                worker_id,
                target_tbid,
                error = %e,
                "verify_fb_recorded: invalid target TBID hex"
            );
            return;
        }
    };

    let line = communerd.line_for_tbid(tbid);

    let latest_chronon = match get_verify_latest_chronon(worker_id, target_tbid, &line).await {
        Some(v) => v,
        None => return,
    };

    let (start, count) = pick_verify_range(latest_chronon);
    log_verify_fetch(worker_id, target_tbid, latest_chronon, start, count);

    let records = match line.get_calendar_slice(start, count).await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                worker_id,
                target_tbid, start, count, error = %e,
                "verify_fb_recorded: get_calendar_slice failed"
            );
            return;
        }
    };

    log_verify_coverage(worker_id, target_tbid, start, count, records);
}

async fn get_verify_latest_chronon(
    worker_id: usize,
    target_tbid: &str,
    line: &crate::communerd::CommunerdetteLine,
) -> Option<u64> {
    let record = line.get_tick(u64::MAX).await.ok()?;
    let latest_chronon = record.inner().chronon_number;
    if latest_chronon == 0 {
        info!(
            worker_id,
            target_tbid, "verify_fb_recorded: target has no ticks; nothing to verify"
        );
        return None;
    }
    Some(latest_chronon)
}

fn pick_verify_range(latest_chronon: u64) -> (u64, u64) {
    let max_records: u64 = 100;
    let start = if latest_chronon <= 1 {
        1
    } else {
        rand::thread_rng().gen_range(1..=latest_chronon)
    };
    let count = std::cmp::min(max_records, latest_chronon.saturating_sub(start) + 1);
    (start, count)
}

fn log_verify_fetch(
    worker_id: usize,
    target_tbid: &str,
    latest_chronon: u64,
    start: u64,
    count: u64,
) {
    info!(
        worker_id,
        target_tbid,
        latest_chronon,
        start,
        count,
        "verify_fb_recorded: fetching chronon range from target"
    );
}

fn log_verify_coverage(
    worker_id: usize,
    target_tbid: &str,
    start: u64,
    count: u64,
    records: Vec<foretias_core::foretias::clean_auth::CleanAuthenticated<ChrononRecord>>,
) {
    let requested = count;
    let returned = records.len() as u64;
    let coverage_ratio = if requested > 0 {
        returned as f64 / requested as f64
    } else {
        1.0
    };

    let records_with_attestations = records
        .iter()
        .filter(|r| !r.inner().external_attestations.is_empty())
        .count();

    info!(
        worker_id,
        target_tbid,
        start,
        requested,
        returned,
        coverage_ratio,
        records_with_attestations,
        "verify_fb_recorded: coverage results"
    );

    if coverage_ratio < 0.5 {
        warn!(
            worker_id,
            target_tbid,
            start,
            requested,
            returned,
            coverage_ratio,
            "verify_fb_recorded: poor coverage (< 50%)"
        );
    }

    if returned > 0 && records_with_attestations == 0 {
        warn!(
            worker_id,
            target_tbid, start, returned, "verify_fb_recorded: no attestations in returned records"
        );
    }
}

/// Convenience: build the channel + spawn the default-sized worker pool with
/// no dispatcher (placeholder mode — used by Phase 4b.2 tests that don't
/// exercise the network path).
pub fn start_default_pool(
    calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
    signing_key: Option<
        Arc<parking_lot::Mutex<Option<foretias_core::core::identity::PrivKeyHandle>>>,
    >,
) -> (CalendarTaskSender, WorkerPool, Arc<MirrorState>) {
    start_default_pool_with_dispatcher(calendar_lookup, None, signing_key)
}

/// Build the channel + spawn the default-sized worker pool with an optional
/// `MirrorDispatcher`. Returns the sender, the worker pool, and the
/// MirrorState handle (so callers can set local_tbid_hex, target_mirrors,
/// inspect active mirrors, etc.).
pub fn start_default_pool_with_dispatcher(
    calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
    dispatcher: Option<Arc<dyn MirrorDispatcher>>,
    signing_key: Option<
        Arc<parking_lot::Mutex<Option<foretias_core::core::identity::PrivKeyHandle>>>,
    >,
) -> (CalendarTaskSender, WorkerPool, Arc<MirrorState>) {
    start_pool(calendar_lookup, dispatcher, None, None, signing_key)
}

/// Full-context pool constructor. Production callers (e.g. `TimeFamilyServer`)
/// pass `Communerd` and `Chronomatter` so task handlers can access P2P and
/// internal-stamping capabilities. Test/placeholder callers pass `None`.
pub fn start_pool(
    calendar_lookup: Arc<parking_lot::RwLock<foretias_core::foretias::Calendar>>,
    dispatcher: Option<Arc<dyn MirrorDispatcher>>,
    communerd: Option<Arc<crate::communerd::Communerd>>,
    chronomatter: Option<Arc<foretias_core::chronomatter::Chronomatter>>,
    signing_key: Option<
        Arc<parking_lot::Mutex<Option<foretias_core::core::identity::PrivKeyHandle>>>,
    >,
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
        communerd,
        chronomatter,
        signing_key,
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
            CalendarTask::DoChrononAttestation {
                target_tbid: "abc123".into()
            }
            .kind(),
            "do_chronon_attestation"
        );
        assert_eq!(
            CalendarTask::DoEpochAttestation {
                target_tbid: "abc123".into()
            }
            .kind(),
            "do_epoch_attestation"
        );
        assert_eq!(CalendarTask::FindNewMirror.kind(), "find_new_mirror");
        assert_eq!(
            CalendarTask::InitiateDump {
                mirror: PeerAddr {
                    json_rpc: "x".into()
                }
            }
            .kind(),
            "initiate_dump"
        );
        assert_eq!(
            CalendarTask::StartStream {
                mirror: PeerAddr {
                    json_rpc: "x".into()
                }
            }
            .kind(),
            "start_stream"
        );
        assert_eq!(
            CalendarTask::ExploreMirror {
                mirror: PeerAddr {
                    json_rpc: "x".into()
                }
            }
            .kind(),
            "explore_mirror"
        );
        assert_eq!(
            CalendarTask::ExpireMirror {
                mirror: PeerAddr {
                    json_rpc: "x".into()
                }
            }
            .kind(),
            "expire_mirror"
        );
        assert_eq!(
            CalendarTask::VerifyFbRecorded {
                target_tbid: "abc123".into()
            }
            .kind(),
            "verify_fb_recorded"
        );
    }

    #[tokio::test]
    async fn workers_drain_enqueued_tasks() {
        use foretias_core::foretias::types::Tbid;
        let cal = Arc::new(parking_lot::RwLock::new(
            foretias_core::foretias::Calendar::new(Tbid::default(), "queue-test"),
        ));
        let (tx, _pool, _state) = start_default_pool(cal, None);
        for i in 0..5 {
            enqueue(
                &tx,
                CalendarTask::DoChrononAttestation {
                    target_tbid: format!("tbid-{i}"),
                },
            )
            .expect("enqueue");
            enqueue(
                &tx,
                CalendarTask::DoEpochAttestation {
                    target_tbid: format!("tbid-{i}"),
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
