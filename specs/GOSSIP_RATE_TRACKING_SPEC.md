# GOSSIP_RATE_TRACKING_SPEC.md

**Spec: Gossip Rate Tracking and Peer Alerting**
**Status: PROPOSED**
**Date: 2026-06-05**

---

## 1. Overview

Communerd/Communerdette currently tracks per-peer **success/failure counts** and **RTT** (see `CommunerdetteStats`), but has **no rate tracking** for incoming requests. A misbehaving or compromised peer could flood a time being with requests without detection or throttling.

This spec adds:
1. **Per-TBID request rate tracking** across multiple time windows
2. **Configurable thresholds** per window (requests/second, /minute, /hour, etc.)
3. **Gossip alerting** when a TBID exceeds thresholds — signed report broadcast to peers
4. **Receiving peer behavior** — configurable lower threshold for acting on alerts from others
5. **Local throttling** — reject requests from offending TBIDs

---

## 2. Use Cases

### Use Case 1: Flood Detection

**Situation:** Time being A starts sending 1000 stamp requests per second to time being B.

**What happens:**
1. B's Communerdette tracks A's request rate
2. A's rate exceeds B's configured threshold (e.g., 100/minute)
3. B throttles A's requests (reject with error)
4. B gossips an alert: "TBID-A is flooding me, here are the stats"
5. Other peers receive the alert and apply their own (lower) threshold to A

### Use Case 2: Slow Degradation

**Situation:** Time being C gradually increases request rate over hours — 10/min → 50/min → 200/min.

**What happens:**
1. C's rate stays below B's per-minute threshold but exceeds B's per-hour threshold
2. B detects the hourly rate violation
3. B gossips the alert with hourly stats
4. Other peers can see the trend in the stats

### Use Case 3: Gossip Alert Reception

**Situation:** Time being D receives a gossip alert about TBID-A from time being B.

**What happens:**
1. D verifies B's signature on the alert (B must be a known time being)
2. D checks: is A's rate against D also concerning? (D has its own, lower threshold)
3. If A's rate against D exceeds D's lower threshold → D starts throttling A
4. If A's rate against D is below D's threshold → D logs the alert but doesn't throttle
5. D may also increase its monitoring of A (more frequent rate checks)

### Use Case 4: False Positive / Dispute

**Situation:** Time being E receives a gossip alert about TBID-F, but F's rate against E is normal.

**What happens:**
1. E checks F's rate against E — it's below E's threshold
2. E logs the alert but does NOT throttle F
3. E may gossip a counter-report: "TBID-F's rate against me is normal"
4. The probity system can aggregate these conflicting reports

### Use Case 5: Self-Protection

**Situation:** Time being G is under attack from multiple TBIDs simultaneously.

**What happens:**
1. G detects rate violations from multiple TBIDs
2. G gossips alerts for each offending TBID
3. G applies local throttling to all offending TBIDs
4. G's probity score for each offending TBID decreases
5. Other peers see the alerts and may preemptively throttle the offending TBIDs

---

## 3. Design

### 3.1 Request Rate Tracker

Per-TBID, per-request-type rate tracking across multiple time windows.

```rust
/// Time windows for rate tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeWindow {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// Request types tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequestType {
    Stamp,
    Verify,
    GetTick,
    GetCalendarSlice,
    StartStream,
    MirrorAnnounce,
    HistoryDumpChunk,
    HistoryDumpComplete,
    MirrorHealthCheck,
    ChannelBind,
    AuthenticatedPing,
}

/// Per-TBID, per-request-type rate counter.
#[derive(Debug, Clone, Default)]
pub struct RateCounter {
    /// Sliding window counters: (window, count, window_start_ns)
    windows: HashMap<TimeWindow, WindowCounter>,
}

#[derive(Debug, Clone)]
struct WindowCounter {
    count: u64,
    window_start_ns: u64,
    /// Sub-second precision: timestamps of requests in current window
    timestamps: VecDeque<u64>,
}

/// Rate tracker: per-TBID counters.
#[derive(Debug)]
pub struct RateTracker {
    /// Per-TBID, per-request-type counters.
    counters: HashMap<String, HashMap<RequestType, RateCounter>>,
    /// Maximum number of TBIDs to track (LRU eviction).
    max_tbids: usize,
}
```

