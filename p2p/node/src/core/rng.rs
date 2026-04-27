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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_produces_exact_length() {
        let mut buf = vec![0u8; 64];
        random_bytes(&mut buf).unwrap();
        assert_eq!(buf.len(), 64);
    }

    #[test]
    fn random_bytes_produces_exact_length_small() {
        let mut buf = [0u8; 1];
        random_bytes(&mut buf).unwrap();
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn random_bytes_produces_exact_length_large() {
        let mut buf = vec![0u8; 4096];
        random_bytes(&mut buf).unwrap();
        assert_eq!(buf.len(), 4096);
    }

    #[test]
    fn two_random_calls_produce_different_output() {
        let mut buf1 = vec![0u8; 32];
        let mut buf2 = vec![0u8; 32];
        random_bytes(&mut buf1).unwrap();
        random_bytes(&mut buf2).unwrap();
        assert_ne!(buf1, buf2);
    }

    #[test]
    fn random_bytes_are_not_zero() {
        let mut buf = vec![0u8; 256];
        random_bytes(&mut buf).unwrap();
        assert!(!buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn memzero_zeroes_memory() {
        let mut buf = vec![0xABu8; 64];
        memzero(&mut buf);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn memzero_zero_length_is_safe() {
        let mut buf: Vec<u8> = vec![];
        memzero(&mut buf);
    }

    #[test]
    fn memzero_single_byte() {
        let mut buf = [0xFFu8; 1];
        memzero(&mut buf);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn random_bytes_empty_buffer_is_safe() {
        let mut buf: Vec<u8> = vec![];
        let result = random_bytes(&mut buf);
        assert!(result.is_ok());
    }
}
