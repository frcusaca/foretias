//! TimeFamily top-level configuration.

use serde::{Deserialize, Serialize};

use super::p2p::P2PConfig;
use super::chronomatter::ChronomatterConfig;
use super::calendar::CalendarConfig;

/// Top-level configuration for a TimeFamily node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeFamilyConfig {
    /// Config file version for migration compatibility
    pub version: String,
    /// Chronomatter (ticking engine) configuration
    #[serde(default)]
    pub chronomatter: ChronomatterConfig,
    /// Calendar configurations (one per TimeBeing member)
    #[serde(default = "default_calendars")]
    pub calendars: Vec<CalendarConfig>,
    /// P2P / networking configuration
    #[serde(default)]
    pub p2p: P2PConfig,
}

fn default_calendars() -> Vec<CalendarConfig> {
    vec![CalendarConfig::default()]
}

impl Default for TimeFamilyConfig {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            chronomatter: ChronomatterConfig::default(),
            calendars: default_calendars(),
            p2p: P2PConfig::default(),
        }
    }
}

impl TimeFamilyConfig {
    /// Load from a JSON file path, falling back to defaults on error.
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Returns the default path: ~/.config/foretias/foretias.json
    pub fn default_path() -> String {
        std::env::var("HOME")
            .map(|h| format!("{}/.config/foretias/foretias.json", h))
            .unwrap_or_else(|_| ".config/foretias/foretias.json".to_string())
    }
}
