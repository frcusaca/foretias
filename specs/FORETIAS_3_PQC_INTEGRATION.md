# Foretias — Post-Quantum Cryptography Integration (liboqs)

**Project:** Foretias (Free and Open-source Resilient Time Integrity Attestation Service)
**This document:** Specification for integrating liboqs into the C11 core to support multiple post-quantum signature and KEM algorithms.
**Companion documents:**
- `foretias-v1.md` — Foretias v1 product spec
- `FORETIAS_0_OVERVIEW.md` — design invariants, roadmap
- `FORETIAS_1_MVP_SPEC.md` — v0.1 local-server stack (C11 core, Rust node)
- `FORETIAS_2_IMPLEMENTATION_PLAN.md` — v0.2 P2P mutual attestation

**Target:** AI Coding Specialist for execution. Comments to human reader in parenthesis `(@human ...)`.

**Breaking Changes:** This is a pre-MVP development branch. All changes are breaking. No backward compatibility shims, no `#[serde(default)]` for new fields, no legacy migration paths. Upgrade paths will be designed when a public release exists.

---

## READING ORDER

1. Read `foretias-v1.md` — defines TimeBeing, Tick, Calendar, Chronomatter, Foretis, TimeFamily.
2. Read `FORETIAS_0_OVERVIEW.md` Parts 0-3 — design invariants and project structure.
3. Read this document end to end.
4. Scan `p2p/core-engine/src/crypto_server/` and `p2p/core-engine/src/foretias/tick.rs` for current crypto shape.

---

## PART 0 — DESIGN GOALS & ALGORITHM SELECTION

### 0.1 Goal

Replace the monolithic Ed25519-only signing model with a pluggable, multi-algorithm signing system backed by liboqs (Open Quantum Safe). The default signing algorithm becomes **SPHINCS+**, with **Dilithium** as an optional alternative. Key exchange remains **NOISE_XX** (Ed25519) by default, with **ML-KEM** (Kyber) as an optional alternative.

### 0.2 Why liboqs in C11

liboqs is a C library. It belongs in the C11 verified core (`p2p/core/`), following the same pattern as libsodium:

```
libsodium (C) → p2p/core/src/signing_ed25519.c → foretias_core.h → Rust FFI → CryptoServer trait
liboqs  (C) → p2p/core/src/signing_sphincs.c   → foretias_core.h → Rust FFI → CryptoServer trait
```

No separate Rust binding crate. `build.rs` compiles and links liboqs alongside libsodium.

### 0.3 Algorithm Selection & Rationale

> **CRITICAL VERSIONING NOTE (from liboqs research):**
> - **liboqs 0.13.0** (wrapped by `oqs` crate 0.11.0): SPHINCS+ and Dilithium **are still available**.
> - **liboqs 0.15.0** (current, Nov 2025): Dilithium **removed**, SPHINCS+ **last supported version**.
> - **liboqs 0.16.0** (future): SPHINCS+ **removed** → replaced by SLH-DSA (FIPS 205).
> - **We target liboqs 0.13.0** via the `oqs` Rust crate 0.11.0 to retain SPHINCS+ and Dilithium.
> - Plan for SLH-DSA/ML-DSA migration when `oqs` updates to wrap liboqs 0.15.0+.

| Role | Algorithm | Variant (oqs enum) | liboqs ID String | Security | PubKey | Signature | Default? |
|------|-----------|---------------------|------------------|----------|--------|-----------|----------|
| Signing | **SPHINCS+** | `SphincsSha2128sSimple` | `SPHINCS+-SHA2-128s-simple` | NIST Level 1 | 32 B | 7,856 B | **YES** |
| Signing | Dilithium | `Dilithium3` | `Dilithium3` | NIST Level 3 | 1,952 B | 3,309 B | Optional |
| KEM | **NOISE_XX** | Ed25519/X25519 | `Noise-XX` | Classical | 32 B | — | **YES** |
| KEM | ML-KEM | `MlKem768` | `ML-KEM-768` | NIST Level 3 | 1,184 B | ct:1,088/ss:32 | Optional |

**SPHINCS+-SHA2-128s-simple as default:**
- Stateless hash-based signature — no key reuse concerns
- Smallest public key (32 bytes) — critical for calendar storage
- Signature ~7.8KB — trade-off accepted for stamping correctness
- Most conservative security assumptions (hash-only, no algebraic structure)
- Successor: SLH-DSA (FIPS 205) — plan migration path

**Dilithium3 as optional:**
- Much smaller signatures (3.3KB) — useful for bandwidth-constrained scenarios
- Larger public key (1.9KB) — impacts calendar size
- Lattice-based — different threat model than SPHINCS+
- Successor: ML-DSA-65 (identical params, FIPS 204 name)

**ML-KEM-768 for key exchange:**
- NIST standardization: ML-KEM (FIPS 203)
- Level 3 security balances key/ciphertext sizes
- Replaces NOISE_XX only when explicitly configured

### 0.4 Nomenclature

