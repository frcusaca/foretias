# Fortias — MVP Specification (v0.1 Local-Server Stack)

**Project:** Fortias (Free and Open-source Resilient Time Integrity Attestation Service)
**This document:** Implementation guidance for the v0.1 local-server MVP — C11 verified core, Rust node layer, crypto-server abstraction (software backend), Fortias domain types in Rust, and Python bindings via PyO3.
**Companion documents:**
- `FORTIAS_OVERVIEW.md` — design invariants, project structure, milestone roadmap, configuration, build, dev protocol. **Read this first.**
- `FORTIAS_P2P_SPEC.md` — v0.2–v0.8 network layers (libp2p, probity, collision, epoch, encrypted persistence). Do not start that work until v0.1 is stable.
- `FORTIAS_ENCLAVE_SPEC.md` — v0.9+ custom-plugin backends. Restricted; do not consult until v0.8 lands.

**Target:** AI Coding Specialist for execution. Comments to human reader in parenthesis `(@human ...)`.

---

## READING ORDER

Before touching any file in this document:

1. Read `fortias-v1.md` — defines TimeBeing, Tick, Calendar, Chronomatter, Fortis, TimeFamily.
2. Read `FORTIAS_OVERVIEW.md` end to end — Parts 0 (invariants), 1 (project structure), 2 (MVP target), 3 (mapping), 14 (milestones).
3. Read this document end to end.
4. Scan the existing Python prototype under `fortias/` for current code shape.

---

## PART 4 — C11 VERIFIED CORE

### 4.1 Requirements

- Standard: `-std=c11` enforced at compile time via `_Static_assert(__STDC_VERSION__ >= 201112L)`
- No dynamic allocation in core
- No global mutable state
- Caller owns all buffers; core receives pointers
- Sensitive state zeroed via `core_memzero()` on all error exits
- All public functions return `CoreResult` — no exceptions, no abort
- Compiler flags: `-Wall -Wextra -Wpedantic -Werror -fno-strict-aliasing -fstack-protector -fvisibility=hidden -O2`
- Primary compiler: `clang`. Secondary in CI: `gcc`
- Depends on: libsodium (Ed25519, hash, mem utilities). For P-256, use mbedTLS or BoringSSL-style C primitives.

### 4.2 Public Header — `fortias/p2p/core/include/fortias_core.h`

```c
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

#ifdef __cplusplus
}
#endif
```

### 4.3 Implementation Notes

- `fortias_ed25519_*`: use libsodium `crypto_sign_*` functions. Private key is stored as 32-byte seed; libsodium's 64-byte secret key is derived internally per call.
- `fortias_p256_*`: use mbedTLS ECDSA. Keep the API signature-compatible with Ed25519 where possible.
- `fortias_hash_sha256`: use libsodium `crypto_hash_sha256`.
- `fortias_hash_blake3`: vendor BLAKE3 reference C implementation.
- `fortias_hash_legacy_insecure_md5`: simple public-domain MD5. Must include a compile-time warning banner in a comment block. Do NOT link against OpenSSL for this — vendor a small pure-C implementation.
- `fortias_hash_legacy_insecure_sha1`: same treatment as MD5.
- `fortias_noise_*`: implement Noise_XX manually using libsodium `crypto_kx_*` and `crypto_aead_chacha20poly1305_ietf_*`. Both Ed25519 and P-256 variants share the state machine; only the ECDH step differs.
- `fortias_merkle_*`: sparse merkle over SHA-256 via `fortias_hash_sha256`.
- `fortias_frost_*`: implement Schnorr math over Ed25519 using libsodium `crypto_core_ed25519_*` scalar and point operations. Alternatively, gate this behind a compile flag `FORTIAS_CORE_FROST=1` and stub out when disabled — the Rust layer uses the `frost-ed25519` crate for protocol flow so the C11 primitives are only needed if a future target needs FROST fully inside a custom plugin.
- `fortias_nullifier_derive`: `HMAC-SHA256(priv, context)`.
- `fortias_rng_bytes`: software backend reads from `/dev/urandom` on Unix, `BCryptGenRandom` on Windows. A custom plugin may replace this symbol via link-time substitution.
- `fortias_memzero`: volatile pointer loop. Do not rely on libsodium's `sodium_memzero` to avoid circular dependency.

