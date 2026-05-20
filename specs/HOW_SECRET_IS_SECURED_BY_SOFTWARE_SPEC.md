# HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md

**Date:** 2026-05-20
**Application:** Foretias v0.3
**Scope:** Secret key lifecycle across all software layers — dependency libraries, C11 core, foretias-core, foretias-client, foretias-server, and future language bindings.
**Purpose:** Specify the secrecy guarantees for every secret key type in the system, zone by zone. Each zone documents what objects *can* and *cannot* be represented or accessed within that domain, with explicit language-level and practical secrecy statements.

---

## HARD REQUIREMENTS (Non-Negotiable)

These two requirements apply to **every secret key** in the system, without exception. Where a requirement cannot be met, the violation **must** be thoroughly documented with: (a) the exact reason it is infeasible, (b) the threat model impact, (c) compensating controls, and (d) the owning component (e.g., an external library outside our control).

### HR-1: ALL Original Secrets Must Be Encrypted In Memory

Every secret key that originates from key generation or external input **must be encrypted while stored in memory**. "In memory" means any persistent or semi-persistent storage: heap allocations, stack variables that survive beyond the immediate operation, struct fields, Vec buffers, global state.

**What counts as "encrypted":**
- AEAD encryption (ChaCha20-Poly1305 via libsodium `crypto_secretbox_*`) using the instance KEK
- The encrypted form must include a unique nonce per key (derived from a monotonic counter, as in `privkey.c`)
- The ciphertext + MAC must be stored, not the plaintext

**What does NOT count as "encrypted":**
- Raw bytes in a struct field (`uint8_t bytes[32]`)
- `Zeroizing<[u8; 32]>` (zeroing on drop is NOT encryption)
- `ManuallyDrop` wrappers (opaque access is NOT encryption)
- Library-internal encryption that we cannot verify or control

**Scope:** This applies to:
- Ed25519 seeds (tick keys, Noise static keys, TBID Ed25519 component)
- PQC secret keys (SPHINCS+, Dilithium3, ML-KEM)
- TBID combined secrets
- FROST shares
- Noise session keys (chaining key, send/recv keys)
- Seal keys
- Any future secret key types

**Current compliance:** Only `ForetiasPrivKey` (the opaque handle path in `privkey.c`) satisfies this requirement. All other secret types are **VIOLATIONS** that must be remediated or documented.

### HR-2: ALL Ephemeral Secrets Must Be Zeroed Immediately After Use

Every secret that exists only transiently — on the stack, as an intermediate value, or for a single operation — **must be zeroed before the function returns or the scope exits**. "Immediately" means before any other allocation, I/O, or control flow that could extend the exposure window.

**What counts as "ephemeral":**
- Stack-allocated buffers (`uint8_t tmp[32]`)
- Decrypted seeds used for a single signing operation
- DH intermediate values
- Ephemeral Diffie-Hellman private keys
- Temporary copies of secret material during extraction or conversion
- Noise ephemeral keys (generated per-handshake)

**What does NOT count as "ephemeral":**
- Long-lived keys stored in structs (covered by HR-1 instead)
- Keys persisted across ticks or connections

**Zeroing mechanism:**
- C11: `sodium_memzero()` (per REQ-Z0.6)
- Rust: `zeroize::Zeroizing` or explicit `fill(0)` before scope exit

**Current compliance:** Partially satisfied. Signing operations in `privkey.c` zero temporary buffers correctly. Noise DH intermediates are zeroed with `sodium_memzero`. However, several gaps exist (documented per-zone below).

### Violation Documentation Template

When a secret cannot satisfy HR-1 or HR-2, use this format:

```
**VIOLATION: HR-{1|2} — {Secret Name}**
- **Component:** {file:line or library name}
- **Reason:** {Why encryption/zeroing is infeasible}
- **Threat:** {What an attacker can do with the unencrypted/zeroed secret}
- **Compensating Controls:** {What mitigates the risk}
- **Owner:** {Foretias code / external library (name) / OS / hardware}
- **Remediation Path:** {How to eventually satisfy the requirement, or "N/A — external dependency"}
```

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

#### Practical Secrecy: **PRACTICAL-SECRET**
The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. The seed is encrypted with the instance KEK using ChaCha20-Poly1305. The KEK itself is obfuscated via the rts algorithm (see §1.2). Rust holds only an opaque pointer — no path exists to reconstruct the seed.

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

**Language Secrecy (Rust side):** The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. The `PrivKeyHandle` type does not implement `Deref`, `Debug`, `Clone`, or `Copy`.

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

#### Practical Secrecy: **PRACTICAL-SECRET (via rts obfuscation)**
The KEK is in process memory but is obfuscated using the **rts** (rotated-transposition-substitution) algorithm. The raw bytes of the KEK are not accessible from Rust, AND it is impractical to find the raw bytes and decode the KEK from Rust.

**The rts Algorithm (nuisance implementation — placeholder for higher security):**
The rts algorithm performs bitwise rot13 operations on the KEK material to ensure no secret is ever in memory for a long time in its raw form. The transformation proceeds in three passes:
1. **Left-ward rot13 on each word:** For each 32-bit word of the running system's KEK buffer, perform a left-ward (toward the biggest endian) rot13 bit rotation.
2. **Right-ward rot13 on each byte:** For each byte of the secret, perform a right-ward rot13 bit rotation.
3. **Left-ward rot13 on the entire message:** Perform a final left-ward (toward the biggest endian) rot13 rotation on the entire KEK buffer as a single message.

**This is a nuisance implementation.** The rts algorithm provides no real cryptographic security — rot13 is trivially reversible. It serves as a placeholder to establish the architectural pattern of "encrypt the KEK itself" that will be replaced by a proper key derivation function (e.g., HKDF from a hardware-bound secret, or a password-derived key) in a future release. The immediate benefit is that no secret is ever in memory for a long time in its raw, directly usable form.

**Specified Requirement (REQ-Z1.2):** The KEK global state violates `FORETIAS_1_MVP_SPEC §4.1` ("no global state in core"). This is acknowledged as a **design exemption** with the following rationale:
- The KEK is process-scoped, obfuscated via rts, and zeroed on shutdown.
- Making it caller-owned would require passing a context pointer through every FFI call, breaking the opaque handle design.
- The alternative (per-handle KEK) would defeat the purpose — each handle would need its own decryption key, requiring storage of that key somewhere.
- **Mitigation:** Consider `sodium_mprotect` (REQ-Z0.3) to prevent the KEK from being swapped to disk.
- **Future:** Replace rts with a proper key derivation mechanism (REQ-Z1.2A).

**Specified Requirement (REQ-Z1.2A):** Replace the rts nuisance implementation with a proper KEK derivation mechanism. Candidate approaches: HKDF from a hardware-bound secret (TPM/sealed key), Argon2-derived from a user password, or a hybrid approach. **This is a P1 requirement.**

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

#### Language Secrecy: **LANGUAGE-ACCESSIBLE (in C)**
The struct is a plain C struct with public fields. Any C code can read `priv.bytes[i]`.

#### Practical Secrecy: **PRACTICAL-ACCESSIBLE (in C)**
Raw seed bytes are directly accessible within C. No encryption layer.

**After HR-1 remediation (REQ-Z1.3A):** The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. Consumers migrate to the opaque `ForetiasPrivKey` handle or C-internal derivation.

#### Usage
Used by `signing_ed25519.c:5-27` for signing, `nullifier.c:5-20` for nullifier derivation, `noise_xx.c:112-166` for Noise handshake initialization.

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