| Term | Meaning |
|------|---------|
| `SignatureAlgorithm` | Enum identifying the signing algorithm+variant |
| `KemAlgorithm` | Enum identifying the KEM algorithm+variant |
| `algorithm_id` | Plain-text string stored in Foretis and TickRecord |
| `FORETIAS_SIG_*` | C11 enum constants for signature algorithms |
| `FORETIAS_KEM_*` | C11 enum constants for KEM algorithms |

---

## PART 1 — REFATORING: MULTI-ALGORITHM CAPABILITY

### 1.1 C11 API Extensions (`foretias_core.h`)

Add algorithm enumerations and variable-size types:

```c
/* ── Signature algorithm selector ─────────────────── */
typedef enum {
    FORETIAS_SIG_ED25519        = 1,   // Legacy, retained
    FORETIAS_SIG_SPHINCS_SHA256_128S = 2,  // Default PQ signing
    FORETIAS_SIG_DILITHIUM3     = 3,   // Optional PQ signing
} ForetiasSignatureAlgorithm;

/* ── KEM algorithm selector ───────────────────────── */
typedef enum {
    FORETIAS_KEM_NOISE_XX       = 1,   // Default (Ed25519/X25519 via Noise_XX)
    FORETIAS_KEM_MLKEM_768      = 2,   // Optional PQ key exchange
} ForetiasKemAlgorithm;

/* ── Algorithm ID string constants ────────────────── */
/* These match the liboqs OQS_SIG_alg_* / OQS_KEM_alg_* identifiers */
#define FORETIAS_SIG_ID_ED25519         "Ed25519"
#define FORETIAS_SIG_ID_SPHINCS_128S    "SPHINCS+-SHA2-128s-simple"
#define FORETIAS_SIG_ID_DILITHIUM3      "Dilithium3"
#define FORETIAS_KEM_ID_NOISE_XX        "Noise-XX"
#define FORETIAS_KEM_ID_MLKEM_768       "ML-KEM-768"

/* ── Maximum sizes for buffer allocation ──────────── */
/* SPHINCS+-SHA2-128s-simple: PK=32, SK=64, SIG=7856 */
/* Dilithium3:                 PK=1952, SK=4000, SIG=3309 */
/* Sizes rounded to next power-of-256 boundary */
#define FORETIAS_SIG_MAX_PUBKEY_BYTES   2048   // Dilithium3 pubkey (1952)
#define FORETIAS_SIG_MAX_SECRET_BYTES   4096   // Dilithium3 secret (4000)
#define FORETIAS_SIG_MAX_SIG_BYTES      8192   // SPHINCS+ 128s signature (7856)
#define FORETIAS_KEM_MAX_PUBKEY_BYTES   1184  // ML-KEM-768
#define FORETIAS_KEM_MAX_CIPHERTEXT     1088  // ML-KEM-768
#define FORETIAS_KEM_SHARED_SECRET      32

/* ── Variable-size key/signature types ────────────── */
typedef struct {
    uint8_t bytes[FORETIAS_SIG_MAX_PUBKEY_BYTES];
    size_t  len;
} ForetiasPubKeyVar;

typedef struct {
    uint8_t bytes[FORETIAS_SIG_MAX_SECRET_BYTES];
    size_t  len;
} ForetiasSecretKeyVar;

typedef struct {
    uint8_t bytes[FORETIAS_SIG_MAX_SIG_BYTES];
    size_t  len;
} ForetiasSigVar;

/* ── Algorithm helper ─────────────────────────────── */
const char* foretias_sig_algorithm_id(ForetiasSignatureAlgorithm alg);
const char* foretias_kem_algorithm_id(ForetiasKemAlgorithm alg);
size_t      foretias_sig_pubkey_bytes(ForetiasSignatureAlgorithm alg);
size_t      foretias_sig_secret_bytes(ForetiasSignatureAlgorithm alg);
size_t      foretias_sig_signature_bytes(ForetiasSignatureAlgorithm alg);

/* ── SPHINCS+ operations ──────────────────────────── */
ForetiasResult foretias_sphincs_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out
);

ForetiasResult foretias_sphincs_sign(
    const ForetiasSecretKeyVar* secret,
    const uint8_t*            msg,
    size_t                    msg_len,
    ForetiasSigVar*            sig_out
);

ForetiasResult foretias_sphincs_verify(
    const ForetiasPubKeyVar*  public_key,
    const uint8_t*            msg,
    size_t                    msg_len,
    const ForetiasSigVar*     sig
);

/* ── Dilithium operations ─────────────────────────── */
ForetiasResult foretias_dilithium_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out
);

ForetiasResult foretias_dilithium_sign(
    const ForetiasSecretKeyVar* secret,
    const uint8_t*            msg,
    size_t                    msg_len,
    ForetiasSigVar*            sig_out
);

ForetiasResult foretias_dilithium_verify(
    const ForetiasPubKeyVar*  public_key,
    const uint8_t*            msg,
    size_t                    msg_len,
    const ForetiasSigVar*     sig
);

/* ── ML-KEM operations ────────────────────────────── */
ForetiasResult foretias_mlkem_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out
);

ForetiasResult foretias_mlkem_encapsulate(
    const ForetiasPubKeyVar*  public_key,
    ForetiasSecretKeyVar*     ciphertext_out,   // repurposed for ciphertext
    uint8_t*                  shared_secret_out // 32 bytes
);

ForetiasResult foretias_mlkem_decapsulate(
    const ForetiasSecretKeyVar* secret,
    const ForetiasSecretKeyVar* ciphertext,
    uint8_t*                    shared_secret_out // 32 bytes
);
```

