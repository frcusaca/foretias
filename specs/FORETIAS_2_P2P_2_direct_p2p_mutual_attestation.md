# Foretias — P2P Sub-Spec 1: Direct P2P, Mutual Attestation & Threading Model (v0.2)

**Milestone tag:** `v0.2-direct-p2p-mutual-attestation`
**Prereq:** `v0.1-local-server-mvp` must be tagged.
**Next:** `FORETIAS_2_P2P_3_libp2p_handshake.md` (v0.3 — adds libp2p as a second transport alongside this one).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.1-local-server-mvp` is tagged.
2. Read `FORETIAS_0_OVERVIEW.md` Part 0 (invariants) — especially §0.7 (Python prototype is the semantic source of truth).
3. Read `FORETIAS_1_MVP_SPEC.md` end to end.
4. Read this document end to end.
5. Confirm the threading model (§4) and mutual-attestation flow (§6) are understood before writing any code.

---

## 1. GOAL

Restructure `TimeFamilyServer` into three time beings — **Chronomatter** (*Chronos fidelis authenticus*), **Calendar** (*Chronos fidelis grapha*), **Communerd** (*Chronos fidelis Locutus*)— each with its own TBID, coordinated by a **TimeFamily** (*Chronos fidelis adunatrix*, the orchestrator). Intra-family communication uses direct method calls and callbacks. Only Communerd communicates with extra-family peers. Then, using only the existing v0.1 JSON-RPC primitives (`/stamp`, `/verify`, `/get_calendar_slice`), implement **mutual attestation**: two statically-configured Foretias nodes periodically call each other's `/stamp` with their current `TickRecord` as content, verify the returned `Foretis`, and store it as an `ExternalAttestation` in their local calendar.

**Terminology note:** "auto-attestation" (v0.1/v0.2.0) refers to **intra-node** chronon chaining (same TBID, consecutive chronons linked by `aa_nonce`). "Mutual attestation" (this sub-spec) refers to **cross-node** stamping (different TBID). When TBID is different, it is no longer called "auto-attestation."

(@human — nothing about this sub-spec requires transport confidentiality.
`/stamp` and `/verify` are not secrets; their content is public unless the
caller encrypts it before submitting. Noise handshake and transport
security come in v0.3 when libp2p is added. The direct TCP JSON-RPC
channel used here is intentionally plain. This is the minimal first step
that produces the "two servers stamp each other's ticks" demo.)

---

## 2. WHAT v0.1 PROVIDES

| Symbol | Where | Role |
|---|---|---|
| `CryptoServer` trait | `src/crypto_server/mod.rs` | Signs and hashes; `stamp()` uses it |
| `stamp()` / `verify()` free fns | `src/foretias/tick.rs` | Core primitive — unchanged |
| `Calendar` struct + `CalendarLookup` | `src/foretias/calendar.rs` | Extended here |
| `TickRecord`, `Foretis` | `src/foretias/tick.rs` | Extended here (`TickRecord` gains `external_attestations`) |
| `TimeFamilyServer` | `src/server/mod.rs` | **Replaced** by the new `TimeFamily` architecture |
| JSON-RPC handlers (`stamp`, `verify`, `get_calendar_slice`) | `src/server/handlers.rs` | Preserved; wired to new architecture |

v0.1 tests must keep passing. The JSON-RPC surface is identical.

---

## 3. ACCEPTANCE DEMO

```bash
# Terminal A — chronon 20 s, so ticks are fast for the demo
$ foretias serve \
    --addr 127.0.0.1:4001 \
    --calendar-path /tmp/a/calendar.json \
    --chronon-ns 20000000000 \
    --peer 127.0.0.1:4002 \
    --mutual-attest-every-chronons 1

# Terminal B
$ foretias serve \
    --addr 127.0.0.1:4002 \
    --calendar-path /tmp/b/calendar.json \
    --chronon-ns 20000000000 \
    --peer 127.0.0.1:4001 \
    --mutual-attest-every-chronons 1
