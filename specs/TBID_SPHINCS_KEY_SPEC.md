# TBID as Dual-Key Identity (Ed25519 + SLH-DSA-SHA2-256f) — Specification

**Version:** v0.3
**Status:** Draft — ready for implementation
**Date:** 2026-05-08
**Prerequisites:** Ed25519 via libsodium (C11), SLH-DSA-SHA2-256f via liboqs (`SPHINCS+-SHA2-256f-simple`)

---

## 1. Overview

TBID (Time Being ID) is the application-layer identity of a Foretias Calendar. Currently, TBID is a random 16-byte value (UUID v4). This specification transitions TBID to a **dual-key public key identity**: Ed25519 (fast classical) + SLH-DSA-SHA2-256f (post-quantum, NIST Level 5), making every TBID a verifiable cryptographic identity with both classical and quantum-resistant security.

**Time Being Version (tb_version):** TimeBeings are versioned to allow evolution of the TBID format. This specification defines **tb_version 1.0** as the dual-key format.

**Notation:** The `‖` symbol denotes **byte-level concatenation** (append), NOT bitwise OR. For example `A ‖ B` means the bytes of A followed by the bytes of B.

**Terminology:** "TBID" refers to the capability to sign using the TBID's private keys and verify using the TBID's public keys. The TBID *is* the concatenated public keys stored as an OOP struct.

---

## 2. Algorithm Pair

**tb_version 1.0** uses two algorithms in parallel:

| Algorithm | NIST / FIPS Name | liboqs String | Purpose | PK | SK | Signature |
|-----------|-----------------|---------------|---------|-----|------|-----------|
| Ed25519 | — | `Ed25519` | Fast classical signatures | 32B | 32B | 64B |
| SLH-DSA-SHA2-256f | FIPS 205 Level 5 | `SPHINCS+-SHA2-256f-simple` | Post-quantum signatures | 64B | 128B | 49,856B |

**Combined sizes:**
- TBID public key: **96 bytes** (Ed25519 PK(32) ‖ SLH-DSA PK(64))
- TBID secret key: **160 bytes** (Ed25519 SK(32) ‖ SLH-DSA SK(128))
- TBID signature: **49,920 bytes** (Ed25519 SIG(64) ‖ SLH-DSA SIG(49,856))

Both algorithms are already integrated:
- Ed25519: C11 `signing_ed25519.c` via libsodium
- SLH-DSA-SHA2-256f: liboqs `SPHINCS+-SHA2-256f-simple` (needs new C11 wrapper in `signing_sphincs.c`)

---

## 3. TBID Structure (tb_version 1.0)

### 3.1 Rust OOP Design

```rust
/// Time Being ID — dual-key identity for tb_version 1.0.
/// Layout: Ed25519_PK(32) ‖ SLH-DSA-SHA2-256f_PK(64) = 96 bytes total.
///
/// The `‖` operator denotes byte-level concatenation (NOT bitwise OR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tbid {
    /// Ed25519 public key — fast verification (32 bytes).
    pub ed25519_pub: [u8; 32],
    /// SLH-DSA-SHA2-256f public key — quantum-resistant verification (64 bytes).
    pub slh_dsa_pub: [u8; 64],
}

impl Tbid {
    /// Raw 96-byte representation for wire format / storage.
    pub fn raw_bytes(&self) -> [u8; 96] { ... }

    /// Parse from raw 96-byte representation (ed25519 first, then slh_dsa).
    pub fn from_raw(bytes: [u8; 96]) -> Self { ... }

    /// Ed25519 public key for Ed25519-specific verification.
    pub fn ed25519_public_key(&self) -> &[u8; 32] { &self.ed25519_pub }

    /// SLH-DSA-SHA2-256f public key for SLH-DSA-specific verification.
    pub fn slh_dsa_public_key(&self) -> &[u8; 64] { &self.slh_dsa_pub }

    /// Hex-encoded representation for DHT keys and logging.
    pub fn to_hex(&self) -> String { ... }
}
```

### 3.2 C11 Constants (Single Source of Truth)

All size constants are defined in `foretias_core.h`. Rust code imports them via bindgen — **no duplicated literals**.