### 1.2 C11 Source Files

| File | Purpose |
|------|---------|
| `p2p/core/src/signing_sphincs.c` | SPHINCS+ SHA256/128s wrapper over liboqs `OQS_SIG` API |
| `p2p/core/src/signing_sphincs.h` | Internal header for SPHINCS+ helpers |
| `p2p/core/src/signing_dilithium.c` | Dilithium3 wrapper over liboqs `OQS_SIG` API |
| `p2p/core/src/signing_dilithium.h` | Internal header for Dilithium helpers |
| `p2p/core/src/kem_mlkem.c` | ML-KEM-768 wrapper over liboqs `OQS_KEM` API |
| `p2p/core/src/kem_mlkem.h` | Internal header for ML-KEM helpers |
| `p2p/core/src/algorithms.c` | `foretias_sig_algorithm_id()`, size helpers |
| `p2p/core/src/algorithms.h` | Internal header for algorithm helpers |

Each signing file follows the libsodium pattern: allocate `OQS_SIG` instance, call the appropriate method, free the instance, return `ForetiasResult`.

### 1.3 Rust Build System Updates

**`p2p/core-engine/build.rs`:**
- Clone liboqs from `https://github.com/open-quantum-safe/liboqs` at a pinned tag
- Build with CMake: `oqs_enable_open_ssl=OFF`, `enable_test=OFF`, `enable_unstable=OFF`
- Link `liboqs.a` statically into the foretias binary
- Run bindgen over `foretias_core.h` to expose the new PQ types

**`p2p/core-engine/Cargo.toml`:**
```toml
[build-dependencies]
cmake = "0.1"
```

### 1.4 Rust Safe Wrappers

**New files in `p2p/core-engine/src/core/`:**

| File | Purpose |
|------|---------|
| `signing_sphincs.rs` | `SphincsKeypair`, `SphincsSign`, `SphincsVerify` |
| `signing_dilithium.rs` | `DilithiumKeypair`, `DilithiumSign`, `DilithiumVerify` |
| `kem_mlkem.rs` | `MlKemKeypair`, `MlKemEncapsulate`, `MlKemDecapsulate` |

Each follows the existing pattern from `core/signing.rs`: Box the C state, implement `Drop`, all calls through `unsafe { ... }` wrapped in `Result`.

### 1.5 Type Alias Updates (`foretias/types.rs`)

Replace fixed-size aliases with flexible types:

```rust
// OLD (removed):
// pub type PublicKey = [u8; 32];
// pub type Signature = [u8; 64];

// NEW:
/// Public key bytes — variable length per algorithm.
pub type PublicKeyBytes = Vec<u8>;

/// Signature bytes — variable length per algorithm.
pub type SignatureBytes = Vec<u8>;

/// Algorithm identifier as plain text.
pub type AlgorithmId = String;
```

### 1.6 CryptoServer Trait Extension

The `CryptoServer` trait gains algorithm-aware methods:

```rust
use crate::foretias::SignatureAlgorithm;

pub trait CryptoServer: Send + Sync {
    // ── Existing methods (unchanged) ──────────────────
    fn public_key(&self) -> PublicKeyBytes;
    fn peer_id(&self) -> ForetiasPeerID;
    fn curve(&self) -> ForetiasCurve;
    fn capabilities(&self) -> CryptoServerCapabilities;
    fn sign(&self, msg: &[u8]) -> Result<ForetiasSigVar, CryptoError>;
    fn sha256(&self, data: &[u8]) -> Result<ForetiasHash32, CryptoError>;
    // ... etc

    // ── NEW: Algorithm-aware operations ───────────────
    /// Returns the signature algorithm this server uses.
    fn signature_algorithm(&self) -> SignatureAlgorithm;

    /// Signs with the server's configured algorithm.
    /// Replaces the old `sign()` which was always Ed25519.
    fn sign_with(&self, msg: &[u8], alg: SignatureAlgorithm)
        -> Result<SignatureBytes, CryptoError>;

    /// Verifies a signature given the algorithm ID.
    /// The algorithm ID is carried in the Foretis/TickRecord itself.
    fn verify_with(&self,
        pub_key: &PublicKeyBytes,
        alg_id: &str,          // plain text algorithm identifier
        msg: &[u8],
        sig: &SignatureBytes,
    ) -> Result<bool, CryptoError>;

    /// Generates a keypair for the given algorithm.
    fn generate_keypair(&self, alg: SignatureAlgorithm)
        -> Result<(SignatureBytes, PublicKeyBytes), CryptoError>;
}
```

