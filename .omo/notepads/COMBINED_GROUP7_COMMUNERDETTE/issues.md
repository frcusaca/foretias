# Issues — COMBINED_GROUP7_COMMUNERDETTE Phase 8.2

**Date:** 2026-06-01
**Scope:** Calendar `DoAttestation` placeholder analysis for CommunerdetteLine integration

---

## 1. Current State of Calendar `DoAttestation` Implementation

### 1.1 Task Queue Definition

**File:** `p2p/foretias-server/src/calendar/task_queue.rs`

The `CalendarTask::DoAttestation { peer: PeerAddr }` variant is defined at line 39. It carries a `PeerAddr` (json_rpc address + optional peer_id), NOT a `Tbid`.

### 1.2 Task Handler (PLACEHOLDER)

**File:** `p2p/foretias-server/src/calendar/task_queue.rs:254-259`

```rust
CalendarTask::DoAttestation { peer } => {
    // Phase 4b.4d: move existing mutual-attestation into this task.
    // Existing path in Chronomatter remains the source of truth until
    // we refactor it; this handler is a structural placeholder so the
    // task variant is wired end-to-end.
    debug!(worker_id, peer = %peer.json_rpc, "do_attestation placeholder (Phase 4b.4d follow-up)");
}
```

**Verdict:** The handler is a structural placeholder. It logs a debug message and does nothing else. No stamp exchange, no tick fetching, no ExternalAttestation storage.

### 1.3 Where DoAttestation Tasks Are Enqueued

**File:** `p2p/foretias-server/src/calendar/mod.rs:365`

Only one production enqueue site exists, in a test:
```rust
CalendarTask::DoAttestation { peer: peer.clone() }
```
This is inside `task_queue_accepts_all_variants_after_start` (test code, line 365).

**No production code enqueues `DoAttestation` tasks.** The `MutualAttestConfig` (peers, every_n_chronons) is stored in `CommunerdConfig` but nothing reads it to schedule `DoAttestation` tasks.

### 1.4 Where Mutual Attestation Actually Happens Today

The current mutual attestation flow is **manual**, not automatic:

1. **Chronomatter's `build_auto_attestation`** (`core-engine/src/chronomatter/mod.rs:192`) builds the auto-attestation blob (forward/backward signatures) during tick construction. This is LOCAL signing only — no peer communication.

2. **Integration tests** (`p2p/foretias-server/tests/integration.rs:247-259`) manually call `com.stamp_peer(&peer, ...)` to exchange stamps between nodes. This uses the deprecated `stamp_peer` path (direct PeerAddr transport, bypasses Take 3 gate).

3. **The `mutual_attest_observer`** on Chronomatter is purely for metrics callbacks (`on_mutual_attest_sent/ok/failed`). It does not drive peer communication.

**Summary:** Mutual attestation stamp exchange between peers is NOT automated. The `DoAttestation` task variant exists but is never enqueued by production code, and its handler does nothing.

---

## 2. Transport Paths: Direct PeerAddr vs CommunerdetteLine

### 2.1 Current Paths (Direct PeerAddr)

| Path | File | Status |
|------|------|--------|
| `Communerd::stamp_peer(peer, ...)` | `communerd/mod.rs:304-330` | `#[deprecated]`, bypasses Take 3 gate, uses `from_trusted` |
| `PeerTransport::stamp(peer, ...)` | `communerd/transport.rs:68` | Direct JSON-RPC over TCP |
| `Libp2pTransport::stamp(peer, ...)` | `communerd/libp2p_transport.rs` | Direct JSON-RPC over libp2p |
| `PeerPool::stamp(peer, ...)` | `communerd/peer_pool.rs:126` | Delegates to transport |

All these paths take `PeerAddr` as input and return bare `Foretis` (not `CleanAuthenticated<Foretis>`).

### 2.2 CommunerdetteLine Paths (Tbid-scoped)

