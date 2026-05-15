//! Software crypto backend — uses C11 core + libsodium.

use std::collections::HashMap;

use chacha20poly1305::{ChaCha20Poly1305, Nonce, aead::{Aead, KeyInit}};
use zeroize::Zeroizing;

use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
use crate::crypto_server::kem_mlkem;
use crate::crypto_server::signing_sphincs;
use crate::crypto_server::signing_dilithium;
use crate::error::CryptoError;
use crate::foretias::types::{SignatureAlgorithm, SignatureBytes};
use super::{CryptoServerCapabilities, ForetiasCurve, PublicKeyBytes, SharedSecret, SealedBlob, SignOps, VerifyOps, KexOps, HashOps, SealOps, RngOps, IdentityOps, FrostOps, ProofOps};

/// Derive seal key via HKDF-SHA256 over the Ed25519 seed.
/// The seed never leaves C memory; the derivation happens inside C11.
fn derive_seal_key(handle: &PrivKeyHandle) -> Result<[u8; 32], CryptoError> {
    handle.derive_seal_key(b"foretias-calendar-seal-v1")
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
    pub sphincs_pub_key: Option<SignatureBytes>,
    sphincs_secret_key: Option<zeroize::Zeroizing<SignatureBytes>>,
    pub dilithium_pub_key: Option<SignatureBytes>,
    dilithium_secret_key: Option<zeroize::Zeroizing<SignatureBytes>>,
    pub sphincs_sha2_256f_pub_key: Option<SignatureBytes>,
    #[allow(dead_code)]
    sphincs_sha2_256f_secret_key: Option<zeroize::Zeroizing<SignatureBytes>>,
    pub mlkem_pub_key: Option<SignatureBytes>,
    #[allow(dead_code)]
    mlkem_secret_key: Option<zeroize::Zeroizing<SignatureBytes>>,
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
                let sphincs_keys = signing_sphincs::sphincs_keypair()?;
                let sphincs_pub = sphincs_keys.0;
                let sphincs_secret = sphincs_keys.1;
                let dilithium_keys = signing_dilithium::dilithium3_keypair()?;
                let dilithium_pub = dilithium_keys.0;
                let dilithium_secret = dilithium_keys.1;
                let sphincs_256f_keys = signing_sphincs::sphincs_sha2_256f_keypair()?;
                let sphincs_256f_pub = sphincs_256f_keys.0;
                let sphincs_256f_secret = sphincs_256f_keys.1;
                let mlkem_keys = kem_mlkem::mlkem_768_keypair()?;
                let mlkem_pub = mlkem_keys.0;
                let mlkem_secret = mlkem_keys.1;

                Ok(Self {
                    curve: ForetiasCurve::Ed25519,
                    pub_key: PublicKeyBytes::Ed25519(pub_key),
                    priv_key: handle,
                    peer_id,
                    seal_key: Zeroizing::new(seal_key),
                    frost_shares: parking_lot::Mutex::new(HashMap::new()),
                    sphincs_pub_key: Some(sphincs_pub),
                    sphincs_secret_key: Some(Zeroizing::new(sphincs_secret)),
                    dilithium_pub_key: Some(dilithium_pub),
                    dilithium_secret_key: Some(Zeroizing::new(dilithium_secret)),
                    sphincs_sha2_256f_pub_key: Some(sphincs_256f_pub),
                    sphincs_sha2_256f_secret_key: Some(Zeroizing::new(sphincs_256f_secret)),
                    mlkem_pub_key: Some(mlkem_pub),
                    mlkem_secret_key: Some(Zeroizing::new(mlkem_secret)),
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
        let sphincs_keys = signing_sphincs::sphincs_keypair()?;
        let sphincs_pub = sphincs_keys.0;
        let sphincs_secret = sphincs_keys.1;
        let dilithium_keys = signing_dilithium::dilithium3_keypair()?;
        let dilithium_pub = dilithium_keys.0;
        let dilithium_secret = dilithium_keys.1;
        let sphincs_256f_keys = signing_sphincs::sphincs_sha2_256f_keypair()?;
        let sphincs_256f_pub = sphincs_256f_keys.0;
        let sphincs_256f_secret = sphincs_256f_keys.1;
        let mlkem_keys = kem_mlkem::mlkem_768_keypair()?;
        let mlkem_pub = mlkem_keys.0;
        let mlkem_secret = mlkem_keys.1;

        Ok(Self {
            curve: ForetiasCurve::Ed25519,
            pub_key: PublicKeyBytes::Ed25519(pub_key),
            priv_key: handle,
            peer_id,
            seal_key: Zeroizing::new(seal_key),
            frost_shares: parking_lot::Mutex::new(HashMap::new()),
            sphincs_pub_key: Some(sphincs_pub),
            sphincs_secret_key: Some(Zeroizing::new(sphincs_secret)),
            dilithium_pub_key: Some(dilithium_pub),
            dilithium_secret_key: Some(Zeroizing::new(dilithium_secret)),
            sphincs_sha2_256f_pub_key: Some(sphincs_256f_pub),
            sphincs_sha2_256f_secret_key: Some(Zeroizing::new(sphincs_256f_secret)),
            mlkem_pub_key: Some(mlkem_pub),
            mlkem_secret_key: Some(Zeroizing::new(mlkem_secret)),
        })
    }
}

impl Drop for SoftwareCryptoServer {
    fn drop(&mut self) {
        // Zeroize seal key manually; Zeroizing wrappers on PQC secret keys
        // already handle zeroization on drop, so no need to repeat here.
        self.seal_key.fill(0);
    }
}

impl SignOps for SoftwareCryptoServer {
    fn sign(&self, msg: &[u8]) -> Result<ForetiasSig64, CryptoError> {
        crate::core::signing::ed25519_sign_with_handle(&self.priv_key, msg)
    }

    fn signature_algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::SPHINCS_SHA2_128S
    }

    fn sign_with(&self, msg: &[u8], alg: SignatureAlgorithm) -> Result<SignatureBytes, CryptoError> {
        match alg {
            SignatureAlgorithm::Ed25519 => {
                let sig = self.sign(msg)?;
                Ok(sig.bytes.to_vec().into())
            }
            SignatureAlgorithm::SPHINCS_SHA2_128S => {
                let secret = self.sphincs_secret_key.as_ref()
                    .ok_or(CryptoError::BadKey)?;
                signing_sphincs::sphincs_sign(secret, msg)
            }
            SignatureAlgorithm::Dilithium3 => {
                let secret = self.dilithium_secret_key.as_ref()
                    .ok_or(CryptoError::BadKey)?;
                signing_dilithium::dilithium3_sign(secret, msg)
            }
            SignatureAlgorithm::SLH_DSA_SHA2_256F => {
                let secret = self.sphincs_sha2_256f_secret_key.as_ref()
                    .ok_or(CryptoError::BadKey)?;
                signing_sphincs::sphincs_sha2_256f_sign(secret, msg)
            }
        }
    }
}