```

After ~2 minutes (≥5 ticks each):

```bash
$ foretias inspect-attestations --calendar /tmp/a/calendar.json
tick=1  attester=<B tbid>  attester_tick=2   sig=VALID
tick=2  attester=<B tbid>  attester_tick=4   sig=VALID
...
ALL VALID (5/5)
```

Both calendars must show `ALL VALID`. Both must survive a `Ctrl+C` and reload without data loss.

(@human — the exact attester tick numbers depend on chronon alignment. What
matters is that every `sig=VALID` line appears and the count is non-zero on
both sides.)

---

## 4. THREADING MODEL

### 4.1 Component overview

All three components are **time beings** with their own TBID. They communicate via direct method calls and callbacks within the family. Only Communerd communicates outside the family.

```
TimeFamily — orchestrator, creates and wires all three time beings
│
├── Chronomatter (Chronos fidelis authenticus) ─────────────────────────
│     Time being with its own TBID.
│     Advances tick on chronon timer (autonomous; NOT on /stamp call).
│     Owns stamping and verification.
│     On tick advance: destroys old private key, generates new keypair,
│     calls Calendar.on_tick_advance() (direct callback).
│     Exposes Stamper trait: stamp(), verify() — called directly by Calendar.
│
├── Calendar (Chronos fidelis grapha) ──────────────────────────────────
│     Time being with its own TBID.
│     Owns the in-memory Calendar struct behind a RwLock.
│     Receives tick notification from Chronomatter via TickObserver callback.
│     Persists to disk (periodic flush, atomic .tmp + rename).
│     Owns the mutual-attestation scheduler (fires per N chronons,
│     calls Communerd.send_to_peer() directly,
│     calls Chronomatter.verify() directly, stores the result).
│     Communicates ONLY through Communerd for extra-family calls.
│
└── Communerd (P2P communication) ──────────────────────────────────────
      Time being with its own TBID.
      All point-to-point and peer-to-peer communication.
      Maintains a pool of direct JSON-RPC TCP clients to configured peers.
      Liveness: periodic no-op ping.
      Executes RPC calls on behalf of Calendar's mutual-attest scheduler.
      Transport is behind a PeerTransport trait (§5.1) — direct JSON-RPC
      TCP for v0.2; libp2p stream / gRPC pluggable later.
      Knows NOTHING about Foretias semantics — transparent RPC relay.
      Handles community state queries (peer liveness, known families).
```

**Intra-family communication:** Low-latency method calls and callbacks. No channels, no broadcast, no message passing. Members hold `Arc<dyn Trait>` references to each other.

**Extra-family communication:** Only through Communerd. Calendar calls `communerd.send_to_peer()` for all remote RPC.

**Key responsibilities:**
- **Chronomatter**: ticking, stamping, verification (all crypto operations)
- **Calendar**: data, persistence, mutual attestation scheduling
- **Communerd**: all network communication (Calendar calls Communerd, never talks network directly)
- **TimeFamily**: orchestrator — creates/wires all three time beings

**Communerd etymology:** A Communerd is a communard of a Time Family commune where timing information is shared in communal communion between families, AND he's a nerd about communications.

**Rate limiting and version checks:** Deferred to v0.5 hardening. Not implemented in v0.2.

(@human — the spec originally described "two-layer version/rate checks" but we've deferred those to v0.5 hardening. The double gate remains a design goal but is out of scope for v0.2.)

### 4.2 Type aliases

```rust
// src/foretias/types.rs

/// Time Being ID — 16-byte unique identifier for a time being.
pub type Tbid = [u8; 16];

/// Ed25519 public key — 32 bytes.
pub type PublicKey = [u8; 32];

/// Ed25519 signature — 64 bytes.
pub type Signature = [u8; 64];

/// SHA-256 hash digest — 32 bytes.
pub type Digest = [u8; 32];

/// Message payload (before/after encryption).
pub type Message = Vec<u8>;

/// Auto-attestation nonce — 16 bytes of entropy.
pub type AaNonce = [u8; 16];

/// Tick number.
pub type TickNumber = u64;
```

### 4.3 Intra-family trait interfaces

```rust
// Called by Chronomatter when a new tick advances.
#[async_trait::async_trait]
pub trait TickObserver: Send + Sync {
    fn on_tick_advance(&self, tick_number: TickNumber, public_key: &PublicKey);
}

// Stamper interface — Chronomatter implements this.
#[async_trait::async_trait]
pub trait Stamper: Send + Sync {
    async fn stamp(&self, content: Message, echo: String)
        -> Result<Foretis, NodeError>;
    async fn verify(&self, foretis: &Foretis, content: &Message)
        -> Result<bool, NodeError>;
}

