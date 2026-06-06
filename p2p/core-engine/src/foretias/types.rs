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
        Self {
            inner: FTByteArray::zeros(),
        }
    }
}

impl Tbid {
    /// Raw 96-byte representation for wire format / storage.
    pub fn raw_bytes(&self) -> [u8; 96] {
        *self.inner
    }

    /// Parse from raw 96-byte representation (ed25519 first, then slh_dsa).
    pub fn from_raw(bytes: [u8; 96]) -> Self {
        Self {
            inner: FTByteArray::new(bytes),
        }
    }

    /// Parse from a 96-byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::CryptoError> {
        if bytes.len() != 96 {
            return Err(crate::error::CryptoError::BadInput(
                "TBID public key must be 96 bytes",
            ));
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

    /// Parse from a hex-encoded string (192 hex chars = 96 bytes).
    pub fn from_hex(s: &str) -> Result<Self, crate::error::CryptoError> {
        let bytes =
            hex::decode(s).map_err(|_| crate::error::CryptoError::BadInput("invalid hex TBID"))?;
        Self::from_bytes(&bytes)
    }

    /// Create a test TBID with all bytes set to the given value.
    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            inner: FTByteArray::new([0xAB; 96]),
        }
    }
}

/// Secret key material for a TBID (tb_version 1.0).
/// Both private keys are held together; zeroized on drop.
///
/// Layout: EncEd25519(48) ‖ NonceEd(24) ‖ EncSLH(144) ‖ NonceSLH(24) = 240 bytes total.
pub struct TbidSecret {
    /// Encrypted Ed25519 secret key (48 bytes: 32 ciphertext + 16 MAC).
    encrypted_ed25519: Zeroizing<[u8; 48]>,
    /// Ed25519 nonce for decryption (24 bytes).
    ed25519_nonce: Zeroizing<[u8; 24]>,
    /// Encrypted SLH-DSA secret key (144 bytes: 128 ciphertext + 16 MAC).
    encrypted_slh_dsa: Zeroizing<[u8; 144]>,
    /// SLH-DSA nonce for decryption (24 bytes).
    slh_dsa_nonce: Zeroizing<[u8; 24]>,
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
    pub fn sign(&self, message: &[u8]) -> Result<SignatureBytes, crate::error::CryptoError> {
        use crate::crypto_server::signing_tbid;
        let mut secret_bytes = Vec::with_capacity(240);
        secret_bytes.extend_from_slice(&self.encrypted_ed25519[..]);
        secret_bytes.extend_from_slice(&self.ed25519_nonce[..]);
        secret_bytes.extend_from_slice(&self.encrypted_slh_dsa[..]);
        secret_bytes.extend_from_slice(&self.slh_dsa_nonce[..]);
        let secret_array: [u8; 240] = secret_bytes
            .try_into()
            .map_err(|_| crate::error::CryptoError::BadInput("secret key length mismatch"))?;
        signing_tbid::tbid_sign(&SignatureBytes::from(secret_array), message)
    }

    /// Construct from flat 240-byte encrypted secret key material.
    fn from_bytes(bytes: &[u8]) -> Result<Self, crate::error::CryptoError> {
        if bytes.len() != 240 {
            return Err(crate::error::CryptoError::BadInput(
                "TBID secret must be 240 bytes",
            ));
        }
        let mut encrypted_ed25519 = [0u8; 48];
        encrypted_ed25519.copy_from_slice(&bytes[..48]);
        let mut ed25519_nonce = [0u8; 24];
        ed25519_nonce.copy_from_slice(&bytes[48..72]);
        let mut encrypted_slh_dsa = [0u8; 144];
        encrypted_slh_dsa.copy_from_slice(&bytes[72..216]);
        let mut slh_dsa_nonce = [0u8; 24];
        slh_dsa_nonce.copy_from_slice(&bytes[216..240]);
        Ok(Self {
            encrypted_ed25519: Zeroizing::new(encrypted_ed25519),
            ed25519_nonce: Zeroizing::new(ed25519_nonce),
            encrypted_slh_dsa: Zeroizing::new(encrypted_slh_dsa),
            slh_dsa_nonce: Zeroizing::new(slh_dsa_nonce),
        })
    }
}

/// Public key bytes — variable length per algorithm (32 for Ed25519, 7856 for SPHINCS+, etc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicKeyBytes(Vec<u8>);

impl PublicKeyBytes {
    /// Create from an owned vector.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Create an empty instance.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Create by cloning a slice.
    pub fn from_slice(slice: &[u8]) -> Self {
        Self(slice.to_vec())
    }

