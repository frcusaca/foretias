#include "test_runner.h"

static void test_frost_round1_unsupported(void) {
    ForetiasFrostRound1 state;
    ForetiasResult r = foretias_frost_round1(&state);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "round1 returns UNSUPPORTED");
}

static void test_frost_sign_share_unsupported(void) {
    ForetiasFrostRound1 state;
    ForetiasFrostShare key_share, sig_share;
    memset(&state, 0, sizeof(state));
    memset(&key_share, 0, sizeof(key_share));
    memset(&sig_share, 0, sizeof(sig_share));
    const uint8_t msg[] = "test";
    const uint8_t commits[64] = {0};
    ForetiasResult r = foretias_frost_sign_share(&state, &key_share, msg, 4, commits, 1, 0, &sig_share);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "sign_share returns UNSUPPORTED");
}

static void test_frost_aggregate_unsupported(void) {
    ForetiasFrostShare share;
    int32_t indices[] = {0};
    const uint8_t commits[64] = {0};
    const uint8_t msg[] = "test";
    ForetiasSig64 sig;
    ForetiasResult r = foretias_frost_aggregate(&share, indices, 1, commits, msg, 4, &sig);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "aggregate returns UNSUPPORTED");
}

static void test_frost_destroy_no_crash(void) {
    ForetiasFrostRound1 state;
    memset(&state, 0, sizeof(state));
    /* Should not crash */
    foretias_frost_destroy_round1(&state);
    ASSERT_EQ(1, 1, "destroy_round1 on zeroed state does not crash");
}

int test_frost_stub_main(void) {
    printf("=== frost_stub ===\n");
    test_frost_round1_unsupported();
    test_frost_sign_share_unsupported();
    test_frost_aggregate_unsupported();
    test_frost_destroy_no_crash();
    TEST_REPORT("frost_stub");
    return test_suite_finish();
}