// PeerMessenger interface — Communerd implements this.
#[async_trait::async_trait]
pub trait PeerMessenger: Send + Sync {
    async fn send_to_peer(&self, addr: &PeerAddr, method: &str, params: serde_json::Value)
        -> Result<serde_json::Value, TransportError>;
    async fn query_community(&self, query: CommunityQuery)
        -> Result<CommunityResponse, TransportError>;
}
```

### 4.4 Key types

```rust
// src/time_family/mod.rs

pub struct TimeFamily {
    chronomatter_task: tokio::task::JoinHandle<()>,
    calendar_task:     tokio::task::JoinHandle<()>,
    communerd_task:    tokio::task::JoinHandle<()>,
    /// Direct references to time beings for JSON-RPC handlers
    chronomatter:      Arc<dyn Stamper>,
    calendar:          Arc<RwLock<CalendarData>>,
    /// Config
    config:            NodeConfig,
    /// Dormant flag
    dormant:           AtomicBool,
}
```

The existing `TimeFamilyServer` struct in `src/server/mod.rs` is replaced with `TimeFamily`. All public API surfaces (JSON-RPC handlers, CLI entry points) are updated to use the new type. Old `current_tick: Mutex<u64>` is removed — Chronomatter owns that state now.

---

## 5. PEER TRANSPORT ABSTRACTION

```rust
// src/communerd/transport.rs

#[async_trait::async_trait]
pub trait PeerTransport: Send + Sync {
    /// Call /stamp on the remote peer. Returns the raw Foretis JSON value.
    async fn stamp(&self, peer: &PeerAddr, content_hex: &str, echo: &str)
        -> Result<serde_json::Value, TransportError>;

    /// Call /get_calendar_slice on the remote peer.
    async fn get_calendar_slice(&self, peer: &PeerAddr, tick_start: u64, count: u64)
        -> Result<Vec<TickRecord>, TransportError>;

    /// Liveness probe. Returns Ok(()) if the peer responded.
    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
}

#[derive(Debug, Clone)]
pub struct PeerAddr {
    /// host:port for direct JSON-RPC TCP (v0.2).
    pub json_rpc: String,
    /// Optional libp2p PeerId (populated in v0.3+).
    pub peer_id:  Option<libp2p::PeerId>,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connect failed: {0}")]  Connect(String),
    #[error("rpc error {code}: {message}")] Rpc { code: i32, message: String },
    #[error("timeout")]              Timeout,
    #[error("decode error: {0}")]   Decode(String),
}
```

### 5.1 `JsonRpcTransport` — v0.2 implementation

```rust
// src/communerd/json_rpc_transport.rs

pub struct JsonRpcTransport {
    request_timeout: std::time::Duration,
}

#[async_trait::async_trait]
impl PeerTransport for JsonRpcTransport {
    async fn stamp(&self, peer: &PeerAddr, content_hex: &str, echo: &str)
        -> Result<serde_json::Value, TransportError>
    {
        json_rpc_call_with_timeout(
            &peer.json_rpc,
            "stamp",
            serde_json::json!({"content": content_hex, "echo": echo}),
            self.request_timeout,
        ).await
    }

    async fn get_calendar_slice(&self, peer: &PeerAddr, tick_start: u64, count: u64)
        -> Result<Vec<TickRecord>, TransportError>
    { /* same pattern */ }

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>
    { /* small no-op stamp or a dedicated probe; just checks TCP connects */ }
}
```

`json_rpc_call_with_timeout` is extracted from v0.1's `main.rs` helper; same newline-delimited framing, same 4 KB cap.

(@human — the reason PeerTransport is defined here rather than as part of
the libp2p spec (v0.3) is deliberate: v0.3 adds a second impl, not a first.
If we wait until v0.3 to define the trait we end up retrofitting an
abstraction boundary into already-running code. The cost of the trait in
v0.2 is one extra file and zero runtime overhead.)

---

## 6. MUTUAL ATTESTATION

### 6.1 ExternalAttestation type

```rust
// src/foretias/external_attestation.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAttestation {
    /// The attesting peer's tbid (hex-encoded).
    pub attester_tbid:        String,
    /// The Foretis B issued for A's TickRecord content.
    pub foretis:               Foretis,
    /// B's TickRecord at foretis.tick_number, captured at attestation time.
    /// Stored so we can re-verify offline without contacting B.
    pub attester_tick_record: TickRecord,
    /// Wall-clock receive time, ns since UNIX epoch.
    pub received_at_ns:       u64,
}
```

`TickRecord` gains one new field:

```rust
pub struct TickRecord {
    pub tick_number:          u64,
    pub public_key:           Vec<u8>,
    pub forward_foretis:       Vec<u8>,
    pub backward_foretis:      Vec<u8>,
    #[serde(default)]
    pub external_attestations: Vec<ExternalAttestation>,  // NEW; empty on old records
}
```

`#[serde(default)]` preserves backward compatibility with all v0.1 calendar files.