### 3.2 Rate Thresholds

Configurable thresholds per time window. When a TBID's request count in any window exceeds the threshold, the TBID is flagged.

```rust
/// Rate threshold configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateThresholds {
    /// Maximum requests per second per TBID.
    pub per_second: Option<u64>,
    /// Maximum requests per minute per TBID.
    pub per_minute: Option<u64>,
    /// Maximum requests per hour per TBID.
    pub per_hour: Option<u64>,
    /// Maximum requests per day per TBID.
    pub per_day: Option<u64>,
    /// Maximum requests per week per TBID.
    pub per_week: Option<u64>,
    /// Maximum requests per month per TBID.
    pub per_month: Option<u64>,
    /// Maximum requests per year per TBID.
    pub per_year: Option<u64>,
}

/// Rate tracking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateTrackingConfig {
    /// Enable rate tracking.
    pub enabled: bool,

    /// Thresholds for detecting offending TBIDs.
    pub detection_thresholds: RateThresholds,

    /// Thresholds for acting on gossip alerts from other peers (lower than detection).
    pub alert_thresholds: RateThresholds,

    /// Maximum number of TBIDs to track (LRU eviction).
    pub max_tbids: usize,

    /// Cooldown period (seconds) before a throttled TBID can retry.
    pub throttle_cooldown_secs: u64,

    /// How often (seconds) to recompute rates and check thresholds.
    pub check_interval_secs: u64,
}
```

### 3.3 Rate Alert (Gossip Message)

When a TBID exceeds detection thresholds, the reporter gossips a signed alert.

```rust
/// Rate alert gossip message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateAlertRecord {
    /// TBID of the offending peer (hex-encoded).
    pub offender_tbid: String,

    /// TBID of the reporting peer (hex-encoded).
    pub reporter_tbid: String,

    /// Timestamp of the alert.
    pub timestamp_ns: u64,

    /// Per-window request counts observed.
    pub observed_rates: HashMap<TimeWindow, u64>,

    /// Per-window thresholds that were exceeded.
    pub exceeded_thresholds: HashMap<TimeWindow, u64>,

    /// Request type breakdown (which types of requests were excessive).
    pub request_type_counts: HashMap<RequestType, u64>,

    /// Ed25519 signature over canonical() by reporter's TBID key.
    pub signature: Vec<u8>,
}

impl RateAlertRecord {
    /// Canonical bytes for signing (postcard-encoded, signature zeroed).
    pub fn canonical(&self) -> Vec<u8> { /* ... */ }

    /// Verify the alert signature.
    pub fn verify(&self, crypto: &dyn CryptoServer) -> Result<(), CleanAuthError> { /* ... */ }
}
```

### 3.4 Notification Strategy: Direct + Broadcast + Query API

Rate alerts use a **three-tier notification strategy**:

#### Tier 1: RTBID → Its Own Social Graph (direct, immediate)

RTBID transmits the alert directly to its own trusted peers:

```rust
/// Notify RTBID's social graph via direct request_response.
async fn notify_rtbid_peers(
    alert: &RateAlertRecord,
    communerdette_line: &CommunerdetteLine,
) {
    let social_graph = communerdette_line.get_active_relationships().await;

    for peer_tbid in social_graph {
        if let Ok(line) = communerdette_line.for_tbid(&peer_tbid) {
            line.send_rate_alert(alert).await;
        }
    }
}
```