### 4.4 C11 Tests

`fortias/p2p/core/tests/test_*.c` — one per source file. Each must:

- Test happy path with known test vectors
- Test every error code is reachable
- Test memzero actually zeroes (volatile read back after)
- Test Noise handshake round-trip (both Ed25519 and P-256 paths)
- Test FROST round1 → sign_share → aggregate with k=2, n=3

Use `ctest` via `CMakeLists.txt`.

---

## PART 5 — RUST NODE LAYER

### 5.1 `fortias/p2p/node/Cargo.toml`

```toml
[package]
name    = "fortias_p2p"
version = "0.1.0"
edition = "2021"

[lib]
name        = "fortias_p2p"
crate-type  = ["cdylib", "rlib", "staticlib"]

[features]
default     = ["python", "software-crypto"]
python      = ["dep:pyo3"]
go          = []
software-crypto = []
# Feature flags for custom-plugin backends are documented in
# FORTIAS_ENCLAVE_SPEC.md and added in the v0.9+ milestones.

[dependencies]
libp2p = { version = "0.54", features = [
    "tcp", "quic", "noise", "yamux",
    "kad", "gossipsub", "identify", "ping",
    "relay", "autonat", "upnp", "macros",
]}

tokio        = { version = "1", features = ["full"] }
tokio-util   = "0.7"
futures      = "0.3"

frost-ed25519 = "2"
frost-core    = "2"

ed25519-dalek = { version = "2", features = ["rand_core"] }
p256          = { version = "0.13", features = ["ecdh", "ecdsa", "pem"] }
sha2          = "0.10"
blake3        = "1"
hmac          = "0.12"
chacha20poly1305 = "0.10"
rand          = "0.8"
rand_core     = "0.6"

serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
bincode      = "2"
toml         = "0.8"

pyo3         = { version = "0.21", features = ["extension-module"], optional = true }

thiserror           = "2"
tracing             = "0.1"
tracing-subscriber  = "0.3"
once_cell           = "1"
parking_lot         = "0.12"
uuid                = { version = "1", features = ["v4", "serde"] }

[build-dependencies]
cc      = "1.0"
bindgen = "0.70"

[profile.release]
opt-level = 2
lto       = true
strip     = true
```

### 5.2 `fortias/p2p/node/build.rs`

Compile the C11 core, link libsodium, run bindgen over the public header. Follow this pattern:

```rust
use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let core_dir = PathBuf::from(&manifest).join("../core");

    let sources = [
        "src/identity_ed25519.c",
        "src/identity_p256.c",
        "src/signing_ed25519.c",
        "src/signing_p256.c",
        "src/hash_sha256.c",
        "src/hash_blake3.c",
        "src/hash_legacy_insecure_md5.c",
        "src/hash_legacy_insecure_sha1.c",
        "src/noise_xx.c",
        "src/merkle.c",
        "src/frost_ed25519.c",
        "src/nullifier.c",
        "src/rng_mix.c",
        "src/memzero.c",
    ];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", core_dir.join(s).display());
    }
    println!("cargo:rerun-if-changed={}/include/fortias_core.h", core_dir.display());

    let mut build = cc::Build::new();
    build
        .std("c11")
        .flag("-Wall").flag("-Wextra").flag("-Wpedantic").flag("-Werror")
        .flag("-O2")
        .flag("-fno-strict-aliasing")
        .flag("-fstack-protector")
        .flag("-fvisibility=hidden")
        .include(core_dir.join("include"))
        .include(core_dir.join("src"));

    for s in &sources {
        build.file(core_dir.join(s));
    }
    build.compile("fortias_core");

    println!("cargo:rustc-link-lib=sodium");
    println!("cargo:rustc-link-lib=mbedtls");
    println!("cargo:rustc-link-lib=mbedcrypto");
    println!("cargo:rustc-link-lib=mbedx509");

    let bindings = bindgen::Builder::default()
        .header(core_dir.join("include/fortias_core.h").to_str().unwrap())
        .clang_arg(format!("-I{}", core_dir.join("include").display()))
        .allowlist_type("Fortias.*")
        .allowlist_function("fortias_.*")
        .allowlist_var("FORTIAS_.*")
        .derive_debug(true)
        .derive_copy(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed on fortias_core.h");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("core_bindings.rs")).unwrap();
}
```