### 1.7 Foretias Domain Type Changes

**`Foretis`** gains `signature_algorithm`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foretis {
    pub tick_number: u64,
    pub content_hash: [u8; 32],
    /// Variable-length signature bytes (algorithm-dependent).
    pub signature: Vec<u8>,
    /// Plain-text algorithm identifier (e.g. "SPHINCS+-SHA256/128s").
    pub signature_algorithm: String,
    pub tbid: [u8; 16],
    pub echo: String,
    pub tbn: String,
    pub time_being_reference_time: String,
}
```

**`TickRecord`** gains `signature_algorithm` on each record:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickRecord {
    pub tick_number: u64,
    /// Variable-length public key (algorithm-dependent).
    pub public_key: Vec<u8>,
    /// Plain-text algorithm identifier for this tick's key.
    pub signature_algorithm: String,
    pub forward_foretis: Vec<u8>,
    pub backward_foretis: Vec<u8>,
    pub aa_nonce: [u8; 16],
    #[serde(default)]
    pub stamps_per_tick: u64,
    #[serde(default)]
    pub external_attestations: Vec<ExternalAttestation>,
}
```

### 1.8 `stamp()` and `verify()` Function Signature Changes

```rust
/// Stamp content under the current tick's key.
pub fn stamp(
    server: &dyn CryptoServer,
    tbid: &[u8; 16],
    tick_number: u64,
    content: &[u8],
    echo: &str,
    tbn: &str,
) -> Result<Foretis, NodeError> {
    // sig_input = concat(tbid, tick_number, content) — UNCHANGED
    let signature = server.sign(&sig_input)?;
    let content_hash = server.sha256(content)?;
    let sig_alg = server.signature_algorithm().to_string(); // NEW

    Ok(Foretis {
        tick_number,
        content_hash: content_hash.bytes,
        signature: signature.bytes.to_vec(),
        signature_algorithm: sig_alg,  // NEW
        // ...
    })
}

/// Verify a Foretis against content and calendar.
pub fn verify(
    server: &dyn CryptoServer,
    foretis: &Foretis,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    // ... content hash check ...

    let records = calendar.get(foretis.tick_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick"))?;

    // RECONCILE ALGORITHMS — NEW
    if rec.signature_algorithm != foretis.signature_algorithm {
        return Err(NodeError::AlgorithmMismatch(
            format!("tick uses '{}' but Foretis claims '{}'",
                   rec.signature_algorithm, foretis.signature_algorithm)
        ));
    }

    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&foretis.tbid);
    sig_input.extend_from_slice(&foretis.tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    // VERIFY WITH ALGORITHM — CHANGED
    Ok(server.verify_with(
        &rec.public_key,
        &rec.signature_algorithm,
        &sig_input,
        &foretis.signature,
    )?)
}
```

### 1.9 TimeFamily / Chronomatter Configuration

The `TimeFamily` and `Chronomatter` constructors accept a signature algorithm:

```rust
// In NodeConfig:
pub signature_algorithm: SignatureAlgorithm,  // default: SPHINCS_SHA256_128S
pub kem_algorithm: KemAlgorithm,              // default: NOISE_XX

// In Chronomatter:
pub fn new(
    name: &str,
    chronon_ns: u64,
    serialized: bool,
    server: Arc<dyn CryptoServer>,
    signature_algorithm: SignatureAlgorithm,  // NEW
) -> Result<Self, NodeError>
```

The algorithm is baked into every tick record and every Foretis the Chronomatter produces.

### 1.10 Family-Level Algorithm Compatibility Check

Before the Family delegates stamping to a Time Being, it verifies algorithm compatibility:

```rust
// In TimeFamily (orchestrator):
pub fn stamp(&self, content: &[u8], echo: &str) -> Result<Foretis, NodeError> {
    let my_alg = self.config.signature_algorithm;
    // All time beings in the family must use the same algorithm family.
    // SPHINCS+ and Dilithium are NOT interchangeable for verification.
    // Ed25519 may coexist only during migration (not applicable pre-MVP).
    self.chronomatter.stamp(content, echo)
}
```

The master list of acceptable algorithm identifiers is maintained in code:

```rust
pub const ACCEPTED_SIGNATURE_ALGORITHMS: &[&str] = &[
    "Ed25519",
    "SPHINCS+-SHA256/128s",
    "Dilithium3",
];

pub const ACCEPTED_KEM_ALGORITHMS: &[&str] = &[
    "Noise-XX",
    "ML-KEM-768",
];
```

On TimeFamily creation, the chosen algorithm is validated against this list.

---

## PART 2 — SPHINCS+ IMPLEMENTATION

### 2.1 C11 Implementation (`signing_sphincs.c`)

