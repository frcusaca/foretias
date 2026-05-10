#include "test_runner.h"

static void test_ed25519_keypair_gen(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    ForetiasResult r = foretias_ed25519_generate_keypair(&pub, &priv);
    ASSERT_EQ(r, FORETIAS_OK, "keypair generation succeeds");
    ASSERT_PTR_NOT_NULL(&pub, "pub is non-NULL");
    ASSERT_PTR_NOT_NULL(&priv, "priv is non-NULL");
}

static void test_ed25519_keypair_nonzero(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    foretias_ed25519_generate_keypair(&pub, &priv);
    int pub_nonzero = 0, priv_nonzero = 0;
    for (size_t i = 0; i < 32; i++) {
        if (pub.bytes[i] != 0) pub_nonzero = 1;
        if (priv.bytes[i] != 0) priv_nonzero = 1;
    }
    ASSERT_EQ(pub_nonzero, 1, "pubkey has non-zero bytes");
    ASSERT_EQ(priv_nonzero, 1, "privkey has non-zero bytes");
}

static void test_ed25519_peer_id_success(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    ForetiasPeerID id;
    foretias_ed25519_generate_keypair(&pub, &priv);
    ForetiasResult r = foretias_ed25519_derive_peer_id(&pub, &id);
    ASSERT_EQ(r, FORETIAS_OK, "peer_id derivation succeeds");
}

static void test_ed25519_peer_id_null_pub(void) {
    ForetiasPeerID id;
    ForetiasResult r = foretias_ed25519_derive_peer_id(NULL, &id);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "NULL pubkey returns BAD_INPUT");
}

static void test_ed25519_sign_verify_roundtrip(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    foretias_ed25519_generate_keypair(&pub, &priv);

    const uint8_t msg[] = "hello foretias";
    ForetiasSig64 sig;
    ForetiasResult r = foretias_ed25519_sign(&priv, msg, sizeof(msg) - 1, &sig);
    ASSERT_EQ(r, FORETIAS_OK, "sign succeeds");

    r = foretias_ed25519_verify(&pub, msg, sizeof(msg) - 1, &sig);
    ASSERT_EQ(r, FORETIAS_OK, "verify roundtrip succeeds");
}

static void test_ed25519_verify_wrong_message(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    foretias_ed25519_generate_keypair(&pub, &priv);

    const uint8_t msg[] = "hello foretias";
    ForetiasSig64 sig;
    foretias_ed25519_sign(&priv, msg, sizeof(msg) - 1, &sig);

    const uint8_t wrong[] = "wrong message";
    ForetiasResult r = foretias_ed25519_verify(&pub, wrong, sizeof(wrong) - 1, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_SIG, "verify with wrong message fails");
}

static void test_ed25519_verify_wrong_pubkey(void) {
    ForetiasPubKey32 pub;
    ForetiasPrivKey32 priv;
    ForetiasPubKey32 other_pub;
    ForetiasPrivKey32 other_priv;
    foretias_ed25519_generate_keypair(&pub, &priv);
    foretias_ed25519_generate_keypair(&other_pub, &other_priv);

    const uint8_t msg[] = "hello foretias";
    ForetiasSig64 sig;
    foretias_ed25519_sign(&priv, msg, sizeof(msg) - 1, &sig);

    ForetiasResult r = foretias_ed25519_verify(&other_pub, msg, sizeof(msg) - 1, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_SIG, "verify with wrong pubkey fails");
}

static void test_ed25519_sign_null_message(void) {
    ForetiasPrivKey32 priv;
    ForetiasPubKey32 pub;
    foretias_ed25519_generate_keypair(&pub, &priv);

    ForetiasSig64 sig;
    ForetiasResult r = foretias_ed25519_sign(&priv, NULL, 10, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "sign with NULL message returns error");
}

static void test_ed25519_different_keypairs_different_ids(void) {
    ForetiasPubKey32 pub1, pub2;
    ForetiasPrivKey32 priv1, priv2;
    ForetiasPeerID id1, id2;
    foretias_ed25519_generate_keypair(&pub1, &priv1);
    foretias_ed25519_generate_keypair(&pub2, &priv2);
    foretias_ed25519_derive_peer_id(&pub1, &id1);
    foretias_ed25519_derive_peer_id(&pub2, &id2);
    int different = (memcmp(&id1, &id2, sizeof(id1)) != 0) ? 1 : 0;
    ASSERT_EQ(different, 1, "different keypairs produce different peer IDs");
}

int test_ed25519_main(void) {
    printf("=== ed25519 ===\n");
    test_ed25519_keypair_gen();
    test_ed25519_keypair_nonzero();
    test_ed25519_peer_id_success();
    test_ed25519_peer_id_null_pub();
    test_ed25519_sign_verify_roundtrip();
    test_ed25519_verify_wrong_message();
    test_ed25519_verify_wrong_pubkey();
    test_ed25519_sign_null_message();
    test_ed25519_different_keypairs_different_ids();
    TEST_REPORT("ed25519");
    return test_suite_finish();
}
