#include "test_runner.h"

static void test_range_proof_empty_range(void) {
    ForetiasHash32 leaves[4];
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    foretias_merkle_leaf(a, 1, &leaves[0]);
    foretias_merkle_leaf(b, 1, &leaves[1]);

    ForetiasMerkleRangeProof proof;
    ForetiasResult r = foretias_merkle_range_proof(leaves, 2, 1, 1, &proof);
    ASSERT_EQ(r, FORETIAS_ERR_PROOF_RANGE_EMPTY, "empty range returns PROOF_RANGE_EMPTY");
}

static void test_range_proof_single_leaf(void) {
    const uint8_t a[] = "only";
    ForetiasHash32 leaf;
    foretias_merkle_leaf(a, 4, &leaf);

    ForetiasHash32 root;
    ForetiasResult r = foretias_merkle_root_from_leaves(&leaf, 1, &root);
    ASSERT_EQ(r, FORETIAS_OK, "root from single leaf succeeds");

    ForetiasMerkleRangeProof proof;
    r = foretias_merkle_range_proof(&leaf, 1, 0, 1, &proof);
    ASSERT_EQ(r, FORETIAS_OK, "range proof for single leaf succeeds");
    ASSERT_EQ(proof.leaf_count, 1, "proof has 1 leaf");
    ASSERT_EQ(proof.sibling_count, 0, "proof has 0 siblings");

    ForetiasResult verify_result;
    r = foretias_merkle_verify_range_proof(&root, &proof, 0, 1, &verify_result);
    ASSERT_EQ(r, FORETIAS_OK, "verify returns OK");
    ASSERT_EQ(verify_result, FORETIAS_OK, "single leaf proof verifies");
}

static void test_range_proof_two_leaves(void) {
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    ForetiasHash32 leaves[2];
    foretias_merkle_leaf(a, 1, &leaves[0]);
    foretias_merkle_leaf(b, 1, &leaves[1]);

    ForetiasHash32 root;
    ForetiasResult r = foretias_merkle_root_from_leaves(leaves, 2, &root);
    ASSERT_EQ(r, FORETIAS_OK, "root from 2 leaves succeeds");

    ForetiasMerkleRangeProof proof;
    r = foretias_merkle_range_proof(leaves, 2, 0, 2, &proof);
    ASSERT_EQ(r, FORETIAS_OK, "range proof for 2 leaves succeeds");
    ASSERT_EQ(proof.leaf_count, 2, "proof has 2 leaves");
    ASSERT_EQ(proof.sibling_count, 0, "proof has 0 siblings");

    ForetiasResult verify_result;
    r = foretias_merkle_verify_range_proof(&root, &proof, 0, 2, &verify_result);
    ASSERT_EQ(r, FORETIAS_OK, "verify returns OK");
    ASSERT_EQ(verify_result, FORETIAS_OK, "two leaf proof verifies");
}

static void test_range_proof_adjacent_range(void) {
    ForetiasHash32 leaves[4];
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    const uint8_t c[] = "c";
    const uint8_t d[] = "d";
    foretias_merkle_leaf(a, 1, &leaves[0]);
    foretias_merkle_leaf(b, 1, &leaves[1]);
    foretias_merkle_leaf(c, 1, &leaves[2]);
    foretias_merkle_leaf(d, 1, &leaves[3]);

    ForetiasHash32 root;
    ForetiasResult r = foretias_merkle_root_from_leaves(leaves, 4, &root);
    ASSERT_EQ(r, FORETIAS_OK, "root from 4 leaves succeeds");

    ForetiasMerkleRangeProof proof;
    r = foretias_merkle_range_proof(leaves, 4, 1, 3, &proof);
    ASSERT_EQ(r, FORETIAS_OK, "range proof for [1,3) succeeds");
    ASSERT_EQ(proof.leaf_count, 2, "proof has 2 leaves");

    ForetiasResult verify_result;
    r = foretias_merkle_verify_range_proof(&root, &proof, 1, 3, &verify_result);
    ASSERT_EQ(r, FORETIAS_OK, "verify returns OK");
    ASSERT_EQ(verify_result, FORETIAS_OK, "adjacent range proof verifies");
}

static void test_range_proof_full_range(void) {
    ForetiasHash32 leaves[4];
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    const uint8_t c[] = "c";
    const uint8_t d[] = "d";
    foretias_merkle_leaf(a, 1, &leaves[0]);
    foretias_merkle_leaf(b, 1, &leaves[1]);
    foretias_merkle_leaf(c, 1, &leaves[2]);
    foretias_merkle_leaf(d, 1, &leaves[3]);

    ForetiasHash32 root;
    ForetiasResult r = foretias_merkle_root_from_leaves(leaves, 4, &root);
    ASSERT_EQ(r, FORETIAS_OK, "root from 4 leaves succeeds");

    ForetiasMerkleRangeProof proof;
    r = foretias_merkle_range_proof(leaves, 4, 0, 4, &proof);
    ASSERT_EQ(r, FORETIAS_OK, "range proof for full range succeeds");
    ASSERT_EQ(proof.leaf_count, 4, "proof has 4 leaves");
    ASSERT_EQ(proof.sibling_count, 0, "proof has 0 siblings for full range");

    ForetiasResult verify_result;
    r = foretias_merkle_verify_range_proof(&root, &proof, 0, 4, &verify_result);
    ASSERT_EQ(r, FORETIAS_OK, "verify returns OK");
    ASSERT_EQ(verify_result, FORETIAS_OK, "full range proof verifies");
}

static void test_range_proof_out_of_bounds(void) {
    ForetiasHash32 leaves[2];
    const uint8_t a[] = "a";
    const uint8_t b[] = "b";
    foretias_merkle_leaf(a, 1, &leaves[0]);
    foretias_merkle_leaf(b, 1, &leaves[1]);

    ForetiasMerkleRangeProof proof;
    ForetiasResult r = foretias_merkle_range_proof(leaves, 2, 0, 5, &proof);
    ASSERT_EQ(r, FORETIAS_ERR_PROOF_RANGE_EXCEEDS, "end > n returns PROOF_RANGE_EXCEEDS");

    r = foretias_merkle_range_proof(leaves, 2, 0, 3, &proof);
    ASSERT_EQ(r, FORETIAS_ERR_PROOF_RANGE_EXCEEDS, "end > n returns PROOF_RANGE_EXCEEDS (2)");
}

int test_merkle_range_main(void) {
    printf("=== merkle range ===\n");
    test_range_proof_empty_range();
    test_range_proof_single_leaf();
    test_range_proof_two_leaves();
    test_range_proof_adjacent_range();
    test_range_proof_full_range();
    test_range_proof_out_of_bounds();
    TEST_REPORT("merkle range");
    return test_suite_finish();
}
