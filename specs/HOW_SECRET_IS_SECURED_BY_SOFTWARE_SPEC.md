# HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md

**Date:** 2026-05-20
**Application:** Foretias v0.3
**Scope:** Secret key lifecycle across all software layers — dependency libraries, C11 core, foretias-core, foretias-client, foretias-server, and future language bindings.
**Purpose:** Specify the secrecy guarantees for every secret key type in the system, zone by zone. Each zone documents what objects *can* and *cannot* be represented or accessed within that domain, with explicit language-level and practical secrecy statements.

---

## Terminology: Two Kinds of Secrecy

This specification distinguishes two dimensions of secrecy for every secret:

| Dimension | Question | Example |
|-----------|----------|---------|
| **Language Secrecy** | Does the language/type system prevent access? | "Rust has no syntax to access this field." |
| **Practical Secrecy** | Even with full source access, can an attacker reconstruct the secret? | "Rust code cannot easily find the bytes and KEK to compute the private key." |

A secret can be:
- **Language-secret + Practical-secret** — Ideal. No path exists.
- **Language-secret + Practical-accessible** — Type system blocks it, but the pieces exist elsewhere (e.g., KEK in global state).
- **Language-accessible + Practical-secret** — Raw bytes are reachable, but encrypted (useless without KEK).
- **Language-accessible + Practical-accessible** — **VULNERABILITY**. Raw secret bytes are directly reachable.

---

## Zone 0: Library Facilities

This zone documents the memory zeroing and secret-handling facilities provided by our dependencies. These are the primitives that higher zones build upon.

### 0.1 libsodium

**Library:** libsodium (via `libsodium-dev`)
**Language:** C
**Linkage:** Dynamic library linked by C11 core

| Facility | API | Purpose | Platform Coverage | Compiler Resistance |
|----------|-----|---------|-------------------|---------------------|
| `sodium_memzero(void *ptr, size_t len)` | `#include <sodium.h>` | Secure memory zeroing | All platforms | Uses `volatile` writes + platform-specific optimizations (`explicit_bzero` on BSD/macOS, `SecureZeroMemory` on Windows, inline assembly on some architectures) |
| `randombytes_buf(void *buf, size_t size)` | `#include <sodium.h>` | CSPRNG byte generation | All platforms | OS entropy source (`/dev/urandom`, `getentropy`, `BCryptGenRandom`) |
| `crypto_secretbox_easy` / `crypto_secretbox_open_easy` | `#include <sodium.h>` | ChaCha20-Poly1305 AEAD encryption/decryption | All platforms | N/A (not a zeroing function) |
| `crypto_sign_seed_keypair` | `#include <sodium.h>` | Ed25519 keypair from seed | All platforms | N/A |
| `crypto_scalarmult` / `crypto_scalarmult_base` | `#include <sodium.h>` | X25519 Diffie-Hellman | All platforms | Constant-time implementation |
| `sodium_memcmp(const void *a, const void *b, size_t len)` | `#include <sodium.h>` | Constant-time memory comparison | All platforms | Constant-time |
| `sodium_mprotect(void *ptr, size_t size, int prot)` | `#include <sodium.h>` | Memory protection (mlock/mprotect) | Unix-like | Prevents swap/pageout |

**Current Usage in Foretias:**
- `randombytes_buf` — Used in `privkey.c:69` for KEK generation, `privkey.c:90` for seed generation.
- `crypto_secretbox_*` — Used in `privkey.c:33-50` for seed encryption/decryption.
- `crypto_sign_seed_keypair` — Used in `privkey.c:95`, `signing_ed25519.c:12`.
- `crypto_scalarmult*` — Used in `noise_xx.c:42` for X25519 DH.
- `crypto_aead_chacha20poly1305_ietf_*` — Used in `noise_xx.c:57-89` for Noise encrypted transport.
- `crypto_hash_sha256_*`, `crypto_auth_hmacsha256` — Used throughout for hashing and HMAC.
- **NOT used:** `sodium_memzero` — Foretias uses custom `foretias_memzero` instead.
- **NOT used:** `sodium_memcmp` — No constant-time comparison for secrets in current code.
- **NOT used:** `sodium_mprotect` — No memory protection against swap/pageout.

**Specified Requirement (REQ-Z0.1):** Replace `foretias_memzero` with `sodium_memzero` throughout the C11 core. Rationale: `sodium_memzero` has platform-specific optimizations that our custom `volatile uint8_t*` loop lacks. The functional behavior is identical, but `sodium_memzero` is more robust against compiler optimization on edge platforms.

**Specified Requirement (REQ-Z0.2):** Use `sodium_memcmp` for any constant-time comparison of secret material (signatures, MACs, authentication tags). Currently no such comparisons exist in the C11 core, but any future additions must use this facility.

**Specified Requirement (REQ-Z0.3):** Consider `sodium_mprotect` for the KEK and per-tick private key memory regions. This prevents the OS from swapping secret material to disk. Marked as **P2** (nice-to-have) since it requires tracking the exact memory region and is platform-specific.

### 0.2 liboqs (Open Quantum Safe)

**Library:** liboqs v0.13.0 (cloned and built by `build.rs`)
**Language:** C
**Linkage:** Static library linked by C11 core

| Facility | API | Purpose | Platform Coverage | Compiler Resistance |
|----------|-----|---------|-------------------|---------------------|
| `OQS_MEM_cleanse(void *ptr, size_t len)` | `#include <oqs/oqs.h>` | Secure memory zeroing | All platforms | Uses `volatile` writes + `RtlSecureZeroMemory` on Windows |
| `OQS_SIG_*_keypair` | `#include <oqs/oqs.h>` | PQC keypair generation | All platforms | N/A |
| `OQS_SIG_*_sign` | `#include <oqs/oqs.h>` | PQC signing | All platforms | N/A |
| `OQS_SIG_*_verify` | `#include <oqs/oqs.h>` | PQC verification | All platforms | Constant-time verification |
| `OQS_KEM_*_keypair` | `#include <oqs/oqs.h>` | PQC KEM keypair | All platforms | N/A |
| `OQS_KEM_*_encaps` / `OQS_KEM_*_decaps` | `#include <oqs/oqs.h>` | PQC key encapsulation | All platforms | N/A |

**Current Usage in Foretias:**
- `OQS_MEM_cleanse` — Used in `signing_sphincs.c:18, :39, :76, :97` and `signing_dilithium.c:18, :39` for error-path cleanup of secret/signature buffers.
- `OQS_SIG_*` — Used for SPHINCS+ SHA2-128s, SPHINCS+ SHA2-256f, Dilithium3 signing.
- `OQS_KEM_*` — Used for ML-KEM-768 key exchange.

**Specified Requirement (REQ-Z0.4):** `OQS_MEM_cleanse` is used only on error paths in PQC code. On success paths, secret material in `ForetiasSecretKeyVar` is NOT zeroed by liboqs — it persists until the Rust-side `Zeroizing` wrapper drops. This is acceptable because the Rust wrapper provides the zeroing guarantee. Document this split responsibility: C11 zeros on failure; Rust zeros on scope exit.

### 0.3 Rust `zeroize` Crate

**Library:** `zeroize` (Rust crate)
**Language:** Rust
**Linkage:** Dev/runtime dependency of `foretias-core`

