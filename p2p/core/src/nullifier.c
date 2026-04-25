#include "platform.h"
#include "fortias_core.h"
#include <sodium.h>

FortiasResult fortias_nullifier_derive(
    const FortiasPrivKey32* priv,
    const uint8_t*          context,
    size_t                  context_len,
    FortiasNullifier*       out
) {
    if (priv == NULL || context == NULL || out == NULL) {
        return FORTIAS_ERR_BAD_INPUT;
    }

    unsigned char hmac[crypto_auth_hmacsha256_BYTES];
    crypto_auth_hmacsha256(hmac, context, (unsigned long long)context_len, priv->bytes);
    memcpy(out->bytes, hmac, 32);

    return FORTIAS_OK;
}
