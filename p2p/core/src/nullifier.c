#include "platform.h"
#include "foretias_core.h"
#include <sodium.h>

ForetiasResult foretias_nullifier_derive(
    const ForetiasPrivKey32* priv,
    const uint8_t*          context,
    size_t                  context_len,
    ForetiasNullifier*       out
) {
    if (priv == NULL || context == NULL || out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    unsigned char hmac[crypto_auth_hmacsha256_BYTES];
    crypto_auth_hmacsha256(hmac, context, (unsigned long long)context_len, priv->bytes);
    memcpy(out->bytes, hmac, 32);

    return FORETIAS_OK;
}
