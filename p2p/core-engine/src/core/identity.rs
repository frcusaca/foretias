//! Safe wrappers for Ed25519 identity operations.

use std::ptr::NonNull;
use std::mem::ManuallyDrop;

use crate::core::bindings::*;
use crate::error::{CryptoError, c_result_to_error};

/// Opaque private key handle — private key bytes never leave C memory.
///
/// This type does NOT implement `Clone`, `Copy`, or `Debug`, preventing
/// accidental key exposure through copies or debug output.
pub struct PrivKeyHandle(ManuallyDrop<NonNull<FortiasPrivKey>>);

unsafe impl Send for PrivKeyHandle {}
unsafe impl Sync for PrivKeyHandle {}

impl PrivKeyHandle {
    /// Generate a fresh Ed25519 keypair, returning an opaque handle.
    pub fn generate() -> Result<Self, CryptoError> {
        let ptr = unsafe { fortias_privkey_ed25519_generate() };
        if ptr.is_null() {
            return Err(CryptoError::Internal(-99));
        }
        Ok(Self(ManuallyDrop::new(NonNull::new(ptr).unwrap())))
    }

    /// Create a handle from an existing 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self, CryptoError> {
        let ptr = unsafe { fortias_privkey_ed25519_from_seed(seed.as_ptr()) };
        if ptr.is_null() {
            return Err(CryptoError::Internal(-99));
        }
        Ok(Self(ManuallyDrop::new(NonNull::new(ptr).unwrap())))
    }

    /// Derive the public key for this handle.
    pub fn public_key(&self) -> Result<[u8; 32], CryptoError> {
        let mut out = [0u8; 32];
        unsafe {
            fortias_privkey_ed25519_public(self.0.as_ptr(), out.as_mut_ptr());
        }
        Ok(out)
    }

    /// Sign a message with this handle. Private key bytes never leave C.
    pub fn sign(&self, msg: &[u8]) -> Result<FortiasSig64, CryptoError> {
        let mut sig = [0u8; 64];
        let rc = unsafe {
            fortias_privkey_ed25519_sign(
                self.0.as_ptr(),
                msg.as_ptr(),
                msg.len(),
                sig.as_mut_ptr(),
            )
        };
        c_result_to_error(rc as i32)?;
        Ok(FortiasSig64 { bytes: sig })
    }
}

impl Drop for PrivKeyHandle {
    fn drop(&mut self) {
        unsafe {
            fortias_privkey_free(self.0.as_ptr());
        }
    }
}

/// Generate a fresh Ed25519 keypair.
pub fn generate_ed25519_keypair() -> Result<(FortiasPubKey32, FortiasPrivKey32), CryptoError> {
    let mut pub_key = unsafe { std::mem::zeroed() };
    let mut priv_key = unsafe { std::mem::zeroed() };
    let rc = unsafe { fortias_ed25519_generate_keypair(&mut pub_key, &mut priv_key) };
    c_result_to_error(rc)?;
    Ok((pub_key, priv_key))
}

/// Derive a PeerID from an Ed25519 public key.
pub fn derive_ed25519_peer_id(pub_key: &FortiasPubKey32) -> Result<FortiasPeerID, CryptoError> {
    let mut peer_id = unsafe { std::mem::zeroed() };
    let rc = unsafe { fortias_ed25519_derive_peer_id(pub_key, &mut peer_id) };
    c_result_to_error(rc)?;
    Ok(peer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_keypair_produces_nonzero_keys() {
        let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        assert!(!pub_key.bytes.iter().all(|&b| b == 0));
        assert!(!priv_key.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn generate_keypair_keys_are_different() {
        let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        assert_ne!(pub_key.bytes, priv_key.bytes);
    }

    #[test]
    fn derive_peer_id_is_deterministic() {
        let (pub_key, _priv_key) = generate_ed25519_keypair().unwrap();
        let id1 = derive_ed25519_peer_id(&pub_key).unwrap();
        let id2 = derive_ed25519_peer_id(&pub_key).unwrap();
        assert_eq!(id1.bytes, id2.bytes);
    }

    #[test]
    fn different_keypairs_produce_different_peer_ids() {
        let (pub1, _priv1) = generate_ed25519_keypair().unwrap();
        let (pub2, _priv2) = generate_ed25519_keypair().unwrap();
        let id1 = derive_ed25519_peer_id(&pub1).unwrap();
        let id2 = derive_ed25519_peer_id(&pub2).unwrap();
        assert_ne!(id1.bytes, id2.bytes);
    }

    #[test]
    fn derive_peer_id_produces_nonzero_id() {
        let (pub_key, _priv_key) = generate_ed25519_keypair().unwrap();
        let peer_id = derive_ed25519_peer_id(&pub_key).unwrap();
        assert!(!peer_id.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn multiple_keypairs_are_unique() {
        let mut peer_ids = std::collections::HashSet::new();
        for _ in 0..10 {
            let (pub_key, _priv_key) = generate_ed25519_keypair().unwrap();
            let id = derive_ed25519_peer_id(&pub_key).unwrap();
            assert!(peer_ids.insert(id.bytes));
        }
    }
}
