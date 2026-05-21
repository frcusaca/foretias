#include "platform.h"
#include "foretias_core.h"
#include <sodium.h>
#include <string.h>

#define _NH_H 32
#define _NH_D 32
#define _NH_T 16

static void _nh_mix_hash(uint8_t h[_NH_H], const uint8_t *d, size_t n) {
    crypto_hash_sha256_state s;
    (void)crypto_hash_sha256_init(&s);
    (void)crypto_hash_sha256_update(&s, h, _NH_H);
    if (d && n > 0)
        (void)crypto_hash_sha256_update(&s, d, (unsigned long long)n);
    (void)crypto_hash_sha256_final(&s, h);
}

static void _nh_mix_key(uint8_t ck[_NH_H], const uint8_t dh[_NH_D],
                        uint8_t nck[_NH_H], uint8_t nk[_NH_H]) {
    uint8_t t[_NH_H];
    (void)crypto_auth_hmacsha256(t, dh, _NH_D, ck);
    { uint8_t i = 0x01;
      (void)crypto_auth_hmacsha256(nck, &i, 1, t); }
    { uint8_t i[_NH_H + 1];
      memcpy(i, nck, _NH_H); i[_NH_H] = 0x02;
      (void)crypto_auth_hmacsha256(nk, i, _NH_H + 1, t); }
}

static void _nh_split(uint8_t ck[_NH_H],
                      uint8_t sk[_NH_H], uint8_t rk[_NH_H]) {
    uint8_t t[_NH_H];
    (void)crypto_auth_hmacsha256(t, NULL, 0, ck);
    { uint8_t i = 0x01;
      (void)crypto_auth_hmacsha256(sk, &i, 1, t); }
    { uint8_t i[_NH_H + 1];
      memcpy(i, sk, _NH_H); i[_NH_H] = 0x02;
      (void)crypto_auth_hmacsha256(rk, i, _NH_H + 1, t); }
}

static int _nh_x25519(const uint8_t p[32], const uint8_t q[32], uint8_t o[32]) {
    return crypto_scalarmult(o, p, q);
}

static void _nh_nonce(uint64_t n, uint8_t o[8]) {
    /* Noise spec: 64-bit counter in little-endian byte order. */
    o[0] = (uint8_t)(n & 0xFF);
    o[1] = (uint8_t)((n >> 8) & 0xFF);
    o[2] = (uint8_t)((n >> 16) & 0xFF);
    o[3] = (uint8_t)((n >> 24) & 0xFF);
    o[4] = (uint8_t)((n >> 32) & 0xFF);
    o[5] = (uint8_t)((n >> 40) & 0xFF);
    o[6] = (uint8_t)((n >> 48) & 0xFF);
    o[7] = (uint8_t)((n >> 56) & 0xFF);
}

static int _nh_ae_enc(uint8_t k[32], uint64_t *n,
                      const uint8_t *ad, size_t al,
                      const uint8_t *pt, size_t pl,
                      uint8_t *ct, size_t *cl) {
    uint8_t nc[crypto_aead_chacha20poly1305_ietf_NPUBBYTES];
    memset(nc, 0, 4);
    _nh_nonce(*n, nc + 4);
    unsigned long long cll = *cl;
    if (*n == UINT64_MAX) return FORETIAS_ERR_OVERFLOW;
    int r = crypto_aead_chacha20poly1305_ietf_encrypt(
        ct, &cll, pt, (unsigned long long)pl,
        ad, (unsigned long long)al, NULL, nc, k);
    if (r != 0) return -1;
    (*n)++;
    *cl = (size_t)cll;
    return 0;
}

static int _nh_ae_dec(uint8_t k[32], uint64_t *n,
                      const uint8_t *ad, size_t al,
                      const uint8_t *ct, size_t cl,
                      uint8_t *pt, size_t *pl) {
    uint8_t nc[crypto_aead_chacha20poly1305_ietf_NPUBBYTES];
    memset(nc, 0, 4);
    _nh_nonce(*n, nc + 4);
    if (*n == UINT64_MAX) return FORETIAS_ERR_OVERFLOW;
    unsigned long long pll = *pl;
    int r = crypto_aead_chacha20poly1305_ietf_decrypt(
        pt, &pll, NULL, ct, (unsigned long long)cl,
        ad, (unsigned long long)al, nc, k);
    if (r != 0) return -1;
    (*n)++;
    *pl = (size_t)pll;
    return 0;
}

