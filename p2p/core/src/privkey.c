#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>
#include <stdlib.h>

/* Opaque private key structure — definition is private to this file.
 * Private key bytes never leave this translation unit. */
struct FortiasPrivKey {
    uint8_t       seed[32];       /* Ed25519 seed — never exposed */
    uint8_t       public_key[32]; /* Cached public key */
    FortiasCurve  curve;          /* Curve identifier */
};

FortiasPrivKey* fortias_privkey_ed25519_generate(void) {
    FortiasPrivKey *key = (FortiasPrivKey *)calloc(1, sizeof(FortiasPrivKey));
    if (!key) return NULL;

    key->curve = FORTIAS_CURVE_ED25519;

    uint8_t seed[crypto_sign_SEEDBYTES];
    randombytes_buf(seed, sizeof seed);

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        fortias_memzero(seed, sizeof seed);
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    memcpy(key->seed, seed, sizeof key->seed);
    memcpy(key->public_key, pub, sizeof key->public_key);

    fortias_memzero(sec, sizeof sec);
    fortias_memzero(seed, sizeof seed);

    return key;
}

FortiasPrivKey* fortias_privkey_ed25519_from_seed(const uint8_t seed[32]) {
    if (!seed) return NULL;

    FortiasPrivKey *key = (FortiasPrivKey *)calloc(1, sizeof(FortiasPrivKey));
    if (!key) return NULL;

    key->curve = FORTIAS_CURVE_ED25519;
    memcpy(key->seed, seed, sizeof key->seed);

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, key->seed) != 0) {
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    memcpy(key->public_key, pub, sizeof key->public_key);
    fortias_memzero(sec, sizeof sec);

    return key;
}

uint8_t* fortias_privkey_ed25519_public(const FortiasPrivKey *key, uint8_t out[32]) {
    if (!key || !out) return NULL;
    memcpy(out, key->public_key, 32);
    return out;
}

int fortias_privkey_ed25519_sign(const FortiasPrivKey *key, const uint8_t *msg, size_t msg_len, uint8_t sig[64]) {
    if (!key || !msg || !sig) return FORTIAS_ERR_BAD_INPUT;

    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, key->seed) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    unsigned char raw_sig[crypto_sign_BYTES];
    if (crypto_sign_detached(raw_sig, NULL, msg, (unsigned long long)msg_len, sec) != 0) {
        fortias_memzero(sec, sizeof sec);
        return FORTIAS_ERR_INTERNAL;
    }

    memcpy(sig, raw_sig, 64);
    fortias_memzero(sec, sizeof sec);
    fortias_memzero(raw_sig, sizeof raw_sig);

    return FORTIAS_OK;
}

FortiasResult fortias_nullifier_derive_handle(const FortiasPrivKey *key, const uint8_t *context, size_t context_len, FortiasNullifier *out) {
    if (!key || !context || !out) return FORTIAS_ERR_BAD_INPUT;

    unsigned char hmac[crypto_auth_hmacsha256_BYTES];
    crypto_auth_hmacsha256(hmac, context, (unsigned long long)context_len, key->seed);
    memcpy(out->bytes, hmac, 32);
    fortias_memzero(hmac, sizeof hmac);

    return FORTIAS_OK;
}

void fortias_privkey_free(FortiasPrivKey *key) {
    if (key) {
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
    }
}
