#include "algorithms.h"
#include <string.h>

static const ForetiasSignatureAlgorithm ACCEPTED_SIG_ALGORITHMS[] = {
    FORETIAS_SIG_ED25519,
    FORETIAS_SIG_SPHINCS_SHA2_128S,
    FORETIAS_SIG_DILITHIUM3,
};
#define ACCEPTED_SIG_ALGORITHMS_COUNT 3

static const ForetiasKemAlgorithm ACCEPTED_KEM_ALGORITHMS[] = {
    FORETIAS_KEM_NOISE_XX,
    FORETIAS_KEM_MLKEM_768,
};
#define ACCEPTED_KEM_ALGORITHMS_COUNT 2

const char* foretias_sig_algorithm_id(ForetiasSignatureAlgorithm alg) {
    switch (alg) {
        case FORETIAS_SIG_ED25519:
            return FORETIAS_SIG_ID_ED25519;
        case FORETIAS_SIG_SPHINCS_SHA2_128S:
            return FORETIAS_SIG_ID_SPHINCS_SHA2_128S;
        case FORETIAS_SIG_DILITHIUM3:
            return FORETIAS_SIG_ID_DILITHIUM3;
        default:
            return NULL;
    }
}

const char* foretias_kem_algorithm_id(ForetiasKemAlgorithm alg) {
    switch (alg) {
        case FORETIAS_KEM_NOISE_XX:
            return FORETIAS_KEM_ID_NOISE_XX;
        case FORETIAS_KEM_MLKEM_768:
            return FORETIAS_KEM_ID_MLKEM_768;
        default:
            return NULL;
    }
}

size_t foretias_sig_pubkey_bytes(ForetiasSignatureAlgorithm alg) {
    switch (alg) {
        case FORETIAS_SIG_ED25519:
            return 32;
        case FORETIAS_SIG_SPHINCS_SHA2_128S:
            return 32;
        case FORETIAS_SIG_DILITHIUM3:
            return 1952;
        default:
            return 0;
    }
}

size_t foretias_sig_secret_bytes(ForetiasSignatureAlgorithm alg) {
    switch (alg) {
        case FORETIAS_SIG_ED25519:
            return 32;
        case FORETIAS_SIG_SPHINCS_SHA2_128S:
            return 64;
        case FORETIAS_SIG_DILITHIUM3:
            return 4000;
        default:
            return 0;
    }
}

size_t foretias_sig_signature_bytes(ForetiasSignatureAlgorithm alg) {
    switch (alg) {
        case FORETIAS_SIG_ED25519:
            return 64;
        case FORETIAS_SIG_SPHINCS_SHA2_128S:
            return 7856;
        case FORETIAS_SIG_DILITHIUM3:
            return 3309;
        default:
            return 0;
    }
}

bool foretias_sig_algorithm_accepted(ForetiasSignatureAlgorithm alg) {
    for (size_t i = 0; i < ACCEPTED_SIG_ALGORITHMS_COUNT; ++i) {
        if (ACCEPTED_SIG_ALGORITHMS[i] == alg)
            return true;
    }
    return false;
}

bool foretias_kem_algorithm_accepted(ForetiasKemAlgorithm alg) {
    for (size_t i = 0; i < ACCEPTED_KEM_ALGORITHMS_COUNT; ++i) {
        if (ACCEPTED_KEM_ALGORITHMS[i] == alg)
            return true;
    }
    return false;
}

ForetiasSignatureAlgorithm foretias_sig_algorithm_from_id(const char* id) {
    if (id == NULL)
        return FORETIAS_SIG_ED25519;

    if (strcmp(id, FORETIAS_SIG_ID_ED25519) == 0)
        return FORETIAS_SIG_ED25519;
    if (strcmp(id, FORETIAS_SIG_ID_SPHINCS_SHA2_128S) == 0)
        return FORETIAS_SIG_SPHINCS_SHA2_128S;
    if (strcmp(id, FORETIAS_SIG_ID_DILITHIUM3) == 0)
        return FORETIAS_SIG_DILITHIUM3;

    return FORETIAS_SIG_ED25519;
}

ForetiasKemAlgorithm foretias_kem_algorithm_from_id(const char* id) {
    if (id == NULL)
        return FORETIAS_KEM_NOISE_XX;

    if (strcmp(id, FORETIAS_KEM_ID_NOISE_XX) == 0)
        return FORETIAS_KEM_NOISE_XX;
    if (strcmp(id, FORETIAS_KEM_ID_MLKEM_768) == 0)
        return FORETIAS_KEM_MLKEM_768;

    return FORETIAS_KEM_NOISE_XX;
}
