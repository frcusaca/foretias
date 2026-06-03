# COMBINED_GROUP3_PLAN.md
# Security Correctness — Parallel Agent Plan

**Date:** 2026-05-22
**Paired Spec:** `COMBINED_GROUP3_SPEC.md`
**Status:** ✅ MERGED to alpha 2026-05-23. Checkboxes marked 2026-06-02.
**Pre-flight:** `COMBINED_GROUP2_PLAN.md` Phase A (every agent runs this first)

---

## Agent Assignment Overview

| Branch | Work unit | Files | Phase | Est. effort |
|--------|-----------|-------|-------|-------------|
| `g3-b-noise-send` | Remove `unsafe impl Send for NoiseSession` | `noise.rs`, `identity.rs` | 1 | 2-4 hr |
| `g3-f-secretbytes` | Wrap `secret_bytes` in `Zeroizing` | `signing_tbid.rs` | 1 | 1-2 hr |
| `g3-e-unwrap-sweep` | Bounded unwrap sweep | various (no Clock sites) | 1 | 3-5 hr |
| `g3-d-dht-sign` | Sign+verify DHT records | `communerd/mod.rs` | 3 | 1-2 days |

**Phase 1 branches** can be developed in parallel — no file overlap. **Phase 3**
(g3-d-dht-sign) must wait for Phase 2 (Group 5-A Clock injection) to land first.

---

## Agent Instructions Per Work Unit

Each agent runs:
1. `COMBINED_GROUP2_PLAN.md` Phase A pre-flight (must all pass)
2. Their assigned work unit below
3. Final acceptance criteria from `COMBINED_GROUP3_SPEC.md` §3 for their branch
4. Merge to alpha; remove worktree

---

## Branch g3-b-noise-send — NoiseSession Send Removal

**Spec:** `COMBINED_GROUP3_SPEC.md` §2.1
**Worktree:** `${HOME}/tmp/foretias-worktrees/g3-b-noise-send-$(date +%s)`
**Setup:** `git worktree add -b g3-b-noise-send <path>`

### Steps

- [x] **Pre-flight** — run `COMBINED_GROUP2_PLAN.md` Phase A; all checks pass
- [x] **Locate the unsafe impl**:
      ```bash
      grep -n "unsafe impl Send for NoiseSession" p2p/core-engine/src/noise.rs
      # expect: line 37
      ```
- [x] **Remove** the `unsafe impl Send for NoiseSession {}` line.
- [x] **Update safety comment** above it: replace the existing block with:
      ```rust
      // NoiseSession is intentionally NOT Send. The wrapped C11 ForetiasNoiseState
      // contains mutable nonce counters (`send_nonce`, `recv_nonce`) advanced by
      // foretias_noise_send/recv. Cross-thread access without synchronization
      // can corrupt the nonce sequence and trigger ChaCha20-Poly1305 nonce reuse.
      // If you need to send a session across threads, wrap it in
      // Arc<tokio::sync::Mutex<NoiseSession>> at the call site.
      ```
- [x] **Build:** `cd p2p && cargo build --workspace`
- [x] **Triage every compile error:** for each "NoiseSession cannot be sent between threads" error:
      - Check if the call site genuinely needs Send (e.g., crosses `tokio::spawn`).
      - If yes → wrap in `Arc<tokio::sync::Mutex<NoiseSession>>`.
      - If no (e.g., used inside a single-task `let _ = async move { ... }`) →
        restructure so the session does not cross the move boundary.
      - Document each fix with a one-line comment.
- [x] **Audit `PrivKeyHandle`**:
      ```bash
      grep -n "unsafe impl Send for PrivKeyHandle" p2p/core-engine/src/core/identity.rs
      # expect: line 18
      ```
      Apply the same analysis. If the underlying C `ForetiasPrivKey32` is
      immutable after construction (just a 32-byte seed), the `Send` impl may be
      correct. Document the determination in the safety comment.
- [x] **Add compile-time assertion:** in `p2p/core-engine/tests/secret_no_debug.rs`
      (or a new `tests/send_safety.rs`):
      ```rust
      use static_assertions::assert_not_impl_any;
      use foretias_core::noise::NoiseSession;
      assert_not_impl_any!(NoiseSession: Send);
      ```
- [x] **Run tests:** `cargo test --workspace`
- [x] **Lint:** `cargo clippy --workspace` (no new warnings)
- [x] **Commit:**
      ```
      Major: Security Correctness (Group 3), Phase 1, Unit B — remove NoiseSession unsafe Send
      Claude Code 2.1.119 (Claude Code); claude-opus-4-7
      ```
- [x] **Merge to alpha; remove worktree.**

---

## Branch g3-f-secretbytes — Zeroizing wrap in signing_tbid.rs

**Spec:** `COMBINED_GROUP3_SPEC.md` §2.4
**Worktree:** `${HOME}/tmp/foretias-worktrees/g3-f-secretbytes-$(date +%s)`

### Steps

