//! Safe wrappers for hashing operations.

use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};

/// SHA-256 hash.
#[must_use = "hashing may fail and the error must be handled"]
pub fn sha256(data: &[u8]) -> Result<ForetiasHash32, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: data and out buffers are valid for their lengths.
    let rc = unsafe { foretias_hash_sha256(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// SHA-256 of concatenated a || b.
#[must_use = "hashing may fail and the error must be handled"]
pub fn sha256_concat(a: &[u8], b: &[u8]) -> Result<ForetiasHash32, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: a, b, out buffers are valid for their lengths.
    let rc =
        unsafe { foretias_hash_sha256_concat(a.as_ptr(), a.len(), b.as_ptr(), b.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// BLAKE3 hash (C11 stub returns unsupported).
#[must_use = "hashing may fail and the error must be handled"]
pub fn blake3(data: &[u8]) -> Result<ForetiasHash32, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: data and out buffers are valid for their lengths.
    let rc = unsafe { foretias_hash_blake3(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// Legacy MD5 (noncrypto use only).
#[must_use = "hashing may fail and the error must be handled"]
pub fn legacy_insecure_md5(data: &[u8]) -> Result<ForetiasHash16, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: data and out buffers are valid for their lengths.
    let rc = unsafe { foretias_hash_legacy_insecure_md5(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// Legacy SHA-1 (noncrypto use only).
#[must_use = "hashing may fail and the error must be handled"]
pub fn legacy_insecure_sha1(data: &[u8]) -> Result<ForetiasHash20, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: data and out buffers are valid for their lengths.
    let rc = unsafe { foretias_hash_legacy_insecure_sha1(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty_string_matches_nist_vector() {
        let hash = sha256(b"").unwrap();
        let expected: [u8; 32] =
            hex::decode("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .unwrap()
                .try_into()
                .unwrap();
        assert_eq!(hash.bytes, expected);
    }

    #[test]
    fn sha256_concat_equals_sha256_of_combined() {
        let a = b"hello ";
        let b = b"world";
        let concat_hash = sha256_concat(a, b).unwrap();
        let mut combined = Vec::new();
        combined.extend_from_slice(a);
        combined.extend_from_slice(b);
        let combined_hash = sha256(&combined).unwrap();
        assert_eq!(concat_hash.bytes, combined_hash.bytes);
    }

    #[test]
    fn sha256_concat_empty_parts() {
        let hash = sha256_concat(b"", b"").unwrap();
        let expected = sha256(b"").unwrap();
        assert_eq!(hash.bytes, expected.bytes);
    }

    #[test]
    fn sha256_concat_one_empty_part() {
        let data = b"test data";
        let h1 = sha256_concat(b"", data).unwrap();
        let h2 = sha256_concat(data, b"").unwrap();
        let h3 = sha256(data).unwrap();
        assert_eq!(h1.bytes, h3.bytes);
        assert_eq!(h2.bytes, h3.bytes);
    }

    #[test]
    fn blake3_returns_error() {
        let result = blake3(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn sha256_large_input_works() {
        let data = vec![0u8; 1_048_576];
        let hash = sha256(&data).unwrap();
        assert!(!hash.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn sha256_concat_large_inputs_works() {
        let a = vec![0xABu8; 524_288];
        let b = vec![0xCDu8; 524_288];
        let hash = sha256_concat(&a, &b).unwrap();
        assert!(!hash.bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn sha256_known_vector() {
        let hash = sha256(b"abc").unwrap();
        let expected: [u8; 32] =
            hex::decode("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
                .unwrap()
                .try_into()
                .unwrap();
        assert_eq!(hash.bytes, expected);
    }
}
