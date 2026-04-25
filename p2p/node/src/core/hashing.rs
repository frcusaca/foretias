//! Safe wrappers for hashing operations.

use crate::core::bindings::*;
use crate::error::{CryptoError, c_result_to_error};

/// SHA-256 hash.
pub fn sha256(data: &[u8]) -> Result<FortiasHash32, CryptoError> {
    let mut out = unsafe { std::mem::zeroed() };
    let rc = unsafe { fortias_hash_sha256(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// SHA-256 of concatenated a || b.
pub fn sha256_concat(a: &[u8], b: &[u8]) -> Result<FortiasHash32, CryptoError> {
    let mut out = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        fortias_hash_sha256_concat(a.as_ptr(), a.len(), b.as_ptr(), b.len(), &mut out)
    };
    c_result_to_error(rc)?;
    Ok(out)
}

/// BLAKE3 hash (C11 stub returns unsupported).
pub fn blake3(data: &[u8]) -> Result<FortiasHash32, CryptoError> {
    let mut out = unsafe { std::mem::zeroed() };
    let rc = unsafe { fortias_hash_blake3(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// Legacy MD5 (noncrypto use only).
pub fn legacy_insecure_md5(data: &[u8]) -> Result<FortiasHash16, CryptoError> {
    let mut out = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        fortias_hash_legacy_insecure_md5(data.as_ptr(), data.len(), &mut out)
    };
    c_result_to_error(rc)?;
    Ok(out)
}

/// Legacy SHA-1 (noncrypto use only).
pub fn legacy_insecure_sha1(data: &[u8]) -> Result<FortiasHash20, CryptoError> {
    let mut out = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        fortias_hash_legacy_insecure_sha1(data.as_ptr(), data.len(), &mut out)
    };
    c_result_to_error(rc)?;
    Ok(out)
}