**VIOLATION: HR-1 — ForetiasPrivKey32 (Ed25519 Raw Seed)**
- **Component:** `identity_ed25519.c:5-27`, `foretias_core.h:95`
- **Reason:** `ForetiasPrivKey32` stores the Ed25519 seed as raw `bytes[32]` with no encryption. This type was designed for ephemeral/transient keys (Noise handshake, TBID composition, per-connection keys) where the overhead of KEK-based encryption was deemed unnecessary for short-lived material. However, several consumers store these keys long-term (e.g., `TimeFamilyServer::noise_static_priv` persists for the process lifetime).
- **Threat:** Any code path that accesses `ForetiasPrivKey32` can read the raw seed. A memory dump, core dump, or debugger reveals the seed directly. The `Debug + Copy + Clone` derives on the Rust binding make accidental logging trivial.
- **Compensating Controls:** `noise.rs:95` zeros the Rust-side copy after the Noise handshake. The C11 `foretias_memzero` zeros stack buffers after signing operations.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Migrate all `ForetiasPrivKey32` consumers to use the opaque `ForetiasPrivKey` handle (see REQ-Z1.3A). For truly ephemeral keys (per-connection Noise), derive the X25519 scalar inside C and pass only the derived scalar across FFI, never the raw Ed25519 seed.

**Specified Requirement (REQ-Z1.3A):** Migrate `ForetiasPrivKey32` consumers to the encrypted path. For long-lived keys (Noise static, TBID), use the `ForetiasPrivKey` opaque handle. For ephemeral keys (per-connection Noise), derive X25519 material inside C and pass only the derived scalar. **This is a P0 requirement.**

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
Raw secret bytes are directly accessible within C. No encryption.

**After HR-1 remediation (REQ-Z1.5A):** The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. PQC secrets are encrypted with the instance KEK before crossing the FFI boundary.

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

**VIOLATION: HR-1 — SPHINCS+ Secret Key (ForetiasSecretKeyVar)**
- **Component:** `signing_sphincs.c:7-46`, `foretias_core.h:115-118`
- **Reason:** SPHINCS+ secret keys (96 bytes for SHA2-128s, 128 bytes for SHA2-256f) are stored as raw bytes in `ForetiasSecretKeyVar.bytes[4096]` with no encryption. The KEK-based encryption model in `privkey.c` was designed for 32-byte Ed25519 seeds; extending it to 4096-byte PQC keys requires a different approach (larger ciphertext buffers, different nonce derivation).
- **Threat:** Raw PQC secret bytes are directly accessible in memory. A process memory dump reveals the complete secret key. The `Debug + Copy + Clone` derives on the Rust binding make accidental logging trivial.
- **Compensating Controls:** Rust wrappers in `software.rs` wrap extracted bytes in `Zeroizing<SignatureBytes>`. C-side zeroing on error paths via `OQS_MEM_cleanse`.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt PQC secret keys using the instance KEK with a larger ciphertext buffer. The `ForetiasSecretKeyVar` struct should store `encrypted_bytes[4112]` (4096 + 16 MAC) instead of raw bytes. See REQ-Z1.5A.

**VIOLATION: HR-1 — Dilithium3 Secret Key (ForetiasSecretKeyVar)**
- **Component:** `signing_dilithium.c:7-46`, `foretias_core.h:115-118`
- **Reason:** Same as SPHINCS+ — 4000-byte Dilithium3 secret stored as raw bytes with no encryption.
- **Threat:** Same as SPHINCS+.
- **Compensating Controls:** Same as SPHINCS+.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Same as SPHINCS+ — see REQ-Z1.5A.

**Specified Requirement (REQ-Z1.5A):** Extend the KEK-based encryption model to cover PQC secret keys. The `ForetiasSecretKeyVar` struct should store encrypted bytes: `uint8_t encrypted_bytes[4112]` (4096 plaintext + 16 MAC) with a unique nonce per key. All PQC key generation functions (`foretias_sphincs_*_keypair`, `foretias_dilithium3_keypair`) must encrypt the secret before returning. All PQC signing functions must decrypt before use and zero the decrypted buffer immediately after. **This is a P0 requirement.**

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

**VIOLATION: HR-1 — ML-KEM-768 Secret Key (ForetiasKemSecretKey)**
- **Component:** `kem_mlkem.c:7-25`, `foretias_core.h:130-133`
- **Reason:** ML-KEM-768 secret key (1184 bytes) stored as raw bytes in `ForetiasKemSecretKey.bytes[2400]` with no encryption. Same structural issue as PQC signature keys.
- **Threat:** Raw ML-KEM secret bytes directly accessible. Process memory dump reveals the complete key.
- **Compensating Controls:** Rust wrappers wrap extracted bytes in `Zeroizing<SignatureBytes>`. C-side zeroing on error paths via `OQS_MEM_cleanse`.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt ML-KEM secret keys using the instance KEK. The `ForetiasKemSecretKey` struct should store `encrypted_bytes[2416]` (2400 + 16 MAC). See REQ-Z1.7A.

**VIOLATION: HR-1 — ML-KEM Shared Secret**
- **Component:** `kem_mlkem.c:27-67`
- **Reason:** The 32-byte shared secret produced by encapsulation/decapsulation is written to a caller-provided buffer with no encryption. This is an ephemeral value (used for key derivation), so HR-2 applies more directly.
- **Threat:** Shared secret exists in plaintext until the caller uses it for key derivation.
- **Compensating Controls:** Error-path zeroing via `OQS_MEM_cleanse`. Caller is expected to use the shared secret immediately for key derivation.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Zero the shared secret buffer immediately after the caller extracts it (HR-2 compliance). See REQ-Z1.8A.

**Specified Requirement (REQ-Z1.7A):** Extend the KEK-based encryption model to cover ML-KEM secret keys. The `ForetiasKemSecretKey` struct should store encrypted bytes: `uint8_t encrypted_bytes[2416]` (2400 plaintext + 16 MAC). All ML-KEM key generation functions must encrypt before returning. All decapsulation functions must decrypt before use and zero immediately after. **This is a P0 requirement.**

**Specified Requirement (REQ-Z1.7):** In `build.rs:99`, add `.no_debug("ForetiasKemSecretKey")` to the bindgen builder.

**Specified Requirement (REQ-Z1.8):** In `kem_mlkem.rs:15`, after extracting `secret.bytes[..secret.len]`, zero the C struct: `unsafe { foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasKemSecretKey>()) }`. Same pattern as `kem_mlkem.rs:53-55` (already done for decapsulate).

**Specified Requirement (REQ-Z1.8A):** In `kem_mlkem.c:27-47` (encapsulate) and `kem_mlkem.c:49-67` (decapsulate), zero the `shared_secret_out` buffer on success immediately after writing, and document that the caller must copy the shared secret before the buffer is zeroed. Alternatively, have the caller provide a callback that receives the shared secret and performs key derivation inline, so the secret never sits in an unencrypted buffer. **This is a P0 requirement (HR-2 compliance).**

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

**VIOLATION: HR-1 — TBID V1 Secret Key (ForetiasTbidV1SecretKey)**
- **Component:** `signing_tbid.c:7-41`, `foretias_core.h:146-150`
- **Reason:** TBID V1 combines an Ed25519 seed (32 bytes) and an SLH-DSA secret (128 bytes) into a single struct with no encryption. The Ed25519 component could theoretically use the opaque `ForetiasPrivKey` handle, but the combined struct design requires both components to coexist in a single FFI type.
- **Threat:** Raw 160-byte combined secret directly accessible. The Ed25519 component alone is sufficient for certain forgery attacks. The SLH-DSA component is a one-time-use key — exposure compromises the TBID entirely.
- **Compensating Controls:** C-side zeroing after keypair generation (`signing_tbid.c:37-38`) and after signing (`signing_tbid.c:73`). Rust-side `foretias_tbid_v1_secret_zeroize` call.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt both components of the TBID secret using the instance KEK. The `ForetiasTbidV1SecretKey` struct should store `encrypted_ed25519[48]` (32 + 16 MAC) and `encrypted_slh_dsa[144]` (128 + 16 MAC) with unique nonces. See REQ-Z1.9A.

