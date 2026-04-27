#include "test_runner.h"

static void test_md5_unsupported(void) {
    FortiasHash16 h;
    const uint8_t data[] = "test";
    FortiasResult r = fortias_hash_legacy_insecure_md5(data, 4, &h);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "md5 returns UNSUPPORTED");
}

static void test_sha1_unsupported(void) {
    FortiasHash20 h;
    const uint8_t data[] = "test";
    FortiasResult r = fortias_hash_legacy_insecure_sha1(data, 4, &h);
    ASSERT_EQ(r, FORTIAS_ERR_UNSUPPORTED, "sha1 returns UNSUPPORTED");
}

int test_legacy_hash_stub_main(void) {
    printf("=== legacy_hash_stub ===\n");
    test_md5_unsupported();
    test_sha1_unsupported();
    TEST_REPORT("legacy_hash_stub");
    return test_suite_finish();
}