| Facility | Type/API | Purpose | Compiler Resistance |
|----------|----------|---------|---------------------|
| `Zeroizing<T>` | Wrapper type | Wraps any `T`; calls `zeroize()` on `Drop` | Uses `core::ptr::write_volatile` + `Drop` guarantee prevents compiler from eliding zeroing |
| `Zeroize::zeroize(&mut self)` | Trait method | Zeroizes `self` in place | `write_volatile` prevents optimization |
| `ZeroizeOnDrop` | Trait | Marker for types that self-zero on drop | N/A |

**Current Usage in Foretias:**
- `Zeroizing<[u8; 32]>` — Seal key in `software.rs:34`.
- `Zeroizing<SignatureBytes>` — SPHINCS+, Dilithium3, SLH-DSA-256f, ML-KEM secrets in `software.rs:38-46`.
- `Zeroizing<Vec<u8>>` — FROST shares in `software.rs:36`.

**Specified Requirement (REQ-Z0.5):** All in-process secret material in Rust code MUST be wrapped in `Zeroizing<T>` at the type level. No exceptions. This is the Rust equivalent of the C11 `foretias_memzero`/`sodium_memzero` discipline.

### 0.4 Custom `foretias_memzero` (Current State)

**Location:** `p2p/core/src/memzero.c:4-6`
**Implementation:**
```c
void foretias_memzero(void* ptr, size_t len) {
    volatile uint8_t* p = (volatile uint8_t*)ptr;
    while (len--) *p++ = 0;
}
```

**Analysis:**
- Uses `volatile` qualifier to prevent compiler optimization from eliding the zeroing.
- Correct for most platforms but lacks platform-specific optimizations present in `sodium_memzero`.
- No `mlock`/`mprotect` integration (secret may be swapped to disk before zeroing).
- No constant-time guarantee (loop iterations depend on `len`).

**Specified Requirement (REQ-Z0.6):** Replace all calls to `foretias_memzero` with `sodium_memzero`. Remove `memzero.c` from the C11 core source list. Update `build.rs` to remove `memzero.c` from the compilation sources.

**Specified Requirement (REQ-Z0.7):** Update `foretias_core.h:382` to remove the `foretias_memzero` declaration. The function is an internal implementation detail, not a public API.

---

## Zone 1: C11 Core

This zone covers `p2p/core/` — the C11 cryptographic primitives. Secrets in this zone are analyzed for what C code can and cannot access.

### 1.1 Per-Tick Ed25519 Private Key (via `ForetiasPrivKey` handle)

**Secret:** 32-byte Ed25519 seed
**Struct:** `struct ForetiasPrivKey` (opaque — definition private to `privkey.c`)

#### Generation
```
privkey.c:80-120 — foretias_privkey_ed25519_generate()
  1. randombytes_buf(seed, 32)                    [line 90]
  2. crypto_sign_seed_keypair(pub, sec, seed)     [line 95]
  3. derive_nonce(key->nonce, key_gen_counter)     [line 104]
  4. encrypt_seed(key->encrypted_key, nonce, seed) [line 107]
  5. foretias_memzero(sec, sizeof sec)             [line 116]
  6. foretias_memzero(seed, sizeof seed)           [line 117]
  7. return key (encrypted seed + nonce + pubkey)  [line 119]
```

#### Storage
```
struct ForetiasPrivKey {
    uint8_t encrypted_key[48];  /* 32 bytes ciphertext + 16 bytes MAC */
    uint8_t nonce[24];          /* ChaCha20 nonce                      */
    uint8_t public_key[32];     /* Cached public key (public info)     */
    ForetiasCurve curve;        /* Curve identifier                    */
};
```
Allocated via `calloc` at `privkey.c:83`. Stored on C heap.

#### Language Secrecy: **LANGUAGE-SECRET**
The `ForetiasPrivKey` struct is an opaque forward declaration (`typedef struct ForetiasPrivKey ForetiasPrivKey;` at `foretias_core.h:399`). C code outside `privkey.c` cannot access the struct fields. The raw seed bytes are never exposed — only the encrypted form exists in the struct.

#### Practical Secrecy: **PRACTICAL-SECRET (with caveat)**
The seed is encrypted with the instance KEK using ChaCha20-Poly1305. Without the KEK, the encrypted seed is cryptographically useless. The KEK is stored as global state in `privkey.c:17-18`, accessible only within that translation unit.

**Caveat:** The KEK is process-global mutable state (`static uint8_t instance_kek[32]`). Any C code within `privkey.c` can access it. A memory dump of the process would reveal both the KEK and the encrypted seeds, allowing reconstruction of all private keys.

#### Usage (Signing)
```
privkey.c:164-193 — foretias_privkey_ed25519_sign()
  1. decrypt_seed(tmp, key->encrypted_key, key->nonce)  [line 169]
  2. crypto_sign_seed_keypair(pub, sec, tmp)            [line 176]
  3. crypto_sign_detached(raw_sig, ..., sec)            [line 182]
  4. memcpy(sig, raw_sig, 64)                           [line 188]
  5. foretias_memzero(sec, sizeof sec)                  [line 189]
  6. foretias_memzero(raw_sig, sizeof raw_sig)          [line 190]
  7. foretias_memzero(tmp, sizeof tmp)                  [line 191]
```
The seed is decrypted into a stack buffer (`tmp[32]`), used for signing, then zeroed. The 64-byte derived key (`sec`) is also zeroed.

#### Destruction
```
privkey.c:252-257 — foretias_privkey_free()
  1. foretias_memzero(key, sizeof(ForetiasPrivKey))  [line 254]
  2. free(key)                                       [line 255]
```
Called from Rust `PrivKeyHandle::drop()` at `identity.rs:110-117`.

#### Cross-Boundary (C → Rust)
The handle is an opaque `NonNull<ForetiasPrivKey>` wrapped in `ManuallyDrop`. Rust code holds only the pointer — it cannot dereference or inspect the struct contents. The raw seed bytes **never cross the FFI boundary**.

**Language Secrecy (Rust side):** Rust has no syntax to access the encrypted seed field. The `PrivKeyHandle` type does not implement `Deref`, `Debug`, `Clone`, or `Copy`.

**Specified Requirement (REQ-Z1.1):** This design is correct and meets the secrecy requirements. No changes needed for the `PrivKeyHandle` path.

### 1.2 Instance KEK (Key Encryption Key)

**Secret:** 32-byte random key
**Location:** `privkey.c:17-18` — `static uint8_t instance_kek[32]`

#### Generation
```
privkey.c:67-72 — foretias_privkey_init()
  1. randombytes_buf(instance_kek, 32)  [line 69]
  2. instance_kek_initialized = true    [line 70]
```
Called once at process startup from `PrivKeyHandle::init()` at `identity.rs:33-36`.

#### Storage
Global static variable in `privkey.c`. File-scope (`static`), not exported.

#### Language Secrecy: **LANGUAGE-SECRET (within C)**
The KEK is `static` — only visible within `privkey.c`. Other C translation units cannot access it.

#### Practical Secrecy: **PRACTICAL-ACCESSIBLE (within process)**
The KEK is in process memory as a global variable. Any code with access to the process's memory (debugger, core dump, hypervisor) can find it. Combined with the encrypted seeds, the KEK allows reconstruction of all private keys.

**Specified Requirement (REQ-Z1.2):** The KEK global state violates `FORETIAS_1_MVP_SPEC §4.1` ("no global state in core"). This is acknowledged as a **design exemption** with the following rationale:
- The KEK is process-scoped and zeroed on shutdown.
- Making it caller-owned would require passing a context pointer through every FFI call, breaking the opaque handle design.
- The alternative (per-handle KEK) would defeat the purpose — each handle would need its own decryption key, requiring storage of that key somewhere.
- **Mitigation:** Consider `sodium_mprotect` (REQ-Z0.3) to prevent the KEK from being swapped to disk.

