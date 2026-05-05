#pragma once

#include "foretias_core.h"

// Validation helpers
bool foretias_sig_algorithm_accepted(ForetiasSignatureAlgorithm alg);
bool foretias_kem_algorithm_accepted(ForetiasKemAlgorithm alg);

// Parse from string (returns FORETIAS_SIG_ED25519 / FORETIAS_KEM_NOISE_XX as sentinel on failure)
ForetiasSignatureAlgorithm foretias_sig_algorithm_from_id(const char* id);
ForetiasKemAlgorithm foretias_kem_algorithm_from_id(const char* id);
