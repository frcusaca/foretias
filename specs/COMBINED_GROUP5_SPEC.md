# COMBINED_GROUP5_SPEC.md
# Architectural Foundations — Clock Injection, Crash Recovery, JSON-RPC Auth, AGENTS.md Gap-Fill

**Date:** 2026-05-22 (re-verified against alpha @ 80f0714)
**Status:** Approved (Section C requires human decision before Phase 3 starts)
**Paired Plan:** `COMBINED_GROUP5_PLAN.md`
**Master Coordination:** `COMBINED_GROUP2_SPEC.md` §2.2

---

## 1. Verification Discovery (2026-05-22)

| Original item | Status | Evidence |
|---------------|--------|----------|
| `Clock` trait does not exist | ❌ Wrong — IT EXISTS | `p2p/core-engine/src/clock.rs`; trait method is `now_ns() -> Result<u64, ClockError>` |
| `MockClock` needs to be added | ❌ Wrong — `FixedClock` and `StepClock` ALREADY EXIST | `clock.rs:40-78` |
| `Chronomatter` does not use Clock | ❌ Wrong — IT DOES | `chronomatter/mod.rs:37` field, `:344` use |
| `tick.rs` does not use Clock | ❌ Wrong — IT DOES | `tick.rs:195` use |
| `thread_rng()` is used in tests (non-deterministic) | ✅ DONE | 0 hits in workspace |
| Unverified-vs-Verified rule missing from AGENTS.md | ✅ DONE | `AGENTS.md:880-892` |
| Length-validation at FFI rule missing | ⚠️ PRESENT | `AGENTS.md` has FFI section; explicit "every C `*_with_len` must validate" rule should be verified |

What **is** still genuinely open:

| Open item | Severity | Where |
|-----------|----------|-------|
| 17 sites still call `SystemTime::now()` directly | HIGH | enumerated in §2.1 |
| `Chronomatter::new` constructs its own `SoftwareCryptoServer` | MED | `chronomatter/mod.rs:50-52` |
| `calendar_crash_recovery_corrupt_tmp` test asserts buggy behavior | MED | `foretias/calendar.rs:520` |
| JSON-RPC auth | MED (decision required) | architectural — see §4 |
| Some AGENTS.md rules from review §1.5 may still be absent | MED | inventory needed (§5) |

---

## 2. Section A — Clock Injection Sweep (Phase 2)

**Severity:** HIGH (panics on edge-case clocks; non-deterministic tests)
**Branch:** `g5-a-clock-inject`
**Phase:** 2 (alone — broad surface, blocks Phase 3)
**Why alone:** touches 17 sites across 8 files; doing it in parallel with anything
that also modifies those files guarantees merge conflicts.

### 2.1 Sites to convert

Exact locations as of 2026-05-22:

| # | File | Line | Current call |
|---|------|------|--------------|
| 1 | `p2p/core-engine/src/collision/detector.rs` | ~79 | `SystemTime::now().duration_since(UNIX_EPOCH)` |
| 2 | `p2p/core-engine/src/epoch/scheduler.rs` | ~33 | same |
| 3 | `p2p/foretias-server/src/calendar_store/encrypted_jsonl.rs` | ~64 | same (for `written_at_ns`) |
| 4 | `p2p/foretias-server/src/calendar_store/lru.rs` | ~191 | same |
| 5 | `p2p/foretias-server/src/main.rs` | ~260 | same |
| 6 | `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs` | ~67-68 | same |
| 7-14 | `p2p/foretias-server/src/communerd/mod.rs` | 357, 401, 435, 471, 509, 543, 636, 709 | same (8 sites, mostly `registered_at_ns` / `last_seen_ns`) |
| 15 | `p2p/foretias-client/src/foretias.rs` | ~529 | same |
| (intentional) | `p2p/core-engine/src/clock.rs` | 32 | inside `SystemClock::now_ns()` — DO NOT change |

Run `grep -rn "SystemTime::now\(\)" p2p/{core-engine,foretias-client,foretias-server}/src` at start of work to get current line numbers — they may have drifted slightly.

### 2.2 Fix pattern

For each non-clock-.rs site:
1. Wherever the function or struct already has access to `Arc<dyn Clock>` (e.g.,
   inside `Chronomatter` methods), use `self.clock.now_ns()?`.
2. Wherever it doesn't, **plumb a `clock: Arc<dyn Clock>` parameter** through
   the struct or function signature. This is the bulk of the work.
3. Map the existing `.unwrap_or(0)` fallback to `.unwrap_or(0)` on the
   `now_ns()?` result OR (preferred) propagate the error.

