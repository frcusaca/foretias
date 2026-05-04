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
