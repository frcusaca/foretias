#include "test_runner.h"

static void test_version_fields(void) {
    ForetiasCoreVersion v = foretias_core_version();
    ASSERT_EQ(v.major, 0, "major version is 0");
    ASSERT_EQ(v.minor, 1, "minor version is 1");
    ASSERT_PTR_NOT_NULL(v.build_hash, "build_hash is non-NULL");
}

int test_version_main(void) {
    printf("=== version ===\n");
    test_version_fields();
    TEST_REPORT("version");
    return test_suite_finish();
}
