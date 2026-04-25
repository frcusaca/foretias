//! Software crypto backend — uses C11 core + libsodium.

use std::collections::HashMap;

use chacha20poly1305::{ChaCha20Poly1305, Nonce, aead::{Aead, KeyInit}};
use zeroize::Zeroizing;

use crate::core::bindings::*;
use crate::error::CryptoError;
use super::{CryptoServer, CryptoServerCapabilities, FortiasCurve, PublicKeyBytes, SharedSecret, SealedBlob};

/// Derive seal key via HMAC-SHA256(priv_key, "fortias-seal-v1").
fn derive_seal_key(priv_bytes: &[u8; 32]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(priv_bytes)
        .expect("HMAC key length is always valid");
    mac.update(b"fortias-seal-v1");
    let result = mac.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result.into_bytes());
    key
}

pub struct SoftwareCryptoServer {
    curve: FortiasCurve,
    pub_key: PublicKeyBytes,
    priv_key: Zeroizing<[u8; 32]>,
    peer_id: FortiasPeerID,
    seal_key: Zeroizing<[u8; 32]>,
    frost_shares: parking_lot::Mutex<HashMap<String, Zeroizing<Vec<u8>>>>,
}

impl SoftwareCryptoServer {
    pub fn generate(curve: FortiasCurve) -> Result<Self, CryptoError> {
        match curve {
            FortiasCurve::Ed25519 => {
                let (pub_key_bytes, priv_key_bytes) = crate::core::identity::generate_ed25519_keypair()?;
                let peer_id = crate::core::identity::derive_ed25519_peer_id(&pub_key_bytes)?;
                let seal_key = derive_seal_key(&priv_key_bytes.bytes);

                Ok(Self {
                    curve: FortiasCurve::Ed25519,
                    pub_key: PublicKeyBytes::Ed25519(pub_key_bytes),
                    priv_key: Zeroizing::new(priv_key_bytes.bytes),
                    peer_id,
                    seal_key: Zeroizing::new(seal_key),
                    frost_shares: parking_lot::Mutex::new(HashMap::new()),
                })
            }
            FortiasCurve::P256 => {
                Err(CryptoError::Unsupported("P-256 not implemented yet"))
            }
        }
    }
}

impl Drop for SoftwareCryptoServer {
    fn drop(&mut self) {
        self.priv_key.fill(0);
        self.seal_key.fill(0);
    }
}

impl CryptoServer for SoftwareCryptoServer {
    fn public_key(&self) -> PublicKeyBytes { self.pub_key }
    fn peer_id(&self) -> FortiasPeerID { self.peer_id }
    fn curve(&self) -> FortiasCurve { self.curve }

    fn capabilities(&self) -> CryptoServerCapabilities {
        CryptoServerCapabilities {
            backend_name: "software",
            curve: self.curve,
            supports_proof: false,
            supports_sealing: true,
            max_sealed_bytes: 1_048_576,
            typical_sign_us: 50,
            typical_ecdh_us: 100,
        }
    }

    fn sign(&self, msg: &[u8]) -> Result<FortiasSig64, CryptoError> {
        crate::core::signing::ed25519_sign(&FortiasPrivKey32 { bytes: *self.priv_key }, msg)
    }

    fn verify_ed25519(&self, pub_key: &FortiasPubKey32, msg: &[u8], sig: &FortiasSig64) -> Result<bool, CryptoError> {
        crate::core::signing::ed25519_verify(pub_key, msg, sig)
    }

    fn verify_p256(&self, _pub_key: &FortiasPubKey33, _msg: &[u8], _sig: &FortiasSig64) -> Result<bool, CryptoError> {
        Err(CryptoError::Unsupported("P-256 verify not implemented"))
    }

    fn ecdh_ed25519(&self, _peer_pub: &FortiasPubKey32) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported("ECDH stub"))
    }

    fn ecdh_p256(&self, _peer_pub: &FortiasPubKey33) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported("P-256 ECDH not implemented"))
    }

    fn sha256(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError> {
        crate::core::hashing::sha256(data)
    }

    fn blake3(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError> {
        crate::core::hashing::blake3(data)
    }

    fn legacy_insecure_md5(&self, data: &[u8]) -> Result<FortiasHash16, CryptoError> {
        crate::core::hashing::legacy_insecure_md5(data)
    }

    fn legacy_insecure_sha1(&self, data: &[u8]) -> Result<FortiasHash20, CryptoError> {
        crate::core::hashing::legacy_insecure_sha1(data)
    }

    fn seal_for_self(&self, data: &[u8]) -> Result<SealedBlob, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&*self.seal_key)
            .map_err(|_| CryptoError::Internal(1))?;
        let mut nonce = [0u8; 12];
        crate::core::rng::random_bytes(&mut nonce)?;
        let ct = cipher.encrypt(
            Nonce::from_slice(&nonce),
            data.as_ref(),
        )
            .map_err(|_| CryptoError::Internal(1))?;
        Ok(SealedBlob { ciphertext: ct, nonce, version: 1 })
    }

    fn unseal_for_self(&self, blob: &SealedBlob) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&*self.seal_key)
            .map_err(|_| CryptoError::Internal(1))?;
        let pt = cipher.decrypt(
            Nonce::from_slice(&blob.nonce),
            blob.ciphertext.as_ref(),
        )
            .map_err(|_| CryptoError::BadSignature)?;
        Ok(pt)
    }

    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        crate::core::rng::random_bytes(out)
    }

    fn store_frost_share(&self, committee_id: &str, share: &[u8]) -> Result<(), CryptoError> {
        let mut map = self.frost_shares.lock();
        map.insert(committee_id.to_string(), Zeroizing::new(share.to_vec()));
        Ok(())
    }

    fn frost_sign_partial(&self, _committee_id: &str, _session_state: &[u8]) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::Unsupported("FROST signing stub"))
    }

    fn backend_self_proof(&self, _challenge: &[u8]) -> Result<Option<Vec<u8>>, CryptoError> {
        Ok(None)
    }
}