```c
/* TBID V1 size constants — single source of truth. */
#define FORETIAS_TBID_V1_ED25519_PUB_BYTES  32
#define FORETIAS_TBID_V1_SLH_DSA_PUB_BYTES  64
#define FORETIAS_TBID_V1_PUB_BYTES          96    /* 32 + 64 */
#define FORETIAS_TBID_V1_ED25519_SK_BYTES   32
#define FORETIAS_TBID_V1_SLH_DSA_SK_BYTES   128
#define FORETIAS_TBID_V1_SECRET_BYTES       160   /* 32 + 128 */
#define FORETIAS_TBID_V1_ED25519_SIG_BYTES  64
#define FORETIAS_TBID_V1_SLH_DSA_SIG_BYTES  49856
#define FORETIAS_TBID_V1_SIG_BYTES          49920 /* 64 + 49856 */
#define FORETIAS_TBID_V1_VERSION            1
```

Rust imports these as `bindings::FORETIAS_TBID_V1_PUB_BYTES`, etc.

### 3.3 C11 Structs

```c
typedef struct {
    ForetiasPubKey32 ed25519_pub;    /* first 32 bytes */
    uint8_t          slh_dsa_pub[64]; /* next 64 bytes */
} ForetiasTbidV1PubKey;
```

### 3.3 Private Key Storage (Rust)

```rust
/// Secret key material for a TBID (tb_version 1.0).
/// Both private keys are held together; zeroized on drop.
pub struct TbidSecret {
    ed25519_sk: Zeroizing<[u8; 32]>,
    slh_dsa_sk: Zeroizing<Vec<u8>>,  /* 128 bytes */
}

impl TbidSecret {
    /// Generate a fresh TBID keypair (both Ed25519 and SLH-DSA).
    pub fn generate() -> Result<(Tbid, Self), CryptoError> { ... }

    /// Sign a message with both algorithms.
    /// Returns Ed25519_SIG(64) ‖ SLH-DSA_SIG(49856) = 49,920 bytes.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> { ... }

    /// Ed25519 secret key for Ed25519-specific operations.
    pub fn ed25519_secret_key(&self) -> &[u8; 32] { &self.ed25519_sk }

    /// SLH-DSA-SHA2-256f secret key for SLH-DSA-specific operations.
    pub fn slh_dsa_secret_key(&self) -> &[u8] { &self.slh_dsa_sk }
}
```

### 3.4 Private Key Storage (C11)

```c
typedef struct {
    ForetiasPrivKey32 ed25519_sk;     /* 32 bytes */
    uint8_t           slh_dsa_sk[128]; /* 128 bytes */
    size_t            slh_dsa_sk_len; /* actual length for zeroing */
} ForetiasTbidV1SecretKey;
```

Both private keys are zeroized on destruction using `foretias_memzero()`.

---

## 4. Key Generation

TBID is generated by the Calendar's `CryptoServer` at construction time:

```rust
pub fn generate_tbid_keypair() -> Result<(Tbid, TbidSecret), CryptoError> {
    // Generate Ed25519 keypair
    let ed25519 = signing::ed25519_keypair()?;
    // Generate SLH-DSA-SHA2-256f keypair
    let (slh_dsa_pub, slh_dsa_sec) = signing_sphincs::slh_dsa_sha2_256f_keypair()?;
    // Construct OOP structs
    let tbid = Tbid {
        ed25519_pub: ed25519.public_key,
        slh_dsa_pub: slh_dsa_pub, // [u8; 64]
    };
    let secret = TbidSecret {
        ed25519_sk: Zeroizing::new(ed25519.secret_key),
        slh_dsa_sk: Zeroizing::new(slh_dsa_sec),
    };
    Ok((tbid, secret))
}
```

The TBID secret key is stored alongside the Calendar. The TBID public key (96 bytes as OOP struct) is the identifier exposed everywhere.

---

## 5. Signature Structure

A TBID signature is the concatenation of both algorithm signatures:

```
TBID_SIGNATURE = Ed25519_SIG (64 bytes) ‖ SLH-DSA-SHA2-256f_SIG (49,856 bytes)
                = 49,920 bytes total
```

Signing produces both signatures. Verification requires **both** to be valid.