```c
#include "foretias_core.h"
#include <oqs/oqs.h>

ForetiasResult foretias_sphincs_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out)
{
    OQS_SIG *sig = OQS_SIG_alg_default(OQS_SIG_algorithm_SPHINCSPLUS_SHA256_128s);
    if (!sig) return FORETIAS_ERR_INTERNAL;

    sig->keypair(secret_out->bytes, &secret_out->len, public_out->bytes, &public_out->len);
    OQS_SIG_free(sig);
    return FORETIAS_OK;
}

ForetiasResult foretias_sphincs_sign(
    const ForetiasSecretKeyVar* secret,
    const uint8_t*            msg,
    size_t                    msg_len,
    ForetiasSigVar*            sig_out)
{
    OQS_SIG *sig = OQS_SIG_new(OQS_SIG_algorithm_SPHINCSPLUS_SHA256_128s);
    if (!sig) return FORETIAS_ERR_INTERNAL;

    sig->sign(secret->bytes, secret->len, msg, msg_len,
              sig_out->bytes, &sig_out->len);
    OQS_SIG_free(sig);
    return FORETIAS_OK;
}

ForetiasResult foretias_sphincs_verify(
    const ForetiasPubKeyVar*  public_key,
    const uint8_t*            msg,
    size_t                    msg_len,
    const ForetiasSigVar*     sig)
{
    OQS_SIG *sig = OQS_SIG_new(OQS_SIG_algorithm_SPHINCSPLUS_SHA256_128s);
    if (!sig) return FORETIAS_ERR_INTERNAL;

    int rc = sig->verify(public_key->bytes, public_key->len,
                         msg, msg_len, sig->bytes, sig->len);
    OQS_SIG_free(sig);
    return rc == 0 ? FORETIAS_OK : FORETIAS_ERR_BAD_SIG;
}
```

### 2.2 Rust Wrapper (`core/signing_sphincs.rs`)

```rust
use super::bindings::*;
use crate::error::CryptoError;

pub struct SphincsKeypair {
    secret: Box<ForetiasSecretKeyVar>,
    public: Box<ForetiasPubKeyVar>,
}

impl SphincsKeypair {
    pub fn generate() -> Result<Self, CryptoError> {
        let mut secret = Box::new(unsafe { std::mem::zeroed() });
        let mut public = Box::new(unsafe { std::mem::zeroed() });
        let rc = unsafe { foretias_sphincs_keypair(&mut *secret, &mut *public) };
        check(rc)?;
        Ok(Self { secret, public })
    }

    pub fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut sig = Box::new(unsafe { std::mem::zeroed() });
        let rc = unsafe {
            foretias_sphincs_sign(&*self.secret, msg.as_ptr(), msg.len(), &mut *sig)
        };
        check(rc)?;
        Ok(sig.bytes[..sig.len as usize].to_vec())
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.public.bytes[..self.public.len as usize].to_vec()
    }
}

impl Drop for SphincsKeypair {
    fn drop(&mut self) {
        // Zero secret material
        self.secret.bytes[..self.secret.len as usize].fill(0);
    }
}
```

### 2.3 SoftwareCryptoServer SPHINCS+ Integration

The `SoftwareCryptoServer` now tracks the active signature algorithm:

```rust
pub struct SoftwareCryptoServer {
    curve:             ForetiasCurve,
    sig_algorithm:     SignatureAlgorithm,  // NEW
    pub_key:           PublicKeyBytes,      // Now Vec<u8>
    priv_key:          Zeroizing<Vec<u8>>, // Now variable-length
    peer_id:           ForetiasPeerID,
    // ...
}

impl SoftwareCryptoServer {
    pub fn generate(
        curve: ForetiasCurve,
        sig_algorithm: SignatureAlgorithm,  // NEW parameter
    ) -> Result<Self, CryptoError> {
        match sig_algorithm {
            SignatureAlgorithm::SPHINCS_SHA256_128S => {
                let keypair = SphincsKeypair::generate()?;
                Ok(Self {
                    sig_algorithm,
                    pub_key: keypair.public_key_bytes(),
                    priv_key: Zeroizing::new(keypair.secret_bytes()),
                    // ...
                })
            }
            // ...
        }
    }
}
```

### 2.4 Testing

| Test | Description |
|------|-------------|
| `sphincs_keygen_produces_valid_pair` | Generate keypair, verify sizes |
| `sphincs_sign_verify_roundtrip` | Sign arbitrary message, verify passes |
| `sphincs_verify_rejects_tampered_sig` | Flip a byte in signature, verify fails |
| `sphincs_verify_rejects_wrong_pubkey` | Verify with wrong public key, fails |
| `sphincs_signature_size_is_7856` | Confirm exact signature size (7856 bytes) |
| `sphincs_pubkey_size_is_32` | Confirm exact pubkey size |
| `crypto_server_sphincs_sign` | Full CryptoServer trait path |
| `stamp_with_sphincs_produces_foretis` | stamp() carries algorithm ID |
| `verify_with_sphincs_valid` | verify() resolves algorithm from Foretis |
| `verify_pair_with_sphincs` | Auto-attestation verification works |

