#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>
#include <stdlib.h>

/* ── Instance KEK (Key Encryption Key) ──────────────────────────────────
 *
 * A 32-byte random key generated once per process. Used as the secret
 * for ChaCha20-Poly1305 (libsodium crypto_secretbox) to encrypt each
 * private key's seed at rest. The KEK never leaves this translation unit.
 *
 * Even if an attacker dumps the encrypted key handle from memory, they
 * cannot recover the seed without this KEK — which is process-local and
 * zeroized on cleanup.
 * ─────────────────────────────────────────────────────────────────────── */

static uint8_t  instance_kek[32];
static bool     instance_kek_initialized = false;

/* Monotonic counter for nonce derivation — ensures each key gets a
 * unique nonce even if generated at the same clock tick. */
static uint64_t key_gen_counter = 0;

/* ── Helper: derive nonce from counter via SHA-256 ───────────────────── */
static void derive_nonce(uint8_t nonce[24], uint64_t counter) {
    uint8_t hash[crypto_hash_sha256_BYTES];
    crypto_hash_sha256(hash, (const unsigned char *)&counter, sizeof counter);
    memcpy(nonce, hash, 24);
    fortias_memzero(hash, sizeof hash);
}

/* ── Helper: encrypt seed → encrypted_key (32 ciphertext + 16 MAC) ──── */
static bool encrypt_seed(uint8_t encrypted_key[48], uint8_t nonce[24],
                         const uint8_t seed[32]) {
    if (!instance_kek_initialized) return false;

    /* crypto_secretbox_easy writes msg_len + MACBYTES to `encrypted_key` */
    int rc = crypto_secretbox_easy(encrypted_key, seed, 32, nonce, instance_kek);
    return rc == 0;
}

/* ── Helper: decrypt encrypted_key → tmp (32 bytes) ─────────────────── */
static bool decrypt_seed(uint8_t tmp[32], const uint8_t encrypted_key[48],
                         const uint8_t nonce[24]) {
    if (!instance_kek_initialized) return false;

    /* cipher_len is 32 (ciphertext) + 16 (MAC) = 48 */
    int rc = crypto_secretbox_open_easy(tmp, encrypted_key, 48, nonce, instance_kek);
    return rc == 0;
}

/* ── Opaque private key structure ──────────────────────────────────────
 *
 * The seed is stored encrypted with the instance KEK using
 * ChaCha20-Poly1305 AEAD. The public key is cached since it's
 * public information.
 * ─────────────────────────────────────────────────────────────────────── */
struct FortiasPrivKey {
    uint8_t       encrypted_key[48];  /* 32 bytes ciphertext + 16 bytes MAC */
    uint8_t       nonce[24];          /* ChaCha20 nonce (24 bytes)           */
    uint8_t       public_key[32];     /* Cached public key (public info)     */
    FortiasCurve  curve;              /* Curve identifier                    */
};

/* ── Public API ──────────────────────────────────────────────────────── */

void fortias_privkey_init(void) {
    if (instance_kek_initialized) return;
    randombytes_buf(instance_kek, sizeof instance_kek);
    instance_kek_initialized = true;
    key_gen_counter = 0;
}

void fortias_privkey_cleanup(void) {
    fortias_memzero(instance_kek, sizeof instance_kek);
    instance_kek_initialized = false;
    key_gen_counter = 0;
}

FortiasPrivKey* fortias_privkey_ed25519_generate(void) {
    if (!instance_kek_initialized) return NULL;

    FortiasPrivKey *key = (FortiasPrivKey *)calloc(1, sizeof(FortiasPrivKey));
    if (!key) return NULL;

    key->curve = FORTIAS_CURVE_ED25519;

    /* 1. Generate raw 32-byte seed */
    uint8_t seed[crypto_sign_SEEDBYTES];
    randombytes_buf(seed, sizeof seed);

    /* 2. Derive public key */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        fortias_memzero(seed, sizeof seed);
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    /* 3. Create nonce from monotonic counter */
    key_gen_counter++;
    derive_nonce(key->nonce, key_gen_counter);

    /* 4. Encrypt seed with instance KEK */
    if (!encrypt_seed(key->encrypted_key, key->nonce, seed)) {
        fortias_memzero(seed, sizeof seed);
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    /* 5. Store public key, zero everything else */
    memcpy(key->public_key, pub, sizeof key->public_key);
    fortias_memzero(sec, sizeof sec);
    fortias_memzero(seed, sizeof seed);

    return key;
}

FortiasPrivKey* fortias_privkey_ed25519_from_seed(const uint8_t seed[32]) {
    if (!seed || !instance_kek_initialized) return NULL;

    FortiasPrivKey *key = (FortiasPrivKey *)calloc(1, sizeof(FortiasPrivKey));
    if (!key) return NULL;

    key->curve = FORTIAS_CURVE_ED25519;

    /* Derive public key from the provided seed */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    /* Create nonce from monotonic counter */
    key_gen_counter++;
    derive_nonce(key->nonce, key_gen_counter);

    /* Encrypt the seed */
    if (!encrypt_seed(key->encrypted_key, key->nonce, seed)) {
        fortias_memzero(sec, sizeof sec);
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
        return NULL;
    }

    /* Store public key, zero everything else */
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

    /* 1. Decrypt seed */
    uint8_t tmp[32];
    if (!decrypt_seed(tmp, key->encrypted_key, key->nonce)) {
        return FORTIAS_ERR_INTERNAL;
    }

    /* 2. Derive keypair and sign */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, tmp) != 0) {
        fortias_memzero(tmp, sizeof tmp);
        return FORTIAS_ERR_INTERNAL;
    }

    unsigned char raw_sig[crypto_sign_BYTES];
    if (crypto_sign_detached(raw_sig, NULL, msg, (unsigned long long)msg_len, sec) != 0) {
        fortias_memzero(sec, sizeof sec);
        fortias_memzero(tmp, sizeof tmp);
        return FORTIAS_ERR_INTERNAL;
    }

    memcpy(sig, raw_sig, 64);
    fortias_memzero(sec, sizeof sec);
    fortias_memzero(raw_sig, sizeof raw_sig);
    fortias_memzero(tmp, sizeof tmp);

    return FORTIAS_OK;
}

FortiasResult fortias_nullifier_derive_handle(const FortiasPrivKey *key, const uint8_t *context, size_t context_len, FortiasNullifier *out) {
    if (!key || !context || !out) return FORTIAS_ERR_BAD_INPUT;

    /* Decrypt seed for HMAC */
    uint8_t tmp[32];
    if (!decrypt_seed(tmp, key->encrypted_key, key->nonce)) {
        return FORTIAS_ERR_INTERNAL;
    }

    unsigned char hmac[crypto_auth_hmacsha256_BYTES];
    crypto_auth_hmacsha256(hmac, context, (unsigned long long)context_len, tmp);
    memcpy(out->bytes, hmac, 32);
    fortias_memzero(hmac, sizeof hmac);
    fortias_memzero(tmp, sizeof tmp);

    return FORTIAS_OK;
}

void fortias_privkey_free(FortiasPrivKey *key) {
    if (key) {
        fortias_memzero(key, sizeof(FortiasPrivKey));
        free(key);
    }
}
