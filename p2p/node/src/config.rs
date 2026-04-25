//! Node configuration (TOML).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Runtime configuration for a Fortias P2P node, loaded from a TOML file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Address the node listens on for incoming connections (default: `127.0.0.1:4001`).
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    /// Node version string, defaults to the Cargo package version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Filesystem path where calendar data is persisted (default: `.fortias/calendars`).
    #[serde(default = "default_calendar_path")]
    pub calendar_path: PathBuf,
    /// Chronon interval in nanoseconds, defining the tick period (default: 60s).
    #[serde(default = "default_chronon_ns")]
    pub chronon_ns: u64,
    /// Whether the node operates in serialized mode.
    #[serde(default)]
    pub serialized: bool,
}

fn default_listen_addr() -> String { "127.0.0.1:4001".to_string() }
fn default_version() -> String { env!("CARGO_PKG_VERSION").to_string() }
fn default_calendar_path() -> PathBuf { PathBuf::from(".fortias/calendars") }
fn default_chronon_ns() -> u64 { 60_000_000_000 }

impl NodeConfig {
    /// Loads configuration from a TOML file at the given path; returns defaults on any error.
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
    /// Returns the default filesystem path where the configuration file is expected.
    pub fn default_path() -> String {
        std::env::var("HOME")
            .map(|h| format!("{}/.config/fortias/fortias.settings.json", h))
            .unwrap_or_else(|_| ".config/fortias/fortias.settings.json".to_string())
    }
}