**VIOLATION: HR-2 — TBID Secret Extraction (Rust Vec<u8>)**
- **Component:** `signing_tbid.rs:32-43`
- **Reason:** The 160-byte TBID secret is extracted to a plain `Vec<u8>` which is NOT zeroed. The C struct IS zeroed, but the Rust heap copy persists.
- **Threat:** TBID secret persists in Rust heap memory until the `Vec` is dropped (which does NOT zero the memory).
- **Compensating Controls:** None currently.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Wrap `secret_bytes` in `Zeroizing::new(...)` (REQ-Z2.5). **This is a P0 requirement.**

**Specified Requirement (REQ-Z1.9A):** Extend the KEK-based encryption model to cover TBID V1 secrets. The `ForetiasTbidV1SecretKey` struct should store encrypted components: `uint8_t encrypted_ed25519[48]` and `uint8_t encrypted_slh_dsa[144]` with unique nonces. All TBID key generation and signing functions must encrypt/decrypt through the KEK. **This is a P0 requirement.**

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

#### Practical Secrecy: **PRACTICAL-ACCESSIBLE (in C)**
Raw session keys are directly accessible within C. No encryption.

**After HR-1 remediation (REQ-Z1.11A):** The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. Noise session state is encrypted with the instance KEK.

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

**VIOLATION: HR-1 — Noise Session State (ForetiasNoiseState)**
- **Component:** `noise_xx.c:112-166`, `foretias_core.h:255-272`
- **Reason:** The `ForetiasNoiseState` struct stores `local_static_priv[32]`, `send_key[32]`, `recv_key[32]`, `chaining_key[32]` in plaintext with no encryption. These are long-lived session keys that persist for the duration of a PtP connection.
- **Threat:** Raw session keys directly accessible in memory. A process dump reveals all Noise session material, allowing decryption of all traffic on the connection.
- **Compensating Controls:** `NoiseSession` RAII wrapper zeros the entire struct on drop via `foretias_noise_destroy()`. DH intermediates are zeroed immediately after use with `sodium_memzero`.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt Noise session keys using the instance KEK. The `ForetiasNoiseState` struct should store encrypted forms of `local_static_priv`, `send_key`, `recv_key`, and `chaining_key` with unique nonces. See REQ-Z1.11A.

**VIOLATION: HR-2 — Noise Ephemeral Keys (C11)**
- **Component:** `noise_xx.c:199-214, :333-344`
- **Reason:** Ephemeral private keys are generated on the stack and zeroed with `sodium_memzero` after deriving the public key. This is correct — HR-2 is satisfied for these ephemeral keys.
- **Status:** **COMPLIANT** — ephemeral keys are zeroed immediately after use.

**VIOLATION: HR-2 — Noise Static Key Input (Rust side)**
- **Component:** `noise.rs:68-95`
- **Reason:** The Rust-side `static_priv: &[u8; 32]` is copied into a `ForetiasPrivKey32` struct and zeroed at line 95 (`priv_key.bytes.fill(0)`). However, the original `static_priv` slice passed by the caller is NOT zeroed — the caller retains the original copy.
- **Threat:** The caller's original `static_priv` buffer persists after the handshake.
- **Compensating Controls:** `noise.rs:95` zeros the internal copy. Caller is expected to manage its own buffers.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Document that callers must zero their `static_priv` buffer after use. Consider accepting `Zeroizing<[u8; 32]>` as input to enforce this. See REQ-Z1.11B.

**Specified Requirement (REQ-Z1.11A):** Extend the KEK-based encryption model to cover Noise session state. The `ForetiasNoiseState` struct should store encrypted forms of session keys with unique nonces. All Noise operations must decrypt/encrypt through the KEK. **This is a P0 requirement.**

**Specified Requirement (REQ-Z1.11B):** Document that callers of `NoiseSession::new()` must zero their `static_priv` buffer after the session is created. Consider accepting `Zeroizing<[u8; 32]>` as input to enforce zeroing on the caller side. **This is a P1 requirement.**

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

**VIOLATION: HR-1 — FROST Round 1 State**
- **Component:** `frost_ed25519.c`, `foretias_core.h:337-342`
- **Reason:** FROST nonces (`nonce_d`, `nonce_e`) are stored as raw bytes with no encryption. FROST is currently stubbed (returns `FORETIAS_ERR_UNSUPPORTED`), so no secrets are actually generated yet.
- **Threat:** When FROST is implemented, nonces will be exposed in plaintext. FROST nonces are critical — reuse of a nonce compromises the long-term signing key.
- **Compensating Controls:** `foretias_frost_destroy_round1` zeros the struct on destruction.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** When FROST is implemented, encrypt nonces using the instance KEK. See REQ-Z1.12A.

**Specified Requirement (REQ-Z1.12A):** When FROST is implemented, the `ForetiasFrostRound1` struct must store encrypted nonces using the instance KEK. The struct should use `encrypted_nonce_d[48]` and `encrypted_nonce_e[48]` with unique nonces. **This is a P0 requirement (for when FROST becomes active).**

**Specified Requirement (REQ-Z1.12):** FROST is currently stubbed in the Rust layer. When implemented, ensure the `ForetiasFrostRound1` struct is zeroed after use. The C-side `foretias_frost_destroy_round1` already provides this.

### 1.10 Summary Table: C11 Zone

| Secret | Language Secrecy (C) | Practical Secrecy (C) | Encrypted? | Zeroed on Destruction? | Bindgen Debug? | HR-1 | HR-2 |
|--------|---------------------|----------------------|------------|----------------------|----------------|------|------|
| `ForetiasPrivKey` (handle) | SECRET | SECRET | Yes (KEK+AEAD) | Yes (`privkey_free`) | N/A (opaque) | ✅ PASS | ✅ PASS |
| KEK | SECRET (file-scope) | ACCESSIBLE | N/A | Yes (`privkey_cleanup`) | N/A | ⚠️ N/A (it IS the key) | N/A |
| `ForetiasPrivKey32` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** | ❌ VIOLATION | ❌ VIOLATION |
| `ForetiasSecretKeyVar` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** | ❌ VIOLATION | ❌ VIOLATION |
| `ForetiasKemSecretKey` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — MED** | ❌ VIOLATION | ❌ VIOLATION |
| `ForetiasTbidV1SecretKey` | ACCESSIBLE | ACCESSIBLE | No | Caller's responsibility | **YES — CRIT** | ❌ VIOLATION | ❌ VIOLATION |
| `ForetiasNoiseState` | ACCESSIBLE | ACCESSIBLE | No | Yes (`noise_destroy`) | **YES — HIGH** | ❌ VIOLATION | ✅ PASS (DH intermediates) |
| `ForetiasFrostRound1` | ACCESSIBLE | ACCESSIBLE | No | Yes (`frost_destroy`) | N/A (not used) | ❌ VIOLATION | N/A (stubbed) |

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
The raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. The seed is encrypted with the instance KEK (obfuscated via rts) using ChaCha20-Poly1305. Rust holds only an opaque pointer — no path exists to reconstruct the seed.

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

