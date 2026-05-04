#include "test_runner.h"

static void test_blake3_unsupported(void) {
    ForetiasHash32 h;
    const uint8_t data[] = "test";
    ForetiasResult r = foretias_hash_blake3(data, 4, &h);
    ASSERT_EQ(r, FORETIAS_ERR_UNSUPPORTED, "blake3 returns UNSUPPORTED");
}

int test_blake3_stub_main(void) {
    printf("=== blake3_stub ===\n");
    test_blake3_unsupported();
    TEST_REPORT("blake3_stub");
    return test_suite_finish();
}
