# POST7 Audit & Cleanup Specification

**Prefix:** `POST7_AUDIT_CLEANUP`
**Group:** POST7
**Pairs with:** `POST7_AUDIT_CLEANUP_PLAN.md`
**Status:** Draft — pending human review
**Date:** 2026-06-01
**Sources:**
- `docs/security/unwrap-audit.md` (Phase 18.6, Group 7)
- F2 Code Quality Review (Group 7 Final Verification Wave)
- F3 Real Manual QA (Group 7 Final Verification Wave)
- F4 Scope Fidelity Check (Group 7 Final Verification Wave)
- `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/issues.md` (Phase 8.2)
- `.omo/notepads/COMBINED_GROUP7_COMMUNERDETTE/learnings.md`

---

## 0. Overview

After completing Group 7 (Communerdette), multiple code review passes and audits revealed a set of findings ranging from critical security issues to informational observations. This specification consolidates ALL findings into a single document with clear categorization: what must be fixed, what should be confirmed with the human, and what is informational only.

### What This Spec Covers

1. **Unwrap/Expect Remediation** — 10 items from the Phase 18.6 audit (3 actionable fixes, 7 informational confirmations)
2. **Mutex Poison Migration** — ~50 `std::sync::Mutex` unwraps in communerd/ that could cascade on poison
3. **Dead Code Stub Cleanup** — 28 warnings in communerdette.rs for Phase 6/7/8 stubs awaiting future implementation
4. **Pre-P2P Review Findings** — Any unaddressed findings from prior code reviews

### What This Spec Does NOT Cover

- Phase 8.2 Calendar DoAttestation blockers (7-10h effort, stays as Group 7 open items, higher priority)
- Test code unwraps (~494, acceptable in test code)
- Build-time unwraps (build.rs, excluded from audit)
- Product design recommendations (README optimization, etc.) — those are separate work items

---

## 1. Unwrap/Expect Remediation

### 1.1 Actionable Fixes (P0-P1) — Will Be Fixed

#### Item #1 — CRITICAL: External input handler with unwrap

**File:** `p2p/foretias-server/src/server/handlers.rs:590`
**Code:** `let prev = verified.last().unwrap();`
**Context:** `handle_ship_ack` — processes `history_dump_ack` JSON-RPC from remote peers.

**Current State:** The unwrap is protected by control flow (i==0 branch pushes to verified before i>0 can execute). However, this is a logic-dependent invariant on external input.

**Fix:** Replace with fallible variant:
```rust
let prev = verified.last().ok_or_else(|| {
    resp_error(server, id, jsonrpc::INVALID_PARAMS,
        "chain verification: missing previous record (internal invariant violation)")
})?;
```

**Why This Matters:** A future refactor that changes the loop structure could introduce a panic-on-external-input DoS vector. Making the invariant explicit prevents this class of bugs.

---

#### Item #2 — MODERATE: Public function with crypto library expect

**File:** `p2p/core-engine/src/snapshot_signature.rs:68`
**Code:** `.expect("Argon2id hash should not fail with valid inputs")`
**Context:** `derive_keypair(passphrase: &str)` — public function deriving Ed25519 keys from passphrase.

**Current State:** Argon2 can fail under OOM conditions or with certain edge-case inputs. A panic here crashes the process.

**Fix:** Change return type to `Result<(SigningKey, VerifyingKey), SnapshotSignatureError>`:
```rust
pub enum SnapshotSignatureError {
    Argon2Failure(argon2::Error),
    // ... other errors
}

pub fn derive_keypair(passphrase: &str) -> Result<(SigningKey, VerifyingKey), SnapshotSignatureError> {
    let argon2 = Argon2::default();
    let mut hash_output = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), SALT, &mut hash_output)
        .map_err(SnapshotSignatureError::Argon2Failure)?;
    // ...
}
```

**Why This Matters:** The public API should not panic. Callers should be able to handle key derivation failures gracefully.

---

#### Item #3 — MODERATE: Client config trait impl panics on connect failure

**File:** `p2p/foretias-client/src/foretias.rs:284`
**Code:** `.expect("PtpConfig construction failed")`
**Context:** `with_config_ptp(cfg: PtpConfig)` — PtP config trait implementation.

