//! Calendar persistence wrapper with config metadata header.

use serde::{Deserialize, Serialize};
use super::calendar::CalendarConfig;

/// On-disk calendar wrapper with config metadata header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedCalendar {
    /// Non-repeating config metadata
    pub config: CalendarMetadata,
    /// Append-only tick records (raw JSON from ChrononRecord)
    pub ticks: Vec<serde_json::Value>,
}

/// Calendar metadata header persisted alongside tick data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarMetadata {
    /// TimeBeing ID (derived, not configurable)
    pub tbid: String,
    /// TimeBeing Name
    pub tbn: String,
    /// Stamp TimeBeing ID
    pub stamp_tbid: String,
    /// Software version that last persisted this calendar
    pub persisted_by: String,
    /// Calendar configuration snapshot
    pub calendar_config: CalendarConfig,
}
