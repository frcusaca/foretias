use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy)]
pub enum MetricField {
    StampsTotal,
    StampsRateLimited,
    AutoAttestSent,
    AutoAttestOk,
    AutoAttestFailed,
    CalendarFlushCount,
    HeartbeatsSent,
    HeartbeatsReceived,
    CollisionsDetected,
    DormantTransitions,
}

pub struct NodeMetrics {
    pub stamps_total: AtomicU64,
    pub stamps_rate_limited: AtomicU64,
    pub auto_attest_sent: AtomicU64,
    pub auto_attest_ok: AtomicU64,
    pub auto_attest_failed: AtomicU64,
    pub calendar_flush_count: AtomicU64,
    pub heartbeats_sent: AtomicU64,
    pub heartbeats_received: AtomicU64,
    pub collisions_detected: AtomicU64,
    pub dormant_transitions: AtomicU64,
}

impl NodeMetrics {
    pub fn new() -> Self {
        Self {
            stamps_total: AtomicU64::new(0),
            stamps_rate_limited: AtomicU64::new(0),
            auto_attest_sent: AtomicU64::new(0),
            auto_attest_ok: AtomicU64::new(0),
            auto_attest_failed: AtomicU64::new(0),
            calendar_flush_count: AtomicU64::new(0),
            heartbeats_sent: AtomicU64::new(0),
            heartbeats_received: AtomicU64::new(0),
            collisions_detected: AtomicU64::new(0),
            dormant_transitions: AtomicU64::new(0),
        }
    }

    pub fn inc(&self, field: MetricField) {
        match field {
            MetricField::StampsTotal => {
                self.stamps_total.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::StampsRateLimited => {
                self.stamps_rate_limited.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::AutoAttestSent => {
                self.auto_attest_sent.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::AutoAttestOk => {
                self.auto_attest_ok.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::AutoAttestFailed => {
                self.auto_attest_failed.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::CalendarFlushCount => {
                self.calendar_flush_count.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::HeartbeatsSent => {
                self.heartbeats_sent.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::HeartbeatsReceived => {
                self.heartbeats_received.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::CollisionsDetected => {
                self.collisions_detected.fetch_add(1, Ordering::Relaxed);
            }
            MetricField::DormantTransitions => {
                self.dormant_transitions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "stamps_total":        self.stamps_total.load(Ordering::Relaxed),
            "stamps_rate_limited": self.stamps_rate_limited.load(Ordering::Relaxed),
            "auto_attest_sent":    self.auto_attest_sent.load(Ordering::Relaxed),
            "auto_attest_ok":      self.auto_attest_ok.load(Ordering::Relaxed),
            "auto_attest_failed":  self.auto_attest_failed.load(Ordering::Relaxed),
            "calendar_flush_count": self.calendar_flush_count.load(Ordering::Relaxed),
            "heartbeats_sent":     self.heartbeats_sent.load(Ordering::Relaxed),
            "heartbeats_received": self.heartbeats_received.load(Ordering::Relaxed),
            "collisions_detected": self.collisions_detected.load(Ordering::Relaxed),
            "dormant_transitions": self.dormant_transitions.load(Ordering::Relaxed),
        })
    }
}

impl foretias_core::foretias::callbacks::AutoAttestObserver for NodeMetrics {
    fn on_auto_attest_sent(&self) {
        self.inc(MetricField::AutoAttestSent);
    }
    fn on_auto_attest_ok(&self) {
        self.inc(MetricField::AutoAttestOk);
    }
    fn on_auto_attest_failed(&self) {
        self.inc(MetricField::AutoAttestFailed);
    }
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_metrics_are_zero() {
        let m = NodeMetrics::new();
        assert_eq!(m.stamps_total.load(Ordering::Relaxed), 0);
        assert_eq!(m.stamps_rate_limited.load(Ordering::Relaxed), 0);
        assert_eq!(m.auto_attest_sent.load(Ordering::Relaxed), 0);
        assert_eq!(m.auto_attest_ok.load(Ordering::Relaxed), 0);
        assert_eq!(m.auto_attest_failed.load(Ordering::Relaxed), 0);
        assert_eq!(m.calendar_flush_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.heartbeats_sent.load(Ordering::Relaxed), 0);
        assert_eq!(m.heartbeats_received.load(Ordering::Relaxed), 0);
        assert_eq!(m.collisions_detected.load(Ordering::Relaxed), 0);
        assert_eq!(m.dormant_transitions.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn inc_increments_correct_field() {
        let m = NodeMetrics::new();
        m.inc(MetricField::StampsTotal);
        m.inc(MetricField::StampsTotal);
        m.inc(MetricField::AutoAttestSent);
        m.inc(MetricField::AutoAttestOk);
        m.inc(MetricField::AutoAttestFailed);
        m.inc(MetricField::CalendarFlushCount);
        m.inc(MetricField::HeartbeatsSent);
        m.inc(MetricField::HeartbeatsReceived);
        m.inc(MetricField::CollisionsDetected);
        m.inc(MetricField::DormantTransitions);
        assert_eq!(m.stamps_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.auto_attest_sent.load(Ordering::Relaxed), 1);
        assert_eq!(m.auto_attest_ok.load(Ordering::Relaxed), 1);
        assert_eq!(m.auto_attest_failed.load(Ordering::Relaxed), 1);
        assert_eq!(m.calendar_flush_count.load(Ordering::Relaxed), 1);
        assert_eq!(m.heartbeats_sent.load(Ordering::Relaxed), 1);
        assert_eq!(m.heartbeats_received.load(Ordering::Relaxed), 1);
        assert_eq!(m.collisions_detected.load(Ordering::Relaxed), 1);
        assert_eq!(m.dormant_transitions.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn as_json_contains_all_fields() {
        let m = NodeMetrics::new();
        m.inc(MetricField::StampsTotal);
        m.inc(MetricField::CalendarFlushCount);
        m.inc(MetricField::CollisionsDetected);
        let json = m.as_json();
        assert_eq!(json["stamps_total"], 1);
        assert_eq!(json["calendar_flush_count"], 1);
        assert_eq!(json["auto_attest_sent"], 0);
        assert!(json.get("stamps_rate_limited").is_some());
        assert!(json.get("auto_attest_ok").is_some());
        assert!(json.get("auto_attest_failed").is_some());
        assert!(json.get("heartbeats_sent").is_some());
        assert!(json.get("heartbeats_received").is_some());
        assert!(json.get("collisions_detected").is_some());
        assert!(json.get("dormant_transitions").is_some());
    }
}