```rust
impl TbidSecret {
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let ed25519_sig = signing::ed25519_sign(&self.ed25519_sk, message)?;
        let slh_dsa_sig = signing_sphincs::slh_dsa_sha2_256f_sign(&self.slh_dsa_sk, message)?;
        let mut combined = Vec::with_capacity(64 + 49_856);
        combined.extend_from_slice(&ed25519_sig);
        combined.extend_from_slice(&slh_dsa_sig);
        Ok(combined)
    }
}

pub fn tbid_verify(tbid: &Tbid, message: &[u8], signature: &[u8]) -> Result<bool, CryptoError> {
    if signature.len() != 49_920 {
        return Err(CryptoError::InvalidSignatureLength);
    }
    let ed25519_sig = &signature[..64];
    let slh_dsa_sig = &signature[64..];
    let ed25519_valid = signing::ed25519_verify(&tbid.ed25519_pub, message, ed25519_sig)?;
    let slh_dsa_valid = signing_sphincs::slh_dsa_sha2_256f_verify(&tbid.slh_dsa_pub, message, slh_dsa_sig)?;
    Ok(ed25519_valid && slh_dsa_valid)
}
```

---

## 6. Genesis Tick Signing

Only the **genesis tick (tick 1)** is signed by the TBID keypair. The genesis TickRecord carries a TBID signature that binds the genesis tick to the Calendar's identity.

**Genesis TickRecord structure (new fields only):**

```rust
pub struct TickRecord {
    // ... existing fields unchanged ...
    /// TBID-signature over the genesis tick identity blob.
    /// Present ONLY on tick 1. Verifies that this tick belongs to the TBID's Calendar.
    /// Format: Ed25519_SIG(64) ‖ SLH-DSA_SIG(49856) = 49,920 bytes total
    #[serde(default)]
    pub genesis_signature: Vec<u8>,
    /// Time Being version that produced this record.
    /// 0 = legacy (random TBID), 1 = dual-key TBID (tb_version 1.0)
    #[serde(default = "default_tb_version")]
    pub tb_version: u32,
}

fn default_tb_version() -> u32 { 0 }
```

**Genesis signature input:**
```
genesis_blob = tbid_raw_bytes (96B) || tick_number (8B, BE) || per_tick_public_key (variable)
genesis_sig  = tbid_secret.sign(genesis_blob)
             = ed25519_sign(ed25519_sk, genesis_blob) ‖ slh_dsa_sign(slh_dsa_sk, genesis_blob)
```

**Genesis verification:**
```
rebuilt_blob = foretis.tbid.raw_bytes() || tick_number || tick_record.public_key
tbid_verify(&foretis.tbid, &rebuilt_blob, &tick_record.genesis_signature)
```

This binds the genesis tick irrevocably to the Calendar's TBID identity. Any mirror or verifier can confirm the Calendar's origin by verifying the genesis signature against the TBID public key — no calendar lookup required.

---

## 7. C11 Combiner Interface

The C11 core provides a dedicated combiner module (`signing_tbid.c`) that wraps both Ed25519 and SLH-DSA operations:

```c
#define FORETIAS_TBID_V1_PUB_BYTES    96
#define FORETIAS_TBID_V1_ED25519_PUB  32
#define FORETIAS_TBID_V1_SLH_DSA_PUB  64
#define FORETIAS_TBID_V1_SIG_BYTES    49920  /* 64 + 49856 */
#define FORETIAS_TBID_V1_SECRET_BYTES 160

typedef struct {
    ForetiasPubKey32 ed25519_pub;     /* 32 bytes */
    uint8_t          slh_dsa_pub[64]; /* 64 bytes */
} ForetiasTbidV1PubKey;

typedef struct {
    ForetiasPrivKey32 ed25519_sk;     /* 32 bytes */
    uint8_t           slh_dsa_sk[128];/* 128 bytes */
    size_t            slh_dsa_sk_len;
} ForetiasTbidV1SecretKey;

typedef struct {
    uint8_t bytes[FORETIAS_TBID_V1_SIG_BYTES];
    size_t  len;
} ForetiasTbidV1Sig;

/* Generate a fresh dual-key TBID keypair */
ForetiasResult foretias_tbid_v1_keypair(
    ForetiasTbidV1SecretKey* secret_out,
    ForetiasTbidV1PubKey*    public_out
);

/* Sign with both algorithms; output = ed25519_sig(64) ‖ slh_dsa_sig(49856) */
ForetiasResult foretias_tbid_v1_sign(
    const ForetiasTbidV1SecretKey* secret,
    const uint8_t*                 msg,
    size_t                         msg_len,
    ForetiasTbidV1Sig*             sig_out
);

/* Verify both signatures; both must be valid */
ForetiasResult foretias_tbid_v1_verify(
    const ForetiasTbidV1PubKey*  public_key,
    const uint8_t*              msg,
    size_t                      msg_len,
    const ForetiasTbidV1Sig*    sig
);

/* Securely zeroize the secret key material */
void foretias_tbid_v1_secret_zeroize(ForetiasTbidV1SecretKey* secret);
```

