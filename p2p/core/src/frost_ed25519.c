#include "platform.h"
#include "foretias_core.h"

ForetiasResult foretias_frost_round1(ForetiasFrostRound1* out) {
    (void)out;
    return FORETIAS_ERR_UNSUPPORTED;
}

ForetiasResult foretias_frost_sign_share(
    const ForetiasFrostRound1* my_state,
    const ForetiasFrostShare*  my_key_share,
    const uint8_t*            msg,
    size_t                    msg_len,
    const uint8_t*            all_commits,
    size_t                    n_signers,
    int32_t                   my_index,
    ForetiasFrostShare*        sig_share_out
) {
    (void)my_state;
    (void)my_key_share;
    (void)msg;
    (void)msg_len;
    (void)all_commits;
    (void)n_signers;
    (void)my_index;
    (void)sig_share_out;
    return FORETIAS_ERR_UNSUPPORTED;
}

ForetiasResult foretias_frost_aggregate(
    const ForetiasFrostShare* shares,
    const int32_t*           indices,
    size_t                   k,
    const uint8_t*           all_commits,
    const uint8_t*           msg,
    size_t                   msg_len,
    ForetiasSig64*            sig_out
) {
    (void)shares;
    (void)indices;
    (void)k;
    (void)all_commits;
    (void)msg;
    (void)msg_len;
    (void)sig_out;
    return FORETIAS_ERR_UNSUPPORTED;
}

void foretias_frost_destroy_round1(ForetiasFrostRound1* state) {
    if (state) foretias_memzero(state, sizeof(ForetiasFrostRound1));
}