### 5.3 Safe Wrappers — `src/core/*.rs`

Standard pattern:

```rust
// src/core/noise.rs
use super::bindings::*;
use crate::error::CryptoError;

pub struct NoiseSession {
    state: Box<FortiasNoiseState>,
}

impl NoiseSession {
    pub fn new_initiator_ed25519(
        my_priv:   &FortiasPrivKey32,
        their_pub: &FortiasPubKey32,
    ) -> Result<Self, CryptoError> {
        let mut state = Box::new(unsafe { std::mem::zeroed() });
        let rc = unsafe {
            fortias_noise_init_ed25519(&mut *state, my_priv, their_pub, true)
        };
        check(rc)?;
        Ok(Self { state })
    }

    pub fn new_responder_ed25519(my_priv: &FortiasPrivKey32) -> Result<Self, CryptoError> {
        let mut state = Box::new(unsafe { std::mem::zeroed() });
        let rc = unsafe {
            fortias_noise_init_ed25519(&mut *state, my_priv, std::ptr::null(), false)
        };
        check(rc)?;
        Ok(Self { state })
    }

    // ... step / send / recv ...
}

impl Drop for NoiseSession {
    fn drop(&mut self) {
        unsafe { fortias_noise_destroy(&mut *self.state); }
    }
}
```

All safe wrappers follow this pattern: Box the state, implement Drop to call the C destroy function, all calls through `unsafe { ... }` inside methods that return Rust `Result`.

---

## PART 6 — CRYPTO-SERVER ABSTRACTION

This is the layer that makes the cryptographic backend pluggable. It is the single point through which all key operations flow. The MVP ships with a single backend (software). A custom plugin may be slotted in later under the same trait — implementation details of any such plugin live in `FORTIAS_ENCLAVE_SPEC.md`.

### 6.1 Design Principles

- **Synchronous API** — you confirmed this. Each call returns immediately with a result.
- **Single identity per instance** — you confirmed this. Construct multiple CryptoServers if the application needs multiple identities.
- **Private key never escapes** — every method that would expose a key is absent from the trait. Only operations that consume keys internally are exposed.
- **Opaque backend** — the trait tells callers only what capabilities the backend has, not implementation details.
- **Software backend is always available** — it's the fallback when no custom plugin is loaded.

### 6.2 The Trait — `src/crypto_server/mod.rs`

