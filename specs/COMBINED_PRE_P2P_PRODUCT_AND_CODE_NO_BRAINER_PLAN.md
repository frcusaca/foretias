# COMBINED_PRE_P2P_PRODUCT_AND_CODE_NO_BRAINER_PLAN.md

**Date:** 2026-05-20
**Source:** `specs/COMBINED_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md`
**Exclusion:** Items already planned in `specs/HOW_SECRET_IS_SECURED_BY_SOFTWARE_PLAN.md`. Anything covered there is intentionally **not** duplicated here.
**Selection criteria (all must hold):**
1. **Small change** — typically 1 to 10 lines per fix, no architectural ripple.
2. **Already well-thought-out** in COMBINED — the fix sketch is explicit; no design decision pending.
3. **Easily testable** — by an existing test or by one short additional test.
4. **Not a duplicate** of any task already in the secret-handling PLAN.
5. **No merge-conflict risk** with the secret-handling PLAN's phases (or such risk is explicitly called out).

This plan lists ten Tier-A no-brainers. Each is independently shippable and testable. Tier-B candidates (slightly larger or with caveats) appear at the end under §"Tier B — Also Worth Considering" so they are not lost.

---

## Worktree Setup

- [ ] Create worktree `git worktree add -b combined_pre_p2p_no_brainer ${HOME}/tmp/foretias-worktrees/COMBINED_PRE_P2P_PRODUCT_AND_CODE_NO_BRAINER_PLAN_4821`
- [ ] `cd ${HOME}/tmp/foretias-worktrees/COMBINED_PRE_P2P_PRODUCT_AND_CODE_NO_BRAINER_PLAN_4821`; reset current session work directory to be the full worktree path.

## How to Work This Plan

- Phases are **independent** — each task block can be its own PR. There is no enforced ordering except where called out under "Merge-conflict awareness".
- Each task lists: source COMBINED section, files touched, the change (with a code sketch where helpful), the test that proves correctness, and an estimated effort.
- Follow `AGENTS.md` plan conventions: check the box and timestamp on the next indented line when complete.
- Do **not** start a task while any test in the workspace is failing (`AGENTS.md` Development Rules).
- Commit messages follow the project format: `Major: ... ; <model identity per AGENTS.md>`.

---

## Phase A — Honesty Fixes for FROST/Epoch Stub Handlers

These two changes prevent callers from acting on a false-positive epoch verification or a false "epoch zero" state. They are pure honesty fixes — they do not implement FROST; they make the unimplemented state observable instead of silently returning success/empty.

**Merge-conflict awareness:** independent of the secret-handling plan.

### Task A.1 — `handle_verify_epoch_snapshot` returns `valid: false` with reason

**Source:** COMBINED §2.3.5 — "Immediate mitigation (cheap)".
**File:** `p2p/foretias-server/src/server/handlers.rs:354-376`.
**Change:**
- Replace the unconditional `valid: true` payload with `{"valid": false, "reason": "FROST epoch verification not yet implemented"}`.
**Test (add):** `foretias-server/tests/epoch_honesty.rs` (new file) — issue a `verify_epoch_snapshot` RPC with any payload, assert `valid == false` and reason string matches.
**Effort:** 15 min.
**Acceptance:** unit test passes; no behavior depends on the old `valid: true`.

- [ ] **A.1** Complete

### Task A.2 — `handle_get_latest_epoch` returns JSON-RPC error instead of zero-stub

**Source:** COMBINED §2.3.5 "Additionally (NEW — sibling stub `handle_get_latest_epoch`)".
**File:** `p2p/foretias-server/src/server/handlers.rs:225-242`.
**Change:**
- Replace the all-zeros stub response with a JSON-RPC error:
  ```rust
  return resp_error(server, id, JSONRPC_INTERNAL_ERROR, "FROST epoch data not yet implemented".into());
  ```
**Test (add):** in the same `epoch_honesty.rs` file — call `get_latest_epoch`, assert it returns a JSON-RPC error envelope (not a success result with zero data).
**Effort:** 15 min.
**Acceptance:** clients consuming `get_latest_epoch` are forced to handle the unimplemented state explicitly.

- [ ] **A.2** Complete

---

## Phase B — C11 Bounds-Check Hardening (Two Tiny Patches)

Two tiny patches that close out-of-bounds vulnerabilities in C11 code paths.

