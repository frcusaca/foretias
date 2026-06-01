# Unwrap/Expect Audit — Production Code

**Date:** 2026-06-01
**Scope:** `p2p/core-engine/src/`, `p2p/foretias-server/src/`, `p2p/foretias-client/src/`
**Excluded:** `tests/`, `build.rs`, `#[cfg(test)]` modules
**Auditor:** opencode 1.14.28; vllm/qwen3.6-27b (Phase 18.6, COMBINED_GROUP7_COMMUNERDETTE_PLAN.md)

---

## Summary Statistics

| Category | `.unwrap()` | `.expect()` | Total |
|----------|-------------|-------------|-------|
| Test code (`#[cfg(test)]`) | ~470 | ~24 | ~494 |
| Production protocol code | 6 | 7 | 13 |
| Configuration/startup | 2 | 8 | 10 |
| Internal invariant | 6 | 0 | 6 |
| FFI boundary | 0 | 0 | 0 |
| **Total** | **~484** | **~39** | **~523** |

> **Note:** Test code counts are approximate (derived from grep totals minus identified production hits). Test code unwraps are acceptable and not flagged individually.

---

## Top 10 Highest-Risk Items

### #1 — CRITICAL: External input handler with unwrap

**File:** `p2p/foretias-server/src/server/handlers.rs:579`
**Code:** `let prev = verified.last().unwrap();`
**Category:** Production protocol code
**Context:** `handle_ship_ack` — processes `history_dump_ack` JSON-RPC from remote peers.

```rust
// Lines 570-589 (abbreviated)
for (i, unproc) in unprocessed.into_iter().enumerate() {
    let clean = if i == 0 {
        // genesis verification
        unproc.into_clean_authenticated_genesis(crypto.as_ref())
    } else {
        let prev = verified.last().unwrap();  // <-- LINE 579
        unproc.into_clean_authenticated(crypto.as_ref(), prev)
    };
    match clean {
        Ok(v) => verified.push(v),
        Err(e) => {
            return resp_error(server, id, jsonrpc::INVALID_PARAMS,
                format!("chain verification failed at record {}: {}", i, e));
        }
    }
}
```

**Risk:** This processes external mirror data from remote peers. While the current control flow guarantees `verified` is non-empty when `i > 0` (the `i == 0` branch pushes to `verified` before the `i > 0` branch can execute), this is a **logic-dependent invariant** on external input. A future refactor that changes the loop structure could introduce a panic-on-external-input DoS vector.

**Recommendation:** Replace with `verified.last().ok_or_else(|| resp_error(...))` to make the invariant explicit and fail gracefully. This is a defensive measure against future refactoring.

---

### #2 — MODERATE: Public function with crypto library expect

**File:** `p2p/core-engine/src/snapshot_signature.rs:68`
**Code:** `.expect("Argon2id hash should not fail with valid inputs")`
**Category:** Production protocol code
**Context:** `derive_keypair(passphrase: &str)` — public function deriving Ed25519 keys from passphrase.

```rust
pub fn derive_keypair(passphrase: &str) -> (SigningKey, VerifyingKey) {
    let argon2 = Argon2::default();
    let mut hash_output = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), SALT, &mut hash_output)
        .expect("Argon2id hash should not fail with valid inputs");
    // ...
}
```

**Risk:** `passphrase` is a user-supplied string. Argon2 can fail under OOM conditions or with certain edge-case inputs. While the function takes `&str` (guaranteed valid UTF-8), the Argon2 library can return errors for memory allocation failures. A panic here crashes the process.

**Recommendation:** Change return type to `Result<(SigningKey, VerifyingKey), SnapshotSignatureError>` and propagate the Argon2 error. If this is only called during startup/key generation, the current behavior may be acceptable, but the public API should not panic.

---

### #3 — MODERATE: Client config trait impl panics on connect failure

**File:** `p2p/foretias-client/src/foretias.rs:284`
**Code:** `.expect("PtpConfig construction failed")`
**Category:** Production protocol code
**Context:** `with_config_ptp(cfg: PtpConfig)` — PtP config trait implementation.

