#pragma once

#if __STDC_VERSION__ < 201112L
#error "C11 or later required"
#endif

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <assert.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ────────────────────────────────────── */
#define FORETIAS_CORE_VERSION_MAJOR 0
#define FORETIAS_CORE_VERSION_MINOR 1

typedef struct {
    int         major;
    int         minor;
    const char* build_hash;    /* populated by build.rs / CMake */
} ForetiasCoreVersion;

ForetiasCoreVersion foretias_core_version(void);

/* ── Result codes ───────────────────────────────── */
typedef enum {
    FORETIAS_OK                 =  0,
    FORETIAS_ERR_BAD_SIG        = -1,
    FORETIAS_ERR_BAD_PROOF      = -2,
    FORETIAS_ERR_BAD_KEY        = -3,
    FORETIAS_ERR_STALE          = -4,
    FORETIAS_ERR_REPLAY         = -5,
    FORETIAS_ERR_BAD_INPUT      = -6,
    FORETIAS_ERR_OVERFLOW       = -7,
    FORETIAS_ERR_UNSUPPORTED    = -8,
    FORETIAS_ERR_INTERNAL       = -99,
} ForetiasResult;

/* ── Curve selector ─────────────────────────────── */
typedef enum {
    FORETIAS_CURVE_ED25519 = 1,
    FORETIAS_CURVE_P256    = 2,
} ForetiasCurve;

/* ── Key / signature types ──────────────────────── */
typedef struct { uint8_t bytes[32]; } ForetiasPubKey32;   /* Ed25519 pub, P-256 X */
typedef struct { uint8_t bytes[33]; } ForetiasPubKey33;   /* P-256 compressed    */
typedef struct { uint8_t bytes[32]; } ForetiasPrivKey32;  /* Ed25519 seed / P256 scalar */
typedef struct { uint8_t bytes[32]; } ForetiasPeerID;
typedef struct { uint8_t bytes[64]; } ForetiasSig64;      /* Ed25519 / P-256 ECDSA */
typedef struct { uint8_t bytes[32]; } ForetiasHash32;
typedef struct { uint8_t bytes[16]; } ForetiasHash16;     /* MD5 legacy          */
typedef struct { uint8_t bytes[20]; } ForetiasHash20;     /* SHA-1 legacy        */
typedef struct { uint8_t bytes[32]; } ForetiasNullifier;
typedef struct { uint8_t bytes[32]; } ForetiasFrostShare;

_Static_assert(sizeof(ForetiasPubKey32)  == 32, "ForetiasPubKey32");
_Static_assert(sizeof(ForetiasPrivKey32) == 32, "ForetiasPrivKey32");
_Static_assert(sizeof(ForetiasSig64)     == 64, "ForetiasSig64");
_Static_assert(sizeof(ForetiasHash32)    == 32, "ForetiasHash32");

/* ── Identity (Ed25519) ─────────────────────────── */
ForetiasResult foretias_ed25519_generate_keypair(
    ForetiasPubKey32*  pub_out,
    ForetiasPrivKey32* priv_out
);

ForetiasResult foretias_ed25519_derive_peer_id(
    const ForetiasPubKey32* pub,
    ForetiasPeerID*         id_out
);

ForetiasResult foretias_ed25519_sign(
    const ForetiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    ForetiasSig64*           sig_out
);

ForetiasResult foretias_ed25519_verify(
    const ForetiasPubKey32*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const ForetiasSig64*     sig
);

/* ── Identity (P-256) ───────────────────────────── */
ForetiasResult foretias_p256_generate_keypair(
    ForetiasPubKey33*  pub_out,
    ForetiasPrivKey32* priv_out
);

ForetiasResult foretias_p256_derive_peer_id(
    const ForetiasPubKey33* pub,
    ForetiasPeerID*         id_out
);

ForetiasResult foretias_p256_sign(
    const ForetiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    ForetiasSig64*           sig_out
);

ForetiasResult foretias_p256_verify(
    const ForetiasPubKey33*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const ForetiasSig64*     sig
);