### 1.3 Ed25519 Raw Seed (via `ForetiasPrivKey32`)

**Secret:** 32-byte Ed25519 seed
**Struct:** `ForetiasPrivKey32 { uint8_t bytes[32]; }` at `foretias_core.h:95`

#### Generation
```
identity_ed25519.c — foretias_ed25519_generate_keypair()
  1. randombytes_buf(priv_out->bytes, 32)
  2. crypto_sign_seed_keypair(pub_out->bytes, sec, priv_out->bytes)
  3. foretias_memzero(sec, sizeof sec)
```

#### Storage
Raw bytes in a `ForetiasPrivKey32` struct. No encryption. No KEK.

#### Language Secrecy: **LANGUAGE-ACCESSIBLE**
The struct is a plain C struct with public fields. Any C code can read `priv.bytes[i]`.

#### Practical Secrecy: **PRACTICAL-ACCESSIBLE**
Raw seed bytes are directly accessible. No encryption layer.

#### Usage
Used by `signing_ed25519.c:5-27` for signing, `nullifier.c:5-20` for nullifier derivation, `noise_xx.c:112-166` for Noise handshake initialization.

#### Cross-Boundary (C → Rust)
**CRITICAL:** `ForetiasPrivKey32` is exposed to Rust via bindgen at `bindings.rs:103-106`:
```rust
#[derive(Debug, Copy, Clone)]
pub struct ForetiasPrivKey32 {
    pub bytes: [u8; 32usize],
}
```
The `Debug` derive means `format!("{:?}", priv_key)` emits the full 32-byte seed. The `Copy` derive means the seed can be duplicated without the programmer's explicit intent.

**Specified Requirement (REQ-Z1.3):** In `build.rs:99`, add `.no_debug("ForetiasPrivKey32")` to the bindgen builder. This suppresses the `Debug` derive on the generated struct. The `Copy` derive is acceptable for FFI interoperability but should be noted as a risk.

**Specified Requirement (REQ-Z1.4):** Code that uses `ForetiasPrivKey32` in Rust MUST zero the bytes after use. Currently, `noise.rs:95` does `priv_key.bytes.fill(0)` after the FFI call — this is correct. Audit all other uses.

### 1.4 SPHINCS+ Secret Key

**Secret:** 96-byte (SHA2-128s) or 64-byte (SHA2-256f) SPHINCS+ secret key
**Struct:** `ForetiasSecretKeyVar { uint8_t bytes[4096]; size_t len; }` at `foretias_core.h:115-118`

#### Generation
```
signing_sphincs.c:65-84 — foretias_sphincs_sha2_256f_keypair()
  1. OQS_SIG_sphincs_sha2_256f_simple_keypair(pub, sec)  [line 75]
  2. On failure: OQS_MEM_cleanse(sec, len)               [line 76]
  3. Returns FORETIAS_OK with secret in caller's buffer   [line 83]
```
The secret is written into the caller-provided `ForetiasSecretKeyVar` buffer. liboqs does NOT zero the secret on success.

#### Storage
Caller-owned buffer. In the Rust wrapper (`kem_mlkem.rs`, `signing_sphincs.rs`), the secret is extracted to a `Vec<u8>` and wrapped in `Zeroizing`.

#### Language Secrecy: **LANGUAGE-ACCESSIBLE (in C)**
The `ForetiasSecretKeyVar` struct has public fields. Any C code can read the secret bytes.

#### Practical Secrecy: **PRACTICAL-ACCESSIBLE (in C)**
Raw secret bytes are directly accessible. No encryption.

#### Cross-Boundary (C → Rust)
**CRITICAL:** `ForetiasSecretKeyVar` is exposed to Rust via bindgen at `bindings.rs:214-218`:
```rust
#[derive(Debug, Copy, Clone)]
pub struct ForetiasSecretKeyVar {
    pub bytes: [u8; 4096usize],
    pub len: usize,
}
```
The `Debug` derive means up to 4096 bytes of secret material can be logged.

**Specified Requirement (REQ-Z1.5):** In `build.rs:99`, add `.no_debug("ForetiasSecretKeyVar")` to the bindgen builder.

**Specified Requirement (REQ-Z1.6):** The Rust wrapper in `software.rs:38` wraps the extracted bytes in `Zeroizing<SignatureBytes>`. This is correct — the C-side buffer is zeroed by the Rust `Zeroizing` wrapper on drop. However, the C-side `ForetiasSecretKeyVar` struct itself is NOT zeroed after extraction. The Rust wrappers (`signing_sphincs.rs`, `kem_mlkem.rs`) should call `foretias_memzero` (or `sodium_memzero`) on the C struct after extracting the bytes.

### 1.5 Dilithium3 Secret Key

**Secret:** 4000-byte Dilithium3 secret key
**Struct:** `ForetiasSecretKeyVar` (same as SPHINCS+)

Same analysis as §1.4. See REQ-Z1.5 through REQ-Z1.6.

### 1.6 ML-KEM-768 Secret Key

**Secret:** 1632-byte ML-KEM-768 secret key
**Struct:** `ForetiasKemSecretKey { uint8_t bytes[2400]; size_t len; }` at `foretias_core.h:130-133`

#### Generation
```
kem_mlkem.c:7-25 — foretias_mlkem_768_keypair()
  1. OQS_KEM_ml_kem_768_keypair(pub, sec)  [line 15]
  2. On failure: OQS_MEM_cleanse(sec, len)  [line 17]
```

#### Cross-Boundary (C → Rust)
**CRITICAL:** `ForetiasKemSecretKey` is exposed to Rust via bindgen at `bindings.rs:259-264`:
```rust
#[derive(Debug, Copy, Clone)]
pub struct ForetiasKemSecretKey {
    pub bytes: [u8; 2400usize],
    pub len: usize,
}
```

**Specified Requirement (REQ-Z1.7):** In `build.rs:99`, add `.no_debug("ForetiasKemSecretKey")` to the bindgen builder.

**Specified Requirement (REQ-Z1.8):** In `kem_mlkem.rs:15`, after extracting `secret.bytes[..secret.len]`, zero the C struct: `unsafe { foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasKemSecretKey>()) }`. Same pattern as `kem_mlkem.rs:53-55` (already done for decapsulate).

### 1.7 TBID V1 Secret Key

**Secret:** 160 bytes (32-byte Ed25519 seed + 128-byte SLH-DSA-SHA2-256f secret)
**Struct:** `ForetiasTbidV1SecretKey` at `foretias_core.h:146-150`

#### Generation
```
signing_tbid.c:7-41 — foretias_tbid_v1_keypair()
  1. foretias_ed25519_generate_keypair(&ed25519_pub, &ed25519_sk)  [line 17]
  2. foretias_sphincs_sha2_256f_keypair(&slh_dsa_secret, ...)      [line 23]
  3. Copy into output structs                                       [line 30-35]
  4. foretias_memzero(&ed25519_sk, 32)                              [line 37]
  5. foretias_memzero(slh_dsa_secret.bytes, len)                    [line 38]
```

