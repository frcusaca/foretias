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
#define FORTIAS_CORE_VERSION_MAJOR 0
#define FORTIAS_CORE_VERSION_MINOR 1

typedef struct {
    int         major;
    int         minor;
    const char* build_hash;    /* populated by build.rs / CMake */
} FortiasCoreVersion;

FortiasCoreVersion fortias_core_version(void);

/* ── Result codes ───────────────────────────────── */
typedef enum {
    FORTIAS_OK                 =  0,
    FORTIAS_ERR_BAD_SIG        = -1,
    FORTIAS_ERR_BAD_PROOF      = -2,
    FORTIAS_ERR_BAD_KEY        = -3,
    FORTIAS_ERR_STALE          = -4,
    FORTIAS_ERR_REPLAY         = -5,
    FORTIAS_ERR_BAD_INPUT      = -6,
    FORTIAS_ERR_OVERFLOW       = -7,
    FORTIAS_ERR_UNSUPPORTED    = -8,
    FORTIAS_ERR_INTERNAL       = -99,
} FortiasResult;

/* ── Curve selector ─────────────────────────────── */
typedef enum {
    FORTIAS_CURVE_ED25519 = 1,
    FORTIAS_CURVE_P256    = 2,
} FortiasCurve;

/* ── Key / signature types ──────────────────────── */
typedef struct { uint8_t bytes[32]; } FortiasPubKey32;   /* Ed25519 pub, P-256 X */
typedef struct { uint8_t bytes[33]; } FortiasPubKey33;   /* P-256 compressed    */
typedef struct { uint8_t bytes[32]; } FortiasPrivKey32;  /* Ed25519 seed / P256 scalar */
typedef struct { uint8_t bytes[32]; } FortiasPeerID;
typedef struct { uint8_t bytes[64]; } FortiasSig64;      /* Ed25519 / P-256 ECDSA */
typedef struct { uint8_t bytes[32]; } FortiasHash32;
typedef struct { uint8_t bytes[16]; } FortiasHash16;     /* MD5 legacy          */
typedef struct { uint8_t bytes[20]; } FortiasHash20;     /* SHA-1 legacy        */
typedef struct { uint8_t bytes[32]; } FortiasNullifier;
typedef struct { uint8_t bytes[32]; } FortiasFrostShare;

_Static_assert(sizeof(FortiasPubKey32)  == 32, "FortiasPubKey32");
_Static_assert(sizeof(FortiasPrivKey32) == 32, "FortiasPrivKey32");
_Static_assert(sizeof(FortiasSig64)     == 64, "FortiasSig64");
_Static_assert(sizeof(FortiasHash32)    == 32, "FortiasHash32");

/* ── Identity (Ed25519) ─────────────────────────── */
FortiasResult fortias_ed25519_generate_keypair(
    FortiasPubKey32*  pub_out,
    FortiasPrivKey32* priv_out
);

FortiasResult fortias_ed25519_derive_peer_id(
    const FortiasPubKey32* pub,
    FortiasPeerID*         id_out
);

FortiasResult fortias_ed25519_sign(
    const FortiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    FortiasSig64*           sig_out
);

FortiasResult fortias_ed25519_verify(
    const FortiasPubKey32*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const FortiasSig64*     sig
);

/* ── Identity (P-256) ───────────────────────────── */
FortiasResult fortias_p256_generate_keypair(
    FortiasPubKey33*  pub_out,
    FortiasPrivKey32* priv_out
);

FortiasResult fortias_p256_derive_peer_id(
    const FortiasPubKey33* pub,
    FortiasPeerID*         id_out
);

FortiasResult fortias_p256_sign(
    const FortiasPrivKey32* priv,
    const uint8_t*          msg,
    size_t                  msg_len,
    FortiasSig64*           sig_out
);

FortiasResult fortias_p256_verify(
    const FortiasPubKey33*  pub,
    const uint8_t*          msg,
    size_t                  msg_len,
    const FortiasSig64*     sig
);

/* ── Hashing — secure ───────────────────────────── */
FortiasResult fortias_hash_sha256(
    const uint8_t* data,
    size_t         len,
    FortiasHash32* out
);

FortiasResult fortias_hash_sha256_concat(
    const uint8_t* a, size_t a_len,
    const uint8_t* b, size_t b_len,
    FortiasHash32* out
);

FortiasResult fortias_hash_blake3(
    const uint8_t* data,
    size_t         len,
    FortiasHash32* out
);

/* ── Hashing — legacy / insecure (NONCRYPTO USE ONLY) ─ */
/* @human: names deliberately verbose to prevent accidental
   security use. Callers that see these names must have a
   non-security reason (file checksums, protocol interop). */

FortiasResult fortias_hash_legacy_insecure_md5(
    const uint8_t* data,
    size_t         len,
    FortiasHash16* out
);

FortiasResult fortias_hash_legacy_insecure_sha1(
    const uint8_t* data,
    size_t         len,
    FortiasHash20* out
);

/* ── Noise_XX handshake ─────────────────────────── */
#define FORTIAS_NOISE_MAX_MSG 65535

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
    int32_t  curve;                /* FortiasCurve */
    uint8_t  _pad[4];
} FortiasNoiseState;

FortiasResult fortias_noise_init_ed25519(
    FortiasNoiseState*      state,
    const FortiasPrivKey32* my_static_priv,
    const FortiasPubKey32*  their_static_pub,  /* NULL for responder */
    bool                    is_initiator
);

