//! Calendar and encryption configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Calendar configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarConfig {
    /// Time Being number (tick number).
    #[serde(default)]
    pub tbn: u64,

    /// Filesystem path where calendar data is persisted (default: `.foretias/calendars`).
    #[serde(default = "default_persist_path")]
    pub persist_path: PathBuf,

    /// Encryption configuration.
    #[serde(default)]
    pub encryption: EncryptionConfig,
}

/// Encryption configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Whether encryption is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Encryption algorithm identifier.
    #[serde(default)]
    pub algorithm: String,
}

// ── Defaults ──

fn default_persist_path() -> PathBuf {
    PathBuf::from(".foretias/calendars")
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            tbn: 0,
            persist_path: default_persist_path(),
            encryption: EncryptionConfig::default(),
        }
    }
}