#### Cross-Boundary (C → Rust)
**CRITICAL:** `ForetiasTbidV1SecretKey` is exposed to Rust via bindgen at `bindings.rs:305-312`:
```rust
#[derive(Debug, Copy, Clone)]
pub struct ForetiasTbidV1SecretKey {
    pub ed25519_sk: ForetiasPrivKey32,
    pub slh_dsa_sk: [u8; 128usize],
    pub slh_dsa_sk_len: usize,
}
```

**Specified Requirement (REQ-Z1.9):** In `build.rs:99`, add `.no_debug("ForetiasTbidV1SecretKey")` to the bindgen builder.

**Specified Requirement (REQ-Z1.10):** In `signing_tbid.rs:32-43`, the TBID secret is extracted to a plain `Vec<u8>`:
```rust
let mut secret_bytes = vec![0u8; 160];
secret_bytes[..32].copy_from_slice(&secret.ed25519_sk.bytes);
secret_bytes[32..].copy_from_slice(&secret.slh_dsa_sk[..128]);
```
This `Vec<u8>` is NOT wrapped in `Zeroizing`. The C struct IS zeroed at line 41 (`foretias_tbid_v1_secret_zeroize`), but the Rust heap allocation persists. **Wrap `secret_bytes` in `Zeroizing::new(...)`.**

### 1.8 Noise Session State

**Secrets:** `local_static_priv[32]`, `send_key[32]`, `recv_key[32]`, `chaining_key[32]`
**Struct:** `ForetiasNoiseState` at `foretias_core.h:255-272`

#### Generation
```
noise_xx.c:112-166 — foretias_noise_init_ed25519()
  1. Derive X25519 seed from Ed25519 key via HMAC+HKDF  [line 123-130]
  2. crypto_scalarmult_base(epub, seed)                  [line 133]
  3. memcpy(state->local_static_priv, seed, 32)          [line 139]
  4. sodium_memzero(seed, 32)                            [line 141]
```

#### Storage
The `ForetiasNoiseState` struct holds all session keys in plaintext. No encryption.

#### Language Secrecy: **LANGUAGE-ACCESSIBLE (in C)**
All fields are public. Any C code can read the session keys.

#### Cross-Boundary (C → Rust)
**CRITICAL:** `ForetiasNoiseState` is exposed to Rust via bindgen at `bindings.rs:412-428`:
```rust
#[derive(Debug, Copy, Clone)]
pub struct ForetiasNoiseState {
    pub chaining_key: [u8; 32usize],
    pub handshake_hash: [u8; 32usize],
    pub local_static_priv: [u8; 32usize],
    // ... 12 more fields
}
```

**Specified Requirement (REQ-Z1.11):** In `build.rs:99`, add `.no_debug("ForetiasNoiseState")` to the bindgen builder.

**Mitigation (existing):** `NoiseSession` in `noise.rs:31` wraps the state in `ManuallyDrop<NonNull<ForetiasNoiseState>>` and calls `foretias_noise_destroy()` on drop (`noise.rs:208-216`), which calls `foretias_memzero` on the entire struct. This is correct — the RAII wrapper ensures zeroing.

### 1.9 FROST Round 1 State

**Secrets:** `nonce_d[32]`, `nonce_e[32]`
**Struct:** `ForetiasFrostRound1` at `foretias_core.h:337-342`

#### Generation
```
frost_ed25519.c — foretias_frost_round1()
  1. randombytes for nonce_d and nonce_e
```

#### Destruction
```
frost_ed25519.c — foretias_frost_destroy_round1()
  1. foretias_memzero on entire struct
```

**Specified Requirement (REQ-Z1.12):** FROST is currently stubbed in the Rust layer. When implemented, ensure the `ForetiasFrostRound1` struct is zeroed after use. The C-side `foretias_frost_destroy_round1` already provides this.

### 1.10 Summary Table: C11 Zone

| Secret | Language Secrecy (C) | Practical Secrecy (C) | Encrypted? | Zeroed on Destruction? | Bindgen Debug? |
|--------|---------------------|----------------------|------------|----------------------|----------------|
| `ForetiasPrivKey` (handle) | SECRET | SECRET | Yes (KEK+AEAD) | Yes (`privkey_free`) | N/A (opaque) |
| KEK | SECRET (file-scope) | ACCESSIBLE | N/A | Yes (`privkey_cleanup`) | N/A |
| `ForetiasPrivKey32` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** |
| `ForetiasSecretKeyVar` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** |
| `ForetiasKemSecretKey` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — MED** |
| `ForetiasTbidV1SecretKey` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** |
| `ForetiasNoiseState` | ACCESSIBLE | ACCESSIBLE | No | Yes (`noise_destroy`) | **YES — HIGH** |
| `ForetiasFrostRound1` | ACCESSIBLE | ACCESSIBLE | No | Yes (`frost_destroy`) | N/A (not used) |

---

## Zone 2: foretias-core (Rust)

This zone covers `p2p/core-engine/` — the safe Rust wrappers over C11 FFI, domain types, and the `CryptoServer` trait.

### 2.1 PrivKeyHandle (Ed25519 Tick Key)

**Location:** `core-engine/src/core/identity.rs:9-117`
**Type:** `PrivKeyHandle(ManuallyDrop<NonNull<ForetiasPrivKey>>)`

#### Language Secrecy: **LANGUAGE-SECRET**
- No `Deref` — cannot access inner fields.
- No `Debug` — cannot log via `{:?}`.
- No `Clone` / `Copy` — cannot duplicate.
- Only `Send` + `Sync` (unsafe impls, justified by thread-safe C internals).

#### Practical Secrecy: **PRACTICAL-SECRET**
The raw seed bytes exist only in C memory, encrypted with the KEK. Rust code cannot reconstruct the seed without calling into C (which requires the KEK).

#### Operations
- `generate()` — Calls C `foretias_privkey_ed25519_generate()`. Returns opaque handle.
- `from_seed(seed)` — Calls C `foretias_privkey_ed25519_from_seed()`. **Note:** The caller's `seed` buffer is copied into C but NOT zeroed from the Rust side. **Specified Requirement (REQ-Z2.1):** After `from_seed`, zero the caller's buffer: `seed.fill(0)`.
- `sign(msg)` — Calls C `foretias_privkey_ed25519_sign()`. Seed decrypted in C, used for signing, zeroed in C.
- `derive_seal_key(info)` — Calls C `foretias_privkey_derive_seal_key()`. Returns derived key.
- `public_key()` — Returns cached public key (not secret).
- `drop()` — Calls C `foretias_privkey_free()` which memzero's the entire struct.

**Verdict:** Correct design. No changes needed except REQ-Z2.1.

### 2.2 SoftwareCryptoServer

**Location:** `core-engine/src/crypto_server/software.rs:24-47`

| Field | Type | Secret? | Secured By |
|-------|------|---------|------------|
| `priv_key` | `PrivKeyHandle` | Yes | Opaque handle (Zone 2.1) |
| `seal_key` | `Zeroizing<[u8; 32]>` | Yes | `Zeroizing` on drop |
| `frost_shares` | `Mutex<HashMap<String, Zeroizing<Vec<u8>>>>` | Yes | `Zeroizing` on drop |
| `sphincs_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop |
| `dilithium_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop |
| `sphincs_sha2_256f_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop |
| `mlkem_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop |

**Language Secrecy:** All secret fields are secured by either `PrivKeyHandle` (opaque) or `Zeroizing` (auto-zero on drop).

**Practical Secrecy:** Secrets are in process memory but zeroed on scope exit. The `Drop` implementation at `software.rs:135-141` explicitly zeros the seal key; PQC secrets are handled by `Zeroizing`.

