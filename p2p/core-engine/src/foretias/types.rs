//! Type aliases for core Foretias data types.
//!
//! These are zero-cost `type` aliases — no runtime difference from the underlying types.
//! They make signatures self-documenting and prevent mixing up byte arrays.

use serde::{Deserialize, Serialize};

/// Time Being ID — 16-byte unique identifier for a time being.
pub type Tbid = [u8; 16];

/// Public key bytes — variable length per algorithm (32 for Ed25519, 7856 for SPHINCS+, etc).
pub type PublicKeyBytes = Vec<u8>;

/// Signature bytes — variable length per algorithm (64 for Ed25519, 7856 for SPHINCS+, etc).
pub type SignatureBytes = Vec<u8>;

/// Algorithm identifier as plain text (e.g. "SPHINCS+-SHA2-128s-simple").
pub type AlgorithmId = String;

/// SHA-256 hash digest — 32 bytes.
pub type Digest = [u8; 32];

/// Encrypted message payload (before/after encryption).
pub type Message = Vec<u8>;

/// Auto-attestation nonce — 16 bytes of entropy.
pub type AaNonce = [u8; 16];

/// Tick number.
pub type TickNumber = u64;

/// Signature algorithm selector.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub enum SignatureAlgorithm {
    /// Ed25519 — legacy, retained for compatibility.
    Ed25519,
    /// SPHINCS+-SHA2-128s-simple — default post-quantum signing.
    SPHINCS_SHA2_128S,
    /// Dilithium3 — optional post-quantum signing (NIST Level 3).
    Dilithium3,
}

impl SignatureAlgorithm {
    /// Returns the liboqs algorithm identifier string.
    pub fn to_id_string(&self) -> &'static str {
        match self {
            Self::Ed25519         => "Ed25519",
            Self::SPHINCS_SHA2_128S => "SPHINCS+-SHA2-128s-simple",
            Self::Dilithium3      => "Dilithium3",
        }
    }

    /// Parse from a liboqs algorithm identifier string.
    pub fn from_id_string(id: &str) -> Result<Self, crate::error::CryptoError> {
        match id {
            "Ed25519"                => Ok(Self::Ed25519),
            "SPHINCS+-SHA2-128s-simple" => Ok(Self::SPHINCS_SHA2_128S),
            "Dilithium3"             => Ok(Self::Dilithium3),
            _ => Err(crate::error::CryptoError::UnknownAlgorithm(id.to_string())),
        }
    }

    /// Maximum public key size in bytes for this algorithm.
    pub fn pubkey_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519         => 32,
            Self::SPHINCS_SHA2_128S => 32,
            Self::Dilithium3      => 1952,
        }
    }

    /// Maximum signature size in bytes for this algorithm.
    pub fn signature_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519         => 64,
            Self::SPHINCS_SHA2_128S => 7856,
            Self::Dilithium3      => 3309,
        }
    }
}

impl From<SignatureAlgorithm> for String {
    fn from(alg: SignatureAlgorithm) -> Self {
        alg.to_id_string().to_string()
    }
}

impl TryFrom<String> for SignatureAlgorithm {
    type Error = crate::error::CryptoError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_id_string(&s)
    }
}

impl std::fmt::Display for SignatureAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_id_string())
    }
}

/// KEM algorithm selector.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub enum KemAlgorithm {
    /// Noise-XX — default classical key exchange (Ed25519/X25519).
    NoiseXX,
    /// ML-KEM-768 — optional post-quantum key exchange (NIST Level 3).
    MLKEM_768,
}

impl KemAlgorithm {
    /// Returns the liboqs algorithm identifier string.
    pub fn to_id_string(&self) -> &'static str {
        match self {
            Self::NoiseXX  => "Noise-XX",
            Self::MLKEM_768 => "ML-KEM-768",
        }
    }

    /// Parse from a liboqs algorithm identifier string.
    pub fn from_id_string(id: &str) -> Result<Self, crate::error::CryptoError> {
        match id {
            "Noise-XX"  => Ok(Self::NoiseXX),
            "ML-KEM-768" => Ok(Self::MLKEM_768),
            _ => Err(crate::error::CryptoError::UnknownAlgorithm(id.to_string())),
        }
    }
}

impl From<KemAlgorithm> for String {
    fn from(alg: KemAlgorithm) -> Self {
        alg.to_id_string().to_string()
    }
}

impl TryFrom<String> for KemAlgorithm {
    type Error = crate::error::CryptoError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_id_string(&s)
    }
}

impl std::fmt::Display for KemAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_id_string())
    }
}
