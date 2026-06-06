//! Concurrent stamp stress test — verifies that `fetch_add` eliminates tick counter conflicts.

use foretias_core::chronomatter::Chronomatter;
use foretias_core::crypto_server::{self, ForetiasCurve};
use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::types::{Tbid, TickNumber};
use foretias_core::foretias::Calendar;
use foretias_core::foretias::ChrononRecord;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::Arc;

struct StressObserver {
    last_tick: Arc<AtomicU64>,
    calendar: Arc<RwLock<Calendar>>,
}
impl TickObserver for StressObserver {
    fn on_tick_advance(
        &self,
        chronon_number: TickNumber,
        _public_key: &[u8],
        tick_record: &ChrononRecord,
    ) {
        self.last_tick.store(chronon_number.0, SeqCst);
        self.calendar.write().append(tick_record.clone()).unwrap();
    }
}

#[tokio::test]
async fn concurrent_stamps_no_conflicts() {
    let last_tick = Arc::new(AtomicU64::new(0));
    let calendar = Arc::new(RwLock::new(Calendar::new(
        Tbid::from_raw([0u8; 96]),
        "stress-test",
    )));
    let observer = Arc::new(StressObserver {
        last_tick: Arc::clone(&last_tick),
        calendar: Arc::clone(&calendar),
    });
    let crypto =
        Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519).expect("crypto server"));
    let cm =
        Arc::new(Chronomatter::new(1_000_000_000, observer, crypto).expect("create Chronomatter"));
    calendar.write().tbid = cm.get_tbid();
    calendar.write().tbn = cm.get_tbn().to_string();

    let tasks = 16u64;
    let stamps_per_task = 100u64;

    let mut handles = Vec::new();

    for _ in 0..tasks {
        let cm = Arc::clone(&cm);
        handles.push(tokio::spawn(async move {
            let mut results = Vec::new();
            for i in 0..stamps_per_task {
                let r = cm.stamp(format!("msg-{}", i).into_bytes(), "stress".to_string());
                results.push(r);
            }
            results
        }));
    }

    let mut all_results = Vec::new();
    for handle in handles {
        all_results.push(handle.await.unwrap());
    }

    let mut errors = Vec::new();
    for task_results in &all_results {
        for r in task_results {
            if let Err(e) = r {
                errors.push(e.to_string());
            }
        }
    }

    for err in &errors {
        assert!(
            !err.contains("tick counter conflict"),
            "Should never get tick counter conflict with fetch_add: {}",
            err
        );
    }

    let total_stamps = tasks * stamps_per_task;
    assert_eq!(cm.current_tick(), total_stamps);
}