```rust
use crate::core::bindings::{
    FortiasPubKey32, FortiasPubKey33, FortiasPeerID, FortiasSig64,
    FortiasHash32, FortiasHash16, FortiasHash20, FortiasCurve,
};
use crate::error::CryptoError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicKeyBytes {
    Ed25519(FortiasPubKey32),
    P256Compressed(FortiasPubKey33),
}

#[derive(Debug, Clone)]
pub struct SharedSecret(pub [u8; 32]);

impl Drop for SharedSecret {
    fn drop(&mut self) { self.0.fill(0); }
}

#[derive(Debug, Clone)]
pub struct SealedBlob {
    pub ciphertext: Vec<u8>,
    pub nonce:      [u8; 24],
    pub version:    u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CryptoServerCapabilities {
    pub backend_name:        &'static str,   // "software" | "custom"
    pub curve:               FortiasCurve,   // identity curve
    pub supports_proof:      bool,           // see backend_self_proof()
    pub supports_sealing:    bool,
    pub max_sealed_bytes:    usize,
    pub typical_sign_us:     u32,            // for capacity planning
    pub typical_ecdh_us:     u32,
}

/// The CryptoServer is the per-entity identity and key vault.
///
/// Contract:
///   - Construction generates a fresh identity keypair.
///   - The public key is retrievable; the private key is not.
///   - Identity lives for the server's lifetime only.
///   - Dropping the server zeroes all key material.
pub trait CryptoServer: Send + Sync {
    // ── Identity ────────────────────────────────────────
    fn public_key(&self) -> PublicKeyBytes;
    fn peer_id(&self)    -> FortiasPeerID;
    fn curve(&self)      -> FortiasCurve;
    fn capabilities(&self) -> CryptoServerCapabilities;

    // ── Signing ─────────────────────────────────────────
    fn sign(&self, msg: &[u8]) -> Result<FortiasSig64, CryptoError>;

    // ── Verification (any pub key, not just mine) ───────
    fn verify_ed25519(
        &self,
        pub_key: &FortiasPubKey32,
        msg:     &[u8],
        sig:     &FortiasSig64,
    ) -> Result<bool, CryptoError>;

    fn verify_p256(
        &self,
        pub_key: &FortiasPubKey33,
        msg:     &[u8],
        sig:     &FortiasSig64,
    ) -> Result<bool, CryptoError>;

    // ── ECDH for Noise ──────────────────────────────────
    fn ecdh_ed25519(&self, peer_pub: &FortiasPubKey32) -> Result<SharedSecret, CryptoError>;
    fn ecdh_p256(&self,    peer_pub: &FortiasPubKey33) -> Result<SharedSecret, CryptoError>;

    // ── Hashing (secure) ────────────────────────────────
    fn sha256(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError>;
    fn blake3(&self, data: &[u8]) -> Result<FortiasHash32, CryptoError>;

    // ── Hashing (legacy, noncrypto use only) ────────────
    fn legacy_insecure_md5(&self,  data: &[u8]) -> Result<FortiasHash16, CryptoError>;
    fn legacy_insecure_sha1(&self, data: &[u8]) -> Result<FortiasHash20, CryptoError>;

    // ── Sealing ─────────────────────────────────────────
    /// Encrypt data under own public key. Only this instance can unseal.
    fn seal_for_self(&self,   data: &[u8]) -> Result<SealedBlob, CryptoError>;
    fn unseal_for_self(&self, blob: &SealedBlob) -> Result<Vec<u8>, CryptoError>;

    // ── RNG ─────────────────────────────────────────────
    fn random_bytes(&self, out: &mut [u8]) -> Result<(), CryptoError>;

    // ── FROST (optional per backend) ────────────────────
    fn store_frost_share(&self, committee_id: &str, share: &[u8]) -> Result<(), CryptoError>;
    fn frost_sign_partial(&self, committee_id: &str, session_state: &[u8]) -> Result<Vec<u8>, CryptoError>;

    // ── Optional backend self-proof ─────────────────────
    /// Returns optional backend-specific proof bytes.
    /// The software backend always returns `None`.
    /// A custom plugin may return non-`None`; interpretation is plugin-specific
    /// and documented in `FORTIAS_ENCLAVE_SPEC.md`.
    fn backend_self_proof(&self, challenge: &[u8]) -> Result<Option<Vec<u8>>, CryptoError>;
}
```

### 6.3 Software Backend — `src/crypto_server/software.rs`

