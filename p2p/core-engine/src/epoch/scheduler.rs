//! EpochScheduler — wall-clock-aligned epoch phase timer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::clock::{Clock, SystemClock};

/// Manages epoch numbering and boundary alignment.
pub struct EpochScheduler {
    epoch_duration: Duration,
    epoch_number:   AtomicU64,
    clock: Arc<dyn Clock>,
}

impl EpochScheduler {
    /// Create a new scheduler with the given epoch duration in seconds.
    pub fn new(epoch_duration_secs: u64) -> Self {
        Self::with_clock(epoch_duration_secs, Arc::new(SystemClock))
    }

    /// Create a scheduler with an injected clock (for testing).
    pub fn with_clock(epoch_duration_secs: u64, clock: Arc<dyn Clock>) -> Self {
        Self {
            epoch_duration: Duration::from_secs(epoch_duration_secs),
            epoch_number: AtomicU64::new(0),
            clock,
        }
    }

    /// Current epoch number.
    pub fn current_epoch_number(&self) -> u64 {
        self.epoch_number.load(Ordering::Relaxed)
    }

    /// Advance to the next epoch and return the new number.
    pub fn next_epoch_number(&self) -> u64 {
        self.epoch_number.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Time remaining until the next epoch boundary.
    pub fn align_to_next_boundary(&self) -> Duration {
        let now_ns = self.clock.now_ns().unwrap_or(0);
        let now_secs = now_ns / 1_000_000_000;
        let epoch_secs = self.epoch_duration.as_secs();
        let current = now_secs / epoch_secs;
        let next = (current + 1) * epoch_secs;
        Duration::from_secs(next.saturating_sub(now_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_scheduler_aligns_to_boundary() {
        let scheduler = EpochScheduler::new(3600);
        let wait = scheduler.align_to_next_boundary();
        // Should be between 0 and 3600 seconds
        assert!(wait.as_secs() <= 3600);
    }

    #[test]
    fn epoch_scheduler_increment_epoch() {
        let scheduler = EpochScheduler::new(120);
        assert_eq!(scheduler.current_epoch_number(), 0);
        assert_eq!(scheduler.next_epoch_number(), 1);
        assert_eq!(scheduler.current_epoch_number(), 1);
        assert_eq!(scheduler.next_epoch_number(), 2);
        assert_eq!(scheduler.current_epoch_number(), 2);
    }
}
