//! Node configuration (TOML).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_calendar_path")]
    pub calendar_path: PathBuf,
    #[serde(default = "default_chronon_ns")]
    pub chronon_ns: u64,
    #[serde(default)]
    pub serialized: bool,
}

fn default_listen_addr() -> String { "127.0.0.1:4001".to_string() }
fn default_version() -> String { env!("CARGO_PKG_VERSION").to_string() }
fn default_calendar_path() -> PathBuf { PathBuf::from(".fortias/calendars") }
fn default_chronon_ns() -> u64 { 60_000_000_000 }

impl NodeConfig {
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn default_path() -> String {
        std::env::var("HOME")
            .map(|h| format!("{}/.config/fortias/fortias.settings.json", h))
            .unwrap_or_else(|_| ".config/fortias/fortias.settings.json".to_string())
    }
}
