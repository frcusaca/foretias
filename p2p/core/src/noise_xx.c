#include "platform.h"
#include "fortias_core.h"

FortiasResult fortias_noise_init_ed25519(
    FortiasNoiseState*      state,
    const FortiasPrivKey32* my_static_priv,
    const FortiasPubKey32*  their_static_pub,
    bool                    is_initiator
) {
    (void)state;
    (void)my_static_priv;
    (void)their_static_pub;
    (void)is_initiator;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_noise_init_p256(
    FortiasNoiseState*      state,
    const FortiasPrivKey32* my_static_priv,
    const FortiasPubKey33*  their_static_pub,
    bool                    is_initiator
) {
    (void)state;
    (void)my_static_priv;
    (void)their_static_pub;
    (void)is_initiator;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_noise_step(
    FortiasNoiseState* state,
    const uint8_t*     input,
    size_t             input_len,
    uint8_t*           output,
    size_t*            output_len
) {
    (void)state;
    (void)input;
    (void)input_len;
    (void)output;
    (void)output_len;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_noise_send(
    FortiasNoiseState* state,
    const uint8_t*     plaintext,
    size_t             pt_len,
    uint8_t*           ciphertext,
    size_t*            ct_len
) {
    (void)state;
    (void)plaintext;
    (void)pt_len;
    (void)ciphertext;
    (void)ct_len;
    return FORTIAS_ERR_UNSUPPORTED;
}

FortiasResult fortias_noise_recv(
    FortiasNoiseState* state,
    const uint8_t*     ciphertext,
    size_t             ct_len,
    uint8_t*           plaintext,
    size_t*            pt_len
) {
    (void)state;
    (void)ciphertext;
    (void)ct_len;
    (void)plaintext;
    (void)pt_len;
    return FORTIAS_ERR_UNSUPPORTED;
}

void fortias_noise_destroy(FortiasNoiseState* state) {
    if (state) fortias_memzero(state, sizeof(FortiasNoiseState));
}
