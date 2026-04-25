#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>

FortiasResult fortias_ed25519_sign(const FortiasPrivKey32* priv, const uint8_t* msg, size_t msg_len, FortiasSig64* sig_out) {
    if (priv == NULL || msg == NULL || sig_out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, priv->bytes) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    unsigned char sig[crypto_sign_BYTES];
    if (crypto_sign_detached(sig, NULL, msg, (unsigned long long)msg_len, sec) != 0) {
        fortias_memzero(sec, sizeof sec);
        return FORTIAS_ERR_INTERNAL;
    }

    memcpy(sig_out->bytes, sig, sizeof sig_out->bytes);
    fortias_memzero(sec, sizeof sec);
    fortias_memzero(sig, sizeof sig);

    return FORTIAS_OK;
}

FortiasResult fortias_ed25519_verify(const FortiasPubKey32* pub, const uint8_t* msg, size_t msg_len, const FortiasSig64* sig) {
    if (pub == NULL || msg == NULL || sig == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    if (crypto_sign_verify_detached(sig->bytes, msg, (unsigned long long)msg_len, pub->bytes) == 0) {
        return FORTIAS_OK;
    }

    return FORTIAS_ERR_BAD_SIG;
}
