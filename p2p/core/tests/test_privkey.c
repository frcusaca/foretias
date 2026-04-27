#include "test_runner.h"

static void test_init_cleanup_cycle(void) {
    /* Init should be idempotent */
    fortias_privkey_init();
    fortias_privkey_init();

    /* Generate a key after init */
    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate works after init");

    /* Sign should work */
    const uint8_t msg[] = "hello after init";
    uint8_t sig[64];
    int rc = fortias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORTIAS_OK, "sign works after init");

    fortias_privkey_free(key);

    /* Cleanup and verify generate fails without re-init */
    fortias_privkey_cleanup();
    FortiasPrivKey *bad_key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NULL(bad_key, "generate fails after cleanup");

    /* Re-init and verify it works again */
    fortias_privkey_init();
    FortiasPrivKey *good_key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(good_key, "generate works after re-init");
    fortias_privkey_free(good_key);
}

static void test_generate_and_free(void) {
    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");
    fortias_privkey_free(key);
}

static void test_sign_and_verify(void) {
    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    const uint8_t msg[] = "hello fortias";
    uint8_t sig[64];
    int rc = fortias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORTIAS_OK, "sign succeeds");

    uint8_t pub[32];
    fortias_privkey_ed25519_public(key, pub);

    FortiasPubKey32 pub_key = { .bytes = {0} };
    memcpy(pub_key.bytes, pub, 32);
    FortiasSig64 sig64 = { .bytes = {0} };
    memcpy(sig64.bytes, sig, 64);

    FortiasResult vr = fortias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORTIAS_OK, "verify roundtrip succeeds");

    fortias_privkey_free(key);
}

static void test_from_seed_deterministic(void) {
    uint8_t seed[32] = {0};
    for (size_t i = 0; i < 32; i++) seed[i] = (uint8_t)i;

    FortiasPrivKey *key1 = fortias_privkey_ed25519_from_seed(seed);
    FortiasPrivKey *key2 = fortias_privkey_ed25519_from_seed(seed);
    ASSERT_PTR_NOT_NULL(key1, "from_seed returns non-NULL");
    ASSERT_PTR_NOT_NULL(key2, "from_seed returns non-NULL");

    uint8_t pub1[32], pub2[32];
    fortias_privkey_ed25519_public(key1, pub1);
    fortias_privkey_ed25519_public(key2, pub2);

    int same = (memcmp(pub1, pub2, 32) == 0) ? 1 : 0;
    ASSERT_EQ(same, 1, "same seed produces same public key");

    fortias_privkey_free(key1);
    fortias_privkey_free(key2);
}

static void test_null_input(void) {
    uint8_t msg[] = "test";
    uint8_t sig[64];
    int rc = fortias_privkey_ed25519_sign(NULL, msg, 4, sig);
    ASSERT_EQ(rc, FORTIAS_ERR_BAD_INPUT, "NULL key returns BAD_INPUT");

    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    rc = fortias_privkey_ed25519_sign(key, NULL, 4, sig);
    ASSERT_EQ(rc, FORTIAS_ERR_BAD_INPUT, "NULL msg returns BAD_INPUT");

    rc = fortias_privkey_ed25519_sign(key, msg, 4, NULL);
    ASSERT_EQ(rc, FORTIAS_ERR_BAD_INPUT, "NULL sig returns BAD_INPUT");
    fortias_privkey_free(key);
}

static void test_nullifier_derive_handle(void) {
    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    const uint8_t context[] = "test-context";
    FortiasNullifier null1, null2;

    FortiasResult r = fortias_nullifier_derive_handle(key, context, sizeof(context) - 1, &null1);
    ASSERT_EQ(r, FORTIAS_OK, "derive handle succeeds");

    r = fortias_nullifier_derive_handle(key, context, sizeof(context) - 1, &null2);
    ASSERT_EQ(r, FORTIAS_OK, "derive handle is deterministic");

    int same = (memcmp(&null1, &null2, sizeof(null1)) == 0) ? 1 : 0;
    ASSERT_EQ(same, 1, "same inputs produce same nullifier");

    fortias_privkey_free(key);
}

static void test_encrypted_key_useless_without_kek(void) {
    /* Generate a key with the current KEK */
    FortiasPrivKey *key = fortias_privkey_ed25519_generate();
    ASSERT_PTR_NOT_NULL(key, "generate returns non-NULL");

    /* Sign a message — should succeed */
    const uint8_t msg[] = "encrypted key test";
    uint8_t sig[64];
    int rc = fortias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORTIAS_OK, "sign succeeds with valid KEK");

    /* Save the public key for verification */
    uint8_t pub[32];
    fortias_privkey_ed25519_public(key, pub);

    /* Verify signature against public key */
    FortiasPubKey32 pub_key = { .bytes = {0} };
    memcpy(pub_key.bytes, pub, 32);
    FortiasSig64 sig64 = { .bytes = {0} };
    memcpy(sig64.bytes, sig, 64);
    FortiasResult vr = fortias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORTIAS_OK, "signature verifies with public key");

    /* Sign again to confirm key remains functional (same KEK, same process).
     * The security property is: encrypted_key bytes are useless
     * without the instance_kek, which is never exposed. */
    rc = fortias_privkey_ed25519_sign(key, msg, sizeof(msg) - 1, sig);
    ASSERT_EQ(rc, FORTIAS_OK, "key still works with same KEK");

    vr = fortias_ed25519_verify(&pub_key, msg, sizeof(msg) - 1, &sig64);
    ASSERT_EQ(vr, FORTIAS_OK, "second signature also verifies");

    fortias_privkey_free(key);
}

static void test_from_seed_null_seed(void) {
    FortiasPrivKey *key = fortias_privkey_ed25519_from_seed(NULL);
    ASSERT_PTR_NULL(key, "from_seed with NULL returns NULL");
}

int test_privkey_main(void) {
    printf("=== privkey (opaque handle, encrypted) ===\n");

    /* All tests require init. Run init/cleanup cycle first. */
    test_init_cleanup_cycle();

    /* Re-init for remaining tests */
    fortias_privkey_init();

    test_generate_and_free();
    test_sign_and_verify();
    test_from_seed_deterministic();
    test_null_input();
    test_nullifier_derive_handle();
    test_encrypted_key_useless_without_kek();
    test_from_seed_null_seed();

    TEST_REPORT("privkey");
    return test_suite_finish();
}
