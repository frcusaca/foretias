#include "platform.h"
#include "foretias_core.h"

ForetiasResult foretias_p256_generate_keypair(
    ForetiasPubKey33*  pub_out,
    ForetiasPrivKey32* priv_out
) {
    (void)pub_out;
    (void)priv_out;
    return FORETIAS_ERR_UNSUPPORTED;
}

ForetiasResult foretias_p256_derive_peer_id(
    const ForetiasPubKey33* pub,
    ForetiasPeerID*         id_out
) {
    (void)pub;
    (void)id_out;
    return FORETIAS_ERR_UNSUPPORTED;
}
