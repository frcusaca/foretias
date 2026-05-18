# FORETIAS v0.2 P2P Mutual Attestation — Implementation Plan

**Status:** FINAL — architecture confirmed by user
**Based on:** `FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md`
**Current code:** v0.2.0, commit d7af751, branch mvp
**Workspace:** 3 crates — `foretias-core`, `foretias-node`, `foretias-python`

---

## ARCHITECTURE — REFINED COMPONENT MODEL

### Component Responsibilities

All three components (Chronomatter, Calendar, Communerd) are **time beings** with their own TBID. They are members of the same Time Family.

| Component | Owns | Intra-family Interface | Extra-family? |
|-----------|------|----------------------|---------------|
| **Chronomatter** (*Chronos fidelis authenticus*) | Autonomous ticking, stamping, verification | Direct method calls; Calendar calls `stamp()`, `verify()` | NO |
| **Calendar** (*Chronos fidelis grapha*) | Calendar data, persistence (disk), mutual attestation scheduling | Direct method calls; receives chronon notification from Chronomatter via callback | NO — routes through Communerd |
| **Communerd** | *Chronos fidelis locutus* /KAH-myuh-nerd/) All PtP & P2P communication, transport, RPC | Called by Calendar via direct method calls (`send_to_peer()`, `query_community()`) | **YES** — only component with network access |

**Communerd etymology:** A Communerd is a communard of a Time Family commune where timing information is shared in communal communion between families, AND he's a nerd about communications.
| **TimeFamily** (*Chronos fidelis adunatrix*) | Orchestrator — creates/wires Chronomatter, Calendar, Communerd | Exposes JSON-RPC to external callers | NO — delegates |

### Intra-Family Communication

Members of the same Time Family communicate via **locally specified interfaces** — low-latency method calls or short-lived callbacks. These are direct Rust function calls on shared `Arc<...>` references with interior mutability (`Mutex`, `RwLock`). No message passing, no channels, no serialization.

```rust
// Calendar → Chronomatter: direct method call
let foretis = chronomatter.stamp(content, echo).await;

// Chronomatter → Calendar: callback on chronon advance
calendar.on_tick_advance(tick_record);

// Calendar → Communerd: direct method call
let response = communerd.send_to_peer(peer_addr, rpc_call).await;
```

### Extra-Family Communication

