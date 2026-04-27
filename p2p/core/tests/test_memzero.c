#include "test_runner.h"

static void test_memzero_zeroes_memory(void) {
    uint8_t buf[16] = {0xFF};
    fortias_memzero(buf, sizeof(buf));
    for (size_t i = 0; i < sizeof(buf); i++) {
        ASSERT_EQ(buf[i], 0, "byte zeroed");
    }
}

static void test_memzero_partial_zero(void) {
    uint8_t buf[16];
    memset(buf, 0xFF, sizeof(buf));
    fortias_memzero(buf, 8);
    for (size_t i = 0; i < 8; i++) {
        ASSERT_EQ(buf[i], 0, "first half zeroed");
    }
    for (size_t i = 8; i < 16; i++) {
        ASSERT_EQ(buf[i], 0xFF, "second half untouched");
    }
}

static void test_memzero_zero_length_noop(void) {
    uint8_t buf[8] = {0xAB};
    fortias_memzero(buf, 0);
    ASSERT_EQ(buf[0], 0xAB, "zero length no-op");
}

static void test_memzero_large_buffer(void) {
    uint8_t buf[1024];
    memset(buf, 0xAA, sizeof(buf));
    fortias_memzero(buf, sizeof(buf));
    for (size_t i = 0; i < sizeof(buf); i++) {
        ASSERT_EQ(buf[i], 0, "large buffer byte zeroed");
    }
}

int test_memzero_main(void) {
    printf("=== memzero ===\n");
    test_memzero_zeroes_memory();
    test_memzero_partial_zero();
    test_memzero_zero_length_noop();
    test_memzero_large_buffer();
    TEST_REPORT("memzero");
    return test_suite_finish();
}