| Method | File | Returns |
|--------|------|---------|
| `CommunerdetteLine::stamp(content, echo)` | `communerdette.rs:1792-1812` | `CleanAuthenticated<Foretis>` |
| `CommunerdetteLine::get_tick(tick_number)` | `communerdette.rs:1764-1783` | `CleanAuthenticated<ChrononRecord>` |
| `CommunerdetteLine::get_calendar_slice(start, count)` | `communerdette.rs:1738-1758` | `Vec<CleanAuthenticated<ChrononRecord>>` |
| `CommunerdetteLine::stamp_chronon(content, serialization, echo)` | `communerdette.rs:1862-1873` | `CleanAuthenticated<Foretis>` |

All these methods go through the Take 3 inbound gate and return `CleanAuthenticated<R>`.

### 2.3 MirrorDispatcher (Calendar's Current Network Interface)

**File:** `p2p/foretias-server/src/calendar/task_queue.rs:96-133`

The `MirrorDispatcher` trait has 5 methods, all mirror-specific:
- `known_peers()` → `Vec<PeerAddr>`
- `mirror_announce()` → `Result<bool, String>`
- `history_dump_chunk()` → `Result<u64, String>`
- `history_dump_complete()` → `Result<u64, String>`
- `mirror_health_check()` → `Result<u64, String>`

**No stamp or tick methods exist on `MirrorDispatcher`.** The trait was designed for mirror replication (priority 4-5), not mutual attestation (priority 3).

---

## 3. What Would Need to Change for CommunerdetteLine Integration

### 3.1 Gap 1: PeerAddr → Tbid Mapping (CRITICAL BLOCKER)

**Problem:** `DoAttestation { peer: PeerAddr }` carries a network address, but `CommunerdetteLine` requires a `Tbid`. There is no reverse mapping from `PeerAddr` → `Tbid` in the current codebase.

**Current state:**
- `MutualAttestConfig.peers` stores addresses as `Vec<String>` (e.g., "127.0.0.1:4002")
- `Communerd::tbid_index` maps `Tbid hex → PeerRegistrationRecord` (forward direction only)
- No data structure maps `PeerAddr → Tbid`

**Options to resolve:**
1. **Change `DoAttestation` to carry `Tbid` instead of `PeerAddr`:** `DoAttestation { target_tbid: Tbid }`. This requires changing `MutualAttestConfig.peers` from `Vec<String>` (addresses) to `Vec<String>` (TBID hex), or adding a parallel `mutual_attest_tbids` field.
2. **Add a reverse mapping:** Maintain `HashMap<String, Tbid>` in Communerd that maps json_rpc addresses to TBIDs. This requires populating it when peers register.
3. **DHT lookup at task execution time:** When `DoAttestation` runs, look up the peer's TBID via DHT. This adds latency and requires the peer to have registered.

