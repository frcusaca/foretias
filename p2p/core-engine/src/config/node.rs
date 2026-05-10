//! Node configuration (JSON).

use crate::foretias::types::{KemAlgorithm, SignatureAlgorithm};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::p2p::CollisionConfig;

/// Runtime configuration for a Foretias P2P node, loaded from a JSON file.
#[deprecated(since = "0.5.0", note = "Use TimeFamilyConfig instead")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Address the node listens on for incoming connections (default: `127.0.0.1:4001`).
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    /// Node version string, defaults to the Cargo package version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Filesystem path where calendar data is persisted (default: `.foretias/calendars`).
    #[serde(default = "default_calendar_path")]
    pub calendar_path: PathBuf,
    /// Chronon interval in nanoseconds, defining the tick period (default: 60s).
    #[serde(default = "default_chronon_ns")]
    pub chronon_ns: u64,
    /// Whether the node operates in serialized mode.
    #[serde(default)]
    pub serialized: bool,
    /// Peer addresses for auto attestation (e.g., "host:port").
    #[serde(default)]
    pub peers: Vec<String>,
    /// Mutual attestation frequency in chronons (default 1 = every tick).
    #[serde(default = "default_auto_attest_every_n")]
    pub auto_attest_every_n: u64,
    /// RPC request timeout in seconds (default 5).
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    /// libp2p listen address as a multiaddr string (e.g. "/ip4/0.0.0.0/tcp/9901").
    #[serde(default)]
    pub p2p_listen: Option<String>,
    /// libp2p peers to dial at startup (e.g. "/ip4/127.0.0.1/tcp/9901/p2p/<PeerId>").
    #[serde(default)]
    pub p2p_dial: Vec<String>,
    /// DHT namespace for Kademlia protocol isolation (default: "mainnet").
    #[serde(default = "default_dht_namespace")]
    pub dht_namespace: String,
    /// DHT bootstrap peer multiaddrs (e.g. "/ip4/bootstrap.foretias.example/tcp/4101/p2p/<PeerId>").
    #[serde(default)]
    pub dht_bootstrap: Vec<String>,
    /// Port range for auto-selection when --p2p-listen is omitted. Format: [start, end] (inclusive start, exclusive end). Default: [9900, 9999]
    #[serde(default = "default_p2p_port_range")]
    pub p2p_port_range: [u16; 2],
    /// Known server addresses for auto-registration, format "host:port".
    #[serde(default)]
    pub known_servers: Vec<String>,
    /// Maximum number of peers to auto-discover from DHT (default: 13).
    #[serde(default = "default_max_discovered_peers")]
    pub max_discovered_peers: usize,
    /// Collision detection configuration.
    #[serde(default)]
    pub collision: CollisionConfig,
    /// Signature algorithm for stamping (default: SPHINCS+-SHA2-128s-simple).
    #[serde(default = "default_signature_algorithm")]
    pub signature_algorithm: SignatureAlgorithm,
    /// KEM algorithm for key exchange (default: Noise-XX).
    #[serde(default = "default_kem_algorithm")]
    pub kem_algorithm: KemAlgorithm,
}

pub const FORETIAS_MUTUAL_ATTESTATION_MINIMUM: u64 = 60_000_000_000;

pub fn compute_attestation_interval(my_chronon_ns: u64, peer_chronon_ns: u64) -> u64 {
    FORETIAS_MUTUAL_ATTESTATION_MINIMUM.max(50 * (my_chronon_ns + peer_chronon_ns))
}

fn default_listen_addr() -> String { "127.0.0.1:4001".to_string() }
fn default_version() -> String { env!("CARGO_PKG_VERSION").to_string() }
fn default_calendar_path() -> PathBuf { PathBuf::from(".foretias/calendars") }
fn default_chronon_ns() -> u64 { 60_000_000_000 }
fn default_auto_attest_every_n() -> u64 { 1 }
fn default_request_timeout_secs() -> u64 { 5 }
fn default_dht_namespace() -> String { "mainnet".to_string() }
fn default_signature_algorithm() -> SignatureAlgorithm { SignatureAlgorithm::SPHINCS_SHA2_128S }
fn default_kem_algorithm() -> KemAlgorithm { KemAlgorithm::NoiseXX }
fn default_p2p_port_range() -> [u16; 2] { [9900, 9999] }
fn default_max_discovered_peers() -> usize { 13 }

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            version: default_version(),
            calendar_path: default_calendar_path(),
            chronon_ns: default_chronon_ns(),
            serialized: false,
            peers: Vec::new(),
            auto_attest_every_n: default_auto_attest_every_n(),
            request_timeout_secs: default_request_timeout_secs(),
            p2p_listen: None,
            p2p_dial: Vec::new(),
            dht_namespace: default_dht_namespace(),
            dht_bootstrap: Vec::new(),
            p2p_port_range: default_p2p_port_range(),
            known_servers: Vec::new(),
            max_discovered_peers: default_max_discovered_peers(),
            collision: CollisionConfig::default(),
            signature_algorithm: default_signature_algorithm(),
            kem_algorithm: default_kem_algorithm(),
        }
    }
}

impl NodeConfig {
    /// Loads configuration from a JSON file at the given path; returns defaults on any error.
    pub fn load(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    /// Returns the default filesystem path where the configuration file is expected.
    pub fn default_path() -> String {
        std::env::var("HOME")
            .map(|h| format!("{}/.config/foretias/foretias.settings.json", h))
            .unwrap_or_else(|_| ".config/foretias/foretias.settings.json".to_string())
    }
    /// Validates the configuration, returning an error string if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.dht_namespace.is_empty() {
            return Err("dht_namespace must not be empty".to_string());
        }
        if self.dht_namespace.len() > 64 {
            return Err("dht_namespace must be 64 bytes or fewer".to_string());
        }
        Ok(())
    }
}
