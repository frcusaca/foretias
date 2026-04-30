# Fortias — P2P Sub-Spec 6: Heartbeats & Identity Collision Detection (v0.7)

**Milestone tag:** `v0.7-collision-detection`
**Prereq:** `v0.6-probity-gossip` must be tagged.
**Next:** `FORTIAS_2_P2P_8_epoch_consensus.md` (v0.8 — FROST epoch snapshots).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## READING ORDER

1. Confirm `v0.6-probity-gossip` is tagged.
2. Re-read `FORTIAS_0_OVERVIEW.md` §0.1 (per-run entity lifecycle) and §0.2
   (identity collision = dual termination → dormancy). These invariants are
   the specification for §5 of this document.
3. Re-read `FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md` §4.1
   (`TimeFamilyInner.dormant: AtomicBool`) — that field is set here.
4. Read this document end to end before writing any code.

---

## 1. GOAL

Two mechanisms, both in this milestone:

**A. Signed heartbeats.** Every node broadcasts a fresh signed `Heartbeat`
message every 30 seconds (configurable) on a GossipSub topic. Each heartbeat
carries a random nonce so replays are detectable.

**B. Collision detection.** A `CollisionDetector` watches the heartbeat
stream for messages that (a) claim our own `peer_id` and (b) carry a valid
signature under our own public key but a nonce we did not issue. That is
cryptographic proof that another process holds our private key and is
actively using it.

On confirmed collision both the local node and (eventually) the remote
impersonator transition to **dormant** state: they stop stamping and
stop participating in the P2P network, but continue to serve `/verify`
and `/get_calendar_slice` to local clients with the `dormant: true` flag
set in every response.

(@human — "near-impossible under good RNG, possible under VM-clone or
RNG-failure" per §0.2. The detector exists not because collisions are
expected but because the consequences if one occurs — two nodes signing
as the same identity — silently corrupt every attestation chain that
references either of them. Fail-safe dormancy is the correct response.)

---

## 2. ACCEPTANCE DEMO

```bash
# Launch two processes with intentionally shared identity
# (test helper: --force-tbid copies A's key material into B at startup)
$ fortias serve --addr 127.0.0.1:4001 --p2p-listen /ip4/127.0.0.1/tcp/4101
$ fortias serve --addr 127.0.0.1:4002 --p2p-listen /ip4/127.0.0.1/tcp/4102 \
    --p2p-dial /ip4/127.0.0.1/tcp/4101/p2p/<PeerID-A> \
    --force-tbid <tbid-of-A>          # test-only flag; not in production builds
```

Within 60 s:
```
A logs: WARN collision: confirmed peer=<PeerID-A> — entering dormancy
B logs: WARN collision: confirmed peer=<PeerID-A> — entering dormancy

# A's JSON-RPC now returns dormant flag on every response:
$ fortias rpc --method stamp --params '{"content":"68656c6c6f","echo":"test"}'
{"error":{"code":-32001,"message":"node is dormant"}}

# Verify still works on dormant nodes:
$ fortias rpc --method verify ...
{"result":{"valid":true},"dormant":true}
```

(@human — the `--force-tbid` flag is compiled only in `#[cfg(test)]`
or behind a `collision-test` feature flag. It must never ship in
production builds. The CI integration test uses it programmatically
via the in-process API, not the CLI.)

---

## 3. HEARTBEAT — `src/collision/heartbeat.rs`

### 3.1 Type

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    /// PeerId of the broadcaster (hex-encoded libp2p PeerId).
    pub peer_id:      String,
    /// Wall-clock time of this heartbeat, ns since UNIX epoch.
    pub timestamp_ns: u64,
    /// Fresh random 16-byte nonce — unique per heartbeat, never reused.
    pub nonce:        [u8; 16],
    /// 1 = Ed25519, 2 = P-256.
    pub curve:        u8,
    /// Signature over canonical() bytes.
    pub signature:    Vec<u8>,
}

