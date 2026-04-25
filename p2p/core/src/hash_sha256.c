#include "fortias_core.h"
#include <sodium.h>

FortiasResult fortias_hash_sha256(const uint8_t* data, size_t len, FortiasHash32* out) {
    if (data == NULL || out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }
    crypto_hash_sha256(out->bytes, data, (unsigned long long)len);
    return FORTIAS_OK;
}

FortiasResult fortias_hash_sha256_concat(const uint8_t* a, size_t a_len, const uint8_t* b, size_t b_len, FortiasHash32* out) {
    if (a == NULL || b == NULL || out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    crypto_hash_sha256_state state;
    if (crypto_hash_sha256_init(&state) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_update(&state, a, (unsigned long long)a_len) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_update(&state, b, (unsigned long long)b_len) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    if (crypto_hash_sha256_final(&state, out->bytes) != 0) {
        return FORTIAS_ERR_INTERNAL;
    }

    return FORTIAS_OK;
}