    /// View as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// View as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this instance contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Extract the owned inner vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl std::ops::Deref for PublicKeyBytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PublicKeyBytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<[u8]> for PublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for PublicKeyBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<PublicKeyBytes> for Vec<u8> {
    fn from(v: PublicKeyBytes) -> Self {
        v.0
    }
}

impl<const N: usize> From<[u8; N]> for PublicKeyBytes {
    fn from(arr: [u8; N]) -> Self {
        Self(arr.to_vec())
    }
}

impl std::ops::Index<usize> for PublicKeyBytes {
    type Output = u8;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::IndexMut<usize> for PublicKeyBytes {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl std::ops::Index<std::ops::Range<usize>> for PublicKeyBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::Range<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeTo<usize>> for PublicKeyBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeFrom<usize>> for PublicKeyBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeFull> for PublicKeyBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeFull) -> &Self::Output {
        &self.0[index]
    }
}

/// Signature bytes — variable length per algorithm (64 for Ed25519, 7856 for SPHINCS+, etc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureBytes(Vec<u8>);

impl SignatureBytes {
    /// Create from an owned vector.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Create an empty instance.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Create by cloning a slice.
    pub fn from_slice(slice: &[u8]) -> Self {
        Self(slice.to_vec())
    }

    /// View as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// View as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this instance contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Extract the owned inner vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl std::ops::Deref for SignatureBytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SignatureBytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<[u8]> for SignatureBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for SignatureBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<crate::foretias::encoding::FTByteVector> for SignatureBytes {
    fn from(v: crate::foretias::encoding::FTByteVector) -> Self {
        Self(v.into())
    }
}

impl From<SignatureBytes> for Vec<u8> {
    fn from(v: SignatureBytes) -> Self {
        v.0
    }
}

impl<const N: usize> From<[u8; N]> for SignatureBytes {
    fn from(arr: [u8; N]) -> Self {
        Self(arr.to_vec())
    }
}

impl std::ops::Index<usize> for SignatureBytes {
    type Output = u8;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::IndexMut<usize> for SignatureBytes {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl std::ops::Index<std::ops::Range<usize>> for SignatureBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::Range<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeTo<usize>> for SignatureBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeFrom<usize>> for SignatureBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::Index<std::ops::RangeFull> for SignatureBytes {
    type Output = [u8];
    fn index(&self, index: std::ops::RangeFull) -> &Self::Output {
        &self.0[index]
    }
}

impl zeroize::Zeroize for SignatureBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// Algorithm identifier as plain text (e.g. "SPHINCS+-SHA2-128s-simple").
pub type AlgorithmId = String;

/// SHA-256 hash digest — 32 bytes.
pub type Digest = [u8; 32];

/// Encrypted message payload (before/after encryption).
pub type Message = Vec<u8>;

/// Auto-attestation nonce — 16 bytes of entropy.
pub type AaNonce = [u8; 16];

/// Tick number — monotonically increasing identifier for a calendar tick.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
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
            Self::Ed25519 => "Ed25519",
            Self::SPHINCS_SHA2_128S => "SPHINCS+-SHA2-128s-simple",
            Self::Dilithium3 => "Dilithium3",
            Self::SLH_DSA_SHA2_256F => "SPHINCS+-SHA2-256f-simple",
        }
    }

    /// Parse from a liboqs algorithm identifier string.
    pub fn from_id_string(id: &str) -> Result<Self, crate::error::CryptoError> {
        match id {
            "Ed25519" => Ok(Self::Ed25519),
            "SPHINCS+-SHA2-128s-simple" => Ok(Self::SPHINCS_SHA2_128S),
            "Dilithium3" => Ok(Self::Dilithium3),
            "SPHINCS+-SHA2-256f-simple" => Ok(Self::SLH_DSA_SHA2_256F),
            _ => Err(crate::error::CryptoError::UnknownAlgorithm(id.to_string())),
        }
    }

    /// Maximum public key size in bytes for this algorithm.
    pub fn pubkey_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519 => 32,
            Self::SPHINCS_SHA2_128S => 32,
            Self::Dilithium3 => 1952,
            Self::SLH_DSA_SHA2_256F => 64,
        }
    }

    /// Maximum signature size in bytes for this algorithm.
    pub fn signature_max_bytes(&self) -> usize {
        match self {
            Self::Ed25519 => 64,
            Self::SPHINCS_SHA2_128S => 7856,
            Self::Dilithium3 => 3309,
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
            Self::NoiseXX => "Noise-XX",
            Self::MLKEM_768 => "ML-KEM-768",
        }
    }

    /// Parse from a liboqs algorithm identifier string.
    pub fn from_id_string(id: &str) -> Result<Self, crate::error::CryptoError> {
        match id {
            "Noise-XX" => Ok(Self::NoiseXX),
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