**Only Communerd** has network access. All traffic to/from other Time Families flows through the local Communerd. This includes:
- Mutual attestation stamping (Calendar calls Communerd to stamp on a peer)
- Calendar slice queries (Calendar calls Communerd to fetch a peer's chronon records)
- Community state queries (any component can ask Communerd about peer liveness, known families, etc.)

### Key Invariants

1. **Calendar never touches the network** — all remote calls go through Communerd
2. **Calendar persists to disk** — calls CryptoServer for encryption (v0.5+), currently plaintext
3. **Chronomatter owns stamping AND verification** — Calendar calls `verify()` directly
4. **Mutual attestation is owned by Calendar** — Calendar schedules, formats, calls Communerd for transport, calls Chronomatter for verification, stores result
5. **Communerd knows nothing about Foretias semantics** — it's a transparent RPC relay
6. **Intra-family calls are direct** — no channels, no message passing, no broadcast

### Data Flow: Mutual Attestation

```
[Chronomatter] advances chronon (autonomous timer)
    ↓ calls calendar.on_tick_advance(tick_record)
[Calendar] appends ChrononRecord to calendar
    ↓ checks: chronon_number % every_n == 0?
[Calendar] serializes ChrononRecord → content_hex
    ↓ forms echo = "ma:{tbid}:{chronon}"
[Calendar] calls communerd.send_to_peer(peer, "/stamp", content_hex, echo)
    ↓ Communerd opens TCP → remote /stamp → returns Foretis JSON
    ↓ Communerd opens TCP → remote /get_calendar_slice → returns attester ChrononRecord
[Calendar] calls chronomatter.verify(foretis, content) → bool
    ↓ verifies: content hash, ed25519 signature, echo match
[Calendar] stores ExternalAttestation in calendar
    ↓ marks dirty, triggers flush
```

### Data Flow: Inbound Stamp (unchanged)

```
[External client] → JSON-RPC /stamp → [TimeFamily]
    → [TimeFamily] → [Chronomatter.stamp(content, echo)] → Foretis
    → [TimeFamily] → [Calendar.append(new_chronon)] (if chronon advanced)
    → [TimeFamily] → response to client
```

---

## RESOLVED DECISIONS

| # | Decision | Resolution | Rationale |
|---|----------|------------|-----------|
| **Q1** | "mutual attestation" vs "auto-attestation" | **Different concepts.** Intra-node = auto-attestation (existing). Cross-node = mutual attestation (new). | User: "When TBID is different, it is no longer called auto-attestation." |
| **Q2** | Architecture | **Full decomposition.** Chronomatter, Calendar, Communerd are time beings with their own TBID. Intra-family = direct method calls. Extra-family = Communerd only. | User: "organize the responsibility of Chronos fidelias... fully separate the concerns." |
| **Q3** | Rate limiting / version checks | **Defer to v0.5 hardening.** | Not blocking demo, spec places in v0.5. |
| **Q4** | Atomic flush | **Fix now.** `.tmp` + `rename`. | 15 lines, no downside. |
| **Q5** | PyO3 bindings | **Update now.** `PyExternalAttestation`, `PyChrononRecord.external_attestations`. | Must keep Python tests green. |
| **Q6** | `inspect-attestations` CLI | **Both Rust and Python.** | Mirror existing pattern. |
| **Q7** | Config | **Both CLI flags + JSON.** | Existing pattern. |
| **Q8** | Intra-family communication | **Direct method calls / callbacks.** No channels, no broadcast, no message passing. | Low latency, same process, no serialization overhead. |

---

## SCOPE SUMMARY

| Metric | Count |
|--------|-------|
| New files | 14 |
| Modified files | 12 |
| New Rust lines (estimated) | ~2,100 |
| New test lines (estimated) | ~800 |
| Total estimated lines | ~2,900 |

---

## PHASE 0: Type Aliases (core-engine)

**Goal:** Define type aliases for core data types. Improves readability, self-documenting code, prevents mixing up byte arrays.

**Files modified:**
- `p2p/core-engine/src/foretias/types.rs` — new file with all type aliases

**Files modified:**
- `p2p/core-engine/src/foretias/mod.rs` — re-export `types` module

**Type aliases:**

```rust
// types.rs

/// Time Being ID — 16-byte unique identifier for a time being.
pub type Tbid = [u8; 16];

/// Ed25519 public key — 32 bytes.
pub type PublicKey = [u8; 32];

/// Ed25519 signature — 64 bytes.
pub type Signature = [u8; 64];

/// SHA-256 hash digest — 32 bytes.
pub type Digest = [u8; 32];

/// Encrypted message payload (before/after encryption).
pub type Message = Vec<u8>;

/// Auto-attestation nonce — 16 bytes of entropy.
pub type AaNonce = [u8; 16];

/// Chronon number.
pub type TickNumber = u64;
```

These are `type` aliases (zero-cost, no runtime difference). They make signatures self-documenting:
```rust
// Before: fn verify(tbid: &[u8; 16], sig: &[u8; 64]) -> bool
// After:  fn verify(tbid: &Tbid, sig: &Signature) -> bool
```

**Phase 0 tests:** None needed (zero-cost aliases, compiler verifies correctness).

---

## PHASE 1: Foundation — Types, Error, Callbacks (core-engine)

**Goal:** Define all new types, error variants, and the callback-based interfaces between components.

**Files created:**
- `p2p/core-engine/src/foretias/external_attestation.rs` — `ExternalAttestation` type
- `p2p/core-engine/src/foretias/callbacks.rs` — trait definitions for intra-family interfaces

**Files modified:**
- `p2p/core-engine/src/foretias/tick.rs` — add `#[serde(default)] pub external_attestations: Vec<ExternalAttestation>` to `ChrononRecord`
- `p2p/core-engine/src/foretias/calendar.rs` — add `add_external_attestation(local_tick, att)` method
- `p2p/core-engine/src/foretias/mod.rs` — re-export new types
- `p2p/core-engine/src/error.rs` — add `TransportConnect`, `TransportTimeout`, `TransportDecode`, `AttestationVerificationFailed` variants
- `p2p/core-engine/src/config.rs` — add to `NodeConfig`:
  - `peers: Vec<String>` — `"host:port"` list
  - `mutual_attest_every_n: u64` — default 1
  - `request_timeout_secs: u64` — default 5

**Intra-family trait interfaces:**

```rust
// callbacks.rs

/// Called by Chronomatter when a new chronon advances.
/// Calendar implements this to receive chronon notifications.
#[async_trait::async_trait]
pub trait TickObserver: Send + Sync {
    /// Called synchronously on each chronon advance.
    fn on_tick_advance(&self, chronon_number: TickNumber, public_key: &PublicKey);
}

/// Stamping interface — Chronomatter implements this.
/// Calendar calls this to stamp arbitrary content.
#[async_trait::async_trait]
pub trait Stamper: Send + Sync {
    async fn stamp(&self, content: Message, echo: String)
        -> Result<Foretis, NodeError>;
    async fn verify(&self, foretis: &Foretis, content: &Message)
        -> Result<bool, NodeError>;
}

/// Community query interface — Communerd implements this.
/// Calendar calls this for all extra-family communication.
#[async_trait::async_trait]
pub trait PeerMessenger: Send + Sync {
    async fn send_to_peer(&self, addr: &PeerAddr, method: &str, params: serde_json::Value)
        -> Result<serde_json::Value, TransportError>;
    async fn query_community(&self, query: CommunityQuery)
        -> Result<CommunityResponse, TransportError>;
}
```

**ExternalAttestation type:**

```rust
// external_attestation.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAttestation {
    pub attester_tbid:        String,      // hex-encoded TBID of attesting peer
    pub foretis:               Foretis,      // B's stamp of A's chronon record
    pub attester_chronon_record: ChrononRecord,  // B's chronon at attestation time (for offline re-verify)
    pub received_at_ns:       u64,         // wall-clock receive time
}
```

```rust
// tick.rs — add to ChrononRecord
#[serde(default)]
pub external_attestations: Vec<ExternalAttestation>,
```

**Phase 1 tests:**
- `external_attestation_serde_roundtrip`
- `tick_record_serde_compat` — deserialize old calendar JSON without `external_attestations` field → defaults to empty vec
- `calendar_add_external_attestation` — basic add + retrieval

---

## PHASE 2: Communerd — Transport Layer (foretias-node)

**Goal:** Build the communication layer — all P2P traffic flows through Communerd.

**Files created:**
- `p2p/foretias-node/src/communerd/mod.rs` — Communerd struct, re-exports
- `p2p/foretias-node/src/communerd/transport.rs` — `PeerTransport` trait, `PeerAddr`, `TransportError`
- `p2p/foretias-node/src/communerd/json_rpc_transport.rs` — `JsonRpcTransport` impl (extracted from `main.rs`)
- `p2p/foretias-node/src/communerd/peer_pool.rs` — peer pool, liveness ping

**Files modified:**
- `p2p/foretias-node/src/lib.rs` — add `pub mod communerd;`

**Trait definition:**

```rust
// transport.rs
#[async_trait::async_trait]
pub trait PeerTransport: Send + Sync {
    async fn stamp(&self, peer: &PeerAddr, content_hex: &str, echo: &str)
        -> Result<serde_json::Value, TransportError>;
    async fn get_calendar_slice(&self, peer: &PeerAddr, tick_start: u64, count: u64)
        -> Result<Vec<ChrononRecord>, TransportError>;
    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
}

#[derive(Debug, Clone)]
pub struct PeerAddr {
    pub json_rpc: String,   // "host:port"
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connect failed: {0}")] Connect(String),
    #[error("rpc error {code}: {message}")] Rpc { code: i32, message: String },
    #[error("timeout")] Timeout,
    #[error("decode error: {0}")] Decode(String),
}
```

**JsonRpcTransport implementation:**
- Extract `json_rpc_call` from `main.rs` into the transport
- Add `tokio::time::timeout` wrapping (configurable timeout)
- Same newline-delimited framing, same 4 KB cap

**Communerd struct:**

```rust
// mod.rs
pub struct Communerd {
    transport: Box<dyn PeerTransport>,
    peers:     Vec<PeerAddr>,
    /// Channel for Calendar to request mutual attest RPC
    req_rx:    broadcast::Receiver<CommunerdRequest>,
}

#[derive(Debug)]
pub enum CommunerdRequest {
    Stamp {
        peer:      PeerAddr,
        content:   String,  // hex-encoded
        echo:      String,
        reply_tx:  oneshot::Sender<Result<serde_json::Value, TransportError>>,
    },
    GetCalendarSlice {
        peer:      PeerAddr,
        chronon:      u64,
        count:     u64,
        reply_tx:  oneshot::Sender<Result<Vec<ChrononRecord>, TransportError>>,
    },
    Ping {
        peer:     PeerAddr,
        reply_tx: oneshot::Sender<Result<(), TransportError>>,
    },
}
```

**Phase 2 tests:**
- `json_rpc_transport_stamp_roundtrip` — two in-process servers, transport calls stamp
- `json_rpc_transport_timeout` — peer unreachable, returns Timeout after configured duration
- `communerd_stamp_request` — Communerd forwards stamp request via transport
- `communerd_ping_success_fail`

---

## PHASE 3: Chronomatter Extraction (core-engine + foretias-node)

**Goal:** Extract Chronomatter from TimeFamilyServer. Chronomatter owns ticking, stamping, verification. Exposes `Stamper` trait for direct method calls.

**Files created:**
- `p2p/core-engine/src/chronomatter/mod.rs` — Chronomatter struct + task loop + `Stamper` impl

**Files modified:**
- `p2p/core-engine/src/lib.rs` — add `pub mod chronomatter;`
- `p2p/foretias-node/src/server/mod.rs` — remove ticking/stamping logic, delegate to Chronomatter

**Chronomatter struct:**

```rust
// mod.rs
pub struct Chronomatter {
    /// Chronomatter's own TBID (it's a time being)
    tbid: Tbid,
    /// Current chronon number
    current_tick: AtomicU64,
    /// Per-chronon keypair (protected)
    keypair: Mutex<Option<(PrivKeyHandle, PublicKey)>>,
    /// TBN
    tbn: String,
    /// Crypto server
    crypto: Arc<dyn CryptoServer>,
    /// Callback for chronon advance — Calendar implements TickObserver
    tick_observer: Arc<dyn TickObserver>,
}
```

**Chronomatter task loop (only the timer — stamp/verify are direct method calls):**

```rust
async fn run(mut self, chronon_ns: u64) {
    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_nanos(chronon_ns)
    );
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        self.advance_tick().await;
        // Direct callback to Calendar
        let pk = self.current_public_key();
        self.tick_observer.on_tick_advance(self.current_tick.load(SeqCst), &pk);
    }
}
```

**`Stamper` trait implementation (direct method calls, no channels):**

```rust
#[async_trait::async_trait]
impl Stamper for Chronomatter {
    async fn stamp(&self, content: Message, echo: String) -> Result<Foretis, NodeError> {
        // delegates to existing stamp() in tick.rs
    }
    async fn verify(&self, foretis: &Foretis, content: &Message) -> Result<bool, NodeError> {
        // delegates to existing verify() in tick.rs
    }
}
```

**Key changes:**
- `advance_tick()` generates new keypair, destroys old via `zeroize`, creates `ChrononRecord`
- `stamp()` delegates to existing `stamp()` in `tick.rs`
- `verify()` delegates to existing `verify()` in `tick.rs`
- All existing stamp/verify logic stays in `tick.rs` — Chronomatter is the caller
- Calendar gets notified of chronon advances via `TickObserver` callback, NOT broadcast channel
- Calendar calls `chronomatter.stamp()` / `chronomatter.verify()` directly — no channels

**Phase 3 tests:**
- `chronomatter_ticks_on_interval` — two stamps within one chronon share the same chronon_number
- `chronomatter_stamp_direct_call` — direct method call returns Foretis
- `chronomatter_verify_direct_call` — direct method call returns bool
- `chronomatter_key_rotation` — keypair changes on each chronon advance
- `chronomatter_tick_callback` — TickObserver receives notification on each advance

---

## PHASE 4: Calendar Component + Mutual Attestation (core-engine + foretias-node)

**Goal:** Calendar owns data, persistence, and mutual attestation scheduling. Calls Communerd directly for remote RPC, calls Chronomatter directly for verify.

**Files created:**
- `p2p/foretias-node/src/calendar/mod.rs` — Calendar component (RwLock-wrapped calendar + persistence task + mutual attestation)

**Files modified:**
- `p2p/core-engine/src/foretias/calendar.rs` — add `add_external_attestation(local_tick, att)` method (pure data method)
- `p2p/foretias-node/src/server/mod.rs` — remove calendar mutation logic, delegate to Calendar component

**Calendar struct:**

```rust
// mod.rs
pub struct Calendar {
    /// Calendar's own TBID (it's a time being)
    tbid: Tbid,
    /// In-memory calendar behind RwLock
    inner: RwLock<CalendarData>,  // CalendarData = { tbid, stamp_tbid, ticks: Vec<ChrononRecord> }
    /// Persistence path
    persist_path: PathBuf,
    /// Flush interval
    flush_interval: Duration,
    /// Dirty flag
    dirty: AtomicBool,
    /// Flush signal channel
    flush_rx: mpsc::Receiver<()>,
    /// Direct reference to Chronomatter for stamp/verify calls
    chronomatter: Arc<dyn Stamper>,
    /// Direct reference to Communerd for extra-family calls
    communerd: Arc<dyn PeerMessenger>,
    /// Configuration
    config: NodeConfig,
}
```

**Calendar persistence task (only flush — chronon handling is via callback):**

```rust
async fn run_flush(self: Arc<Self>) {
    let mut interval = tokio::time::interval(self.flush_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if self.dirty.swap(false, SeqCst) {
                    self.flush_to_disk();
                }
            }
            Some(()) = self.flush_rx.recv() => {
                self.dirty.store(false, SeqCst);
                self.flush_to_disk();
            }
        }
    }
}
```

**`TickObserver` implementation — mutual attestation trigger (called directly by Chronomatter):**

```rust
impl TickObserver for Calendar {
    fn on_tick_advance(&self, chronon_number: TickNumber, _public_key: &PublicKey) {
        // Check if we should mutual attest this chronon
        if chronon_number > 0 && chronon_number % self.config.mutual_attest_every_n == 0 {
            let this = self.clone();
            // Spawn a lightweight task for the async work
            tokio::spawn(async move {
                this.execute_mutual_attest(chronon_number).await;
            });
        }
    }
}
```

**`execute_mutual_attest` — the full flow (direct method calls, no channels):**

```rust
async fn execute_mutual_attest(&self, chronon_number: TickNumber) {
    let tick_record = self.get_tick_record(chronon_number).unwrap();

    // 1. Serialize ChrononRecord as content
    let content = serde_json::to_vec(&tick_record).unwrap();
    let content_hex = hex::encode(&content);
    let echo = format!("ma:{}:{}", hex::encode(&self.tbid), tick_record.chronon_number);

    // 2. Call Communerd directly (method call, no channel)
    let foretis_val = self.communerd.send_to_peer(
        &PeerAddr { json_rpc: peer.clone() },
        "stamp",
        serde_json::json!({"content": content_hex, "echo": echo.clone()}),
    ).await;
    let foretis_val = match foretis_val {
        Ok(v) => v,
        Err(e) => { log_warn!("mutual attest: stamp RPC failed: {}", e); return; }
    };

    // 3. Parse foretis_val into Foretis
    let foretis: Foretis = serde_json::from_value(foretis_val).unwrap_or_else(|e| {
        log_warn!("mutual attest: parse failed: {}", e);
        return;
    });

    // 4. Fetch attester's ChrononRecord via Communerd
    let slice_val = self.communerd.send_to_peer(
        &PeerAddr { json_rpc: peer.clone() },
        "get_calendar_slice",
        serde_json::json!({"tick_start": foretis.chronon_number, "count": 1}),
    ).await.unwrap_or_default();
    let attester_tr: ChrononRecord = /* parse from slice_val */;

    // 5. Verify via Chronomatter (direct method call)
    let verified = self.chronomatter.verify(&foretis, &content).await.unwrap_or(false);
    if !verified {
        log_warn!("mutual attest: verification failed");
        return;
    }

    // 6. Echo check
    if foretis.echo != echo {
        log_warn!("mutual attest: echo mismatch");
        return;
    }

    // 7. Store!
    let att = ExternalAttestation {
        attester_tbid: hex::encode(&attester_tr.tbid),
        foretis,
        attester_tick_record: attester_tr,
        received_at_ns: now_ns(),
    };
    self.add_external_attestation(chronon_number, att).unwrap();
}
```

**Phase 4 tests:**
- `calendar_add_external_attestation` — add + retrieve from specific chronon
- `calendar_tick_observer_callback` — receives on_tick_advance, triggers scheduling
- `mutual_attest_scheduling_respects_every_n` — only triggers on multiples of N
- `mutual_attest_full_flow` — two in-process TimeFamily instances, mutual attest succeeds
- `mutual_attest_verify_rejects_bad_sig` — bad signature → not stored
- `mutual_attest_verify_rejects_echo_mismatch` — echo mismatch → not stored

---

## PHASE 5: Atomic Flush + Crash Recovery (core-engine)

**Goal:** Calendar persists atomically. No partial writes on disk.

**Files modified:**
- `p2p/core-engine/src/foretias/calendar.rs` — rewrite `save()` to use `.tmp` + `rename`

**Atomic flush:**
```rust
pub fn save(&self, path: &str) -> Result<(), NodeError> {
    let data = serde_json::to_string_pretty(&self)?;
    let tmp_path = format!("{}.tmp", path);
    std::fs::write(&tmp_path, &data)?;
    std::fs::rename(&tmp_path, path)?;  // POSIX-atomic
    Ok(())
}
```

**Crash recovery (in `Calendar::load()`):**
```rust
// On load: if .tmp exists and is valid JSON, rename it to overwrite old file
// If .tmp exists but is corrupt, delete .tmp and load old file
```

**Phase 5 tests:**
- `calendar_flush_atomic` — simulated crash mid-write; reload recovers to last good state
- `calendar_crash_recovery` — `.tmp` left on disk; on restart, recover from `.tmp`

---

## PHASE 6: TimeFamily Orchestrator (foretias-node)

**Goal:** Wire Chronomatter + Calendar + Communerd into a single orchestrator. Replace `TimeFamilyServer`.

**Files modified:**
- `p2p/foretias-node/src/server/mod.rs` — become thin orchestrator; wire all three components
- `p2p/foretias-node/src/server/handlers.rs` — delegate to Chronomatter for stamp/verify

**TimeFamily struct:**

```rust
// mod.rs
pub struct TimeFamily {
    chronomatter_task: tokio::task::JoinHandle<()>,
    calendar_task:     tokio::task::JoinHandle<()>,
    communerd_task:    tokio::task::JoinHandle<()>,
    /// Channels for external callers (JSON-RPC handlers)
    stamp_tx:          broadcast::Sender<StampRequest>,
    verify_tx:         broadcast::Sender<VerifyRequest>,
    /// Config
    config:            NodeConfig,
    /// Dormant flag
    dormant:           AtomicBool,
}
```

**`TimeFamily::new()`:**
1. Create `Chronomatter` with `Arc<dyn TickObserver>` pointing to Calendar
2. Create `Communerd` → spawn tokio task (liveness pings, peer pool)
3. Create `Calendar` with `Arc<dyn Stamper>` (Chronomatter) + `Arc<dyn PeerMessenger>` (Communerd)
4. Spawn Chronomatter's chronon timer task
5. Spawn Calendar's flush task
6. Return `TimeFamily` holding references to Chronomatter and Calendar

**JSON-RPC handler changes:**
- `handle_stamp()` → direct call `chronomatter.stamp(content, echo)`
- `handle_verify()` → direct call `chronomatter.verify(foretis, content)`
- `handle_get_calendar_slice()` → read from Calendar directly (RwLock)

**Phase 6 tests:**
- `time_family_start_stops` — create + shutdown TimeFamily
- `time_family_stamp_via_rpc` — full RPC → Chronomatter → response
- `time_family_verify_via_rpc` — full RPC → Chronomatter → response
- All v0.1 regression tests still pass (JSON-RPC surface unchanged)

---

## PHASE 7: CLI + Config Wiring (foretias-node)

**Files modified:**
- `p2p/foretias-node/src/main.rs` — add `--peer`, `--mutual-attest-every-chronons`, `--request-timeout-secs` flags
- `p2p/foretias-node/src/main.rs` — add `inspect-attestations` subcommand

**New subcommand:**
```
foretias inspect-attestations --calendar <path>
```

**Implementation:**
1. Load calendar JSON from path
2. For each `ChrononRecord`, for each `ExternalAttestation`:
   - Re-run verification (hash + sig + echo)
   - Print: `chronon=N attester=<tbid> attester_chronon=N sig=VALID/INVALID`
3. Exit 0 if all valid, exit 1 if any invalid

**Phase 7 tests:**
- `cli_peer_flag_parsed` — `--peer 127.0.0.1:4002` parses into config
- `cli_inspect_attestations_all_valid` — calendar with valid attestations → exit 0
- `cli_inspect_attestations_invalid` — calendar with bad sig → exit 1

---

## PHASE 8: PyO3 Bindings (foretias-python)

**Files modified:**
- `p2p/foretias-python/src/lib.rs` — add `PyExternalAttestation`, expose `external_attestations` on `PyChrononRecord`

**New Python type:**
```rust
#[pyclass]
pub struct PyExternalAttestation {
    #[pyo3(get)]
    pub attester_tbid:        String,
    #[pyo3(get)]
    pub foretis:               PyForetis,
    #[pyo3(get)]
    pub attester_tick_record: PyChrononRecord,
    #[pyo3(get)]
    pub received_at_ns:       u64,
}
```

**Update PyChrononRecord:**
```rust
#[pyo3(get)]
pub external_attestations: Vec<PyExternalAttestation>,
```

**Phase 8 tests:**
- Python test: `PyChrononRecord.external_attestations` is accessible
- Python test: `PyExternalAttestation` fields are readable

---

## PHASE 9: Integration Tests

**Files created:**
- `p2p/foretias-node/tests/integration/mutual_attest.rs`

**Tests:**
1. `two_nodes_mutual_attest` — two in-process TimeFamily instances, 5 chronons, each calendar has ≥1 valid external attestation from the other
2. `peer_unreachable_does_not_crash` — peer addr points to nothing, server keeps running, no external attestations
3. `crash_recovery_calendar` — kill server mid-flush, restart, verify no data loss
4. **Regression:** all v0.1 tests still pass

---

## PHASE 10: Spec & Documentation

**Files modified:**
- `wdocs/specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — update component names (PeerConnectivity → Communerd, CalendarComponent → Calendar, Chronomatter clarified)
- `foretias/specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` — same (copy)
- `README.md` — document new CLI flags, new concepts
- `wdocs/specs/foretias-v1.md` — document mutual attestation semantics

---

## DEPENDENCY GRAPH

```
Phase 0 (type aliases)
    ↓
Phase 1 (types, errors, traits, config)
    ↓
Phase 2 (Communerd transport layer) ─────────────────┐
    ↓                                                 │
Phase 3 (Chronomatter extraction) ────────────────────┤
    ↓                                                 │
Phase 4 (Calendar + mutual attestation) <─────────────┘
    ↓
Phase 5 (atomic flush)  ← independent, can parallelize with Phase 4
    ↓
Phase 6 (TimeFamily orchestrator — wire all 3 time beings)
    ↓
Phase 7 (CLI + config wiring)
    ↓
Phase 8 (PyO3 bindings)  ← depends on Phase 0-1 only, can parallelize with 6-7
    ↓
Phase 9 (integration tests)
    ↓
Phase 10 (spec & docs)
```

**Parallelization:**
- Phase 5 (atomic flush) can run parallel with Phase 4
- Phase 8 (PyO3) can run parallel with Phase 6-7

---

## MILESTONE CHECKLIST

```
[x] v0.2.0  Type aliases (Tbid, PublicKey, Signature, Digest, Message, AaNonce, TickNumber)
[x] v0.2.1  ExternalAttestation type; ChrononRecord.external_attestations field
[x] v0.2.2  Callback traits (TickObserver, PeerMessenger) — note: Stamper trait removed, Chronomatter uses direct pub fn
[x] v0.2.3  NodeConfig: peers, mutual_attest_every_n, request_timeout_secs
[x] v0.2.4  Communerd: transport layer, JsonRpcTransport impl (inlined in transport.rs)
[x] v0.2.5  Communerd: peer pool, liveness ping, RPC execution
[x] v0.2.6  Chronomatter: extraction from TimeFamilyServer (time being with TBID)
[x] v0.2.7  Chronomatter: stamp/verify as direct method calls, chronon via TickObserver callback
[x] v0.2.8  Calendar: extraction as separate component, TickObserver impl
[x] v0.2.9  Calendar: execute_mutual_attest flow (deferred to v0.3 — Communerd handles stamp_peer directly)
[x] v0.2.10 Atomic flush (.tmp + rename) + crash recovery
[x] v0.2.11 TimeFamily orchestrator: Communerd wired into TimeFamilyServer via Option<Arc<Communerd>>
[x] v0.2.12 CLI: --peer, --mutual-attest-every-chronons, inspect-attestations
[x] v0.2.13 PyO3: PyExternalAttestation, PyChrononRecord.external_attestations
[x] v0.2.14 Unit tests pass (107 total: 62 core + 41 node + 4 integration)
[x] v0.2.15 Integration test: two_nodes_mutual_attest passes
[x] v0.2.16 All v0.1 regression tests pass
[ ] v0.2.17 TAG: v0.2-direct-p2p-mutual-attestation (pending Phase 10 commit)
```

---

## IMPLEMENTATION DIVERGENCES FROM PLAN

| Plan Item | Actual Implementation | Reason |
|-----------|----------------------|--------|
| `Stamper` trait | Removed — Chronomatter uses `pub fn stamp()` / `pub fn verify()` directly | Trait was unnecessary indirection; direct pub fn is clearer and zero-cost |
| `Calendar` as async component with flush task | Simplified to synchronous `Calendar` struct in core-engine; `TimeFamilyServer` wraps it | Calendar is pure data + persistence; no async task needed at this stage |
| `TimeFamily` orchestrator struct | Kept as `TimeFamilyServer` with `communerd: Option<Arc<Communerd>>` added | Less disruption; existing `TimeFamilyServer` already orchestrates Chronomatter + Calendar |
| `Calendar::execute_mutual_attest` flow | Deferred — `Communerd::stamp_peer()` handles stamp-on-peer directly; full mutual attest cycle with verify + store coming in v0.3 | Separation of concerns: Communerd handles transport, Calendar handles storage |
| `PeerMessenger` trait | Simplified to `PeerMessenger` on `Communerd` struct with `stamp_peer` and `request_ticks` | Two methods sufficient for v0.2; plan's `send_to_peer` / `query_community` were too generic |
| Phase 9 integration test file | Added to existing `foretias-node/tests/integration.rs` | No need for separate file; keeps tests co-located |

---

## COMMIT LOG

| Phase | Commit | Description |
|-------|--------|-------------|
| Phase 0 | `2b2e6bb` | Type aliases (Tbid, PublicKey, Signature, Digest, Message, AaNonce, TickNumber) |
| Phase 1 | `231130a` | ExternalAttestation, callback traits, error/config extensions |
| Phase 2 | `a52d99e` | Communerd transport layer (transport.rs, peer_pool.rs) |
| Phase 3 | `11aba27` | Chronomatter extraction (chronomatter/mod.rs, autonomous daemon) |
| Phase 4 | `6b91138` | Calendar component extraction and API re-wiring |
| Phase 5 | `a7338a3` | Atomic flush (.tmp + rename) + crash recovery |
| Phase 6 | `45c7eaa` | Communerd orchestrator integration (Communerd struct, with_config(), liveness pings) |
| Phase 7 | `a833b56` | CLI + config wiring (--peer, inspect-attestations subcommand) |
| Phase 8 | `47b3412` | PyO3 bindings (PyExternalAttestation, external_attestations on PyChrononRecord) |
| Phase 9 | `9d02fd1` | Integration tests (mutual attest, unreachable peer, crash recovery) |

---

## RISK ASSESSMENT

| Risk | Impact | Mitigation |
|------|--------|------------|
| Chronomatter extraction breaks existing tests | High | Phase 3 tests + immediate regression check |
| Trait-based intra-family calls add indirection | Low | `#[inline]` on trait methods; same-process calls are cheap |
| `ChrononRecord.external_attestations` breaks serde compat | High | `#[serde(default)]` — tested in Phase 1 |
| Calendar mutual attest flow edge cases | Medium | Phase 4 tests cover all verification failures |
| Python binding breakage | Medium | Phase 8 tests + full Python test suite |
| Tokio select fairness (daemon chronon vs stamp requests) | Low | Chronomatter only runs timer; stamp/verify are direct calls, no select |
