//! Safe wrappers for RNG and secure memory operations.

use crate::core::bindings::*;
use crate::error::{CryptoError, c_result_to_error};

/// Fill buffer with cryptographically secure random bytes.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    let rc = unsafe { fortias_rng_bytes(buf.as_mut_ptr(), buf.len()) };
    c_result_to_error(rc)?;
    Ok(())
}

/// Securely zero memory (volatile write loop).
pub fn memzero(buf: &mut [u8]) {
    unsafe { fortias_memzero(buf.as_mut_ptr() as *mut std::ffi::c_void, buf.len()) }
}