**Social graph includes:**
- **FullyBound (FB) relationships** — peers that have completed mutual attestation
- **Recent helpers** — peers that provided stamp/verify/getCalendar in the recent past
- **Active mirror peers** — peers currently mirroring RTBID's calendar
- **DHT neighbors** — peers in RTBID's DHT neighborhood

#### Tier 2: GossipSub Broadcast About OTBID (eventual)

Broadcast the alert to the namespace-wide topic. The alert is **about OTBID** — any peer that receives it can check if they've interacted with OTBID:

```rust
// In gossip.rs
pub fn rate_alert_topic(namespace: &str) -> gossipsub::IdentTopic {
    gossipsub::IdentTopic::new(format!("/foretias/{}/rate-alert/v1", namespace))
}

pub fn publish_rate_alert(alert: &RateAlertRecord) -> Vec<u8> {
    serde_json::to_vec(alert).unwrap_or_default()
}
```

#### Tier 3: Query API (on-demand)

Peers can query other peers: "do you know about TBID-X being unruly?" This is a **request-response endpoint**, not gossip.

```rust
/// Query a peer for rate alerts about a specific TBID.
async fn query_rate_alerts(
    peer: &PeerAddr,
    target_tbid: &str,
) -> Result<Vec<RateAlertRecord>, TransportError> {
    // Send request: { "method": "query_rate_alerts", "params": { "tbid": target_tbid } }
    // Response: list of RateAlertRecord for that TBID (if any)
}
```

**When to use the query API:**
- Peer X encounters OTBID for the first time — queries its known peers for alerts
- Peer X receives a request from OTBID — queries before processing
- Peer X is about to mirror OTBID's calendar — queries for alerts first
- Periodic background check — query peers about TBIDs in the PeerPool

**Why this works:**
- Peers that interact with OTBID will naturally discover alerts when they query
- No need for RTBID to find OTBID's peers — they find the alerts themselves
- The query is cheap (single request_response) and can be cached locally

#### Notification Flow

```
RTBID detects OTBID exceeding thresholds
    │
    ├─→ Tier 1: RTBID's social graph → direct RPC (immediate)
    │   (FB, recent helpers, mirrors, DHT neighbors)
    │
    └─→ Tier 2: GossipSub broadcast about OTBID → all peers (eventual)
        (any peer can check if they've interacted with OTBID)

Meanwhile, other peers:
    │
    └─→ Tier 3: Query API → "do you know about OTBID?" (on-demand)
        (peers query when they encounter OTBID)
```

**Why not per-TBID GossipSub topics?**
Per-TBID topics don't scale. Each topic creates a separate mesh (D=6 peers), subscription messages broadcast to all connected peers, and with many TBIDs this is prohibitive.

**Why not DHT-based notification?**
DHT-based notification requires RTBID to find OTBID's peers, which is expensive (DHT queries) and may miss peers. The query API flips the direction: peers that encounter OTBID query for alerts, which is cheaper and more reliable.

### 3.5 Receiving Peer Behavior

When a peer receives a rate alert (via **direct RPC** or **GossipSub broadcast**):

1. **Verify signature** — check reporter's Ed25519 signature over canonical bytes
2. **Check reporter credibility** — reporter must have probity score > threshold (e.g., > -50.0)
3. **Check alert freshness** — timestamp must be within 5 minutes of current time
4. **Compare with local data** — check offender's rate against this peer using `alert_thresholds`
5. **Act or log:**
   - If offender's rate against this peer exceeds `alert_thresholds` → throttle offender
   - If offender's rate is below `alert_thresholds` → log alert, do not throttle
   - If no local data for offender → log alert, increase monitoring frequency