| Field | Type | Secret? | Secured By | HR-1 | HR-2 |
|-------|------|---------|------------|------|------|
| `priv_key` | `PrivKeyHandle` | Yes | Opaque handle (Zone 2.1) | ✅ PASS | ✅ PASS |
| `seal_key` | `Zeroizing<[u8; 32]>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |
| `frost_shares` | `Mutex<HashMap<String, Zeroizing<Vec<u8>>>>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |
| `sphincs_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |
| `dilithium_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |
| `sphincs_sha2_256f_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |
| `mlkem_secret_key` | `Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` on drop | ❌ VIOLATION | N/A |

**VIOLATION: HR-1 — Seal Key (Zeroizing<[u8; 32]>)**
- **Component:** `software.rs:34`
- **Reason:** `Zeroizing<[u8; 32]>` provides zeroing on drop, but NOT encryption. The seal key is stored as raw bytes in memory.
- **Threat:** Process memory dump reveals the seal key in plaintext.
- **Compensating Controls:** `Zeroizing` ensures the key is zeroed on drop. `Drop` impl explicitly calls `fill(0)`.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Derive the seal key on-demand from the `PrivKeyHandle` (via `derive_seal_key()`) instead of storing it. This eliminates the need for persistent storage. See REQ-Z2.2A.

**VIOLATION: HR-1 — PQC Secret Keys (Zeroizing<SignatureBytes>)**
- **Component:** `software.rs:38-46`
- **Reason:** `Zeroizing<SignatureBytes>` provides zeroing on drop, but NOT encryption. PQC secret keys are stored as raw bytes in memory.
- **Threat:** Process memory dump reveals PQC secret keys in plaintext.
- **Compensating Controls:** `Zeroizing` ensures keys are zeroed on drop.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt PQC secrets using the instance KEK at the C11 level (REQ-Z1.5A, REQ-Z1.7A). The Rust wrapper receives encrypted material and decrypts on-demand. See REQ-Z2.2B.

**VIOLATION: HR-1 — FROST Shares (Zeroizing<Vec<u8>>)**
- **Component:** `software.rs:36`
- **Reason:** `Zeroizing<Vec<u8>>` provides zeroing on drop, but NOT encryption. FROST shares are stored as raw bytes in memory.
- **Threat:** Process memory dump reveals FROST shares in plaintext.
- **Compensating Controls:** `Zeroizing` ensures shares are zeroed on drop.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Encrypt FROST shares using the instance KEK. See REQ-Z2.2C.

**Language Secrecy:** All secret fields are secured by either `PrivKeyHandle` (opaque) or `Zeroizing` (auto-zero on drop). For `PrivKeyHandle` fields: the raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust.

**Practical Secrecy:** For `PrivKeyHandle` fields: the raw bytes of the key are not accessible from Rust, AND it is impractical to find the raw bytes and KEK to decode this key from Rust. For `Zeroizing` fields: secrets are in process memory but zeroed on scope exit. After HR-1 remediation (REQ-Z2.2A/B/C), all fields will satisfy the same guarantee as `PrivKeyHandle`.

**Specified Requirement (REQ-Z2.2A):** Instead of storing `seal_key: Zeroizing<[u8; 32]>` as a persistent field, derive it on-demand from `self.priv_key.derive_seal_key(info)` when needed. This eliminates the storage violation. If caching is required for performance, encrypt the cached value using the instance KEK. **This is a P0 requirement.**

**Specified Requirement (REQ-Z2.2B):** PQC secret keys stored in `SoftwareCryptoServer` must be encrypted using the instance KEK. The C11 layer (REQ-Z1.5A, REQ-Z1.7A) must provide encrypted forms, and the Rust wrapper must decrypt on-demand for signing operations. **This is a P0 requirement.**

**Specified Requirement (REQ-Z2.2C):** FROST shares stored in `SoftwareCryptoServer` must be encrypted using the instance KEK. Each share should be encrypted with a unique nonce derived from the peer ID. **This is a P0 requirement.**

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

**VIOLATION: HR-1 — Ephemeral Ed25519 Key (noise_ptp)**
- **Component:** `noise_ptp.rs:66-69`
- **Reason:** `generate_ed25519_keypair()` returns a `ForetiasPrivKey32` with raw, unencrypted bytes. The ephemeral key is generated, used for the Noise handshake, and then the Rust-side copy is zeroed at `noise.rs:95`. However, the key is NOT encrypted at any point during its lifetime.
- **Threat:** Between generation and zeroing, the raw seed exists in plaintext. A memory dump during this window reveals the ephemeral key.
- **Compensating Controls:** `noise.rs:95` zeros the Rust-side copy after the handshake. The `NoiseSession` RAII wrapper zeros the C-side state on drop.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** For ephemeral keys, the priority is HR-2 (immediate zeroing) over HR-1 (encryption). The key is zeroed at `noise.rs:95`, which satisfies HR-2. For HR-1 compliance, generate the ephemeral keypair inside C (via `foretias_noise_init_ed25519`) and never expose the raw seed to Rust. See REQ-Z3.1A.

**VIOLATION: HR-2 — Ephemeral Ed25519 Key (noise_ptp)**
- **Component:** `noise_ptp.rs:66-69`
- **Reason:** The ephemeral key is zeroed at `noise.rs:95` (inside `NoiseSession::new()`), but the `_pub_key` (public key) is NOT zeroed — though this is public material so it's not a concern. The `priv_key.bytes` is zeroed after the FFI call, which is the correct behavior.
- **Status:** **PASS** — the ephemeral key IS zeroed immediately after use (at `noise.rs:95`).

**Specified Requirement (REQ-Z3.1A):** For ephemeral Noise keys, generate the keypair entirely inside C. The `noise_handshake` function should accept no raw seed from Rust — instead, call `foretias_ed25519_generate_keypair()` internally within `foretias_noise_init_ed25519()`. This eliminates the need to pass raw bytes across the FFI boundary. **This is a P0 requirement.**

**Specified Requirement (REQ-Z3.1):** The current pattern is acceptable for ephemeral keys. The key is zeroed after use. However, the `Debug` derive on `ForetiasPrivKey32` (REQ-Z1.3) means any logging between generation and zeroing would leak the key. Fixing REQ-Z1.3 closes this gap.

### 3.2 Summary Table: foretias-client Zone

| Secret | Rust Type | Language Secrecy | Practical Secrecy | HR-1 | HR-2 |
|--------|-----------|-----------------|-------------------|------|------|
| Ephemeral Ed25519 key | `ForetiasPrivKey32` | ACCESSIBLE | ACCESSIBLE | ❌ VIOLATION | ✅ PASS (noise.rs:95) |
| Noise session keys | `NoiseSession` (opaque) | SECRET | SECRET | ❌ VIOLATION (C11 plaintext) | ✅ PASS (DH intermediates) |

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

**VIOLATION: HR-1 — Noise Static Private Key (TimeFamilyServer)**
- **Component:** `server/mod.rs:43`
- **Reason:** `noise_static_priv: [u8; 32]` is a plain array with no encryption. This is the server's long-term identity key for Noise PtP handshakes — it persists for the entire process lifetime.
- **Threat:** Process memory dump reveals the Noise static private key. An attacker with this key can impersonate the server on PtP connections and decrypt all traffic.
- **Compensating Controls:** None currently. `Zeroizing` wrapping (REQ-Z4.1) provides zeroing on drop, but NOT encryption.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Use the `PrivKeyHandle` opaque path for the Noise static key. Generate the key via `PrivKeyHandle::generate()` and derive the X25519 scalar inside C. Store only the opaque handle in `TimeFamilyServer`. See REQ-Z4.1A.

**VIOLATION: HR-2 — Noise Static Key Input (TimeFamilyServer)**
- **Component:** `server/mod.rs:78-87, :105-114`
- **Reason:** `generate_ed25519_keypair()` returns a `ForetiasPrivKey32` whose bytes are copied into `noise_static_priv`. The original `priv_key` from `generate_ed25519_keypair()` is NOT zeroed after the copy — it goes out of scope, but the stack memory is not explicitly zeroed.
- **Threat:** The stack slot for `priv_key` retains the raw seed until overwritten by subsequent allocations.
- **Compensating Controls:** None currently.
- **Owner:** Foretias code — within our control.
- **Remediation Path:** Zero `priv_key.bytes` immediately after copying to `noise_static_priv`. See REQ-Z4.1B.

**Specified Requirement (REQ-Z4.1A):** Replace `noise_static_priv: [u8; 32]` with `noise_static_priv: PrivKeyHandle`. Generate the key via `PrivKeyHandle::generate()` and derive the X25519 scalar for Noise handshakes inside C (via a new C11 function that takes a `ForetiasPrivKey` handle and outputs the X25519 public key). This satisfies HR-1 (encryption at rest) and HR-2 (zeroing on drop). **This is a P0 requirement.**

**Specified Requirement (REQ-Z4.1B):** If REQ-Z4.1A is not yet implemented, zero `priv_key.bytes` immediately after copying to `noise_static_priv`:
```rust
let (pub_key, mut priv_key) = generate_ed25519_keypair()?;
let noise_static_priv = priv_key.bytes;
priv_key.bytes.fill(0);  // Zero immediately after copy
```
**This is a P0 requirement (HR-2 compliance).**

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

| Secret | Rust Type | Language Secrecy | Practical Secrecy | HR-1 | HR-2 |
|--------|-----------|-----------------|-------------------|------|------|
| Noise static private key | `[u8; 32]` (plain) | ACCESSIBLE | ACCESSIBLE | ❌ VIOLATION | ❌ VIOLATION |
| Tick keypairs | `Vec<TickKeyPair>` | SECRET (handle) | SECRET (handle) | ✅ PASS | ✅ PASS (but unbounded Vec) |
| TBID secret | `Option<TbidSecret>` | Depends on TbidSecret | Depends on TbidSecret | ❌ VIOLATION (pending audit) | ❌ VIOLATION (pending audit) |

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

## Zone 6: Formal Analysis of Secret Usage

The encryption and opaque handle mechanisms described above protect secrets at rest and in transit across the FFI boundary. However, they do not protect against **misuse** of the cryptographic APIs. Rust code that holds an opaque `PrivKeyHandle` pointer could still construct arbitrary calls like:

```rust
// Hypothetical misuse — anyone can write this:
unsafe { foretias_privkey_ed25519_sign(handle.as_ptr(), bad_message.as_ptr(), bad_message.len(), sig.as_mut_ptr()) };
```

The type system prevents reading the raw key bytes, but it does not prevent invoking signing operations with arbitrary data. This zone specifies the requirements for a formal analysis system that tracks all paths by which data can reach the cryptographic operations that consume secrets.

### 6.1 Problem Statement

The core concern is **information flow control**: ensuring that only authorized data flows reach the functions that consume secret keys. Without this guarantee, an attacker who can inject arbitrary data into the system could:

1. **Forge attestations** — sign arbitrary messages using the tick keypair.
2. **Create fake Foretis stamps** — produce valid-looking timestamps for arbitrary content.
3. **Compromise mutual attestation** — sign malicious attestations for peer calendars.
4. **Derive unauthorized keys** — call `derive_seal_key` or nullifier derivation with attacker-controlled context strings.

The existing defenses (opaque handles, KEK encryption, zeroing) prevent the attacker from **reading** the secret. But they do not prevent the attacker from **using** the secret if they can control the input data to the signing functions.

### 6.2 Scope of Formal Analysis

The formal analysis must cover all functions that consume secret material:

| Function | Secret Consumed | Input Data | Risk if Input is Controlled |
|----------|----------------|------------|----------------------------|
| `foretias_privkey_ed25519_sign` | Tick Ed25519 seed | `msg`, `msg_len` | Forged attestation |
| `foretias_nullifier_derive_handle` | Tick Ed25519 seed | `context`, `context_len` | Unauthorized nullifier |
| `foretias_privkey_derive_seal_key` | Tick Ed25519 seed | `info`, `info_len` | Seal key compromise |
| `foretias_ed25519_sign` | Raw Ed25519 seed | `msg`, `msg_len` | Forged signature |
| `foretias_tbid_v1_sign` | TBID combined secret | `msg`, `msg_len` | Forged TBID attestation |
| `foretias_sphincs_*_sign` | SPHINCS+ secret | `msg`, `msg_len` | Forged PQC signature |
| `foretias_dilithium3_sign` | Dilithium3 secret | `msg`, `msg_len` | Forged PQC signature |
| `foretias_mlkem_768_encapsulate` | ML-KEM secret | `public_key` | KEM abuse |
| `foretias_mlkem_768_decapsulate` | ML-KEM secret | `ciphertext` | Shared secret extraction |
| `foretias_noise_handshake` | Noise session keys | Remote handshake data | Protocol manipulation |

### 6.3 Analysis Requirements

**Specified Requirement (REQ-Z6.1):** Conduct a **taint analysis** of all data flows that reach the functions listed in §6.2. The analysis must:
1. Identify all entry points where external data enters the system (network messages, file reads, CLI arguments, FFI inputs from bindings).
2. Track the propagation of tainted data through the codebase to the signing functions.
3. Identify all sanitization points where tainted data is validated, hashed, or otherwise transformed before reaching a signing function.
4. Flag any path where tainted data can reach a signing function without passing through a verified sanitization point.

**Specified Requirement (REQ-Z6.2):** Conduct an **information flow analysis** that classifies each variable and parameter as either:
- **Trusted:** Data generated internally by the system (e.g., calendar hashes, tick numbers, peer IDs from verified attestations).
- **Untrusted:** Data received from external sources (network messages, files, user input).
- **Derived:** Data computed from a mix of trusted and untrusted inputs.

The analysis must ensure that only Trusted or properly sanitized Derived data reaches the signing functions.

**Specified Requirement (REQ-Z6.3):** Document all **sanitization points** — functions or operations that transform Untrusted data into Trusted data. Examples:
- Hashing an untrusted message before signing (the hash is trusted because it's a deterministic transformation).
- Validating a peer ID against the known peer list before using it in attestation.
- Verifying a remote calendar entry before incorporating it into the local calendar.

Each sanitization point must be documented with:
- The function or code location.
- The input classification (Untrusted).
- The output classification (Trusted).
- The transformation that justifies the reclassification.

**Specified Requirement (REQ-Z6.4):** For each signing function, document the **expected input contract** — what data is expected to be signed, and how the caller is expected to construct that data. Example:
```
foretias_privkey_ed25519_sign(handle, msg, msg_len):
  Expected: msg = SHA-256(calendar_entry || foretis_content)
  Contract: The caller must hash the calendar entry and content before signing.
  Violation: Signing raw content without hashing would produce a forgeable stamp.
