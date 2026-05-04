#include "platform.h"
#include "foretias_core.h"
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
    foretias_memzero(hash, sizeof hash);
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
struct ForetiasPrivKey {
    uint8_t       encrypted_key[48];  /* 32 bytes ciphertext + 16 bytes MAC */
    uint8_t       nonce[24];          /* ChaCha20 nonce (24 bytes)           */
    uint8_t       public_key[32];     /* Cached public key (public info)     */
    ForetiasCurve  curve;              /* Curve identifier                    */
};

/* ── Public API ──────────────────────────────────────────────────────── */

void foretias_privkey_init(void) {
    if (instance_kek_initialized) return;
    randombytes_buf(instance_kek, sizeof instance_kek);
    instance_kek_initialized = true;
    key_gen_counter = 0;
}

void foretias_privkey_cleanup(void) {
    foretias_memzero(instance_kek, sizeof instance_kek);
    instance_kek_initialized = false;
    key_gen_counter = 0;
}

ForetiasPrivKey* foretias_privkey_ed25519_generate(void) {
    if (!instance_kek_initialized) return NULL;

    ForetiasPrivKey *key = (ForetiasPrivKey *)calloc(1, sizeof(ForetiasPrivKey));
    if (!key) return NULL;

    key->curve = FORETIAS_CURVE_ED25519;

    /* 1. Generate raw 32-byte seed */
    uint8_t seed[crypto_sign_SEEDBYTES];
    randombytes_buf(seed, sizeof seed);

    /* 2. Derive public key */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        foretias_memzero(seed, sizeof seed);
        foretias_memzero(key, sizeof(ForetiasPrivKey));
        free(key);
        return NULL;
    }

    /* 3. Create nonce from monotonic counter */
    key_gen_counter++;
    derive_nonce(key->nonce, key_gen_counter);

    /* 4. Encrypt seed with instance KEK */
    if (!encrypt_seed(key->encrypted_key, key->nonce, seed)) {
        foretias_memzero(seed, sizeof seed);
        foretias_memzero(key, sizeof(ForetiasPrivKey));
        free(key);
        return NULL;
    }

    /* 5. Store public key, zero everything else */
    memcpy(key->public_key, pub, sizeof key->public_key);
    foretias_memzero(sec, sizeof sec);
    foretias_memzero(seed, sizeof seed);

    return key;
}

ForetiasPrivKey* foretias_privkey_ed25519_from_seed(const uint8_t seed[32]) {
    if (!seed || !instance_kek_initialized) return NULL;

    ForetiasPrivKey *key = (ForetiasPrivKey *)calloc(1, sizeof(ForetiasPrivKey));
    if (!key) return NULL;

    key->curve = FORETIAS_CURVE_ED25519;

    /* Derive public key from the provided seed */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, seed) != 0) {
        foretias_memzero(key, sizeof(ForetiasPrivKey));
        free(key);
        return NULL;
    }

    /* Create nonce from monotonic counter */
    key_gen_counter++;
    derive_nonce(key->nonce, key_gen_counter);

    /* Encrypt the seed */
    if (!encrypt_seed(key->encrypted_key, key->nonce, seed)) {
        foretias_memzero(sec, sizeof sec);
        foretias_memzero(key, sizeof(ForetiasPrivKey));
        free(key);
        return NULL;
    }

    /* Store public key, zero everything else */
    memcpy(key->public_key, pub, sizeof key->public_key);
    foretias_memzero(sec, sizeof sec);

    return key;
}

uint8_t* foretias_privkey_ed25519_public(const ForetiasPrivKey *key, uint8_t out[32]) {
    if (!key || !out) return NULL;
    memcpy(out, key->public_key, 32);
    return out;
}