```rust
/// Handle incoming rate alert gossip message.
fn handle_rate_alert(
    alert: RateAlertRecord,
    crypto: &dyn CryptoServer,
    probity_store: &ProbityStore,
    rate_tracker: &RateTracker,
    config: &RateTrackingConfig,
) -> RateAlertAction {
    // 1. Verify signature
    alert.verify(crypto)?;

    // 2. Check reporter credibility
    let reporter_score = probity_store.get_score(&alert.reporter_tbid);
    if reporter_score < -50.0 {
        return RateAlertAction::Ignored("reporter score too low");
    }

    // 3. Check freshness
    let now_ns = clock.now_ns();
    if alert.timestamp_ns > now_ns + 5 * 60 * 1_000_000_000 {
        return RateAlertAction::Ignored("alert too far in the future");
    }

    // 4. Check local data for offender
    let local_rate = rate_tracker.get_rate(&alert.offender_tbid);

    // 5. Act or log
    if local_rate.exceeds_any(&config.alert_thresholds) {
        RateAlertAction::Throttle(alert.offender_tbid.clone())
    } else {
        RateAlertAction::Logged(alert.offender_tbid.clone())
    }
}
```

### 3.6 Local Throttling

When a TBID is flagged (either locally detected or via gossip alert):

1. **Reject requests** from that TBID with a specific error code
2. **Log the rejection** with reason (local detection vs gossip alert)
3. **Cooldown period** — after `throttle_cooldown_secs`, allow requests again
4. **Re-check** — if the TBID's rate drops below thresholds, remove throttle

```rust
/// Throttle state for a TBID.
#[derive(Debug, Clone)]
pub struct ThrottleState {
    /// TBID being throttled.
    pub tbid: String,
    /// When throttling started.
    pub started_ns: u64,
    /// Reason for throttling.
    pub reason: ThrottleReason,
    /// Number of requests rejected while throttled.
    pub rejected_count: u64,
}

#[derive(Debug, Clone)]
pub enum ThrottleReason {
    /// Local detection: rate exceeded detection thresholds.
    LocalDetection,
    /// Gossip alert: received alert from another peer.
    GossipAlert { reporter_tbid: String },
}
```

---

## 4. Communerdette as Unruliness Status Maintainer

**Communerdette** is the natural maintainer of unruliness status for each TBID. It already maintains per-TBID state (binding status, stats, backoff, route stats). Adding unruliness status extends this existing pattern.

### 4.1 Unruliness Status

```rust
/// Unruliness status for a TBID (maintained by Communerdette).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnrulinessStatus {
    /// Normal — no rate violations detected.
    Normal,
    /// Warning — rate approaching thresholds (e.g., >80% of detection threshold).
    Warning,
    /// Throttled — rate exceeded thresholds, requests being rejected.
    Throttled {
        /// When throttling started.
        started_ns: u64,
        /// Reason for throttling.
        reason: ThrottleReason,
        /// Number of requests rejected while throttled.
        rejected_count: u64,
    },
    /// Alerted — received gossip alert from another peer, monitoring closely.
    Alerted {
        /// TBID of the reporting peer.
        reporter_tbid: String,
        /// When the alert was received.
        alerted_at_ns: u64,
    },
}
```

### 4.2 Integration with CommunerdetteState

```rust
// In CommunerdetteState (existing per-TBID state)
pub struct CommunerdetteState {
    // ... existing fields (binding, stats, backoff, route_stats, etc.) ...

    /// Unruliness status for this TBID.
    pub unruliness: UnrulinessStatus,

    /// Rate tracker for inbound requests from this TBID.
    pub rate_tracker: RateCounter,

    /// Last time unruliness status was checked.
    pub last_unruliness_check_ns: u64,
}
```

### 4.3 CommunerdetteLine API

CommunerdetteLine (the narrow capability handle used by Calendar/Chronomatter) exposes unruliness queries:

```rust
impl CommunerdetteLine {
    /// Check if this TBID is unruly.
    pub fn is_unruly(&self) -> bool {
        matches!(self.state.unruliness, UnrulinessStatus::Throttled { .. })
    }

    /// Get the unruliness status for this TBID.
    pub fn unruliness_status(&self) -> UnrulinessStatus {
        self.state.unruliness.clone()
    }

    /// Record an inbound request (updates rate tracker).
    pub fn record_inbound_request(&self, request_type: RequestType) {
        self.state.rate_tracker.record(request_type, now_ns());
        self.check_unruliness_thresholds();
    }
}
```

