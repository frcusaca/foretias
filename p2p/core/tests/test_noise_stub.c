#include "test_runner.h"

static void test_noise_init_ed25519_unsupported(void) {
    FortiasNoiseState state;
    FortiasPrivKey32 priv;
    memset(&state, 0, sizeof(state));
    memset(&priv, 0, sizeof(priv));
    FortiasResult r = fortias_noise_init_ed25519(&state, &priv, NULL, true);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "init_ed25519 returns UNSUPPORTED");
}

static void test_noise_init_p256_unsupported(void) {
    FortiasNoiseState state;
    FortiasPrivKey32 priv;
    memset(&state, 0, sizeof(state));
    memset(&priv, 0, sizeof(priv));
    FortiasResult r = fortias_noise_init_p256(&state, &priv, NULL, true);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "init_p256 returns UNSUPPORTED");
}

static void test_noise_step_unsupported(void) {
    FortiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t out[64];
    size_t out_len = sizeof(out);
    FortiasResult r = fortias_noise_step(&state, NULL, 0, out, &out_len);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "step returns UNSUPPORTED");
}

static void test_noise_send_unsupported(void) {
    FortiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t ct[64];
    size_t ct_len;
    const uint8_t pt[] = "hello";
    FortiasResult r = fortias_noise_send(&state, pt, 5, ct, &ct_len);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "send returns UNSUPPORTED");
}

static void test_noise_recv_unsupported(void) {
    FortiasNoiseState state;
    memset(&state, 0, sizeof(state));
    uint8_t pt[64];
    size_t pt_len;
    const uint8_t ct[] = "hello";
    FortiasResult r = fortias_noise_recv(&state, ct, 5, pt, &pt_len);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "recv returns UNSUPPORTED");
}

static void test_noise_destroy_no_crash(void) {
    FortiasNoiseState state;
    memset(&state, 0, sizeof(state));
    /* Should not crash */
    fortias_noise_destroy(&state);
    ASSERT_EQ(1, 1, "destroy on zeroed state does not crash");
}

int test_noise_stub_main(void) {
    printf("=== noise_stub ===\n");
    test_noise_init_ed25519_unsupported();
    test_noise_init_p256_unsupported();
    test_noise_step_unsupported();
    test_noise_send_unsupported();
    test_noise_recv_unsupported();
    test_noise_destroy_no_crash();
    TEST_REPORT("noise_stub");
    return test_suite_finish();
}
