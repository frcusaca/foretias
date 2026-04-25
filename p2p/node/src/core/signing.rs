//! Safe wrappers for Ed25519 signing operations.

use crate::core::bindings::*;
use crate::error::{CryptoError, c_result_to_error};

/// Sign a message with Ed25519.
pub fn ed25519_sign(priv_key: &FortiasPrivKey32, msg: &[u8]) -> Result<FortiasSig64, CryptoError> {
    let mut sig = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        fortias_ed25519_sign(priv_key, msg.as_ptr(), msg.len(), &mut sig)
    };
    c_result_to_error(rc)?;
    Ok(sig)
}

/// Verify an Ed25519 signature. Returns Ok(true) if valid.
pub fn ed25519_verify(
    pub_key: &FortiasPubKey32,
    msg: &[u8],
    sig: &FortiasSig64,
) -> Result<bool, CryptoError> {
    let rc = unsafe {
        fortias_ed25519_verify(pub_key, msg.as_ptr(), msg.len(), sig)
    };
    if rc == 0 {
        Ok(true)
    } else if rc == -1 {
        Ok(false)
    } else {
        c_result_to_error(rc).map(|_| false)
    }
}