**Merge-conflict awareness:**
- B.1 is in `noise_xx.c` AEAD encrypt/decrypt paths, **not** the struct layout. The secret-handling PLAN Phase 5 rewrites `ForetiasNoiseState` struct layout but does not touch `_nh_ae_enc`. Safe to do in parallel; B.1 should be merged first so the test code is already proven against the existing layout.
- B.2 is in `foretias_privkey_derive_seal_key` (HKDF expand path). The secret-handling PLAN Phase 0 adds new encrypt/decrypt entry points to `privkey.c` but does not touch the HKDF expand function. Safe to do in parallel; B.2 should be merged first to prove the same test infrastructure.

### Task B.1 — Nonce overflow guard in `noise_xx.c`

**Source:** COMBINED §2.3.9 — "CRIT".
**File:** `p2p/core/src/noise_xx.c:69, :86` (both `_nh_ae_enc` / `_nh_ae_dec` callsites with `(*n)++`).
**Change:** Before each `(*n)++`, insert:
```c
if (*n == UINT64_MAX) return FORETIAS_ERR_NONCE_EXHAUSTED;
```
- Add `FORETIAS_ERR_NONCE_EXHAUSTED` constant in `foretias_core.h` (next free error number).
**Test (add):** `p2p/core/tests/test_noise.c` — initialize a `ForetiasNoiseState`, set `send_nonce = UINT64_MAX`, attempt one send, assert return is `FORETIAS_ERR_NONCE_EXHAUSTED`. (Test does not need to actually exhaust the nonce — just preset the counter.)
**Effort:** 30 min.
**Acceptance:** new test passes; existing Noise round-trip tests unchanged.

- [ ] **B.1** Complete

### Task B.2 — HKDF `info_len` overflow guard in `privkey.c`

**Source:** COMBINED §2.3.10 — "HIGH".
**File:** `p2p/core/src/privkey.c:236-240` (top of `foretias_privkey_derive_seal_key`).
**Change:** At function entry:
```c
if (info_len > 63) return FORETIAS_ERR_BAD_INPUT;
```
**Test (add):** `p2p/core/tests/test_privkey.c` — generate a PrivKeyHandle, call `foretias_privkey_derive_seal_key` with `info_len = 64`, assert return is `FORETIAS_ERR_BAD_INPUT`. Also assert that `info_len = 63` still succeeds (boundary correctness).
**Effort:** 20 min.
**Acceptance:** new tests pass; existing seal-key derivation tests unchanged.

- [ ] **B.2** Complete

---

## Phase C — Input Validation at Trust Boundaries

Small length-check additions at trust boundaries that prevent specific bypasses or DoS vectors.

**Merge-conflict awareness:** C.1 (genesis check) is in `tick.rs`, **not** in any file the secret-handling PLAN touches in Phases 0-12. C.2 (PQC sig length check) is in `signing_sphincs.c` / `signing_dilithium.c` — Phase 3 of secret plan rewrites these files' `keypair` and `sign` functions. **C.2 should be merged before secret-plan Phase 3 begins**, since it touches the same files; once merged, Phase 3 picks up the bounds check naturally. C.3 (libp2p RPC codec) is in `communerd/p2p/rpc_protocol.rs`, untouched by secret plan.

### Task C.1 — Reject genesis ChrononRecord with short `forward_foretis`

**Source:** COMBINED §2.3.7 / §1.3.2 — "HIGH (genesis verification bypass)".
**File:** `p2p/core-engine/src/foretias/tick.rs:308-317` (genesis split path).
**Change:** Before splitting `forward_foretis` into Ed25519 + genesis components, validate length:
```rust
const EXPECTED_GENESIS_FORETIS_MIN_LEN: usize = 64; // Ed25519 sig length; genesis component appended after
if record.forward_foretis.len() < EXPECTED_GENESIS_FORETIS_MIN_LEN {
    return Err(NodeError::InvalidGenesis("forward_foretis too short for genesis split".into()));
}
```
- If the genesis component itself has a known minimum (e.g., the SLH-DSA signature is 49,856 bytes), use that for `tb_version == 1`; for `tb_version == 0` the minimum is just 64.
**Test (add):** in the existing `tick.rs` test module — construct a ChrononRecord with `tb_version: 0`, `forward_foretis: vec![0u8; 50]`, attempt verification, assert `InvalidGenesis` error.
**Effort:** 30 min.
**Acceptance:** new test passes; existing genesis-verify tests unchanged.