**Specified Requirement (REQ-Z2.2):** The `Drop` impl at `software.rs:135-141` manually zeros `self.seal_key` but notes that PQC secrets are handled by `Zeroizing`. This is correct. However, `self.priv_key` (the `PrivKeyHandle`) is NOT explicitly mentioned in the `Drop` impl. It IS dropped automatically by Rust's field-drop order, but add a comment clarifying this:
```rust
impl Drop for SoftwareCryptoServer {
    fn drop(&mut self) {
        // seal_key: explicitly zeroed below
        // priv_key: dropped by Rust field-drop order → PrivKeyHandle::drop() → C11 memzero
        // PQC secrets: dropped by Rust field-drop order → Zeroizing::drop()
        // frost_shares: dropped by Rust field-drop order → Zeroizing::drop() per entry
        self.seal_key.fill(0);
    }
}
```

### 2.3 Bindings (bindgen-generated)

**Location:** `core-engine/src/core/bindings.rs` (generated by `build.rs:93-102`)

**Current bindgen configuration (`build.rs:93-99`):**
```rust
let bindings = bindgen::Builder::default()
    .header(...)
    .allowlist_type("Foretias.*")
    .allowlist_function("foretias_.*")
    .allowlist_var("FORETIAS_.*")
    .derive_debug(true).derive_copy(true)  // ← PROBLEM
    ...
```

The `.derive_debug(true)` applies to ALL generated structs, including those holding secret material.

**Specified Requirement (REQ-Z2.3):** Update `build.rs:99` to exclude secret types from `Debug` derivation:
```rust
.derive_debug(true)
.derive_copy(true)
.no_debug("ForetiasPrivKey32")
.no_debug("ForetiasSecretKeyVar")
.no_debug("ForetiasKemSecretKey")
.no_debug("ForetiasTbidV1SecretKey")
.no_debug("ForetiasNoiseState")
.no_debug("ForetiasFrostRound1")
```

**Specified Requirement (REQ-Z2.4):** Consider whether `.derive_copy(true)` should also be restricted for secret types. `Copy` on `ForetiasPrivKey32` means the 32-byte seed can be silently duplicated. However, removing `Copy` may break existing FFI patterns. **Decision: Keep `Copy` for now, but document the risk.** A future pass can restrict `Copy` on secret types.

### 2.4 signing_tbid.rs — TBID Secret Extraction

**Location:** `core-engine/src/crypto_server/signing_tbid.rs:18-44`

**Current code:**
```rust
pub fn tbid_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut secret: ForetiasTbidV1SecretKey = unsafe { std::mem::zeroed() };
    let mut public: ForetiasTbidV1PubKey = unsafe { std::mem::zeroed() };
    let rc = unsafe { foretias_tbid_v1_keypair(&mut secret, &mut public) };
    c_result_to_error(rc)?;

    let mut secret_bytes = vec![0u8; 160];  // ← NOT Zeroizing
    secret_bytes[..32].copy_from_slice(&secret.ed25519_sk.bytes);
    secret_bytes[32..].copy_from_slice(&secret.slh_dsa_sk[..128]);

    // ... public_bytes extraction ...

    unsafe { foretias_tbid_v1_secret_zeroize(&mut secret) };  // C struct zeroed
    Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
}
```

