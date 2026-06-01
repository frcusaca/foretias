//! ProbityStore — ingests, deduplicates, and aggregates probity reports.

use foretias_core::error::NodeError;
use parking_lot::RwLock;
use std::collections::HashMap;

use super::aggregator::{aggregate, UShapeConfig};
use super::report::ProbityReport;

pub struct ProbityStore {
    reports: RwLock<HashMap<String, Vec<ProbityReport>>>,
    scores: RwLock<HashMap<String, f32>>,
    config: UShapeConfig,
    max_reports_per_peer: usize,
}

impl ProbityStore {
    pub fn new() -> Self {
        Self::with_config(UShapeConfig::default(), 500)
    }

    pub fn with_config(config: UShapeConfig, max_reports_per_peer: usize) -> Self {
        Self {
            reports: RwLock::new(HashMap::new()),
            scores: RwLock::new(HashMap::new()),
            config,
            max_reports_per_peer,
        }
    }

    /// Ingest a verified report. Caller must have verified the signature already.
    pub fn ingest(&self, report: ProbityReport) -> Result<(), NodeError> {
        let mut guard = self.reports.write();
        let list = guard.entry(report.subject.clone()).or_default();
        let dup = list.iter().any(|r| {
            r.reporter == report.reporter
                && r.attribute == report.attribute
                && r.timestamp_ns == report.timestamp_ns
        });
        if dup {
            return Ok(());
        }
        list.push(report);
        if list.len() > self.max_reports_per_peer {
            list.sort_by_key(|r| r.timestamp_ns);
            let excess = list.len() - self.max_reports_per_peer;
            list.drain(0..excess);
        }
        Ok(())
    }

    /// Recompute all scores. Call on a 60 s timer.
    pub fn recompute_all(&self, now_ns: u64) {
        let reports = self.reports.read();

        let cred1 = |_: &str| 1.0f32;
        let pass1: HashMap<String, f32> = reports
            .iter()
            .map(|(subj, reps)| (subj.clone(), aggregate(reps, now_ns, &cred1, &self.config)))
            .collect();

        let cred2 =
            |peer: &str| -> f32 { pass1.get(peer).copied().unwrap_or(0.0).max(0.0) / 100.0 };

        let mut scores = self.scores.write();
        scores.clear();
        for (subj, reps) in reports.iter() {
            scores.insert(subj.clone(), aggregate(reps, now_ns, &cred2, &self.config));
        }
    }

    pub fn score(&self, peer_id: &str) -> f32 {
        self.scores.read().get(peer_id).copied().unwrap_or(0.0)
    }

    /// Return the number of reports for a given subject.
    pub fn report_count(&self, peer_id: &str) -> usize {
        self.reports
            .read()
            .get(peer_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report(subject: &str, reporter: &str, ts: u64) -> ProbityReport {
        ProbityReport {
            subject: subject.to_string(),
            reporter: reporter.to_string(),
            attribute: "correctness".to_string(),
            value: -10.0,
            timestamp_ns: ts,
            signature: vec![],
            curve: 1,
            slow_signature: vec![],
        }
    }

    #[test]
    fn probity_store_ingest_deduplicates() {
        let store = ProbityStore::new();
        let r = make_report("A", "B", 1000);
        store.ingest(r.clone()).unwrap();
        store.ingest(r.clone()).unwrap();
        assert_eq!(store.report_count("A"), 1);
    }

    #[test]
    fn probity_store_evicts_oldest_over_cap() {
        let store = ProbityStore::with_config(UShapeConfig::default(), 3);
        for i in 0..5 {
            store
                .ingest(make_report("A", &format!("R{}", i), i))
                .unwrap();
        }
        assert_eq!(store.report_count("A"), 3);
        // Oldest two should have been evicted
        let reports = store.reports.read();
        let list = reports.get("A").unwrap();
        assert!(list.iter().all(|r| r.timestamp_ns >= 2));
    }

    #[test]
    fn probity_store_recompute_two_pass() {
        let store = ProbityStore::new();
        let now = 1_000_000_000_000;

        // V↔W mutual vouching gives both positive pass-1 scores → credibility in pass 2
        store
            .ingest(ProbityReport {
                subject: "W".into(),
                reporter: "V".into(),
                attribute: "correctness".into(),
                value: 90.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();
        store
            .ingest(ProbityReport {
                subject: "V".into(),
                reporter: "W".into(),
                attribute: "correctness".into(),
                value: 80.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();

        // W→Z→X chain gives Z and X positive pass-1 scores
        store
            .ingest(ProbityReport {
                subject: "Z".into(),
                reporter: "W".into(),
                attribute: "correctness".into(),
                value: 90.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();
        store
            .ingest(ProbityReport {
                subject: "X".into(),
                reporter: "Z".into(),
                attribute: "correctness".into(),
                value: 50.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();

        // X gives strong good report on A (+50), Y gives weak bad report (-20)
        // A pass-1 score = +50 - 20 = +30 → A has credibility in pass 2
        store
            .ingest(ProbityReport {
                subject: "A".into(),
                reporter: "X".into(),
                attribute: "correctness".into(),
                value: 50.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();
        store
            .ingest(ProbityReport {
                subject: "A".into(),
                reporter: "Y".into(),
                attribute: "correctness".into(),
                value: -20.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();

        // A reports badly on Y → Y gets negative pass-1 score
        // In pass 2: Y credibility = 0 (negative pass-1), A's bad report carries weight
        store
            .ingest(ProbityReport {
                subject: "Y".into(),
                reporter: "A".into(),
                attribute: "correctness".into(),
                value: -100.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .unwrap();

        store.recompute_all(now);

        // Pass 1: V=80, W=90, Z=90, X=50, A=30, Y=-100
        // Pass 2 creds: V=0.8, W=0.9, Z=0.9, X=0.5, A=0.3, Y=0.0
        // Y pass-2 = A_cred × (-100) = 0.3 × (-100) = -30
        let score_y = store.score("Y");
        assert!(
            score_y < 0.0,
            "Y should have negative score, got {}",
            score_y
        );

        // Y's pass-2 credibility is 0, so Y's report on A contributes nothing
        // A pass-2 = X_cred × 50 + 0 × (-20) = 0.5 × 50 = 25
        let score_a = store.score("A");
        assert!(
            score_a > 0.0,
            "A should have positive score, got {}",
            score_a
        );
    }

    #[test]
    fn probity_store_score_unknown_returns_zero() {
        let store = ProbityStore::new();
        assert_eq!(store.score("unknown"), 0.0);
    }
}
