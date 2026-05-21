use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

/// Generate a Dilithium3 keypair.
/// Returns (public_key, secret_key) as variable-length byte vectors.
/// Secret is encrypted at C11 layer; ciphertext + nonce is returned.
pub fn dilithium3_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    secret.plaintext_len = FORETIAS_SIG_MAX_SECRET_BYTES as usize;
    let mut public: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
    public.len = FORETIAS_SIG_MAX_PUBKEY_BYTES as usize;
    let rc = unsafe {
        foretias_dilithium3_keypair(&mut secret, &mut public)
    };
    c_result_to_error(rc)?;
    let ct_len = secret.plaintext_len + 16;
    let mut secret_bytes = Vec::with_capacity(ct_len + 24);
    secret_bytes.extend_from_slice(&secret.encrypted_bytes[..ct_len]);
    secret_bytes.extend_from_slice(&secret.nonce);
    let public_bytes = public.bytes[..public.len].to_vec();
    Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
}

/// Sign a message with Dilithium3. Returns the signature bytes.
/// Secret is encrypted ciphertext + nonce (from dilithium3_keypair).
pub fn dilithium3_sign(secret_key: &SignatureBytes, msg: &[u8]) -> Result<SignatureBytes, CryptoError> {
    let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
    let nonce_start = secret_key.len() - 24;
    let ct_len = nonce_start;
    secret.encrypted_bytes[..ct_len].copy_from_slice(&secret_key[..ct_len]);
    secret.nonce.copy_from_slice(&secret_key[nonce_start..]);
    secret.plaintext_len = ct_len - 16;
    let mut sig: ForetiasSigVar = unsafe { std::mem::zeroed() };
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES as usize;
    let rc = unsafe {
        foretias_dilithium3_sign(&secret, msg.as_ptr(), msg.len(), &mut sig)
    };
    c_result_to_error(rc)?;
    let sig_bytes = sig.bytes[..sig.len].to_vec();
    Ok(sig_bytes.into())
}

/// Verify a Dilithium3 signature. Returns Ok(true) if valid, Ok(false) if invalid.
pub fn dilithium3_verify(
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
