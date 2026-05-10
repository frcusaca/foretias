# CALENDAR REPLICATION SPEC (v0.1)

**Status**: Draft — pending implementation
**Date**: 2026-05-05
**Depends on**: DHT TBID index (PEERING_V1_PLAN.md), cross-node verify (handlers.rs)

---

## 1. Overview

The calendar replication service enables Time Beings to share calendar data (TickRecords) across peers. Three distinct features compose this service:

1. **Time Family Stamp Routing** — Ask any peer in the time family to stamp content; use DHT to locate the owner when calendar data is missing.
2. **One-Way Calendar Mirror** — Request another calendar to ship and continuously mirror its TickRecords.
3. **Mutual Calendar Mirroring** — Two calendars actively maintain mirrored copies of each other's data with bidirectional correctness guarantees.

All replication uses **point-to-point JSON-RPC connections** over the existing `PeerTransport` layer. No new transport protocol is introduced.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Source Calendar** | The calendar whose data is being mirrored (the "original") |
| **Mirror Calendar** | The calendar that stores a replicated copy |
| **Mirrored TBID** | A TBID whose calendar data is stored as a mirror (not the mirror calendar's own TBID) |
| **Tick Count** | Total number of TickRecords in a calendar |
| **Hash Sanity** | Fast completeness check: SHA-256 of concatenated tick numbers |
| **Catchup** | Initial bulk transfer to bring a mirror up to date |
| **Stream** | Continuous delta delivery of new ticks after catchup |

---

## 3. Feature 1 — Time Family Stamp Routing

### 3.1 Problem

A Time Being wants to stamp content but may not have the target TBID's calendar data locally. Currently, `handle_verify` with `cross_node: true` solves verification, but stamp routing is symmetric — if node A asks node B to stamp, node B stamps with its **own** TBID. We need node B to be able to **forward** a stamp request to the correct owner (node C who owns TBID X).

### 3.2 Design

**New RPC method**: `route_stamp`

```json
{
  "method": "route_stamp",
  "params": {
    "target_tbid": "<hex>",
    "content_hex": "<hex>",
    "echo": "<string>"
  }
}
```

**Behavior on receipt**:

1. If `target_tbid == my_tbid` → stamp locally, return Foretis
2. If `target_tbid` found in DHT TBID index → forward `route_stamp` to owner, return result
3. If `target_tbid` not in DHT → return error `{"error": "tbid_not_found"}`

**Response**:
```json
{
  "result": {
    "foretis": { ... },
    "routed": true | false
  }
}
```

### 3.3 Implementation

| Component | Changes |
|-----------|---------|
| `server/handlers.rs` | New `handle_route_stamp()` handler |
| `communerd/mod.rs` | `route_stamp(&self, target_tbid, content_hex, echo) -> Result<Foretis, NodeError>` |
| `transport.rs` | `async fn route_stamp(&self, peer, target_tbid, content_hex, echo) -> Result<serde_json::Value, TransportError>` |
| `main.rs` | CLI: `foretias stamp --route <tbid> <content>` |

---

## 4. Feature 2 — One-Way Calendar Mirror Protocol

### 4.1 Problem

A calendar wants to obtain a complete copy of another calendar's TickRecords for local caching (accelerated verification, redundancy).

### 4.2 Mirror Acceptance Policy

**Accept criteria** (configurable):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_mirrored_tbids` | 64 | Maximum distinct TBIDs this calendar will mirror |
| `max_mirror_ticks_per_tbid` | 100000 | Maximum ticks stored per mirrored TBID |

A mirror request is accepted if:
- `mirrored_tbids.len() < max_mirrored_tbids`
- Total mirrored ticks + requested calendar tick count < some global cap

**Reject otherwise** with `{"error": "mirror_limit_exceeded"}`.

### 4.3 Protocol Phases

#### Phase 1: Mirror Request & Acceptance

```
Mirror → Source: mirror_request { tbid, my_addr }
Source → Mirror: mirror_accept { tick_count, latest_tick, hash_sanity }
   OR
Source → Mirror: mirror_reject { reason }
```

**`hash_sanity` computation**:
```
SHA-256( tick_number_1 || tick_number_2 || ... || tick_number_n )
```
where `||` is byte concatenation of each tick_number as big-endian u64.

This allows the mirror to verify completeness after catchup without fetching every tick's full signature.

#### Phase 2: Initial Ship (Catchup)

Source ships the full calendar in batches:

```
Source → Mirror: ship_batch { start_tick, records: [TickRecord] }
   ... (repeat until complete)
Mirror → Source: ship_ack { confirmed_up_to_tick }
```

**Batch size**: Configurable, default 1000 TickRecords per batch.

**Verification during catchup**:
- Mirror verifies `verify_pair()` on each consecutive tick pair from the source calendar
- Mirror computes its own `hash_sanity` on received tick numbers
- After all batches received, mirror compares its `hash_sanity` against the one from `mirror_accept`
- **Mismatch → abort mirror, log `ERROR:hash_sanity_mismatch`**

#### Phase 3: Stream Mode (Continuous)

Once catchup completes, source pushes new ticks as they arrive:

```
Source → Mirror: stream_tick { tick_record: TickRecord }
Mirror → Source: stream_ack { tick_number }
```

- Uses the existing point-to-point connection (no new connection needed)
- Source sends `stream_tick` whenever Chronomatter produces a new tick
- Mirror stores tick, verifies pair, acks
- **If ack not received within 5s → source retries once, then logs `ERROR:stream_acked_timeout`**

### 4.4 New RPC Methods

| Method | Direction | Description |
|--------|-----------|-------------|
| `mirror_request` | Mirror → Source | Request to mirror a calendar |
| `mirror_accept` | Source → Mirror | Accept with tick_count, latest_tick, hash_sanity |
| `mirror_reject` | Source → Mirror | Reject with reason |
| `ship_batch` | Source → Mirror | Batch of TickRecords for catchup |
| `ship_ack` | Mirror → Source | Acknowledge received batch |
| `stream_tick` | Source → Mirror | Single new tick (stream phase) |
| `stream_ack` | Mirror → Source | Acknowledge streamed tick |

### 4.5 Data Storage

Mirrored calendars stored separately from the local calendar:

```
~/.foretias/mirrors/{tbid_hex}/calendar.json
```

Each mirrored calendar has its own JSONL file. The mirror calendar's `CalendarLookup` implementation supports querying mirrored calendars by TBID prefix.

**New trait extension**:
```rust
pub trait MirrorStore: Send + Sync {
    fn get_mirrored(&self, tbid: &[u8; 16], tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError>;
    fn insert_mirrored(&self, tbid: &[u8; 16], record: TickRecord) -> Result<(), NodeError>;
    fn list_mirrored_tbids(&self) -> Vec<[u8; 16]>;
    fn mirror_tick_count(&self, tbid: &[u8; 16]) -> u64;
}
```

---

## 5. Feature 3 — Mutual Calendar Mirroring

### 5.1 Problem

Two calendars want to mirror each other simultaneously. Both act as source and mirror for the other.

### 5.2 Protocol

Mutual mirroring is simply two one-way mirrors established atomically:

```
A → B: mirror_request { tbid: B_tbid, my_addr: A_addr }
B → A: mirror_request { tbid: A_tbid, my_addr: B_addr }
```

Both sides independently go through accept → catchup → stream.

**New RPC method**: `mirror_mutual`

```json
{
  "method": "mirror_mutual",
  "params": {
    "peer_addr": "<host:port>",
    "my_tbid": "<hex>",
    "peer_tbid": "<hex>"
  }
}
```

This is a convenience wrapper that initiates both one-way mirrors in a single round-trip.

### 5.3 Active Correctness Maintenance

Both sides maintain correctness through periodic reconciliation:

| Check | Interval | Action on Mismatch |
|-------|----------|-------------------|
| Tick count comparison | Every 60s | Request missing range |
| Hash sanity spot-check | Every 300s | Full re-catchup from divergence point |
| Random tick verification | Every 600s (1 random tick) | Re-fetch and verify_pair |

**Reconciliation RPC**:
```json
{
  "method": "mirror_reconcile",
  "params": {
    "tbid": "<hex>",
    "my_tick_count": 12345,
    "my_latest_tick": 12345,
    "my_hash_sanity": "<hex>"
  }
}
```

Response from peer:
```json
{
  "result": {
    "status": "in_sync" | "behind" | "ahead" | "diverged",
    "divergence_tick": <u64 or null>,
    "peer_tick_count": <u64>,
    "peer_latest_tick": <u64>
  }
}
```

**Reconciliation outcomes**:
- `in_sync` → no action
- `behind` → stream missing ticks
- `ahead` → peer will fetch from me
- `diverged` → full re-catchup from `divergence_tick`

---

## 6. Transport Layer

### 6.1 Connection Management

| Direction | Connection Type | Purpose |
|-----------|----------------|---------|
| Mirror → Source | Persistent TCP JSON-RPC | Request, catchup, stream reception, reconciliation |
| Source → Mirror | Same connection (bidirectional) | Ship batches, stream ticks |

The same JSON-RPC connection handles all phases. Connection established on `mirror_request`, maintained throughout mirror lifetime.

### 6.2 PeerTransport Trait Extensions

Add to `transport.rs`:

```rust
#[async_trait]
pub trait PeerTransport: Send + Sync {
    // ... existing methods ...

    async fn mirror_request(&self, peer: &PeerAddr, tbid: &str, my_addr: &str) -> Result<serde_json::Value, TransportError>;
    async fn route_stamp(&self, peer: &PeerAddr, target_tbid: &str, content_hex: &str, echo: &str) -> Result<serde_json::Value, TransportError>;
}
```

---

## 7. Logging

All replication activity writes to per-peer log files:

```
~/.foretias/logs/{peer_id}/replication.log
```

**Log format** (line-delimited JSON):

```json
{"ts": "<ISO8601>", "level": "INFO|ERROR|WARN", "event": "mirror_request_sent", "tbid": "<hex>", "peer": "<addr>"}
{"ts": "<ISO8601>", "level": "ERROR", "event": "hash_sanity_mismatch", "expected": "<hex>", "got": "<hex>", "tbid": "<hex>"}
{"ts": "<ISO8601>", "level": "ERROR", "event": "connection_timeout", "peer": "<addr>", "phase": "catchup"}
{"ts": "<ISO8601>", "level": "ERROR", "event": "verify_mismatch", "tbid": "<hex>", "tick": 12345}
{"ts": "<ISO8601>", "level": "ERROR", "event": "tbid_not_found_in_dht", "tbid": "<hex>"}
{"ts": "<ISO8601>", "level": "ERROR", "event": "verify_incorrect_answer", "expected": true, "got": false, "tbid": "<hex>"}
```

**ERROR prefix convention**: Every error line begins with `ERROR:` for easy grep:

```bash
grep "ERROR:" ~/.foretias/logs/*/replication.log | sort | uniq -c
```

**Events that generate ERROR lines**:
- Connection timeout during catchup or stream
- Hash sanity mismatch after catchup
- `verify_pair` failure on received tick
- DHT lookup returns no result for target TBID
- Verify returns unexpected result
- Mirror rejection (policy)
- Stream ack timeout
- Reconciliation divergence detected

---

## 8. Configuration

New config section in `config.rs`:

```rust
pub struct ReplicationConfig {
    pub max_mirrored_tbids: usize,         // default: 64
    pub max_mirror_ticks_per_tbid: u64,    // default: 100000
    pub ship_batch_size: usize,            // default: 1000
    pub stream_ack_timeout_ms: u64,        // default: 5000
    pub reconcile_interval_s: u64,         // default: 60
    pub hash_sanity_interval_s: u64,       // default: 300
    pub random_verify_interval_s: u64,     // default: 600
}
```

---

## 9. Error Handling

| Error | Recovery |
|-------|----------|
| Connection drops during catchup | Retry from last `ship_ack` |
| Connection drops during stream | Reconnect, request delta since last `stream_ack` |
| Hash sanity mismatch | Abort mirror, remove mirrored data, log ERROR |
| `verify_pair` failure on received tick | Reject tick, request re-send, log ERROR |
| Peer unreachable for 5 reconciliation cycles | Mark mirror stale, log ERROR, do not auto-retry |
| Mirror limit exceeded | Reject request, return `mirror_reject` |

---

## 10. Implementation Order

1. **MirrorStore trait + mirrored calendar storage** — new module `calendar/mirror.rs`
2. **One-way mirror protocol** — `mirror_request` → `ship_batch` → `stream_tick` handlers
3. **Hash sanity computation** — utility function in `foretias/tick.rs`
4. **Mutual mirror** — `mirror_mutual` convenience wrapper
5. **Reconciliation loop** — background task in `communerd/mod.rs`
6. **Route stamp** — `route_stamp` handler in `server/handlers.rs`
7. **Logging infrastructure** — per-peer log files in `communerd/mod.rs`

---

## 11. Stress Test Integration

Update `dht_stress_test.sh` to:

1. Start N peers with DHT discovery
2. For each round:
   - Pick random peer A, ask it to mirror a random peer B's calendar
   - Wait for catchup to complete
   - Pick random peer C, ask it to stamp via route (targeting A's TBID)
   - Pick random peer D, ask it to verify the stamp (cross-node)
   - Check log files for ERROR lines
3. Run reconciliation round on all mirror pairs
4. Report success/failure counts and ERROR summary