- [ ] **C.1** Complete

### Task C.2 — Bound `sig->len` in PQC sign paths

**Source:** COMBINED §2.3.8 / §1.3.6 / §1.5.D.
**Files:**
- `p2p/core/src/signing_sphincs.c:53` (and equivalent in 256f variant).
- `p2p/core/src/signing_dilithium.c:53`.
**Change:** At the top of each `_sign` function, before the liboqs call:
```c
if (sig->len > FORETIAS_SIG_MAX_SIG_BYTES) return FORETIAS_ERR_BAD_INPUT;
```
- Mirror the same check on the Rust side in `core-engine/src/core/signing.rs` for defense in depth (add a length assertion before the FFI call).
**Test (add):** `p2p/core/tests/test_sphincs.c` and `test_dilithium.c` — pass a `ForetiasSecretKeyVar` with `len > FORETIAS_SIG_MAX_SIG_BYTES`, assert `FORETIAS_ERR_BAD_INPUT`.
**Effort:** 30 min (both files).
**Acceptance:** new tests pass; existing PQC tests unchanged.

- [ ] **C.2** Complete

### Task C.3 — Bound length-prefixed allocation in libp2p RPC codec

**Source:** COMBINED §2.3.15 — "CRIT, NEW from consolidated review".
**File:** `p2p/foretias-server/src/communerd/p2p/rpc_protocol.rs:70-80` (`ForetiasRpcCodec::read_request`).
**Change:** Add a constant `MAX_RPC_FRAME_BYTES = 16 * 1024 * 1024` (16 MiB) (or coordinate with whatever `server/mod.rs` uses for the TCP path). Before `vec![0u8; len]`, reject:
```rust
const MAX_RPC_FRAME_BYTES: usize = 16 * 1024 * 1024;
if len > MAX_RPC_FRAME_BYTES {
    return Err(io::Error::new(io::ErrorKind::InvalidData, "RPC frame too large"));
}
```
**Test (add):** `foretias-server/tests/rpc_codec_bounds.rs` — feed the codec a buffer whose 4-byte length prefix is `0xFFFFFFFF`, assert an `InvalidData` error is returned and no large allocation occurs (test completes in <100 ms even with the malicious length).
**Effort:** 30 min.
**Acceptance:** new test passes; existing libp2p RPC tests unchanged.

- [ ] **C.3** Complete

---

## Phase D — Strict Serde + Algorithm-from-Backend

Two small Rust correctness fixes that close silent-failure modes.

**Merge-conflict awareness:** independent of secret-handling plan.

### Task D.1 — Remove `#[serde(default)]` from `stamps_per_tick`

**Source:** COMBINED §1.3.10 / §34.4.
**File:** `p2p/core-engine/src/foretias/tick.rs:27`.
**Change:** Delete the `#[serde(default)]` attribute on the `stamps_per_tick: u64` field. A missing field on deserialization will now produce an error, which is the desired behavior (legitimate-0 is distinguishable from missing-field).
**Test (add):** in the existing `tick.rs` deserialize tests — attempt to deserialize a JSON object that omits `stamps_per_tick`, assert `Err`. Also assert that an object with `"stamps_per_tick": 0` still succeeds.
**Effort:** 10 min.
**Acceptance:** new tests pass; any test fixtures that previously relied on the default (likely none) are updated.

- [ ] **D.1** Complete

### Task D.2 — Query algorithm from `CryptoServer` rather than hardcoding `Ed25519`

**Source:** COMBINED §1.3.9 / §3.6.
**Files:** `p2p/core-engine/src/chronomatter/mod.rs:259, :283, :355` (three sites).
**Change:** At the top of `stamp()` and `build_tick_record()`, compute `let alg = self.crypto.signature_algorithm().to_id_string();` once, then use the local binding instead of `SignatureAlgorithm::Ed25519.to_id_string()` at each site.
**Test (add):** in `chronomatter/mod.rs` test module — create a mock `CryptoServer` (or use a feature-flagged stub) that returns `SignatureAlgorithm::Dilithium3` from `signature_algorithm()`, stamp once, assert the recorded `signature_algorithm` string is `"Dilithium3"` (not `"Ed25519"`).
**Effort:** 30 min.
**Acceptance:** new test passes; existing stamp tests (which use the default Ed25519 backend) continue to pass.