### 6.2 Mutual-attest flow (Calendar component owns scheduling)

```
Every N chronons (default N=1), for each configured peer:

1. Calendar::schedule_tick() is called by the chronomatter after
    each tick advance.
2. Calendar checks if this tick number mod every_n == 0. If yes:
3. Snapshot current_tick and its TickRecord (read from in-memory calendar).
4. content = serde_json::to_vec(&tick_record)  // minified JSON — the whole record
5. content_hex = hex::encode(&content)
6. echo = format!("ca:{}:{}", local_tbid_hex, tick_number)
7. Hand (peer_addr, content_hex, echo, local_tick_number) to Communerd.

Communerd executes two calls in sequence:
8.  foretis_val = transport.stamp(peer, content_hex, echo).await?
9.  Parse foretis_val into Foretis struct.
10. tr_vec = transport.get_calendar_slice(peer, foretis.tick_number, 1).await?
11. attester_tr = tr_vec.into_iter().next()?

Verification (local, no network):
12. recomputed_hash = sha256(content)
    → must equal foretis.content_hash
13. sig_input = foretis.tbid || foretis.tick_number.to_be_bytes() || content
    → verify ed25519(attester_tr.public_key, sig_input, foretis.signature)
14. foretis.echo must equal the echo we sent in step 6.
15. If any check fails: log WARN, record failure counter. Do NOT store.
    (v0.6 will attach a ProbityReport here.)

On success:
16. attestation = ExternalAttestation { attester_tbid, foretis, attester_tick_record, received_at_ns }
17. Calendar::add_external_attestation(local_tick_number, attestation)
```

### 6.3 Inbound: handling foreign mutual-attest calls

Foreign peers call our standard `/stamp` endpoint. There is nothing special about a mutual-attest call from the receiver's perspective — it is simply an inbound `/stamp` with some bytes as content. B does not parse or interpret `content`; it stamps it.

The only version check for inbound requests is the general two-layer check described in §4.1 (family layer + chronomatter layer). No special mutual-attest handler exists on the receiver side.

(@human — this is the cleanest design choice: the receiver has no concept
of "mutual attestation." It just stamps whatever it receives. The semantic
interpretation ("those bytes are actually A's tick record") lives entirely
in A's Calendar. This means the /stamp interface is fully general
and any third party can use it as a one-way timestamping service without
knowing about the mutual-attest protocol.)

---

## 7. CHRONOMATTER — AUTONOMOUS TICKING

```rust
// src/chronomatter/mod.rs

pub struct Chronomatter {
    current_tick: AtomicU64,
    keypair: Mutex<Option<(PrivKeyHandle, Vec<u8>)>>,
    tbid: [u8; 16],
    tbn: String,
    crypto: Arc<dyn CryptoServer>,
    /// Channel for stamp requests from TimeFamily/Calendar
    stamp_rx: broadcast::Receiver<StampRequest>,
    /// Channel for verify requests from Calendar
    verify_rx: broadcast::Receiver<VerifyRequest>,
    /// Broadcast new tick events to Calendar
    tick_tx: broadcast::Sender<TickEvent>,
}

#[derive(Debug)]
pub struct StampRequest {
    pub content:  Vec<u8>,
    pub echo:     String,
    pub reply_tx: oneshot::Sender<Result<Foretis, NodeError>>,
}

#[derive(Debug)]
pub struct VerifyRequest {
    pub foretis:   Foretis,
    pub content:  Vec<u8>,
    pub reply_tx: oneshot::Sender<Result<bool, NodeError>>,
}

#[derive(Clone)]
pub struct TickEvent {
    pub tick_number: u64,
    pub public_key:  Vec<u8>,   // new tick's public key, for Calendar
}
```

