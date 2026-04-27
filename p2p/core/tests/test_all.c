#include "test_runner.h"

int test_memzero_main(void);
int test_rng_main(void);
int test_version_main(void);
int test_ed25519_main(void);
int test_sha256_main(void);
int test_merkle_main(void);
int test_nullifier_main(void);
int test_p256_stub_main(void);
int test_blake3_stub_main(void);
int test_noise_stub_main(void);
int test_frost_stub_main(void);
int test_legacy_hash_stub_main(void);

int main(void) {
    printf("Fortias Core Test Suite\n");
    printf("=======================\n\n");

    int failures = 0;

    failures += test_memzero_main();
    failures += test_rng_main();
    failures += test_version_main();
    failures += test_ed25519_main();
    failures += test_sha256_main();
    failures += test_merkle_main();
    failures += test_nullifier_main();
    failures += test_p256_stub_main();
    failures += test_blake3_stub_main();
    failures += test_noise_stub_main();
    failures += test_frost_stub_main();
    failures += test_legacy_hash_stub_main();

    printf("\n=======================\n");
    if (failures == 0) {
        printf("ALL TESTS PASSED\n");
    } else {
        printf("TESTS FAILED in %d suite(s)\n", failures);
    }

    return failures;
}