**Current State:** `Self::connect()` is an async network operation. If the remote server is unreachable, the client panics instead of returning an error.

**Fix:** Return `Result<Self, ForetiasError>` from `with_config_ptp`:
```rust
fn with_config_ptp(cfg: PtpConfig) -> Result<Self, ForetiasError> {
    Self::connect(
        cfg.standalone.tbn,
        cfg.peers,
        cfg.timeout_secs,
        cfg.standalone.persist_path,
    )
}
```

**Why This Matters:** A network failure should not crash the client. This is a DoS vector if an attacker controls the peer list or network conditions.

---

### 1.2 Informational Items (P2-P3) — Documented, Human Confirmation Required

#### Item #4 — LOW: Postcard serialization expects (canonical methods)

**Files:**
- `p2p/core-engine/src/probity/report.rs:74` — `expect("postcard serialize ProbityReport")`
- `p2p/core-engine/src/foretias/tick.rs:185` — `expect("postcard serialize Foretis")`
- `p2p/foretias-server/src/communerd/mod.rs:91` — `expect("postcard serialize PeerRegistrationRecord")`

**Risk:** Postcard serialization on owned, well-formed structs is extremely unlikely to fail (only OOM). These are canonicalization methods used for signing.

**Recommendation:** Acceptable as-is. Document with inline comment explaining the invariant.

---

#### Item #5 — LOW: serde_json unwrap in canonical_bytes

**File:** `p2p/core-engine/src/epoch/snapshot.rs:45,46,58`
**Code:** `serde_json::to_value(self).unwrap()`, `val.as_object_mut().unwrap()`, `serde_json::to_vec(&val).unwrap()`

**Risk:** `serde_json::to_value` on a `Serialize` struct only fails for custom serializers that explicitly error. `as_object_mut()` on a struct serialization is guaranteed to return `Some`.

**Recommendation:** Acceptable as-is. The invariants are strong (struct → JSON → Vec).

---

#### Item #6 — LOW: Mutex unwrap on internal state

**Files:**
- `p2p/core-engine/src/clock.rs:73` — `self.current.lock().unwrap()`
- `p2p/foretias-server/src/communerd/p2p/swarm.rs:294` — `local_multiaddr.lock().unwrap()`
- `p2p/foretias-server/src/communerd/mod.rs` — ~20 `lock().unwrap()` calls
- `p2p/foretias-server/src/communerd/communerdette.rs` — ~30 `read().unwrap()` / `write().unwrap()` calls

**Risk:** Mutex poisoning (panic while holding lock) causes all subsequent `lock()` calls to return `PoisonError`, which `.unwrap()` turns into a panic. This is a cascading failure mode.

**Recommendation:** Addressed in Section 2 (Mutex Poison Migration).

---

#### Item #7 — LOW: Internal invariant unwraps (guaranteed by type/layout)

**Files:**
- `p2p/core-engine/src/foretias/types.rs:50` — `self.inner[..32].try_into().unwrap()`
- `p2p/core-engine/src/foretias/types.rs:55` — `self.inner[32..].try_into().unwrap()`
- `p2p/core-engine/src/foretias/clean_auth.rs:411` — `self.signatures.last().unwrap()`
- `p2p/foretias-server/src/communerd/communerdette.rs:1109` — `prev.expect("idx > 0 implies prev exists")`

**Risk:** These are provably safe given the current code structure. The `try_into()` on fixed-size slices can never fail. The `last()` after `is_empty()` check is guaranteed.

**Recommendation:** Acceptable as-is. Document with inline comments if the invariant is not obvious.

---

#### Item #8 — LOW: Configuration/startup expects

**Files:**
- `p2p/foretias-server/src/main.rs:221,224` — duration formatting
- `p2p/foretias-server/src/server/mod.rs:273` — tokio runtime builder
- `p2p/foretias-server/src/communerd/mod.rs:229` — libsodium availability
- `p2p/foretias-server/src/communerd/p2p/behaviour.rs:31,42,47,51,88` — protocol strings, gossipsub config

**Risk:** These panic during startup if configuration is invalid. A panic here means the server fails to start, which is preferable to running in a broken state.

**Recommendation:** Acceptable as-is. These are fail-fast startup checks.