- [x] **Pre-flight** — `COMBINED_GROUP2_PLAN.md` Phase A passes
- [x] **Open** `p2p/core-engine/src/crypto_server/signing_tbid.rs` at line 32-46
- [x] **Verify** the current `secret_bytes` extraction matches the spec quote
- [x] **Determine downstream type:**
      ```bash
      grep -n "fn from.*Vec<u8>\|impl From<Vec<u8>> for SignatureBytes" p2p/core-engine/src/crypto_server/
      ```
      Read `SignatureBytes::from(Vec<u8>)`'s implementation. If it stores the
      Vec in a field, that field must also be Zeroizing-aware or the protection
      is lost on conversion.
- [x] **Apply minimum-impact fix:**
      - **Path A (preferred if SignatureBytes already protects):** change
        `let mut secret_bytes = Vec::with_capacity(240);` to
        `let mut secret_bytes = zeroize::Zeroizing::new(Vec::with_capacity(240));`
        and at the return site dereference: `SignatureBytes::from(secret_bytes.to_vec())`
        — but this copies, defeating Zeroize. Better:
      - **Path B:** if `SignatureBytes` takes ownership and zeroizes on drop,
        just verify that by reading its implementation. If yes, this work unit
        is already implicitly correct — document it with a comment and a test.
      - **Path C (most conservative):** keep the conversion but ensure
        `secret.encrypted_ed25519` etc. are zeroized on the C side AFTER extraction
        (likely already done by `sodium_memzero`). Add a `Zeroizing` wrap around
        `secret_bytes` and a manual `secret_bytes.zeroize()` before any early
        return path, even if `SignatureBytes` doesn't protect.
- [x] **Verify SignatureBytes is appropriately protected:** if not, file a
      follow-up note in the work unit's commit message (do not expand scope).
- [x] **Tests:** `cargo test -p foretias-core -- tbid`
- [x] **Commit:**
      ```
      Major: Security Correctness (Group 3), Phase 1, Unit F2 — Zeroizing wrap on TBID secret_bytes
      ```
- [x] **Merge to alpha; remove worktree.**

---

## Branch g3-e-unwrap-sweep — Bounded unwrap sweep

**Spec:** `COMBINED_GROUP3_SPEC.md` §2.3
**Worktree:** `${HOME}/tmp/foretias-worktrees/g3-e-unwrap-sweep-$(date +%s)`

### Steps

- [x] **Pre-flight** — `COMBINED_GROUP2_PLAN.md` Phase A passes
- [x] **For each of the 6 catalogued sites, verify it still exists:**
      ```bash
      grep -n "try_into().unwrap()" p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs
      grep -n ".to_str().unwrap()" p2p/foretias-server/src/server/mod.rs
      grep -n "from_value.*\.ok()" p2p/foretias-server/src/server/handlers.rs
      ```
- [x] **Skip Clock-related sites** (`SystemTime::now().unwrap()`) — they are
      handled by Group 5-A. Do NOT touch these files for clock fixes.
- [x] **Apply fixes** per spec §2.3 — see the spec for exact patterns
- [x] **Add three regression tests** as outlined in spec §2.3 Tests subsection
- [x] **Run:** `cargo test --workspace`
- [x] **Commit + merge.**

---

## Branch g3-d-dht-sign — DHT record signing (Phase 3)

**Spec:** `COMBINED_GROUP3_SPEC.md` §2.2
**Prerequisite:** Group 5-A (Clock injection) must be merged to alpha first
**Worktree:** `${HOME}/tmp/foretias-worktrees/g3-d-dht-sign-$(date +%s)`

### Steps

- [x] **Pre-flight** — Phase A passes; also confirm Group 5-A has merged
      (`grep "self.clock.now_ns" p2p/foretias-server/src/communerd/mod.rs` shows hits)
- [x] **Extend `PeerRegistrationRecord`** per spec §2.2 step 1
- [x] **Add `canonical_payload()` method** — reference `ProbityReport::canonical()`
      at `p2p/foretias-server/src/probity/report.rs:34` for the length-prefix
      encoding pattern; use the same `(field_bytes.len() as u16).to_le_bytes()`
      prefix scheme for string fields
- [x] **Sign on publish** — modify both `PutRecord` send sites (lines ~644 and ~656)
- [x] **Verify on consume** — replace `validate_peer_registration(&record)` calls
      with `validate_peer_registration(&record, crypto)?` returning Result, and
      handle the `Ok(false)` case as `continue` (skip the record with a warn log)
- [x] **Create test file** `p2p/foretias-server/tests/dht_record_signature.rs`
      with the four cases listed in spec §2.2 Tests
- [x] **Run:** `cargo test -p foretias-server -- dht_record_signature`
- [x] **Run full workspace tests:** `cargo test --workspace`
- [x] **Commit:**
      ```
      Major: Security Correctness (Group 3), Phase 3, Unit D3 — DHT record sign+verify
      ```
- [x] **Merge to alpha; remove worktree.**

---

## Group-Level Completion Criteria

After all four branches are merged to alpha:

- [x] `NoiseSession: !Send` enforced via `static_assertions`
- [x] `secret_bytes` in `signing_tbid.rs` is `Zeroizing`-wrapped
- [x] Unwrap sweep covered all 6 catalogued sites (or delegated to G5-A)
- [x] `PeerRegistrationRecord` is signed and verified
- [x] `cargo test --workspace` passes
- [x] `cargo build --workspace` zero warnings
- [x] No regression in CI gates (`secret_no_debug.rs`, `trust_boundary_type_usage.rs`)
