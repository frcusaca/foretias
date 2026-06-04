//! Rust wrappers for TBID V1 dual-key identity (Ed25519 + SLH-DSA-SHA2-256f).
//!
//! TBID V1 combines an Ed25519 keypair (classic identity) with an
//! SLH-DSA-SHA2-256f keypair (post-quantum identity) into a single
//! 96-byte public key and 240-byte encrypted secret key.
//!
//! Layout:
//!   pub_key  = Ed25519_PK(32) || SLH_DSA_PK(64)               = 96 bytes
//!   secret   = EncEd25519(48) || NonceEd(24) || EncSLH(144) || NonceSLH(24) = 240 bytes
//!   sig      = Ed25519_Sig(64) || SLH_DSA_Sig(49856)          = 49920 bytes

use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::{SignatureBytes, Tbid};

/* ── TBID V1 keypair generation ──────────────────────── */

/// Generate a TBID V1 dual-key keypair.
///
/// Returns (public_key, secret_key) where:
///   - public_key  is 96 bytes: Ed25519_PK(32) || SLH_DSA_PK(64)
///   - secret_key  is 240 bytes: EncEd25519(48) || NonceEd(24) || EncSLH(144) || NonceSLH(24)
pub fn tbid_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] structs from bindings.
    let mut secret: ForetiasTbidV1SecretKey = unsafe { std::mem::zeroed() };
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut public: ForetiasTbidV1PubKey = unsafe { std::mem::zeroed() };
    // SAFETY: secret and public buffers are valid and properly sized.
    let rc = unsafe { foretias_tbid_v1_keypair(&mut secret, &mut public) };
    c_result_to_error(rc)?;

    // Secret is now encrypted: ed25519 (48 bytes ct + 24 nonce) + slh_dsa (144 bytes ct + 24 nonce).
    // Wrap in Zeroizing so a panic between here and the function return zeroes the heap allocation.
    // TODO(g3-f follow-up): SignatureBytes does not implement ZeroizeOnDrop, so once we hand the
    // inner Vec off below it persists in caller memory until SignatureBytes is explicitly zeroized
    // or dropped. A separate audit should add ZeroizeOnDrop to SignatureBytes (or split secret/
    // public byte types) to extend zeroize discipline beyond this function's local scope.
    let mut secret_bytes: zeroize::Zeroizing<Vec<u8>> =
        zeroize::Zeroizing::new(Vec::with_capacity(240));
    secret_bytes.extend_from_slice(&secret.encrypted_ed25519);
    secret_bytes.extend_from_slice(&secret.ed25519_nonce);
    secret_bytes.extend_from_slice(&secret.encrypted_slh_dsa);
    secret_bytes.extend_from_slice(&secret.slh_dsa_nonce);

    let mut public_bytes = vec![0u8; FORETIAS_TBID_V1_PUB_BYTES as usize];
    public_bytes[..32].copy_from_slice(&public.ed25519_pub.bytes);
    public_bytes[32..]
        .copy_from_slice(&public.slh_dsa_pub[..FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES as usize]);

    // SAFETY: zeroize secret key material immediately after extraction.
    unsafe { foretias_tbid_v1_secret_zeroize(&mut secret) };

    // Move the inner Vec out of the Zeroizing wrapper (no copy). The Zeroizing wrapper now holds
    // an empty Vec which is harmlessly zeroized when it drops.
    let secret_inner = std::mem::take(&mut *secret_bytes);
    Ok((
        SignatureBytes::from(public_bytes),
        SignatureBytes::from(secret_inner),
    ))
}

/* ── TBID V1 signing ─────────────────────────────────── */

/// Sign a message with TBID V1 dual-key. Returns the signature bytes.
///
/// The signature is 49,920 bytes: Ed25519_Sig(64) || SLH_DSA_Sig(49856).
pub fn tbid_sign(secret_key: &SignatureBytes, msg: &[u8]) -> Result<SignatureBytes, CryptoError> {
    // secret_key layout: encrypted_ed25519(48) || ed25519_nonce(24) || encrypted_slh_dsa(144) || slh_dsa_nonce(24)
    if secret_key.len() != 240 {
        return Err(CryptoError::BadInput("secret key length mismatch"));
    }

    // Reconstruct encrypted ForetiasTbidV1SecretKey from stored bytes.
    let mut secret: ForetiasTbidV1SecretKey = unsafe { std::mem::zeroed() };
    secret.encrypted_ed25519.copy_from_slice(&secret_key[..48]);
    secret.ed25519_nonce.copy_from_slice(&secret_key[48..72]);
    secret
        .encrypted_slh_dsa
        .copy_from_slice(&secret_key[72..216]);
    secret.slh_dsa_nonce.copy_from_slice(&secret_key[216..240]);
    secret.slh_dsa_plaintext_len = FORETIAS_TBID_V1_SLH_DSA_SK_BYTES as usize;

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
    public_key_struct
        .ed25519_pub
        .bytes
        .copy_from_slice(&public_key[..32]);
    public_key_struct.slh_dsa_pub[..FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES as usize]
        .copy_from_slice(&public_key[32..96]);

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
