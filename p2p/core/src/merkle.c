#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>

static void sha256_two_32byte(const uint8_t* a, const uint8_t* b, uint8_t* out) {
    crypto_hash_sha256_state state;
    crypto_hash_sha256_init(&state);
    crypto_hash_sha256_update(&state, a, 32);
    crypto_hash_sha256_update(&state, b, 32);
    crypto_hash_sha256_final(&state, out);
}

FortiasResult fortias_merkle_leaf(const uint8_t* data, size_t len, FortiasHash32* leaf_out) {
    if (data == NULL || leaf_out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    crypto_hash_sha256_state state;
    if (crypto_hash_sha256_init(&state) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    uint8_t prefix = 0x00;
    if (crypto_hash_sha256_update(&state, &prefix, 1) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_update(&state, data, (unsigned long long)len) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_final(&state, leaf_out->bytes) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    return FORTIAS_OK;
}

FortiasResult fortias_merkle_verify(
    const FortiasHash32*      root,
    const FortiasHash32*      leaf,
    const FortiasMerkleProof* proof
) {
    if (root == NULL || leaf == NULL || proof == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    if (proof->depth <= 0 || proof->depth > FORTIAS_MERKLE_MAX_DEPTH) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    FortiasHash32 current = *leaf;

    for (int32_t i = 0; i < proof->depth; i++) {
        uint8_t direction = proof->directions[i];
        uint8_t hash[32];

        if (direction == 0) {
            sha256_two_32byte(proof->siblings[i].bytes, current.bytes, hash);
        } else if (direction == 1) {
            sha256_two_32byte(current.bytes, proof->siblings[i].bytes, hash);
        } else {
            return FORTIAS_ERR_BAD_PROOF;
        }

        memcpy(current.bytes, hash, 32);
    }

    if (memcmp(current.bytes, root->bytes, 32) == 0) {
        return FORTIAS_OK;
    }

    return FORTIAS_ERR_BAD_PROOF;
}