```rust
pub struct SoftwareCryptoServer {
    curve:    FortiasCurve,
    pub_key:  PublicKeyBytes,
    priv_key: Zeroizing<[u8; 32]>,  // zeroizing wraps and drops cleanly
    peer_id:  FortiasPeerID,
    // Sealing key — derived from pub+priv via HKDF, kept in memory
    seal_key: Zeroizing<[u8; 32]>,
    frost_shares: parking_lot::Mutex<std::collections::HashMap<String, Zeroizing<Vec<u8>>>>,
}

impl SoftwareCryptoServer {
    pub fn generate(curve: FortiasCurve) -> Result<Self, CryptoError> {
        match curve {
            FortiasCurve::Ed25519 => {
                let mut pub_bytes  = [0u8; 32];
                let mut priv_bytes = [0u8; 32];
                let rc = unsafe {
                    fortias_ed25519_generate_keypair(
                        pub_bytes.as_mut_ptr() as _,
                        priv_bytes.as_mut_ptr() as _,
                    )
                };
                check(rc)?;
                let pub_key = FortiasPubKey32 { bytes: pub_bytes };
                let mut pid = FortiasPeerID { bytes: [0; 32] };
                let rc = unsafe { fortias_ed25519_derive_peer_id(&pub_key, &mut pid) };
                check(rc)?;

                // Derive a sealing key via HKDF-SHA256(priv_bytes, "fortias-seal-v1")
                let seal_key = derive_seal_key(&priv_bytes);

                Ok(Self {
                    curve,
                    pub_key: PublicKeyBytes::Ed25519(pub_key),
                    priv_key: Zeroizing::new(priv_bytes),
                    peer_id: pid,
                    seal_key: Zeroizing::new(seal_key),
                    frost_shares: Default::default(),
                })
            }
            FortiasCurve::P256 => { /* analogous */ todo!() }
        }
    }
}

impl CryptoServer for SoftwareCryptoServer {
    // ... implement all trait methods by calling into C11 core ...
    // backend_self_proof() returns Ok(None) for the software backend.
}
```

**Performance characteristics (document in comments for each method):**

```
typical_sign_us (software, Ed25519):   ~50 μs
typical_sign_us (software, P-256):     ~300 μs
typical_ecdh_us (software, X25519):    ~100 μs
typical_ecdh_us (software, P-256):     ~500 μs
```

### 6.4 Custom Plugin Backends

Stub these initially so the trait is satisfied for any later backend. Implementation work for v0.9+ — including the plugin module layout, build system, and runtime selection — is documented in `FORTIAS_ENCLAVE_SPEC.md` and is intentionally not part of the v0.1 MVP.

### 6.5 CryptoServer Factory

```rust
// src/crypto_server/mod.rs (bottom)

pub fn new_best_available(curve: FortiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    // v0.9+: try custom plugin backends first if their cargo features are enabled.
    // See FORTIAS_ENCLAVE_SPEC.md for the gating logic.
    let software = SoftwareCryptoServer::generate(curve)?;
    Ok(Box::new(software))
}

pub fn new_software(curve: FortiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    Ok(Box::new(SoftwareCryptoServer::generate(curve)?))
}
```

---

## PART 7 — FORTIAS DOMAIN TYPES IN RUST

These mirror the existing Python types. They exist to let the Rust side carry TickRecord / Fortis / Calendar values natively and to let the Python side either use the Rust types (via PyO3) or keep using the existing pure-Python implementation.

### 7.1 `src/fortias/tick.rs`