---

## PART 3 — DILITHIUM IMPLEMENTATION

### 3.1 C11 Implementation (`signing_dilithium.c`)

Same pattern as SPHINCS+, using `OQS_SIG_algorithm_Dilithium3`:

```c
ForetiasResult foretias_dilithium_keypair(...)  // OQS_SIG + Dilithium3
ForetiasResult foretias_dilithium_sign(...)     // OQS_SIG + Dilithium3
ForetiasResult foretias_dilithium_verify(...)   // OQS_SIG + Dilithium3
```

### 3.2 Rust Wrapper (`core/signing_dilithium.rs`)

Same pattern as `signing_sphincs.rs`, wrapping the Dilithium C functions.

### 3.3 Testing

Same test matrix as SPHINCS+, with Dilithium-specific sizes:
- Pubkey: 1932 bytes
- Signature: 3309 bytes

---

## PART 4 — ML-KEM IMPLEMENTATION

### 4.1 C11 Implementation (`kem_mlkem.c`)

```c
#include "foretias_core.h"
#include <oqs/oqs.h>

ForetiasResult foretias_mlkem_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out)
{
    OQS_KEM *kem = OQS_KEM_new(OQS_KEM_algorithm_MLKEM_768);
    if (!kem) return FORETIAS_ERR_INTERNAL;

    kem->keypair(public_out->bytes, &public_out->len,
                 secret_out->bytes, &secret_out->len);
    OQS_KEM_free(kem);
    return FORETIAS_OK;
}

ForetiasResult foretias_mlkem_encapsulate(
    const ForetiasPubKeyVar*  public_key,
    ForetiasSecretKeyVar*     ciphertext_out,
    uint8_t*                  shared_secret_out)
{
    OQS_KEM *kem = OQS_KEM_new(OQS_KEM_algorithm_MLKEM_768);
    if (!kem) return FORETIAS_ERR_INTERNAL;

    kem->encapsulate(ciphertext_out->bytes, &ciphertext_out->len,
                     shared_secret_out, public_key->bytes, public_key->len);
    OQS_KEM_free(kem);
    return FORETIAS_OK;
}

ForetiasResult foretias_mlkem_decapsulate(
    const ForetiasSecretKeyVar* secret,
    const ForetiasSecretKeyVar* ciphertext,
    uint8_t*                    shared_secret_out)
{
    OQS_KEM *kem = OQS_KEM_new(OQS_KEM_algorithm_MLKEM_768);
    if (!kem) return FORETIAS_ERR_INTERNAL;

    int rc = kem->decapsulate(shared_secret_out,
                              ciphertext->bytes, ciphertext->len,
                              secret->bytes, secret->len);
    OQS_KEM_free(kem);
    return rc == 0 ? FORETIAS_OK : FORETIAS_ERR_BAD_SIG;
}
```

### 4.2 Rust Wrapper (`core/kem_mlkem.rs`)

Wraps the ML-KEM C functions with safe Rust types.

### 4.3 Noise_XX Integration

ML-KEM replaces the ECDH step in the Noise_XX handshake. The Noise state machine remains identical; only the Diffie-Hellan function changes:

```c
// In noise_xx.c — ECDH step becomes algorithm-selectable:
static ForetiasResult noise_ecdh_step(
    ForetiasNoiseState* state,
    const uint8_t*      local_private,
    size_t              local_priv_len,
    const uint8_t*      remote_public,
    size_t              remote_pub_len,
    uint8_t*            shared_secret_out)
{
    if (state->kem == FORETIAS_KEM_NOISE_XX) {
        return noise_ecdh_x25519(local_private, remote_public, shared_secret_out);
    } else if (state->kem == FORETIAS_KEM_MLKEM_768) {
        // ML-KEM encapsulation with remote public key
        ForetiasSecretKeyVar ct = {0};
        ForetiasResult rc = foretias_mlkem_encapsulate(
            &(ForetiasPubKeyVar){.bytes = remote_public, .len = remote_pub_len},
            &ct, shared_secret_out);
        // Store ciphertext for verification
        memcpy(state->last_kem_ct, ct.bytes, ct.len);
        state->last_kem_ct_len = ct.len;
        return rc;
    }
    return FORETIAS_ERR_UNSUPPORTED;
}
```

### 4.4 Testing

| Test | Description |
|------|-------------|
| `mlkem_keygen_valid` | Generate keypair, check sizes |
| `mlkem_encapsulate_decapsulate_roundtrip` | Shared secrets match |
| `mlkem_wrong_secret_fails` | Decapsulate with wrong key produces garbage |
| `noise_handshake_with_mlkem` | Full Noise_XX handshake using ML-KEM ECDH |

---

## PART 5 — CONFIGURATION

### 5.1 `NodeConfig` Extensions

```rust
pub struct NodeConfig {
    // ... existing fields ...
    pub signature_algorithm: SignatureAlgorithm,
    pub kem_algorithm:       KemAlgorithm,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            signature_algorithm: SignatureAlgorithm::SPHINCS_SHA256_128S,
            kem_algorithm:       KemAlgorithm::NOISE_XX,
            // ...
        }
    }
}
```

