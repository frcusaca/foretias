#include "platform.h"
#include "foretias_core.h"
#include <sodium.h>

/*@ requires \valid(a + (0 .. 31));
  @ requires \valid(b + (0 .. 31));
  @ requires \valid(out + (0 .. 31));
  @ assigns out->bytes[0 .. 31];
  @*/
static void sha256_two_32byte(const uint8_t* a, const uint8_t* b, uint8_t* out) {
    crypto_hash_sha256_state state;
    crypto_hash_sha256_init(&state);
    crypto_hash_sha256_update(&state, a, 32);
    crypto_hash_sha256_update(&state, b, 32);
    crypto_hash_sha256_final(&state, out);
}

/*@ requires data == NULL || \valid_read(data + (0 .. len-1));
  @ requires \valid(leaf_out);
  @ assigns leaf_out->bytes[0 .. 31];
  @ ensures \result == FORETIAS_OK ==> \valid_read(leaf_out->bytes + (0 .. 31));
  @*/
ForetiasResult foretias_merkle_leaf(const uint8_t* data, size_t len, ForetiasHash32* leaf_out) {
    if (data == NULL || leaf_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    crypto_hash_sha256_state state;
    if (crypto_hash_sha256_init(&state) != 0) {
        return FORETIAS_ERR_INTERNAL;
    }

    uint8_t prefix = 0x00;
    if (crypto_hash_sha256_update(&state, &prefix, 1) != 0) {
        return FORETIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_update(&state, data, (unsigned long long)len) != 0) {
        return FORETIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_final(&state, leaf_out->bytes) != 0) {
        return FORETIAS_ERR_INTERNAL;
    }

    return FORETIAS_OK;
}

/*@ requires \valid(root);
  @ requires \valid(leaf);
  @ requires \valid(proof);
  @ requires proof->depth > 0 && proof->depth <= FORETIAS_MERKLE_MAX_DEPTH;
  @ requires \valid(proof->siblings + (0 .. proof->depth-1));
  @ requires \valid(proof->directions + (0 .. proof->depth-1));
  @ ensures \result == FORETIAS_OK || \result == FORETIAS_ERR_BAD_PROOF;
  @ behavior pure:
  @   reads root->bytes[0 .. 31];
  @   reads leaf->bytes[0 .. 31];
  @   reads proof->siblings[0 .. proof->depth-1]->bytes[0 .. 31];
  @   reads proof->directions[0 .. proof->depth-1];
  @*/
ForetiasResult foretias_merkle_verify(
    const ForetiasHash32*      root,
    const ForetiasHash32*      leaf,
    const ForetiasMerkleProof* proof
) {
    if (root == NULL || leaf == NULL || proof == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    if (proof->depth <= 0 || proof->depth > FORETIAS_MERKLE_MAX_DEPTH) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    ForetiasHash32 current = *leaf;

    for (int32_t i = 0; i < proof->depth; i++) {
        uint8_t direction = proof->directions[i];
        uint8_t hash[32];

        if (direction == 0) {
            sha256_two_32byte(proof->siblings[i].bytes, current.bytes, hash);
        } else if (direction == 1) {
            sha256_two_32byte(current.bytes, proof->siblings[i].bytes, hash);
        } else {
            return FORETIAS_ERR_BAD_PROOF;
        }

        memcpy(current.bytes, hash, 32);
    }

    if (memcmp(current.bytes, root->bytes, 32) == 0) {
        return FORETIAS_OK;
    }

    return FORETIAS_ERR_BAD_PROOF;
}

#define MERKLE_MAX_TREE_PADDED 128

static void merkle_node_hash(const uint8_t* left, const uint8_t* right, uint8_t* out) {
    crypto_hash_sha256_state state;
    crypto_hash_sha256_init(&state);
    uint8_t prefix = 0x01;
    crypto_hash_sha256_update(&state, &prefix, 1);
    crypto_hash_sha256_update(&state, left, 32);
    crypto_hash_sha256_update(&state, right, 32);
    crypto_hash_sha256_final(&state, out);
}

static size_t merkle_next_pow2(size_t n) {
    size_t p = 1;
    while (p < n) {
        p <<= 1;
    }
    return p;
}

static int merkle_log2(size_t n) {
    int d = 0;
    while (n > 1) {
        n >>= 1;
        d++;
    }
    return d;
}

ForetiasResult foretias_merkle_root_from_leaves(
    const ForetiasHash32* leaves,
    size_t                n,
    ForetiasHash32*       root_out
) {
    if (leaves == NULL || root_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (n == 0) {
        return FORETIAS_ERR_PROOF_RANGE_EMPTY;
    }

    size_t padded = merkle_next_pow2(n);
    if (padded > MERKLE_MAX_TREE_PADDED) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    ForetiasHash32 buf0[MERKLE_MAX_TREE_PADDED];
    ForetiasHash32 buf1[MERKLE_MAX_TREE_PADDED];
    memset(buf0, 0, sizeof(buf0));
    for (size_t i = 0; i < n; i++) {
        buf0[i] = leaves[i];
    }

    size_t count = padded;
    ForetiasHash32* cur = buf0;
    ForetiasHash32* nxt = buf1;

    while (count > 1) {
        for (size_t i = 0; i < count; i += 2) {
            merkle_node_hash(cur[i].bytes, cur[i + 1].bytes, nxt[i / 2].bytes);
        }
        count /= 2;
        ForetiasHash32* tmp = cur;
        cur = nxt;
        nxt = tmp;
    }

    *root_out = cur[0];
    return FORETIAS_OK;
}

ForetiasResult foretias_merkle_range_proof(
    const ForetiasHash32*     leaves,
    size_t                    n,
    size_t                    start,
    size_t                    end,
    ForetiasMerkleRangeProof* proof_out
) {
    if (leaves == NULL || proof_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (n == 0) {
        return FORETIAS_ERR_PROOF_RANGE_EMPTY;
    }
    if (start >= end) {
        return FORETIAS_ERR_PROOF_RANGE_EMPTY;
    }
    if (end > n) {
        return FORETIAS_ERR_PROOF_RANGE_EXCEEDS;
    }
    if ((int32_t)(end - start) > FORETIAS_MERKLE_MAX_RANGE_PROOF) {
        return FORETIAS_ERR_PROOF_RANGE_EXCEEDS;
    }

    size_t padded = merkle_next_pow2(n);
    if (padded > MERKLE_MAX_TREE_PADDED) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    int depth = merkle_log2(padded);

    size_t offsets[8];
    offsets[0] = 0;
    for (int k = 1; k <= depth; k++) {
        offsets[k] = offsets[k - 1] + (padded >> (k - 1));
    }

    size_t total_nodes = offsets[depth] + 1;
    ForetiasHash32 tree[MERKLE_MAX_TREE_PADDED * 2];
    memset(tree, 0, total_nodes * sizeof(ForetiasHash32));
    for (size_t i = 0; i < n; i++) {
        tree[i] = leaves[i];
    }

    for (int k = 0; k < depth; k++) {
        size_t count = padded >> k;
        for (size_t i = 0; i < count; i += 2) {
            size_t parent_idx = offsets[k + 1] + i / 2;
            merkle_node_hash(
                tree[offsets[k] + i].bytes,
                tree[offsets[k] + i + 1].bytes,
                tree[parent_idx].bytes
            );
        }
    }

    memset(proof_out, 0, sizeof(*proof_out));
    proof_out->n = (int32_t)n;
    proof_out->leaf_count = (int32_t)(end - start);
    for (size_t i = start; i < end; i++) {
        proof_out->leaves[i - start] = tree[i];
    }

    size_t s = start;
    size_t e = end;
    int32_t sib_idx = 0;

    for (int k = 0; k < depth; k++) {
        size_t level_n = padded >> k;
        if (s % 2 == 1) {
            proof_out->siblings[sib_idx++] = tree[offsets[k] + s - 1];
        }
        if (e % 2 == 1 && e < level_n) {
            proof_out->siblings[sib_idx++] = tree[offsets[k] + e];
        }
        s /= 2;
        e = (e + 1) / 2;
    }

    proof_out->sibling_count = sib_idx;
    return FORETIAS_OK;
}

ForetiasResult foretias_merkle_verify_range_proof(
    const ForetiasHash32*       root,
    const ForetiasMerkleRangeProof* proof,
    size_t                   start,
    size_t                   end,
    ForetiasResult*          result_out
) {
    if (root == NULL || proof == NULL || result_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (proof->n <= 0) {
        *result_out = FORETIAS_ERR_PROOF_RANGE_EMPTY;
        return FORETIAS_OK;
    }
    if (start >= end) {
        *result_out = FORETIAS_ERR_PROOF_RANGE_EMPTY;
        return FORETIAS_OK;
    }
    if (end > (size_t)proof->n) {
        *result_out = FORETIAS_ERR_PROOF_RANGE_EXCEEDS;
        return FORETIAS_OK;
    }
    if (proof->leaf_count != (int32_t)(end - start)) {
        *result_out = FORETIAS_ERR_BAD_PROOF;
        return FORETIAS_OK;
    }

    size_t padded = merkle_next_pow2((size_t)proof->n);
    int depth = merkle_log2(padded);

    ForetiasHash32 level_nodes[MERKLE_MAX_TREE_PADDED];
    size_t level_count = end - start;
    for (size_t i = 0; i < level_count; i++) {
        level_nodes[i] = proof->leaves[i];
    }

    ForetiasHash32 next_nodes[MERKLE_MAX_TREE_PADDED];
    size_t sibling_idx = 0;
    size_t s = start;
    size_t e = end;

    for (int k = 0; k < depth; k++) {
        size_t next_count = 0;
        size_t i = 0;

        if (s % 2 == 1) {
            merkle_node_hash(
                proof->siblings[sibling_idx].bytes,
                level_nodes[0].bytes,
                next_nodes[next_count].bytes
            );
            sibling_idx++;
            next_count++;
            i = 1;
        }

        while (i + 1 < level_count) {
            merkle_node_hash(
                level_nodes[i].bytes,
                level_nodes[i + 1].bytes,
                next_nodes[next_count].bytes
            );
            next_count++;
            i += 2;
        }

        if (i < level_count) {
            merkle_node_hash(
                level_nodes[i].bytes,
                proof->siblings[sibling_idx].bytes,
                next_nodes[next_count].bytes
            );
            sibling_idx++;
            next_count++;
        }

        for (size_t j = 0; j < next_count; j++) {
            level_nodes[j] = next_nodes[j];
        }
        level_count = next_count;
        s /= 2;
        e = (e + 1) / 2;
    }

    if (level_count == 1 && memcmp(level_nodes[0].bytes, root->bytes, 32) == 0) {
        *result_out = FORETIAS_OK;
    } else {
        *result_out = FORETIAS_ERR_BAD_PROOF;
    }

    return FORETIAS_OK;
}
