//! Rust wrappers for TBID V1 dual-key identity (Ed25519 + SLH-DSA-SHA2-256f).
//!
//! TBID V1 combines an Ed25519 keypair (classic identity) with an
//! SLH-DSA-SHA2-256f keypair (post-quantum identity) into a single
//! 96-byte public key and 160-byte secret key.
//!
//! Layout:
//!   pub_key  = Ed25519_PK(32) || SLH_DSA_PK(64)   = 96 bytes
//!   secret   = Ed25519_SK(32) || SLH_DSA_SK(128)   = 160 bytes
//!   sig      = Ed25519_Sig(64) || SLH_DSA_Sig(49856) = 49920 bytes

use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::{SignatureBytes, Tbid};

/* ── TBID V1 keypair generation ──────────────────────── */

/// Generate a TBID V1 dual-key keypair.
///
/// Returns (public_key, secret_key) where:
///   - public_key  is 96 bytes: Ed25519_PK(32) || SLH_DSA_PK(64)
///   - secret_key  is 160 bytes: Ed25519_SK(32) || SLH_DSA_SK(128)
pub fn tbid_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] structs from bindings.
    let mut secret: ForetiasTbidV1SecretKey = unsafe { std::mem::zeroed() };
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut public: ForetiasTbidV1PubKey = unsafe { std::mem::zeroed() };
    // SAFETY: secret and public buffers are valid and properly sized.
    let rc = unsafe { foretias_tbid_v1_keypair(&mut secret, &mut public) };
    c_result_to_error(rc)?;

    let mut secret_bytes = vec![0u8; FORETIAS_TBID_V1_SECRET_BYTES as usize];
    secret_bytes[..32].copy_from_slice(&secret.ed25519_sk.bytes);
    secret_bytes[32..].copy_from_slice(&secret.slh_dsa_sk[..FORETIAS_TBID_V1_SLH_DSA_SK_BYTES as usize]);

    let mut public_bytes = vec![0u8; FORETIAS_TBID_V1_PUB_BYTES as usize];
    public_bytes[..32].copy_from_slice(&public.ed25519_pub.bytes);
    public_bytes[32..].copy_from_slice(&public.slh_dsa_pub[..FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES as usize]);

    // SAFETY: zeroize secret key material immediately after extraction.
    unsafe { foretias_tbid_v1_secret_zeroize(&mut secret) };

    Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
}

/* ── TBID V1 signing ─────────────────────────────────── */

/// Sign a message with TBID V1 dual-key. Returns the signature bytes.
///
/// The signature is 49,920 bytes: Ed25519_Sig(64) || SLH_DSA_Sig(49856).
pub fn tbid_sign(
    secret_key: &SignatureBytes,
    msg: &[u8],
) -> Result<SignatureBytes, CryptoError> {
    if secret_key.len() != FORETIAS_TBID_V1_SECRET_BYTES as usize {
        return Err(CryptoError::BadInput("secret key length mismatch"));
    }

    // Reconstruct ForetiasTbidV1SecretKey from raw bytes.
    let mut secret: ForetiasTbidV1SecretKey = unsafe { std::mem::zeroed() };
    secret.ed25519_sk.bytes.copy_from_slice(&secret_key[..32]);
    secret.slh_dsa_sk[..FORETIAS_TBID_V1_SLH_DSA_SK_BYTES as usize].copy_from_slice(&secret_key[32..160]);
    secret.slh_dsa_sk_len = FORETIAS_TBID_V1_SLH_DSA_SK_BYTES as usize;

    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut sig: ForetiasTbidV1Sig = unsafe { std::mem::zeroed() };
    sig.len = FORETIAS_TBID_V1_SIG_BYTES as usize;

    // SAFETY: secret, msg, sig buffers are valid and properly sized.
    let rc = unsafe { foretias_tbid_v1_sign(&secret, msg.as_ptr(), msg.len(), &mut sig) };
    c_result_to_error(rc)?;

    let sig_bytes = sig.bytes[..sig.len].to_vec();

    // SAFETY: zeroize secret key material immediately after signing.
    unsafe { foretias_tbid_v1_secret_zeroize(&mut secret) };

    Ok(sig_bytes.into())
}

/* ── TBID V1 verification ────────────────────────────── */

/// Verify a TBID V1 dual-key signature.
///
/// Returns Ok(true) if both Ed25519 and SLH-DSA signatures validate.
/// Returns Ok(false) if either signature is invalid.
pub fn tbid_verify(
    public_key: &SignatureBytes,
    msg: &[u8],
    sig: &SignatureBytes,
) -> Result<bool, CryptoError> {
    if public_key.len() != FORETIAS_TBID_V1_PUB_BYTES as usize {
        return Err(CryptoError::BadInput("public key length mismatch"));
    }
    if sig.len() < FORETIAS_TBID_V1_SIG_BYTES as usize {
        return Err(CryptoError::BadInput("signature too short"));
    }

    // Reconstruct ForetiasTbidV1PubKey from raw bytes.
    let mut public_key_struct: ForetiasTbidV1PubKey = unsafe { std::mem::zeroed() };
    public_key_struct.ed25519_pub.bytes.copy_from_slice(&public_key[..32]);
    public_key_struct.slh_dsa_pub[..FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES as usize].copy_from_slice(&public_key[32..96]);

    // Reconstruct ForetiasTbidV1Sig from raw bytes.
    let mut sig_struct: ForetiasTbidV1Sig = unsafe { std::mem::zeroed() };
    sig_struct.bytes[..sig.len()].copy_from_slice(sig);
    sig_struct.len = sig.len();

    // SAFETY: public_key_struct, msg, sig_struct buffers are valid and properly sized.
    let rc = unsafe {
        foretias_tbid_v1_verify(&public_key_struct, msg.as_ptr(), msg.len(), &sig_struct)
    };
    if rc == 0 {
        Ok(true)
    } else if rc == -1 {
        Ok(false)
    } else {
        c_result_to_error(rc).map(|_| false)
    }
}

/* ── TBID public key helpers ─────────────────────────── */

/// Extract the Ed25519 public key (first 32 bytes) from a TBID V1 public key.
pub fn tbid_ed25519_pub(public_key: &SignatureBytes) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&public_key[..32]);
    out
}

/// Extract the SLH-DSA public key (bytes 32..96) from a TBID V1 public key.
pub fn tbid_slh_dsa_pub(public_key: &SignatureBytes) -> [u8; 64] {
    let mut out = [0u8; 64];
    out.copy_from_slice(&public_key[32..96]);
    out
}

/// Convert a TBID V1 public key byte vector to a `Tbid` struct.
pub fn tbid_from_bytes(public_key: &SignatureBytes) -> Result<Tbid, CryptoError> {
    if public_key.len() != 96 {
        return Err(CryptoError::BadInput("TBID public key must be 96 bytes"));
    }
    let mut raw = [0u8; 96];
    raw.copy_from_slice(public_key);
    Ok(Tbid::from_raw(raw))
}
