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

### 3.4 Notification Strategy: Two-Pronged + Broadcast

Rate alerts use a **three-tier notification strategy** that prioritizes peers most likely to be affected by the offending TBID (OTBID):

#### Tier 1: RTBID → OTBID's Peers (DHT-based, immediate)

OTBID's peers are those that OTBID can easily find on the DHT. These peers are likely interacting with OTBID and should be notified first.

```rust
/// Notify OTBID's peers via DHT lookup + direct request_response.
async fn notify_otbid_peers(
    alert: &RateAlertRecord,
    communerd: &Communerd,
    otbid: &str,
) {
    // 1. Query DHT for OTBID's registration record
    //    Key: /foretias/{namespace}/tbid/{otbid_hex}/v1
    if let Some(record) = communerd.lookup_tbid(otbid).await {
        // 2. Find peers near OTBID's DHT key (Kademlia XOR distance)
        let nearby_peers = communerd.get_closest_peers(&record.peer_id).await;

        // 3. Send rate alert directly to each peer via request_response
        for peer in nearby_peers {
            communerd.send_rate_alert(&peer, alert).await;
        }
    }
}
```

**Why this works:** Peers near OTBID's DHT key are likely interacting with OTBID (they're in OTBID's "neighborhood" on the Kademlia XOR distance metric). They receive the alert immediately via direct RPC, before the GossipSub broadcast reaches them.

#### Tier 2: RTBID → Its Own Social Graph (direct)

RTBID transmits the alert to its own trusted peers — TBIDs that RTBID has recently interacted with:

```rust
/// Notify RTBID's social graph via direct request_response.
async fn notify_rtbid_peers(
    alert: &RateAlertRecord,
    communerdette_line: &CommunerdetteLine,
) {
    // Get TBIDs that RTBID has recently interacted with
    let social_graph = communerdette_line.get_active_relationships().await;

    for peer_tbid in social_graph {
        // Send rate alert directly via CommunerdetteLine
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

#### Tier 3: GossipSub Broadcast (eventual)

Also publish to the namespace-wide topic for all other peers:

```rust
// In gossip.rs
pub fn rate_alert_topic(namespace: &str) -> gossipsub::IdentTopic {
    gossipsub::IdentTopic::new(format!("/foretias/{}/rate-alert/v1", namespace))
}

pub fn publish_rate_alert(alert: &RateAlertRecord) -> Vec<u8> {
    serde_json::to_vec(alert).unwrap_or_default()
}
```

#### Notification Flow

```
RTBID detects OTBID exceeding thresholds
    │
    ├─→ Tier 1: DHT lookup for OTBID's peers → direct RPC (immediate)
    │
    ├─→ Tier 2: RTBID's social graph → direct RPC (immediate)
    │
    └─→ Tier 3: GossipSub broadcast → all peers (eventual, ~100-500ms)
```

**Why not per-TBID GossipSub topics?**
Per-TBID topics don't scale. Each topic creates a separate mesh (D=6 peers), subscription messages broadcast to all connected peers, and with many TBIDs this is prohibitive. The two-pronged approach achieves targeted notification without topic overhead.

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

## 4. Implementation Notes

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