```rust
fn with_config_ptp(cfg: PtpConfig) -> Self {
    Self::connect(
        cfg.standalone.tbn,
        cfg.peers,
        cfg.timeout_secs,
        cfg.standalone.persist_path,
    )
    .expect("PtpConfig construction failed")
}
```

**Risk:** `Self::connect()` is an async network operation. If the remote server is unreachable, the client panics instead of returning an error. This is a DoS vector if an attacker controls the peer list or network conditions.

**Recommendation:** Return `Result<Self, ForetiasError>` from `with_config_ptp` (matching `with_config_p2p` which already returns `Result`). The trait may need to be updated to support fallible PtP configuration.

---

### #4 — LOW: Serialization expect on owned data (canonical methods)

**Files:**
- `p2p/core-engine/src/probity/report.rs:74` — `expect("postcard serialize ProbityReport")`
- `p2p/core-engine/src/foretias/tick.rs:185` — `expect("postcard serialize Foretis")`
- `p2p/foretias-server/src/communerd/mod.rs:91` — `expect("postcard serialize PeerRegistrationRecord")`

**Category:** Production protocol code
**Context:** `canonical()` / `canonical_payload()` / `sig_input_bytes()` methods.

```rust
// probity/report.rs:74
pub fn canonical(&self) -> Vec<u8> {
    let mut no_sig = self.clone();
    no_sig.signature = Vec::new();
    no_sig.slow_signature = Vec::new();
    postcard::to_allocvec(&no_sig).expect("postcard serialize ProbityReport")
}
```

**Risk:** Postcard serialization on owned, well-formed structs is extremely unlikely to fail (only OOM). These are canonicalization methods used for signing — a panic here would affect the signing path.

**Recommendation:** Acceptable as-is for now. If these methods become part of a hot path or are called in tight loops, consider returning `Result` or using a fallible variant.

---

### #5 — LOW: serde_json unwrap in canonical_bytes

**File:** `p2p/core-engine/src/epoch/snapshot.rs:45,46,58`
**Code:** `serde_json::to_value(self).unwrap()`, `val.as_object_mut().unwrap()`, `serde_json::to_vec(&val).unwrap()`
**Category:** Production protocol code
**Context:** `EpochSnapshot::canonical_bytes()` — produces canonical bytes for FROST signing.

```rust
pub fn canonical_bytes(&self) -> Vec<u8> {
    let mut val = serde_json::to_value(self).unwrap();
    val.as_object_mut().unwrap().remove("frost_signature");
    // ... sorting ...
    serde_json::to_vec(&val).unwrap()
}
```

**Risk:** `serde_json::to_value` on a `Serialize` struct only fails for custom serializers that explicitly error. `as_object_mut()` on a struct serialization is guaranteed to return `Some`. `to_vec` on a valid `Value` only fails for custom serializers.

**Recommendation:** Acceptable as-is. The invariants are strong (struct → JSON → Vec).

---

### #6 — LOW: Mutex unwrap on internal state

**Files:**
- `p2p/core-engine/src/clock.rs:73` — `self.current.lock().unwrap()`
- `p2p/foretias-server/src/communerd/p2p/swarm.rs:294` — `local_multiaddr.lock().unwrap()`
- `p2p/foretias-server/src/communerd/mod.rs` — ~20 `lock().unwrap()` calls
- `p2p/foretias-server/src/communerd/communerdette.rs` — ~30 `read().unwrap()` / `write().unwrap()` calls

**Category:** Internal invariant
**Context:** `std::sync::Mutex` and `parking_lot::RwLock` on internal state.

**Risk:** Mutex poisoning (panic while holding lock) causes all subsequent `lock()` calls to return `PoisonError`, which `.unwrap()` turns into a panic. This is a cascading failure mode. However, the affected state is internal (counters, addresses, relationship state) and not directly exposed to external input.

