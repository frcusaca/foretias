#include "platform.h"
#include "foretias_core.h"
#include <oqs/oqs.h>

#ifdef OQS_ENABLE_SIG_sphincs_sha2_128s_simple

ForetiasResult foretias_sphincs_sha2_128s_keypair(ForetiasSecretKeyVar* secret_out, ForetiasPubKeyVar* public_out) {
    if (secret_out == NULL || public_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (secret_out->len < OQS_SIG_sphincs_sha2_128s_simple_length_secret_key ||
        public_out->len < OQS_SIG_sphincs_sha2_128s_simple_length_public_key) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_SIG_sphincs_sha2_128s_simple_keypair(public_out->bytes, secret_out->bytes);
    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(secret_out->bytes, secret_out->len);
        return FORETIAS_ERR_INTERNAL;
    }

    secret_out->len = OQS_SIG_sphincs_sha2_128s_simple_length_secret_key;
    public_out->len = OQS_SIG_sphincs_sha2_128s_simple_length_public_key;

    return FORETIAS_OK;
}

ForetiasResult foretias_sphincs_sha2_128s_sign(const ForetiasSecretKeyVar* secret, const uint8_t* msg, size_t msg_len, ForetiasSigVar* sig_out) {
    if (secret == NULL || msg == NULL || sig_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (sig_out->len < OQS_SIG_sphincs_sha2_128s_simple_length_signature) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    size_t sig_len = OQS_SIG_sphincs_sha2_128s_simple_length_signature;
    OQS_STATUS st = OQS_SIG_sphincs_sha2_128s_simple_sign(sig_out->bytes, &sig_len, msg, msg_len, secret->bytes);
    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(sig_out->bytes, sig_out->len);
        return FORETIAS_ERR_INTERNAL;
    }

    sig_out->len = sig_len;

    return FORETIAS_OK;
}

ForetiasResult foretias_sphincs_sha2_128s_verify(const ForetiasPubKeyVar* public_key, const uint8_t* msg, size_t msg_len, const ForetiasSigVar* sig) {
    if (public_key == NULL || msg == NULL || sig == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_SIG_sphincs_sha2_128s_simple_verify(msg, msg_len, sig->bytes, sig->len, public_key->bytes);
    if (st == OQS_SUCCESS) {
        return FORETIAS_OK;
    }

    return FORETIAS_ERR_BAD_SIG;
}

#endif
