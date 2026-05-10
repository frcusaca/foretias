#include "platform.h"
#include "foretias_core.h"
#include <string.h>

ForetiasResult foretias_tbid_v1_keypair(
    ForetiasTbidV1SecretKey* secret_out,
    ForetiasTbidV1PubKey*    public_out)
{
    if (secret_out == NULL || public_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    ForetiasPubKey32 ed25519_pub;
    ForetiasPrivKey32 ed25519_sk;
    ForetiasResult result = foretias_ed25519_generate_keypair(&ed25519_pub, &ed25519_sk);
    if (result != FORETIAS_OK) {
        return result;
    }

    ForetiasSecretKeyVar slh_dsa_secret = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SECRET_BYTES };
    ForetiasPubKeyVar slh_dsa_public = { .bytes = {0}, .len = FORETIAS_SIG_MAX_PUBKEY_BYTES };
    result = foretias_sphincs_sha2_256f_keypair(&slh_dsa_secret, &slh_dsa_public);
    if (result != FORETIAS_OK) {
        foretias_memzero(&ed25519_sk, sizeof(ed25519_sk));
        return result;
    }

    public_out->ed25519_pub = ed25519_pub;
    memcpy(public_out->slh_dsa_pub, slh_dsa_public.bytes, FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES);

    secret_out->ed25519_sk = ed25519_sk;
    memcpy(secret_out->slh_dsa_sk, slh_dsa_secret.bytes, FORETIAS_TBID_V1_SLH_DSA_SK_BYTES);
    secret_out->slh_dsa_sk_len = FORETIAS_TBID_V1_SLH_DSA_SK_BYTES;

    foretias_memzero(&ed25519_sk, sizeof(ed25519_sk));
    foretias_memzero(slh_dsa_secret.bytes, slh_dsa_secret.len);

    return FORETIAS_OK;
}

ForetiasResult foretias_tbid_v1_sign(
    const ForetiasTbidV1SecretKey* secret,
    const uint8_t*                 msg,
    size_t                         msg_len,
    ForetiasTbidV1Sig*             sig_out)
{
    if (secret == NULL || msg == NULL || sig_out == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (sig_out->len < FORETIAS_TBID_V1_SIG_BYTES) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    ForetiasSig64 ed25519_sig;
    ForetiasResult result = foretias_ed25519_sign(&secret->ed25519_sk, msg, msg_len, &ed25519_sig);
    if (result != FORETIAS_OK) {
        return result;
    }

    ForetiasSecretKeyVar slh_dsa_secret = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SECRET_BYTES };
    memcpy(slh_dsa_secret.bytes, secret->slh_dsa_sk, secret->slh_dsa_sk_len);
    slh_dsa_secret.len = secret->slh_dsa_sk_len;

    ForetiasSigVar slh_dsa_sig = { .bytes = {0}, .len = FORETIAS_SIG_MAX_SIG_BYTES };
    result = foretias_sphincs_sha2_256f_sign(&slh_dsa_secret, msg, msg_len, &slh_dsa_sig);
    if (result != FORETIAS_OK) {
        foretias_memzero(slh_dsa_secret.bytes, slh_dsa_secret.len);
        return result;
    }

    foretias_memzero(slh_dsa_secret.bytes, slh_dsa_secret.len);

    memcpy(sig_out->bytes, ed25519_sig.bytes, FORETIAS_TBID_V1_ED25519_SIG_BYTES);
    memcpy(sig_out->bytes + FORETIAS_TBID_V1_ED25519_SIG_BYTES, slh_dsa_sig.bytes, slh_dsa_sig.len);
    sig_out->len = FORETIAS_TBID_V1_ED25519_SIG_BYTES + slh_dsa_sig.len;

    return FORETIAS_OK;
}

ForetiasResult foretias_tbid_v1_verify(
    const ForetiasTbidV1PubKey*  public_key,
    const uint8_t*               msg,
    size_t                       msg_len,
    const ForetiasTbidV1Sig*     sig)
{
    if (public_key == NULL || msg == NULL || sig == NULL) {
        return FORETIAS_ERR_BAD_INPUT;
    }
    if (sig->len < FORETIAS_TBID_V1_SIG_BYTES) {
        return FORETIAS_ERR_BAD_INPUT;
    }

    ForetiasSig64 ed25519_sig;
    memcpy(ed25519_sig.bytes, sig->bytes, FORETIAS_TBID_V1_ED25519_SIG_BYTES);
    ForetiasResult ed25519_result = foretias_ed25519_verify(&public_key->ed25519_pub, msg, msg_len, &ed25519_sig);
    if (ed25519_result != FORETIAS_OK) {
        return ed25519_result;
    }

    size_t slh_dsa_sig_offset = FORETIAS_TBID_V1_ED25519_SIG_BYTES;
    size_t slh_dsa_sig_len = sig->len - slh_dsa_sig_offset;

    ForetiasPubKeyVar slh_dsa_pub = { .bytes = {0}, .len = FORETIAS_SIG_MAX_PUBKEY_BYTES };
    memcpy(slh_dsa_pub.bytes, public_key->slh_dsa_pub, FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES);
    slh_dsa_pub.len = FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES;

    ForetiasSigVar slh_dsa_sig;
    memcpy(slh_dsa_sig.bytes, sig->bytes + slh_dsa_sig_offset, slh_dsa_sig_len);
    slh_dsa_sig.len = slh_dsa_sig_len;

    return foretias_sphincs_sha2_256f_verify(&slh_dsa_pub, msg, msg_len, &slh_dsa_sig);
}

void foretias_tbid_v1_secret_zeroize(ForetiasTbidV1SecretKey* secret) {
    if (secret == NULL) return;
    foretias_memzero(&secret->ed25519_sk, sizeof(secret->ed25519_sk));
    foretias_memzero(secret->slh_dsa_sk, secret->slh_dsa_sk_len);
    secret->slh_dsa_sk_len = 0;
}