### 4.4 Query API for Unruliness

The query API ("do you know about TBID-X being unruly?") reads from Communerdette:

```rust
/// Query handler: returns unruliness status for a TBID.
fn handle_query_unruliness(tbid: &str) -> Option<UnrulinessStatus> {
    // Look up Communerdette for this TBID
    communerd.line_for_tbid(tbid).ok().map(|line| line.unruliness_status())
}
```

### 4.5 Rate Tracking Integration

Communerdette already tracks per-TBID stats (success/failure counts, RTT). Rate tracking extends this:

```rust
// In Communerdette::handle_inbound_request()
fn handle_inbound_request(&self, tbid: &str, request_type: RequestType) {
    // Existing: update stats
    self.record_route_success(route, now_ns);

    // New: update rate tracker
    if let Ok(line) = self.line_for_tbid(tbid) {
        line.record_inbound_request(request_type);
    }

    // New: check unruliness
    if let Ok(line) = self.line_for_tbid(tbid) {
        if line.is_unruly() {
            // Reject request with error code
            return Err(TransportError::RateLimited(tbid.to_string()));
        }
    }
}
```

---

## 5. Implementation Notes

### 4.1 Sliding Window Counters

For efficient rate tracking, use sliding window counters:

```rust
impl WindowCounter {
    fn record(&mut self, now_ns: u64, window_duration_ns: u64) {
        // Remove timestamps outside the window
        while let Some(&front) = self.timestamps.front() {
            if now_ns - front > window_duration_ns {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
        self.timestamps.push_back(now_ns);
        self.count = self.timestamps.len() as u64;
    }

    fn count(&self) -> u64 {
        self.count
    }
}
```

### 4.2 Memory Management

With `max_tbids` limit (e.g., 10,000), use LRU eviction for TBIDs not seen recently. Each TBID's counters take ~1KB, so 10,000 TBIDs ≈ 10MB.

### 4.3 Integration with Existing Infrastructure

- **Communerdette** already has per-TBID state — add `RateTracker` alongside `CommunerdetteState`
- **ProbityReport** pattern — use the same signing/verification approach for `RateAlertRecord`
- **GossipSub** — new topic, same publish/subscribe pattern as probity

---

## 5. Scope

### In Scope

- `RateTracker` with sliding window counters
- `RateThresholds` configuration
- `RateAlertRecord` gossip message (signing, verification, topic)
- Receiving peer behavior (verify, compare, act/log)
- Local throttling with cooldown
- Integration test: rate detection → gossip alert → peer throttling

### Out of Scope

- Probity score integration (future: rate alerts affect probity)
- Distributed rate aggregation (each peer tracks independently)
- Certificate-based authentication for alerts (TBID stamps sufficient)
- Rate tracking for outbound requests (only inbound)

---

## 6. Implementation TODO

- [ ] Write `RateTracker` with sliding window counters
- [ ] Write `RateThresholds` and `RateTrackingConfig` structs
- [ ] Write `RateAlertRecord` with `canonical()` and `verify()`
- [ ] Add `/foretias/{namespace}/rate-alert/v1` gossip topic
- [ ] Implement rate tracking in Communerdette (record inbound requests)
- [ ] Implement threshold checking (detection thresholds)
- [ ] Implement gossip alert publishing (sign + publish when threshold exceeded)
- [ ] Implement alert reception handler (verify, compare, act/log)
- [ ] Implement local throttling (reject requests from flagged TBIDs)
- [ ] Add integration test: flood detection → gossip alert → peer throttling
- [ ] Add integration test: false positive (alert received, local rate normal)
- [ ] Document configuration and tuning guide
