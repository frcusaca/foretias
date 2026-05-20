# HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md

**Date:** 2026-05-20
**Application:** Foretias v0.3+
**Companion spec:** `specs/HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md`
**Goal:** Bring the Foretias codebase into **maximum compliance** with the Five Hard Requirements (HR-1 through HR-5) and every REQ-Z\* in the spec's Master Requirements Index (§1.7).

This is a `*_PLAN.md` per `AGENTS.md` conventions: checkboxes are progress tracked; completed checkboxes carry a timestamp on the next indented line.

---

## How to Read and Work This Plan

- Each **Phase** is a dependency-ordered chunk of work. Phases that don't share files can run in parallel; the dependency note at the top of each phase says what blocks it.
- Each **Task** within a phase is a code change small enough to commit individually. Each task lists the **REQ-Z\* IDs it satisfies**, the **HR it serves**, the **files it touches**, the **acceptance test** that proves completion, and the **estimated effort**.
- Sub-tasks may be added under a task if scoping is found to be incorrect (per `AGENTS.md` plan convention).
- A phase is complete when every task checkbox below it is checked and timestamped.
- **Do not begin a phase if any test is currently broken in the workspace** (per `AGENTS.md` Development Rules).

---

## Worktree Setup (Required Before Any Phase)

- [x] Chose `RANDOM=93283` differentiator for the worktree path (e.g., `42`);
- [ ] `export FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/HOW_SECRET_IS_SECURED_BY_SOFTWARE_93283`
- [ ] `export BRANCH_NAME=secret-compliance-93283`
- [ ] `git worktree add -b ${BRANCH_NAME} ${FULL_WORKTREE_PATH}`
- [ ] `cd ${FULL_WORKTREE_PATH}`; reset session working directory to the worktree path.
- [ ] `export CMAKE_BUILD_PARALLEL_LEVEL=10`
- [ ] `export CARGO_TARGET_DIR="${HOME}/.cache/cargo/foretias-secret-compliance-93283"` (per-worktree target dir per `AGENTS.md`)
- [ ] Verify no broken tests in alpha before starting:
      `cd ${FULL_WORKTREE_PATH}/p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build && cd build && ctest --output-on-failure`
      `cd ${FULL_WORKTREE_PATH}/p2p && cargo build --workspace && cargo test --workspace`

---

## Phase Dependency Graph

```
            ┌─ Phase 1: Bindgen Debug Removal (HR-3) ──┐
Phase 0 ────┤                                          ├─ Phase 6: Migrate ForetiasPrivKey32 ─┐
(Encrypt    └─ Phase 2: Zeroizing Wrappers (HR-2) ─────┘                                      │
 Infra)        (interim HR-2 sweeping)                                                        │
   │                                                                                          │
   ├─ Phase 3: Encrypt PQC Secrets (HR-1) ───────────────────────────────────────────────────┤
   ├─ Phase 4: Encrypt TBID V1 (HR-1) ───────────────────────────────────────────────────────┤
   └─ Phase 5: Encrypt Noise State (HR-1) ───────────────────────────────────────────────────┤
                                                                                              │
Phase 7: C Struct Zeroing After Extraction (HR-2)  ───────────────────────────────────────────┤
Phase 8: sodium_memzero Migration (infra)          ───────────────────────────────────────────┤
Phase 9: Bound Keypairs Vec (HR-2)                 ───────────────────────────────────────────┤
                                                                                              │
                                                                              Phase 10: CI Enforcement Gates (HR-3, HR-4, HR-5)
                                                                              Phase 11: Documentation & Audit (P2)
                                                                              Phase 12: Final Verification & Merge
```

Phases 1, 2, 7, 8, 9 are runnable in parallel with Phase 0 and with each other (no shared files except `build.rs`). Phases 3, 4, 5 depend on Phase 0. Phase 6 depends on Phase 1. Phase 10 depends on Phases 1-9 having landed.

---

## Phase 0 — Encryption Infrastructure (Foundation for HR-1)

**Goal:** Extend the existing KEK infrastructure in `privkey.c` to support arbitrary-length secrets so that subsequent phases can encrypt PQC keys, TBID secrets, and Noise session state without re-deriving the encryption primitives.
**REQ-Z\*:** REQ-Z2.2A (foundation), prerequisite for REQ-Z1.5A, REQ-Z1.7A, REQ-Z1.9A, REQ-Z1.11A, REQ-Z2.2B, REQ-Z2.2C.
**Depends on:** Worktree setup.
**Blocks:** Phases 3, 4, 5.
**Effort estimate:** ~4 hours.
**Risk:** Medium (new C11 public API; downstream consumers added in later phases).

### Tasks

- [ ] **0.1** Design generic encrypt/decrypt API in `p2p/core/include/foretias_core.h`:
  ```c
  /* Encrypt arbitrary-length plaintext with the instance KEK.
   * Caller supplies a nonce buffer (24 bytes) that will be filled.
   * Output buffer must be at least pt_len + 16 bytes (ciphertext + 16-byte MAC).
   * Returns FORETIAS_OK on success.
   */
  int foretias_privkey_encrypt(const uint8_t *plaintext, size_t pt_len,
                               uint8_t *ciphertext_out, uint8_t nonce_out[24]);

  /* Decrypt arbitrary-length ciphertext with the instance KEK.
   * Returns plaintext length (== ct_len - 16) on success; negative on auth failure.
   */
  int foretias_privkey_decrypt(const uint8_t *ciphertext, size_t ct_len,
                               const uint8_t nonce[24], uint8_t *plaintext_out);
  ```
