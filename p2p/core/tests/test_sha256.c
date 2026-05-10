#include "test_runner.h"

/* Helper: compare hash bytes to a hex string */
static int hash_eq_hex(const ForetiasHash32* h, const char* hex) {
    for (size_t i = 0; i < 32; i++) {
        uint8_t expected = 0;
        for (int j = 0; j < 2; j++) {
            char c = hex[i * 2 + j];
            expected <<= 4;
            if (c >= '0' && c <= '9') expected |= (uint8_t)(c - '0');
            else if (c >= 'a' && c <= 'f') expected |= (uint8_t)(c - 'a' + 10);
            else if (c >= 'A' && c <= 'F') expected |= (uint8_t)(c - 'A' + 10);
            else return 0;
        }
        if (h->bytes[i] != expected) return 0;
    }
    return 1;
}

static void test_sha256_empty(void) {
    ForetiasHash32 h;
    const uint8_t empty[1] = {0};
    ForetiasResult r = foretias_hash_sha256(empty, 0, &h);
    ASSERT_EQ(r, FORETIAS_OK, "empty input returns OK");
    ASSERT_EQ(hash_eq_hex(&h,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        1, "SHA-256 of empty string matches NIST vector");
}

static void test_sha256_abc(void) {
    const uint8_t data[] = "abc";
    ForetiasHash32 h;
    ForetiasResult r = foretias_hash_sha256(data, 3, &h);
    ASSERT_EQ(r, FORETIAS_OK, "abc input returns OK");
    ASSERT_EQ(hash_eq_hex(&h,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        1, "SHA-256 of 'abc' matches NIST vector");
}

static void test_sha256_abcdbcdecdefdefgefgefghhghghijhijijkljljklmnlmnomnopnopq(void) {
    const uint8_t data[] = "abcdbcdecdefdefgefgefghghghijhijijkljljklmnlmnomnopnopq";
    ForetiasHash32 h;
    ForetiasResult r = foretias_hash_sha256(data, sizeof(data) - 1, &h);
    ASSERT_EQ(r, FORETIAS_OK, "long input returns OK");
    ASSERT_EQ(hash_eq_hex(&h,
        "1183f43e6ae27f1b0fb012f5d297a116482776b9314ba5a72a632c44a51a3872"),
        1, "SHA-256 of long string matches NIST vector");
}

static void test_sha256_large_input(void) {
    /* 64KB of 0xFF */
    size_t len = 64 * 1024;
    uint8_t* data = calloc(1, len);
    memset(data, 0xFF, len);
    ForetiasHash32 h;
    ForetiasResult r = foretias_hash_sha256(data, len, &h);
    ASSERT_EQ(r, FORETIAS_OK, "64KB input returns OK");
    /* Verify determinism: hash again, should be identical */
    ForetiasHash32 h2;
    foretias_hash_sha256(data, len, &h2);
    ASSERT_EQ(memcmp(&h, &h2, sizeof(h)), 0, "large input is deterministic");
    free(data);
}

static void test_sha256_concat_equal_halves(void) {
    /* sha256_concat(half, half) == sha256(combined) */
    uint8_t half[64];
    memset(half, 0xAA, sizeof(half));
    uint8_t combined[128];
    memcpy(combined, half, 64);
    memcpy(combined + 64, half, 64);

    ForetiasHash32 h_concat, h_combined;
    foretias_hash_sha256_concat(half, 64, half, 64, &h_concat);
    foretias_hash_sha256(combined, 128, &h_combined);
    ASSERT_EQ(memcmp(&h_concat, &h_combined, sizeof(h_concat)), 0,
        "sha256_concat of equal halves equals sha256 of combined");
}

static void test_sha256_concat_one_empty(void) {
    /* sha256_concat(data, 0, NULL, 0) == sha256(data) */
    const uint8_t data[] = "hello concat";
    ForetiasHash32 h_concat, h_single;
    foretias_hash_sha256_concat(data, sizeof(data) - 1, (const uint8_t*)"", 0, &h_concat);
    foretias_hash_sha256(data, sizeof(data) - 1, &h_single);
    ASSERT_EQ(memcmp(&h_concat, &h_single, sizeof(h_concat)), 0,
        "sha256_concat with one empty half equals sha256 of the other");
}

int test_sha256_main(void) {
    printf("=== sha256 ===\n");
    test_sha256_empty();
    test_sha256_abc();
    test_sha256_abcdbcdecdefdefgefgefghhghghijhijijkljljklmnlmnomnopnopq();
    test_sha256_large_input();
    test_sha256_concat_equal_halves();
    test_sha256_concat_one_empty();
    TEST_REPORT("sha256");
    return test_suite_finish();
}
