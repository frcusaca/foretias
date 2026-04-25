#include "platform.h"
#include "fortias_core.h"

FortiasResult fortias_frost_round1(FortiasFrostRound1* out) {
    (void)out;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_frost_sign_share(
    const FortiasFrostRound1* my_state,
    const FortiasFrostShare*  my_key_share,
    const uint8_t*            msg,
    size_t                    msg_len,
    const uint8_t*            all_commits,
    size_t                    n_signers,
    int32_t                   my_index,
    FortiasFrostShare*        sig_share_out
) {
    (void)my_state;
    (void)my_key_share;
    (void)msg;
    (void)msg_len;
    (void)all_commits;
    (void)n_signers;
    (void)my_index;
    (void)sig_share_out;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_frost_aggregate(
    const FortiasFrostShare* shares,
    const int32_t*           indices,
    size_t                   k,
    const uint8_t*           all_commits,
    const uint8_t*           msg,
    size_t                   msg_len,
    FortiasSig64*            sig_out
) {
    (void)shares;
    (void)indices;
    (void)k;
    (void)all_commits;
    (void)msg;
    (void)msg_len;
    (void)sig_out;
    return FORTIAS_ERR_UNSUPPORTED;
}

void fortias_frost_destroy_round1(FortiasFrostRound1* state) {
    if (state) fortias_memzero(state, sizeof(FortiasFrostRound1));
}
