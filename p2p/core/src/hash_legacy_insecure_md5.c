#include "foretias_core.h"
#include <openssl/evp.h>

/*
 * WARNING: This function computes MD5 which is CRYPTOGRAPHICALLY BROKEN.
 * Use ONLY for non-security purposes (file checksums, protocol interop).
 * For security, use foretias_hash_sha256 or foretias_hash_blake3.
 */

ForetiasResult foretias_hash_legacy_insecure_md5(const uint8_t* data, size_t len, ForetiasHash16* out) {
    EVP_MD_CTX* ctx = EVP_MD_CTX_new();
    if (!ctx) return FORETIAS_ERR_INTERNAL;

    if (EVP_DigestInit_ex(ctx, EVP_md5(), NULL) != 1 ||
        EVP_DigestUpdate(ctx, data, len) != 1 ||
        EVP_DigestFinal_ex(ctx, out->bytes, NULL) != 1) {
        EVP_MD_CTX_free(ctx);
        return FORETIAS_ERR_INTERNAL;
    }

    EVP_MD_CTX_free(ctx);
    return FORETIAS_OK;
}
