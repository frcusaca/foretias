#include "test_runner.h"

static void test_merkle_leaf_deterministic(void) {
    FortiasHash32 h1, h2;
    const uint8_t data[] = "test leaf";
    fortias_merkle_leaf(data, sizeof(data) - 1, &h1);
    fortias_merkle_leaf(data, sizeof(data) - 1, &h2);
    ASSERT_EQ(memcmp(&h1, &h2, sizeof(h1)), 0, "same data produces same leaf hash");
}

static void test_merkle_leaf_same_data(void) {
    FortiasHash32 h1, h2;
    const uint8_t data[] = "same input";
    FortiasResult r1 = fortias_merkle_leaf(data, sizeof(data) - 1, &h1);
    FortiasResult r2 = fortias_merkle_leaf(data, sizeof(data) - 1, &h2);
    ASSERT_EQ(r1, FORTIAS_OK, "first call returns OK");
    ASSERT_EQ(r2, FORTIAS_OK, "second call returns OK");
    ASSERT_EQ(memcmp(&h1, &h2, sizeof(h1)), 0, "deterministic output");
}

static void test_merkle_verify_correct_proof(void) {
    /* Build a 2-level tree: root = sha256(leaf0 || leaf1) */
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    FortiasHash32 leaf0, leaf1, root;

    fortias_merkle_leaf(a, 1, &leaf0);
    fortias_merkle_leaf(b, 1, &leaf1);
    fortias_hash_sha256_concat(leaf0.bytes, 32, leaf1.bytes, 32, &root);

    /* Proof for leaf0: sibling=leaf1, direction=0 (sibling is on left) */
    FortiasMerkleProof proof;
    memset(&proof, 0, sizeof(proof));
    proof.siblings[0] = leaf1;
    proof.directions[0] = 1;
    proof.depth = 1;

    FortiasResult r = fortias_merkle_verify(&root, &leaf0, &proof);
    ASSERT_EQ(r, FORTIAS_OK, "correct proof verifies");
}

static void test_merkle_verify_tampered_sibling(void) {
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    FortiasHash32 leaf0, leaf1, root;

    fortias_merkle_leaf(a, 1, &leaf0);
    fortias_merkle_leaf(b, 1, &leaf1);
    fortias_hash_sha256_concat(leaf0.bytes, 32, leaf1.bytes, 32, &root);

    FortiasMerkleProof proof;
    memset(&proof, 0, sizeof(proof));
    /* Tamper with sibling */
    memset(proof.siblings[0].bytes, 0xFF, 32);
    proof.directions[0] = 0;
    proof.depth = 1;

    FortiasResult r = fortias_merkle_verify(&root, &leaf0, &proof);
    ASSERT_EQ(r, FORTIAS_ERR_BAD_PROOF, "tampered sibling fails verification");
}

static void test_merkle_verify_wrong_root(void) {
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    FortiasHash32 leaf0, leaf1, root, wrong_root;

    fortias_merkle_leaf(a, 1, &leaf0);
    fortias_merkle_leaf(b, 1, &leaf1);
    fortias_hash_sha256_concat(leaf0.bytes, 32, leaf1.bytes, 32, &root);

    FortiasMerkleProof proof;
    memset(&proof, 0, sizeof(proof));
    proof.siblings[0] = leaf1;
    proof.directions[0] = 0;
    proof.depth = 1;

    /* Use a wrong root */
    memset(wrong_root.bytes, 0xFF, 32);
    FortiasResult r = fortias_merkle_verify(&wrong_root, &leaf0, &proof);
    ASSERT_EQ(r, FORTIAS_ERR_BAD_PROOF, "wrong root fails verification");
}

int test_merkle_main(void) {
    printf("=== merkle ===\n");
    test_merkle_leaf_deterministic();
    test_merkle_leaf_same_data();
    test_merkle_verify_correct_proof();
    test_merkle_verify_tampered_sibling();
    test_merkle_verify_wrong_root();
    TEST_REPORT("merkle");
    return test_suite_finish();
}