/* ── Hashing — secure ───────────────────────────── */
ForetiasResult foretias_hash_sha256(
    const uint8_t* data,
    size_t         len,
    ForetiasHash32* out
);

ForetiasResult foretias_hash_sha256_concat(
    const uint8_t* a, size_t a_len,
    const uint8_t* b, size_t b_len,
    ForetiasHash32* out
);

ForetiasResult foretias_hash_blake3(
    const uint8_t* data,
    size_t         len,
    ForetiasHash32* out
);

/* ── Hashing — legacy / insecure (NONCRYPTO USE ONLY) ─ */
/* @human: names deliberately verbose to prevent accidental
   security use. Callers that see these names must have a
   non-security reason (file checksums, protocol interop). */

ForetiasResult foretias_hash_legacy_insecure_md5(
    const uint8_t* data,
    size_t         len,
    ForetiasHash16* out
);

ForetiasResult foretias_hash_legacy_insecure_sha1(
    const uint8_t* data,
    size_t         len,
    ForetiasHash20* out
);

/* ── Noise_XX handshake ─────────────────────────── */
#define FORETIAS_NOISE_MAX_MSG 65535

typedef struct {
    uint8_t  chaining_key[32];
    uint8_t  handshake_hash[32];
    uint8_t  local_static_priv[32];
    uint8_t  local_static_pub[32];
    uint8_t  local_ephemeral[32];
    uint8_t  remote_ephemeral[32];
    uint8_t  remote_static[32];
    uint8_t  send_key[32];
    uint8_t  recv_key[32];
    uint64_t send_nonce;
    uint64_t recv_nonce;
    int32_t  step;
    int32_t  is_initiator;
    int32_t  handshake_complete;
    int32_t  curve;                /* ForetiasCurve */
    uint8_t  _pad[4];
} ForetiasNoiseState;

ForetiasResult foretias_noise_init_ed25519(
    ForetiasNoiseState*      state,
    const ForetiasPrivKey32* my_static_priv,
    const ForetiasPubKey32*  their_static_pub,  /* NULL for responder */
    bool                    is_initiator
);

ForetiasResult foretias_noise_init_p256(
    ForetiasNoiseState*      state,
    const ForetiasPrivKey32* my_static_priv,
    const ForetiasPubKey33*  their_static_pub,  /* NULL for responder */
    bool                    is_initiator
);

ForetiasResult foretias_noise_step(
    ForetiasNoiseState* state,
    const uint8_t*     input,
    size_t             input_len,
    uint8_t*           output,
    size_t*            output_len
);

ForetiasResult foretias_noise_send(
    ForetiasNoiseState* state,
    const uint8_t*     plaintext,
    size_t             pt_len,
    uint8_t*           ciphertext,       /* caller: pt_len + 16 */
    size_t*            ct_len
);

ForetiasResult foretias_noise_recv(
    ForetiasNoiseState* state,
    const uint8_t*     ciphertext,
    size_t             ct_len,
    uint8_t*           plaintext,        /* caller: ct_len */
    size_t*            pt_len
);

void foretias_noise_destroy(ForetiasNoiseState* state);

/* ── Sparse Merkle ──────────────────────────────── */
#define FORETIAS_MERKLE_MAX_DEPTH 32

typedef struct {
    ForetiasHash32 siblings[FORETIAS_MERKLE_MAX_DEPTH];
    uint8_t       directions[FORETIAS_MERKLE_MAX_DEPTH];
    int32_t       depth;
} ForetiasMerkleProof;

ForetiasResult foretias_merkle_leaf(
    const uint8_t* data,
    size_t         len,
    ForetiasHash32* leaf_out
);

ForetiasResult foretias_merkle_verify(
    const ForetiasHash32*      root,
    const ForetiasHash32*      leaf,
    const ForetiasMerkleProof* proof
);

/* ── FROST (Ed25519 base) ───────────────────────── */
typedef struct {
    uint8_t nonce_d[32];     /* SECRET */
    uint8_t nonce_e[32];     /* SECRET */
    uint8_t commit_D[32];
    uint8_t commit_E[32];
} ForetiasFrostRound1;

ForetiasResult foretias_frost_round1(ForetiasFrostRound1* out);

