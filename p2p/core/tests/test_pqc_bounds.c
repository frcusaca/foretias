#include "test_runner.h"
#include <string.h>

static void test_sphincs_sha2_128s_sign_rejects_oversized_buffer(void) {
#ifdef OQS_ENABLE_SIG_sphincs_sha2_128s_simple
    ForetiasSecretKeyVar secret = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SECRET_BYTES };
    ForetiasPubKeyVar pubkey = { .bytes = {0}, .len = FORETIAS_SIG_MAX_PUBKEY_BYTES };

    ForetiasResult kr = foretias_sphincs_sha2_128s_keypair(&secret, &pubkey);
    if (kr != FORETIAS_OK) {
        printf("  SKIP: sphincs_sha2_128s keypair failed\n");
        return;
    }

    /* Craft a sig_out with len exceeding FORETIAS_SIG_MAX_SIG_BYTES */
    ForetiasSigVar sig;
    memset(&sig, 0, sizeof(sig));
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES + 1;

    ForetiasResult rc = foretias_sphincs_sha2_128s_sign(&secret, (const uint8_t *)"test", 4, &sig);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "sign rejects oversized sig buffer (len > MAX)");
#else
    printf("  SKIP: OQS_ENABLE_SIG_sphincs_sha2_128s_simple not defined\n");
#endif
}

static void test_sphincs_sha2_256f_sign_rejects_oversized_buffer(void) {
#ifdef OQS_ENABLE_SIG_sphincs_sha2_256f_simple
    ForetiasSecretKeyVar secret = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SECRET_BYTES };
    ForetiasPubKeyVar pubkey = { .bytes = {0}, .len = FORETIAS_SIG_MAX_PUBKEY_BYTES };

    ForetiasResult kr = foretias_sphincs_sha2_256f_keypair(&secret, &pubkey);
    if (kr != FORETIAS_OK) {
        printf("  SKIP: sphincs_sha2_256f keypair failed\n");
        return;
    }

    ForetiasSigVar sig;
    memset(&sig, 0, sizeof(sig));
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES + 1;

    ForetiasResult rc = foretias_sphincs_sha2_256f_sign(&secret, (const uint8_t *)"test", 4, &sig);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "sign rejects oversized sig buffer (len > MAX)");
#else
    printf("  SKIP: OQS_ENABLE_SIG_sphincs_sha2_256f_simple not defined\n");
#endif
}

static void test_dilithium3_sign_rejects_oversized_buffer(void) {
#ifdef OQS_ENABLE_SIG_dilithium_3
    ForetiasSecretKeyVar secret = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SECRET_BYTES };
    ForetiasPubKeyVar pubkey = { .bytes = {0}, .len = FORETIAS_SIG_MAX_PUBKEY_BYTES };

    ForetiasResult kr = foretias_dilithium3_keypair(&secret, &pubkey);
    if (kr != FORETIAS_OK) {
        printf("  SKIP: dilithium3 keypair failed\n");
        return;
    }

    ForetiasSigVar sig;
    memset(&sig, 0, sizeof(sig));
    sig.len = FORETIAS_SIG_MAX_SIG_BYTES + 1;

    ForetiasResult rc = foretias_dilithium3_sign(&secret, (const uint8_t *)"test", 4, &sig);
    ASSERT_EQ(rc, FORETIAS_ERR_BAD_INPUT, "sign rejects oversized sig buffer (len > MAX)");
#else
    printf("  SKIP: OQS_ENABLE_SIG_dilithium_3 not defined\n");
#endif
}

int test_pqc_bounds_main(void) {
    printf("=== PQC bounds ===\n");

    test_sphincs_sha2_128s_sign_rejects_oversized_buffer();
    test_sphincs_sha2_256f_sign_rejects_oversized_buffer();
    test_dilithium3_sign_rejects_oversized_buffer();

    TEST_REPORT("pqc_bounds");
    return test_suite_finish();
}
