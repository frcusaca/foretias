//! TimeFamily top-level configuration.

use serde::{Deserialize, Serialize};

use super::p2p::{CommunerdConfig, DHTConfig, MutualAttestConfig};
use super::chronomatter::{ChronomatterConfig, KeyRotationConfig};
use super::calendar::{CalendarConfig, EncryptionConfig};
use super::node::NodeConfig;

/// Logging configuration for the TimeFamily logger.
///
/// Controls file output, per-component log levels, and format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Directory where log files are written.
    /// Default: `~/.local/share/foretias/log/`
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    /// Global minimum log level. Lower levels (TRACE, DEBUG) are filtered
    /// before any formatting work occurs. Default: "info".
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Per-component overrides in `tracing-subscriber` EnvFilter syntax.
    /// Examples: ["communerd=debug", "foretias_core::chronomatter=trace"]
    /// These refine or override the global `level` for specific modules.
    #[serde(default)]
    pub component_levels: Vec<String>,
    /// Log file format: "pretty" (human-readable), "json" (structured).
    /// Default: "pretty".
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_listen_addr() -> String {
    "127.0.0.1:4001".to_string()
}

fn default_log_dir() -> String {
    std::env::var("HOME")
        .map(|h| format!("{}/.local/share/foretias/log", h))
        .unwrap_or_else(|_| "/tmp/foretias-log".to_string())
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            level: default_log_level(),
            component_levels: Vec::new(),
            format: default_log_format(),
        }
    }
}

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
    /// Communerd (P2P networking) configuration
    #[serde(default)]
    pub communerd: CommunerdConfig,
    /// Address the node listens on for incoming connections.
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    /// Logging configuration (file output, levels, format)
    #[serde(default)]
    pub logging: LoggingConfig,
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
            communerd: CommunerdConfig::default(),
            listen_addr: default_listen_addr(),
            logging: LoggingConfig::default(),
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

    /// Build a TimeFamilyConfig from CLI arguments, optionally merging with a config file.
    pub fn from_cli_and_file(
        listen_addr: &str,
        chronon_ns: u64,
        persist_path: Option<std::path::PathBuf>,
        dormant: bool,
        peers: Vec<String>,
        auto_attest_every_n: u64,
        request_timeout_secs: u64,
        p2p_listen: Option<String>,
        p2p_port_range: [u16; 2],
        p2p_dial: Vec<String>,
        known_servers: Vec<String>,
        dht_namespace: &str,
        dht_bootstrap: Vec<String>,
        max_discovered_peers: usize,
        config_file_path: Option<&str>,
    ) -> Self {
        let mut cfg = if let Some(path) = config_file_path {
            Self::load(path)
        } else {
            Self::default()
        };

        cfg.chronomatter.chronon_ns = chronon_ns;
        cfg.chronomatter.dormant = dormant;

        cfg.listen_addr = listen_addr.to_string();

        cfg.communerd.mutual_attest.peers = peers;
        cfg.communerd.mutual_attest.every_n_chronons = auto_attest_every_n;
        cfg.communerd.mutual_attest.request_timeout_secs = request_timeout_secs;

        cfg.communerd.p2p_listen = p2p_listen;
        cfg.communerd.p2p_port_range = p2p_port_range;
        cfg.communerd.p2p_dial = p2p_dial;
        cfg.communerd.known_servers = known_servers;
        cfg.communerd.max_discovered_peers = max_discovered_peers;
        cfg.communerd.dht.namespace = dht_namespace.to_string();
        cfg.communerd.dht.bootstrap = dht_bootstrap;

        if let Some(p) = persist_path {
            if cfg.calendars.is_empty() {
                cfg.calendars.push(CalendarConfig::default());
            }
            cfg.calendars[0].persist_path = p;
        }

        cfg
    }

    /// Get the first calendar's persist path, or the default.
    pub fn persist_path(&self) -> Option<std::path::PathBuf> {
        self.calendars.first().map(|c| c.persist_path.clone())
    }

    /// Get the peers list from mutual_attest config.
    pub fn mutual_attest_peers(&self) -> Vec<String> {
        self.communerd.mutual_attest.peers.clone()
    }

    /// Get the mutual-attest frequency.
    pub fn mutual_attest_every_n(&self) -> u64 {
        self.communerd.mutual_attest.every_n_chronons
    }

    /// Get the mutual-attest request timeout.
    pub fn mutual_attest_request_timeout_secs(&self) -> u64 {
        self.communerd.mutual_attest.request_timeout_secs
    }
}

impl From<NodeConfig> for TimeFamilyConfig {
    fn from(node: NodeConfig) -> Self {
        Self {
            version: node.version,
            chronomatter: ChronomatterConfig {
                chronon_ns: node.chronon_ns,
                tbn: "Default".to_string(),
                dormant: false,
                signature_algorithm: node.signature_algorithm,
                kem_algorithm: node.kem_algorithm,
                key_rotation: KeyRotationConfig::default(),
            },
            calendars: vec![CalendarConfig {
                tbn: 0,
                persist_path: node.calendar_path,
                encryption: EncryptionConfig::default(),
            }],
            communerd: CommunerdConfig {
                mutual_attest: MutualAttestConfig {
                    every_n_chronons: node.auto_attest_every_n,
                    request_timeout_secs: node.request_timeout_secs,
                    peers: node.peers,
                },
                p2p_listen: node.p2p_listen,
                p2p_port_range: node.p2p_port_range,
                p2p_dial: node.p2p_dial,
                known_servers: node.known_servers,
                max_discovered_peers: node.max_discovered_peers,
                dht: DHTConfig {
                    namespace: node.dht_namespace,
                    bootstrap: node.dht_bootstrap,
                },
                collision: node.collision,
            },
            listen_addr: node.listen_addr,
            logging: LoggingConfig::default(),
        }
    }
}
