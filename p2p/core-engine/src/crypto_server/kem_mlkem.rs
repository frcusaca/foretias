use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

pub fn mlkem_768_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] structs from bindings.
    let mut secret: ForetiasKemSecretKey = unsafe { std::mem::zeroed() };
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut public: ForetiasKemPubKey = unsafe { std::mem::zeroed() };
    // SAFETY: secret and public buffers are valid and properly sized.
    let rc = unsafe {
        foretias_mlkem_768_keypair(&mut secret, &mut public)
    };
    c_result_to_error(rc)?;
    let secret_bytes = secret.bytes[..secret.len as usize].to_vec();
    let public_bytes = public.bytes[..public.len as usize].to_vec();
    Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
}

pub fn mlkem_768_encapsulate(public_key: &SignatureBytes) -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut pk: ForetiasKemPubKey = unsafe { std::mem::zeroed() };
    pk.bytes[..public_key.len()].copy_from_slice(public_key);
    pk.len = public_key.len() as usize;
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut ct: ForetiasKemCiphertext = unsafe { std::mem::zeroed() };
    let mut ss: [u8; 32] = [0u8; 32];
    // SAFETY: pk, ct, ss buffers are valid and properly sized.
    let rc = unsafe {
        foretias_mlkem_768_encapsulate(&pk, &mut ct, ss.as_mut_ptr())
    };
    c_result_to_error(rc)?;
    let ct_bytes = ct.bytes[..ct.len as usize].to_vec();
    Ok((SignatureBytes::from(ct_bytes), SignatureBytes::from(ss.to_vec())))
}

pub fn mlkem_768_decapsulate(secret_key: &SignatureBytes, ciphertext: &SignatureBytes) -> Result<SignatureBytes, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut sk: ForetiasKemSecretKey = unsafe { std::mem::zeroed() };
    sk.bytes[..secret_key.len()].copy_from_slice(secret_key);
    sk.len = secret_key.len() as usize;
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut ct: ForetiasKemCiphertext = unsafe { std::mem::zeroed() };
    ct.bytes[..ciphertext.len()].copy_from_slice(ciphertext);
    ct.len = ciphertext.len() as usize;
    let mut ss: [u8; 32] = [0u8; 32];
    // SAFETY: sk, ct, ss buffers are valid and properly sized.
    let rc = unsafe {
        foretias_mlkem_768_decapsulate(&sk, &ct, ss.as_mut_ptr())
    };
    c_result_to_error(rc)?;
    // SAFETY: sk is the ForetiasKemSecretKey struct above; size_of matches its layout.
    unsafe {
        foretias_memzero(&mut sk as *mut _ as *mut _, std::mem::size_of::<ForetiasKemSecretKey>());
    }
    Ok(ss.to_vec().into())
}