impl VerifyOps for SoftwareCryptoServer {
    fn verify_ed25519(&self, pub_key: &ForetiasPubKey32, msg: &[u8], sig: &ForetiasSig64) -> Result<bool, CryptoError> {
        crate::core::signing::ed25519_verify(pub_key, msg, sig)
    }

    fn verify_p256(&self, _pub_key: &ForetiasPubKey33, _msg: &[u8], _sig: &ForetiasSig64) -> Result<bool, CryptoError> {
        Err(CryptoError::Unsupported("not supported"))
    }

    fn verify_with(&self, pub_key: &[u8], alg_id: &str, msg: &[u8], sig: &[u8]) -> Result<bool, CryptoError> {
        match SignatureAlgorithm::from_id_string(alg_id)? {
            SignatureAlgorithm::Ed25519 => {
                let pk_bytes: [u8; 32] = pub_key[..32].try_into()
                    .map_err(|_| CryptoError::BadKey)?;
                let sig_bytes: [u8; 64] = sig[..64].try_into()
                    .map_err(|_| CryptoError::BadSignature)?;
                self.verify_ed25519(&ForetiasPubKey32 { bytes: pk_bytes }, msg, &ForetiasSig64 { bytes: sig_bytes })
            }
            SignatureAlgorithm::SPHINCS_SHA2_128S => {
                signing_sphincs::sphincs_verify(&SignatureBytes::from(pub_key.to_vec()), msg, &SignatureBytes::from(sig.to_vec()))
            }
            SignatureAlgorithm::Dilithium3 => {
                signing_dilithium::dilithium3_verify(&SignatureBytes::from(pub_key.to_vec()), msg, &SignatureBytes::from(sig.to_vec()))
            }
            SignatureAlgorithm::SLH_DSA_SHA2_256F => {
                signing_sphincs::sphincs_sha2_256f_verify(&SignatureBytes::from(pub_key.to_vec()), msg, &SignatureBytes::from(sig.to_vec()))
            }
        }
    }
}

impl KexOps for SoftwareCryptoServer {}

impl HashOps for SoftwareCryptoServer {
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
}

impl SealOps for SoftwareCryptoServer {
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
        Ok(SealedBlob { ciphertext: ct.into(), nonce: nonce.to_vec().into() })
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
}

impl RngOps for SoftwareCryptoServer {
    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        crate::core::rng::random_bytes(out)
    }
}

impl IdentityOps for SoftwareCryptoServer {
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
}

impl FrostOps for SoftwareCryptoServer {
    fn store_frost_share(&self, committee_id: &str, share: &[u8]) -> Result<(), CryptoError> {
        self.frost_shares.lock().insert(committee_id.to_string(), Zeroizing::new(share.to_vec()));
        Ok(())
    }

    fn frost_sign_partial(&self, _committee_id: &str, _session: &[u8]) -> Result<Vec<u8>, CryptoError> {
        Err(CryptoError::Unsupported("FROST signing not supported in software backend"))
    }
}

impl ProofOps for SoftwareCryptoServer {
    fn backend_self_proof(&self, _challenge: &[u8]) -> Result<Option<Vec<u8>>, CryptoError> {
        Err(CryptoError::Unsupported("backend self proof not supported"))
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