---

#### Item #9 — LOW: RpcProtocolFactory create_protocol expect

**File:** `p2p/foretias-server/src/communerd/p2p/behaviour.rs:88`
**Code:** `.expect("valid protocol string")`

**Risk:** `self.namespace` is set at startup from CLI args. The format string is always valid for `StreamProtocol` (non-empty, valid characters).

**Recommendation:** Acceptable as-is. The namespace is validated at startup.

---

#### Item #10 — LOW: Postcard serialization in tick sig_input_bytes

**File:** `p2p/core-engine/src/foretias/tick.rs:185`
**Code:** `postcard::to_allocvec(self).expect("postcard serialize Foretis")`

**Risk:** Same as #4 — postcard on owned data. This is on the signing hot path, so a panic here would affect stamping.

**Recommendation:** Acceptable as-is. Postcard serialization of `Serialize` structs is deterministic and only fails on OOM.

---

## 2. Mutex Poison Migration

### 2.1 What Is Mutex Poisoning?

In Rust's `std::sync::Mutex`, if a thread panics while holding the lock, the mutex enters a "poisoned" state. All subsequent `lock()` calls return `PoisonError` instead of `LockGuard`. If the code calls `.unwrap()` on the `PoisonError`, it panics — creating a cascading failure where one panic brings down all threads that use that mutex.

**Trigger:** Any panic while holding the mutex (e.g., assertion failure, unwrap on bad data, etc.)
**Cascade:** Thread A panics → mutex poisoned → Thread B calls `.unwrap()` → Thread B panics → more mutexes poisoned → cascading failure

### 2.2 Why parking_lot::Mutex Fixes This

`parking_lot::Mutex` (and `parking_lot::RwLock`) do NOT have a poison state. If a thread panics while holding the lock, the lock is automatically released and subsequent `lock()` calls succeed normally. This eliminates the cascading failure mode entirely.

**Technical Difference:**
- `std::sync::Mutex`: Uses a flag to track poison state. Returns `Result<LockGuard, PoisonError>`.
- `parking_lot::Mutex`: No poison flag. Returns `MutexGuard` directly (no Result wrapper).

### 2.3 Performance Benefits

`parking_lot` is also faster than `std::sync` under contention:
- Uses futex-based waiting (kernel-assisted) instead of busy-waiting
- Better fairness properties (reduces starvation under high contention)
- Smaller binary size (no poison state tracking)

### 2.4 API Compatibility

`parking_lot::Mutex` is largely drop-in compatible:
- `lock()` returns `MutexGuard` directly (no `.unwrap()` needed)
- `RwLock::read()` / `write()` return guards directly (no `.unwrap()` needed)
- The guard types implement `Deref` and `DerefMut` the same way

**Migration Pattern:**
```rust
// Before (std::sync::Mutex):
let guard = self.state.lock().unwrap();

// After (parking_lot::Mutex):
let guard = self.state.lock();
```

### 2.5 Why Wasn't This Done Initially?

1. **std::sync is in the standard library** — no additional dependency needed
2. **Poisoning is rare in practice** — most codebases never encounter it
3. **The warning is subtle** — `lock().unwrap()` compiles fine; the poison risk only manifests at runtime under panic conditions
4. **Security-critical applications need this** — Foretias is one of those applications. A cascading failure in the P2P layer could affect the entire network.

### 2.6 Scope of Migration

Approximately 50 `lock().unwrap()` calls across:
- `p2p/foretias-server/src/communerd/mod.rs` (~20 calls)
- `p2p/foretias-server/src/communerd/communerdette.rs` (~30 calls — mostly `read().unwrap()` / `write().unwrap()` on RwLock)
- `p2p/foretias-server/src/communerd/p2p/swarm.rs` (1 call)
- `p2p/core-engine/src/clock.rs` (1 call — `StepClock`, test-only)

### 2.8 Research Evidence

