#include "test_runner.h"
#include <string.h>

static void test_md5_known_vector(void) {
    ForetiasHash16 h;
    const uint8_t data[] = "test";
    ForetiasResult r = foretias_hash_legacy_insecure_md5(data, 4, &h);
    ASSERT_EQ(r, FORETIAS_OK, "md5 returns OK");

    /* MD5("test") = 098f6bcd4621d373cade4e832627b4f6 */
    uint8_t expected[16] = {
        0x09, 0x8f, 0x6b, 0xcd, 0x46, 0x21, 0xd3, 0x73,
        0xca, 0xde, 0x4e, 0x83, 0x26, 0x27, 0xb4, 0xf6
    };
    ASSERT_EQ(0, memcmp(h.bytes, expected, 16), "md5 matches known vector");
}

static void test_md5_empty(void) {
    ForetiasHash16 h;
    ForetiasResult r = foretias_hash_legacy_insecure_md5(NULL, 0, &h);
    ASSERT_EQ(r, FORETIAS_OK, "md5 of empty returns OK");

    /* MD5("") = d41d8cd98f00b204e9800998ecf8427e */
    uint8_t expected[16] = {
        0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04,
        0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42, 0x7e
    };
    ASSERT_EQ(0, memcmp(h.bytes, expected, 16), "md5 of empty matches known vector");
}

static void test_sha1_known_vector(void) {
    ForetiasHash20 h;
    const uint8_t data[] = "test";
    ForetiasResult r = foretias_hash_legacy_insecure_sha1(data, 4, &h);
    ASSERT_EQ(r, FORETIAS_OK, "sha1 returns OK");

    /* SHA-1("test") = a94a8fe5ccb19ba61c4c0873d391e987982fbbd3 */
    uint8_t expected[20] = {
        0xa9, 0x4a, 0x8f, 0xe5, 0xcc, 0xb1, 0x9b, 0xa6,
        0x1c, 0x4c, 0x08, 0x73, 0xd3, 0x91, 0xe9, 0x87,
        0x98, 0x2f, 0xbb, 0xd3
    };
    ASSERT_EQ(0, memcmp(h.bytes, expected, 20), "sha1 matches known vector");
}

static void test_sha1_empty(void) {
    ForetiasHash20 h;
    ForetiasResult r = foretias_hash_legacy_insecure_sha1(NULL, 0, &h);
    ASSERT_EQ(r, FORETIAS_OK, "sha1 of empty returns OK");

    /* SHA-1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709 */
    uint8_t expected[20] = {
        0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d,
        0x32, 0x55, 0xbf, 0xef, 0x95, 0x60, 0x18, 0x90,
        0xaf, 0xd8, 0x07, 0x09
    };
    ASSERT_EQ(0, memcmp(h.bytes, expected, 20), "sha1 of empty matches known vector");
}

int test_legacy_hash_main(void) {
    printf("=== legacy_hash ===\n");
    test_md5_known_vector();
    test_md5_empty();
    test_sha1_known_vector();
    test_sha1_empty();
    TEST_REPORT("legacy_hash");
    return test_suite_finish();
}
