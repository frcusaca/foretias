//! Safe wrappers for Ed25519 signing operations.

use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
use crate::error::{c_result_to_error, CryptoError};

#[deprecated(
    note = "use ed25519_sign_with_handle; see HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md REQ-Z2.8"
)]
pub fn ed25519_sign(
    priv_key: &ForetiasPrivKey32,
    msg: &[u8],
) -> Result<ForetiasSig64, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut sig = unsafe { std::mem::zeroed() };
    // SAFETY: priv_key, msg, sig buffers are valid for their lengths.
    let rc = unsafe { foretias_ed25519_sign(priv_key, msg.as_ptr(), msg.len(), &mut sig) };
    c_result_to_error(rc)?;
    Ok(sig)
}

/// Sign a message using an opaque handle. Private key bytes never cross the FFI.
pub fn ed25519_sign_with_handle(
    handle: &PrivKeyHandle,
    msg: &[u8],
) -> Result<ForetiasSig64, CryptoError> {
    handle.sign(msg)
}

/// Verify an Ed25519 signature. Returns Ok(true) if valid.
pub fn ed25519_verify(
    pub_key: &ForetiasPubKey32,
    msg: &[u8],
    sig: &ForetiasSig64,
) -> Result<bool, CryptoError> {
    // SAFETY: pub_key, msg, sig buffers are valid for their lengths.
    let rc = unsafe { foretias_ed25519_verify(pub_key, msg.as_ptr(), msg.len(), sig) };
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
    use crate::core::identity::{generate_ed25519_keypair, PrivKeyHandle};

    #[test]
    fn ed25519_sign_with_handle_produces_valid_signature() {
        PrivKeyHandle::init();
        let (_pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        let handle = PrivKeyHandle::from_seed(&priv_key.bytes).unwrap();
        let msg = b"test message for signing";
        let sig = ed25519_sign_with_handle(&handle, msg).unwrap();
        assert!(!sig.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn ed25519_signatures_are_different_for_different_messages() {
        PrivKeyHandle::init();
        let (_pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        let handle = PrivKeyHandle::from_seed(&priv_key.bytes).unwrap();
        let sig1 = ed25519_sign_with_handle(&handle, b"msg1").unwrap();
        let sig2 = ed25519_sign_with_handle(&handle, b"msg2").unwrap();
        assert_ne!(sig1.bytes, sig2.bytes);
    }
}