static int _nh_eh(uint8_t h[_NH_H], uint8_t k[32],
                  uint64_t *n, const uint8_t *pt, size_t pl,
                  uint8_t *o, size_t *ol) {
    int r = _nh_ae_enc(k, n, h, _NH_H, pt, pl, o, ol);
    if (r != 0) return -1;
    _nh_mix_hash(h, o, *ol);
    return 0;
}

static int _nh_dec_hash(uint8_t h[_NH_H], uint8_t k[32],
                        uint64_t *n, const uint8_t *ct, size_t cl,
                        uint8_t *pt, size_t *pl) {
    uint8_t th[_NH_H];
    memcpy(th, h, _NH_H);
    _nh_mix_hash(th, ct, cl);
    int r = _nh_ae_dec(k, n, h, _NH_H, ct, cl, pt, pl);
    if (r != 0) return -1;
    memcpy(h, th, _NH_H);
    return 0;
}

/* ── HR-1: KEK encrypt/decrypt helpers for Noise session secrets ── */

static int _nh_decrypt_key(const uint8_t encrypted[48],
                           const uint8_t nonce[24],
                           uint8_t out[32]) {
    int rc = foretias_privkey_decrypt(encrypted, 48, nonce, out);
    if (rc < 0) return -1;
    return 0;
}

static int _nh_encrypt_key(const uint8_t plaintext[32],
                           uint8_t encrypted_out[48],
                           uint8_t nonce_out[24]) {
    int rc = foretias_privkey_encrypt(plaintext, 32, encrypted_out, nonce_out);
    if (rc != FORETIAS_OK) return -1;
    return 0;
}

/* ── Inline helpers for encrypting state fields ── */

static int _nh_encrypt_chaining_key(ForetiasNoiseState *st, const uint8_t ck[32]) {
    return _nh_encrypt_key(ck, st->encrypted_chaining_key, st->chaining_key_nonce);
}

static int _nh_decrypt_chaining_key(const ForetiasNoiseState *st, uint8_t ck[32]) {
    return _nh_decrypt_key(st->encrypted_chaining_key, st->chaining_key_nonce, ck);
}

static int _nh_encrypt_local_static(ForetiasNoiseState *st, const uint8_t lsp[32]) {
    return _nh_encrypt_key(lsp, st->encrypted_local_static, st->local_static_nonce);
}

static int _nh_decrypt_local_static(const ForetiasNoiseState *st, uint8_t lsp[32]) {
    return _nh_decrypt_key(st->encrypted_local_static, st->local_static_nonce, lsp);
}

static int _nh_encrypt_send_key(ForetiasNoiseState *st, const uint8_t sk[32]) {
    return _nh_encrypt_key(sk, st->encrypted_send_key, st->send_key_nonce);
}

static int _nh_decrypt_send_key(const ForetiasNoiseState *st, uint8_t sk[32]) {
    return _nh_decrypt_key(st->encrypted_send_key, st->send_key_nonce, sk);
}

static int _nh_encrypt_recv_key(ForetiasNoiseState *st, const uint8_t rk[32]) {
    return _nh_encrypt_key(rk, st->encrypted_recv_key, st->recv_key_nonce);
}

static int _nh_decrypt_recv_key(const ForetiasNoiseState *st, uint8_t rk[32]) {
    return _nh_decrypt_key(st->encrypted_recv_key, st->recv_key_nonce, rk);
}


