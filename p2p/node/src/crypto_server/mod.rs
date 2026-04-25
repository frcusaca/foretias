//! CryptoServer abstraction — pluggable cryptographic backend.

use crate::core::bindings::*;
use crate::error::CryptoError;

/// Supported curve types.
#[derive(Debug, Clone, Copy)]
pub enum FortiasCurve {
    Ed25519,
    P256,
}

impl From<FortiasCurve> for u32 {
    fn from(curve: FortiasCurve) -> Self {
        match curve {
            FortiasCurve::Ed25519 => FortiasCurve_FORTIAS_CURVE_ED25519,
            FortiasCurve::P256 => FortiasCurve_FORTIAS_CURVE_P256,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PublicKeyBytes {
    Ed25519(FortiasPubKey32),
    P256Compressed(FortiasPubKey33),
}

#[derive(Debug, Clone)]
pub struct SharedSecret(pub [u8; 32]);

impl Drop for SharedSecret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Debug, Clone)]
pub struct SealedBlob {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    pub version: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CryptoServerCapabilities {
    pub backend_name: &'static str,
    pub curve: FortiasCurve,
    pub supports_proof: bool,
    pub supports_sealing: bool,
    pub max_sealed_bytes: usize,
    pub typical_sign_us: u32,
    pub typical_ecdh_us: u32,
}

pub trait CryptoServer: Send + Sync {
    fn public_key(&self) -> PublicKeyBytes;
    fn peer_id(&self) -> FortiasPeerID;
    fn curve(&self) -> FortiasCurve;
    fn capabilities(&self) -> CryptoServerCapabilities;

    fn sign(&self, msg: &[u8]) -> Result<FortiasSig64, CryptoError>;
    fn verify_ed25519(&self, pub_key: &FortiasPubKey32, msg: &[u8], sig: &FortiasSig64) -> Result<bool, CryptoError>;
    fn verify_p256(&self, pub_key: &FortiasPubKey33, msg: &[u8], sig: &FortiasSig64) -> Result<bool, CryptoError>;

    fn ecdh_ed25519(&self, peer_pub: &FortiasPubKey32) -> Result<SharedSecret, CryptoError>;
    fn ecdh_p256(&self, peer_pub: &FortiasPubKey33) -> Result<SharedSecret, CryptoError>;

    fn sha256(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError>;
    fn blake3(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError>;
    fn legacy_insecure_md5(&self, data: &[u8]) -> Result<FortiasHash16, CryptoError>;
    fn legacy_insecure_sha1(&self, data: &[u8]) -> Result<FortiasHash20, CryptoError>;

    fn seal_for_self(&self, data: &[u8]) -> Result<SealedBlob, CryptoError>;
    fn unseal_for_self(&self, blob: &SealedBlob) -> Result<Vec<u8>, CryptoError>;

    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError>;
    fn store_frost_share(&self, committee_id: &str, share: &[u8]) -> Result<(), CryptoError>;
    fn frost_sign_partial(&self, committee_id: &str, session_state: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn backend_self_proof(&self, challenge: &[u8]) -> Result<Option<Vec<u8>>, CryptoError>;
}

pub mod software;

pub fn new_software(curve: FortiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    Ok(Box::new(software::SoftwareCryptoServer::generate(curve)?))
}

pub fn new_best_available(curve: FortiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    new_software(curve)
}
