#include "fortias_core.h"

/*
 * WARNING: This function computes SHA-1 which is CRYPTOGRAPHICALLY WEAK.
 * Use ONLY for non-security purposes (file checksums, protocol interop).
 * For security, use fortias_hash_sha256 or fortias_hash_blake3.
 */

FortiasResult fortias_hash_legacy_insecure_sha1(const uint8_t* data, size_t len, FortiasHash20* out) {
    (void)data;
    (void)len;
    (void)out;
    return FORTIAS_ERR_UNSUPPORTED;
}
