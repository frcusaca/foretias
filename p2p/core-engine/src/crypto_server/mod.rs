//! CryptoServer abstraction — pluggable cryptographic backend.

use serde::{Deserialize, Serialize};

use crate::core::bindings::*;
use crate::error::CryptoError;
use crate::foretias::encoding::FTByteVector;
use crate::foretias::types::{SignatureAlgorithm, SignatureBytes};

/// Supported elliptic curve types for signing and key exchange.
#[derive(Debug, Clone, Copy)]
pub enum ForetiasCurve {
    /// Ed25519 curve — the primary and fully supported curve.
    Ed25519,
    /// NIST P-256 curve — supported but currently stubbed.
    P256,
}

impl From<ForetiasCurve> for u32 {
    fn from(curve: ForetiasCurve) -> Self {
        match curve {
            ForetiasCurve::Ed25519 => ForetiasCurve_FORETIAS_CURVE_ED25519,
            ForetiasCurve::P256 => ForetiasCurve_FORETIAS_CURVE_P256,
        }
    }
}

/// Serialized public key, parameterized by curve type.
#[derive(Debug, Clone, Copy)]
pub enum PublicKeyBytes {
    /// 32-byte Ed25519 public key.
    Ed25519(ForetiasPubKey32),
    /// 33-byte compressed P-256 public key.
    P256Compressed(ForetiasPubKey33),
}

/// A 32-byte shared secret derived via ECDH; zeroized on drop.
#[derive(Debug)]
pub struct SharedSecret(pub [u8; 32]);

impl Drop for SharedSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// An encrypted blob sealed with the node's derived seal key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedBlob {
    /// The 12-byte nonce used for ChaCha20-Poly1305 encryption.
    pub nonce: FTByteVector,
    /// The encrypted payload (plaintext length + 16-byte AEAD tag).
    pub ciphertext: FTByteVector,
}

/// Describes the capabilities and performance characteristics of a crypto backend.
#[derive(Debug, Clone, Copy)]
pub struct CryptoServerCapabilities {
    /// Human-readable name of the backend (e.g. `"software"`).
    pub backend_name: &'static str,
    /// The curve this backend operates on.
    pub curve: ForetiasCurve,
    /// Whether the backend can produce a proof of authentic execution.
    pub supports_proof: bool,
    /// Whether sealing/unsealing operations are supported.
    pub supports_sealing: bool,
    /// Maximum size in bytes of data that can be sealed in a single operation.
    pub max_sealed_bytes: usize,
    /// Typical signature generation latency in microseconds.
    pub typical_sign_us: u32,
    /// Typical ECDH key exchange latency in microseconds.
    pub typical_ecdh_us: u32,
}

const NOT_SUPPORTED: &str = "CryptoServer: operation not supported by this backend";

// ─── Sub–traits ──────────────────────────────────────────────────────────────────

/// Signing operations.
pub trait SignOps: Send + Sync {
    /// Signs the given message with the server's private key.
    fn sign(&self, msg: &[u8]) -> Result<ForetiasSig64, CryptoError>;

    /// Returns the signature algorithm this server uses.
    fn signature_algorithm(&self) -> SignatureAlgorithm;

    /// Signs with the server's configured algorithm.
    fn sign_with(&self, msg: &[u8], alg: SignatureAlgorithm)
        -> Result<SignatureBytes, CryptoError>;
}

/// Verification operations.
pub trait VerifyOps: Send + Sync {
    /// Verifies an Ed25519 signature against a public key and message.
    fn verify_ed25519(&self, pub_key: &ForetiasPubKey32, msg: &[u8], sig: &ForetiasSig64)
        -> Result<bool, CryptoError>;

    /// Verifies a P-256 signature against a public key and message.
    fn verify_p256(&self, pub_key: &ForetiasPubKey33, msg: &[u8], sig: &ForetiasSig64)
        -> Result<bool, CryptoError>;

    /// Verifies a signature given the algorithm ID.
    fn verify_with(&self,
        pub_key: &[u8],
        alg_id: &str,
        msg: &[u8],
        sig: &[u8],
    ) -> Result<bool, CryptoError>;
}

/// Key-exchange (ECDH) operations.
pub trait KexOps: Send + Sync {
    /// Performs Ed25519-based ECDH to derive a shared secret with a peer.
    fn ecdh_ed25519(&self, peer_pub: &ForetiasPubKey32) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported(NOT_SUPPORTED))
    }
    /// Performs P-256 ECDH to derive a shared secret with a peer.
    fn ecdh_p256(&self, peer_pub: &ForetiasPubKey33) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported(NOT_SUPPORTED))
    }
}

