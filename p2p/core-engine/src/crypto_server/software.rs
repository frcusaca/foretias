//! Software crypto backend — uses C11 core + libsodium.

use std::collections::HashMap;

use chacha20poly1305::{ChaCha20Poly1305, Nonce, aead::{Aead, KeyInit}};
use zeroize::Zeroizing;

use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
use crate::error::CryptoError;
use super::{CryptoServer, CryptoServerCapabilities, ForetiasCurve, PublicKeyBytes, SharedSecret, SealedBlob};

/// Derive seal key via HKDF-SHA256 over the Ed25519 seed.
/// The seed never leaves C memory; the derivation happens inside C11.
fn derive_seal_key(handle: &PrivKeyHandle) -> Result<[u8; 32], CryptoError> {
    handle.derive_seal_key(b"fortias-calendar-seal-v1")
}

/// Software-based crypto server backed by libsodium and ChaCha20-Poly1305.
pub struct SoftwareCryptoServer {
    /// The curve this server operates on.
    curve: ForetiasCurve,
    /// The public key generated at construction time.
    pub_key: PublicKeyBytes,
    /// The opaque private key handle — bytes never leave C memory.
    priv_key: PrivKeyHandle,
    /// The peer ID derived from the public key.
    peer_id: ForetiasPeerID,
    /// Derived seal key for ChaCha20-Poly1305 encryption, zeroized on drop.
    seal_key: Zeroizing<[u8; 32]>,
    /// In-memory store for FROST threshold signing shares.
    frost_shares: parking_lot::Mutex<HashMap<String, Zeroizing<Vec<u8>>>>,
}

impl SoftwareCryptoServer {
    /// Generates a new keypair and initializes a software crypto server for the given curve.
    pub fn generate(curve: ForetiasCurve) -> Result<Self, CryptoError> {
        PrivKeyHandle::init();
        match curve {
            ForetiasCurve::Ed25519 => {
                let handle = PrivKeyHandle::generate()?;
                let pub_key_bytes: [u8; 32] = handle.public_key()?;
                let pub_key = ForetiasPubKey32 { bytes: pub_key_bytes };
                let peer_id = crate::core::identity::derive_ed25519_peer_id(&pub_key)?;
                let seal_key = derive_seal_key(&handle)?;

                Ok(Self {
                    curve: ForetiasCurve::Ed25519,
                    pub_key: PublicKeyBytes::Ed25519(pub_key),
                    priv_key: handle,
                    peer_id,
                    seal_key: Zeroizing::new(seal_key),
                    frost_shares: parking_lot::Mutex::new(HashMap::new()),
                })
            }
            ForetiasCurve::P256 => {
                Err(CryptoError::Unsupported("P-256 not implemented yet"))
            }
        }
    }

    pub fn from_seed(seed: &[u8; 32]) -> Result<Self, CryptoError> {
        PrivKeyHandle::init();
        let handle = PrivKeyHandle::from_seed(seed)?;
        let pub_key_bytes: [u8; 32] = handle.public_key()?;
        let pub_key = ForetiasPubKey32 { bytes: pub_key_bytes };
        let peer_id = crate::core::identity::derive_ed25519_peer_id(&pub_key)?;
        let seal_key = derive_seal_key(&handle)?;

        Ok(Self {
            curve: ForetiasCurve::Ed25519,
            pub_key: PublicKeyBytes::Ed25519(pub_key),
            priv_key: handle,
            peer_id,
            seal_key: Zeroizing::new(seal_key),
            frost_shares: parking_lot::Mutex::new(HashMap::new()),
        })
    }
}

impl Drop for SoftwareCryptoServer {
    fn drop(&mut self) {
        self.seal_key.fill(0);
    }
}

impl CryptoServer for SoftwareCryptoServer {
    fn public_key(&self) -> PublicKeyBytes { self.pub_key }
    fn peer_id(&self) -> ForetiasPeerID { self.peer_id }
    fn curve(&self) -> ForetiasCurve { self.curve }

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

    fn sign(&self, msg: &[u8]) -> Result<ForetiasSig64, CryptoError> {
        crate::core::signing::ed25519_sign_with_handle(&self.priv_key, msg)
    }

    fn verify_ed25519(&self, pub_key: &ForetiasPubKey32, msg: &[u8], sig: &ForetiasSig64) -> Result<bool, CryptoError> {
        crate::core::signing::ed25519_verify(pub_key, msg, sig)
    }

    fn verify_p256(&self, _pub_key: &ForetiasPubKey33, _msg: &[u8], _sig: &ForetiasSig64) -> Result<bool, CryptoError> {
        Err(CryptoError::Unsupported("P-256 verify not implemented"))
    }

    fn ecdh_ed25519(&self, _peer_pub: &ForetiasPubKey32) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported("ECDH stub"))
    }

    fn ecdh_p256(&self, _peer_pub: &ForetiasPubKey33) -> Result<SharedSecret, CryptoError> {
        Err(CryptoError::Unsupported("P-256 ECDH not implemented"))
    }

    fn sha256(&self, data: &[u8]) -> Result<ForetiasHash32, CryptoError> {
        crate::core::hashing::sha256(data)
    }

    fn blake3(&self, data: &[u8]) -> Result<ForetiasHash32, CryptoError> {
        crate::core::hashing::blake3(data)
    }

    fn legacy_insecure_md5(&self, data: &[u8]) -> Result<ForetiasHash16, CryptoError> {
        crate::core::hashing::legacy_insecure_md5(data)
    }

    fn legacy_insecure_sha1(&self, data: &[u8]) -> Result<ForetiasHash20, CryptoError> {
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
        Ok(SealedBlob { ciphertext: ct, nonce: nonce.to_vec() })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unseal_roundtrip() {
        let server = SoftwareCryptoServer::generate(ForetiasCurve::Ed25519).unwrap();
        let plaintext = b"the quick brown fox jumps over the lazy calendar";
        let blob = server.seal_for_self(plaintext).unwrap();
        assert!(!blob.ciphertext.is_empty());
        assert_eq!(blob.nonce.len(), 12);
        let decrypted = server.unseal_for_self(&blob).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn seal_wrong_key_fails() {
        let server_a = SoftwareCryptoServer::generate(ForetiasCurve::Ed25519).unwrap();
        let server_b = SoftwareCryptoServer::generate(ForetiasCurve::Ed25519).unwrap();
        let plaintext = b"secret message for server a only";
        let blob = server_a.seal_for_self(plaintext).unwrap();
        let result = server_b.unseal_for_self(&blob);
        assert!(result.is_err());
    }
}