The chronomatter task loop:

```rust
async fn run(mut self, chronon_ns: u64) {
    let mut interval = tokio::time::interval(
        tokio::time::Duration::from_nanos(chronon_ns)
    );
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                self.advance_tick().await;
                // Broadcast tick event to Calendar
                let _ = self.tick_tx.send(TickEvent {
                    tick_number: self.current_tick.load(SeqCst),
                    public_key: self.current_public_key(),
                });
            }
            Some(req) = self.stamp_rx.recv() => {
                let result = self.stamp(&req.content, &req.echo).await;
                let _ = req.reply_tx.send(result);
            }
            Some(req) = self.verify_rx.recv() => {
                let result = self.verify(&req.foretis, &req.content).await;
                let _ = req.reply_tx.send(result);
            }
        }
    }
}
```

`advance_tick` generates a new keypair, destroys the old private key via `zeroize`, creates the `TickRecord`, and calls `calendar.on_tick_advance(tick_number, &public_key)`.

(@human — the tick advancing independently of /stamp calls changes the
v0.1 behaviour where every stamp advanced the tick. Under the new model
multiple stamps within one chronon all get the same tick_number and sign
with the same ephemeral key. The chain-of-trust invariant holds because
the key is still destroyed when the tick advances — it's just that
"advance" now means "chronon elapsed," not "stamp called." The Python
prototype's per-stamp tick advance was an artifact of not having a
background timer; this is the correct behavior per the design intent.)

---

## 8. CALENDAR — AUTONOMOUS PERSISTENCE

```rust
// src/foretias-node/src/calendar/mod.rs

/// Calendar is a time being with its own TBID.
pub struct Calendar {
    tbid:             Tbid,
    inner:            RwLock<CalendarData>,   // in-memory calendar data
    persist_path:     PathBuf,
    flush_interval:   Duration,               // default 30 s
    dirty:            AtomicBool,             // set on any mutation
    flush_rx:         mpsc::Receiver<()>,     // signal: flush now
    chronomatter:     Arc<dyn Stamper>,       // direct ref for stamp/verify
    communerd:        Arc<dyn PeerMessenger>, // direct ref for extra-family RPC
    config:           NodeConfig,
}

impl Calendar {
    pub fn add_tick_record(&self, rec: TickRecord) -> Result<(), NodeError> {
        self.inner.write().append(rec)?;
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn add_external_attestation(&self, local_tick: u64, att: ExternalAttestation)
        -> Result<(), NodeError>
    {
        self.inner.write().add_external_attestation(local_tick, att)?;
        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn snapshot(&self) -> CalendarData { self.inner.read().clone() }
}

/// Calendar implements TickObserver — called by Chronomatter on each tick advance.
impl TickObserver for Calendar {
    fn on_tick_advance(&self, tick_number: TickNumber, _public_key: &PublicKey) {
        // Check if we should mutual attest this tick
        if tick_number > 0 && tick_number % self.config.mutual_attest_every_n == 0 {
            let this = Arc::clone(self);
            tokio::spawn(async move { this.execute_mutual_attest(tick_number).await });
        }
    }
}
```

The calendar task:

```rust
async fn run(self: Arc<Self>) {
    let mut interval = tokio::time::interval(self.flush_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if self.dirty.swap(false, Ordering::Relaxed) {
                    self.flush_to_disk();
                }
            }
            Some(()) = self.flush_rx.recv() => {
                self.dirty.store(false, Ordering::Relaxed);
                self.flush_to_disk();
            }
        }
    }
}
```

`flush_to_disk` writes the calendar JSON atomically: write to `<path>.tmp`, then `rename` (POSIX-atomic). Never leaves a partial file on disk.

---

## 9. CONFIGURATION

```json
{
  "listen_addr":   "127.0.0.1:4001",
  "calendar_path": "/var/lib/foretias/calendar.json",
  "chronon_ns":    60000000000,
  "version":       1,
  "calendar": {
    "flush_interval_secs": 30
  },
  "mutual_attest": {
    "peers":                ["127.0.0.1:4002"],
    "every_n_chronons":     1,
    "request_timeout_secs": 5
  }
}
```

