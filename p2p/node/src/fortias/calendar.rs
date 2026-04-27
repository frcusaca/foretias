//! In-memory Calendar with append-only tick records.

use super::tick::{TickRecord, CalendarLookup};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

/// An append-only chronological record of TickRecords belonging to a TimeFamily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// TimeBeing identifier of this calendar's owner.
    pub tbid: [u8; 16],
    /// TimeBeing name (human-readable identifier).
    pub tbn: String,
    /// Stamp TimeBeing identifier used for attestation.
    pub stamp_tbid: [u8; 16],
    /// Ordered list of tick records.
    pub ticks: Vec<TickRecord>,
}

impl Calendar {
    /// Creates a new empty calendar with the given TimeBeing ID and name.
    pub fn new(tbid: [u8; 16], tbn: &str) -> Self {
        Self {
            tbid,
            tbn: tbn.to_string(),
            stamp_tbid: tbid,
            ticks: Vec::new(),
        }
    }

    /// Appends a tick record; returns an error if the tick number is not strictly greater than the last.
    pub fn append(&mut self, record: TickRecord) -> Result<(), NodeError> {
        if let Some(last) = self.ticks.last() {
            if record.tick_number <= last.tick_number {
                return Err(NodeError::Internal(format!(
                    "tick number {} is not strictly greater than last tick {}",
                    record.tick_number, last.tick_number
                )));
            }
        }
        self.ticks.push(record);
        Ok(())
    }

    /// Verifies chain integrity by checking that tick numbers are strictly ascending.
    pub fn integrity_check(&self, _server: &dyn CryptoServer) -> Result<bool, NodeError> {
        if self.ticks.len() < 2 {
            return Ok(true);
        }
        // Chain verification stub - would call _verify_pair for each pair
        // For now, structural check only
        for i in 1..self.ticks.len() {
            if self.ticks[i].tick_number <= self.ticks[i-1].tick_number {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Persists the calendar to a JSON file at the given path.
    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap_or_else(|| std::path::Path::new(".")))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Loads a calendar from a JSON file at the given path.
    pub fn load(path: &str) -> Result<Self, NodeError> {
        let data = std::fs::read_to_string(path)?;
        let cal: Calendar = serde_json::from_str(&data)?;
        Ok(cal)
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError> {
        Ok(self.ticks.iter()
            .filter(|t| t.tick_number >= tick_number)
            .take(count)
            .cloned()
            .collect())
    }

    fn latest(&self) -> Option<u64> {
        self.ticks.last().map(|t| t.tick_number)
    }
}