impl Heartbeat {
    /// Canonical bytes for signing: peer_id || timestamp_ns || nonce || curve.
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.peer_id.as_bytes());
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(&self.nonce);
        buf.push(self.curve);
        buf
    }
}
```

### 3.2 GossipSub topic

```rust
// src/network/gossip.rs  (addition)
pub fn heartbeat_topic(namespace: &str) -> IdentTopic {
    IdentTopic::new(format!("/fortias/{}/heartbeat/v1", namespace))
}
```

Subscribe on swarm start, alongside the probity topic.

### 3.3 Heartbeat broadcaster task

Started as a Tokio task inside `TimeFamily::start`:

```rust
async fn heartbeat_broadcaster(
    inner:          Arc<TimeFamilyInner>,
    interval_secs:  u64,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if inner.dormant.load(Ordering::SeqCst) { continue; } // dormant: stop broadcasting
        let hb = build_heartbeat(&inner);
        if let Some(handle) = inner.swarm_handle.lock().as_ref() {
            let _ = handle.cmd_tx.send(SwarmCommand::PublishHeartbeat(hb));
        }
    }
}

fn build_heartbeat(inner: &TimeFamilyInner) -> Heartbeat {
    let mut nonce = [0u8; 16];
    inner.crypto.fill_random(&mut nonce).expect("rng");
    let timestamp_ns = now_ns();
    let peer_id = inner.local_peer_id_str.clone();
    let curve = 1u8;
    let mut hb = Heartbeat { peer_id, timestamp_ns, nonce, curve, signature: vec![] };
    if let Ok(sig) = inner.crypto.sign(&hb.canonical()) {
        hb.signature = sig.bytes.to_vec();
    }
    hb
}
```

The broadcaster does nothing while `dormant` is `true`. It does not
exit — if the application ever clears dormancy (not currently possible;
reserved for future FOSITAS logic), the loop resumes automatically.

---

## 4. COLLISION DETECTOR — `src/collision/detector.rs`

```rust
pub struct CollisionDetector {
    my_peer_id:       String,
    my_pub_key:       FortiasPubKey32,       // our own Ed25519 public key
    my_nonces:        parking_lot::Mutex<std::collections::VecDeque<[u8; 16]>>,
    // Nonces we have issued in the last heartbeat_window; a heartbeat
    // with our peer_id but an unknown nonce is a collision signal.
    nonce_window:     usize,                 // default 10 (last 10 heartbeats kept)
}

impl CollisionDetector {
    /// Register a nonce we just broadcast. Call after every self-heartbeat.
    pub fn register_own_nonce(&self, nonce: [u8; 16]) {
        let mut guard = self.my_nonces.lock();
        if guard.len() >= self.nonce_window { guard.pop_front(); }
        guard.push_back(nonce);
    }

    /// Called for every heartbeat received from the gossip stream.
    /// Returns Some(CollisionEvent) if a confirmed collision is detected.
    pub fn on_heartbeat(
        &self,
        hb:     &Heartbeat,
        crypto: &dyn CryptoServer,
    ) -> Option<CollisionEvent> {
        if hb.peer_id != self.my_peer_id { return None; }

        // A heartbeat claiming our peer_id: check if it's actually ours.
        let is_known_nonce = self.my_nonces.lock().iter().any(|n| *n == hb.nonce);
        if is_known_nonce { return None; } // our own heartbeat echoed back

        // Unknown nonce claiming our peer_id: verify the signature under our key.
        // If it verifies, this is cryptographic proof of collision.
        let sig_bytes: [u8; 64] = hb.signature.get(..64)?.try_into().ok()?;
        let sig = FortiasSig64 { bytes: sig_bytes };
        let valid = crypto.verify_ed25519(&self.my_pub_key, &hb.canonical(), &sig).ok()?;
        if !valid { return None; } // bogus claim; ignore

        Some(CollisionEvent::Confirmed { foreign_heartbeat: hb.clone() })
    }
}

