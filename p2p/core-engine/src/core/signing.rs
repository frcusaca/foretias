//! Safe wrappers for Ed25519 signing operations.

use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
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

/// Sign a message using an opaque handle. Private key bytes never cross the FFI.
pub fn ed25519_sign_with_handle(handle: &PrivKeyHandle, msg: &[u8]) -> Result<FortiasSig64, CryptoError> {
    handle.sign(msg)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::identity::generate_ed25519_keypair;

    #[test]
    fn ed25519_sign_produces_valid_signature() {
        let (_pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        let msg = b"test message for signing";
        let sig = ed25519_sign(&priv_key, msg).unwrap();
        assert!(!sig.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn ed25519_signatures_are_different_for_different_messages() {
        let (_pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        let sig1 = ed25519_sign(&priv_key, b"msg1").unwrap();
        let sig2 = ed25519_sign(&priv_key, b"msg2").unwrap();
        assert_ne!(sig1.bytes, sig2.bytes);
    }
}
