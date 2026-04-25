#include "platform.h"
#include "fortias_core.h"

FortiasResult fortias_p256_sign(
    const FortiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    FortiasSig64*           sig_out
) {
    (void)priv;
    (void)msg;
    (void)msg_len;
    (void)sig_out;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_p256_verify(
    const FortiasPubKey33*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const FortiasSig64*     sig
) {
    (void)pub;
    (void)msg;
    (void)msg_len;
    (void)sig;
    return FORTIAS_ERR_UNSUPPORTED;
}