#[derive(Debug, Clone)]
pub enum CollisionEvent {
    Confirmed { foreign_heartbeat: Heartbeat },
}
```

---

## 5. DORMANCY PROTOCOL

On receiving `CollisionEvent::Confirmed`:

```rust
// src/collision/escalation.rs

pub async fn handle_confirmed_collision(
    inner:  Arc<TimeFamilyInner>,
    event:  CollisionEvent,
) {
    tracing::warn!(?event, "collision confirmed — entering dormancy");

    // 1. Set dormant flag immediately. All inbound /stamp requests now
    //    return NodeError::Dormant (-32001). /verify and /get_calendar_slice
    //    continue to work but include "dormant": true in every response.
    inner.dormant.store(true, Ordering::SeqCst);

    // 2. Stop the heartbeat broadcaster (it checks dormant flag itself).

    // 3. Stop the mutual-attest scheduler.
    inner.peer_conn.set_dormant(true);

    // 4. Notify the liege channel (stub in v0.7; real implementation is
    //    FOSITAS application logic doing stable-marriage peer matching).
    inner.liege.send_help(HelpReason::IdentityCollision {
        foreign_heartbeat: match &event { CollisionEvent::Confirmed { foreign_heartbeat } =>
            foreign_heartbeat.clone()
        },
    }).ok();

    // 5. Wait up to 30 s for application response (liege may instruct migration
    //    or graceful handover; for v0.7 the stub does nothing and we proceed).
    tokio::time::sleep(Duration::from_secs(30)).await;

    // 6. Disconnect all libp2p peers. Stop DHT announcements. Stop GossipSub
    //    publishing. The node remains alive to serve local verify queries.
    if let Some(handle) = inner.swarm_handle.lock().as_ref() {
        let _ = handle.cmd_tx.send(SwarmCommand::EnterDormancy);
    }

    tracing::warn!("dormancy complete: P2P suspended; local verify still active");
}
```

`SwarmCommand::EnterDormancy` causes the swarm loop to:
- Close all open connections (`swarm.disconnect_peer_id` for each).
- Stop providing on the DHT (stop cross-attest-willing provider record).
- Unsubscribe from all GossipSub topics.
- Stop accepting new connections (stop the TCP listener).

(@human — "remain a local process that can still serve verify/calendar
requests" is the design invariant from §0.2. A Fortias node with an ongoing
obligation to a local client — e.g., a user who stamped documents this
hour — must be able to answer "is this stamp valid?" even after a collision
terminates its network participation. The dormant flag in every response
tells callers the node's state so they can seek corroboration elsewhere.)

---

## 6. LIEGE CHANNEL STUB — `src/liege/stub.rs`

```rust
pub trait LiegeChannel: Send + Sync {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError>;
}

#[derive(Debug, Clone)]
pub enum HelpReason {
    IdentityCollision { foreign_heartbeat: Heartbeat },
    SuspectedBadActor { peer_id: String },
}

/// Stub implementation: pushes to an internal mpsc channel.
/// The application can poll this channel to observe collision events.
pub struct StubLiegeChannel {
    tx: tokio::sync::mpsc::UnboundedSender<HelpReason>,
}