Each call should become:
```rust
let now_ns = self.clock.now_ns()
    .map_err(|e| NodeError::Internal(format!("clock: {e}")))?;
```
or, for sites where infallibility is documented:
```rust
let now_ns = self.clock.now_ns()
    .expect("invariant: system clock is after UNIX_EPOCH");
```

### 2.3 Plumbing changes (struct/function signature changes)

The largest signature changes are in `Communerd::new` — it must accept
`clock: Arc<dyn Clock>` and store it as a struct field. Then the 8 sites
inside `communerd/mod.rs` use `self.clock.now_ns()`.

Similarly `TbidHandshake` (which lives inside `Communerd`) must hold an
`Arc<dyn Clock>`.

`main.rs:260` is at program startup; pass `Arc::new(SystemClock) as Arc<dyn Clock>` into the top-level construction.

`CalendarStore` (and `EncryptedJsonlCalendarStore`) — pass `clock` into the
constructor, store as field.

### 2.4 DI consistency fix (bundled with clock)

`Chronomatter::new` at lines 50-52 constructs its own `SoftwareCryptoServer`:
```rust
let crypto: Arc<dyn CryptoServer> = Arc::from(crypto_server::new_software(
    crypto_server::ForetiasCurve::Ed25519,
)?);
```

**Fix:** Change `Chronomatter::new` to accept `crypto: Arc<dyn CryptoServer>`
as a parameter (matching `Chronomatter::from_calendar`). `TimeFamilyServer::new`
(currently at `server/mod.rs:69`) becomes the site that picks the production
implementation:
```rust
let crypto = Arc::from(crypto_server::new_software(ForetiasCurve::Ed25519)?);
let clock = Arc::new(crate::clock::SystemClock) as Arc<dyn Clock>;
let mut cm = Chronomatter::new(chronon_ns, calendar_observer, crypto, clock)?;
```

Tests construct with `FixedClock` and a test-double `CryptoServer` if desired.

### 2.5 Acceptance

- [ ] `grep -rn "SystemTime::now()" p2p/{core-engine,foretias-client,foretias-server}/src` → matches only the `clock.rs:32` site
- [ ] `Chronomatter::new` takes `crypto: Arc<dyn CryptoServer>` explicitly
- [ ] `TimeFamilyServer::new` passes `Arc::new(SystemClock)` and a production crypto
- [ ] At least one existing test rewritten to use `FixedClock` (proves the seam works)
- [ ] `cargo test --workspace` passes deterministically across two consecutive runs

---

## 3. Section B — Crash Recovery Test Fix (Phase 1)

**Severity:** MED (a passing test enshrines a known bug)
**Branch:** `g5-b-crash-recovery`
**Phase:** 1
**Files modified:**
- `p2p/core-engine/src/foretias/calendar.rs` (production code + test)

### 3.1 Problem

`p2p/core-engine/src/foretias/calendar.rs:506-524` defines
`calendar_crash_recovery_corrupt_tmp`, which **passes** today because line 520
asserts:
```rust
assert!(Path::new(&tmp_path).exists(), "corrupt .tmp not removed by current implementation");
```
The test confirms the implementation **does not** clean up a corrupt `.tmp`
file — i.e., the test enshrines buggy behavior as the spec.

### 3.2 Fix

1. **Modify `Calendar::load`** (or whichever method scans for a `.tmp` recovery
   file) to: on detecting a `.tmp` file that fails to deserialize, **remove it**
   and log a warning. Loading proceeds from the main file. This is the safe
   crash-recovery behavior.
2. **Update the test** to assert the corrected behavior:
   ```rust
   // After load, corrupt .tmp must be removed; the loader can either succeed
   // from the main file or fail with a clear error — but the corrupt .tmp must
   // not be left lying around.
   assert!(!Path::new(&tmp_path).exists(),
       "corrupt .tmp must be removed by crash-recovery scan");
   ```
3. **Add a sibling test** that verifies the warn log is emitted when a corrupt
   `.tmp` is encountered (optional — only if there is an existing log-capture
   infrastructure; otherwise skip).

### 3.3 Acceptance

- [ ] `calendar_crash_recovery_corrupt_tmp` test asserts the corrupt `.tmp` is
      removed (not retained)
- [ ] All existing calendar tests continue to pass
- [ ] No new failure modes for legitimate recovery scenarios

---

## 4. Section C — JSON-RPC Authentication

**Severity:** MED (operational security)
**Branch:** `g5-c-rpc-auth`
**Phase:** 3 (requires human decision before work starts)
**Status:** ⏸️ **BLOCKED — awaiting human decision**

### 4.1 Decision required

