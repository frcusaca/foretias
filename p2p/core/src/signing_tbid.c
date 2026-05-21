#include "platform.h"
#include "foretias_core.h"
#include <oqs/oqs.h>
#include <string.h>

#ifdef OQS_ENABLE_SIG_sphincs_sha2_256f_simple
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

    uint8_t plaintext_slh[OQS_SIG_sphincs_sha2_256f_simple_length_secret_key];
    uint8_t plaintext_pk[OQS_SIG_sphincs_sha2_256f_simple_length_public_key];
    OQS_STATUS st = OQS_SIG_sphincs_sha2_256f_simple_keypair(plaintext_pk, plaintext_slh);
    if (st != OQS_SUCCESS) {
        sodium_memzero(&ed25519_sk, sizeof(ed25519_sk));
        sodium_memzero(plaintext_slh, sizeof(plaintext_slh));
        return FORETIAS_ERR_INTERNAL;
    }

    public_out->ed25519_pub = ed25519_pub;
    memcpy(public_out->slh_dsa_pub, plaintext_pk, FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES);

    // Encrypt Ed25519 secret (32 bytes)
    ForetiasResult rc = foretias_privkey_encrypt(
        ed25519_sk.bytes, FORETIAS_TBID_V1_ED25519_SK_BYTES,
        secret_out->encrypted_ed25519, secret_out->ed25519_nonce);
    sodium_memzero(&ed25519_sk, sizeof(ed25519_sk));
    if (rc != FORETIAS_OK) {
        sodium_memzero(plaintext_slh, sizeof(plaintext_slh));
        return rc;
    }

    // Encrypt SLH-DSA secret (128 bytes)
    rc = foretias_privkey_encrypt(
        plaintext_slh, FORETIAS_TBID_V1_SLH_DSA_SK_BYTES,
        secret_out->encrypted_slh_dsa, secret_out->slh_dsa_nonce);
    sodium_memzero(plaintext_slh, sizeof(plaintext_slh));
    if (rc != FORETIAS_OK) {
        return rc;
    }
    secret_out->slh_dsa_plaintext_len = FORETIAS_TBID_V1_SLH_DSA_SK_BYTES;

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

    // Decrypt Ed25519 secret
    ForetiasPrivKey32 ed25519_sk;
    ForetiasResult rc = foretias_privkey_decrypt(
        secret->encrypted_ed25519, FORETIAS_TBID_V1_ED25519_SK_BYTES + 16,
        secret->ed25519_nonce, ed25519_sk.bytes);
    if (rc < 0) return FORETIAS_ERR_INTERNAL;

    // Sign with Ed25519
    ForetiasSig64 ed25519_sig;
    ForetiasResult result = foretias_ed25519_sign(&ed25519_sk, msg, msg_len, &ed25519_sig);
    sodium_memzero(&ed25519_sk, sizeof(ed25519_sk));
    if (result != FORETIAS_OK) {
        return result;
    }

    // Decrypt SLH-DSA secret
    uint8_t plaintext_slh[OQS_SIG_sphincs_sha2_256f_simple_length_secret_key];
    rc = foretias_privkey_decrypt(
        secret->encrypted_slh_dsa, secret->slh_dsa_plaintext_len + 16,
        secret->slh_dsa_nonce, plaintext_slh);
    if (rc < 0) return FORETIAS_ERR_INTERNAL;

    size_t sig_len = OQS_SIG_sphincs_sha2_256f_simple_length_signature;
    uint8_t slh_sig[OQS_SIG_sphincs_sha2_256f_simple_length_signature];
    OQS_STATUS st = OQS_SIG_sphincs_sha2_256f_simple_sign(slh_sig, &sig_len, msg, msg_len, plaintext_slh);
    sodium_memzero(plaintext_slh, sizeof(plaintext_slh));
    if (st != OQS_SUCCESS) {
        return FORETIAS_ERR_INTERNAL;
    }

    memcpy(sig_out->bytes, ed25519_sig.bytes, FORETIAS_TBID_V1_ED25519_SIG_BYTES);
    memcpy(sig_out->bytes + FORETIAS_TBID_V1_ED25519_SIG_BYTES, slh_sig, sig_len);
    sig_out->len = FORETIAS_TBID_V1_ED25519_SIG_BYTES + sig_len;

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
    sodium_memzero(secret, sizeof(ForetiasTbidV1SecretKey));
}

#endif
