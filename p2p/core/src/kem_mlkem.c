#include "platform.h"
#include "foretias_core.h"
#include <oqs/oqs.h>

#ifdef OQS_ENABLE_KEM_ml_kem_768

ForetiasResult foretias_mlkem_768_keypair(
    ForetiasKemSecretKey* secret_out,
    ForetiasKemPubKey*    public_out
) {
    if (secret_out == NULL || public_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_KEM_ml_kem_768_keypair(public_out->bytes, secret_out->bytes);
    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(secret_out->bytes, secret_out->len);
        return FORETIAS_ERR_INTERNAL;
    }

    secret_out->len = OQS_KEM_ml_kem_768_length_secret_key;
    public_out->len = OQS_KEM_ml_kem_768_length_public_key;

    return FORETIAS_OK;
}

ForetiasResult foretias_mlkem_768_encapsulate(
    const ForetiasKemPubKey*  public_key,
    ForetiasKemCiphertext*    ciphertext_out,
    uint8_t*                  shared_secret_out
) {
    if (public_key == NULL || ciphertext_out == NULL || shared_secret_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_KEM_ml_kem_768_encaps(
        ciphertext_out->bytes, shared_secret_out, public_key->bytes
    );
    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(shared_secret_out, FORETIAS_KEM_SHARED_SECRET);
        return FORETIAS_ERR_INTERNAL;
    }

    ciphertext_out->len = OQS_KEM_ml_kem_768_length_ciphertext;

    return FORETIAS_OK;
}

ForetiasResult foretias_mlkem_768_decapsulate(
    const ForetiasKemSecretKey* secret,
    const ForetiasKemCiphertext* ciphertext,
    uint8_t*                    shared_secret_out
) {
    if (secret == NULL || ciphertext == NULL || shared_secret_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    OQS_STATUS st = OQS_KEM_ml_kem_768_decaps(
        shared_secret_out, ciphertext->bytes, secret->bytes
    );
    if (st != OQS_SUCCESS) {
        OQS_MEM_cleanse(shared_secret_out, FORETIAS_KEM_SHARED_SECRET);
        return FORETIAS_ERR_INTERNAL;
    }

    return FORETIAS_OK;
}

#endif