ForetiasResult foretias_noise_init_ed25519(
    ForetiasNoiseState*      state,
    const ForetiasPrivKey32* my_static_priv,
    const ForetiasPubKey32*  their_static_pub,
    bool                    is_initiator
) {
    if (!state || !my_static_priv) return FORETIAS_ERR_BAD_INPUT;
    if (sodium_init() < 0) return FORETIAS_ERR_INTERNAL;

    memset(state, 0, sizeof(*state));

    const char *salt = "Noise_XX_static_key_ed25519";
    uint8_t prk[32];
    (void)crypto_auth_hmacsha256(prk, my_static_priv->bytes, 32,
                                  (const uint8_t *)salt);

    uint8_t seed[32];
    { uint8_t i = 0x01;
      (void)crypto_auth_hmacsha256(seed, &i, 1, prk); }

    uint8_t epub[32];
    if (crypto_scalarmult_base(epub, seed) != 0) {
        sodium_memzero(seed, 32);
        sodium_memzero(prk, 32);
        return FORETIAS_ERR_BAD_KEY;
    }

    /* HR-1: Encrypt local_static_priv before storing */
    if (_nh_encrypt_local_static(state, seed) != 0) {
        sodium_memzero(seed, 32);
        sodium_memzero(prk, 32);
        return FORETIAS_ERR_INTERNAL;
    }
    sodium_memzero(seed, 32);

    memcpy(state->local_static_pub, epub, 32);
    sodium_memzero(prk, 32);
    sodium_memzero(epub, 32);

    if (their_static_pub)
        memcpy(state->remote_static, their_static_pub->bytes, 32);

    const char *proto = "Noise_XX_25519_ChaChaPoly_SHA256";
    size_t proto_len = strlen(proto);
    if (proto_len > _NH_H) {
        (void)crypto_hash_sha256(state->handshake_hash,
                                  (const uint8_t *)proto,
                                  (unsigned long long)proto_len);
    } else {
        memcpy(state->handshake_hash, proto, proto_len);
        memset(state->handshake_hash + proto_len, 0, _NH_H - proto_len);
    }
    /* HR-1: Encrypt chaining_key before storing */
    { uint8_t ck[32];
      memcpy(ck, state->handshake_hash, _NH_H);
      if (_nh_encrypt_chaining_key(state, ck) != 0) {
          sodium_memzero(ck, 32);
          return FORETIAS_ERR_INTERNAL;
      }
      sodium_memzero(ck, 32);
    }

    state->step               = 0;
    state->is_initiator       = is_initiator ? 1 : 0;
    state->handshake_complete = 0;
    state->curve              = FORETIAS_CURVE_ED25519;

    return FORETIAS_OK;
}

ForetiasResult foretias_noise_init_with_handle(
    ForetiasNoiseState*      state,
    const ForetiasPrivKey*   priv_handle,
    const ForetiasPubKey32*  their_static_pub,
    bool                    is_initiator
) {
    if (!state || !priv_handle) return FORETIAS_ERR_BAD_INPUT;
    if (sodium_init() < 0) return FORETIAS_ERR_INTERNAL;

    memset(state, 0, sizeof(*state));

    uint8_t ed_seed[32];
    ForetiasResult rc = foretias_privkey_ed25519_get_seed(priv_handle, ed_seed);
    if (rc != FORETIAS_OK) return rc;

    const char *salt = "Noise_XX_static_key_ed25519";
    uint8_t prk[32];
    (void)crypto_auth_hmacsha256(prk, ed_seed, 32, (const uint8_t *)salt);

    uint8_t x25519_seed[32];
    { uint8_t i = 0x01;
      (void)crypto_auth_hmacsha256(x25519_seed, &i, 1, prk); }

    uint8_t epub[32];
    if (crypto_scalarmult_base(epub, x25519_seed) != 0) {
        sodium_memzero(x25519_seed, 32);
        sodium_memzero(prk, 32);
        sodium_memzero(ed_seed, 32);
        return FORETIAS_ERR_BAD_KEY;
    }

    /* HR-1: Encrypt local_static_priv before storing */
    if (_nh_encrypt_local_static(state, x25519_seed) != 0) {
        sodium_memzero(x25519_seed, 32);
        sodium_memzero(prk, 32);
        sodium_memzero(ed_seed, 32);
        return FORETIAS_ERR_INTERNAL;
    }
    sodium_memzero(x25519_seed, 32);

    memcpy(state->local_static_pub, epub, 32);
    sodium_memzero(prk, 32);
    sodium_memzero(ed_seed, 32);
    sodium_memzero(epub, 32);

    if (their_static_pub)
        memcpy(state->remote_static, their_static_pub->bytes, 32);

    const char *proto = "Noise_XX_25519_ChaChaPoly_SHA256";
    size_t proto_len = strlen(proto);
    if (proto_len > _NH_H) {
        (void)crypto_hash_sha256(state->handshake_hash,
                                  (const uint8_t *)proto,
                                  (unsigned long long)proto_len);
    } else {
        memcpy(state->handshake_hash, proto, proto_len);
        memset(state->handshake_hash + proto_len, 0, _NH_H - proto_len);
    }
    /* HR-1: Encrypt chaining_key before storing */
    { uint8_t ck[32];
      memcpy(ck, state->handshake_hash, _NH_H);
      if (_nh_encrypt_chaining_key(state, ck) != 0) {
          sodium_memzero(ck, 32);
          return FORETIAS_ERR_INTERNAL;
      }
      sodium_memzero(ck, 32);
    }

    state->step               = 0;
    state->is_initiator       = is_initiator ? 1 : 0;
    state->handshake_complete = 0;
    state->curve              = FORETIAS_CURVE_ED25519;

    return FORETIAS_OK;
}

