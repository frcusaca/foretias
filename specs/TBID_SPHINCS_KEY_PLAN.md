# TBID as Dual-Key Identity (Ed25519 + SLH-DSA-SHA2-256f) — Implementation Plan

**Parent SPEC:** `TBID_SPHINCS_KEY_SPEC.md`
**Status:** Plan — ready for implementation
**Date:** 2026-05-08
**Scope:** Change `Tbid` from `[u8; 16]` to OOP struct (96B), generate from dual-key keypair, sign genesis tick
**Worktree:** `FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/TBID_SPHINCS_KEY_3580`
**Branch:** `tbid-dual-key-v1`

---

## READING ORDER

1. Read `TBID_SPHINCS_KEY_SPEC.md` end to end
2. Re-read `p2p/core/include/foretias_core.h` (C11 constants, types, algorithm enum)
3. Re-read `p2p/core/src/signing_sphincs.c` (existing 128s wrappers — add 256f alongside)
4. Re-read `p2p/core-engine/src/foretias/types.rs` (current Tbid alias)
5. Read this plan end to end before touching code

---

## Phase 0 — C11 Foundation (Single Source of Truth)

**Goal:** Add size constants, algorithm enum, 256f wrappers, and TBID combiner to C11 core.

### Task 0.1 — Add C11 Constants & Algorithm Enum
- **File:** `p2p/core/include/foretias_core.h`
- Add `FORETIAS_SIG_SLH_DSA_SHA2_256F = 4` to `ForetiasSignatureAlgorithm` enum
- Add `FORETIAS_SIG_ID_SLH_DSA_SHA2_256F` string define
- Add TBID V1 size constants (`FORETIAS_TBID_V1_*`)
- Bump `FORETIAS_SIG_MAX_SIG_BYTES` from 8192 → 65536
- Bump `FORETIAS_SIG_MAX_PUBKEY_BYTES` from 2048 → 2048 (unchanged — 64B fits)
- Bump `FORETIAS_SIG_MAX_SECRET_BYTES` from 4096 → 4096 (unchanged — 128B fits)
- Add TBID combiner type declarations and function prototypes
- Update `foretias_sig_pubkey_bytes()`, `foretias_sig_secret_bytes()`, `foretias_sig_signature_bytes()` to handle new variant
- **Verification:** `cmake --build build` compiles cleanly

### Task 0.2 — Add SLH-DSA-SHA2-256f C11 Wrappers
- **File:** `p2p/core/src/signing_sphincs.c`
- Add `#ifdef OQS_ENABLE_SIG_sphincs_sha2_256f_simple` block with:
  - `foretias_sphincs_sha2_256f_keypair()`
  - `foretias_sphincs_sha2_256f_sign()`
  - `foretias_sphincs_sha2_256f_verify()`
- Pattern: copy existing 128s functions, replace `sha2_128s` → `sha2_256f`
- **Verification:** C11 compiles, CMake includes `OQS_ENABLE_SIG_sphincs_sha2_256f_simple`

### Task 0.3 — Create C11 TBID Combiner
- **File:** `p2p/core/src/signing_tbid.c` (NEW)
- Implement:
  - `foretias_tbid_v1_keypair()` — generates Ed25519 + SLH-DSA keypairs, fills both structs
  - `foretias_tbid_v1_sign()` — signs with both, concatenates Ed25519_SIG ‖ SLH-DSA_SIG
  - `foretias_tbid_v1_verify()` — splits signature, verifies both, both must pass
  - `foretias_tbid_v1_secret_zeroize()` — zeroizes all secret material
- Use C11 constants exclusively (no hardcoded sizes)
- **Verification:** C11 compiles cleanly

### Task 0.4 — Create C11 TBID Test
- **File:** `p2p/core/tests/test_tbid.c` (NEW)
- Tests:
  - Key generation produces valid 96B public key
  - Sign → verify roundtrip passes
  - Tampered message fails verification
  - Tampered signature fails verification
  - Secret zeroization clears all bytes
  - Ed25519 portion verifies independently
  - SLH-DSA portion verifies independently