CLI flags: `--peer <host:port>` (repeatable), `--mutual-attest-every-chronons N`, `--calendar-path <path>`, `--version N`.

---

## 10. ERROR HANDLING

| Condition | Behaviour |
|---|---|
| Peer unreachable on mutual-attest | Log WARN once per chronon per peer; skip that peer this cycle; failure counter incremented |
| Mutual-attest verification failure (bad sig / hash) | Log WARN; discard attestation; failure counter incremented; v0.6 emits ProbityReport |
| Calendar flush I/O error | Log ERROR; set `dirty=true` (retried next flush interval); process does not crash |
| Process killed mid-flush | `<path>.tmp` left on disk; on next startup: if `.tmp` exists and is valid JSON, rename it to overwrite the old file; if corrupt, delete `.tmp` and load old file |

---

## 11. NEW CLI COMMANDS

```
foretias serve        -- as before, extended with cross-attest flags
foretias inspect-attestations --calendar <path>
    Loads calendar JSON, re-runs verification on every ExternalAttestation,
    prints one line per attestation, exits 0 if all valid.
```

---

## 12. TEST PLAN

### 12.1 Unit

- `chronomatter_ticks_on_interval` — two stamps within one chronon share the same tick_number.
- `chronomatter_version_check` — mismatched version job returns VersionMismatch error.
- `chronomatter_rate_limit` — burst beyond limit returns RateLimited.
- `calendar_flush_atomic` — simulated crash mid-write; reload recovers to last good state.
- `external_attestation_serde_compat` — v0.1 calendar JSON loads; `external_attestations` defaults to empty.
- `mutual_attest_verify_rejects_bad_sig`
- `mutual_attest_verify_rejects_echo_mismatch`

### 12.2 Integration

- `two_nodes_mutual_attest` — two in-process TimeFamily instances; 5 chronons; each calendar has ≥1 valid external attestation from the other.
- `peer_unreachable_does_not_crash` — peer addr points to nothing; server keeps running; calendar has no external attestations.
- `crash_recovery` — kill Calendar task mid-flush; restart; verify no data loss.

### 12.3 Regression

All v0.1 integration tests pass unchanged (JSON-RPC surface is identical).

---

## 13. NON-GOALS FOR v0.2

- ❌ Transport encryption or authentication — plain TCP; content security is caller's concern.
- ❌ libp2p — belongs to v0.3.
- ❌ DHT peer discovery — belongs to v0.4.
- ❌ Probity reports — v0.6 attaches them to the failure-counter sites.
- ❌ Heartbeats / collision detection — v0.7.
- ❌ Encrypted calendar persistence — v0.5.

---

## 14. MILESTONE CHECKLIST

```
[ ] v0.2.1  Define TimeFamily, TimeFamilyInner, component interfaces;
            cargo build succeeds; all v0.1 tests pass.
[ ] v0.2.2  Implement Chronomatter (autonomous tick, stamp/verify via channels).
[ ] v0.2.3  Implement Calendar (RwLock, persistence task, atomic flush).
[ ] v0.2.4  Wire Chronomatter → Calendar on tick advance.
[ ] v0.2.5  Define PeerTransport trait; implement JsonRpcTransport.
[ ] v0.2.6  Implement Communerd (pool, liveness ping, RPC execution).
[ ] v0.2.7  Implement ExternalAttestation type; TickRecord backward-compat serde.
[ ] v0.2.8  Implement mutual-attest scheduler in Calendar.
[ ] v0.2.9  inspect-attestations CLI command.
[ ] v0.2.10 Unit tests pass; integration test two_nodes_mutual_attest passes.
[ ] v0.2.11 Demo script passes (§3 scenario, automated, CI-runnable).
[ ] v0.2.12 Re-run every v0.1 test — all pass.
[ ] v0.2.13 TAG: v0.2-direct-p2p-mutual-attestation.
```

(@human — at the end of v0.2 you have the stated demo: two servers start,
find each other via static config, cross-stamp each other's ticks on every
chronon, and persist the foretises. Every attestation is locally verifiable.
No libp2p, no Noise, no DHT. The networking is transparent TCP JSON-RPC.)

---
# END OF SUB-SPEC 1 (v0.2)
