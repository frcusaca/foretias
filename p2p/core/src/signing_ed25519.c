#include "platform.h"
#include "foretias_core.h"
#include <sodium.h>

ForetiasResult foretias_ed25519_sign(const ForetiasPrivKey32* priv, const uint8_t* msg, size_t msg_len, ForetiasSig64* sig_out) {
    if (priv == NULL || msg == NULL || sig_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, priv->bytes) != 0) {
        return FORETIAS_ERR_INTERNAL;
    }

    unsigned char sig[crypto_sign_BYTES];
    if (crypto_sign_detached(sig, NULL, msg, (unsigned long long)msg_len, sec) != 0) {
        foretias_memzero(sec, sizeof sec);
        return FORETIAS_ERR_INTERNAL;
    }

    memcpy(sig_out->bytes, sig, sizeof sig_out->bytes);
    foretias_memzero(sec, sizeof sec);
    foretias_memzero(sig, sizeof sig);

    return FORETIAS_OK;
}

ForetiasResult foretias_ed25519_verify(const ForetiasPubKey32* pub, const uint8_t* msg, size_t msg_len, const ForetiasSig64* sig) {
    if (pub == NULL || msg == NULL || sig == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    if (crypto_sign_verify_detached(sig->bytes, msg, (unsigned long long)msg_len, pub->bytes) == 0) {
        return FORETIAS_OK;
    }

    return FORETIAS_ERR_BAD_SIG;
}
