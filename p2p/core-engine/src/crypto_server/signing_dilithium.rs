use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::SignatureBytes;

macro_rules! pq_signing_suite {
    (
        doc_prefix: $doc_prefix:expr,
        keypair_fn: $keypair_fn:ident,
        sign_fn: $sign_fn:ident,
        verify_fn: $verify_fn:ident,
        ffi_keypair: $ffi_kp:ident,
        ffi_sign: $ffi_sign:ident,
        ffi_verify: $ffi_verify:ident
    ) => {
        #[doc = concat!("Generate a ", $doc_prefix, " keypair.")]
        #[doc = "Returns (public_key, secret_key) as variable-length byte vectors."]
        #[doc = "Secret is encrypted at C11 layer; ciphertext + nonce is returned."]
        #[must_use]
        pub fn $keypair_fn() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
            let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
            secret.plaintext_len = FORETIAS_SIG_MAX_SECRET_BYTES as usize;
            let mut public: ForetiasPubKeyVar = unsafe { std::mem::zeroed() };
            public.len = FORETIAS_SIG_MAX_PUBKEY_BYTES as usize;
            let rc = unsafe { $ffi_kp(&mut secret, &mut public) };
            c_result_to_error(rc)?;
            let ct_len = secret.plaintext_len + 16;
            let mut secret_bytes = Vec::with_capacity(ct_len + 24);
            secret_bytes.extend_from_slice(&secret.encrypted_bytes[..ct_len]);
            secret_bytes.extend_from_slice(&secret.nonce);
            let public_bytes = public.bytes[..public.len].to_vec();
            Ok((public_bytes.into(), secret_bytes.into()))
        }

        #[doc = concat!("Sign a message with ", $doc_prefix, ". Returns the signature bytes.")]
        #[doc = "Secret is encrypted ciphertext + nonce (from keypair)."]
        #[must_use]
        pub fn $sign_fn(
            secret_key: &SignatureBytes,
            msg: &[u8],
        ) -> Result<SignatureBytes, CryptoError> {
            let mut secret: ForetiasSecretKeyVar = unsafe { std::mem::zeroed() };
            let nonce_start = secret_key.len() - 24;
            let ct_len = nonce_start;
            secret.encrypted_bytes[..ct_len].copy_from_slice(&secret_key[..ct_len]);
            secret.nonce.copy_from_slice(&secret_key[nonce_start..]);
            secret.plaintext_len = ct_len - 16;
            let mut sig: ForetiasSigVar = unsafe { std::mem::zeroed() };
            sig.len = FORETIAS_SIG_MAX_SIG_BYTES as usize;
            let rc = unsafe { $ffi_sign(&secret, msg.as_ptr(), msg.len(), &mut sig) };
            c_result_to_error(rc)?;
            let sig_bytes = sig.bytes[..sig.len].to_vec();
            Ok(sig_bytes.into())
        }

        #[doc = concat!("Verify a ", $doc_prefix, " signature. Returns Ok(true) if valid, Ok(false) if invalid.")]
        #[must_use]
        pub fn $verify_fn(
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
            let rc = unsafe { $ffi_verify(&public_key_var, msg.as_ptr(), msg.len(), &sig_var) };
            if rc == 0 {
                Ok(true)
            } else if rc == -1 {
                Ok(false)
            } else {
                c_result_to_error(rc).map(|_| false)
            }
        }
    };
}

pq_signing_suite!(
    doc_prefix: "Dilithium3",
    keypair_fn: dilithium3_keypair,
    sign_fn: dilithium3_sign,
    verify_fn: dilithium3_verify,
    ffi_keypair: foretias_dilithium3_keypair,
    ffi_sign: foretias_dilithium3_sign,
    ffi_verify: foretias_dilithium3_verify
);