- [ ] **0.2** Implement in `p2p/core/src/privkey.c`. Reuse the existing `encrypt_seed`/`decrypt_seed` ChaCha20-Poly1305 paths; expose generically.
- [ ] **0.3** Use `key_gen_counter` (existing monotonic counter) for nonce derivation. Document the 2^96-nonces-per-process budget (more than sufficient for any realistic deployment).
- [ ] **0.4** Add C11 unit tests in `p2p/core/tests/test_privkey.c`:
  - encrypt → decrypt round-trip for sizes 32, 96, 128, 160, 1184, 1632, 4000, 4112 bytes.
  - decrypt with wrong nonce → error.
  - decrypt with tampered ciphertext → MAC failure.
- [ ] **0.5** Build and run C11 tests:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p/core && cmake --build build && cd build && ctest --output-on-failure -R privkey
  ```
- [ ] **0.6** Regenerate Rust bindings:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build -p foretias-core
  ```
- [ ] **0.7** Verify `bindings.rs` exposes `foretias_privkey_encrypt` and `foretias_privkey_decrypt`.
- [ ] **0.8** Add a Rust integration test in `core-engine/src/core/privkey.rs` (or new `privkey_test.rs`) confirming the encrypt/decrypt round-trip works through FFI.
- [ ] **0.9** Commit:
  `Major: Phase 0 — KEK encryption infrastructure for arbitrary-length secrets, Phase: Implementation complete`

### Acceptance Criteria
- C11 tests pass for all enumerated sizes.
- Rust integration test passes.
- Both `foretias_privkey_encrypt` and `foretias_privkey_decrypt` visible in `bindings.rs`.

---

## Phase 1 — Bindgen Debug Removal (HR-3 — P0)

**Goal:** Suppress `#[derive(Debug)]` on all bindgen-generated FFI structs that hold secret material. This is the single highest-leverage, lowest-effort HR-3 win in the codebase.
**REQ-Z\*:** REQ-Z2.3 (umbrella), implements REQ-Z1.3, REQ-Z1.5, REQ-Z1.7, REQ-Z1.9, REQ-Z1.11.
**Depends on:** Worktree setup.
**Can run in parallel with:** Phase 0, 2, 7, 8, 9.
**Effort estimate:** ~30 minutes for the bindgen change + ~1 hour for any compile-error fallout.
**Risk:** Low — affects debug formatting only; runtime behavior unchanged.

### Tasks

- [ ] **1.1** Edit `p2p/core-engine/build.rs`. Locate the `bindgen::Builder::default()` chain (around lines 93–99). Append:
  ```rust
  .no_debug("ForetiasPrivKey32")
  .no_debug("ForetiasSecretKeyVar")
  .no_debug("ForetiasKemSecretKey")
  .no_debug("ForetiasTbidV1SecretKey")
  .no_debug("ForetiasNoiseState")
  .no_debug("ForetiasFrostRound1")
  ```
- [ ] **1.2** Rebuild:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build -p foretias-core
  ```
- [ ] **1.3** Verify the generated bindings have no `Debug` on the listed types:
  ```bash
  grep -A1 'pub struct ForetiasPrivKey32' \
    ${HOME}/.cache/cargo/foretias-secret-compliance-93283/debug/build/foretias-core-*/out/bindings.rs
  # Expect: no Debug in the preceding #[derive(...)]
  ```
- [ ] **1.4** Fix any compile errors from removed `Debug` derives:
  - Any `format!("{:?}", obj)` or `tracing::debug!(?obj, ...)` on the listed types will fail compilation.
  - Replace with one of: omit the log line entirely, log only a non-secret identifier (`tbid.short_hex()`), or write a manual `Debug` impl that emits `"<redacted N bytes>"`.
- [ ] **1.5** Run full test suite:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test --workspace
  ```
- [ ] **1.6** Add a regression negative test in `core-engine/tests/secret_no_debug.rs` (NEW file):
  ```rust
  /// HR-3 enforcement: bindings must not derive Debug for secret-holding types.
  /// This test confirms (via a `static_assertions::assert_not_impl_any!`) that
  /// `Debug` is not implemented for each enumerated FFI struct.
  use foretias_core::core::bindings::*;
  use static_assertions::assert_not_impl_any;

  assert_not_impl_any!(ForetiasPrivKey32: std::fmt::Debug);
  assert_not_impl_any!(ForetiasSecretKeyVar: std::fmt::Debug);
  assert_not_impl_any!(ForetiasKemSecretKey: std::fmt::Debug);
  assert_not_impl_any!(ForetiasTbidV1SecretKey: std::fmt::Debug);
  assert_not_impl_any!(ForetiasNoiseState: std::fmt::Debug);
  assert_not_impl_any!(ForetiasFrostRound1: std::fmt::Debug);
  ```
  Add `static_assertions = "1.1"` to `[dev-dependencies]` in `core-engine/Cargo.toml`.