- **Verification:** `ctest` passes

### Task 0.5 — Update CMakeLists.txt
- **File:** `p2p/core/CMakeLists.txt`
- Add `signing_tbid.c` to `foretias_core` sources
- Add `test_tbid.c` to test sources
- Verify `OQS_ENABLE_SIG_sphincs_sha2_256f_simple` is enabled in CMake config
- **Verification:** `cmake --build build` includes new files

---

## Phase 1 — Rust Types & Bindings

**Goal:** Replace `Tbid` alias with OOP struct, constants from C11 via bindgen, add 256f API.

### Task 1.1 — Change `Tbid` to OOP Struct
- **File:** `core-engine/src/foretias/types.rs`
- Replace `pub type Tbid = [u8; 16];` with OOP struct:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
  pub struct Tbid {
      pub ed25519_pub: [u8; 32],
      pub slh_dsa_pub: [u8; 64],
  }
  ```
- Add `raw_bytes()`, `from_raw()`, `ed25519_public_key()`, `slh_dsa_public_key()`, `to_hex()` methods
- Import size constants from `bindings` (bindgen from C11 header)
- **Verification:** `cargo check -p foretias-core` will surface all downstream errors

### Task 1.2 — Fix All Hardcoded `[u8; 16]` TBID Sites
After Task 1.1, `cargo check` will list every file/line that needs updating. Fix each:

| File | What changes |
|------|-------------|
| `tick.rs:81,100,134,143` | `Foretis.tbid`, `new()` param, `CalendarLookup::tbid()`, `stamp()` param → `Tbid` struct |
| `calendar.rs:12,16,23,175` | `Calendar.tbid`, `stamp_tbid`, `new()` param, `CalendarLookup::tbid()` → `Tbid` struct |
| `foretias-node/calendar/mod.rs:22,99` | `new()` param, `CalendarLookup::tbid()` → `Tbid` struct |
| `foretias-node/main.rs:733` | `CalendarLookup::tbid()` → `Tbid` struct |
| `foretias-node/server/mod.rs:120` | `get_tbid()` → `Tbid` struct |
| `foretias-node/communerd/mod.rs:488,598` | `publish_*` params → `Tbid` struct |
| `foretias-node/communerd/p2p/tbid_handshake.rs` | Uses `Tbid` alias (auto-fixes), test fixtures need update |
| `foretias-python/src/lib.rs:305,318,383,528` | `PyTimeFamily.tbid` → `Tbid` struct |
| `foretias-java/` | `generateTbid()` → 96 bytes |

- **Verification:** `cargo check --workspace` clean

### Task 1.3 — Add `SignatureAlgorithm::SLH_DSA_SHA2_256F`
- **File:** `core-engine/src/foretias/types.rs`
- Add new enum variant with `to_id_string()`, `from_id_string()`, `pubkey_max_bytes()` (64), `signature_max_bytes()` (49856)
- **Verification:** `cargo check -p foretias-core` clean

### Task 1.4 — Add SLH-DSA-SHA2-256f Rust Wrappers
- **File:** `core-engine/src/crypto_server/signing_sphincs.rs`
- Add:
  - `slh_dsa_sha2_256f_keypair() -> Result<([u8; 64], Vec<u8>), CryptoError>`
  - `slh_dsa_sha2_256f_sign(&secret, &message) -> Result<Vec<u8>, CryptoError>`
  - `slh_dsa_sha2_256f_verify(&public_key, &message, &signature) -> Result<bool>, CryptoError>`
- Pattern: copy existing 128s functions, route through C11 `sha2_256f` wrappers
- **Verification:** `cargo check -p foretias-core` clean

### Task 1.5 — Create Rust TBID Combiner Module
- **File:** `core-engine/src/crypto_server/signing_tbid.rs` (NEW)
- Implement:
  - `TbidSecret` struct with `Zeroizing` wrappers
  - `generate_tbid_keypair() -> Result<(Tbid, TbidSecret), CryptoError>`
  - `TbidSecret::sign(&self, msg) -> Result<Vec<u8>, CryptoError>`
  - `tbid_verify(tbid, msg, sig) -> Result<bool>, CryptoError>`
- Uses C11 constants via `bindings` for all sizes
- **Verification:** `cargo check -p foretias-core` clean

---

## Phase 2 — Genesis Tick Signing

**Goal:** Sign the genesis tick (tick 1) with the TBID dual-key keypair.

### Task 2.1 — Add `genesis_signature` and `tb_version` Fields to TickRecord
- **File:** `core-engine/src/foretias/tick.rs`
- Add:
  ```rust
  #[serde(default)]
  pub genesis_signature: Vec<u8>,
  #[serde(default = "default_tb_version")]
  pub tb_version: u32,
  ```
- Update `TickRecord::new()` — no change (defaults to empty)
- **Verification:** `cargo check -p foretias-core` clean

### Task 2.2 — Sign Genesis Tick in Chronomatter
- **File:** `core-engine/src/chronomatter/mod.rs`
- Replace UUID v4 TBID generation with dual-key keypair generation
- Store `TbidSecret` in `Chronomatter` struct
- On tick 1: build genesis blob (`tbid.raw_bytes() || tick_number || per_tick_public_key`), sign with `TbidSecret`
- **Verification:** Unit test — genesis tick has non-empty `genesis_signature`

### Task 2.3 — Verify Genesis Signature
- **File:** `core-engine/src/foretias/tick.rs`
- Add:
  ```rust
  pub fn verify_genesis_signature(
      tbid: &Tbid,
      tick_record: &TickRecord,
  ) -> Result<bool, NodeError>
  ```
- Rebuild blob, verify both Ed25519 and SLH-DSA portions
- **Verification:** Unit tests — valid/tampered/wrong TBID cases

### Task 2.4 — Wire Genesis Verification into `verify()` and `integrity_check()`
- **File:** `core-engine/src/foretias/tick.rs` (verify), `calendar.rs` (integrity_check)
- If `tick_number == 1` and `genesis_signature` non-empty, verify against `foretis.tbid`
- **Verification:** Unit tests — verify tick 1 with/without genesis sig

---

## Phase 3 — Communerd & Calendar Upgrades

**Goal:** Calendar and Communerd both use the same `Tbid` struct (already shared via type alias/struct).

### Task 3.1 — Update Communerd TBID Handshake
- **File:** `foretias-node/src/communerd/p2p/tbid_handshake.rs`
- `TbidHandshake` uses `Tbid` type — auto-updates to OOP struct
- Update capacity/offset math (96B instead of 16B)
- Update test fixtures
- **Verification:** `cargo test -p foretias-node -- tbid_handshake` passes

### Task 3.2 — Update Communerd DHT Publishing
- **File:** `foretias-node/src/communerd/mod.rs`
- `publish_tbid_index` and `refresh_self_registration` params → `Tbid` struct
- `hex::encode(tbid)` → `tbid.to_hex()` or `hex::encode(tbid.raw_bytes())`
- **Verification:** `cargo test -p foretias-node -- communerd` passes

### Task 3.3 — Update Calendar Module
- **File:** `foretias-node/src/calendar/mod.rs`
- `new()` param and `CalendarLookup::tbid()` → `Tbid` struct
- **Verification:** `cargo check -p foretias-node` clean

---

## Phase 4 — PyO3 Bindings & Test Fixtures

**Goal:** Update all language bindings and test data.

### Task 4.1 — Update PyO3 Python Bindings
- **File:** `foretias-python/src/lib.rs`
- `PyTimeFamily.tbid` → use `Tbid` struct (serializes to dict or hex string)
- `get_tbid()` validation → "tbid must be 96 bytes"
- Generation → dual-key keypair
- **Verification:** `cargo test -p foretias-python` passes

### Task 4.2 — Update Rust Test Fixtures
- **Files:** All test modules with TBID literals
- Replace `[X; 16]` → use `Tbid { ed25519_pub: [X; 32], slh_dsa_pub: [X; 64] }`
- Consider helper: `fn test_tbid() -> Tbid { Tbid { ed25519_pub: [0xAB; 32], slh_dsa_pub: [0xAB; 64] } }`
- **Verification:** `cargo test --workspace` — all tests pass

### Task 4.3 — Update Python Shim Tests
- **File:** `tests/test_shim.py`
- Update any TBID size assertions
- **Verification:** `pytest tests/ -v` — 25 pass

### Task 4.4 — Update Java Bindings
- **Files:** `foretias-java/src/main/java/foretias/Crypto.java`, `Cli.java`
- `generateTbid()` → 96 bytes
- Help text → "96-byte TBID (hex)"
- **Verification:** `./build.sh` + `IntegrationTest` passes

---

## Phase 5 — Integration & CI

**Goal:** Verify entire build chain and CI pipeline.

### Task 5.1 — Full C11 Test
- **Command:** `cd p2p/core/build && ctest --output-on-failure`
- **Verification:** All pass including `test_tbid.c`

### Task 5.2 — Full Rust Workspace Test
- **Command:** `cd p2p && cargo test --workspace`
- **Verification:** 0 failures

### Task 5.3 — Python Shim Test
- **Command:** `cd /home/hcbusy/webhash/foretias && python -m pytest tests/ -v`
- **Verification:** 25 pass

### Task 5.4 — Python Bindings Test
- **Command:** `cd p2p/foretias-python && pytest tests/python/ -v`
- **Verification:** All pass

### Task 5.5 — CI Pipeline Update
- **File:** `.github/workflows/ci.yml`
- Verify SLH-DSA-SHA2-256f is enabled in CMake (check `OQS_ENABLE_SIG_sphincs_sha2_256f_simple`)
- **Verification:** CI runs green

---

## Milestone Checklist

```
Phase 0 — C11 Foundation
[ ] M1  C11 constants & algorithm enum added (foretias_core.h)
[ ] M2  SLH-DSA-SHA2-256f C11 wrappers added (signing_sphincs.c)
[ ] M3  C11 TBID combiner created (signing_tbid.c)
[ ] M4  C11 TBID tests pass (test_tbid.c)
[ ] M5  CMakeLists.txt updated

