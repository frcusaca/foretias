#include "test_runner.h"

static void test_p256_generate_keypair_unsupported(void) {
    ForetiasPubKey33 pub;
    ForetiasPrivKey32 priv;
    ForetiasResult r = foretias_p256_generate_keypair(&pub, &priv);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "generate_keypair returns UNSUPPORTED");
}

static void test_p256_derive_peer_id_unsupported(void) {
    ForetiasPubKey33 pub;
    ForetiasPeerID id;
    ForetiasResult r = foretias_p256_derive_peer_id(&pub, &id);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "derive_peer_id returns UNSUPPORTED");
}

static void test_p256_sign_unsupported(void) {
    ForetiasPrivKey32 priv;
    ForetiasSig64 sig;
    const uint8_t msg[] = "test";
    ForetiasResult r = foretias_p256_sign(&priv, msg, 4, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "sign returns UNSUPPORTED");
}

static void test_p256_verify_unsupported(void) {
    ForetiasPubKey33 pub;
    ForetiasSig64 sig;
    const uint8_t msg[] = "test";
    ForetiasResult r = foretias_p256_verify(&pub, msg, 4, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "verify returns UNSUPPORTED");
}

int test_p256_stub_main(void) {
    printf("=== p256_stub ===\n");
    test_p256_generate_keypair_unsupported();
    test_p256_derive_peer_id_unsupported();
    test_p256_sign_unsupported();
    test_p256_verify_unsupported();
    TEST_REPORT("p256_stub");
    return test_suite_finish();
}