- [ ] **1.7** Commit:
  `Major: Phase 1 — Suppress Debug derive on secret-holding FFI bindings (HR-3 REQ-Z2.3 et al.), Phase: Complete`

### Acceptance Criteria
- `cargo test --workspace` passes.
- `secret_no_debug` test compiles and passes.
- Manual grep confirms no `Debug` derive on the six listed FFI types.

---

## Phase 2 — Zeroizing Wrappers (HR-2 — P0 Interim)

**Goal:** Wrap all in-process Rust secret values that are not yet behind opaque handles in `zeroize::Zeroizing<T>`. This is the **interim HR-2** sweep — full HR-1 compliance happens in Phases 3-5 and 6.
**REQ-Z\*:** REQ-Z2.5 (= REQ-Z1.10), REQ-Z2.6, REQ-Z4.1 (interim), REQ-Z4.1B (interim), REQ-Z0.5 partial.
**Depends on:** Worktree setup.
**Can run in parallel with:** Phase 0, 1, 7, 8, 9.
**Effort estimate:** ~2 hours.
**Risk:** Low — type-level change; the compiler enforces correctness.

### Tasks

- [ ] **2.1** TBID secret in `core-engine/src/crypto_server/signing_tbid.rs:32-44` (REQ-Z2.5 / REQ-Z1.10):
  ```rust
  use zeroize::Zeroizing;
  let mut secret_bytes = Zeroizing::new(vec![0u8; 160]);
  secret_bytes[..32].copy_from_slice(&secret.ed25519_sk.bytes);
  secret_bytes[32..].copy_from_slice(&secret.slh_dsa_sk[..128]);
  // ...
  // Note: returning from here transfers ownership of the Zeroizing<Vec<u8>>;
  // the consumer must keep the Zeroizing wrapper. See REQ-Z2.6 for SignatureBytes audit.
  ```
- [ ] **2.2** Audit `SignatureBytes` and `TbidSecret` newtypes in `core-engine/src/foretias/types.rs` (REQ-Z2.6):
  - If `SignatureBytes` is `pub struct SignatureBytes(Vec<u8>);` then change to `pub struct SignatureBytes(zeroize::Zeroizing<Vec<u8>>);`.
  - If `TbidSecret` is `pub struct TbidSecret(Vec<u8>);` then change to `pub struct TbidSecret(zeroize::Zeroizing<Vec<u8>>);`.
  - Update any field accessors and constructors to dereference through `Zeroizing`.
- [ ] **2.3** Noise static key in `foretias-server/src/server/mod.rs:43, :78-89` (REQ-Z4.1 interim):
  ```rust
  use zeroize::Zeroizing;

  pub struct TimeFamilyServer {
      // ...
      noise_static_priv: Zeroizing<[u8; 32]>,
      noise_static_pub: [u8; 32],  // public, no wrapping
  }

  // In constructor:
  let (pub_key, mut priv_key) = generate_ed25519_keypair()?;
  let noise_static_priv = Zeroizing::new(priv_key.bytes);
  priv_key.bytes.fill(0);  // REQ-Z4.1B: zero the FFI temporary
  ```
- [ ] **2.4** Update any references to `self.noise_static_priv` to dereference the `Zeroizing` wrapper (`&*self.noise_static_priv`).
- [ ] **2.5** Build and test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build --workspace && cargo test --workspace
  ```
- [ ] **2.6** Add tests in `core-engine/tests/zeroize_wrappers.rs` (NEW):
  - Construct a TBID secret, drop it, confirm the underlying `Vec<u8>` buffer is zeroed.
  - (Where feasible) Verify that `noise_static_priv` is zeroed after `TimeFamilyServer` is dropped.
- [ ] **2.7** Commit:
  `Major: Phase 2 — Zeroizing wrappers for TBID secret, noise_static_priv, SignatureBytes (HR-2 interim REQ-Z2.5, Z4.1, Z4.1B), Phase: Complete`

### Acceptance Criteria
- All references to plain `Vec<u8>` or `[u8;N]` for the three named secrets are replaced with `Zeroizing`.
- Tests pass.
- Compiler-enforced zeroization on drop.

---

## Phase 3 — Encrypt PQC Secrets at C11 Layer (HR-1 — P0)

**Goal:** Bring SPHINCS+ and Dilithium3 secret keys into HR-1 compliance by storing them in encrypted form within `ForetiasSecretKeyVar`.
**REQ-Z\*:** REQ-Z1.5A, REQ-Z2.2B.
**Depends on:** Phase 0 (encryption infra), Phase 1 (Debug removed so refactor is safer).
**Can run in parallel with:** Phase 4, 5, 6, 7, 8, 9.
**Effort estimate:** ~6 hours.
**Risk:** High — changes FFI struct layout; touches every PQC signing site.

### Tasks

- [ ] **3.1** Update `p2p/core/include/foretias_core.h` `ForetiasSecretKeyVar`:
  ```c
  typedef struct {
      uint8_t encrypted_bytes[4112];  /* 4096 plaintext + 16 MAC */
      uint8_t nonce[24];
      size_t  plaintext_len;          /* Original plaintext length pre-encryption */
  } ForetiasSecretKeyVar;
  ```
- [ ] **3.2** Update `signing_sphincs.c`:
  - `foretias_sphincs_sha2_128s_keypair` and `foretias_sphincs_sha2_256f_keypair`: after liboqs writes the raw secret, immediately encrypt it via `foretias_privkey_encrypt` and store ciphertext + nonce + plaintext_len. Zero the raw buffer.
  - `foretias_sphincs_*_sign`: decrypt to a stack buffer, sign, `sodium_memzero` the stack buffer before return (Pattern §1.6.4).
- [ ] **3.3** Update `signing_dilithium.c` with the same pattern.
- [ ] **3.4** Update Rust wrappers (`core-engine/src/crypto_server/software.rs` PQC paths; `signing_sphincs.rs`, `signing_dilithium.rs` if separate):
  - The Rust-side `Zeroizing<SignatureBytes>` storage continues to hold the encrypted form. Decryption happens C-side per sign call.
  - Optionally remove the Rust-side `Zeroizing<SignatureBytes>` for the secret if no plaintext is ever held Rust-side — but verify with grep first.
- [ ] **3.5** Build C11:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p/core && cmake --build build && cd build && ctest --output-on-failure
  ```
