#include "test_runner.h"

static void test_rng_nonzero_output(void) {
    uint8_t buf[32];
    ForetiasResult r = foretias_rng_bytes(buf, sizeof(buf));
    ASSERT_EQ(r, FORETIAS_OK, "rng returns OK");
    int nonzero = 0;
    for (size_t i = 0; i < sizeof(buf); i++) {
        if (buf[i] != 0) nonzero = 1;
    }
    ASSERT_EQ(nonzero, 1, "rng produces non-zero bytes");
}

static void test_rng_different_calls(void) {
    uint8_t a[32], b[32];
    foretias_rng_bytes(a, sizeof(a));
    foretias_rng_bytes(b, sizeof(b));
    int different = (memcmp(a, b, sizeof(a)) != 0) ? 1 : 0;
    ASSERT_EQ(different, 1, "two calls produce different output");
}

static void test_rng_exact_length(void) {
    uint8_t buf[128];
    memset(buf, 0, sizeof(buf));
    ForetiasResult r = foretias_rng_bytes(buf, 64);
    ASSERT_EQ(r, FORETIAS_OK, "rng fills exact length");
    /* First 64 bytes should be non-zero (probabilistically), last 64 still zero */
    int last64_zero = 1;
    for (size_t i = 64; i < 128; i++) {
        if (buf[i] != 0) last64_zero = 0;
    }
    ASSERT_EQ(last64_zero, 1, "only requested bytes filled");
}

static void test_rng_null_zero_len_ok(void) {
    ForetiasResult r = foretias_rng_bytes(NULL, 0);
    ASSERT_EQ(r, FORETIAS_OK, "NULL buf with 0 len returns OK");
}

static void test_rng_null_nonzero_len_error(void) {
    ForetiasResult r = foretias_rng_bytes(NULL, 32);
    ASSERT_EQ(r, FORETIAS_ERR_BAD_INPUT, "NULL buf with nonzero len returns BAD_INPUT");
}

int test_rng_main(void) {
    printf("=== rng ===\n");
    test_rng_nonzero_output();
    test_rng_different_calls();
    test_rng_exact_length();
    test_rng_null_zero_len_ok();
    test_rng_null_nonzero_len_error();
    TEST_REPORT("rng");
    return test_suite_finish();
}