```rust
use serde::{Deserialize, Serialize};
use crate::core::bindings::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickRecord {
    pub tick_number:     u64,       // nanoseconds since Unix epoch
    pub public_key:      Vec<u8>,   // hex-decoded
    pub forward_fortis:  Vec<u8>,
    pub backward_fortis: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fortis {
    pub tick_number:     u64,
    pub content_hash:    [u8; 32], // SHA-256
    pub signature:       [u8; 64], // Ed25519 or P-256
    pub tbid:            [u8; 16], // UUID v4 bytes
    pub echo:            String,
    pub tbn:             String,
}

pub fn stamp(
    server: &dyn CryptoServer,
    tbid:   &[u8; 16],
    tick_number: u64,
    content: &[u8],
    echo:    &str,
    tbn:     &str,
) -> Result<Fortis, NodeError> {
    // Mirrors the existing _stamp() in fortias/_timebeing.py
    let mut sig_input = Vec::with_capacity(16 + 8 + content.len());
    sig_input.extend_from_slice(tbid);
    sig_input.extend_from_slice(&tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let sig = server.sign(&sig_input)?;
    let content_hash = server.sha256(content)?;

    Ok(Fortis {
        tick_number,
        content_hash: content_hash.bytes,
        signature:    sig.bytes,
        tbid:         *tbid,
        echo:         echo.to_string(),
        tbn:          tbn.to_string(),
    })
}

pub fn verify(
    server:  &dyn CryptoServer,
    fortis:  &Fortis,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    // Mirrors the existing _verify() in fortias/_timebeing.py

    // 1. Content hash check
    let recomputed = server.sha256(content)?;
    if recomputed.bytes != fortis.content_hash {
        return Ok(false);
    }

    // 2. Look up the public key at this tick
    let records = calendar.get(fortis.tick_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick".into()))?;

    // 3. Reconstruct signature input
    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&fortis.tbid);
    sig_input.extend_from_slice(&fortis.tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    // 4. Verify — assume Ed25519 for now; P-256 path when curve stored in calendar
    let pub_key = FortiasPubKey32 {
        bytes: rec.public_key[..32].try_into()
            .map_err(|_| NodeError::BadFormat("public_key length".into()))?,
    };
    let sig = FortiasSig64 { bytes: fortis.signature };
    let ok = server.verify_ed25519(&pub_key, &sig_input, &sig)?;
    Ok(ok)
}

pub trait CalendarLookup: Send + Sync {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError>;
    fn latest(&self) -> Option<u64>;
}
```

### 7.2 `src/fortias/calendar.rs`

```rust
use super::tick::{TickRecord, CalendarLookup};

pub struct Calendar {
    tbid:        [u8; 16],
    stamp_tbid:  [u8; 16],
    ticks:       Vec<TickRecord>,
}

impl Calendar {
    pub fn new(tbid: [u8; 16]) -> Self {
        Self { tbid, stamp_tbid: tbid, ticks: Vec::new() }
    }

    pub fn append(&mut self, record: TickRecord) {
        assert!(self.ticks.last().map(|r| record.tick_number > r.tick_number).unwrap_or(true),
                "tick numbers must be strictly ascending");
        self.ticks.push(record);
    }

    pub fn integrity_check(&self, server: &dyn CryptoServer) -> Result<bool, NodeError> {
        // Mirrors Calendar.integrity_check in Python
        for pair in self.ticks.windows(2) {
            if !verify_pair(server, &pair[0], &pair[1], &self.tbid)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError> {
        Ok(self.ticks.iter()
            .filter(|t| t.tick_number >= tick_number)
            .take(count)
            .cloned()
            .collect())
    }

    fn latest(&self) -> Option<u64> {
        self.ticks.last().map(|t| t.tick_number)
    }
}
```

For v0.1 the calendar is held in memory and persisted with the existing
plaintext JSON format inherited from the Python prototype. The encrypted
JSONL upgrade is a v0.7 milestone documented in `FORTIAS_P2P_SPEC.md`
Part 12.

### 7.3 `src/fortias/timefamily.rs`

```rust
/// Rust-side TimeFamily. Mirrors the Python TimeFamily orchestrator.
///
/// The Python TimeFamily in the existing code can either:
///   1. Keep its existing pure-Python implementation (backward compat), OR
///   2. Delegate to this Rust TimeFamily via PyO3 (new path).
///
/// The choice is runtime-configurable. See Part 13 for the Python integration.
pub struct TimeFamily {
    tbid:         [u8; 16],
    tbn:          String,
    chronon_ns:   u64,
    serialized:   bool,
    server:       Arc<dyn CryptoServer>,
    calendar:     parking_lot::RwLock<Calendar>,
    current_tick: parking_lot::Mutex<u64>,
    ticker:       Option<std::thread::JoinHandle<()>>,
    shutdown:     Arc<std::sync::atomic::AtomicBool>,
}

impl TimeFamily {
    pub fn new(
        name:       &str,
        chronon_ns: u64,
        serialized: bool,
        server:     Arc<dyn CryptoServer>,
    ) -> Result<Self, NodeError> {
        let tbid = *uuid::Uuid::new_v4().as_bytes();
        let tbn  = format!("Time Family {}", hex::encode(tbid));
        // ... initialize genesis tick using server to sign ...
        // ... spawn ticker thread if !serialized ...
    }

    pub fn stamp(&self, content: &[u8], echo: &str) -> Result<Fortis, NodeError> {
        let tick_number = *self.current_tick.lock();
        crate::fortias::tick::stamp(
            self.server.as_ref(), &self.tbid, tick_number,
            content, echo, &self.tbn,
        )
    }

    pub fn verify(&self, fortis: &Fortis, content: &[u8]) -> Result<bool, NodeError> {
        let cal = self.calendar.read();
        crate::fortias::tick::verify(
            self.server.as_ref(), fortis, content, &*cal,
        )
    }

    // ... tick(), get(), save(), load() ...
}
```