int foretias_privkey_ed25519_sign(const ForetiasPrivKey *key, const uint8_t *msg, size_t msg_len, uint8_t sig[64]) {
    if (!key || !msg || !sig) return FORETIAS_ERR_BAD_INPUT;

    /* 1. Decrypt seed */
    uint8_t tmp[32];
    if (!decrypt_seed(tmp, key->encrypted_key, key->nonce)) {
        return FORETIAS_ERR_INTERNAL;
    }

    /* 2. Derive keypair and sign */
    unsigned char pub[crypto_sign_PUBLICKEYBYTES];
    unsigned char sec[crypto_sign_SECRETKEYBYTES];
    if (crypto_sign_seed_keypair(pub, sec, tmp) != 0) {
        foretias_memzero(tmp, sizeof tmp);
        return FORETIAS_ERR_INTERNAL;
    }

    unsigned char raw_sig[crypto_sign_BYTES];
    if (crypto_sign_detached(raw_sig, NULL, msg, (unsigned long long)msg_len, sec) != 0) {
        foretias_memzero(sec, sizeof sec);
        foretias_memzero(tmp, sizeof tmp);
        return FORETIAS_ERR_INTERNAL;
    }

    memcpy(sig, raw_sig, 64);
    foretias_memzero(sec, sizeof sec);
    foretias_memzero(raw_sig, sizeof raw_sig);
    foretias_memzero(tmp, sizeof tmp);

    return FORETIAS_OK;
}

ForetiasResult foretias_nullifier_derive_handle(const ForetiasPrivKey *key, const uint8_t *context, size_t context_len, ForetiasNullifier *out) {
    if (!key || !context || !out) return FORETIAS_ERR_BAD_INPUT;

    /* Decrypt seed for HMAC */
    uint8_t tmp[32];
    if (!decrypt_seed(tmp, key->encrypted_key, key->nonce)) {
        return FORETIAS_ERR_INTERNAL;
    }

    unsigned char hmac[crypto_auth_hmacsha256_BYTES];
    crypto_auth_hmacsha256(hmac, context, (unsigned long long)context_len, tmp);
    memcpy(out->bytes, hmac, 32);
    foretias_memzero(hmac, sizeof hmac);
    foretias_memzero(tmp, sizeof tmp);

    return FORETIAS_OK;
}

ForetiasResult foretias_privkey_derive_seal_key(
    const ForetiasPrivKey *key,
    const uint8_t *info,
    size_t info_len,
    uint8_t seal_key[32]
) {
    if (!key || !info || !seal_key) return FORETIAS_ERR_BAD_INPUT;

    /* 1. Decrypt seed */
    uint8_t tmp[32];
    if (!decrypt_seed(tmp, key->encrypted_key, key->nonce)) {
        return FORETIAS_ERR_INTERNAL;
    }

    /* 2. HKDF-SHA256 per RFC 5869:
       Extract: LMK = HMAC-SHA256(salt=0x00..0x00, seed)
       Expand:  OKM = HMAC-SHA256(LMK, info || 0x01) truncated to 32 bytes */
    uint8_t lmk[32];
    uint8_t salt[32] = {0};
    crypto_auth_hmacsha256(lmk, tmp, 32, salt);

    /* Expand: okm = HMAC(LMK, info || 0x01) */
    uint8_t expand_input[64];
    memcpy(expand_input, info, info_len);
    expand_input[info_len] = 0x01;
    unsigned char okm[32];
    crypto_auth_hmacsha256(okm, expand_input, info_len + 1, lmk);
    memcpy(seal_key, okm, 32);

    /* 3. Zeroize all temporary buffers */
    foretias_memzero(lmk, sizeof lmk);
    foretias_memzero(expand_input, sizeof expand_input);
    foretias_memzero(okm, sizeof okm);
    foretias_memzero(tmp, sizeof tmp);

    return FORETIAS_OK;
}

void foretias_privkey_free(ForetiasPrivKey *key) {
    if (key) {
        foretias_memzero(key, sizeof(ForetiasPrivKey));
        free(key);
    }
}