### 7.1 SLH-DSA-SHA2-256f C11 Wrappers

New functions added to `signing_sphincs.c`:

```c
#ifdef OQS_ENABLE_SIG_sphincs_sha2_256f_simple
ForetiasResult foretias_sphincs_sha2_256f_keypair(
    ForetiasSecretKeyVar* secret_out,
    ForetiasPubKeyVar*    public_out
);
ForetiasResult foretias_sphincs_sha2_256f_sign(
    const ForetiasSecretKeyVar* secret,
    const uint8_t*            msg,
    size_t                    msg_len,
    ForetiasSigVar*           sig_out
);
ForetiasResult foretias_sphincs_sha2_256f_verify(
    const ForetiasPubKeyVar*  public_key,
    const uint8_t*            msg,
    size_t                    msg_len,
    const ForetiasSigVar*     sig
);
#endif
```

### 7.2 C11 Buffer Size Updates

`FORETIAS_SIG_MAX_SIG_BYTES` must be bumped to accommodate SLH-DSA-SHA2-256f signatures:

```c
#define FORETIAS_SIG_MAX_SIG_BYTES  65536  /* SLH-DSA-SHA2-256f signature (49,856) */
```

This affects `ForetiasSigVar` which is used for all variable-length signature buffers.

### 7.3 C11 Algorithm Enum Update

```c
typedef enum {
    FORETIAS_SIG_ED25519              = 1,
    FORETIAS_SIG_SPHINCS_SHA2_128S    = 2,
    FORETIAS_SIG_DILITHIUM3           = 3,
    FORETIAS_SIG_SLH_DSA_SHA2_256F    = 4,  /* NEW: NIST Level 5 */
} ForetiasSignatureAlgorithm;

#define FORETIAS_SIG_ID_SLH_DSA_SHA2_256F "SPHINCS+-SHA2-256f-simple"
```

---

## 8. Wire Format Changes

### 8.1 Foretis.tbid

The `Foretis` struct changes from a flat `[u8; 16]` to the OOP `Tbid` struct:

```diff
- pub tbid: [u8; 16],
+ pub tbid: Tbid,  /* OOP struct: ed25519_pub(32) + slh_dsa_pub(64) */
```

JSON serialization uses the struct's field names. Raw wire format uses `tbid.raw_bytes()` (96 bytes).

### 8.2 Calendar.tbid / stamp_tbid

```diff
- pub tbid: [u8; 16],
- pub stamp_tbid: [u8; 16],
+ pub tbid: Tbid,
+ pub stamp_tbid: Tbid,
```

### 8.3 DHT Keys

`PeerRegistrationRecord.tbid` remains a hex `String`. Key length changes from 32 hex chars to 192 hex chars (96 bytes × 2). DHT key paths accommodate the longer key.

### 8.4 Stamp Signature Input

```diff
- sig_input = tbid (16B) || tick_number (8B) || content
+ sig_input = tbid_raw_bytes (96B) || tick_number (8B) || content
```

This is a breaking wire-format change. Stamps created with 16-byte TBID cannot be verified after migration. **Mitigation:** this is a v0.2 breaking change — all calendars restart with new TBIDs.

---

## 9. Time Being Versioning

### 9.1 Version Field