**Benchmarks** (Cuong Le, Oct 2025 — [cuongleqq/mutex-benches](https://github.com/cuongleqq/mutex-benches)):

| Scenario | std::Mutex | parking_lot | Winner |
|----------|-----------|-------------|--------|
| Hog (1 monopolizing thread) | 820 ops/s (others: 6-16 ops) | 2,965 ops/s (others: 7,023-7,109) | **parking_lot (261% faster, prevents starvation)** |
| Burst (8 threads, periodic spikes) | 1.15M ops/s | 1.37M ops/s | **parking_lot (+18.5%)** |
| Long hold (8 threads, 500µs) | 188ms stddev | 3.67ms stddev | **parking_lot (51× more stable)** |

**Why parking_lot is better for Foretias:**
1. **No poisoning** — single panic doesn't cascade across all threads
2. **Fairness** — prevents thread starvation (critical for P2P peers that must all make progress)
3. **Timeout support** — `try_lock_for()` / `try_lock_until()` (no std equivalent)
4. **Predictable latency** — 51× more stable under heavy load
5. **Negligible downsides** — we use system allocator (no cyclic dependency), ~24 peers (negligible memory overhead)

### 2.9 API Migration Pattern

```rust
// Before:
use std::sync::{Mutex, RwLock};
let guard = self.state.lock().unwrap();
let guard = self.data.read().unwrap();

// After:
use parking_lot::{Mutex, RwLock};
let guard = self.state.lock();
let guard = self.data.read();
```

Note: `lock()` no longer returns `Result`. Code that pattern-matches on `Ok(guard)` / `Err(poisoned)` needs updating.

### 2.7 Downsides

- **Additional dependency** — `parking_lot` crate (well-maintained, widely used)
- **Slight API change** — `lock()` no longer returns `Result`, so error handling patterns change
- **No poison detection** — If you WANT to detect panics (for debugging), parking_lot doesn't provide this

---

## 3. Dead Code Stub Cleanup

### 3.1 What Are These Stubs?

The `communerdette.rs` file contains ~28 dead code warnings for types and methods that were designed for Phase 6/7/8 features but not yet implemented. These are intentional stubs — the architecture was designed ahead of implementation, but the stubs generate compiler warnings.

### 3.2 Why Address This Now?

1. **Compiler noise** — 28 warnings make it harder to spot real issues
2. **Code rot** — Stubs can drift out of sync with the actual architecture as it evolves
3. **Human inspection** — The human should review each stub to decide: keep, mark with `#[allow(dead_code)]`, or remove temporarily

### 3.3 Stub Inventory

**Root Cause:** `CommunerdetteExecutor` is private (`struct`, not `pub`), so the `pub(super) fn spawn_*` methods that accept `Arc<CommunerdetteExecutor>` cannot be called from outside the module. The only live entry point is `trigger_channel_bind` (called from mod.rs), which creates its own executor inline. The `CommunerdetteLine` methods bypass the queue entirely — they create executors inline and call `execute_*` directly.

#### Item 1 — `bridge_dht_to_communerdette` (mod.rs:1051)

- **What:** Method that bridges DHT records into Communerdette state
- **Phase:** 3.1 — DHT Claim and Route Refresh
- **Why dead:** Gossip event loop does the same thing inline (mod.rs:640-656). Method was factored out but never wired up.
- **Recommendation:** **(a) Wire up** — replace inline block with method call (~5 min)

#### Item 2 — `CommunerdetteRouteStats` (communerdette.rs:169)

- **What:** Per-route statistics: success/failure counts, smoothed RTT, consecutive failures
- **Phase:** 6 — Liveness and Stats Refactor
- **Why dead:** `record_success`/`record_failure` only called from `spawn_queue_task` (also dead). `route_stats_snapshot` never called.
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete, well-tested. Keep for Phase 5.2 integration (~2 hrs to revive)

#### Item 3 — `ChannelBindingState` (communerdette.rs:429)

- **What:** Three-state enum: `Unbound`, `FullyBound`, `Rejected`
- **Phase:** 12.0 — Channel-Binding Establishment
- **Why dead:** Not stored in `CommunerdetteState`. Binding task returns result directly instead of updating state.
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-correct enum. Missing state field is the gap (~1 hr to revive)

#### Item 4 — `CommunerdettePriority` (communerdette.rs:881)

- **What:** Four-level priority: `Bulk < Normal < High < Critical`
- **Phase:** 5.1 — Request Priority
- **Why dead:** Only used by `QueuedCommand` and `spawn_queue_task` (both dead).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete with unit tests (~3 hrs to revive)

#### Item 5 — `CommunerdetteCommand` (communerdette.rs:897)

- **What:** Command enum: `CalendarSlice`, `Stamp`, `Shutdown`
- **Phase:** 5.2 — Queue Worker
- **Why dead:** Only consumed by `spawn_queue_task` (dead).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete. Keep paired with Priority and QueuedCommand (~3 hrs to revive)

#### Item 6 — `QueuedCommand` (communerdette.rs:938)

- **What:** Wraps Priority + sequence + Command. Has `Ord` impl for BinaryHeap ordering.
- **Phase:** 5.2 — Queue Worker
- **Why dead:** Only used inside `spawn_queue_task` (dead).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete with unit tests (~3 hrs to revive)

#### Item 7 — `spawn_queue_task` (communerdette.rs:1253)

- **What:** Priority queue worker loop — receives commands, processes highest-priority first
- **Phase:** 5.2 — Queue Worker
- **Why dead:** **Never called.** `CommunerdetteLine` methods bypass queue entirely.
- **Recommendation:** **(b) `#[allow(dead_code)]`** — most significant dead item. Spec-complete, represents intended architecture (~4-6 hrs to revive)

#### Item 8 — `spawn_channel_bind_task` (communerdette.rs:1327)

- **What:** Spawns task for channel binding: nonce, DHT resolution, challenge RPC, dual-key verification
- **Phase:** 12.0 — Channel-Binding Establishment
- **Why dead:** **Double-spawn bug.** `trigger_channel_bind` spawns a task that calls `spawn_channel_bind_task`, which spawns another task. Compiler flags it dead because the outer spawn captures the call.
- **Recommendation:** **(a) Fix double-spawn** — refactor `trigger_channel_bind` to call `spawn_channel_bind_task` directly (~30 min)

#### Item 9 — `spawn_l1_liveness_task` (communerdette.rs:1429)

- **What:** L1 liveness loop — checks application RPC success, falls back to transport ping
- **Phase:** 12.1 — L1 Liveness
- **Why dead:** **Never spawned.** `LivenessCycleFlags` are written by Line methods but no L1 task reads them.
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete with unit tests (~1 hr to revive)

#### Item 10 — `spawn_l2_liveness_task` (communerdette.rs:1558)

- **What:** L2 liveness loop — fast-key authenticated ping, `AuthenticatedPong` verification
- **Phase:** 12.3 — L2 TBID Identity Confirmed
- **Why dead:** **Never spawned.** Depends on L1 being live.
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete with unit tests (~1 hr to revive)

#### Item 11 — `AuthenticatedPong` + envelope (communerdette.rs:1498-1551)

- **What:** Authenticated pong struct + envelope type with `from_json_value` and `verify` methods
- **Phase:** 12.3 — L2 TBID Identity Confirmed
- **Why dead:** Only consumed by `spawn_l2_liveness_task` (dead).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete types. Dead only because L2 task not spawned (~0 min to revive)

#### Item 12 — `spawn_l3_liveness_task` (communerdette.rs:1633)

- **What:** L3 liveness loop — full stamp + Take 3 gate verification
- **Phase:** 12.4 — L3 Chronomatter Responsive
- **Why dead:** **Never spawned.** Depends on L2 being live + Phase 11 (PQC genesis).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — spec-complete with unit tests (~1 hr to revive)

#### Item 13 — Mirror RPC stubs (communerdette.rs:823-855)

- **What:** Five methods: `mirror_request`, `ship_batch`, `ship_ack`, `stream_tick`, `mirror_reconcile`
- **Phase:** 8 — Calendar and Verification Integration
- **Why dead:** Group 4 mirror work is DEFERRED pending Communerdette completion. Intentional placeholders.
- **Recommendation:** **(a) Keep as stubs** — define API surface for when mirror work resumes (8-16 hrs to implement)

#### Item 14 — Unused `CommunerdetteHost` trait methods

- **What:** Six methods: `host_execute_channel_bind_challenge`, `host_execute_ping`, `host_sign_probity_report`, `host_publish_probity_report`, `host_lookup_tbid_cached`
- **Phase:** 3.3/12 — Host helper surface
- **Why dead:** Only called from dead `spawn_*` tasks. `host_lookup_tbid_cached` IS called from `refresh_dht_binding` (live).
- **Recommendation:** **(b) `#[allow(dead_code)]`** — keep trait surface intact for future integration

#### Item 15 — `choose_route` (communerdette.rs:564, no `_with_host` suffix)

- **What:** Route selection based on `route_stats` HashMap scoring
- **Phase:** 3.2 — Relationship Route State
- **Why dead:** Live code uses `choose_route_with_host` (same logic + swarm availability checks). Plain variant never called.
- **Recommendation:** **(c) Remove** — strict subset of `choose_route_with_host` with no unique behavior (~15 min)

### 3.4 Summary

| # | Item | Phase | Recommendation | Effort |
|---|------|-------|---------------|--------|
| 1 | `bridge_dht_to_communerdette` | 3.1 | (a) Wire up | 5 min |
| 2 | `CommunerdetteRouteStats` | 6 | (b) `#[allow(dead_code)]` | 2 hrs |
| 3 | `ChannelBindingState` | 12.0 | (b) `#[allow(dead_code)]` | 1 hr |
| 4 | `CommunerdettePriority` | 5.1 | (b) `#[allow(dead_code)]` | 3 hrs |
| 5 | `CommunerdetteCommand` | 5.2 | (b) `#[allow(dead_code)]` | 3 hrs |
| 6 | `QueuedCommand` | 5.2 | (b) `#[allow(dead_code)]` | 3 hrs |
| 7 | `spawn_queue_task` | 5.2 | (b) `#[allow(dead_code)]` | 4-6 hrs |
| 8 | `spawn_channel_bind_task` | 12.0 | (a) Fix double-spawn | 30 min |
| 9 | `spawn_l1_liveness_task` | 12.1 | (b) `#[allow(dead_code)]` | 1 hr |
| 10 | `spawn_l2_liveness_task` | 12.3 | (b) `#[allow(dead_code)]` | 1 hr |
| 11 | `AuthenticatedPong` + envelope | 12.3 | (b) `#[allow(dead_code)]` | 0 min |
| 12 | `spawn_l3_liveness_task` | 12.4 | (b) `#[allow(dead_code)]` | 1 hr |
| 13 | Mirror RPC stubs (5 methods) | 8 | (a) Keep as stubs | 8-16 hrs |
| 14 | Unused `CommunerdetteHost` methods | 3.3/12 | (b) `#[allow(dead_code)]` | Varies |
| 15 | `choose_route` (no suffix) | 3.2 | (c) Remove | 15 min |

---

## 4. Pre-P2P Review Findings

### 4.1 Test That Enshrines a Known Bug

**File:** `core-engine/src/foretias/calendar.rs:464-483`
**Test:** `calendar_crash_recovery_corrupt_tmp`

**Issue:** The test documents that "corrupt .tmp not removed by current implementation" and **asserts the buggy behavior** rather than testing the correct behavior.

**Recommendation:** Either fix the bug and update the test, or mark the test `#[should_panic]` with a `FIXME` comment.

---

## 5. Summary of Actions

| Category | Actionable | Informational | Total |
|----------|-----------|---------------|-------|
| Unwrap/Expect | 3 fixes | 7 confirmations | 10 |
| Mutex Poison | 1 migration | 0 | 1 |
| Dead Code | TBD (per-stub) | 0 | TBD |
| Pre-P2P Findings | 1 fix | 0 | 1 |
| **TOTAL** | **~5-10** | **7** | **~12-17** |

---

## 6. Verification Strategy

Each fix will be verified by:
1. **Compilation** — `cargo check --workspace` passes
2. **Tests** — `cargo test --workspace` passes
3. **Regression tests** — New tests added for each fix to prevent regression
4. **Human confirmation** — Informational items confirmed with human before marking complete

---

## 7. Success Criteria

- All 3 actionable unwrap fixes implemented and tested
- Mutex poison migration complete (~50 calls migrated)
- Dead code stubs reviewed and addressed (per-stub decisions)
- Pre-P2P finding (corrupt .tmp test) addressed
- All informational items confirmed with human
- `cargo test --workspace` passes with 0 failures
- No new warnings introduced
