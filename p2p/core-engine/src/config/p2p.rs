//! P2P, DHT, and collision detection configuration.

use serde::{Deserialize, Serialize};

/// P2P networking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PConfig {
    /// Address the node listens on for incoming connections (default: `127.0.0.1:4001`).
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// libp2p listen address as a multiaddr string (e.g. "/ip4/0.0.0.0/tcp/9901").
    #[serde(default)]
    pub p2p_listen: Option<String>,

    /// Port range for auto-selection when --p2p-listen is omitted.
    /// Format: [start, end] (inclusive start, exclusive end). Default: [9900, 9999].
    #[serde(default = "default_p2p_port_range")]
    pub p2p_port_range: [u16; 2],

    /// libp2p peers to dial at startup.
    #[serde(default)]
    pub p2p_dial: Vec<String>,

    /// Known server addresses for auto-registration, format "host:port".
    #[serde(default)]
    pub known_servers: Vec<String>,

    /// Maximum number of peers to auto-discover from DHT (default: 13).
    #[serde(default = "default_max_discovered_peers")]
    pub max_discovered_peers: usize,

    /// DHT configuration.
    #[serde(default)]
    pub dht: DHTConfig,

    /// Collision detection configuration.
    #[serde(default)]
    pub collision: CollisionConfig,
}

/// DHT configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DHTConfig {
    /// DHT namespace for Kademlia protocol isolation (default: "mainnet").
    #[serde(default = "default_dht_namespace")]
    pub namespace: String,

    /// DHT bootstrap peer multiaddrs.
    #[serde(default)]
    pub bootstrap: Vec<String>,
}

/// Collision detection configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollisionConfig {
    /// Heartbeat broadcast interval in seconds (default: 30).
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,

    /// Number of recent nonces to track for collision detection (default: 10).
    #[serde(default = "default_nonce_window")]
    pub nonce_window: usize,

    /// Seconds to wait for liege response after collision (default: 30).
    #[serde(default = "default_liege_wait_secs")]
    pub liege_wait_secs: u64,
}

// ── Defaults ──

fn default_listen_addr() -> String {
    "127.0.0.1:4001".to_string()
}

fn default_p2p_port_range() -> [u16; 2] {
    [9900, 9999]
}

fn default_max_discovered_peers() -> usize {
    13
}

fn default_dht_namespace() -> String {
    "mainnet".to_string()
}

fn default_heartbeat_interval_secs() -> u64 {
    30
}

fn default_nonce_window() -> usize {
    10
}

fn default_liege_wait_secs() -> u64 {
    30
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            p2p_listen: None,
            p2p_port_range: default_p2p_port_range(),
            p2p_dial: Vec::new(),
            known_servers: Vec::new(),
            max_discovered_peers: default_max_discovered_peers(),
            dht: DHTConfig::default(),
            collision: CollisionConfig::default(),
        }
    }
}
