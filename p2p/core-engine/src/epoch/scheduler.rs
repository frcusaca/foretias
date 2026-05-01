//! EpochScheduler — wall-clock-aligned epoch phase timer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Manages epoch numbering and boundary alignment.
pub struct EpochScheduler {
    epoch_duration: Duration,
    epoch_number:   AtomicU64,
}

impl EpochScheduler {
    /// Create a new scheduler with the given epoch duration in seconds.
    pub fn new(epoch_duration_secs: u64) -> Self {
        Self {
            epoch_duration: Duration::from_secs(epoch_duration_secs),
            epoch_number: AtomicU64::new(0),
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let epoch_ns = self.epoch_duration.as_secs();
        let current = now.as_secs() / epoch_ns;
        let next = (current + 1) * epoch_ns;
        let elapsed = now.as_secs();
        Duration::from_secs(next.saturating_sub(elapsed))
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
