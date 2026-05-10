#include "platform.h"
#include "foretias_core.h"

ForetiasResult foretias_p256_sign(
    const ForetiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    ForetiasSig64*           sig_out
) {
    (void)priv;
    (void)msg;
    (void)msg_len;
    (void)sig_out;
    return FORETIAS_ERR_UNSUPPORTED;
}

ForetiasResult foretias_p256_verify(
    const ForetiasPubKey33*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const ForetiasSig64*     sig
) {
    (void)pub;
    (void)msg;
    (void)msg_len;
    (void)sig;
    return FORETIAS_ERR_UNSUPPORTED;
}
