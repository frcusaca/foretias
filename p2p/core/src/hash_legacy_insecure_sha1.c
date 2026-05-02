#include "fortias_core.h"
#include <openssl/evp.h>

/*
 * WARNING: This function computes SHA-1 which is CRYPTOGRAPHICALLY WEAK.
 * Use ONLY for non-security purposes (file checksums, protocol interop).
 * For security, use fortias_hash_sha256 or fortias_hash_blake3.
 */

FortiasResult fortias_hash_legacy_insecure_sha1(const uint8_t* data, size_t len, FortiasHash20* out) {
    EVP_MD_CTX* ctx = EVP_MD_CTX_new();
    if (!ctx) return FORTIAS_ERR_INTERNAL;

    if (EVP_DigestInit_ex(ctx, EVP_sha1(), NULL) != 1 ||
        EVP_DigestUpdate(ctx, data, len) != 1 ||
        EVP_DigestFinal_ex(ctx, out->bytes, NULL) != 1) {
        EVP_MD_CTX_free(ctx);
        return FORTIAS_ERR_INTERNAL;
    }

    EVP_MD_CTX_free(ctx);
    return FORTIAS_OK;
}
