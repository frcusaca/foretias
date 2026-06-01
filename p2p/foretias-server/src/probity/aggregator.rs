//! U-shape time-weighted aggregator for probity reports.

use super::report::ProbityReport;

#[derive(Debug, Clone, Copy)]
pub struct UShapeConfig {
    pub hot_window_ns: u64,
    pub valley_center_ns: u64,
    pub ancient_start_ns: u64,
    pub valley_floor: f32,
    pub max_weight: f32,
}

impl Default for UShapeConfig {
    fn default() -> Self {
        Self {
            hot_window_ns: 3_600_000_000_000,        //  1 h
            valley_center_ns: 604_800_000_000_000,   //  7 d
            ancient_start_ns: 2_592_000_000_000_000, // 30 d
            valley_floor: 0.1,
            max_weight: 1.0,
        }
    }
}

pub fn u_shape_weight(age_ns: u64, cfg: &UShapeConfig) -> f32 {
    let age = age_ns as f32;
    let hot = cfg.hot_window_ns as f32;
    let valley = cfg.valley_center_ns as f32;
    let ancient = cfg.ancient_start_ns as f32;

    if age_ns <= cfg.hot_window_ns {
        cfg.max_weight
    } else if age_ns <= cfg.valley_center_ns {
        let t = (age - hot) / (valley - hot);
        cfg.max_weight - (cfg.max_weight - cfg.valley_floor) * t
    } else if age_ns <= cfg.ancient_start_ns {
        cfg.valley_floor
    } else {
        let t = 1.0 - (ancient / age).clamp(0.0, 1.0);
        cfg.valley_floor + (cfg.max_weight - cfg.valley_floor) * t
    }
}

/// Aggregate a probity score for one subject from a slice of reports.
pub fn aggregate(
    reports: &[ProbityReport],
    now_ns: u64,
    credibility: &dyn Fn(&str) -> f32,
    cfg: &UShapeConfig,
) -> f32 {
    let mut score = 0.0f32;
    for r in reports {
        if r.timestamp_ns > now_ns {
            continue;
        }
        let age = now_ns - r.timestamp_ns;
        let w = u_shape_weight(age, cfg);
        let cr = credibility(&r.reporter).max(0.0);
        score += w * cr * r.value;
    }
    score.clamp(-100.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u_shape_weight_boundaries() {
        let cfg = UShapeConfig::default();

        // Hot: full weight
        let w_hot = u_shape_weight(0, &cfg);
        assert!((w_hot - 1.0).abs() < 1e-4);

        // Just past hot window (2 hours, clearly in the decay zone)
        let w_past_hot = u_shape_weight(cfg.hot_window_ns * 2, &cfg);
        assert!(w_past_hot < 1.0);
        assert!(w_past_hot > cfg.valley_floor);

        // Valley center: should be at floor
        let w_valley = u_shape_weight(cfg.valley_center_ns, &cfg);
        assert!((w_valley - cfg.valley_floor).abs() < 1e-4);

        // Between valley and ancient: floor
        let mid = (cfg.valley_center_ns + cfg.ancient_start_ns) / 2;
        let w_mid = u_shape_weight(mid, &cfg);
        assert!((w_mid - cfg.valley_floor).abs() < 1e-4);

        // Very ancient: weight climbs back
        let w_ancient = u_shape_weight(cfg.ancient_start_ns * 10, &cfg);
        assert!(w_ancient > cfg.valley_floor);
        assert!(w_ancient < cfg.max_weight);
    }

    #[test]
    fn aggregate_empty_reports() {
        let cfg = UShapeConfig::default();
        let cred = |_: &str| 1.0f32;
        let score = aggregate(&[], 1_000_000, &cred, &cfg);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn aggregate_future_dated_rejected() {
        let cfg = UShapeConfig::default();
        let now = 1_000_000_000_000;
        let reports = vec![ProbityReport {
            subject: "x".into(),
            reporter: "y".into(),
            attribute: "correctness".into(),
            value: -50.0,
            timestamp_ns: now + 1_000_000_000, // future
            signature: vec![],
            curve: 1,
            slow_signature: vec![],
        }];
        let cred = |_: &str| 1.0f32;
        let score = aggregate(&reports, now, &cred, &cfg);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn aggregate_clamps_to_range() {
        let cfg = UShapeConfig::default();
        let now = 1_000_000_000_000;
        let reports: Vec<ProbityReport> = (0..1000)
            .map(|i| ProbityReport {
                subject: "x".into(),
                reporter: format!("r{}", i),
                attribute: "correctness".into(),
                value: 100.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
                slow_signature: vec![],
            })
            .collect();
        let cred = |_: &str| 1.0f32;
        let score = aggregate(&reports, now, &cred, &cfg);
        assert!(score <= 100.0);
        assert!(score >= -100.0);
    }
}