```

**Specified Requirement (REQ-Z6.5):** The formal analysis must be **repeated** after any significant code change that:
- Adds new signing operations.
- Modifies existing signing paths.
- Introduces new external data entry points.
- Changes the sanitization pipeline.

### 6.4 Methodology

The formal analysis should employ a combination of:

1. **Static analysis:** Use tools like `cargo-miri`, `cargo-llvm-cov`, or custom taint-tracking passes to trace data flows through the Rust codebase.
2. **Manual review:** Expert review of the C11 code paths, particularly the FFI boundaries where static analysis tools may not be effective.
3. **Test coverage:** Ensure that the test suite covers all signing paths with both valid and invalid inputs, to catch misuse patterns at runtime.
4. **Documentation:** Maintain a living document that maps each signing function to its expected input contract and sanitization pipeline.

### 6.5 Deliverables

The formal analysis must produce:

1. **Taint flow diagram:** A visual representation of all data flows from external entry points to signing functions, with sanitization points marked.
2. **Violation report:** A list of any paths where untrusted data can reach a signing function without sanitization. Each violation must be classified as P0 (immediate fix required), P1 (fix before release), or P2 (acceptable risk).
3. **Input contract documentation:** For each signing function, a documented contract specifying the expected input format and the caller's responsibilities.
4. **Sanitization registry:** A catalog of all sanitization points, their input/output classifications, and the transformations they perform.

**Specified Requirement (REQ-Z6.6):** The formal analysis deliverables must be reviewed and approved before any public release of Foretias. Any P0 violations must be resolved before the analysis is considered complete.

---

## Zone 7: Compliance Requirements

This zone consolidates all specified requirements from the COMBINED review and the HR-1/HR-2 mandate, organized by priority.

### 6.0 HR Violation Summary

| Secret | HR-1 (Encrypted) | HR-2 (Zeroed) | Status |
|--------|------------------|----------------|--------|
| `ForetiasPrivKey` (handle) | ✅ PASS | ✅ PASS | Compliant |
| KEK | ⚠️ N/A (it IS the key) | N/A | Exempt |
| `ForetiasPrivKey32` | ❌ VIOLATION | ❌ VIOLATION | REQ-Z1.3A, REQ-Z1.4 |
| `ForetiasSecretKeyVar` (SPHINCS+, Dilithium) | ❌ VIOLATION | ❌ VIOLATION | REQ-Z1.5A, REQ-Z1.6 |
| `ForetiasKemSecretKey` (ML-KEM) | ❌ VIOLATION | ❌ VIOLATION | REQ-Z1.7A, REQ-Z1.8, REQ-Z1.8A |
| `ForetiasTbidV1SecretKey` | ❌ VIOLATION | ❌ VIOLATION | REQ-Z1.9A, REQ-Z1.10 |
| `ForetiasNoiseState` | ❌ VIOLATION | ✅ PASS (DH intermediates) | REQ-Z1.11A, REQ-Z1.11B |
| `ForetiasFrostRound1` | ❌ VIOLATION | N/A (stubbed) | REQ-Z1.12A |
| Seal key (`SoftwareCryptoServer`) | ❌ VIOLATION | N/A | REQ-Z2.2A |
| PQC secrets (`SoftwareCryptoServer`) | ❌ VIOLATION | N/A | REQ-Z2.2B |
| FROST shares (`SoftwareCryptoServer`) | ❌ VIOLATION | N/A | REQ-Z2.2C |
| Ephemeral Noise key (`noise_ptp`) | ❌ VIOLATION | ✅ PASS | REQ-Z3.1A |
| Noise static key (`TimeFamilyServer`) | ❌ VIOLATION | ❌ VIOLATION | REQ-Z4.1A, REQ-Z4.1B |
| TBID secret (`Chronomatter`) | ❌ VIOLATION | ❌ VIOLATION | REQ-Z4.4 |

### 6.1 P0 — Critical (Must Fix Before Public Release)

#### HR-1: Encryption Mandate

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z1.3A | Migrate `ForetiasPrivKey32` consumers to encrypted path (opaque handle or C-internal derivation) | Z1.3 | HR-1 |
| REQ-Z1.5A | Encrypt PQC secret keys (SPHINCS+, Dilithium) using instance KEK | Z1.4 | HR-1 |
| REQ-Z1.7A | Encrypt ML-KEM secret keys using instance KEK | Z1.6 | HR-1 |
| REQ-Z1.9A | Encrypt TBID V1 secrets using instance KEK | Z1.7 | HR-1 |
| REQ-Z1.11A | Encrypt Noise session state using instance KEK | Z1.8 | HR-1 |
| REQ-Z1.12A | Encrypt FROST Round 1 nonces (when FROST becomes active) | Z1.9 | HR-1 |
| REQ-Z2.2A | Derive seal key on-demand (eliminate persistent storage) | Z2.2 | HR-1 |
| REQ-Z2.2B | Encrypt PQC secrets in `SoftwareCryptoServer` | Z2.2 | HR-1 |
| REQ-Z2.2C | Encrypt FROST shares in `SoftwareCryptoServer` | Z2.2 | HR-1 |
| REQ-Z4.1A | Use `PrivKeyHandle` for `TimeFamilyServer::noise_static_priv` | Z4.1 | HR-1 |

#### HR-2: Immediate Zeroing Mandate

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z1.4 | Zero `ForetiasPrivKey32` bytes after use (audit all callers) | Z1.3 | HR-2 |
| REQ-Z1.8A | Zero ML-KEM shared secret immediately after extraction | Z1.6 | HR-2 |
| REQ-Z1.11B | Document/zero caller's `static_priv` after `NoiseSession::new()` | Z1.8 | HR-2 |
| REQ-Z2.5 | Wrap TBID secret `Vec<u8>` in `Zeroizing` | Z2.4 | HR-2 |
| REQ-Z3.1A | Generate ephemeral Noise keypair inside C (no FFI exposure) | Z3.1 | HR-2 |
| REQ-Z4.1B | Zero `priv_key.bytes` immediately after copying to `noise_static_priv` | Z4.1 | HR-2 |

#### Bindgen Debug Removal (Defense-in-Depth)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z1.3 | Remove `Debug` from `ForetiasPrivKey32` via bindgen `.no_debug()` | Z1.3 | COMBINED §2.3.1 |
| REQ-Z1.5 | Remove `Debug` from `ForetiasSecretKeyVar` via bindgen `.no_debug()` | Z1.4 | COMBINED §2.3.1 |
| REQ-Z1.7 | Remove `Debug` from `ForetiasKemSecretKey` via bindgen `.no_debug()` | Z1.6 | COMBINED §2.3.1 |
| REQ-Z1.9 | Remove `Debug` from `ForetiasTbidV1SecretKey` via bindgen `.no_debug()` | Z1.7 | COMBINED §2.3.1 |
| REQ-Z1.11 | Remove `Debug` from `ForetiasNoiseState` via bindgen `.no_debug()` | Z1.8 | COMBINED §2.3.1 |
| REQ-Z2.3 | Update `build.rs` bindgen config (implements all .no_debug above) | Z2.3 | COMBINED §1.3.5 |
| REQ-Z4.1 | Wrap `noise_static_priv` in `Zeroizing<[u8; 32]>` (interim, before REQ-Z4.1A) | Z4.1 | COMBINED §1.3.11 |

### 6.2 P1 — High (Fix Before v0.5)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z0.6 | Replace `foretias_memzero` with `sodium_memzero` | Z0.4 | This spec |
| REQ-Z0.7 | Remove `foretias_memzero` from public header | Z0.4 | This spec |
| REQ-Z1.6 | Zero C struct after PQC key extraction (sphincs, dilithium) | Z1.4 | This spec |
| REQ-Z1.8 | Zero C struct after ML-KEM keypair extraction | Z1.6 | This spec |
| REQ-Z2.1 | Zero caller's buffer after `PrivKeyHandle::from_seed` | Z2.1 | This spec |
| REQ-Z2.7 | Zero C struct after ML-KEM keypair extraction (Rust side) | Z2.5 | This spec |
| REQ-Z4.3 | Bound `keypairs` Vec to 2 entries | Z4.2 | COMBINED §2.5 |
| REQ-Z4.4 | Audit and encrypt `TbidSecret` in `Chronomatter` | Z4.3 | HR-1 |

### 6.3 P2 — Medium (Fix Before v1.0)

| REQ | Description | Zone | Source |
|-----|-------------|------|--------|
| REQ-Z0.3 | Consider `sodium_mprotect` for KEK and tick key memory | Z0.1 | This spec |
| REQ-Z2.4 | Evaluate restricting `Copy` on secret binding types | Z2.3 | This spec |
| REQ-Z2.6 | Audit `SignatureBytes` for `Zeroizing` internal wrapping | Z2.4 | This spec |
| REQ-Z2.8 | Document `ed25519_sign` as unsafe/raw key path | Z2.6 | This spec |
| REQ-Z1.2 | Document KEK global state as design exemption | Z1.2 | This spec |

---

## Zone 8: Implementation Plan

This zone provides a concrete plan for bringing the codebase into compliance with the HR-1/HR-2 mandate and all requirements above.

### 7.0 Phase 0: Encryption Infrastructure (P0 — Foundation)

**Effort:** ~4 hours
**Risk:** Medium (new C11 API, changes struct layouts)
**Prerequisite for:** All HR-1 compliance phases

The existing KEK infrastructure in `privkey.c` supports only 32-byte Ed25519 seeds. It must be extended to support arbitrary-length secrets.

1. **Extend KEK API** (`privkey.c` / `foretias_core.h`):
   - Add `foretias_privkey_encrypt(const uint8_t *plaintext, size_t pt_len, uint8_t *ciphertext_out, uint8_t nonce[24])` — encrypts arbitrary-length data with KEK, returns `pt_len + 16` bytes (ciphertext + MAC). Uses the monotonic counter for nonce derivation.
   - Add `foretias_privkey_decrypt(const uint8_t *ciphertext, size_t ct_len, const uint8_t nonce[24], uint8_t *plaintext_out)` — decrypts arbitrary-length data. Returns plaintext length on success.
   - These functions reuse the existing `encrypt_seed()` and `decrypt_seed()` helpers but expose them for general use.

2. **Update struct layouts** (`foretias_core.h`):
   - `ForetiasSecretKeyVar`: Change `bytes[4096]` to `encrypted_bytes[4112]` (4096 + 16 MAC). Add `nonce[24]` field.
   - `ForetiasKemSecretKey`: Change `bytes[2400]` to `encrypted_bytes[2416]` (2400 + 16 MAC). Add `nonce[24]` field.
   - `ForetiasTbidV1SecretKey`: Change `ed25519_sk: ForetiasPrivKey32` to `encrypted_ed25519[48]` + `ed25519_nonce[24]`. Change `slh_dsa_sk[128]` to `encrypted_slh_dsa[144]` + `slh_dsa_nonce[24]`.
   - `ForetiasNoiseState`: Change `local_static_priv[32]` to `encrypted_local_static[48]` + `static_nonce[24]`. Similarly for `send_key`, `recv_key`, `chaining_key`.

3. **Rebuild and test:**
   ```bash
   cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
   cd p2p/core/build && ctest --output-on-failure
   cd p2p && cargo build -p foretias-core
   cd p2p && cargo test -p foretias-core
   ```

### 7.1 Phase 1: Bindgen Debug Removal (P0)

**Effort:** ~30 minutes
**Risk:** Low (only affects debug output, not runtime behavior)
**Can run in parallel with:** Phase 0

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
**Can run in parallel with:** Phase 0

1. **TBID Secret** (`signing_tbid.rs:32-43`):
   - Wrap `secret_bytes` in `Zeroizing::new(...)`.
   - Ensure `TbidSecret` in `types.rs` uses `Zeroizing<Vec<u8>>` internally.

2. **Noise Static Key** (`server/mod.rs:43`):
   - Change `noise_static_priv: [u8; 32]` to `noise_static_priv: Zeroizing<[u8; 32]>`.
   - Update all references to dereference: `&*self.noise_static_priv`.

3. **Zero ephemeral keys** (`noise_ptp.rs`, `server/mod.rs`):
   - Zero `priv_key.bytes` immediately after use (REQ-Z4.1B).

4. **Rebuild and test:**
   ```bash
   cd p2p && cargo build --workspace
   cd p2p && cargo test --workspace
   ```

### 7.3 Phase 3: Encrypt PQC Secrets (P0 — HR-1 Compliance)

**Effort:** ~6 hours
**Risk:** High (changes struct layouts, FFI signatures, all signing paths)
**Depends on:** Phase 0

1. **SPHINCS+** (`signing_sphincs.c`):
   - `foretias_sphincs_*_keypair()`: Encrypt secret key before returning. Store encrypted form + nonce in `ForetiasSecretKeyVar`.
   - `foretias_sphincs_*_sign()`: Decrypt secret key, sign, zero decrypted buffer immediately.
   - Update Rust wrappers (`signing_sphincs.rs`) to handle encrypted structs.

2. **Dilithium3** (`signing_dilithium.c`):
   - Same pattern as SPHINCS+.

3. **ML-KEM** (`kem_mlkem.c`):
   - `foretias_mlkem_768_keypair()`: Encrypt secret key before returning.
   - `foretias_mlkem_768_decapsulate()`: Decrypt secret key, decapsulate, zero decrypted buffer immediately.
   - `foretias_mlkem_768_encapsulate()`: Zero shared secret after writing (HR-2).

4. **Rebuild and test:**
   ```bash
   cd p2p/core && cmake --build build
   cd p2p/core/build && ctest --output-on-failure
   cd p2p && cargo build --workspace
   cd p2p && cargo test --workspace
   ```

### 7.4 Phase 4: Encrypt TBID Secrets (P0 — HR-1 Compliance)

**Effort:** ~3 hours
**Risk:** High (changes TBID struct layout, signing path)
**Depends on:** Phase 0

1. **TBID** (`signing_tbid.c`):
   - `foretias_tbid_v1_keypair()`: Encrypt both Ed25519 and SLH-DSA components.
   - `foretias_tbid_v1_sign()`: Decrypt both components, sign, zero decrypted buffers immediately.
   - Update Rust wrappers (`signing_tbid.rs`) to handle encrypted structs.

2. **Rebuild and test.**

### 7.5 Phase 5: Encrypt Noise Session State (P0 — HR-1 Compliance)

**Effort:** ~4 hours
**Risk:** High (changes Noise struct layout, all handshake paths)
**Depends on:** Phase 0

1. **Noise** (`noise_xx.c`):
   - `foretias_noise_init_ed25519()`: Encrypt `local_static_priv` before storing.
   - All DH operations: Decrypt session keys on-demand, use, zero immediately.
   - `foretias_noise_destroy()`: Zero encrypted state (already done).
   - Update Rust wrappers (`noise.rs`) to handle encrypted structs.

2. **Ephemeral key generation** (`noise_ptp.rs`):
   - Generate ephemeral keypair inside C (REQ-Z3.1A). No raw seed crosses FFI.

3. **Rebuild and test.**

### 7.6 Phase 6: Migrate ForetiasPrivKey32 Consumers (P0 — HR-1 Compliance)

**Effort:** ~3 hours
**Risk:** Medium (changes API surface for raw key consumers)
**Depends on:** Phase 0

1. **TimeFamilyServer** (`server/mod.rs`):
   - Replace `noise_static_priv: Zeroizing<[u8; 32]>` with `noise_static_priv: PrivKeyHandle`.
   - Derive X25519 scalar inside C via new `foretias_privkey_derive_x25519()` function.

2. **`ed25519_sign`** (`signing.rs`):
   - Mark as deprecated. Prefer `ed25519_sign_with_handle`.

3. **`generate_ed25519_keypair`** (`identity.rs`):
   - Mark as deprecated. Prefer `PrivKeyHandle::generate()`.

4. **Rebuild and test.**

### 7.7 Phase 7: C Struct Zeroing After Extraction (P1)

**Effort:** ~1 hour
**Risk:** Low (adds zeroing calls, doesn't change behavior)

1. **ML-KEM keypair** (`kem_mlkem.rs:14-15`): Add `sodium_memzero` after extraction.
2. **SPHINCS+ keypair** (`signing_sphincs.rs`): Add `sodium_memzero` after extraction.
3. **Dilithium keypair** (`signing_dilithium.rs`): Add `sodium_memzero` after extraction.
4. **TBID keypair** (`signing_tbid.rs:41`): Already zeros C struct. Verify.

### 7.8 Phase 8: Replace `foretias_memzero` with `sodium_memzero` (P1)

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

### 7.9 Phase 9: Bound Keypairs Vec (P1)

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

Can code in Zone X access secrets from Zone Y? (Current state — before HR compliance)

| Secret | C11 Code | foretias-core | foretias-client | foretias-server | Python/Java | HR-1 |
|--------|----------|---------------|-----------------|-----------------|-------------|------|
| `ForetiasPrivKey` (handle) | Encrypted only | Opaque pointer | Opaque pointer | Opaque pointer | Opaque handle | ✅ PASS |
| KEK | File-scope static | No access | No access | No access | No access | ⚠️ N/A |
| `ForetiasPrivKey32` | Raw bytes | Raw bytes (via FFI) | Raw bytes | Raw bytes | Raw bytes (if exposed) | ❌ VIOLATION |
| PQC secrets (C struct) | Raw bytes | Raw bytes (via FFI) | N/A | N/A | N/A | ❌ VIOLATION |
| PQC secrets (Rust wrapper) | N/A | Zeroizing wrapper (plaintext) | N/A | N/A | N/A | ❌ VIOLATION |
| TBID secret (Rust Vec) | N/A | **Plain Vec — GAP** | N/A | Via core-engine | Via core-engine | ❌ VIOLATION |
| Noise static key | N/A | N/A | N/A | **Plain [u8;32] — GAP** | N/A | ❌ VIOLATION |
| Noise session state | Raw bytes | Opaque (RAII) | Opaque (RAII) | Opaque (RAII) | N/A | ❌ VIOLATION |
| Tick keypairs | N/A | Opaque handles | N/A | Via core-engine | Via core-engine | ✅ PASS |
| Seal key | N/A | Zeroizing wrapper (plaintext) | N/A | N/A | N/A | ❌ VIOLATION |
| FROST shares | N/A | Zeroizing wrapper (plaintext) | N/A | N/A | N/A | ❌ VIOLATION |

**Target state (after HR compliance):** All rows except KEK should show "Encrypted" or "Opaque" in every zone column, with HR-1 = ✅ PASS.

---

## Appendix D: libp2p External Dependency Violation

**VIOLATION: HR-1 — libp2p Noise Protocol Keys**
- **Component:** `libp2p-noise` crate (external dependency)
- **Reason:** The libp2p Noise transport layer generates and stores X25519 static/ephemeral keys internally. These keys are managed by the libp2p crate, not by Foretias code. We cannot modify libp2p's internal key storage to use our KEK-based encryption.
- **Threat:** libp2p static and ephemeral keys exist in plaintext within the libp2p crate's memory. A process dump reveals these keys.
- **Compensating Controls:**
  - libp2p uses its own `Noise` protocol with forward secrecy — ephemeral keys are rotated per connection.
  - libp2p static keys are only used for authentication, not bulk encryption.
  - The libp2p crate is a well-audited, widely-used dependency.
- **Owner:** External library (`libp2p-noise`) — NOT within our control.
- **Remediation Path:** N/A — external dependency. If this becomes unacceptable, we would need to fork libp2p or implement a custom transport layer, which is not currently planned.

**VIOLATION: HR-1 — libp2p Peer Identity Keys**
- **Component:** `libp2p-identify` crate (external dependency)
- **Reason:** libp2p peer identity keys (used for peer authentication in the swarm) are stored by the libp2p crate. Foretias passes the key pair to libp2p during initialization, after which libp2p manages the keys internally.
- **Threat:** Peer identity keys exist in plaintext within libp2p memory.
- **Compensating Controls:** Peer identity keys are used only for authentication, not for signing time attestations. The Foretias identity (tick keys) remains protected via `PrivKeyHandle`.
- **Owner:** External library (`libp2p-identify`) — NOT within our control.
- **Remediation Path:** N/A — external dependency.

---

## Appendix C: Verification Checklist

After implementing all requirements, verify:

### HR-1: Encryption Compliance
- [ ] `ForetiasPrivKey32` consumers migrated to opaque handle path (REQ-Z1.3A)
- [ ] `ForetiasSecretKeyVar` stores encrypted bytes + nonce (REQ-Z1.5A)
- [ ] `ForetiasKemSecretKey` stores encrypted bytes + nonce (REQ-Z1.7A)
- [ ] `ForetiasTbidV1SecretKey` stores encrypted components + nonces (REQ-Z1.9A)
- [ ] `ForetiasNoiseState` stores encrypted session keys + nonces (REQ-Z1.11A)
- [ ] `ForetiasFrostRound1` stores encrypted nonces (REQ-Z1.12A)
- [ ] Seal key derived on-demand, not stored persistently (REQ-Z2.2A)
- [ ] PQC secrets in `SoftwareCryptoServer` encrypted (REQ-Z2.2B)
- [ ] FROST shares in `SoftwareCryptoServer` encrypted (REQ-Z2.2C)
- [ ] `TimeFamilyServer::noise_static_priv` uses `PrivKeyHandle` (REQ-Z4.1A)
- [ ] All HR-1 violations documented with violation template (libp2p, etc.)

### HR-2: Immediate Zeroing Compliance
- [ ] `ForetiasPrivKey32` bytes zeroed after every use (REQ-Z1.4)
- [ ] ML-KEM shared secret zeroed immediately after extraction (REQ-Z1.8A)
- [ ] Caller's `static_priv` zeroed after `NoiseSession::new()` (REQ-Z1.11B)
- [ ] TBID secret `Vec<u8>` wrapped in `Zeroizing` (REQ-Z2.5)
- [ ] Ephemeral Noise keypair generated inside C (REQ-Z3.1A)
- [ ] `priv_key.bytes` zeroed immediately after copying to `noise_static_priv` (REQ-Z4.1B)

### Defense-in-Depth
- [ ] `bindings.rs` has no `#[derive(Debug)]` on `ForetiasPrivKey32`, `ForetiasSecretKeyVar`, `ForetiasKemSecretKey`, `ForetiasTbidV1SecretKey`, `ForetiasNoiseState`, `ForetiasFrostRound1`
- [ ] `foretias_memzero` calls replaced with `sodium_memzero` (or `memzero.c` removed)
- [ ] `memzero.c` removed from `build.rs` sources list
- [ ] `foretias_memzero` declaration removed from `foretias_core.h`
- [ ] `noise_static_priv` in `TimeFamilyServer` wrapped in `Zeroizing<[u8; 32]>` (interim)
- [ ] C structs zeroed after PQC key extraction (ML-KEM, SPHINCS+, Dilithium)
- [ ] `keypairs` Vec bounded to 2 entries
- [ ] `TbidSecret` uses `Zeroizing<Vec<u8>>` internally
- [ ] `SignatureBytes` uses `Zeroizing<Vec<u8>>` internally
- [ ] `ed25519_sign` documented as unsafe/raw path
- [ ] All tests pass: `cargo test --workspace`
- [ ] No `{:?}` formatting on secret-holding types in production code

---

*End of specification.*