JSON-RPC currently has no authentication. Any peer reachable on the TCP port
can issue stamp/verify/calendar-slice requests. The transport-layer Noise_XX
session proves the other party knows its private key, but does not enforce
per-method authorization.

**Choose one (human):**

#### Option A — Bearer token
- **What:** A shared secret (preconfigured per `TimeFamilyConfig`) included in
  a JSON-RPC field or HTTP-equivalent header
- **Pros:** Simple; low operational overhead; trivial to revoke
- **Cons:** Requires token distribution out-of-band; rotation is operational pain

#### Option B — mTLS / TBID-pinned client certificate
- **What:** Each peer presents a client certificate; server validates the cert
  chains to the peer's claimed TBID
- **Pros:** Strong identity; reuses the cryptographic infrastructure already in place
- **Cons:** Higher complexity; requires certificate management tooling

#### Option C — Defer
- **What:** Document explicitly in `AGENTS.md` and `HOWTO.md` that JSON-RPC has
  no auth and relies on network-layer protection (firewall, VPN, mTLS in front)
- **Pros:** Smallest scope change
- **Cons:** Operationally risky for any internet-facing deployment

**Recommended:** Option A as v1, Option B as later hardening, Option C documented
in the interim. The Plan blocks until this is resolved.

### 4.2 Once decided (Option A skeleton)

If A: Add `auth: Option<AuthConfig>` to `TimeFamilyConfig`. In `server/mod.rs`,
intercept each request before dispatch and reject if no/wrong token. Exempt
`ping` (liveness probes must work without credentials). Update `HOWTO.md`
operator section. Add integration tests: unauthenticated → 401; authenticated → 200.

---

## 5. Section D — AGENTS.md Gap-Fill (Phase 1)

**Severity:** LOW (documentation for reviewers)
**Branch:** `g5-d-agents-rules`
**Phase:** 1
**Files modified:**
- `AGENTS.md`

### 5.1 Inventory current state

Run before any edits:
```bash
grep -in "secret material\|zeroize\|atomic counter\|fetch_add\|unverified\|verified type\|length valid\|stub" AGENTS.md
```

Note: AGENTS.md already has:
- "Unverified vs Verified" rule (line 880-892)
- Some FFI-related rules
- A reference to secret material at line 1049

### 5.2 Rules to add (only if missing — verify with grep first)

The reviewer must check each before adding; do not duplicate. Each addition
goes into the most appropriate existing subsection.

#### 5.2.1 "Secret Material Handling" — extend if not full

Verify the AGENTS.md secret-material rules already say all of:
- Wrap in `zeroize::Zeroizing<T>` or opaque handle
- No `Debug` derive on secret types; use `.no_debug()` for bindgen
- No `Clone`/`Copy`/`Serialize` unless protocol-required; clones must each be Zeroizing

If any of those bullets is missing, add the missing ones.

#### 5.2.2 "Atomic Counter Idioms" — likely missing; add

If `grep "fetch_add\|Atomic Counter" AGENTS.md` shows no rule about CAS-as-counter,
add (under "Concurrency and Async"):

> **Atomic Counter Idioms.** Use `fetch_add`, `fetch_sub`, `fetch_or` for
> unconditional read-modify-write on `Atomic*`. Reserve `compare_exchange` for
> operations that branch on the previous value's content. CAS-as-counter creates
> a spurious failure mode under contention and is forbidden.

#### 5.2.3 "Stubs return errors, not false-success" — likely missing; add

If `grep "stub.*returns.*success\|todo!\|Unsupported" AGENTS.md` does not have
a "stubs return errors" rule, add (under "API Design"):

> **Stubs return errors, not false success.** A handler that has not yet
> implemented its operation must return an explicit error (`Err(NodeError::Unsupported(...))`,
> JSON-RPC error object, etc.) rather than a success value like `valid: true` or
> a zero signature. Mark with `todo!("TRACKING: <issue>")` for cases where a
> panic is acceptable in dev-only paths.

#### 5.2.4 "FFI length validation at both layers" — verify

If existing FFI section does not explicitly require validation in BOTH C and
Rust wrappers, add the dual-layer requirement.

### 5.3 Acceptance

- [ ] Each of the 4 candidate rules either confirmed present or newly added
- [ ] `git diff AGENTS.md` shows only additive changes (no deletions of existing rules)
- [ ] AGENTS.md still parses as well-formed Markdown

---

## 6. Out of Scope for Group 5

- Full clock-rewriting of `clock.rs` itself (it is the implementation)
- Production-grade `MockClock` API redesign (existing `FixedClock`/`StepClock` suffice)
- Full FROST epoch implementation (separate major)
- Replacing PQC algorithms or KEMs
- Performance optimization
- Network-layer firewall configuration
