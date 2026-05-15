use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

/// Generate a Dilithium3 keypair.
/// Returns (public_key, secret_key) as variable-length byte vectors.
pub fn dilithium3_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] structs from bindings.
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    secret.len = FORETIAS_SIG_MAX_SECRET_BYTES as usize;
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut public: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
    public.len = FORETIAS_SIG_MAX_PUBKEY_BYTES as usize;
    // SAFETY: secret and public buffers are valid and properly sized.
    let rc = unsafe {
        foretias_dilithium3_keypair(&mut secret, &mut public)
    };
    c_result_to_error(rc)?;
    let secret_bytes = secret.bytes[..secret.len].to_vec();
    let public_bytes = public.bytes[..public.len].to_vec();
    Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
}

/// Sign a message with Dilithium3. Returns the signature bytes.
pub fn dilithium3_sign(secret_key: &SignatureBytes, msg: &[u8]) -> Result<SignatureBytes, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    secret.bytes[..secret_key.len()].copy_from_slice(secret_key);
    secret.len = secret_key.len();
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut sig: ForetiasSigVar = unsafe { std::mem::zeroed() };
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES as usize;
    // SAFETY: secret, msg, sig buffers are valid and properly sized.
    let rc = unsafe {
        foretias_dilithium3_sign(&secret, msg.as_ptr(), msg.len(), &mut sig)
    };
    c_result_to_error(rc)?;
    let sig_bytes = sig.bytes[..sig.len].to_vec();
    // SAFETY: secret is the ForetiasSecretKeyVar struct above; size_of matches its layout.
    unsafe {
        foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasSecretKeyVar>());
    }
    Ok(sig_bytes.into())
}

/// Verify a Dilithium3 signature. Returns Ok(true) if valid, Ok(false) if invalid.
pub fn dilithium3_verify(
    public_key: &SignatureBytes,
    msg: &[u8],
    sig: &SignatureBytes,
) -> Result<bool, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut public_key_var: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
    public_key_var.bytes[..public_key.len()].copy_from_slice(public_key);
    public_key_var.len = public_key.len();
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut sig_var: ForetiasSigVar = unsafe { std::mem::zeroed() };
    sig_var.bytes[..sig.len()].copy_from_slice(sig);
    sig_var.len = sig.len();
    // SAFETY: public_key_var, msg, sig_var buffers are valid and properly sized.
    let rc = unsafe {
        foretias_dilithium3_verify(&public_key_var, msg.as_ptr(), msg.len(), &sig_var)
    };
    if rc == 0 {
        Ok(true)
    } else if rc == -1 {
        Ok(false)
    } else {
        c_result_to_error(rc).map(|_| false)
    }
}