**Recommendation:** Option 1 is cleanest. The mutual attestation config should store TBIDs (which is what you're attesting with), and the DHT/Communerdette resolves addresses at execution time.

### 3.2 Gap 2: Calendar Needs Access to CommunerdetteLine

**Problem:** The `WorkerContext` holds `Option<Arc<dyn MirrorDispatcher>>`. `MirrorDispatcher` has no stamp/tick methods. Calendar cannot call `CommunerdetteLine::stamp` through the current trait.

**Options to resolve:**
1. **Add stamp/tick methods to `MirrorDispatcher`:** Extend the trait with:
   ```rust
   async fn mutual_attest_stamp(&self, target_tbid: &str, content: Vec<u8>, echo: &str)
       -> Result<CleanAuthenticated<Foretis>, String>;
   async fn mutual_attest_get_tick(&self, target_tbid: &str, tick_number: u64)
       -> Result<CleanAuthenticated<ChrononRecord>, String>;
   ```
   Communerd implements these by delegating to `self.line_for_tbid(tbid).stamp(...)`.
2. **Give Calendar direct `Arc<Communerd>` access:** Violates the narrow-API invariant (Calendar should not reach into Communerd internals).
3. **Create a separate `AttestationDispatcher` trait:** Parallel to `MirrorDispatcher`, scoped to attestation operations.

**Recommendation:** Option 1. Extending `MirrorDispatcher` keeps Calendar decoupled from transport details. The trait name can stay (it's the dispatcher for all Calendar network calls) even if the methods cover attestation too.

### 3.3 Gap 3: Automatic Scheduling of DoAttestation Tasks

**Problem:** Nothing enqueues `DoAttestation` tasks based on `MutualAttestConfig.every_n_chronons`. The task variant exists but is never triggered.

**What's needed:**
- A scheduling mechanism that fires `DoAttestation` tasks at the configured cadence
- This could be: (a) a `TickObserver` callback on Calendar that enqueues tasks every N ticks, (b) a dedicated tokio task that polls the config and enqueues, or (c) integration with the existing daemon tick loop

**Recommendation:** Option (a) is most aligned with the priority system. Calendar already implements `TickObserver` (line 193 of `mod.rs`). Adding logic to `on_tick_advance` to enqueue `DoAttestation` tasks every N chronons keeps scheduling within Calendar's priority hierarchy.

### 3.4 Gap 4: ExternalAttestation Storage

**Problem:** `CoreCalendar::add_external_attestation()` exists (`core-engine/src/foretias/calendar.rs:190`) but is **never called** anywhere in the server code. When a mutual attestation stamp is received, the `Foretis` is returned to the caller but never stored as an `ExternalAttestation` record.

**What's needed:**
- After `CommunerdetteLine::stamp(...)` returns `CleanAuthenticated<Foretis>`, construct an `ExternalAttestation` from the verified data
- Call `CoreCalendar::add_external_attestation(chronon_number, att)` to persist it
- The `ExternalAttestation` struct needs: `attester_tbid`, `foretis`, `signature`, `signature_algorithm`, `attester_tick_record`, `received_at_ns`

**Recommendation:** The `DoAttestation` handler should:
1. Call `dispatcher.mutual_attest_stamp(target_tbid, content, echo)` → `CleanAuthenticated<Foretis>`
2. Call `dispatcher.mutual_attest_get_tick(target_tbid, foretis.chronon_number)` → `CleanAuthenticated<ChrononRecord>` (attester's tick record)
3. Construct `ExternalAttestation` from the verified data
4. Store via `calendar.add_external_attestation(foretis.chronon_number, att)`

---

## 4. Items 2 and 3: Already Satisfied or Need Work?

### Item 2: Store only verified `ExternalAttestation` records

**Status: NEEDS WORK (currently broken)**

- `add_external_attestation()` accepts raw `ExternalAttestation` with no verification gate
- It is never called, so the issue is theoretical but the API is unsafe
- For CommunerdetteLine integration, the `CleanAuthenticated<Foretis>` return type provides the verification guarantee
- The handler would need to construct `ExternalAttestation` from `CleanAuthenticated<Foretis>` + `CleanAuthenticated<ChrononRecord>` (attester tick)
- Consider adding a constructor like `ExternalAttestation::from_clean_authenticated(foretis: &CleanAuthenticated<Foretis>, attester_tick: &CleanAuthenticated<ChrononRecord>, received_at_ns: u64)` to enforce the invariant at construction time

### Item 3: Confirm receiver treats inbound mutual-attestation as ordinary `stamp` request

**Status: ALREADY SATISFIED**

- `handle_route_stamp` (`handlers.rs:78-133`) checks if `target_tbid == my_tbid_hex` (line 110)
- When they match, it delegates to `handle_stamp` (line 111): `return handle_stamp(server, params)`
- `handle_stamp` calls `cm.stamp(content, echo)` (line 61) — the same path as any local stamp
- The receiver does NOT distinguish mutual-attestation stamps from regular stamps
- This is correct behavior: mutual attestation IS just stamping the peer's tick content

---

## 5. Clear List of Blockers and Estimated Effort

### Blocker 1: PeerAddr → Tbid Mapping (HIGH effort)
- **Impact:** Cannot call `CommunerdetteLine` without a `Tbid`
- **Work:** Change `DoAttestation` to carry `Tbid`; update `MutualAttestConfig` or add TBID resolution; update all enqueue sites
- **Estimated effort:** 2-3 hours (config change + task variant change + enqueue site updates)
- **Risk:** Medium — config format change affects CLI and test fixtures

### Blocker 2: MirrorDispatcher Missing Methods (MEDIUM effort)
- **Impact:** Calendar cannot make stamp/tick calls through its dispatcher
- **Work:** Add 2 methods to `MirrorDispatcher` trait; implement on `Communerd` (delegate to `line_for_tbid`)
- **Estimated effort:** 1-2 hours
- **Risk:** Low — trait extension is additive, no breaking changes

### Blocker 3: DoAttestation Handler is Placeholder (MEDIUM effort)
- **Impact:** No actual attestation logic runs
- **Work:** Implement the handler: stamp → get_tick → construct ExternalAttestation → store
- **Estimated effort:** 2-3 hours
- **Risk:** Low — new code in existing handler slot

### Blocker 4: No Automatic Scheduling (MEDIUM effort)
- **Impact:** `DoAttestation` tasks are never enqueued
- **Work:** Add scheduling logic (TickObserver callback or dedicated task)
- **Estimated effort:** 1-2 hours
- **Risk:** Low — additive behavior

### Blocker 5: ExternalAttestation Construction from CleanAuthenticated (LOW effort)
- **Impact:** No safe constructor for verified attestations
- **Work:** Add `ExternalAttestation::from_clean_authenticated()` constructor
- **Estimated effort:** 30 min
- **Risk:** Low — new constructor, no changes to existing code

### Non-Blockers (Already Working)
- **Inbound stamp handling:** Receiver treats mutual-attestation as ordinary stamp (Item 3 satisfied)
- **CommunerdetteLine::stamp:** Returns `CleanAuthenticated<Foretis>` through Take 3 gate
- **CommunerdetteLine::get_tick:** Returns `CleanAuthenticated<ChrononRecord>` through Take 3 gate
- **CoreCalendar::add_external_attestation:** API exists, just needs callers

### Total Estimated Effort: 7-10 hours

### Recommended Implementation Order
1. Blocker 5 (ExternalAttestation constructor) — foundation, no dependencies
2. Blocker 2 (MirrorDispatcher methods) — enables Calendar to call through dispatcher
3. Blocker 1 (PeerAddr → Tbid mapping) — enables targeting the right peer
4. Blocker 3 (DoAttestation handler) — implements the actual logic
5. Blocker 4 (Automatic scheduling) — wires it all together

---

## 6. Cross-Reference: Related Code Locations

| Component | File | Lines |
|-----------|------|-------|
| DoAttestation task variant | `p2p/foretias-server/src/calendar/task_queue.rs` | 39, 70, 254, 481, 528 |
| DoAttestation handler (placeholder) | `p2p/foretias-server/src/calendar/task_queue.rs` | 254-259 |
| DoAttestation enqueue (test only) | `p2p/foretias-server/src/calendar/mod.rs` | 365 |
| MirrorDispatcher trait | `p2p/foretias-server/src/calendar/task_queue.rs` | 96-133 |
| MirrorDispatcher impl on Communerd | `p2p/foretias-server/src/communerd/mod.rs` | 1122+ |
| CommunerdetteLine::stamp | `p2p/foretias-server/src/communerd/communerdette.rs` | 1792-1812 |
| CommunerdetteLine::get_tick | `p2p/foretias-server/src/communerd/communerdette.rs` | 1764-1783 |
| ExternalAttestation struct | `p2p/core-engine/src/foretias/external_attestation.rs` | 10-25 |
| add_external_attestation (never called) | `p2p/core-engine/src/foretias/calendar.rs` | 190-205 |
| handle_route_stamp | `p2p/foretias-server/src/server/handlers.rs` | 78-133 |
| handle_stamp | `p2p/foretias-server/src/server/handlers.rs` | 31-76 |
| MutualAttestConfig | `p2p/core-engine/src/config/p2p.rs` | 7-19 |
| Chronomatter build_auto_attestation | `p2p/core-engine/src/chronomatter/mod.rs` | 192-208 |
| MutualAttestObserver trait | `p2p/core-engine/src/foretias/callbacks.rs` | 73-75 |
| Deprecated stamp_peer | `p2p/foretias-server/src/communerd/mod.rs` | 304-330 |
| route_stamp (via CommunerdetteLine) | `p2p/foretias-server/src/communerd/mod.rs` | 336-347 |