- [ ] **3.6** Build and test Rust workspace:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build --workspace && cargo test --workspace
  ```
- [ ] **3.7** Add C11 test (`p2p/core/tests/test_sphincs.c`, `test_dilithium.c`): after `keypair()`, scan the struct's `encrypted_bytes` for the original `OQS_SIG_*_keypair`-produced secret bytes; assert they are NOT directly present (entropy/MAC will differ).
- [ ] **3.8** Commit:
  `Major: Phase 3 — Encrypt PQC secrets (SPHINCS+, Dilithium3) at C11 with KEK (HR-1 REQ-Z1.5A), Phase: Complete`

### Acceptance Criteria
- C11 tests pass.
- Rust tests pass.
- Struct scan test confirms no plaintext secret in `encrypted_bytes` post-keypair.

---

## Phase 4 — Encrypt TBID V1 Secrets at C11 Layer (HR-1 — P0)

**Goal:** Bring TBID V1 dual-key secrets (Ed25519 seed + SLH-DSA secret) into HR-1 compliance.
**REQ-Z\*:** REQ-Z1.9A, REQ-Z4.4 (indirectly).
**Depends on:** Phase 0.
**Can run in parallel with:** Phase 3, 5, 6, 7, 8, 9.
**Effort estimate:** ~3 hours.
**Risk:** High — changes TBID FFI layout; touches `signing_tbid.rs` Rust wrapper.

### Tasks

- [ ] **4.1** Update `p2p/core/include/foretias_core.h` `ForetiasTbidV1SecretKey`:
  ```c
  typedef struct {
      uint8_t encrypted_ed25519[48];   /* 32 plaintext + 16 MAC */
      uint8_t ed25519_nonce[24];
      uint8_t encrypted_slh_dsa[144];  /* 128 plaintext + 16 MAC */
      uint8_t slh_dsa_nonce[24];
      size_t  slh_dsa_plaintext_len;
  } ForetiasTbidV1SecretKey;
  ```
- [ ] **4.2** Update `signing_tbid.c`:
  - `foretias_tbid_v1_keypair`: after generating raw Ed25519 seed and SLH-DSA secret, encrypt each and zero the raw temporaries.
  - `foretias_tbid_v1_sign`: decrypt both to stack buffers, sign with each, `sodium_memzero` both stack buffers before return.
  - `foretias_tbid_v1_secret_zeroize`: continues to memzero the entire struct (now contains only ciphertexts; still safe to zero).
- [ ] **4.3** Update Rust wrapper `core-engine/src/crypto_server/signing_tbid.rs`:
  - The Rust-side `Zeroizing<Vec<u8>>` from Phase 2.1 continues to hold the encrypted form.
  - Document via comment that the bytes are ciphertext (not plaintext).
- [ ] **4.4** Build C11 + Rust + run tests.
- [ ] **4.5** Add C11 test: struct scan post-`keypair` asserts no plaintext Ed25519 seed in `encrypted_ed25519`.
- [ ] **4.6** Commit:
  `Major: Phase 4 — Encrypt TBID V1 dual-key secrets at C11 with KEK (HR-1 REQ-Z1.9A), Phase: Complete`

---

## Phase 5 — Encrypt Noise Session State at C11 Layer (HR-1 — P0)

**Goal:** Bring `ForetiasNoiseState` session keys (`local_static_priv`, `send_key`, `recv_key`, `chaining_key`) into HR-1 compliance.
**REQ-Z\*:** REQ-Z1.11A.
**Depends on:** Phase 0.
**Can run in parallel with:** Phase 3, 4, 6, 7, 8, 9.
**Effort estimate:** ~4 hours.
**Risk:** High — changes Noise FFI layout; affects every Noise handshake and AEAD operation.

### Tasks

- [ ] **5.1** Update `p2p/core/include/foretias_core.h` `ForetiasNoiseState`:
  - `local_static_priv[32]` → `encrypted_local_static[48]` + `local_static_nonce[24]`.
  - `send_key[32]` → `encrypted_send_key[48]` + `send_key_nonce[24]`.
  - `recv_key[32]` → `encrypted_recv_key[48]` + `recv_key_nonce[24]`.
  - `chaining_key[32]` → `encrypted_chaining_key[48]` + `chaining_key_nonce[24]`.
- [ ] **5.2** Update `noise_xx.c`:
  - All places that read/write the four secret fields gain a decrypt-then-use-then-zero pattern (Pattern §1.6.4) or an encrypt-then-store pattern.
  - DH and AEAD operations decrypt session keys into a stack buffer, perform the op, zero the buffer before any return.
  - `foretias_noise_destroy` continues to memzero the entire struct.
- [ ] **5.3** Update Rust wrapper `core-engine/src/noise.rs`:
  - The `NoiseSession` RAII wrapper continues to call `foretias_noise_destroy` on drop.
  - No additional Rust-side changes needed if the API surface is unchanged.
- [ ] **5.4** Build C11 + Rust + run tests including the Noise round-trip integration tests.
- [ ] **5.5** Add C11 test: after `noise_init_ed25519`, scan `encrypted_local_static` for the original seed bytes; assert not present.
- [ ] **5.6** Commit:
  `Major: Phase 5 — Encrypt Noise session state at C11 with KEK (HR-1 REQ-Z1.11A), Phase: Complete`

---

## Phase 6 — Migrate `ForetiasPrivKey32` Consumers (HR-1 — P0)

**Goal:** Eliminate consumers of the raw `ForetiasPrivKey32` type. Long-lived keys migrate to `PrivKeyHandle`; ephemeral keys (Noise per-connection) move their generation inside C.
**REQ-Z\*:** REQ-Z1.3A, REQ-Z3.1A, REQ-Z4.1A, REQ-Z1.11B (caller-discipline doc), REQ-Z1.4 (final sweep).
**Depends on:** Phase 1 (Debug removed first), Phase 5 (Noise encrypted).
**Effort estimate:** ~3 hours.
**Risk:** Medium — changes API surface for raw-key consumers.

### Tasks

- [ ] **6.1** Add new C11 function in `noise_xx.c`:
  ```c
  /* Initialize Noise session from a PrivKeyHandle (encrypted seed inside C),
   * rather than from a raw ForetiasPrivKey32. Internally derives X25519 scalar
   * from the decrypted seed and zeros the temporary.
   */
  int foretias_noise_init_with_handle(
      ForetiasNoiseState *state,
      const ForetiasPrivKey *priv_handle,
      const ForetiasPubKey32 *remote_pub);
  ```
- [ ] **6.2** Add Rust wrapper in `core-engine/src/noise.rs`:
  `pub fn noise_handshake_with_handle(handle: &PrivKeyHandle, ...) -> Result<NoiseSession, ...>`
- [ ] **6.3** Migrate `foretias-server/src/server/mod.rs`:
  - Replace `noise_static_priv: Zeroizing<[u8; 32]>` (interim from Phase 2) with `noise_static_priv: PrivKeyHandle`.
  - Generate via `PrivKeyHandle::generate(ForetiasCurve::Ed25519)`.
  - Use the new `noise_handshake_with_handle` API for outbound handshakes.
- [ ] **6.4** Migrate `foretias-client/src/noise_ptp.rs`:
  - Replace `generate_ed25519_keypair()` + `noise_handshake(..., &priv_key.bytes, ...)` with `PrivKeyHandle::generate()` + `noise_handshake_with_handle(&handle, ...)`.
  - This satisfies REQ-Z3.1A: no raw ephemeral seed crosses FFI.
- [ ] **6.5** Mark `ed25519_sign(priv_key: &ForetiasPrivKey32, ...)` as deprecated in `core-engine/src/core/signing.rs` (REQ-Z2.8). Add doc comment:
  ```rust
  /// # Security
  /// This function accepts a raw seed. Prefer [`ed25519_sign_with_handle`].
  #[deprecated(note = "use ed25519_sign_with_handle; see HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md REQ-Z2.8")]
  pub fn ed25519_sign(priv_key: &ForetiasPrivKey32, msg: &[u8]) -> ...
  ```
- [ ] **6.6** Sweep remaining callers of `ForetiasPrivKey32`-receiving functions; replace where possible (REQ-Z1.4 final sweep — every remaining caller must zero `bytes` after use).
- [ ] **6.7** Build + test workspace.
- [ ] **6.8** Add an integration test: spin up a `TimeFamilyServer`, exchange a Noise PtP message with a client, confirm the static key is unreachable from Rust (the field type is `PrivKeyHandle`, no `Deref`).
- [ ] **6.9** Commit:
  `Major: Phase 6 — Migrate ForetiasPrivKey32 consumers to opaque handle path (HR-1 REQ-Z1.3A, Z3.1A, Z4.1A), Phase: Complete`

### Acceptance Criteria
- Server and client compile with `noise_static_priv: PrivKeyHandle`.
- Existing Noise integration tests pass.
- `ed25519_sign` deprecation warning surfaces in clippy output.

---

## Phase 7 — C Struct Zeroing After Rust-Side Extraction (HR-2 — P1)

**Goal:** Zero the C-side `ForetiasKemSecretKey` and `ForetiasSecretKeyVar` structs after the Rust wrapper has copied bytes out, so the FFI struct does not retain plaintext after extraction.
**REQ-Z\*:** REQ-Z2.7, REQ-Z1.6, REQ-Z1.8 (partial completion).
**Depends on:** Worktree setup (independent of other phases except Phase 3 if PQC is encrypted at C11 — then the struct only holds ciphertext and the zeroing is defense-in-depth).
**Effort estimate:** ~1 hour.
**Risk:** Low — adds zeroing calls; no behavior change otherwise.

### Tasks

- [ ] **7.1** `core-engine/src/crypto_server/kem_mlkem.rs:keypair()`:
  ```rust
  // After extracting secret.bytes[..secret.len].to_vec():
  unsafe {
      foretias_memzero(&mut secret as *mut _ as *mut _, std::mem::size_of::<ForetiasKemSecretKey>());
  }
  ```
  (Or `sodium_memzero` after Phase 8.)
- [ ] **7.2** Same for `encapsulate()` if it produces a secret struct.
- [ ] **7.3** Same pattern for `signing_sphincs.rs` and `signing_dilithium.rs` Rust extractions of `ForetiasSecretKeyVar`. (After Phase 3, the struct holds ciphertext; zeroing remains defense-in-depth.)
- [ ] **7.4** Build + test.
- [ ] **7.5** Commit:
  `Major: Phase 7 — Zero C structs after Rust-side extraction (HR-2 REQ-Z2.7, Z1.6, Z1.8), Phase: Complete`

---

## Phase 8 — Replace `foretias_memzero` with `sodium_memzero` (Infrastructure — P1)

**Goal:** Consolidate on libsodium's `sodium_memzero` for all C11 zeroing. Remove the custom `memzero.c`.
**REQ-Z\*:** REQ-Z0.1, REQ-Z0.6, REQ-Z0.7.
**Depends on:** Worktree setup.
**Effort estimate:** ~1 hour.
**Risk:** Low — functional equivalent, better platform support.

### Tasks

- [ ] **8.1** Choose strategy: replace in place vs. compatibility shim.
  - **Strategy A** (preferred): replace every `foretias_memzero(` call with `sodium_memzero(`; remove `memzero.c`.
  - **Strategy B** (transitional): make `foretias_memzero` a static-inline shim around `sodium_memzero` in the header.
- [ ] **8.2** Implement Strategy A across all `p2p/core/src/*.c`:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p/core/src && \
    grep -rln 'foretias_memzero(' | xargs sed -i 's/foretias_memzero(/sodium_memzero(/g'
  ```
  Verify all touched files compile.
- [ ] **8.3** Remove `p2p/core/src/memzero.c` from `p2p/core/CMakeLists.txt` source list.
- [ ] **8.4** Remove `memzero.c` from `p2p/core-engine/build.rs` parallel build source list (if present).
- [ ] **8.5** Remove `foretias_memzero` declaration from `p2p/core/include/foretias_core.h`.
- [ ] **8.6** Delete `p2p/core/src/memzero.c`.
- [ ] **8.7** Build C11 + Rust + test:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p/core && cmake --build build && cd build && ctest --output-on-failure
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test --workspace
  ```
- [ ] **8.8** Commit:
  `Major: Phase 8 — Replace foretias_memzero with sodium_memzero; remove custom memzero.c (REQ-Z0.1, Z0.6, Z0.7), Phase: Complete`

---

## Phase 9 — Bound `Chronomatter::keypairs` Vec (HR-2 — P1)

**Goal:** Enforce Pattern §1.6.7 (Bounded Retention). The `Vec<TickKeyPair>` must retain only current + previous tick keypairs; older entries evicted and zeroized.
**REQ-Z\*:** REQ-Z4.3.
**Depends on:** Worktree setup.
**Effort estimate:** ~30 minutes.
**Risk:** Medium — changes tick key retention behavior; needs verification that `build_auto_attestation` still has access to the previous tick's key.

### Tasks

- [ ] **9.1** Edit `core-engine/src/chronomatter/mod.rs:generate_and_store_keypair` (around line 140):
  ```rust
  fn generate_and_store_keypair(&self) -> Result<usize, NodeError> {
      let kp = PrivKeyHandle::generate(ForetiasCurve::Ed25519)
          .map_err(|e| NodeError::Internal(e.to_string()))?;
      let pub_key = kp.public_key()?;
      let mut guard = self.keypairs.write();
      guard.push(TickKeyPair { pub_key, priv_key: kp });
      // REQ-Z4.3: retain only current + previous; evict older.
      while guard.len() > 2 {
          // remove(0) triggers Drop on the evicted PrivKeyHandle → C11 memzero.
          let _ = guard.remove(0);
      }
      Ok(guard.len() - 1)
  }
  ```
- [ ] **9.2** Verify `build_auto_attestation` and `verify_pair` only access keypairs at indices `len() - 1` (current) and `len() - 2` (previous), never older. If they index by tick number directly, refactor to track a "(tick_number → vec_index)" map or use the relative index.
  - **Sub-task** (likely needed): **9.2.1** Refactor `build_auto_attestation` to use relative indices rather than `tick - 1`.
- [ ] **9.3** Add test in `core-engine/tests/chronomatter_bounded.rs`:
  - Stamp 10 times.
  - Assert `keypairs.read().len() == 2` after.
  - (Where memory inspection is feasible) assert evicted keypairs' underlying ciphertext is zeroed.
- [ ] **9.4** Build + test.
- [ ] **9.5** Commit:
  `Major: Phase 9 — Bound Chronomatter::keypairs to 2 entries (HR-2 Pattern §1.6.7 REQ-Z4.3), Phase: Complete`

---

## Phase 10 — CI Enforcement Gates (HR-3, HR-4, HR-5 — P1)

**Goal:** Add automated grep/lint/test gates that prevent future regression of HR-3, HR-4, and HR-5 in CI.
**REQ-Z\*:** REQ-Z8.1, REQ-Z8.2, REQ-Z8.3, REQ-Z8.4, REQ-Z8.5.
**Depends on:** Phases 1-9 (no point in adding gates that fail on existing code).
**Effort estimate:** ~3 hours.
**Risk:** Low — additive CI scripts.

### Tasks

- [ ] **10.1** Add `.github/workflows/secret-discipline.yml` (or extend the main PR workflow) with these gates:

  **Gate 10.1.a — HR-3 Debug-derive scan (REQ-Z8.1):**
  ```bash
  if grep -rnE '#\[derive\([^)]*Debug[^)]*\)\]' \
       --include='*.rs' \
       p2p/core-engine/src p2p/foretias-client/src p2p/foretias-server/src \
     | grep -iE '(Secret|Priv|Seed|Share|Nonce)([^a-zA-Z]|$)' ; then
    echo "HR-3 violation: Debug derive on secret-holding type"; exit 1
  fi
  ```

  **Gate 10.1.b — HR-4 memcmp scan (REQ-Z8.2):**
  ```bash
  if grep -nE '\bmemcmp\(' p2p/core/src/*.c ; then
    echo "HR-4 violation: bare memcmp in core; use sodium_memcmp"; exit 1
  fi
  ```

  **Gate 10.1.c — HR-3 negative compile test (REQ-Z8.4):** runs `cargo test --workspace -- secret_no_debug` (test from Phase 1.6).

- [ ] **10.2** Add a Rust test in `core-engine/tests/heap_scan_post_tick.rs` (REQ-Z8.3) — best-effort heap scan:
  - Stamp once (records the current tick's key bytes in a separate buffer for the test only).
  - Stamp again (forces tick advance and eviction of the previous key per Phase 9).
  - Allocate a large buffer, fill with `0xAA`, then free, then scan; or use `procfs /proc/self/maps` + read scanner.
  - Assert the previously-recorded key bytes are not found in the process's heap pages.
  - **Note:** this is inherently flaky (heap reuse is non-deterministic). Run in CI as informational, not blocking, unless reliability can be established.

- [ ] **10.3** Add a snapshot test in `foretias-server/tests/cli_no_hex_leak.rs` (REQ-Z8.5):
  - Run `foretias serve --verbose ...` in subprocess with a known TBID/keys.
  - Capture stdout and stderr.
  - Assert no contiguous hex string of length ≥ 64 chars appears (a heuristic for "32+ bytes of secret material").
  - Repeat for `foretias stamp` and panic paths (force a panic via crafted input).

- [ ] **10.4** Configure CI to fail PRs that violate gate 10.1.a, 10.1.b, or 10.1.c. Gate 10.2 is informational. Gate 10.3 blocks.

- [ ] **10.5** Commit:
  `Major: Phase 10 — CI enforcement gates for HR-3/HR-4/HR-5 (REQ-Z8.1-Z8.5), Phase: Complete`

---

## Phase 11 — Documentation and Audit (P2)

**Goal:** Close the documentation-only requirements and run the post-implementation verification checklist (Appendix C of the spec).
**REQ-Z\*:** REQ-Z0.4, REQ-Z1.2, REQ-Z2.2 (Drop comment), REQ-Z2.8, REQ-Z4.2.
**Depends on:** Phases 1-10.
**Effort estimate:** ~2 hours.
**Risk:** None.

### Tasks

- [ ] **11.1** Add doc comment to `core-engine/src/crypto_server/software.rs:SoftwareCryptoServer::drop` documenting field-drop order (REQ-Z2.2):
  ```rust
  impl Drop for SoftwareCryptoServer {
      fn drop(&mut self) {
          // Field drop order (Rust source order):
          // 1. priv_key (PrivKeyHandle) → C11 foretias_privkey_free → sodium_memzero
          // 2. seal_key (Zeroizing<[u8;32]>) → Zeroizing::drop → ptr::write_volatile zero
          // 3. PQC secrets (Zeroizing<SignatureBytes>) → Zeroizing::drop
          // 4. frost_shares (Mutex<HashMap<_, Zeroizing<Vec<u8>>>>) → per-entry Zeroizing::drop
          // No manual fill(0) needed; Zeroizing handles all secret fields.
      }
  }
  ```
- [ ] **11.2** Add doc comment to `TimeFamilyServer` (REQ-Z4.2): document that `noise_static_priv` is now a `PrivKeyHandle` (after Phase 6) whose drop zeros via C11.
- [ ] **11.3** Add doc comment to `core/src/privkey.c` (REQ-Z1.2): document the KEK global-state design exemption and reference the spec.
- [ ] **11.4** Add doc comment to `core-engine/src/core/signing.rs:ed25519_sign` (REQ-Z2.8): mark as unsafe/raw path; already deprecated in Phase 6.
- [ ] **11.5** Add doc comment to `signing_sphincs.c`, `signing_dilithium.c`, `kem_mlkem.c` documenting the C-zeros-on-failure / Rust-zeros-on-scope-exit split (REQ-Z0.4).
- [ ] **11.6** Walk through `HOW_SECRET_IS_SECURED_BY_SOFTWARE_SPEC.md Appendix C: Audit Verification Checklist` and confirm every checkbox can be marked.
- [ ] **11.7** Update the **Status** column of the Master REQ-Z\* Index in §1.7 of the spec from "❌ Open" to "✅ Done" for every requirement closed by this PLAN.
- [ ] **11.8** Commit:
  `Major: Phase 11 — Documentation and audit closeout for secret-handling compliance, Phase: Complete`

---

## Phase 12 — Final Verification and Merge

**Goal:** Confirm full compliance, merge to alpha.
**Depends on:** All prior phases.
**Effort estimate:** ~1 hour (plus any merge-conflict remediation).
**Risk:** Medium — alpha may have moved during the work.

### Tasks

- [ ] **12.1** Final build and test in worktree:
  ```bash
  cd ${FULL_WORKTREE_PATH}/p2p/core && cmake --build build && cd build && ctest --output-on-failure
  cd ${FULL_WORKTREE_PATH}/p2p && cargo build --workspace --release
  cd ${FULL_WORKTREE_PATH}/p2p && cargo test --workspace
  ```
- [ ] **12.2** Run the CI enforcement gates manually (10.1.a, 10.1.b, 10.1.c, 10.3) to confirm green.
- [ ] **12.3** Review `bindings.rs` final state; confirm no `Debug` on the six enumerated FFI types.
- [ ] **12.4** Verify all work in `${FULL_WORKTREE_PATH}` is committed to `${BRANCH_NAME}`.
- [ ] **12.5** Merge `${BRANCH_NAME}` to `alpha`:
      ```bash
      cd ${HOME}/webhash/foretias && git merge ${BRANCH_NAME}
      ```
  - [ ] **12.5.1** (Conditional) If merge conflict: resolve, re-run all tests in alpha, then commit the merge.
  - [ ] **12.5.2** Confirm no test regressions in alpha post-merge:
        `cd ${HOME}/webhash/foretias/p2p && cargo test --workspace`
- [ ] **12.6** Finalize:
  - [ ] Confirm `HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md` has all but the cleanup checkboxes complete.
  - [ ] `git worktree remove ${FULL_WORKTREE_PATH}` (cleanup worktree).
  - [ ] `git branch -d ${BRANCH_NAME}` (after merge confirmed).
  - [ ] This is the last checkbox to be checked in this plan.

---

## Out-of-Scope (Tracked Separately)

The following items from the spec are explicitly **not** addressed in this plan and are tracked elsewhere:

- **REQ-Z0.3** (`sodium_mprotect` for KEK / tick key memory): P2 hardening; deferred to a separate "memory-protection" plan.
- **REQ-Z1.2A** (replace `rts` placeholder with proper KEK derivation): P1; deferred to a separate "KEK key-derivation upgrade" plan.
- **REQ-Z1.12 / REQ-Z1.12A** (FROST nonce encryption): blocked on FROST implementation. Will be addressed by the FROST implementation plan.
- **REQ-Z5.\*** (Python/Java bindings): blocked on bindings re-introduction (`SCOPE_REDUCTION_SPEC.md`).
- **REQ-Z6.\*** (formal taint / information-flow analysis): tracked as a separate "formal analysis of secret usage" deliverable (Zone 6 of the spec).
- **REQ-Z7.2** (pin libp2p version that uses zeroize): tracked in supply-chain hygiene.

---

## Completion Criteria for This Plan

The plan is complete when:

1. Every task checkbox above is marked `[x]` with a timestamp.
2. Every REQ-Z\* status field in spec §1.7 that this plan addresses is updated to `✅ Done` or `✅ Verified`.
3. The merge to alpha is complete; no test regressions.
4. The worktree is cleaned up.
5. The HR Violation Summary in spec Zone 8 reflects the new state (most ❌ → ✅).
