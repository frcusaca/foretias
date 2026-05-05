use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

/// Generate a SPHINCS+ SHA2-128s-simple keypair.
/// Returns (public_key, secret_key) as variable-length byte vectors.
pub fn sphincs_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    secret.len = FORETIAS_SIG_MAX_SECRET_BYTES as usize;
    let mut public: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
    public.len = FORETIAS_SIG_MAX_PUBKEY_BYTES as usize;
    let rc = unsafe {
        foretias_sphincs_sha2_128s_keypair(&mut secret, &mut public)
    };
    c_result_to_error(rc)?;
    let secret_bytes = secret.bytes[..secret.len].to_vec();
    let public_bytes = public.bytes[..public.len].to_vec();
    Ok((public_bytes, secret_bytes))
}

/// Sign a message with SPHINCS+. Returns the signature bytes.
pub fn sphincs_sign(secret_key: &SignatureBytes, msg: &[u8]) -> Result<SignatureBytes, CryptoError> {
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    secret.bytes[..secret_key.len()].copy_from_slice(secret_key);
    secret.len = secret_key.len();
    let mut sig: ForetiasSigVar = unsafe { std::mem::zeroed() };
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES as usize;
    let rc = unsafe {
        foretias_sphincs_sha2_128s_sign(&secret, msg.as_ptr(), msg.len(), &mut sig)
    };
    c_result_to_error(rc)?;
    let sig_bytes = sig.bytes[..sig.len].to_vec();
    unsafe {
        foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasSecretKeyVar>());
    }
    Ok(sig_bytes)
}

/// Verify a SPHINCS+ signature. Returns Ok(true) if valid, Ok(false) if invalid.
pub fn sphincs_verify(
    public_key: &SignatureBytes,
    msg: &[u8],
    sig: &SignatureBytes,
) -> Result<bool, CryptoError> {
    let mut public_key_var: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
    public_key_var.bytes[..public_key.len()].copy_from_slice(public_key);
    public_key_var.len = public_key.len();
    let mut sig_var: ForetiasSigVar = unsafe { std::mem::zeroed() };
    sig_var.bytes[..sig.len()].copy_from_slice(sig);
    sig_var.len = sig.len();
    let rc = unsafe {
        foretias_sphincs_sha2_128s_verify(&public_key_var, msg.as_ptr(), msg.len(), &sig_var)
    };
    if rc == 0 {
        Ok(true)
    } else if rc == -1 {
        Ok(false)
    } else {
        c_result_to_error(rc).map(|_| false)
    }
}
