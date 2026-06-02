//! Clock abstraction for time-dependent operations.
//!
//! Injects a deterministic time source, making protocol logic testable without relying on
//! `SystemTime::now()` directly. Per AGENTS.md: "Do not call system time deep inside
//! protocol logic. Inject a clock."

use parking_lot::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Errors from clock operations.
#[derive(Debug, Clone)]
pub struct ClockError(pub String);

impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ClockError {}

/// Time source abstraction.
pub trait Clock: Send + Sync {
    /// Returns the current timestamp in nanoseconds since UNIX epoch.
    fn now_ns(&self) -> Result<u64, ClockError>;
}

/// Real system clock backed by `SystemTime::now()`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ns(&self) -> Result<u64, ClockError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .map_err(|e| ClockError(format!("SystemTime before UNIX_EPOCH: {e}")))
    }
}

/// Fixed clock that always returns the same timestamp. Useful for deterministic tests.
pub struct FixedClock {
    ns: u64,
}

impl FixedClock {
    pub fn new(ns: u64) -> Self {
        Self { ns }
    }
}

impl Clock for FixedClock {
    fn now_ns(&self) -> Result<u64, ClockError> {
        Ok(self.ns)
    }
}

/// Step clock that advances by a configurable amount each call. Useful for tests.
pub struct StepClock {
    current: Mutex<u64>,
    step: Duration,
}

impl StepClock {
    pub fn new(initial_ns: u64, step: Duration) -> Self {
        Self {
            current: Mutex::new(initial_ns),
            step,
        }
    }
}

impl Clock for StepClock {
    fn now_ns(&self) -> Result<u64, ClockError> {
        let mut cur = self.current.lock();
        let now = *cur;
        *cur += self.step.as_nanos() as u64;
        Ok(now)
    }
}
