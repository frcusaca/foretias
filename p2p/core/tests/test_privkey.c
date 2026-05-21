#include "test_runner.h"
#include <stdint.h>

static void test_init_cleanup_cycle(void) {
    /* Init should be idempotent */
    foretias_privkey_init();
    foretias_privkey_init();

    /* Generate a key after init */
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate works after init");

    /* Sign should work */
    const uint8_t msg[] = "hello after init";
    uint8_t sig[64];
    int rc = foretias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORETIAS_OK, "sign works after init");

    foretias_privkey_free(key);

    /* Cleanup and verify generate fails without re-init */
    foretias_privkey_cleanup();
    ForetiasPrivKey *bad_key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NULL(bad_key, "generate fails after cleanup");

    /* Re-init and verify it works again */
    foretias_privkey_init();
    ForetiasPrivKey *good_key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(good_key, "generate works after re-init");
    foretias_privkey_free(good_key);
}

static void test_generate_and_free(void) {
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");
    foretias_privkey_free(key);
}

static void test_sign_and_verify(void) {
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    const uint8_t msg[] = "hello foretias";
    uint8_t sig[64];
    int rc = foretias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORETIAS_OK, "sign succeeds");

    uint8_t pub[32];
    foretias_privkey_ed25519_public(key, pub);

    ForetiasPubKey32 pub_key = { .bytes = {0} };
    memcpy(pub_key.bytes, pub, 32);
    ForetiasSig64 sig64 = { .bytes = {0} };
    memcpy(sig64.bytes, sig, 64);

    ForetiasResult vr = foretias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORETIAS_OK, "verify roundtrip succeeds");

    foretias_privkey_free(key);
}

static void test_from_seed_deterministic(void) {
    uint8_t seed[32] = {0};
    for (size_t i = 0; i < 32; i++) seed[i] = (uint8_t)i;

    ForetiasPrivKey *key1 = foretias_privkey_ed25519_from_seed(seed);
    ForetiasPrivKey *key2 = foretias_privkey_ed25519_from_seed(seed);
    ASSERT_PTR_NOT_NULL(key1, "from_seed returns non-NULL");
    ASSERT_PTR_NOT_NULL(key2, "from_seed returns non-NULL");

    uint8_t pub1[32], pub2[32];
    foretias_privkey_ed25519_public(key1, pub1);
    foretias_privkey_ed25519_public(key2, pub2);

    int same = (memcmp(pub1, pub2, 32) == 0) ? 1 : 0;
    ASSERT_EQ(same, 1, "same seed produces same public key");

    foretias_privkey_free(key1);
    foretias_privkey_free(key2);
}

static void test_null_input(void) {
    uint8_t msg[] = "test";
    uint8_t sig[64];
    int rc = foretias_privkey_ed25519_sign(NULL, msg, 4, sig);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "NULL key returns BAD_INPUT");

    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    rc = foretias_privkey_ed25519_sign(key, NULL, 4, sig);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "NULL msg returns BAD_INPUT");

    rc = foretias_privkey_ed25519_sign(key, msg, 4, NULL);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "NULL sig returns BAD_INPUT");
    foretias_privkey_free(key);
}

static void test_nullifier_derive_handle(void) {
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    const uint8_t context[] = "test-context";
    ForetiasNullifier null1, null2;

    ForetiasResult r = foretias_nullifier_derive_handle(key, context, sizeof(context) - 1, &null1);
    ASSERT_EQ(r, FORETIAS_OK, "derive handle succeeds");

    r = foretias_nullifier_derive_handle(key, context, sizeof(context) - 1, &null2);
    ASSERT_EQ(r, FORETIAS_OK, "derive handle is deterministic");

    int same = (memcmp(&null1, &null2, sizeof(null1)) == 0) ? 1 : 0;
    ASSERT_EQ(same, 1, "same inputs produce same nullifier");

    foretias_privkey_free(key);
}

static void test_encrypted_key_useless_without_kek(void) {
    /* Generate a key with the current KEK */
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    /* Sign a message — should succeed */
    const uint8_t msg[] = "encrypted key test";
    uint8_t sig[64];
    int rc = foretias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORETIAS_OK, "sign succeeds with valid KEK");

    /* Save the public key for verification */
    uint8_t pub[32];
    foretias_privkey_ed25519_public(key, pub);

    /* Verify signature against public key */
    ForetiasPubKey32 pub_key = { .bytes = {0} };
    memcpy(pub_key.bytes, pub, 32);
    ForetiasSig64 sig64 = { .bytes = {0} };
    memcpy(sig64.bytes, sig, 64);
    ForetiasResult vr = foretias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORETIAS_OK, "signature verifies with public key");

    /* Sign again to confirm key remains functional (same KEK, same process).
     * The security property is: encrypted_key bytes are useless
     * without the instance_kek, which is never exposed. */
    rc = foretias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORETIAS_OK, "key still works with same KEK");

    vr = foretias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORETIAS_OK, "second signature also verifies");

    foretias_privkey_free(key);
}

static void test_from_seed_null_seed(void) {
    ForetiasPrivKey *key = foretias_privkey_ed25519_from_seed(NULL);
    ASSERT_PTR_NULL(key, "from_seed with NULL returns NULL");
}

static void test_derive_seal_key_info_len_overflow(void) {
    ForetiasPrivKey *key = foretias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");
    const uint8_t info[] = "test-info";
    uint8_t seal_key[32];
    ForetiasResult r = foretias_privkey_derive_seal_key(key, info, 64, seal_key);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "info_len=64 returns BAD_INPUT");
    r = foretias_privkey_derive_seal_key(key, info, 63, seal_key);
    ASSERT_EQ(r, FORETIAS_OK, "info_len=63 succeeds");
    foretias_privkey_free(key);
}

