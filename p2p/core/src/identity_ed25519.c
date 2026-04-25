#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>

FortiasResult fortias_ed25519_generate_keypair(FortiasPubKey32* pub_out, FortiasPrivKey32* priv_out) {
    if (pub_out == NULL || priv_out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    uint8_t seed[crypto_sign_SEEDBYTES];
    randombytes_buf(seed, sizeof seed);

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        fortias_memzero(seed, sizeof seed);
        return FORTIAS_ERR_INTERNAL;
    }

    memcpy(priv_out->bytes, seed, sizeof priv_out->bytes);
    memcpy(pub_out->bytes,  pub,  sizeof pub_out->bytes);

    fortias_memzero(sec, sizeof sec);
    fortias_memzero(seed, sizeof seed);

    return FORTIAS_OK;
}

FortiasResult fortias_ed25519_derive_peer_id(const FortiasPubKey32* pub, FortiasPeerID* id_out) {
    if (pub == NULL || id_out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    crypto_hash_sha256(id_out->bytes, pub->bytes, sizeof pub->bytes);
    return FORTIAS_OK;
}
