#include "platform.h"
#include "foretias_core.h"
#include <sodium.h>

ForetiasResult foretias_ed25519_generate_keypair(ForetiasPubKey32* pub_out, ForetiasPrivKey32* priv_out) {
    if (pub_out == NULL || priv_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    uint8_t seed[crypto_sign_SEEDBYTES];
    randombytes_buf(seed, sizeof seed);

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        sodium_memzero(seed, sizeof seed);
        return FORETIAS_ERR_INTERNAL;
    }

    memcpy(priv_out->bytes, seed, sizeof priv_out->bytes);
    memcpy(pub_out->bytes,  pub,  sizeof pub_out->bytes);

    sodium_memzero(sec, sizeof sec);
    sodium_memzero(seed, sizeof seed);

    return FORETIAS_OK;
}

ForetiasResult foretias_ed25519_derive_peer_id(const ForetiasPubKey32* pub, ForetiasPeerID* id_out) {
    if (pub == NULL || id_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    crypto_hash_sha256(id_out->bytes, pub->bytes, sizeof pub->bytes);
    return FORETIAS_OK;
}