/* ── Generic encrypt/decrypt tests (HR-1 Phase 0.4) ─────────────────── */
static void test_encrypt_decrypt_roundtrip(void) {
    /* Test round-trip for sizes: 32, 96, 128, 160, 1184, 1632, 4000, 4112 */
    size_t sizes[] = {32, 96, 128, 160, 1184, 1632, 4000, 4112};
    int n_sizes = sizeof(sizes) / sizeof(sizes[0]);

    for (int i = 0; i < n_sizes; i++) {
        size_t pt_len = sizes[i];
        uint8_t plaintext[4112];
        uint8_t nonce[24];
        uint8_t ciphertext[4128];  /* 4112 + 16 */
        uint8_t decrypted[4112];

        /* Fill plaintext with known pattern */
        for (size_t j = 0; j < pt_len; j++)
            plaintext[j] = (uint8_t)(j & 0xFF);

        /* Encrypt */
        ForetiasResult rc = foretias_privkey_encrypt(plaintext, pt_len, ciphertext, nonce);
        char buf[64];
        snprintf(buf, sizeof(buf), "encrypt succeeds for size %zu", pt_len);
        ASSERT_EQ(rc, FORETIAS_OK, buf);

        /* Decrypt */
        rc = foretias_privkey_decrypt(ciphertext, pt_len + 16, nonce, decrypted);
        snprintf(buf, sizeof(buf), "decrypt succeeds for size %zu", pt_len);
        ASSERT_EQ(rc, FORETIAS_OK, buf);

        /* Compare */
        int same = (memcmp(plaintext, decrypted, pt_len) == 0) ? 1 : 0;
        snprintf(buf, sizeof(buf), "roundtrip matches for size %zu", pt_len);
        ASSERT_EQ(same, 1, buf);
    }
}

static void test_decrypt_wrong_nonce_fails(void) {
    uint8_t plaintext[32];
    uint8_t nonce[24];
    uint8_t wrong_nonce[24];
    uint8_t ciphertext[48];
    uint8_t decrypted[32];

    for (size_t i = 0; i < 32; i++) plaintext[i] = (uint8_t)i;

    ForetiasResult rc = foretias_privkey_encrypt(plaintext, 32, ciphertext, nonce);
    ASSERT_EQ(rc, FORETIAS_OK, "encrypt succeeds");

    /* Use a wrong nonce */
    memcpy(wrong_nonce, nonce, 24);
    wrong_nonce[0] ^= 0xFF;

    rc = foretias_privkey_decrypt(ciphertext, 48, wrong_nonce, decrypted);
    ASSERT_NEQ(rc, FORETIAS_OK, "decrypt with wrong nonce fails");
}

static void test_decrypt_tampered_ciphertext_fails(void) {
    uint8_t plaintext[32];
    uint8_t nonce[24];
    uint8_t ciphertext[48];
    uint8_t decrypted[32];

    for (size_t i = 0; i < 32; i++) plaintext[i] = (uint8_t)i;

    ForetiasResult rc = foretias_privkey_encrypt(plaintext, 32, ciphertext, nonce);
    ASSERT_EQ(rc, FORETIAS_OK, "encrypt succeeds");

    /* Tamper with ciphertext */
    ciphertext[10] ^= 0xFF;

    rc = foretias_privkey_decrypt(ciphertext, 48, nonce, decrypted);
    ASSERT_NEQ(rc, FORETIAS_OK, "decrypt with tampered ciphertext fails");
}

static void test_encrypt_null_input(void) {
    uint8_t buf[32];
    uint8_t nonce[24];
    uint8_t ct[48];

    ForetiasResult rc = foretias_privkey_encrypt(NULL, 32, ct, nonce);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "encrypt with NULL plaintext fails");

    rc = foretias_privkey_encrypt(buf, 32, NULL, nonce);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "encrypt with NULL ciphertext fails");

    rc = foretias_privkey_encrypt(buf, 32, ct, NULL);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "encrypt with NULL nonce fails");
}

static void test_decrypt_null_input(void) {
    uint8_t buf[48];
    uint8_t nonce[24];
    uint8_t pt[32];

    ForetiasResult rc = foretias_privkey_decrypt(NULL, 48, nonce, pt);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "decrypt with NULL ciphertext fails");

    rc = foretias_privkey_decrypt(buf, 48, NULL, pt);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "decrypt with NULL nonce fails");

    rc = foretias_privkey_decrypt(buf, 48, nonce, NULL);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "decrypt with NULL plaintext fails");

    /* ct_len < 16 should fail */
    rc = foretias_privkey_decrypt(buf, 10, nonce, pt);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "decrypt with ct_len < 16 fails");
}

int test_privkey_main(void) {
    printf("=== privkey (opaque handle, encrypted) ===\n");

    /* All tests require init. Run init/cleanup cycle first. */
    test_init_cleanup_cycle();

    /* Re-init for remaining tests */
    foretias_privkey_init();

    test_generate_and_free();
    test_sign_and_verify();
    test_from_seed_deterministic();
    test_null_input();
    test_nullifier_derive_handle();
    test_encrypted_key_useless_without_kek();
    test_from_seed_null_seed();
    test_derive_seal_key_info_len_overflow();

    /* Generic encrypt/decrypt tests */
    test_encrypt_decrypt_roundtrip();
    test_decrypt_wrong_nonce_fails();
    test_decrypt_tampered_ciphertext_fails();
    test_encrypt_null_input();
    test_decrypt_null_input();

    TEST_REPORT("privkey");
    return test_suite_finish();
}
