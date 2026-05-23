# COMBINED_GROUP3_SPEC.md
# Security Correctness — Residual Items After Type-Enforced Trust Boundaries

**Date:** 2026-05-22 (re-verified against alpha @ 80f0714)
**Status:** Approved — implementation ready, parallel-agent friendly
**Paired Plan:** `COMBINED_GROUP3_PLAN.md`
**Master Coordination:** `COMBINED_GROUP2_SPEC.md` §2.2

---

## 1. Verification Discovery (2026-05-22)

A line-by-line audit found that **most** of the security gaps catalogued in
`COMBINED_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` are already closed by the
`TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION` and `Secret compliance` work. The
table below records what is **already done** so future readers do not chase
ghosts.

| Original concern | Status | Evidence |
|-----------------|--------|----------|
| Gossip handler accepts unsigned ProbityReports | ✅ DONE | `probity/gossip_handler.rs:59-106` — `verify_report_signature()` calls Ed25519 verify against pubkey extracted from TBID; rejects on failure |
| `Debug` derives on `ForetiasPrivKey32`/`SecretKeyVar`/etc. | ✅ DONE | `core-engine/build.rs:100-105` uses `.no_debug(...)`; `tests/secret_no_debug.rs` enforces via `assert_not_impl_any!` |
| `handle_ship_ack` accepts records without integrity check | ✅ DONE | `server/handlers.rs:482-502` — every record goes through `into_clean_authenticated_genesis` or `into_clean_authenticated`, which performs full chain-of-trust signature verification before `insert_mirrored` |
| `handle_verify` local path lacks algorithm match | ✅ DONE | `server/handlers.rs:175-178` — wraps record in `CleanAuthenticatedChrononRecord` then `into_clean_authenticated()` which internally validates algorithm |
| `noise_static_priv: [u8; 32]` not zeroized | ✅ DONE | `server/mod.rs:44` — `noise_static_priv: Zeroizing<[u8; 32]>` |
| `ProbityReport` canonical form uses null-byte separators | ✅ DONE | `probity/report.rs:34-60` — length-prefixed encoding (u16-LE prefixes) with NaN/±0 normalization |
| C11 `sig->len` not bounded in PQC sign | ✅ DONE | `core/src/signing_sphincs.c:42`, `signing_dilithium.c:42` — `if (sig_out->len > FORETIAS_SIG_MAX_SIG_BYTES) return ERR_BAD_INPUT` |

**Net result:** four items remain genuinely open. They are described below
as Work Units B, D3, E, F2 (using the original spec letters to preserve
traceability to the source review).

---

## 2. Work Units (Residual Open Items)

Each work unit below is a self-contained PR with its own branch. They have **no
file overlap** with each other and can be worked in parallel by separate agents.

### 2.1 Work Unit B — Remove `unsafe impl Send for NoiseSession`

**Severity:** CRIT (potential nonce reuse in ChaCha20-Poly1305)
**Branch:** `g3-b-noise-send`
**Files modified:**
- `p2p/core-engine/src/noise.rs` (single file)
- Possibly downstream call sites if compiler errors surface

#### Problem

`p2p/core-engine/src/noise.rs:37` declares `unsafe impl Send for NoiseSession {}`
with the justification (lines 32-36):

```rust
// SAFETY: NoiseSession wraps a C11 `ForetiasNoiseState` allocated via std::alloc.
// The C11 state is only accessed through this handle, and Drop ensures deterministic
// cleanup via foretias_noise_destroy + dealloc. No interior mutability exists in C.
unsafe impl Send for NoiseSession {}
```

The claim **"No interior mutability exists in C"** is incorrect. The C11
`ForetiasNoiseState` (defined in `p2p/core/include/foretias_noise.h`) contains
mutable counters `send_nonce` and `recv_nonce` that are advanced by every
`foretias_noise_send`/`foretias_noise_recv` call. If a `NoiseSession` is moved
across a thread boundary while another thread holds a reference, concurrent
encrypt/decrypt operations can corrupt the nonce counters, causing **nonce reuse
in ChaCha20-Poly1305 — a catastrophic encryption failure**.

#### Fix