impl LiegeChannel for StubLiegeChannel {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError> {
        self.tx.send(reason).map_err(|e| NodeError::Internal(e.to_string()))
    }
}
```

`Arc<dyn LiegeChannel>` is held on `TimeFamilyInner`. Default: `StubLiegeChannel`.
Real implementation comes from FOSITAS application logic (stable-marriage
peer matching); it replaces the stub by supplying a different `LiegeChannel`
impl at `TimeFamily` construction time.

(@human — the stub exists so that v0.7 can be tested end-to-end without
FOSITAS. The collision event is observable through the application's
mpsc channel and through the node's dormant state. Real liege logic —
which might say "there is a living backup; transfer stamping authority" —
is FOSITAS's concern, not the P2P layer's.)

---

## 7. INTERPLAY WITH PRIOR SUB-SPECS

| Component | Dormancy effect |
|---|---|
| `/stamp` handler | Returns `{"error":{"code":-32001,"message":"node is dormant"}}` |
| `/verify` handler | Returns result normally; adds `"dormant": true` to response |
| `/get_calendar_slice` | Returns result normally; adds `"dormant": true` |
| Mutual-attest scheduler | Stops sending new requests; in-flight requests complete |
| Inbound mutual-attest (foreign `/stamp` call) | Returns dormant error |
| Heartbeat broadcaster | Stops broadcasting |
| GossipSub publisher | Stops publishing (probity reports, heartbeats) |
| GossipSub subscriber | Continues receiving (node still observes) |
| DHT provider record | Removed (node no longer advertises as cross-attest-willing) |
| `/get_health` admin | Returns `"dormant": true`; includes `"collision_at_ns"` timestamp |

---

## 8. PROBITY INTEGRATION

On confirmed collision, before entering dormancy, emit a probity report
on the colliding peer_id:

```rust
inner.time_family.report_probity(
    &colliding_peer_id,
    "identity_integrity",
    -100.0,   // maximum negative — collision is the worst possible signal
);
```

This gossips a signed report so the rest of the network learns about the
collision through the v0.6 probity mechanism, not just through direct
observation.

---

## 9. CONFIGURATION ADDITIONS

```json
{
  "collision": {
    "heartbeat_interval_secs": 30,
    "nonce_window":            10,
    "liege_wait_secs":         30
  }
}
```

---

## 10. TEST PLAN

- `heartbeat_canonical_roundtrip` — build a Heartbeat, sign via software CryptoServer, call `canonical()`, verify signature.
- `detector_ignores_own_echo` — detector receives the same heartbeat it just sent; returns None.
- `detector_ignores_different_peer` — heartbeat from a different peer_id; returns None.
- `detector_ignores_invalid_signature` — heartbeat for our peer_id with wrong sig; returns None.
- `detector_confirms_collision` — heartbeat for our peer_id, valid sig, unknown nonce; returns `Some(Confirmed)`.
- `dormancy_stops_stamping` — after `handle_confirmed_collision`, `/stamp` returns dormant error.
- `dormancy_allows_verify` — after dormancy, `/verify` still returns `valid: true` with `dormant: true`.
- Integration: `two_processes_collision_both_go_dormant` — intentionally shared identity (in-process, no `--force-tbid` needed); both TimeFamily instances detect collision and set dormant within 60 s.
- All v0.1–v0.6 tests pass.

---

## 11. NON-GOALS FOR v0.7

- ❌ Real liege logic (stable-marriage matching) — FOSITAS application layer.
- ❌ Automatic recovery from dormancy — no mechanism exists; dormancy is permanent per §0.2.
- ❌ P-256 heartbeat signatures — Ed25519 only in v0.7.
- ❌ Special election triggered by collision (the epoch framework doesn't exist yet) — v0.8 wires this.

---

## 12. MILESTONE CHECKLIST

```
[ ] v0.7.1  Heartbeat type + canonical() + GossipSub topic.
[ ] v0.7.2  Heartbeat broadcaster task; register_own_nonce integration.
[ ] v0.7.3  CollisionDetector: on_heartbeat, nonce window.
[ ] v0.7.4  LiegeChannel trait + StubLiegeChannel.
[ ] v0.7.5  handle_confirmed_collision: dormant flag, scheduler stop,
            liege notify, 30 s wait, SwarmCommand::EnterDormancy.
[ ] v0.7.6  SwarmCommand::EnterDormancy: disconnect peers, stop DHT
            providing, unsubscribe gossip topics, stop listener.
[ ] v0.7.7  Update /stamp, /verify, /get_calendar_slice handlers for
            dormant flag.
[ ] v0.7.8  Probity report emitted on collision.
[ ] v0.7.9  two_processes_collision_both_go_dormant integration test.
[ ] v0.7.10 All prior tests pass.
[ ] v0.7.11 TAG: v0.7-collision-detection.
```

---
# END OF SUB-SPEC 6 (v0.7)