FortiasResult fortias_noise_init_p256(
    FortiasNoiseState*      state,
    const FortiasPrivKey32* my_static_priv,
    const FortiasPubKey33*  their_static_pub,  /* NULL for responder */
    bool                    is_initiator
);

FortiasResult fortias_noise_step(
    FortiasNoiseState* state,
    const uint8_t*     input,
    size_t             input_len,
    uint8_t*           output,
    size_t*            output_len
);

FortiasResult fortias_noise_send(
    FortiasNoiseState* state,
    const uint8_t*     plaintext,
    size_t             pt_len,
    uint8_t*           ciphertext,       /* caller: pt_len + 16 */
    size_t*            ct_len
);

FortiasResult fortias_noise_recv(
    FortiasNoiseState* state,
    const uint8_t*     ciphertext,
    size_t             ct_len,
    uint8_t*           plaintext,        /* caller: ct_len */
    size_t*            pt_len
);

void fortias_noise_destroy(FortiasNoiseState* state);

/* ── Sparse Merkle ──────────────────────────────── */
#define FORTIAS_MERKLE_MAX_DEPTH 32

typedef struct {
    FortiasHash32 siblings[FORTIAS_MERKLE_MAX_DEPTH];
    uint8_t       directions[FORTIAS_MERKLE_MAX_DEPTH];
    int32_t       depth;
} FortiasMerkleProof;

FortiasResult fortias_merkle_leaf(
    const uint8_t* data,
    size_t         len,
    FortiasHash32* leaf_out
);

FortiasResult fortias_merkle_verify(
    const FortiasHash32*      root,
    const FortiasHash32*      leaf,
    const FortiasMerkleProof* proof
);

/* ── FROST (Ed25519 base) ───────────────────────── */
typedef struct {
    uint8_t nonce_d[32];     /* SECRET */
    uint8_t nonce_e[32];     /* SECRET */
    uint8_t commit_D[32];
    uint8_t commit_E[32];
} FortiasFrostRound1;

FortiasResult fortias_frost_round1(FortiasFrostRound1* out);

FortiasResult fortias_frost_sign_share(
    const FortiasFrostRound1* my_state,
    const FortiasFrostShare*  my_key_share,
    const uint8_t*            msg,
    size_t                    msg_len,
    const uint8_t*            all_commits,     /* n * 64 */
    size_t                    n_signers,
    int32_t                   my_index,
    FortiasFrostShare*        sig_share_out
);

FortiasResult fortias_frost_aggregate(
    const FortiasFrostShare* shares,
    const int32_t*           indices,
    size_t                   k,
    const uint8_t*           all_commits,
    const uint8_t*           msg,
    size_t                   msg_len,
    FortiasSig64*            sig_out
);

void fortias_frost_destroy_round1(FortiasFrostRound1* state);

/* ── Nullifier ──────────────────────────────────── */
FortiasResult fortias_nullifier_derive(
    const FortiasPrivKey32* priv,
    const uint8_t*          context,
    size_t                  context_len,
    FortiasNullifier*       out
);

/* ── RNG ────────────────────────────────────────── */
/* Core RNG interface. Software backend mixes OS randomness only.
   Custom-plugin backends mix OS + plugin-supplied entropy. See rng_mix.c. */
FortiasResult fortias_rng_bytes(uint8_t* buf, size_t len);

/* ── Secure zero ────────────────────────────────── */
void fortias_memzero(void* ptr, size_t len);

/* ── Opaque Private Key Handle ──────────────────── */
/* C11 Secret Containment: private key bytes never cross
   the C↔Rust boundary as raw bytes. The handle is an
   opaque pointer; the struct definition is private to
   privkey.c.

   Keys are encrypted at rest with a per-process KEK
   (Key Encryption Key) using ChaCha20-Poly1305 AEAD.
   The KEK never leaves C11 — even a memory dump of the
   key handle is useless without this C11 instance.

   Call fortias_privkey_init() once at process startup
   before any key operations. Call fortias_privkey_cleanup()
   at shutdown to zeroize the KEK. */

typedef struct FortiasPrivKey FortiasPrivKey;

/* Initialize the instance KEK. Must be called once at startup
   before any key operations. Generates a random 32-byte KEK. */
void fortias_privkey_init(void);

/* Zeroize and destroy the instance KEK. Call at shutdown. */
void fortias_privkey_cleanup(void);

/* Generate a fresh Ed25519 keypair, returning an opaque handle.
   The private key bytes are encrypted with the instance KEK
   and never exposed in plaintext outside C memory. */
FortiasPrivKey* fortias_privkey_ed25519_generate(void);

/* Create a handle from an existing 32-byte seed.
   The seed is copied into C memory and zeroed from the caller's buffer. */
FortiasPrivKey* fortias_privkey_ed25519_from_seed(const uint8_t seed[32]);

/* Derive and return the public key for the handle.
   Writes 32 bytes into `out`; returns `out` for chaining. */
uint8_t* fortias_privkey_ed25519_public(const FortiasPrivKey *key, uint8_t out[32]);

/* Sign a message with the handle. Private key bytes never leave C. */
int fortias_privkey_ed25519_sign(
    const FortiasPrivKey *key,
    const uint8_t *msg,
    size_t msg_len,
    uint8_t sig[64]
);

/* Derive a nullifier using the handle. */
FortiasResult fortias_nullifier_derive_handle(
    const FortiasPrivKey *key,
    const uint8_t *context,
    size_t context_len,
    FortiasNullifier *out
);

/* Destroy the handle, securely zeroing all key material. */
void fortias_privkey_free(FortiasPrivKey *key);

#ifdef __cplusplus
}
#endif
