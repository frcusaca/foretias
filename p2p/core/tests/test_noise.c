#include "test_runner.h"

static void test_noise_init_ed25519_ok(void) {
    ForetiasNoiseState state;
    ForetiasPrivKey32 priv;
    memset(&state, 0, sizeof(state));
    memset(&priv, 0, sizeof(priv));
    ForetiasResult r = foretias_noise_init_ed25519(&state, &priv, NULL, true);
    ASSERT_EQ(r, FORETIAS_OK, "init_ed25519 returns OK");
}

static void test_noise_init_p256_unsupported(void) {
    ForetiasNoiseState state;
    ForetiasPrivKey32 priv;
    memset(&state, 0, sizeof(state));
    memset(&priv, 0, sizeof(priv));
    ForetiasResult r = foretias_noise_init_p256(&state, &priv, NULL, true);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "init_p256 returns UNSUPPORTED");
}

static void test_noise_step_bad_input(void) {
    ForetiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t out[64];
    size_t out_len = sizeof(out);
    ForetiasResult r = foretias_noise_step(&state, NULL, 0, out, &out_len);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_KEY, "step on zeroed state returns BAD_KEY (encrypted chaining key fails decrypt)");
}

static void test_noise_send_bad_input(void) {
    ForetiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t ct[64];
    size_t ct_len;
    const uint8_t pt[] = "hello";
    ForetiasResult r = foretias_noise_send(&state, pt, 5, ct, &ct_len);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "send on zeroed state returns BAD_INPUT");
}

static void test_noise_recv_bad_input(void) {
    ForetiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t pt[64];
    size_t pt_len;
    const uint8_t ct[] = "hello";
    ForetiasResult r = foretias_noise_recv(&state, ct, 5, pt, &pt_len);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "recv on zeroed state returns BAD_INPUT");
}

static void test_noise_destroy_no_crash(void) {
    ForetiasNoiseState state;
    memset(&state, 0, sizeof(state));
    foretias_noise_destroy(&state);
    ASSERT_EQ(1, 1, "destroy on zeroed state does not crash");
}

int test_noise_main(void) {
    printf("=== noise ===\n");
    test_noise_init_ed25519_ok();
    test_noise_init_p256_unsupported();
    test_noise_step_bad_input();
    test_noise_send_bad_input();
    test_noise_recv_bad_input();
    test_noise_destroy_no_crash();
    TEST_REPORT("noise");
    return test_suite_finish();
}