**Problem:** `secret_bytes: Vec<u8>` is not wrapped in `Zeroizing`. The 160-byte TBID secret persists in the Rust heap until the `Vec` is dropped (which doesn't zero the memory).

**Specified Requirement (REQ-Z2.5):** Wrap `secret_bytes` in `Zeroizing`:
```rust
use zeroize::Zeroizing;

let mut secret_bytes = Zeroizing::new(vec![0u8; 160]);
// ... extraction ...
Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(
    (*secret_bytes).clone()  // unwrap from Zeroizing for return
)))
```
**However**, `SignatureBytes` itself should be `Zeroizing<Vec<u8>>` internally. Check the `SignatureBytes` type definition. If it's already a newtype over `Vec<u8>` without `Zeroizing`, change it to `Zeroizing<Vec<u8>>`.

**Specified Requirement (REQ-Z2.6):** Audit `SignatureBytes` in `foretias/types.rs`. If it wraps `Vec<u8>` without `Zeroizing`, change it to:
```rust
pub struct SignatureBytes(zeroize::Zeroizing<Vec<u8>>);
```
This ensures ALL signature bytes (including PQC secrets that happen to use this type) are zeroed on drop.

### 2.5 kem_mlkem.rs — ML-KEM Secret Extraction

**Location:** `core-engine/src/crypto_server/kem_mlkem.rs:5-18`

**Current code:**
```rust
pub fn mlkem_768_keypair() -> Result<(SignatureBytes, SignatureBytes), CryptoError> {
    let mut secret: ForetiasKemSecretKey = unsafe { std::mem::zeroed() };
    let mut public: ForetiasKemPubKey = unsafe { std::mem::zeroed() };
    let rc = unsafe { foretias_mlkem_768_keypair(&mut secret, &mut public) };
    c_result_to_error(rc)?;
    let secret_bytes = secret.bytes[..secret.len as usize].to_vec();  // ← NOT zeroed after
    let public_bytes = public.bytes[..public.len as usize].to_vec();
    Ok(...)
}
```

**Problem:** The C struct `secret` is NOT zeroed after extraction. The 1632-byte ML-KEM secret persists in the stack-allocated `ForetiasKemSecretKey` struct.

**Specified Requirement (REQ-Z2.7):** After extracting `secret_bytes`, zero the C struct:
```rust
unsafe {
    foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasKemSecretKey>());
}
```
This pattern is already used in `kem_mlkem.rs:53-55` for `decapsulate`. Apply it consistently to `keypair` and `encapsulate`.

### 2.6 signing.rs — Ed25519 Signing (Raw Key Path)

**Location:** `core-engine/src/core/signing.rs:8-17`

**Current code:**
```rust
pub fn ed25519_sign(priv_key: &ForetiasPrivKey32, msg: &[u8]) -> Result<ForetiasSig64, CryptoError> {
    let mut sig = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        foretias_ed25519_sign(priv_key, msg.as_ptr(), msg.len(), &mut sig)
    };
    c_result_to_error(rc)?;
    Ok(sig)
}
```

**Analysis:** This function accepts a `&ForetiasPrivKey32` (raw 32-byte seed). The caller is responsible for ensuring the seed is zeroed after use. This is the "raw key" path — less secure than the `PrivKeyHandle` path.

**Specified Requirement (REQ-Z2.8):** Document this function as the **unsafe/raw key path** and prefer `ed25519_sign_with_handle` for all new code. Add a doc comment:
```rust
/// Sign a message with Ed25519 using a raw seed.
///
/// # Security
/// This function accepts raw seed bytes. The caller is responsible for
/// ensuring the seed is zeroed after use. Prefer [`ed25519_sign_with_handle`]
/// which keeps the seed encrypted in C memory.
pub fn ed25519_sign(priv_key: &ForetiasPrivKey32, msg: &[u8]) -> ...
```

### 2.7 Summary Table: foretias-core Zone

| Secret | Rust Type | Language Secrecy | Practical Secrecy | Zeroing |
|--------|-----------|-----------------|-------------------|---------|
| Tick Ed25519 key | `PrivKeyHandle` | SECRET | SECRET | C11 `privkey_free` |
| Seal key | `Zeroizing<[u8;32]>` | ACCESSIBLE | ACCESSIBLE | `Zeroizing` on drop |
| PQC secrets | `Zeroizing<SignatureBytes>` | ACCESSIBLE | ACCESSIBLE | `Zeroizing` on drop |
| FROST shares | `Zeroizing<Vec<u8>>` | ACCESSIBLE | ACCESSIBLE | `Zeroizing` on drop |
| TBID secret | `Vec<u8>` (plain) | ACCESSIBLE | ACCESSIBLE | **NONE — CRIT** |
| ML-KEM secret (C struct) | `ForetiasKemSecretKey` | ACCESSIBLE | ACCESSIBLE | **MISSING in keypair() — HIGH** |
| Raw Ed25519 seed | `ForetiasPrivKey32` | ACCESSIBLE | ACCESSIBLE | Caller's responsibility |

---

## Zone 3: foretias-client (Rust)

This zone covers `p2p/foretias-client/` — the thin client library.

### 3.1 noise_ptp.rs — PtP Client Key Handling

**Location:** `foretias-client/src/noise_ptp.rs:45-121`

**Current code:**
```rust
pub async fn noise_json_rpc(...) -> Result<serde_json::Value, PtPError> {
    // ...
    let (_pub_key, priv_key) = generate_ed25519_keypair()
        .map_err(|e| PtPError::Connect(e.to_string()))?;

    let (mut session, stream) = noise::noise_handshake(stream, &priv_key.bytes, None, true)
        .await
        .map_err(|e| PtPError::Noise(e.to_string()))?;
    // ... session used for encrypted RPC ...
}
```

**Analysis:**
1. `generate_ed25519_keypair()` returns `(ForetiasPubKey32, ForetiasPrivKey32)` — raw key pair.
2. `priv_key.bytes` is passed to `noise_handshake()` which copies it into C memory.
3. `noise.rs:95` zeros the Rust-side copy: `priv_key.bytes.fill(0)`.
4. The `NoiseSession` RAII wrapper zeros the C-side state on drop.

**Language Secrecy:** The `priv_key: ForetiasPrivKey32` is a plain struct with `Copy + Debug`. Before `fill(0)` at `noise.rs:95`, the raw bytes are accessible.

**Practical Secrecy:** The key is ephemeral — generated per-request and zeroed after the handshake. The exposure window is from `generate_ed25519_keypair()` to `noise.rs:95`.

**Specified Requirement (REQ-Z3.1):** The current pattern is acceptable for ephemeral keys. The key is zeroed after use. However, the `Debug` derive on `ForetiasPrivKey32` (REQ-Z1.3) means any logging between generation and zeroing would leak the key. Fixing REQ-Z1.3 closes this gap.

### 3.2 Summary Table: foretias-client Zone

| Secret | Rust Type | Language Secrecy | Practical Secrecy | Zeroing |
|--------|-----------|-----------------|-------------------|---------|
| Ephemeral Ed25519 key | `ForetiasPrivKey32` | ACCESSIBLE | ACCESSIBLE | `fill(0)` in noise.rs:95 |
| Noise session keys | `NoiseSession` (opaque) | SECRET | SECRET | `noise_destroy` on drop |

---

## Zone 4: foretias-server (Rust)

This zone covers `p2p/foretias-server/` — the server and CLI binary.

### 4.1 TimeFamilyServer — Noise Static Keys

**Location:** `foretias-server/src/server/mod.rs:35-45, :78-89`

**Current code:**
```rust
pub struct TimeFamilyServer {
    // ...
    noise_static_priv: [u8; 32],  // ← PLAIN ARRAY
    noise_static_pub: [u8; 32],
}

// In new_with_config():
let (pub_key, priv_key) = generate_ed25519_keypair()?;
Ok(Self {
    // ...
    noise_static_priv: priv_key.bytes,
    noise_static_pub: pub_key.bytes,
})
```

**Analysis:**
1. `noise_static_priv: [u8; 32]` is a plain array. No `Zeroizing` wrapper.
2. `TimeFamilyServer` has no `Drop` impl. The 32-byte private key persists for the entire process lifetime.
3. The key is used for Noise PtP handshakes — it's the server's long-term identity key for encrypted transport.

**Language Secrecy: LANGUAGE-ACCESSIBLE** — Any Rust code with access to the `TimeFamilyServer` instance can read `noise_static_priv`.

**Practical Secrecy: PRACTICAL-ACCESSIBLE** — The key is in process memory as a plain array. A core dump or debugger would reveal it.

**Specified Requirement (REQ-Z4.1):** Wrap `noise_static_priv` in `Zeroizing<[u8; 32]>`:
```rust
use zeroize::Zeroizing;

pub struct TimeFamilyServer {
    // ...
    noise_static_priv: Zeroizing<[u8; 32]>,
    noise_static_pub: [u8; 32],  // public, no wrapping needed
}
```
And in the constructor:
```rust
noise_static_priv: Zeroizing::new(priv_key.bytes),
```

**Specified Requirement (REQ-Z4.2):** After wrapping in `Zeroizing`, the `Drop` impl for `TimeFamilyServer` is NOT needed — `Zeroizing` handles zeroing automatically. However, add a comment documenting the zeroing behavior.

### 4.2 Chronomatter — Tick Keypairs

**Location:** `core-engine/src/chronomatter/mod.rs:24-46, :140-149`

**Current code:**
```rust
struct TickKeyPair {
    pub_key: [u8; 32],
    priv_key: PrivKeyHandle,  // ← Opaque handle, good
}

pub struct Chronomatter {
    // ...
    keypairs: RwLock<Vec<TickKeyPair>>,  // ← GROWS WITHOUT BOUND
}
```

**Analysis:**
1. Each tick generates a new `TickKeyPair` and appends it to `keypairs`.
2. The `PrivKeyHandle` is correctly opaque.
3. **Problem:** The `Vec` grows without bound. Old tick keys are retained in memory even after the tick advances. The `PrivKeyHandle` for old ticks is NOT dropped — it stays in the `Vec`.

**Specified Requirement (REQ-Z4.3):** Bound the `keypairs` Vec to retain only the current and previous tick (2 entries maximum). When a new tick is generated, drop the oldest entry:
```rust
fn generate_and_store_keypair(&self) -> Result<usize, NodeError> {
    let kp = PrivKeyHandle::generate()?;
    let pub_key = kp.public_key()?;
    let mut guard = self.keypairs.write();
    guard.push(TickKeyPair { pub_key, priv_key: kp });
    if guard.len() > 2 {
        guard.remove(0);  // Drop oldest keypair → PrivKeyHandle::drop() → C11 memzero
    }
    Ok(guard.len() - 1)
}
```

### 4.3 Chronomatter — TBID Secret

**Location:** `core-engine/src/chronomatter/mod.rs:30-32`

**Current code:**
```rust
pub struct Chronomatter {
    // ...
    tbid_secret: Option<TbidSecret>,
}
```

**Analysis:** `TbidSecret` is defined in `foretias/types.rs`. Need to check if it's wrapped in `Zeroizing`.

**Specified Requirement (REQ-Z4.4):** Audit `TbidSecret` in `foretias/types.rs`. If it's a newtype over `Vec<u8>` without `Zeroizing`, change it to `Zeroizing<Vec<u8>>`. The TBID secret is the most critical key in the system — it's the dual-key identity that signs the genesis tick.

### 4.4 Summary Table: foretias-server Zone

| Secret | Rust Type | Language Secrecy | Practical Secrecy | Zeroing |
|--------|-----------|-----------------|-------------------|---------|
| Noise static private key | `[u8; 32]` (plain) | ACCESSIBLE | ACCESSIBLE | **NONE — CRIT** |
| Tick keypairs | `Vec<TickKeyPair>` | SECRET (handle) | SECRET (handle) | `PrivKeyHandle::drop` (but unbounded Vec) |
| TBID secret | `Option<TbidSecret>` | Depends on TbidSecret | Depends on TbidSecret | Audit needed |

---

## Zone 5: Bindings and Wrappers (Future)

This zone covers future language bindings (Python PyO3, Java JNI) and the principles they must follow.

### 5.1 Python PyO3 Bindings (Backburnered)

**Status:** Backburnered per `SCOPE_REDUCTION_SPEC.md`. Reintroduction planned.

**Specified Requirement (REQ-Z5.1):** When Python bindings are reintroduced, secret material MUST NOT cross the Python FFI boundary as raw bytes. Use the same opaque handle pattern as `PrivKeyHandle`:
- Python receives an opaque integer handle (Rust `NonNull` cast to `i64`).
- All operations (sign, verify, derive) go through the handle.
- The handle is dropped when the Python object is garbage collected.
- Python code can NEVER access raw secret bytes.

**Specified Requirement (REQ-Z5.2):** Python's `__repr__` and `__str__` on any object holding secret material MUST emit `<redacted>` or similar. Never the raw bytes.

### 5.2 Java JNI Bindings (Backburnered)

Same requirements as §5.1. Java's `toString()` on secret-holding objects must emit `<redacted>`.

### 5.3 General Binding Principles

**Specified Requirement (REQ-Z5.3):** All language bindings MUST follow the **Opaque Handle Pattern**:
1. Secret material is generated and stored in the C11 core or Rust core-engine.
2. The binding layer receives an opaque handle (pointer/integer).
3. All cryptographic operations go through the handle.
4. The binding layer NEVER sees raw secret bytes.
5. The handle is destroyed when the binding object is garbage collected.

**Specified Requirement (REQ-Z5.4):** The `ForetiasPrivKey` opaque handle (Zone 1.1) is the reference implementation of this pattern. All future bindings should use this handle directly, not create new key types.

---

## Zone 6: Compliance Requirements

This zone consolidates all specified requirements from the COMBINED review that relate to secret security, organized by priority.

### 6.1 P0 — Critical (Must Fix Before Public Release)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z1.3 | Remove `Debug` from `ForetiasPrivKey32` via bindgen `.no_debug()` | Z1.3 | COMBINED §2.3.1 |
| REQ-Z1.5 | Remove `Debug` from `ForetiasSecretKeyVar` via bindgen `.no_debug()` | Z1.4 | COMBINED §2.3.1 |
| REQ-Z1.7 | Remove `Debug` from `ForetiasKemSecretKey` via bindgen `.no_debug()` | Z1.6 | COMBINED §2.3.1 |
| REQ-Z1.9 | Remove `Debug` from `ForetiasTbidV1SecretKey` via bindgen `.no_debug()` | Z1.7 | COMBINED §2.3.1 |
| REQ-Z1.11 | Remove `Debug` from `ForetiasNoiseState` via bindgen `.no_debug()` | Z1.8 | COMBINED §2.3.1 |
| REQ-Z2.3 | Update `build.rs` bindgen config (implements REQ-Z1.3, Z1.5, Z1.7, Z1.9, Z1.11) | Z2.3 | COMBINED §1.3.5 |
| REQ-Z2.5 | Wrap TBID secret `Vec<u8>` in `Zeroizing` | Z2.4 | COMBINED §1.3.11 |
| REQ-Z4.1 | Wrap `noise_static_priv` in `Zeroizing<[u8; 32]>` | Z4.1 | COMBINED §1.3.11 |

### 6.2 P1 — High (Fix Before v0.5)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z0.6 | Replace `foretias_memzero` with `sodium_memzero` | Z0.4 | This spec |
| REQ-Z0.7 | Remove `foretias_memzero` from public header | Z0.4 | This spec |
| REQ-Z2.7 | Zero C struct after ML-KEM keypair extraction | Z2.5 | This spec |
| REQ-Z4.3 | Bound `keypairs` Vec to 2 entries | Z4.2 | COMBINED §2.5 |
| REQ-Z4.4 | Audit `TbidSecret` for `Zeroizing` wrapping | Z4.3 | This spec |
| REQ-Z1.6 | Zero C struct after PQC key extraction (sphincs, dilithium) | Z1.4 | This spec |
| REQ-Z1.8 | Zero C struct after ML-KEM keypair extraction | Z1.6 | This spec |
| REQ-Z2.1 | Zero caller's buffer after `PrivKeyHandle::from_seed` | Z2.1 | This spec |

### 6.3 P2 — Medium (Fix Before v1.0)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z0.3 | Consider `sodium_mprotect` for KEK and tick key memory | Z0.1 | This spec |
| REQ-Z2.4 | Evaluate restricting `Copy` on secret binding types | Z2.3 | This spec |
| REQ-Z2.6 | Audit `SignatureBytes` for `Zeroizing` internal wrapping | Z2.4 | This spec |
| REQ-Z2.8 | Document `ed25519_sign` as unsafe/raw key path | Z2.6 | This spec |
| REQ-Z1.2 | Document KEK global state as design exemption | Z1.2 | This spec |

---

## Zone 7: Implementation Plan

This zone provides a concrete plan for bringing the codebase into compliance with the requirements above.

### 7.1 Phase 1: Bindgen Debug Removal (P0)

**Effort:** ~30 minutes
**Risk:** Low (only affects debug output, not runtime behavior)

1. Edit `p2p/core-engine/build.rs:93-99`:
   ```rust
   .derive_debug(true)
   .derive_copy(true)
   .no_debug("ForetiasPrivKey32")
   .no_debug("ForetiasSecretKeyVar")
   .no_debug("ForetiasKemSecretKey")
   .no_debug("ForetiasTbidV1SecretKey")
   .no_debug("ForetiasNoiseState")
   .no_debug("ForetiasFrostRound1")
   ```
2. Rebuild: `cd p2p && cargo build -p foretias-core`
3. Verify: `bindings.rs` no longer has `#[derive(Debug)]` on the listed types.
4. Check for compile errors where `Debug` was used on these types. Fix by removing `{:?}` formatting or adding manual `Debug` impls that emit `<redacted>`.
5. Run tests: `cd p2p && cargo test -p foretias-core`

### 7.2 Phase 2: Zeroizing Wrappers (P0)

**Effort:** ~2 hours
**Risk:** Low (type-level change, compiler-enforced)

1. **TBID Secret** (`signing_tbid.rs:32-43`):
   - Wrap `secret_bytes` in `Zeroizing::new(...)`.
   - Ensure `TbidSecret` in `types.rs` uses `Zeroizing<Vec<u8>>` internally.

2. **Noise Static Key** (`server/mod.rs:43`):
   - Change `noise_static_priv: [u8; 32]` to `noise_static_priv: Zeroizing<[u8; 32]>`.
   - Update all references to dereference: `&*self.noise_static_priv`.

3. **Rebuild and test:**
   ```bash
   cd p2p && cargo build --workspace
   cd p2p && cargo test --workspace
   ```

### 7.3 Phase 3: C Struct Zeroing After Extraction (P1)

**Effort:** ~1 hour
**Risk:** Low (adds zeroing calls, doesn't change behavior)

1. **ML-KEM keypair** (`kem_mlkem.rs:14-15`): Add `foretias_memzero` after extraction.
2. **SPHINCS+ keypair** (`signing_sphincs.rs`): Add `foretias_memzero` after extraction.
3. **Dilithium keypair** (`signing_dilithium.rs`): Add `foretias_memzero` after extraction.
4. **TBID keypair** (`signing_tbid.rs:41`): Already zeros C struct. Verify.

### 7.4 Phase 4: Replace `foretias_memzero` with `sodium_memzero` (P1)

**Effort:** ~1 hour
**Risk:** Low (functional equivalent, better platform support)

1. Edit `p2p/core/src/memzero.c`:
   ```c
   #include <sodium.h>
   void foretias_memzero(void* ptr, size_t len) {
       sodium_memzero(ptr, len);
   }
   ```
   Or better, replace all call sites with `sodium_memzero` directly and remove `memzero.c`.

2. Edit `p2p/core-engine/build.rs:61-71`: Remove `"src/memzero.c"` from sources list.

3. Edit `p2p/core/include/foretias_core.h:382`: Remove `foretias_memzero` declaration.

4. Replace all `foretias_memzero(` calls in C sources with `sodium_memzero(`.

5. Rebuild C11 core and Rust workspace.

### 7.5 Phase 5: Bound Keypairs Vec (P1)

**Effort:** ~30 minutes
**Risk:** Medium (changes tick key retention behavior)

1. Edit `chronomatter/mod.rs:generate_and_store_keypair()`:
   - After pushing new keypair, remove entries beyond index 1 (keep current + previous).
   - Add test: verify old keys are zeroed after eviction.

### 7.6 Phase 6: Documentation and Audit (P2)

**Effort:** ~1 hour
**Risk:** None

1. Add `TbidSecret` audit comment in `types.rs`.
2. Add doc comment to `ed25519_sign` marking it as the unsafe/raw path.
3. Document KEK global state exemption in `privkey.c` comments.
4. Update this spec with any findings from the audit.

---

## Appendix A: Secret Flow Diagrams

### A.1 Per-Tick Ed25519 Key Flow (Secure Path)

```
[OS Entropy]
    │
    ▼
randombytes_buf(seed, 32)          ← privkey.c:90
    │
    ▼
crypto_sign_seed_keypair(pub, sec, seed)  ← privkey.c:95
    │
    ├──► foretias_memzero(sec)     ← privkey.c:116 (64-byte derived key zeroed)
    │
    ▼
encrypt_seed(encrypted_key, nonce, seed)  ← privkey.c:107 (ChaCha20-Poly1305)
    │
    ├──► foretias_memzero(seed)    ← privkey.c:117 (32-byte seed zeroed)
    │
    ▼
ForetiasPrivKey { encrypted_key, nonce, public_key }  ← C heap
    │
    ▼ (FFI — opaque pointer only)
PrivKeyHandle(ManuallyDrop<NonNull<ForetiasPrivKey>>)  ← Rust
    │
    ▼ (on sign)
decrypt_seed(tmp, encrypted_key, nonce)  ← privkey.c:169
    │
    ▼
crypto_sign_seed_keypair + crypto_sign_detached  ← privkey.c:176-182
    │
    ├──► foretias_memzero(tmp)     ← privkey.c:191 (seed zeroed after use)
    │
    ▼ (on drop)
foretias_privkey_free() → foretias_memzero(entire struct) → free()  ← privkey.c:252-257
```

### A.2 TBID V1 Secret Flow (Current — Has Gap)

```
[C: Ed25519 keypair generation]
    │
    ▼
foretias_ed25519_generate_keypair()  ← signing_tbid.c:17
    │
    ▼
foretias_sphincs_sha2_256f_keypair()  ← signing_tbid.c:23
    │
    ▼
Copy into ForetiasTbidV1SecretKey  ← signing_tbid.c:30-35
    │
    ├──► foretias_memzero(temporary copies)  ← signing_tbid.c:37-38
    │
    ▼ (FFI — struct copied to Rust)
ForetiasTbidV1SecretKey (bindgen, Debug+Copy)  ← Rust
    │
    ▼
Extract to Vec<u8> (160 bytes)  ← signing_tbid.rs:32-34
    │
    ├──► foretias_tbid_v1_secret_zeroize(C struct)  ← signing_tbid.rs:41 (C side zeroed)
    │
    ▼
Vec<u8> returned as SignatureBytes  ← signing_tbid.rs:43
    │
    ▼
**GAP: Vec<u8> NOT wrapped in Zeroizing**  ← persists in heap
```

### A.3 Noise Static Key Flow (Current — Has Gap)

```
[C: Ed25519 keypair generation]
    │
    ▼
generate_ed25519_keypair()  ← server/mod.rs:78
    │
    ▼
priv_key.bytes → noise_static_priv: [u8; 32]  ← server/mod.rs:87
    │
    ▼
**GAP: Plain [u8; 32], no Zeroizing, no Drop**  ← persists for process lifetime
```

---

## Appendix B: Cross-Zone Secret Accessibility Matrix

Can code in Zone X access secrets from Zone Y?

| Secret | C11 Code | foretias-core | foretias-client | foretias-server | Python/Java |
|--------|----------|---------------|-----------------|-----------------|-------------|
| `ForetiasPrivKey` (handle) | Encrypted only | Opaque pointer | Opaque pointer | Opaque pointer | Opaque handle |
| KEK | File-scope static | No access | No access | No access | No access |
| `ForetiasPrivKey32` | Raw bytes | Raw bytes (via FFI) | Raw bytes | Raw bytes | Raw bytes (if exposed) |
| PQC secrets (C struct) | Raw bytes | Raw bytes (via FFI) | N/A | N/A | N/A |
| PQC secrets (Rust wrapper) | N/A | Zeroizing wrapper | N/A | N/A | N/A |
| TBID secret (Rust Vec) | N/A | **Plain Vec — GAP** | N/A | Via core-engine | Via core-engine |
| Noise static key | N/A | N/A | N/A | **Plain [u8;32] — GAP** | N/A |
| Tick keypairs | N/A | Opaque handles | N/A | Via core-engine | Via core-engine |

---

## Appendix C: Verification Checklist

After implementing all requirements, verify:

- [ ] `bindings.rs` has no `#[derive(Debug)]` on `ForetiasPrivKey32`, `ForetiasSecretKeyVar`, `ForetiasKemSecretKey`, `ForetiasTbidV1SecretKey`, `ForetiasNoiseState`, `ForetiasFrostRound1`
- [ ] `foretias_memzero` calls replaced with `sodium_memzero` (or `memzero.c` removed)
- [ ] `memzero.c` removed from `build.rs` sources list
- [ ] `foretias_memzero` declaration removed from `foretias_core.h`
- [ ] TBID secret in `signing_tbid.rs` wrapped in `Zeroizing`
- [ ] `noise_static_priv` in `TimeFamilyServer` wrapped in `Zeroizing<[u8; 32]>`
- [ ] C structs zeroed after PQC key extraction (ML-KEM, SPHINCS+, Dilithium)
- [ ] `keypairs` Vec bounded to 2 entries
- [ ] `TbidSecret` uses `Zeroizing<Vec<u8>>` internally
- [ ] `SignatureBytes` uses `Zeroizing<Vec<u8>>` internally
- [ ] `ed25519_sign` documented as unsafe/raw path
- [ ] All tests pass: `cargo test --workspace`
- [ ] No `{:?}` formatting on secret-holding types in production code

---

*End of specification.*
