#include "test_runner.h"

static void test_nullifier_deterministic(void) {
    FortiasPrivKey32 priv;
    memset(priv.bytes, 0x42, sizeof(priv.bytes));
    const uint8_t ctx[] = "test context";

    FortiasNullifier n1, n2;
    FortiasResult r1 = fortias_nullifier_derive(&priv, ctx, sizeof(ctx) - 1, &n1);
    FortiasResult r2 = fortias_nullifier_derive(&priv, ctx, sizeof(ctx) - 1, &n2);
    ASSERT_EQ(r1, FORTIAS_OK, "first derive returns OK");
    ASSERT_EQ(r2, FORTIAS_OK, "second derive returns OK");
    ASSERT_EQ(memcmp(&n1, &n2, sizeof(n1)), 0, "same key+context = same nullifier");
}

static void test_nullifier_different_context(void) {
    FortiasPrivKey32 priv;
    memset(priv.bytes, 0x42, sizeof(priv.bytes));
    const uint8_t ctx1[] = "context A";
    const uint8_t ctx2[] = "context B";

    FortiasNullifier n1, n2;
    fortias_nullifier_derive(&priv, ctx1, sizeof(ctx1) - 1, &n1);
    fortias_nullifier_derive(&priv, ctx2, sizeof(ctx2) - 1, &n2);
    int different = (memcmp(&n1, &n2, sizeof(n1)) != 0) ? 1 : 0;
    ASSERT_EQ(different, 1, "different context produces different nullifier");
}

static void test_nullifier_null_key(void) {
    FortiasNullifier n;
    const uint8_t ctx[] = "ctx";
    FortiasResult r = fortias_nullifier_derive(NULL, ctx, 3, &n);
    ASSERT_EQ(r, FORTIAS_ERR_BAD_INPUT, "NULL key returns BAD_INPUT");
}

static void test_nullifier_null_context(void) {
    FortiasPrivKey32 priv;
    FortiasNullifier n;
    FortiasResult r = fortias_nullifier_derive(&priv, NULL, 0, &n);
    ASSERT_EQ(r, FORTIAS_ERR_BAD_INPUT, "NULL context returns BAD_INPUT");
}

int test_nullifier_main(void) {
    printf("=== nullifier ===\n");
    test_nullifier_deterministic();
    test_nullifier_different_context();
    test_nullifier_null_key();
    test_nullifier_null_context();
    TEST_REPORT("nullifier");
    return test_suite_finish();
}
