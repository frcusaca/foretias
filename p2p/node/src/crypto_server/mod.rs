//! CryptoServer abstraction — pluggable cryptographic backend.

// Full trait defined in v0.1.3 — stub for now.
// See FORTIAS_1_MVP_SPEC.md Part 6 for the complete trait definition.

use crate::error::CryptoError;

/// Trait for pluggable cryptographic backends.
#[allow(dead_code)]
pub trait CryptoServer: Send + Sync {
    /// Sign a message with this server's identity key.
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], CryptoError>;

    /// Verify an Ed25519 signature.
    fn verify_ed25519(&self, pub_key: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<bool, CryptoError>;

    /// SHA-256 hash.
    fn sha256(&self, data: &[u8]) -> Result<[u8; 32], CryptoError>;

    /// Generate random bytes.
    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError>;
}
