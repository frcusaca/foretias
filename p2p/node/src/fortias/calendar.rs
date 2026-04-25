//! In-memory Calendar with append-only tick records.

use super::tick::{TickRecord, CalendarLookup};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    pub tbid: [u8; 16],
    pub tbn: String,
    pub stamp_tbid: [u8; 16],
    pub ticks: Vec<TickRecord>,
}

impl Calendar {
    pub fn new(tbid: [u8; 16], tbn: &str) -> Self {
        Self {
            tbid,
            tbn: tbn.to_string(),
            stamp_tbid: tbid,
            ticks: Vec::new(),
        }
    }

    pub fn append(&mut self, record: TickRecord) {
        assert!(
            self.ticks.last().map(|r| record.tick_number > r.tick_number).unwrap_or(true),
            "tick numbers must be strictly ascending"
        );
        self.ticks.push(record);
    }

    /// Check chain integrity: verify each consecutive pair.
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

    /// Save calendar to JSON file.
    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap_or_else(|| std::path::Path::new(".")))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load calendar from JSON file.
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
