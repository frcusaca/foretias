#include "platform.h"
#include "fortias_core.h"

FortiasResult fortias_p256_generate_keypair(
    FortiasPubKey33*  pub_out,
    FortiasPrivKey32* priv_out
) {
    (void)pub_out;
    (void)priv_out;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_p256_derive_peer_id(
    const FortiasPubKey33* pub,
    FortiasPeerID*         id_out
) {
    (void)pub;
    (void)id_out;
    return FORTIAS_ERR_UNSUPPORTED;
}
