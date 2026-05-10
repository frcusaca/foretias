# Rust Code Cleanup Plan

**Generated:** 2026-05-07
**Based on:** AGENTS.md Rust principles (lines 363-952)
**Scope:** `core-engine/src/`, `foretias-node/src/`, `foretias-python/src/`

---

## Pre-conditions
- All Rust tests must pass before starting: `cd p2p && cargo test --workspace`
- No broken tests allowed during development (AGENTS.md rule)

---

## Phase 1: Security Fixes (Immediate)

### H1: Secret Key Exposure — `SoftwareCryptoServer` public fields
**Problem:** PQC secret keys exposed as `pub` fields. Any code with access to `SoftwareCryptoServer` can read raw secret material.
**Fix:** Make `sphincs_secret_key`, `dilithium_secret_key`, `mlkem_secret_key` private. Add `Zeroizing` wrapper if not already present.
**Files:** `p2p/core-engine/src/crypto_server/software.rs`

### H2: `SharedSecret` Clone without Zeroize
**Problem:** `#[derive(Clone)]` on `SharedSecret` produces a non-zeroized copy, defeating the `Drop` zeroization.
**Fix:** Remove `Clone` derive. If cloning is genuinely needed, implement it explicitly with a clear warning.
**Files:** `p2p/core-engine/src/crypto_server/mod.rs`

### H5: Panic in Network Protocol Path
**Problem:** `assert!(session.is_complete(), ...)` in `noise_handshake()` can panic during production network operations.
**Fix:** Replace with `Err(CryptoError::Internal("handshake incomplete"))`.
**Files:** `p2p/core-engine/src/noise.rs`

### H4: Missing SAFETY Comments on `unsafe` Blocks
**Problem:** ~25 `unsafe` blocks and 2 `unsafe impl Send/Sync` lack safety invariant comments.
**Fix:** Add `// SAFETY:` comments explaining the invariant each block upholds.
**Files:** `core/identity.rs`, `core/signing.rs`, `core/hashing.rs`, `core/rng.rs`, `noise.rs`, `crypto_server/kem_mlkem.rs`, `crypto_server/signing_dilithium.rs`, `crypto_server/signing_sphincs.rs`

---

## Phase 2: Error Handling Reliability

### M1: `unwrap()` on `NonNull::new()` in FFI Wrappers
**Problem:** Redundant `.unwrap()` calls after explicit null checks in `PrivKeyHandle::generate()`, `from_seed()`, `NoiseSession::new()`.
**Fix:** Replace with explicit error paths (the null check already guards these, but principle demands no unwrap).
**Files:** `core/identity.rs:44,56`, `noise.rs:94`

### M2: `unwrap()` in `Chronomatter` internal methods
**Problem:** `self.keypair_pub(kp_idx).unwrap()` and `.get(kp_idx).unwrap()` in `create_foretis()` and `daemon_tick()`.
**Fix:** Replace with `.ok_or_else(|| NodeError::Internal(...))?`.
**Files:** `chronomatter/mod.rs:245,251,347`

### M3: `unwrap()` in `foretias-node/src/main.rs`
**Problem:** `SystemTime::now().duration_since()` unwraps; `Multiaddr` parse unwrap.
**Fix:** Proper error propagation.
**Files:** `foretias-node/src/main.rs:263,360`

---

## Phase 3: Architecture Improvements

### H3: Clock Injection
**Problem:** Direct `SystemTime::now()` in `stamp()` (tick.rs) and `client_echo()` (main.rs) makes testing non-deterministic.
**Fix:** Define `Clock` trait, inject into `stamp()` function signature.
**Files:** `foretias/tick.rs`, `chronomatter/mod.rs`, `foretias-node/src/main.rs`

### M6: Split `CryptoServer` Trait
**Problem:** 22-method mega-trait mixing signing, verification, hashing, ECDH, sealing, RNG, FROST, backend proof.
**Fix:** Split into focused traits: `Signer`, `Verifier`, `Hasher`, `KeyExchange`, `Sealer`, `RngSource`.
**Files:** `crypto_server/mod.rs` + all implementors and call sites (~50 sites)

### M4: Type Aliases → Newtypes
**Problem:** `type Tbid = [u8; 16]`, `type SignatureBytes = Vec<u8>`, `type PublicKeyBytes = Vec<u8>` allow semantic confusion at compile time.
**Fix:** Convert to newtype structs with constructors: `struct Tbid([u8; 16])`, `struct SignatureBytes(Vec<u8>)`, etc.
**Files:** `foretias/types.rs` + ~30 dependent files

### M5: Checked Constructors for Domain Types
**Problem:** `TickRecord` and `Foretis` are plain structs with public fields — invalid states are representable.
**Fix:** Add `TickRecord::new()` and `Foretis::new()` with validation. Make fields private.
**Files:** `foretias/tick.rs` + all construction sites

---

## Phase 4: Style Polish

### L1: Remove `#![allow(missing_docs)]`
**Fix:** Remove crate-level doc suppression; add doc comments to public items.
**Files:** `core-engine/src/lib.rs`, `foretias-node/src/lib.rs`, `foretias-python/src/lib.rs`

### L3: Serialization Strictness
**Fix:** Add explicit validation pass before trusting deserialized network data.
**Files:** `foretias-node/src/main.rs:587`

### L4: Error Specificity in Noise I/O
**Fix:** Map I/O errors to specific variants instead of blanket `Internal(-99)`.
**Files:** `noise.rs:252-270`

### L5: Lint Policy Alignment
**Fix:** Align `unsafe_op_in_unsafe_fn` lint policy across crates.
**Files:** `foretias-python/src/lib.rs`

---

## Execution Order
1. Verify baseline: `cargo test --workspace` — all 111 tests pass
2. Phase 1 (H1-H5, H4) — quick security wins, ~2-3 hours
3. Phase 2 (M1-M3) — unwrap removal, ~1 hour
4. Phase 3 (H3, M6, M4, M5) — structural improvements, ~12-16 hours
5. Phase 4 (L1-L5) — style polish, ~4-6 hours
6. Final verification: `cargo test --workspace` — all tests pass

**Total estimated effort: 14-25 hours**
