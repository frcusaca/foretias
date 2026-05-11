//! P2P, DHT, collision detection, and auto-attestation configuration.

use serde::{Deserialize, Serialize};

/// Auto-attestation configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoAttestConfig {
    /// Mutual attestation frequency in chronons (default 1 = every tick).
    #[serde(default = "default_auto_attest_every_n")]
    pub every_n_chronons: u64,

    /// RPC request timeout in seconds (default 5).
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,

    /// Peer addresses for auto attestation (e.g., "host:port").
    #[serde(default)]
    pub peers: Vec<String>,
}

/// Communerd configuration — P2P networking, auto-attestation, DHT, and collision.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommunerdConfig {
    /// Auto-attestation configuration.
    #[serde(default)]
    pub auto_attest: AutoAttestConfig,

    /// libp2p listen address as a multiaddr string.
    #[serde(default)]
    pub p2p_listen: Option<String>,

    /// Port range for auto-selection when --p2p-listen is omitted.
    #[serde(default = "default_p2p_port_range")]
    pub p2p_port_range: [u16; 2],

    /// libp2p peers to dial at startup.
    #[serde(default)]
    pub p2p_dial: Vec<String>,

    /// Known server addresses for auto-registration.
    #[serde(default)]
    pub known_servers: Vec<String>,

    /// Maximum number of peers to auto-discover from DHT.
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

fn default_auto_attest_every_n() -> u64 {
    1
}

fn default_request_timeout_secs() -> u64 {
    5
}
