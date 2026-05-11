//! Type aliases for core Foretias data types.
//!
//! These are zero-cost `type` aliases — no runtime difference from the underlying types.
//! They make signatures self-documenting and prevent mixing up byte arrays.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::encoding::FTByteArray;

/// Time Being ID — dual-key identity for tb_version 1.0.
/// Layout: Ed25519_PK(32) ‖ SLH-DSA-SHA2-256f_PK(64) = 96 bytes total.
///
/// The `‖` operator denotes byte-level concatenation (NOT bitwise OR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tbid {
    /// Flat 96-byte representation: Ed25519_PK(32) ‖ SLH-DSA_PK(64).
    pub inner: FTByteArray<96>,
}

impl Default for Tbid {
    fn default() -> Self {
        Self { inner: FTByteArray::zeros() }
    }
}

impl Tbid {
    /// Raw 96-byte representation for wire format / storage.
    pub fn raw_bytes(&self) -> [u8; 96] {
        *self.inner
    }

    /// Parse from raw 96-byte representation (ed25519 first, then slh_dsa).
    pub fn from_raw(bytes: [u8; 96]) -> Self {
        Self { inner: FTByteArray::new(bytes) }
    }

    /// Parse from a 96-byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::CryptoError> {
        if bytes.len() != 96 {
            return Err(crate::error::CryptoError::BadInput("TBID public key must be 96 bytes"));
        }
        let mut raw = [0u8; 96];
        raw.copy_from_slice(bytes);
        Ok(Self::from_raw(raw))
    }

    /// Ed25519 public key for Ed25519-specific verification (first 32 bytes).
    pub fn ed25519_public_key(&self) -> [u8; 32] {
        self.inner[..32].try_into().unwrap()
    }

    /// SLH-DSA-SHA2-256f public key for SLH-DSA-specific verification (last 64 bytes).
    pub fn slh_dsa_public_key(&self) -> [u8; 64] {
        self.inner[32..].try_into().unwrap()
    }

    /// Hex-encoded representation for DHT keys and logging.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.inner[..])
    }

    /// Create a test TBID with all bytes set to the given value.
    #[cfg(test)]
    pub fn test() -> Self {
        Self { inner: FTByteArray::new([0xAB; 96]) }
    }
}

/// Secret key material for a TBID (tb_version 1.0).
/// Both private keys are held together; zeroized on drop.
///
/// Layout: Ed25519_SK(32) ‖ SLH-DSA-SHA2-256f_SK(128) = 160 bytes total.
pub struct TbidSecret {
    /// Ed25519 secret key — fast signing (32 bytes).
    ed25519_sk: Zeroizing<[u8; 32]>,
    /// SLH-DSA-SHA2-256f secret key — quantum-resistant signing (128 bytes).
    slh_dsa_sk: Zeroizing<Vec<u8>>,
}

impl TbidSecret {
    /// Generate a fresh TBID keypair (both Ed25519 and SLH-DSA).
    ///
    /// Returns the public key (`Tbid`) and the secret key (`TbidSecret`).
    pub fn generate() -> Result<(Tbid, Self), crate::error::CryptoError> {
        use crate::crypto_server::signing_tbid;
        let (pub_bytes, sec_bytes) = signing_tbid::tbid_keypair()?;
        let tbid = Tbid::from_bytes(&pub_bytes)?;
        let secret = Self::from_bytes(&sec_bytes)?;
        Ok((tbid, secret))
    }

    /// Sign a message with both algorithms.
    /// Returns Ed25519_SIG(64) ‖ SLH-DSA_SIG(49856) = 49,920 bytes.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, crate::error::CryptoError> {
        use crate::crypto_server::signing_tbid;
        let mut secret_bytes = Vec::with_capacity(160);
        secret_bytes.extend_from_slice(&self.ed25519_sk[..]);
        secret_bytes.extend_from_slice(&self.slh_dsa_sk);
        // Safe: we always generate exactly 160 bytes
        let secret_array: [u8; 160] = secret_bytes.try_into()
            .map_err(|_| crate::error::CryptoError::BadInput("secret key length mismatch"))?;
        signing_tbid::tbid_sign(&SignatureBytes::from(secret_array), message)
    }

    /// Ed25519 secret key for Ed25519-specific operations.
    pub fn ed25519_secret_key(&self) -> &[u8; 32] {
        &self.ed25519_sk
    }

    /// SLH-DSA-SHA2-256f secret key for SLH-DSA-specific operations.
    pub fn slh_dsa_secret_key(&self) -> &[u8] {
        &self.slh_dsa_sk
    }

    /// Construct from flat 160-byte secret key material.
    fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::CryptoError> {
        if bytes.len() != 160 {
            return Err(crate::error::CryptoError::BadInput("TBID secret must be 160 bytes"));
        }
        let mut ed25519_sk = [0u8; 32];
        ed25519_sk.copy_from_slice(&bytes[..32]);
        let slh_dsa_sk = bytes[32..160].to_vec();
        Ok(Self {
            ed25519_sk: Zeroizing::new(ed25519_sk),
            slh_dsa_sk: Zeroizing::new(slh_dsa_sk),
        })
    }
}

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

/// Tick number — monotonically increasing identifier for a calendar tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TickNumber(pub u64);

impl TickNumber {
    /// Returns zero-based tick index as usize.
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

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
    /// SLH-DSA-SHA2-256f-simple — NIST Level 5 post-quantum signing.
    SLH_DSA_SHA2_256F,
}

impl SignatureAlgorithm {
    /// Returns the liboqs algorithm identifier string.
    pub fn to_id_string(&self) -> &'static str {
        match self {
            Self::Ed25519         => "Ed25519",
            Self::SPHINCS_SHA2_128S => "SPHINCS+-SHA2-128s-simple",
            Self::Dilithium3      => "Dilithium3",
            Self::SLH_DSA_SHA2_256F => "SPHINCS+-SHA2-256f-simple",
        }
    }

    /// Parse from a liboqs algorithm identifier string.
    pub fn from_id_string(id: &str) -> Result<Self, crate::error::CryptoError> {
        match id {
            "Ed25519"                => Ok(Self::Ed25519),
            "SPHINCS+-SHA2-128s-simple" => Ok(Self::SPHINCS_SHA2_128S),
            "Dilithium3"             => Ok(Self::Dilithium3),
            "SPHINCS+-SHA2-256f-simple" => Ok(Self::SLH_DSA_SHA2_256F),
            _ => Err(crate::error::CryptoError::UnknownAlgorithm(id.to_string())),
        }
    }

    /// Maximum public key size in bytes for this algorithm.
    pub fn pubkey_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519         => 32,
            Self::SPHINCS_SHA2_128S => 32,
            Self::Dilithium3      => 1952,
            Self::SLH_DSA_SHA2_256F => 64,
        }
    }

    /// Maximum signature size in bytes for this algorithm.
    pub fn signature_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519         => 64,
            Self::SPHINCS_SHA2_128S => 7856,
            Self::Dilithium3      => 3309,
            Self::SLH_DSA_SHA2_256F => 49_856,
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