### 5.2 `config/default.toml` Extensions

```toml
[crypto]
signature_algorithm = "SPHINCS+-SHA256/128s"
kem_algorithm       = "Noise-XX"
```

### 5.3 CLI Flag Extensions

```
foretias serve --sig-algo SPHINCS+-SHA256/128s --kem-algo Noise-XX
```

---

## PART 6 — IMPLEMENTATION PHASES & MILESTONES

### Phase 1: Refactoring for Multi-Algorithm Capability

**Goal:** Make all types, functions, and traits algorithm-agnostic. No PQ algorithms yet — just the scaffolding.

| Sub-task | Files | Description |
|----------|-------|-------------|
| 1.1 | `foretias_core.h` | Add `ForetiasSignatureAlgorithm`, `ForetiasKemAlgorithm` enums, variable-size types, helper functions |
| 1.2 | `algorithms.c` / `.h` | Implement `foretias_sig_algorithm_id()`, size helpers, `ACCEPTED_*` lists |
| 1.3 | `Cargo.toml` | Add `cmake = "0.1"` build dependency |
| 1.4 | `types.rs` | Replace fixed-size aliases with `Vec<u8>` types |
| 1.5 | `crypto_server/mod.rs` | Extend `CryptoServer` trait with `sign_with()`, `verify_with()`, `signature_algorithm()` |
| 1.6 | `foretias/tick.rs` | Add `signature_algorithm: String` to `Foretis` and `TickRecord` |
| 1.7 | `foretias/tick.rs` | Update `stamp()`, `verify()`, `verify_pair()` for algorithm-aware operations |
| 1.8 | `config.rs` | Add `signature_algorithm` / `kem_algorithm` fields to `NodeConfig` |
| 1.9 | `error.rs` | Add `AlgorithmMismatch` error variant |
| 1.10 | All tests | Update existing tests to work with new types |

**Acceptance:** All existing tests pass with Ed25519. Types accept variable-length keys/sigs. No PQ algorithms yet.

### Phase 2: SPHINCS+ Implementation

| Sub-task | Files | Description |
|----------|-------|-------------|
| 2.1 | `build.rs` | Clone and build liboqs (CMake, static link) |
| 2.2 | `foretias_core.h` | Add SPHINCS+ API declarations |
| 2.3 | `signing_sphincs.c` / `.h` | C11 wrapper over liboqs SPHINCS+ SHA256/128s |
| 2.4 | `bindgen` | Regenerate bindings with SPHINCS+ functions |
| 2.5 | `signing_sphincs.rs` | Rust safe wrapper for SPHINCS+ |
| 2.6 | `software.rs` | Add SPHINCS+ to `SoftwareCryptoServer::generate()` |
| 2.7 | `software.rs` | Implement `sign_with()` / `verify_with()` for SPHINCS+ |
| 2.8 | Tests | Full SPHINCS+ test matrix (Part 2.4) |

**Acceptance:** `stamp()` with SPHINCS+ produces valid `Foretis` with `signature_algorithm = "SPHINCS+-SHA256/128s"`. `verify()` correctly resolves and verifies. All Ed25519 tests still pass.

### Phase 3: Dilithium Implementation

| Sub-task | Files | Description |
|----------|-------|-------------|
| 3.1 | `foretias_core.h` | Add Dilithium API declarations |
| 3.2 | `signing_dilithium.c` / `.h` | C11 wrapper over liboqs Dilithium3 |
| 3.3 | `bindgen` | Regenerate bindings |
| 3.4 | `signing_dilithium.rs` | Rust safe wrapper |
| 3.5 | `software.rs` | Add Dilithium to `SoftwareCryptoServer` |
| 3.6 | Tests | Full Dilithium test matrix |

**Acceptance:** Dilithium3 works end-to-end through stamp/verify. Calendar stores Dilithium pubkeys and signatures correctly.

### Phase 4: ML-KEM Implementation

| Sub-task | Files | Description |
|----------|-------|-------------|
| 4.1 | `foretias_core.h` | Add ML-KEM API declarations |
| 4.2 | `kem_mlkem.c` / `.h` | C11 wrapper over liboqs ML-KEM-768 |
| 4.3 | `noise_xx.c` | Add KEM-selectable ECDH step |
| 4.4 | `bindgen` | Regenerate bindings |
| 4.5 | `kem_mlkem.rs` | Rust safe wrapper |
| 4.6 | `noise.rs` | Rust Noise state machine — KEM selection |
| 4.7 | Tests | ML-KEM + Noise handshake tests |

**Acceptance:** ML-KEM-768 works for key exchange. Noise_XX handshake with ML-KEM ECDH completes correctly.

### Phase 5: Integration Sanity Tests

**Test all algorithm combinations:**