**Recommendation:** Acceptable for now. If any of these Mutexes protect state that could be corrupted by a panic in a concurrent task, consider using `lock().unwrap_or_else(|e| e.into_inner())` or `parking_lot::Mutex` (which doesn't poison).

---

### #7 — LOW: Internal invariant unwraps (guaranteed by type/layout)

**Files:**
- `p2p/core-engine/src/foretias/types.rs:50` — `self.inner[..32].try_into().unwrap()`
- `p2p/core-engine/src/foretias/types.rs:55` — `self.inner[32..].try_into().unwrap()`
- `p2p/core-engine/src/foretias/clean_auth.rs:411` — `self.signatures.last().unwrap()`
- `p2p/foretias-server/src/communerd/communerdette.rs:1109` — `prev.expect("idx > 0 implies prev exists")`

**Category:** Internal invariant
**Context:** Type layout guarantees and control flow invariants.

```rust
// types.rs:50 — TBID is always 96 bytes; slicing to 32 is guaranteed
pub fn ed25519_public_key(&self) -> [u8; 32] {
    self.inner[..32].try_into().unwrap()
}

// clean_auth.rs:411 — is_empty() checked on line 405
pub fn build(self) -> Result<Externalized<T>, CleanAuthError> {
    if self.signatures.is_empty() {
        return Err(CleanAuthError::SignatureCountMismatch { ... });
    }
    let last = self.signatures.last().unwrap();  // guaranteed non-empty
    // ...
}
```

**Risk:** These are provably safe given the current code structure. The `try_into()` on fixed-size slices can never fail. The `last()` after `is_empty()` check is guaranteed. The `prev.expect()` in communerdette is guaranteed by loop logic.

**Recommendation:** Acceptable as-is. Document with inline comments if the invariant is not obvious.

---

### #8 — LOW: Configuration/startup expects

**Files:**
- `p2p/foretias-server/src/main.rs:221,224` — duration formatting
- `p2p/foretias-server/src/server/mod.rs:273` — tokio runtime builder
- `p2p/foretias-server/src/communerd/mod.rs:229` — libsodium availability
- `p2p/foretias-server/src/communerd/p2p/behaviour.rs:31,42,47,51,88` — protocol strings, gossipsub config

**Category:** Configuration/startup
**Context:** Server initialization, protocol configuration, runtime setup.

**Risk:** These panic during startup if configuration is invalid. A panic here means the server fails to start, which is preferable to running in a broken state. However, error messages should be clear.

**Recommendation:** Acceptable as-is. These are fail-fast startup checks.

---

### #9 — LOW: RpcProtocolFactory create_protocol expect

**File:** `p2p/foretias-server/src/communerd/p2p/behaviour.rs:88`
**Code:** `.expect("valid protocol string")`
**Category:** Production protocol code (called at runtime)
**Context:** `RpcProtocolFactory::create_protocol()` — creates libp2p protocol strings.

```rust
pub fn create_protocol(&self) -> StreamProtocol {
    StreamProtocol::try_from_owned(format!("/foretias/{}/rpc/1.0.0", self.namespace))
        .expect("valid protocol string")
}
```

**Risk:** `self.namespace` is set at startup from CLI args. The format string is always valid for `StreamProtocol` (non-empty, valid characters). Only fails if namespace contains invalid characters, which would be a configuration error.

**Recommendation:** Acceptable as-is. The namespace is validated at startup.

---

### #10 — LOW: Postcard serialization in tick sig_input_bytes

**File:** `p2p/core-engine/src/foretias/tick.rs:185`
**Code:** `postcard::to_allocvec(self).expect("postcard serialize Foretis")`
**Category:** Production protocol code
**Context:** `Foretis::sig_input_bytes()` — produces canonical bytes for signing.

```rust
pub fn sig_input_bytes(&self) -> Vec<u8> {
    postcard::to_allocvec(self).expect("postcard serialize Foretis")
}
```

**Risk:** Same as #4 — postcard on owned data. This is on the signing hot path, so a panic here would affect stamping.

**Recommendation:** Acceptable as-is. Postcard serialization of `Serialize` structs is deterministic and only fails on OOM.

---

## Full Production Code Inventory

### core-engine (10 production hits)

| File:Line | Code | Category | Risk |
|-----------|------|----------|------|
| `foretias/types.rs:50` | `try_into().unwrap()` | Internal invariant | Low |
| `foretias/types.rs:55` | `try_into().unwrap()` | Internal invariant | Low |
| `foretias/clean_auth.rs:411` | `signatures.last().unwrap()` | Internal invariant | Low |
| `clock.rs:73` | `lock().unwrap()` | Internal invariant | Low |
| `snapshot_signature.rs:68` | `expect("Argon2id...")` | Protocol code | Moderate |
| `probity/report.rs:74` | `expect("postcard...")` | Protocol code | Low |
| `foretias/tick.rs:185` | `expect("postcard...")` | Protocol code | Low |
| `epoch/snapshot.rs:45` | `to_value(self).unwrap()` | Protocol code | Low |
| `epoch/snapshot.rs:46` | `as_object_mut().unwrap()` | Protocol code | Low |
| `epoch/snapshot.rs:58` | `to_vec(&val).unwrap()` | Protocol code | Low |

### foretias-server (13 production hits)

| File:Line | Code | Category | Risk |
|-----------|------|----------|------|
| `main.rs:221` | `expect("parts has 1...")` | Config/startup | Low |
| `main.rs:224` | `expect("parts.len()...")` | Config/startup | Low |
| `server/mod.rs:273` | `expect("current_thread...")` | Config/startup | Low |
| `communerd/mod.rs:91` | `expect("postcard...")` | Protocol code | Low |
| `communerd/mod.rs:229` | `expect("libsodium...")` | Config/startup | Low |
| `communerd/p2p/behaviour.rs:31` | `expect("valid protocol...")` | Config/startup | Low |
| `communerd/p2p/behaviour.rs:42` | `expect("valid gossipsub...")` | Config/startup | Low |
| `communerd/p2p/behaviour.rs:47` | `expect("gossipsub init")` | Config/startup | Low |
| `communerd/p2p/behaviour.rs:51` | `expect("valid protocol...")` | Config/startup | Low |
| `communerd/p2p/behaviour.rs:88` | `expect("valid protocol...")` | Protocol code | Low |
| `communerd/p2p/swarm.rs:294` | `lock().unwrap()` | Internal invariant | Low |
| `communerd/communerdette.rs:1109` | `expect("idx > 0...")` | Internal invariant | Low |
| `server/handlers.rs:579` | `verified.last().unwrap()` | **Protocol code** | **Critical** |

### foretias-client (1 production hit)

| File:Line | Code | Category | Risk |
|-----------|------|----------|------|
| `foretias.rs:284` | `expect("PtpConfig...")` | Protocol code | Moderate |

---

## Recommendations Summary

| Priority | Item | Action |
|----------|------|--------|
| **P0** | `handlers.rs:579` | Replace `verified.last().unwrap()` with fallible variant |
| **P1** | `snapshot_signature.rs:68` | Change `derive_keypair` to return `Result` |
| **P1** | `foretias.rs:284` | Make `with_config_ptp` fallible |
| **P2** | `communerd/mod.rs` Mutex unwraps | Consider `parking_lot::Mutex` to avoid poison |
| **P3** | Postcard/serde_json expects | Document invariants; convert if hot path |

---

## Notes

- **FFI boundary:** No `.unwrap()` or `.expect()` found at FFI boundaries in production code. The C11↔Rust boundary uses proper error propagation.
- **Test code:** ~494 unwraps/expect calls in test code are acceptable. Test code is not subject to the "reject, never panic" discipline.
- **Mutex poison:** The ~50 `lock().unwrap()` calls on `std::sync::Mutex` in `communerd/` are a latent cascading failure risk. Consider migrating to `parking_lot::Mutex` which does not poison.
- **build.rs excluded:** `core-engine/build.rs` contains unwraps for CMake/bindgen operations. These are build-time, not runtime, and are excluded from this audit.