1. **Remove** `unsafe impl Send for NoiseSession` at `noise.rs:37`.
2. **Compile** with `cargo build -p foretias-core --tests` and `cargo build -p foretias-server`.
3. **For each compile error** at a site that previously moved `NoiseSession` across a thread boundary:
   - If the session genuinely needs to be sent (e.g., into a `tokio::spawn`),
     wrap it in `Arc<tokio::sync::Mutex<NoiseSession>>` (or `parking_lot::Mutex`
     if the lock is never held across an `.await`). This ensures mutual exclusion
     during encrypt/decrypt.
   - If the session is created and used on the same thread, no change beyond
     potentially adding `// INVARIANT: session never crosses a thread boundary`
     to document.
4. **Audit:** Apply the same check to `unsafe impl Send for PrivKeyHandle` at
   `p2p/core-engine/src/core/identity.rs:18`. Document or remove with the same
   reasoning.

#### Tests

- Existing Noise round-trip tests must still pass: `cargo test -p foretias-core --test '*' -- noise`
- Add a compile-time assertion in `core-engine/tests/secret_no_debug.rs` (or a
  new test file) using `static_assertions::assert_not_impl_any!`:
  ```rust
  assert_not_impl_any!(NoiseSession: Send);   // after the fix
  ```

#### Acceptance

- [ ] `grep -n "unsafe impl Send for NoiseSession" p2p/core-engine/src/noise.rs` → no matches
- [ ] All existing tests pass
- [ ] `assert_not_impl_any!(NoiseSession: Send)` compiles
- [ ] If `PrivKeyHandle Send` is also removed, same gate added

---

### 2.2 Work Unit D3 — Sign and Verify DHT `PeerRegistrationRecord`

**Severity:** HIGH (TBID not cryptographically bound to peer_id)
**Branch:** `g3-d-dht-sign`
**Depends on:** Phase 2 (Clock injection) landing first, because the record
                  carries `registered_at_ns`
**Files modified:**
- `p2p/foretias-server/src/communerd/mod.rs` (struct + publish + consume paths)

#### Problem

`PeerRegistrationRecord` (defined at `communerd/mod.rs:42-53`) is published to
the Kademlia DHT without a cryptographic binding between the claimed TBID and
the peer_id. A malicious peer can publish a record claiming any TBID and route
verification traffic to themselves.

Current state:
- Publish: `communerd/mod.rs:630-661` — `serde_json::to_vec(&peer_record)` direct
- Consume: `validate_peer_registration()` at line 61 — only checks structural
  validity (non-empty fields, TBID is hex, ≥ 64 hex chars)

#### Fix

**Design point (locked):** Sign with the node's TBID Ed25519 key (the same key
that signs ChrononRecords). This makes the DHT record's authenticity part of the
same chain of trust as the rest of the protocol; using the ephemeral libp2p
identity key would split the trust model.

