/*
 * Dilithium3 signing interface.
 *
 * REQ-Z0.4 (Zeroing split):
 *   - C layer (this file): zeroes on failure paths via OQS_MEM_cleanse.
 *     On success, the caller (Rust wrapper) owns the secret struct.
 *   - Rust layer (signing_dilithium.rs): wraps ForetiasSecretKeyVar in
 *     Zeroizing<SignatureBytes>; zeroed on drop automatically.
 */
#include "platform.h"
#include "foretias_core.h"
#include <oqs/oqs.h>

#ifdef OQS_ENABLE_SIG_dilithium_3

ForetiasResult foretias_dilithium3_keypair(ForetiasSecretKeyVar* secret_out, ForetiasPubKeyVar* public_out) {
    if (secret_out == NULL || public_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    size_t pt_len = OQS_SIG_dilithium_3_length_secret_key;

    /* 1. Generate raw keypair */
    uint8_t raw_secret[pt_len];
    OQS_STATUS st = OQS_SIG_dilithium_3_keypair(public_out->bytes, raw_secret);
    if (st != OQS_SUCCESS) {
        sodium_memzero(raw_secret, pt_len);
        return FORETIAS_ERR_INTERNAL;
    }
    public_out->len = OQS_SIG_dilithium_3_length_public_key;

    /* 2. Encrypt secret with KEK, store ciphertext + nonce */
    ForetiasResult rc = foretias_privkey_encrypt(raw_secret, pt_len,
                                                  secret_out->encrypted_bytes,
                                                  secret_out->nonce);
    sodium_memzero(raw_secret, pt_len);
    if (rc != FORETIAS_OK) {
        return FORETIAS_ERR_INTERNAL;
    }
    secret_out->plaintext_len = pt_len;

    return FORETIAS_OK;
}

ForetiasResult foretias_dilithium3_sign(const ForetiasSecretKeyVar* secret, const uint8_t* msg, size_t msg_len, ForetiasSigVar* sig_out) {
    if (secret == NULL || msg == NULL || sig_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (sig_out->len < OQS_SIG_dilithium_3_length_signature) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    /* Decrypt secret to stack buffer, sign, then zero */
    uint8_t raw_secret[secret->plaintext_len];
    ForetiasResult rc = foretias_privkey_decrypt(secret->encrypted_bytes,
                                                  secret->plaintext_len + crypto_secretbox_MACBYTES,
                                                  secret->nonce, raw_secret);
    if (rc != FORETIAS_OK) {
        return FORETIAS_ERR_INTERNAL;
    }

    size_t sig_len = OQS_SIG_dilithium_3_length_signature;
    OQS_STATUS st = OQS_SIG_dilithium_3_sign(sig_out->bytes, &sig_len, msg, msg_len, raw_secret);
    sodium_memzero(raw_secret, sizeof(raw_secret));

    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(sig_out->bytes, sig_out->len);
        return FORETIAS_ERR_INTERNAL;
    }

    sig_out->len = sig_len;

    return FORETIAS_OK;
}

ForetiasResult foretias_dilithium3_verify(const ForetiasPubKeyVar* public_key, const uint8_t* msg, size_t msg_len, const ForetiasSigVar* sig) {
    if (public_key == NULL || msg == NULL || sig == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_SIG_dilithium_3_verify(msg, msg_len, sig->bytes, sig->len, public_key->bytes);
    if (st == OQS_SUCCESS) {
        return FORETIAS_OK;
    }

    return FORETIAS_ERR_BAD_SIG;
}

#endif
