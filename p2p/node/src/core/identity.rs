//! Safe wrappers for Ed25519 identity operations.

use crate::core::bindings::*;
use crate::error::{CryptoError, c_result_to_error};

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