- [ ] **D.2** Complete

---

## Phase E — Atomic Counter Idiom in Chronomatter

**Merge-conflict awareness:** This task touches `chronomatter/mod.rs`. The secret-handling PLAN Phase 9 also touches `chronomatter/mod.rs` (different function: `generate_and_store_keypair`). The CAS-fix touches `stamp()` (lines 302-320) and `daemon_tick()` (lines 421-443). These are different functions; merge conflict risk is **low** but possible if either commit reformats the file. Recommend completing this Phase **before** secret-plan Phase 9 begins. If both are in flight: do this one first.

### Task E.1 — Replace CAS-as-counter with `fetch_add` in `stamp()` and `daemon_tick()`

**Source:** COMBINED §1.3.3 / §2.4 / §4.2.
**Files:** `p2p/core-engine/src/chronomatter/mod.rs:302-320` (`stamp`) and `:421-443` (`daemon_tick`).
**Change:** Replace the `compare_exchange` block with:
```rust
let new_tick = self.current_tick.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
```
Remove the `map_err(|e| NodeError::Internal(format!("tick counter conflict: {}", e)))` plumbing (it can no longer fail).
**Test (add):** `core-engine/tests/concurrent_stamp_stress.rs` (new file) — spawn 16 tokio tasks, each stamping 100 times against a shared `Chronomatter`, await all, assert: (a) no task returned `NodeError::Internal("tick counter conflict: ...")`, (b) the final `current_tick` value equals the total number of stamps.
**Effort:** 40 min.
**Acceptance:** new stress test passes; existing single-threaded stamp tests pass unchanged.

- [ ] **E.1** Complete

---

## Phase Summary Table

| Phase / Task | Severity per COMBINED | LOC | Test | Effort |
|---|---|---|---|---|
| A.1 — verify_epoch_snapshot honesty | HIGH | ~3 | new unit | 15 min |
| A.2 — get_latest_epoch honesty | NEW HIGH | ~3 | new unit | 15 min |
| B.1 — Noise nonce overflow guard | CRIT | ~2 + 1 const | new C unit | 30 min |
| B.2 — HKDF info_len overflow guard | HIGH | ~1 + test | new C unit | 20 min |
| C.1 — Genesis short-foretis check | HIGH | ~4 | new unit | 30 min |
| C.2 — PQC sig->len bound | HIGH | ~2 × 2 files | new C unit × 2 | 30 min |
| C.3 — libp2p RPC codec frame bound | NEW CRIT | ~4 | new unit | 30 min |
| D.1 — Remove serde(default) on stamps_per_tick | MED | 1 | extend existing | 10 min |
| D.2 — Query algorithm from CryptoServer | HIGH | ~3 + 3 sites | new unit | 30 min |
| E.1 — CAS → fetch_add in stamp/daemon | HIGH | ~10 | new stress test | 40 min |

**Total estimated effort:** ~4 hours of focused work for all ten tasks.

---

## Tier B — Also Worth Considering (Not in This Plan)

The following are also small and well-thought-out, but each carries a slight caveat — additional design decision, larger touch surface, or non-trivial test setup. They are surfaced here so they are not forgotten, but they are excluded from the no-brainer list above.