/// Hashing operations.
pub trait HashOps: Send + Sync {
    /// Computes the SHA-256 hash of the given data.
    fn sha256(&self, data: &[u8]) -> Result<ForetiasHash32, CryptoError>;
    /// Computes the BLAKE3 hash of the given data.
    fn blake3(&self, data: &[u8]) -> Result<ForetiasHash32, CryptoError>;
    /// Computes MD5 hash (legacy, insecure — for compatibility only).
    fn legacy_insecure_md5(&self, data: &[u8]) -> Result<ForetiasHash16, CryptoError>;
    /// Computes SHA-1 hash (legacy, insecure — for compatibility only).
    fn legacy_insecure_sha1(&self, data: &[u8]) -> Result<ForetiasHash20, CryptoError>;
}

/// Sealing / unsealing operations (ChaCha20-Poly1305 with derived key).
pub trait SealOps: Send + Sync {
    /// Encrypts data using the server's derived seal key, returning a sealed blob.
    fn seal_for_self(&self, data: &[u8]) -> Result<SealedBlob, CryptoError>;
    /// Decrypts a sealed blob using the server's derived seal key.
    fn unseal_for_self(&self, blob: &SealedBlob) -> Result<Vec<u8>, CryptoError>;
}

/// Random-bytes generation.
pub trait RngOps: Send + Sync {
    /// Fills the output buffer with cryptographically random bytes.
    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError>;
}

/// Identity information (public key, peer id, curve, capabilities).
pub trait IdentityOps: Send + Sync {
    /// Returns the public key of this server.
    fn public_key(&self) -> PublicKeyBytes;
    /// Returns the peer ID derived from the public key.
    fn peer_id(&self) -> ForetiasPeerID;
    /// Returns the curve this server operates on.
    fn curve(&self) -> ForetiasCurve;
    /// Returns the capabilities and performance characteristics of this backend.
    fn capabilities(&self) -> CryptoServerCapabilities;
}

/// FROST threshold-signing operations.
pub trait FrostOps: Send + Sync {
    /// Stores a FROST threshold signing share for the given committee.
    fn store_frost_share(&self, committee_id: &str, share: &[u8]) -> Result<(), CryptoError>;
    /// Produces a partial FROST signature for the given committee and session.
    fn frost_sign_partial(&self, committee_id: &str, session_state: &[u8])
        -> Result<Vec<u8>, CryptoError>;
}

/// Self-proof (attestation) operations.
pub trait ProofOps: Send + Sync {
    /// Returns a proof that this backend executed the operation on a trusted device, if available.
    fn backend_self_proof(&self, challenge: &[u8]) -> Result<Option<Vec<u8>>, CryptoError>;
}

// ─── Composite trait ────────────────────────────────────────────────────────────
// Automatically satisfied by any type that implements all sub-traits.

/// Pluggable cryptographic backend for signing, verification, hashing, and key exchange.
///
/// Implementations may use software (libsodium) or hardware (secure enclave) backends.
///
/// # Blanket implementation
///
/// Any type implementing `SignOps + VerifyOps + KexOps + HashOps + SealOps + RngOps
/// + IdentityOps + FrostOps + ProofOps` automatically satisfies `CryptoServer` via
/// the blanket impl below.  Implementors should implement each sub-trait separately
/// for discoverability and to ring-fence concerns.
pub trait CryptoServer:
    SignOps + VerifyOps + KexOps + HashOps + SealOps
    + RngOps + IdentityOps + FrostOps + ProofOps
{
}

impl<T> CryptoServer for T where
    T: SignOps + VerifyOps + KexOps + HashOps + SealOps
     + RngOps + IdentityOps + FrostOps + ProofOps
{
}

pub mod software;
pub mod signing_sphincs;
pub mod signing_tbid;
pub mod kem_mlkem;
pub mod signing_dilithium;

/// Creates a software-backed crypto server using the specified curve.
pub fn new_software(curve: ForetiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    Ok(Box::new(software::SoftwareCryptoServer::generate(curve)?))
}

/// Creates the best available crypto server (currently falls back to software).
pub fn new_best_available(curve: ForetiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    new_software(curve)
}