| tb_version | TBID Format | Size | Notes |
|------------|-------------|------|-------|
| 0 | Random bytes | 16 | Legacy (UUID v4) |
| 1 | Dual-key OOP struct | 96 | This specification (Ed25519 ‖ SLH-DSA) |

### 9.2 Configuration

`NodeConfig` and `TimeFamilyConfig` include a `tb_version` field:

```rust
pub struct TimeFamilyConfig {
    // ... existing fields ...
    /// Time Being version (0 = legacy, 1 = dual-key TBID)
    pub tb_version: u32,
}
```

Default is `tb_version = 1` (dual-key). Explicit setting to `0` preserves legacy behavior for testing.

### 9.3 Version Negotiation

When nodes discover each other via DHT, they exchange `tb_version` as part of `PeerRegistrationRecord`. Mismatched versions are logged but do not prevent connection.

---

## 10. Impact Summary

| Area | Impact | Breaking? |
|------|--------|-----------|
| `types.rs` — `Tbid` type | `[u8; 16]` → OOP struct (96B) | Yes (struct shape change) |
| `SignatureAlgorithm` enum | Add `SLH_DSA_SHA2_256F` variant | No (new variant) |
| `Foretis.tbid` | Field shape changes | Yes (serde wire format) |
| `Calendar.tbid`, `stamp_tbid` | Field shape changes | Yes (serde wire format) |
| `stamp()` sig_input | 16B → 96B prefix | Yes (existing stamps invalid) |
| `PeerRegistrationRecord.tbid` | Hex string 32→192 chars | No (String type) |
| DHT keys | Longer hex path (192 chars) | No (no length limit) |
| `tbid_handshake.rs` | OOP struct replaces `[u8; 16]` | No (internal protocol) |
| `auto_attestation_blob` | Uses tbid as hex string | No (String, length agnostic) |
| PyO3 bindings | `PyTimeFamily.tbid` struct change | Yes (recompile) |
| Java bindings | `generateTbid()` 16B→96B | Yes (recompile) |
| Test fixtures | All TBID literals need update | No (tests update) |
| Persisted calendars | JSON has old tbid format | Yes (must discard) |
| C11 `FORETIAS_SIG_MAX_SIG_BYTES` | 8192 → 65536 | Yes (buffer sizes) |
| C11 algorithm enum | Add `SLH_DSA_SHA2_256F` | No (new variant) |

---

## 11. Migration Strategy

This is a **v0.2 breaking change**. No backward compatibility is maintained:

1. All existing persisted calendars are discarded (they used 16-byte random TBIDs)
2. All DHT records are re-registered (new TBID keys)
3. All Python/Java bindings are recompiled (new TBID sizes)
4. The `Tbid` OOP struct is the single source of truth — all hardcoded `[u8; 16]` TBID sites are replaced
5. `tb_version` defaults to 1 in all new configurations
6. C11 `FORETIAS_SIG_MAX_SIG_BYTES` bumped to 65536

**Calendar and Communerd share the same TBID.** Both use the `Tbid` struct from `core-engine/src/foretias/types.rs`. When the type changes, both are upgraded simultaneously.

---

## 12. Future Work (Out of Scope)

| Feature | Description |
|---------|-------------|
| Sporadic tick signing | Sign later ticks/periods using TBID key. Planned for calendar active mirroring. |
| tb_version 2.0 | Future evolution (e.g., different algorithm pair). Planned for active mirroring plan. |
| TBID signature in every Foretis | Embed a TBID-signature in every Foretis (not just genesis). Planned for active mirroring. |

---

## 13. Verification

A TBID can be verified against a Calendar's genesis tick:

1. Extract `foretis.tbid` (OOP struct with `ed25519_pub` and `slh_dsa_pub`)
2. Look up tick 1 from the Calendar
3. Rebuild `genesis_blob = tbid.raw_bytes() || 1 || tick1.public_key`
4. Verify Ed25519: `ed25519_verify(tbid.ed25519_pub, genesis_blob, sig[..64])`
5. Verify SLH-DSA: `slh_dsa_verify(tbid.slh_dsa_pub, genesis_blob, sig[64..])`
6. Both must be valid

This requires **no calendar key lookup** — the TBID struct itself contains both verification keys.