| Signing | KEM | Test |
|---------|-----|------|
| SPHINCS+ | Noise-XX | Default configuration — stamp/verify |
| SPHINCS+ | ML-KEM-768 | PQ signing + PQ key exchange |
| Dilithium3 | Noise-XX | Dilithium signing + classical KEM |
| Dilithium3 | ML-KEM-768 | Full PQ configuration |
| SPHINCS+ | Noise-XX | Two-node mutual attestation |
| Dilithium3 | Noise-XX | Two-node mutual attestation |

---

## PART 7 — LIBOQS BUILD DETAILS

### 7.1 liboqs Version

Target: **liboqs 0.10.x** (current stable series at time of writing). Pin to a specific commit hash in `build.rs`.

### 7.2 Build Configuration

```cmake
# In build.rs:
cmake::configure()
    .define("oqs_enable_open_ssl", "OFF")
    .define("oqs_enable_s2n", "OFF")
    .define("enable_test", "OFF")
    .define("enable_benchmark", "OFF")
    .define("enable_sample", "OFF")
    .define("ENABLE_EXPERIMENTAL", "OFF")
    .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
    .define("CMAKE_BUILD_TYPE", "Release")
    .out_dir_binding("foretias_core")
```

### 7.3 liboqs as Submodule or Vendored

**Option A: Git submodule** (recommended for dev)
```
git submodule add https://github.com/open-quantum-safe/liboqs p2p/core/deps/liboqs
```

**Option B: Vendor at build time** (for CI/clean builds)
```rust
// In build.rs:
if !Path::new("deps/liboqs").exists() {
    std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--branch", "v0.10.x",
               "https://github.com/open-quantum-safe/liboqs.git",
               "deps/liboqs"])
        .status()
        .expect("failed to clone liboqs");
}
```

---

## PART 8 — PERFORMANCE CHARACTERISTICS

| Algorithm | KeyGen | Sign | Verify | PubKey | Signature |
|-----------|--------|------|--------|--------|-----------|
| Ed25519 | ~50μs | ~50μs | ~50μs | 32 B | 64 B |
| SPHINCS+ SHA2-128s-simple | ~5ms | ~8ms | ~4ms | 32 B | 7856 B |
| Dilithium3 | ~300μs | ~400μs | ~600μs | 1952 B | 3309 B |
| ML-KEM-768 | ~100μs | ~200μs (enc) | ~300μs (dec) | 1184 B | 1088 B (ct) |

**Impact on Foretias:**
- SPHINCS+ signatures are ~256x larger than Ed25519. Calendar storage grows accordingly.
- SPHINCS+ signing is ~160x slower. Chronomatter tick rate unaffected (key rotation is independent of stamp latency).
- For `chronon_ns = 60s`, signing latency is negligible.
- Calendar JSON file with SPHINCS+ will be significantly larger — plan for ~20KB per tick record.

---

## APPENDIX A — ALGORITHM ID MAPPING

```rust
impl SignatureAlgorithm {
    pub fn to_id_string(&self) -> &'static str {
        match self {
            SignatureAlgorithm::ED25519          => FORETIAS_SIG_ID_ED25519,
            SignatureAlgorithm::SPHINCS_SHA256_128S => FORETIAS_SIG_ID_SPHINCS_128S,
            SignatureAlgorithm::DILITHIUM3       => FORETIAS_SIG_ID_DILITHIUM3,
        }
    }

    pub fn from_id_string(id: &str) -> Result<Self, CryptoError> {
        match id {
            FORETIAS_SIG_ID_ED25519          => Ok(Self::ED25519),
            FORETIAS_SIG_ID_SPHINCS_128S     => Ok(Self::SPHINCS_SHA256_128S),
            FORETIAS_SIG_ID_DILITHIUM3       => Ok(Self::DILITHIUM3),
            _ => Err(CryptoError::UnknownAlgorithm(id.to_string())),
        }
    }
}
```

## APPENDIX B — CALENDAR JSON FORMAT (POST-REFACTOR)

```json
{
  "tbid": "hex-encoded UUID",
  "tbn": "Time Family abc123...",
  "signature_algorithm": "SPHINCS+-SHA256/128s",
  "ticks": [
    {
      "tick_number": 1712345678000000000,
      "signature_algorithm": "SPHINCS+-SHA256/128s",
      "public_key": "hex-encoded 32-byte SPHINCS+ public key",
      "forward_foretis": {
        "tick_number": ...,
        "signature": "hex-encoded 7856-byte signature (SPHINCS+)",
        "signature_algorithm": "SPHINCS+-SHA256/128s",
        ...
      },
      "backward_foretis": { ... },
      "aa_nonce": "hex-encoded 16 bytes"
    }
  ]
}
```

---

# END OF PQC INTEGRATION SPECIFICATION

(@human — this spec covers the complete refactoring and implementation path. Phase 1 is pure scaffolding (types, enums, trait extensions). Phase 2 adds SPHINCS+ as the default. Phases 3-4 add optional algorithms. Phase 5 validates all combinations. The C11 core is the integration point for liboqs — no Rust binding crates needed.)