ForetiasResult foretias_noise_init_p256(
    ForetiasNoiseState*      state,
    const ForetiasPrivKey32* my_static_priv,
    const ForetiasPubKey33*  their_static_pub,
    bool                    is_initiator
) {
    (void)state;
    (void)my_static_priv;
    (void)their_static_pub;
    (void)is_initiator;
    return FORETIAS_ERR_UNSUPPORTED;
}

ForetiasResult foretias_noise_step(
    ForetiasNoiseState* state,
    const uint8_t*     input,
    size_t             input_len,
    uint8_t*           output,
    size_t*            output_len
) {
    if (!state || !output_len) return FORETIAS_ERR_BAD_INPUT;
    if (state->handshake_complete) return FORETIAS_ERR_BAD_INPUT;

    uint8_t ck[_NH_H], h[_NH_H], k[_NH_H];
    if (_nh_decrypt_chaining_key(state, ck) != 0)
        return FORETIAS_ERR_BAD_KEY;
    memcpy(h, state->handshake_hash, _NH_H);
    memset(k, 0, _NH_H);

    if (state->is_initiator) {
        switch (state->step) {
        case 0: {
            if (!output || *output_len < _NH_D) return FORETIAS_ERR_BAD_INPUT;

            uint8_t epriv[32], epub[32];
            randombytes_buf(epriv, 32);
            if (crypto_scalarmult_base(epub, epriv) != 0) {
                sodium_memzero(epriv, 32);
                return FORETIAS_ERR_INTERNAL;
            }

            memcpy(state->local_ephemeral, epub, _NH_D);
            /* HR-1: Encrypt send_key instead of storing plaintext */
            if (_nh_encrypt_send_key(state, epriv) != 0) {
                sodium_memzero(epriv, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(epriv, 32);

            memcpy(output, epub, _NH_D);
            *output_len = _NH_D;
            sodium_memzero(epub, 32);

            _nh_mix_hash(h, state->local_ephemeral, _NH_D);
            memcpy(state->handshake_hash, h, _NH_H);

            state->step = 1;
            return FORETIAS_OK;
        }

        case 1: {
            if (!input || input_len < _NH_D + _NH_D + _NH_T)
                return FORETIAS_ERR_BAD_INPUT;

            memcpy(state->remote_ephemeral, input, _NH_D);
            _nh_mix_hash(h, state->remote_ephemeral, _NH_D);

            /* HR-1: Decrypt send_key for DH */
            uint8_t sk[32];
            if (_nh_decrypt_send_key(state, sk) != 0)
                return FORETIAS_ERR_BAD_KEY;

            uint8_t dh[_NH_D];
            if (_nh_x25519(sk, state->remote_ephemeral, dh) != 0) {
                sodium_memzero(sk, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(sk, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            {
                size_t off  = _NH_D;
                size_t ct_l = _NH_D + _NH_T;
                uint8_t s[_NH_D];
                size_t s_l  = _NH_D;
                if (_nh_dec_hash(h, k, &state->recv_nonce,
                                 input + off, ct_l, s, &s_l) != 0)
                    return FORETIAS_ERR_BAD_SIG;
                if (s_l != _NH_D) return FORETIAS_ERR_BAD_INPUT;
                memcpy(state->remote_static, s, _NH_D);
                sodium_memzero(s, _NH_D);
            }

            /* HR-1: Decrypt send_key for second DH */
            if (_nh_decrypt_send_key(state, sk) != 0)
                return FORETIAS_ERR_BAD_KEY;

            if (_nh_x25519(sk, state->remote_static, dh) != 0) {
                sodium_memzero(sk, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(sk, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            {
                size_t off = _NH_D + _NH_D + _NH_T;
                if (input_len > off) {
                    size_t ct_l = input_len - off;
                    if (ct_l < _NH_T) return FORETIAS_ERR_BAD_INPUT;
                    uint8_t p[FORETIAS_NOISE_MAX_MSG];
                    size_t p_l = sizeof(p);
                    if (_nh_dec_hash(h, k, &state->recv_nonce,
                                     input + off, ct_l, p, &p_l) != 0)
                        return FORETIAS_ERR_BAD_SIG;
                    sodium_memzero(p, sizeof(p));
                }
            }

            /* HR-1: Encrypt send_key (k) — previous value is overwritten */
            if (_nh_encrypt_send_key(state, k) != 0)
                return FORETIAS_ERR_INTERNAL;
            /* HR-1: Encrypt chaining_key */
            if (_nh_encrypt_chaining_key(state, ck) != 0) {
                sodium_memzero(ck, 32); sodium_memzero(k, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(ck, 32); sodium_memzero(k, 32);
            memcpy(state->handshake_hash, h, _NH_H);

            state->step = 2;
            return FORETIAS_OK;
        }

        case 2: {
            if (!output || *output_len < _NH_D + _NH_T)
                return FORETIAS_ERR_BAD_INPUT;

            /* HR-1: Decrypt send_key for encrypt-and-hash */
            if (_nh_decrypt_send_key(state, k) != 0)
                return FORETIAS_ERR_BAD_KEY;

            size_t ct_l = *output_len;
            if (_nh_eh(h, k, &state->send_nonce,
                       state->local_static_pub, _NH_D,
                       output, &ct_l) != 0)
                return FORETIAS_ERR_INTERNAL;

            /* HR-1: Decrypt local_static_priv for DH */
            uint8_t lsp[32];
            if (_nh_decrypt_local_static(state, lsp) != 0)
                return FORETIAS_ERR_BAD_KEY;

            uint8_t dh[_NH_D];
            if (_nh_x25519(lsp, state->remote_ephemeral, dh) != 0) {
                sodium_memzero(lsp, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(lsp, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            uint8_t sk[_NH_H], rk[_NH_H];
            _nh_split(ck, sk, rk);

            /* HR-1: Encrypt send_key and recv_key */
            if (_nh_encrypt_send_key(state, sk) != 0) {
                sodium_memzero(sk, 32); sodium_memzero(rk, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            if (_nh_encrypt_recv_key(state, rk) != 0) {
                sodium_memzero(sk, 32); sodium_memzero(rk, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(sk, _NH_H);
            sodium_memzero(rk, _NH_H);

            /* HR-1: Encrypt chaining_key */
            if (_nh_encrypt_chaining_key(state, ck) != 0) {
                sodium_memzero(ck, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(ck, 32);

            memcpy(state->handshake_hash, h, _NH_H);

            *output_len = ct_l;
            state->step               = 3;
            state->handshake_complete = 1;
            return FORETIAS_OK;
        }

        default:
            return FORETIAS_ERR_BAD_INPUT;
        }
    } else {
        switch (state->step) {
        case 0: {
            if (!input || input_len < _NH_D) return FORETIAS_ERR_BAD_INPUT;

            memcpy(state->remote_ephemeral, input, _NH_D);
            _nh_mix_hash(h, state->remote_ephemeral, _NH_D);
            memcpy(state->handshake_hash, h, _NH_H);

            state->step = 1;
            return FORETIAS_OK;
        }

        case 1: {
            if (!output || *output_len < _NH_D + _NH_D + _NH_T)
                return FORETIAS_ERR_BAD_INPUT;

            uint8_t epriv[32], epub[32];
            randombytes_buf(epriv, 32);
            if (crypto_scalarmult_base(epub, epriv) != 0) {
                sodium_memzero(epriv, 32);
                return FORETIAS_ERR_INTERNAL;
            }

            memcpy(state->local_ephemeral, epub, _NH_D);
            /* HR-1: Encrypt recv_key instead of storing plaintext */
            if (_nh_encrypt_recv_key(state, epriv) != 0) {
                sodium_memzero(epriv, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(epriv, 32);

            memcpy(output, epub, _NH_D);
            sodium_memzero(epub, 32);
            _nh_mix_hash(h, state->local_ephemeral, _NH_D);

            /* HR-1: Decrypt recv_key for DH */
            uint8_t rk[32];
            if (_nh_decrypt_recv_key(state, rk) != 0)
                return FORETIAS_ERR_BAD_KEY;

            uint8_t dh[_NH_D];
            if (_nh_x25519(rk, state->remote_ephemeral, dh) != 0) {
                sodium_memzero(rk, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(rk, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            {
                size_t ct_l = *output_len - _NH_D;
                if (_nh_eh(h, k, &state->send_nonce,
                           state->local_static_pub, _NH_D,
                           output + _NH_D, &ct_l) != 0) {
                    sodium_memzero(k, 32);
                    return FORETIAS_ERR_INTERNAL;
                }
                *output_len = _NH_D + ct_l;
            }

            /* HR-1: Decrypt local_static_priv for DH */
            uint8_t lsp[32];
            if (_nh_decrypt_local_static(state, lsp) != 0) {
                sodium_memzero(k, 32);
                return FORETIAS_ERR_BAD_KEY;
            }

            if (_nh_x25519(lsp, state->remote_ephemeral, dh) != 0) {
                sodium_memzero(lsp, 32);
                sodium_memzero(k, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(lsp, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            /* HR-1: Encrypt send_key (k) */
            if (_nh_encrypt_send_key(state, k) != 0) {
                sodium_memzero(k, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(k, 32);

            /* HR-1: Encrypt chaining_key */
            if (_nh_encrypt_chaining_key(state, ck) != 0) {
                sodium_memzero(ck, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(ck, 32);

            memcpy(state->handshake_hash, h, _NH_H);

            state->step = 2;
            return FORETIAS_OK;
        }

        case 2: {
            if (!input || input_len < _NH_D + _NH_T)
                return FORETIAS_ERR_BAD_INPUT;

            /* HR-1: Decrypt send_key for dec_hash */
            if (_nh_decrypt_send_key(state, k) != 0)
                return FORETIAS_ERR_BAD_KEY;

            {
                size_t ct_l = _NH_D + _NH_T;
                uint8_t s[_NH_D];
                size_t s_l  = _NH_D;
                if (_nh_dec_hash(h, k, &state->recv_nonce,
                                 input, ct_l, s, &s_l) != 0)
                    return FORETIAS_ERR_BAD_SIG;
                if (s_l != _NH_D) return FORETIAS_ERR_BAD_INPUT;
                memcpy(state->remote_static, s, _NH_D);
                sodium_memzero(s, _NH_D);
            }

            /* HR-1: Decrypt recv_key for DH */
            uint8_t rk[32];
            if (_nh_decrypt_recv_key(state, rk) != 0)
                return FORETIAS_ERR_BAD_KEY;

            uint8_t dh[_NH_D];
            if (_nh_x25519(rk, state->remote_static, dh) != 0) {
                sodium_memzero(rk, 32);
                return FORETIAS_ERR_BAD_KEY;
            }
            sodium_memzero(rk, 32);
            _nh_mix_key(ck, dh, ck, k);
            sodium_memzero(dh, _NH_D);

            {
                size_t off = _NH_D + _NH_T;
                if (input_len > off) {
                    size_t ct_l = input_len - off;
                    if (ct_l < _NH_T) return FORETIAS_ERR_BAD_INPUT;
                    uint8_t p[FORETIAS_NOISE_MAX_MSG];
                    size_t p_l = sizeof(p);
                    if (_nh_dec_hash(h, k, &state->recv_nonce,
                                     input + off, ct_l, p, &p_l) != 0)
                        return FORETIAS_ERR_BAD_SIG;
                    sodium_memzero(p, sizeof(p));
                }
            }

            uint8_t sk[_NH_H], rk_out[_NH_H];
            _nh_split(ck, sk, rk_out);

            /* HR-1: Encrypt send_key (rk_out) and recv_key (sk) */
            if (_nh_encrypt_send_key(state, rk_out) != 0) {
                sodium_memzero(sk, 32); sodium_memzero(rk_out, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            if (_nh_encrypt_recv_key(state, sk) != 0) {
                sodium_memzero(sk, 32); sodium_memzero(rk_out, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(sk, _NH_H);
            sodium_memzero(rk_out, _NH_H);

            /* HR-1: Encrypt chaining_key */
            if (_nh_encrypt_chaining_key(state, ck) != 0) {
                sodium_memzero(ck, 32);
                return FORETIAS_ERR_INTERNAL;
            }
            sodium_memzero(ck, 32);

            memcpy(state->handshake_hash, h, _NH_H);

            state->step               = 3;
            state->handshake_complete = 1;
            return FORETIAS_OK;
        }

        default:
            return FORETIAS_ERR_BAD_INPUT;
        }
    }
}

ForetiasResult foretias_noise_send(
    ForetiasNoiseState* state,
    const uint8_t*     plaintext,
    size_t             pt_len,
    uint8_t*           ciphertext,
    size_t*            ct_len
) {
    if (!state || !state->handshake_complete) return FORETIAS_ERR_BAD_INPUT;
    if (!plaintext || !ciphertext || !ct_len) return FORETIAS_ERR_BAD_INPUT;

    size_t out = pt_len + _NH_T;
    if (*ct_len < out) return FORETIAS_ERR_BAD_INPUT;

    /* HR-1: Decrypt send_key for AEAD encryption */
    uint8_t sk[32];
    if (_nh_decrypt_send_key(state, sk) != 0)
        return FORETIAS_ERR_BAD_KEY;

    int r = _nh_ae_enc(sk, &state->send_nonce,
                        NULL, 0, plaintext, pt_len,
                        ciphertext, ct_len);
    sodium_memzero(sk, 32);
    if (r != 0) return FORETIAS_ERR_INTERNAL;

    return FORETIAS_OK;
}

ForetiasResult foretias_noise_recv(
    ForetiasNoiseState* state,
    const uint8_t*     ciphertext,
    size_t             ct_len,
    uint8_t*           plaintext,
    size_t*            pt_len
) {
    if (!state || !state->handshake_complete) return FORETIAS_ERR_BAD_INPUT;
    if (!ciphertext || !plaintext || !pt_len) return FORETIAS_ERR_BAD_INPUT;
    if (ct_len < _NH_T) return FORETIAS_ERR_BAD_INPUT;

    /* HR-1: Decrypt recv_key for AEAD decryption */
    uint8_t rk[32];
    if (_nh_decrypt_recv_key(state, rk) != 0)
        return FORETIAS_ERR_BAD_KEY;

    size_t max_pt = ct_len;
    int r = _nh_ae_dec(rk, &state->recv_nonce,
                        NULL, 0, ciphertext, ct_len,
                        plaintext, &max_pt);
    sodium_memzero(rk, 32);
    if (r != 0) return FORETIAS_ERR_BAD_SIG;

    *pt_len = max_pt;
    return FORETIAS_OK;
}

void foretias_noise_destroy(ForetiasNoiseState *state) {
    if (state) sodium_memzero(state, sizeof(ForetiasNoiseState));
}