1. **Extend `PeerRegistrationRecord`** with a signature field:
   ```rust
   pub struct PeerRegistrationRecord {
       // ...existing fields unchanged...
       #[serde(default)]
       pub signature: Vec<u8>,    // Ed25519 signature over canonical_payload()
   }
   ```
   `#[serde(default)]` means legacy un-signed records still deserialize (so old
   peers don't immediately break). The consumer logic distinguishes signed vs.
   legacy below.

2. **Add a canonical-payload function** to `PeerRegistrationRecord`:
   ```rust
   impl PeerRegistrationRecord {
       /// Canonical bytes for signing — all fields EXCEPT `signature`,
       /// length-prefixed (u16-LE prefix per string field), then fixed-size fields.
       pub fn canonical_payload(&self) -> Vec<u8> { /* ... */ }
   }
   ```
   Use the same length-prefixed scheme as `ProbityReport::canonical()` for
   consistency. Excludes the `signature` field itself.

3. **Sign on publish** (around lines 630-661):
   ```rust
   let mut peer_record = PeerRegistrationRecord { /* ... */, signature: vec![] };
   let canonical = peer_record.canonical_payload();
   let sig = self.crypto.sign(&canonical)
       .map_err(|e| NodeError::Internal(format!("DHT record sign: {e}")))?;
   peer_record.signature = sig.bytes.to_vec();
   ```
   Apply identically to both the `/peers/v1` record (line 630) and the
   `/tbid/{}/v1` index record (line 654).

4. **Verify on consume** — extend `validate_peer_registration()` (line 61):
   ```rust
   fn validate_peer_registration(
       record: &PeerRegistrationRecord,
       crypto: &dyn CryptoServer,
   ) -> Result<bool, NodeError> {
       // Existing structural checks (unchanged) ...

       // Signature check (new) — legacy compat: accept empty signature with
       // a warn-log, but reject any non-empty signature that does not verify.
       if record.signature.is_empty() {
           tracing::debug!(tbid = %record.tbid,
               "DHT record without signature (pre-signing peer); accepted under legacy compat");
           return Ok(true);
       }
       let pubkey = hex::decode(&record.tbid)
           .map_err(|e| NodeError::BadFormat(format!("TBID hex: {e}")))?;
       if pubkey.len() < 32 {
           return Err(NodeError::BadFormat("TBID too short for Ed25519".into()));
       }
       let canonical = record.canonical_payload();
       let valid = crypto.verify_with(&pubkey[..32], "Ed25519", &canonical, &record.signature)
           .map_err(|e| NodeError::Crypto(e))?;
       if !valid {
           tracing::warn!(tbid = %record.tbid,
               "DHT record signature verification failed; discarding");
       }
       Ok(valid)
   }
   ```
   Update both call sites (around lines 463 and 486) to pass `&*self.crypto` and
   `continue` (skip) on `Ok(false)` or `Err(_)`.

5. **Legacy compatibility window:** the `if record.signature.is_empty()` branch
   is a transitional accommodation. Once all production peers have upgraded,
   tighten to `if record.signature.is_empty() { return Ok(false); }`. Track via
   a `// TODO(post-v0.7): require signature` comment.

#### Tests

New file: `p2p/foretias-server/tests/dht_record_signature.rs`:
- `legacy_record_without_signature_accepted_with_warn` — empty `signature` field
  passes `validate_peer_registration`.
- `valid_signature_accepted` — sign with known key, verify passes.
- `tampered_record_rejected` — flip one byte of `multiaddr`, verify fails.
- `wrong_pubkey_rejected` — sign with key A, claim TBID = key B; verify fails.

#### Acceptance

- [ ] `PeerRegistrationRecord` has `#[serde(default)] pub signature: Vec<u8>`
- [ ] Both DHT publish sites populate `signature` before serializing
- [ ] Both DHT consume sites verify or skip with logging
- [ ] New test file passes
- [ ] Backward-compat test (no-signature record) passes

---

### 2.3 Work Unit E — Unwrap Sweep (Bounded)

**Severity:** HIGH (panics on reachable inputs)
**Branch:** `g3-e-unwrap-sweep`
**Scope is bounded:** focus only on sites where the panic is reachable from
**external input** (RPC params, DHT records, file contents, network messages).
The full 467-call sweep is out of scope for this work unit.

#### Problem

Six sites are catalogued in `COMBINED_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md §1.3.1`.
Some may already be fixed by the type-enforced trust boundary work; verify each
before declaring it done.

#### Fix List (verify each first; skip if already fixed)

For each site below, the agent must:
1. Read the current code at that line.
2. If the unwrap is **gone** or now inside a documented-invariant `expect()`,
   tick the box with a `(verified fixed)` note.
3. Otherwise, apply the fix and add a test.

- [ ] `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs:~117`
      `payload_nonce: [u8; 32] = response.signed_payload[nonce_offset..nonce_offset + 32].try_into().unwrap()` →
      replace with `try_into().map_err(|_| NodeError::Protocol("malformed handshake payload".into()))?`
- [ ] `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs:~67`
      `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` → handled by Group 5-A
      (mark as "delegated to G5-A" — do not fix here, to avoid merge conflict)
- [ ] `p2p/core-engine/src/chronomatter/mod.rs:~67` (only if SystemTime unwrap
      survives Group 5-A; check at start)
- [ ] `p2p/foretias-server/src/server/mod.rs:~166`
      `json_path.to_str().unwrap()` →
      `json_path.to_string_lossy().into_owned()` (paths on Linux can be arbitrary bytes)
- [ ] `p2p/foretias-server/src/server/handlers.rs:~138`
      `serde_json::from_value(v.clone()).ok()` (silently discards parse error) →
      surface as `INVALID_PARAMS` JSON-RPC error with the error message in the data field
- [ ] `p2p/core-engine/src/foretias/tick.rs:~88-91`
      `SystemTime::now()` direct call — handled by Group 5-A (do not fix here)

The Group-5 delegations exist because clock injection touches the same lines.
Do not fix them in Group 3-E — they'd cause merge conflicts.

#### Tests

For each non-delegated site fixed, add a regression test:
- For tbid_handshake.rs:117 — feed a 31-byte signed_payload, assert the handler
  returns `Protocol` error (not a panic).
- For server/mod.rs:166 — construct a path with a non-UTF-8 byte (e.g.,
  `OsStr::from_bytes(&[0xFF])`), call the affected function, assert it doesn't panic.
- For handlers.rs:138 — POST a JSON-RPC request with a malformed `signature` value
  (an object instead of a string), assert response is `INVALID_PARAMS` (not 500).

#### Acceptance

- [ ] All non-delegated sites either verified-fixed or fixed
- [ ] At least three new tests
- [ ] `cargo test --workspace` passes

---

### 2.4 Work Unit F2 — Wrap `signing_tbid.rs` `secret_bytes` in `Zeroizing`

**Severity:** HIGH (TBID secret persists on Rust heap until allocator reuse)
**Branch:** `g3-f-secretbytes`
**Files modified:**
- `p2p/core-engine/src/crypto_server/signing_tbid.rs` (single file)

#### Problem

`signing_tbid.rs:32-46` extracts the TBID dual-key secret from the C struct into
a Rust `Vec<u8>` and never zeroizes it:

```rust
let mut secret_bytes = Vec::with_capacity(240);
secret_bytes.extend_from_slice(&secret.encrypted_ed25519);
secret_bytes.extend_from_slice(&secret.ed25519_nonce);
secret_bytes.extend_from_slice(&secret.encrypted_slh_dsa);
secret_bytes.extend_from_slice(&secret.slh_dsa_nonce);
// ...
Ok((SignatureBytes::from(public_bytes), SignatureBytes::from(secret_bytes)))
```

The C side is zeroized via `sodium_memzero`, but the Rust heap allocation
persists with the secret material until the allocator reuses the page.

#### Fix

1. Change `let mut secret_bytes = Vec::with_capacity(240)` to
   `let mut secret_bytes = zeroize::Zeroizing::new(Vec::with_capacity(240))`.
2. Adjust the trailing `SignatureBytes::from(secret_bytes)` call site. If
   `SignatureBytes::from` consumes the `Vec<u8>` by value, dereference
   the `Zeroizing` first (or implement the conversion to consume the inner Vec
   while preserving zeroize-on-drop semantics — check the trait impl first).
3. If `SignatureBytes` itself holds secret material, audit it for `Zeroizing`
   discipline (likely a separate, larger refactor — record as a follow-up if so;
   do not expand scope here).

#### Tests

- Existing `cargo test -p foretias-core -- tbid` passes.
- Add a basic test (or note in code) that the new pattern compiles and produces
  the same downstream signature output.

#### Acceptance

- [ ] `secret_bytes` is `Zeroizing<Vec<u8>>` (or moves into a `Zeroizing`-aware downstream type)
- [ ] Existing tests pass
- [ ] If a follow-up audit is needed, it's documented as a TODO with the agent's name

---

## 3. Cross-Cutting Acceptance Criteria (After all 4 work units land)

- [ ] `cargo test --workspace` passes
- [ ] `cargo build --workspace` produces zero warnings
- [ ] `grep -n "unsafe impl Send for NoiseSession" p2p/core-engine/src/` → 0
- [ ] `grep -n "secret_bytes = Vec::" p2p/core-engine/src/crypto_server/signing_tbid.rs` → 0
- [ ] DHT signature roundtrip test exists and passes

---

## 4. Out of Scope for Group 3

- Full `.unwrap()` audit across all 467 sites (only the catalogued 6 are in scope)
- FROST epoch handler (still returns dummy signature; tracked separately)
- Replacing every `Vec<u8>` secret with `Zeroizing<Vec<u8>>` workspace-wide
- New CI gates beyond `assert_not_impl_any!` for `NoiseSession: !Send`
- JSON-RPC transport-layer auth (Group 5-C)