---

## PART 13 — PYTHON BINDINGS & INTEGRATION

### 13.1 Two Modes

The existing Python code must keep working. The new Rust code is added alongside and called via PyO3.

**Mode A (default, unchanged):** `fortias/` package runs in pure Python using
the existing Chronomatter / Calendar / TimeFamily implementations and
the Python `cryptography` library.

**Mode B (opt-in):** `fortias/` package internally delegates to
`fortias_p2p` Rust crate for all crypto, networking, DHT, gossip, epoch,
and P2P work. The Python user-facing API does not change.

Selection is via `Config.use_native = True` or environment variable `FORTIAS_USE_NATIVE=1`.

### 13.2 `fortias/p2p/bindings/python/fortias_p2p/__init__.py`

```python
"""Fortias P2P native bindings. Loads the Rust-built .so and re-exports the API."""

from ._fortias_p2p import (
    Node,
    CryptoServer,
    Fortis,
    TickRecord,
    Calendar,
    TimeFamily,
    NodeEvent,
    ProbityReport,
    EpochSnapshot,
)

__all__ = [
    "Node", "CryptoServer", "Fortis", "TickRecord", "Calendar",
    "TimeFamily", "NodeEvent", "ProbityReport", "EpochSnapshot",
]
```

### 13.3 PyO3 Bindings — `fortias/p2p/node/src/api/python.rs`

Expose:

- `CryptoServer` — construct via `CryptoServer(curve="ed25519")` or `CryptoServer.new_best_available()`
- `TimeFamily` — Rust-side orchestrator
- `Node` — the full P2P node
- `Fortis`, `TickRecord` — data types
- `NodeEvent` polling API

Method names match the existing Python API where relevant:

```python
from fortias_p2p import TimeFamily, CryptoServer

cs = CryptoServer.new_best_available(curve="ed25519")
tbf = TimeFamily(server=cs, chronon_ns=60_000_000_000, serialized=False)

fortis = tbf.stamp("hello world")
assert tbf.verify("hello world", fortis)
```

### 13.4 Existing Python Integration

Modify `fortias/timefamily.py`:

```python
import os
from .config import Config

def _should_use_native(config: Config) -> bool:
    if getattr(config, "use_native", False):
        return True
    return os.environ.get("FORTIAS_USE_NATIVE", "0") == "1"


class TimeFamily:
    def __new__(cls, *args, **kwargs):
        config = kwargs.get("config") or Config.resolve(kwargs.get("persist_path"))
        if _should_use_native(config):
            try:
                from fortias_p2p import TimeFamily as NativeTimeFamily
                # Construct the native one and wrap it in a compat shim so the
                # rest of the Python code can use either without branching.
                return NativeTimeFamilyShim(NativeTimeFamily(*args, **kwargs))
            except ImportError:
                # Fall back to pure-python
                pass
        return super().__new__(cls)

    def __init__(self, name="timebeing", chronon_ns=60_000_000_000.0, ...):
        # existing implementation preserved
        ...
```

This lets all existing tests continue to pass without the native library
present, and lets advanced users opt into the native path.

---

# END OF MVP SPECIFICATION

(@human — when v0.1 lands, the C11 core, Rust crate, and Python CLI are all wired together end-to-end. The next document to consult is `FORTIAS_P2P_SPEC.md` for v0.2–v0.8.)
