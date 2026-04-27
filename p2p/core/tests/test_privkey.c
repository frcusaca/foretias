#include "test_runner.h"

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

int test_privkey_main(void) {
    printf("=== privkey (opaque handle) ===\n");
    test_generate_and_free();
    test_sign_and_verify();
    test_from_seed_deterministic();
    test_null_input();
    test_nullifier_derive_handle();
    TEST_REPORT("privkey");
    return test_suite_finish();
}