Phase 1 — Rust Types & Bindings
[ ] M6  Tbid OOP struct (types.rs)
[ ] M7  All hardcoded [u8; 16] TBID sites fixed (cargo check clean)
[ ] M8  SignatureAlgorithm::SLH_DSA_SHA2_256F added
[ ] M9  SLH-DSA-SHA2-256f Rust wrappers (signing_sphincs.rs)
[ ] M10 Rust TBID combiner module (signing_tbid.rs)

Phase 2 — Genesis Tick Signing
[ ] M11 genesis_signature + tb_version fields added to TickRecord
[ ] M12 Chronomatter signs genesis tick with TBID key
[ ] M13 verify_genesis_signature() function
[ ] M14 Genesis verification wired into verify() and integrity_check()

Phase 3 — Communerd & Calendar Upgrades
[ ] M15 Communerd TBID handshake updated
[ ] M16 Communerd DHT publishing updated
[ ] M17 Calendar module updated

Phase 4 — Bindings & Test Fixtures
[ ] M18 PyO3 bindings updated
[ ] M19 Rust test fixtures updated
[ ] M20 Python shim tests updated
[ ] M21 Java bindings updated

Phase 5 — Integration & CI
[ ] M22 C11 tests pass (including test_tbid.c)
[ ] M23 cargo test --workspace passes
[ ] M24 pytest tests/ passes (25 tests)
[ ] M25 CI pipeline green
[ ] M26 Merge tbid-dual-key-v1 to alpha
```