ForetiasResult foretias_frost_sign_share(
    const ForetiasFrostRound1* my_state,
    const ForetiasFrostShare*  my_key_share,
    const uint8_t*            msg,
    size_t                    msg_len,
    const uint8_t*            all_commits,     /* n * 64 */
    size_t                    n_signers,
    int32_t                   my_index,
    ForetiasFrostShare*        sig_share_out
);

ForetiasResult foretias_frost_aggregate(
    const ForetiasFrostShare* shares,
    const int32_t*           indices,
    size_t                   k,
    const uint8_t*           all_commits,
    const uint8_t*           msg,
    size_t                   msg_len,
    ForetiasSig64*            sig_out
);

void foretias_frost_destroy_round1(ForetiasFrostRound1* state);

/* ── Nullifier ──────────────────────────────────── */
ForetiasResult foretias_nullifier_derive(
    const ForetiasPrivKey32* priv,
    const uint8_t*          context,
    size_t                  context_len,
    ForetiasNullifier*       out
);

/* ── RNG ────────────────────────────────────────── */
/* Core RNG interface. Software backend mixes OS randomness only.
   Custom-plugin backends mix OS + plugin-supplied entropy. See rng_mix.c. */
ForetiasResult foretias_rng_bytes(uint8_t* buf, size_t len);

/* ── Secure zero ────────────────────────────────── */
void foretias_memzero(void* ptr, size_t len);

/* ── Opaque Private Key Handle ──────────────────── */
/* C11 Secret Containment: private key bytes never cross
   the C↔Rust boundary as raw bytes. The handle is an
   opaque pointer; the struct definition is private to
   privkey.c.

   Keys are encrypted at rest with a per-process KEK
   (Key Encryption Key) using ChaCha20-Poly1305 AEAD.
   The KEK never leaves C11 — even a memory dump of the
   key handle is useless without this C11 instance.

   Call foretias_privkey_init() once at process startup
   before any key operations. Call foretias_privkey_cleanup()
   at shutdown to zeroize the KEK. */

typedef struct ForetiasPrivKey ForetiasPrivKey;

/* Initialize the instance KEK. Must be called once at startup
   before any key operations. Generates a random 32-byte KEK. */
void foretias_privkey_init(void);

/* Zeroize and destroy the instance KEK. Call at shutdown. */
void foretias_privkey_cleanup(void);

/* Generate a fresh Ed25519 keypair, returning an opaque handle.
   The private key bytes are encrypted with the instance KEK
   and never exposed in plaintext outside C memory. */
ForetiasPrivKey* foretias_privkey_ed25519_generate(void);

/* Create a handle from an existing 32-byte seed.
   The seed is copied into C memory and zeroed from the caller's buffer. */
ForetiasPrivKey* foretias_privkey_ed25519_from_seed(const uint8_t seed[32]);

/* Derive and return the public key for the handle.
   Writes 32 bytes into `out`; returns `out` for chaining. */
uint8_t* foretias_privkey_ed25519_public(const ForetiasPrivKey *key, uint8_t out[32]);

/* Sign a message with the handle. Private key bytes never leave C. */
int foretias_privkey_ed25519_sign(
    const ForetiasPrivKey *key,
    const uint8_t *msg,
    size_t msg_len,
    uint8_t sig[64]
);

/* Derive a nullifier using the handle. */
ForetiasResult foretias_nullifier_derive_handle(
    const ForetiasPrivKey *key,
    const uint8_t *context,
    size_t context_len,
    ForetiasNullifier *out
);

/* Derive a 32-byte seal key via HKDF-SHA256 from the handle's seed.
   The seed never leaves C memory. The info string identifies the
   derived key's purpose (e.g. "foretias-calendar-seal-v1"). */
ForetiasResult foretias_privkey_derive_seal_key(
    const ForetiasPrivKey *key,
    const uint8_t *info,
    size_t            info_len,
    uint8_t           seal_key[32]
);

/* Destroy the handle, securely zeroing all key material. */
void foretias_privkey_free(ForetiasPrivKey *key);

#ifdef __cplusplus
}
#endif