| Item | Source | Why It's Not Tier A |
|---|---|---|
| `tbid_handshake.rs:117` `try_into().unwrap()` → `?` propagation | COMBINED §1.3.1 | Trivial but part of a wider unwrap sweep (§1.3.1 lists 6 sites); collecting them into one PR makes sense, but the sweep is a Tier-B size. |
| `server/mod.rs:166` `to_str().unwrap()` → `to_string_lossy()` | pre-public-mvp §3.3 | Same as above — fold into unwrap sweep PR. |
| `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` × 2 | COMBINED §1.3.1 | Properly fixed by `Clock` injection (large refactor); a `?`-propagation patch is interim. |
| `handlers.rs:138` swallowed serde error | pre-public-mvp §4 | Small but requires deciding the right `INVALID_PARAMS` JSON-RPC error shape. |
| Calendar `.tmp` recovery runs `integrity_check` (Rust path) | COMBINED §2.4.7 | Small change but tests need a forged `.tmp` file fixture. |
| Lower `MAX_CONTENT_BYTES` and limit hex-string pre-decode | COMBINED §8.11 | Small change but value (1 GiB → 16 MiB) is a policy decision worth confirming. |
| `sig_input` built twice (DRY) → extract `fn build_sig_input(...)` | COMBINED §8.9 | Small refactor; touches both `stamp()` and `verify()` so risk of subtle behavior change. |
| Locks across `.await` in `generate_and_store_keypair` (move keygen outside lock) | COMBINED §1.3.4 | Will be addressed by secret-plan Phase 9 anyway — duplicating risks merge conflict. |
| Probity gossip signature verification | COMBINED §2.3.4 | Small change but trust model implications warrant a dedicated review. |
| Calendar mirror `integrity_check` on inbound `ship_ack` | COMBINED §2.3.6 | Small change but needs a forged-record fixture and decision on per-record vs per-batch failure. |
| DHT registration signature | COMBINED §2.3.2 | Small change in principle but the signing key choice (TBID? libp2p identity?) is a design point. |
| `handle_verify` local path algorithm match check | COMBINED §1.3.2 NEW | Small `if rec.signature_algorithm != foretis.signature_algorithm { reject }`; testable but worth bundling with the `UnverifiedFoo`/`VerifiedFoo` work. |
| `NoiseSession` `unsafe impl Send` audit | COMBINED §1.3.5 NEW | Small change (remove the impl) but breaks anything that moves a session across threads — needs caller audit first. |

If, after Phases A-E above land, the user wants to continue with quick wins, **task C.1 + C.2 from Tier B (calendar `.tmp` integrity check, then mirror integrity check)** is the next-best pair, followed by **the unwrap sweep** as one bundled PR.

---

## What Was Considered and Excluded

The following items from COMBINED were considered and explicitly excluded from this plan because they fail the "small + easily testable + no design decision" criterion:

| Item | Why excluded |
|---|---|
| §1.3.7 Clock injection | Large refactor across `Chronomatter`, `TbidHandshake`, `daemon`, `collision_detector`; needs a dedicated SPEC. |
| §1.3.8 `Chronomatter::new` DI fix | Touches multiple constructors and downstream callers. |
| §1.3.12 ProbityReport length-prefix canonical form | Wire-format-breaking change; needs version negotiation. |
| §1.3.13 Test determinism sweep | Multi-file sweep; large enough to merit its own SPEC. |
| §2.3.13 JSON-RPC auth | Architectural decision (token vs mTLS vs both). |
| §2.3.14 JSON-RPC transport encryption | Default-Noise change is small-ish but the `--insecure-plaintext` flag and migration of clients is non-trivial. |
| §4.6 `id` from envelope, not params | Multi-handler sweep; touches the dispatcher. |
| §4.7 `block_on` → `async fn` handler refactor | Multi-handler refactor; large. |
| §2.3.5 FROST implementation | Out of scope by definition. |
| §8.10 Ed25519 seed re-derivation cache | Adds new API surface (`_with_keypair` variant). |
| Anything from COMBINED Part I §1.5.A-I (AGENTS.md updates) | Covered by `AGENTS_MD_RUST_UPDATE_SPEC` candidate listed in COMBINED Appendix A; not no-brainer-sized. |
| Anything from COMBINED Part II §2.6 (formal methods) | Covered by `FORMAL_VERIFICATION_ROADMAP_SPEC` candidate. |

---

## Worktree Teardown

- [ ] Verify all work is complete in `${HOME}/tmp/foretias-worktrees/COMBINED_PRE_P2P_PRODUCT_AND_CODE_NO_BRAINER_PLAN_4821` and committed to `combined_pre_p2p_no_brainer`
- [ ] Merge `combined_pre_p2p_no_brainer` to alpha

## Completion Criteria

This plan is complete when every Phase A-E task checkbox above is checked and timestamped, AND:

- All new tests pass on every supported build:
  ```bash
  cd p2p/core && cmake --build build && cd build && ctest --output-on-failure
  cd p2p && cargo test --workspace
  ```
- No existing test regresses.
- Each task lands as its own PR (10 PRs total), or — if bundled — the bundle is small enough for single-sitting review.

If, after writing this plan, you find that some of these "no-brainers" turn out to require more design thought when you start implementing, move them to Tier B and pick the next one.

---

*Plan authored by Claude Opus 4.7 (max effort), 2026-05-20.*
