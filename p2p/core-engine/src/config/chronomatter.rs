//! Chronomatter configuration: chronon timing, signing, key exchange, and attestation.

use crate::foretias::types::{KemAlgorithm, SignatureAlgorithm};
use serde::{Deserialize, Serialize};

/// Chronomatter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronomatterConfig {
    /// Chronon interval in nanoseconds, defining the tick period (default: 60s).
    #[serde(default = "default_chronon_ns")]
    pub chronon_ns: u64,

    /// TimeBeing Name (human-readable family identifier).
    #[serde(default = "default_tbn")]
    pub tbn: String,

    /// Whether the time being is dormant.
    #[serde(default)]
    pub dormant: bool,

    /// Signature algorithm for stamping (default: SPHINCS+-SHA2-128s-simple).
    #[serde(default = "default_signature_algorithm")]
    pub signature_algorithm: SignatureAlgorithm,

    /// KEM algorithm for key exchange (default: Noise-XX).
    #[serde(default = "default_kem_algorithm")]
    pub kem_algorithm: KemAlgorithm,

    /// Auto-attestation configuration.
    #[serde(default)]
    pub auto_attest: AutoAttestConfig,

    /// Key rotation configuration.
    #[serde(default)]
    pub key_rotation: KeyRotationConfig,
}

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

/// Key rotation configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyRotationConfig {
    /// Whether key rotation is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Rotation interval in chronons.
    #[serde(default)]
    pub interval_chronons: u64,
}

/// Default chronon interval: 60 seconds in nanoseconds.
pub const DEFAULT_CHRONON_NS: u64 = 60_000_000_000;

// ── Defaults ──

fn default_chronon_ns() -> u64 {
    DEFAULT_CHRONON_NS
}

fn default_signature_algorithm() -> SignatureAlgorithm {
    SignatureAlgorithm::SPHINCS_SHA2_128S
}

fn default_kem_algorithm() -> KemAlgorithm {
    KemAlgorithm::NoiseXX
}

fn default_tbn() -> String {
    "Default".to_string()
}

fn default_auto_attest_every_n() -> u64 {
    1
}

fn default_request_timeout_secs() -> u64 {
    5
}

impl Default for ChronomatterConfig {
    fn default() -> Self {
        Self {
            chronon_ns: default_chronon_ns(),
            tbn: default_tbn(),
            dormant: false,
            signature_algorithm: default_signature_algorithm(),
            kem_algorithm: default_kem_algorithm(),
            auto_attest: AutoAttestConfig::default(),
            key_rotation: KeyRotationConfig::default(),
        }
    }
}
