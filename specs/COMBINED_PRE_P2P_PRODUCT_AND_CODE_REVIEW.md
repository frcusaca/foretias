# Foretias — Combined Pre-P2P Product & Code Review

**Date:** 2026-05-19
**Synthesis of:**
- `specs/CLAUDE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` (Claude Sonnet 4.6, 2026-05) — 1753 lines, product/code/security
- `specs/OPENCODE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` (OpenCode 1.14.28; Qwen3.6-27B-AWQ-BF16-INT4, 2026-05) — 770 lines, multi-perspective with 4 parallel explore agents
- `specs/pre-public-mvp-code-review.md` (Claude Opus 4.7, 2026-05) — 409 lines, prior technical review (already cross-referenced in CLAUDE doc §34)

**Synthesis author:** Claude Opus 4.7 (max effort), 2026-05-19
**Scope of P2P:** Implemented but not integration-verified against live multi-node networks
**Goal of this document:** Combine all findings from prior reviews into a single, organized, prioritized substrate from which SPEC.md and PLAN.md files can be generated. Two areas — **Rust coding style** and **cybersecurity/threat/formalism/testing** — are treated at the top with substantial depth; remaining concerns are attached afterward without omission.

---

## Reading Order for Subsequent SPEC/PLAN Generation

1. **Part I (§1)** — Rust style audit + AGENTS.md update recommendations. Read this before writing any code-quality SPEC.
2. **Part II (§2)** — Threat model, security findings, formalism, testing. Read this before writing any security SPEC.
3. **Part III (§3-§14)** — Everything else: product, architecture, P2P, spec drift, efficiency, future directions, prioritized improvement lists.
4. **Part IV (§15)** — Per-file risk table for triage.
5. **Part V (§16-§19)** — Open questions, application use cases, release path, closing.

Each finding is annotated with file:line where possible, a severity tag (**CRIT/HIGH/MED/LOW**), and a `[Sources: ...]` marker showing which prior review surfaced it. A finding appearing in both prior reviews is marked `[Sources: CLAUDE+OPENCODE]` (or with the `pre-public-mvp` review's tag where relevant). The annotations exist so a downstream SPEC author can trace any claim back to its source review.

---

# Part I — Rust Coding Style, Conventions, and Practices

This part of the review is the user's first priority and is treated in depth. It audits the codebase against `AGENTS.md §"How To Write Rust Code"`, identifies adherence and violations, and recommends concrete additions to `AGENTS.md` itself.

## §1.0 Why This Part Comes First

The Foretias project's `AGENTS.md` already contains a thoughtful, opinionated Rust style chapter (lines 559-1093). It correctly prioritizes correctness > readability > testability > efficiency > style, and it explicitly addresses the failure modes most relevant to a cryptographic timestamping system: panics in protocol logic, locks across `.await`, validate-before-trust patterns, secrets-in-logs, FFI safety, narrow APIs, and constant-time comparisons.

**The gap is enforcement.** Several findings catalogued in the prior reviews are direct violations of guidance already written in `AGENTS.md`. This means either (a) the guidance is not being consulted during implementation, (b) the guidance lacks specific rules to make the violation obvious to a reviewer, or (c) the codebase predates the guidance and has not been swept for compliance. A targeted SPEC + PLAN to bring the codebase into compliance is the most effective short-term path; longer-term, additions to `AGENTS.md` (proposed in §1.5) close the residual rule gaps.

## §1.1 Summary of Current AGENTS.md Rust Guidance

For convenience to anyone reading this document standalone, the key rules from `AGENTS.md §"How To Write Rust Code"` are:

| Rule | Source | Status in current code |
|---|---|---|
| Correctness > readability > testability > efficiency > style | AGENTS.md "Optimize in this order" | **Mostly honored** |
| Prefer explicit data flow, small functions | "General Rust Style" | **Honored in core-engine; weakening in foretias-server handlers** |
| No panics in library/protocol logic | "General Rust Style" + "Panics and Assertions" | **Violated** in 8+ places (see §1.3.1) |
| Validate before trust | "Cryptographic and Security-Sensitive Code" | **Violated** in cross-node verify, ship_ack, gossip handler |
| Distinguish verified/unverified types | "Cryptographic and Security-Sensitive Code" | **Honored** for TBID but **violated** for inbound calendar records |
| Inject clocks, do not call system time deep in protocol logic | "Time Handling" | **Mostly violated** — `SystemTime::now()` direct in tick.rs |
| Use newtypes for semantically distinct values | "API Design" | **Honored** for `Tbid`, `SignatureBytes`, `PublicKeyBytes` |
| Constructors must validate | "API Design" | **Partially** — `Tbid::from_raw` checked, but `PeerRegistrationRecord` not signed |
| Do not log secrets | "Logging and Observability" | **At risk** — `Debug` derive on secret bindings (see §1.3.5) |
| FFI must validate pointers, lengths, ownership | "FFI and C11 Core Boundaries" | **Partial** — some boundaries lack length validation (see §1.3.6) |
| Every `unsafe` block must have a safety comment | "FFI and C11 Core Boundaries" | **Mostly honored** — needs audit |
| Avoid holding locks across `.await` | "Concurrency and Async" | **Violated** in chronomatter (see §1.3.4) |
| Reject malformed/non-canonical/trailing data | "Serialization and Parsing" | **Violated** — `#[serde(default)]` on `stamps_per_tick` |
| Deterministic tests with injected clocks/RNGs/storage | "Testing Requirements" | **Inconsistent** — many tests use `rand::thread_rng()` non-deterministically |

## §1.2 Adherence Audit — Where Code Faithfully Follows the Guidance

These should be celebrated and pointed at as exemplars when writing future code:

1. **`Tbid`, `TbidSecret`, `SignatureBytes`, `PublicKeyBytes` newtypes** (`p2p/core-engine/src/foretias/types.rs`)
   `[Sources: OPENCODE §2.2]` — Strong types prevent confusion between byte arrays of different meaning. `TbidSecret` correctly avoids `Debug` derive.

2. **`CryptoServer` trait abstraction** (`p2p/core-engine/src/crypto_server/mod.rs`)
   `[Sources: OPENCODE §2.3]` — Algorithm-agile trait is the right shape; it satisfies AGENTS.md "Good traits are small, named after capabilities, and have stable semantics."

3. **`PrivKeyHandle` opaque pattern + C11-side `foretias_memzero`** (`p2p/core-engine/src/core/identity.rs`, C11 `memzero.c`)
   Ed25519 tick private keys are zeroized on drop via the C11 path. Spec §0.3 "private keys never persisted" is honored at this layer.

4. **`Zeroizing<...>` wrappers around PQC secrets** in `SoftwareCryptoServer` (`software.rs:34, :38-46`)
   `[Sources: CLAUDE §19, OPENCODE §3.9]` — The Dilithium drop bug from the prior review (`pre-public-mvp-code-review.md §2.3`) is fixed; all four PQC secret slots and the seal key now use `Zeroizing`.

5. **Three-crate dependency chain** (`p2p/core-engine` → `p2p/foretias-client` → `p2p/foretias-server`)
   `[Sources: OPENCODE §2.1]` — Crate boundaries match domain boundaries (cryptographic primitives, client logic, server/P2P glue). The boundary discipline supports `AGENTS.md "Organize code by responsibility."`

6. **Domain error enums** (`NodeError`, `CryptoError`)
   `[Sources: OPENCODE §2.4]` — Variant-per-failure-mode follows `AGENTS.md "Prefer domain errors."`

7. **`Clock` trait exists** (`p2p/core-engine` clock abstraction)
   The trait is the right shape per `AGENTS.md "Time Handling"`. It is, however, inconsistently used (see §1.3.7).

## §1.3 Adherence Gaps — Specific Violations of AGENTS.md Rules

Each gap below is a direct violation of guidance already in AGENTS.md. Fixing these is the highest-leverage Rust style work because the rule is uncontroversial — the spec has already been agreed.

### §1.3.1 Panics in Production Paths (Violates "Avoid panics in library or protocol logic")

| File:Line | Issue | Source |
|---|---|---|
| `tbid_handshake.rs:117` | `payload_nonce: [u8; 32] = response.signed_payload[nonce_offset..nonce_offset + 32].try_into().unwrap()` — panics on malformed remote message | Read in this session |
| `chronomatter/mod.rs:67-70` | `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` — panics if clock before 1970 | CLAUDE §4 |
| `tbid_handshake.rs:67-70` | Same `unwrap()` on `SystemTime::duration_since` | Read in this session |
| `server/mod.rs:166` | `json_path.to_str().unwrap()` — panics on non-UTF-8 file path (Linux paths are arbitrary bytes) | pre-public-mvp §3.3 |
| `handlers.rs:138` | `serde_json::from_value(v.clone()).ok()` — silently discards parse errors (different anti-pattern, same root cause: error not surfaced) | pre-public-mvp §4 |
| `tick.rs:88-91` | `SystemTime::now()` direct call (panic potential + violates "Inject a clock") | pre-public-mvp §3.2 |

**Fix pattern:** Replace `.unwrap()` with `?` propagating a typed `NodeError::Internal("descriptive context")` or, where the value cannot be missing in any well-formed input, document the invariant with `expect("invariant: ...")` rather than bare `unwrap()`.

**SPEC seed:** "Sweep all `unwrap()` and `expect()` in `p2p/core-engine/`, `p2p/foretias-client/`, `p2p/foretias-server/` — categorize as (a) invariant-based, (b) error-propagation-needed, (c) bug. Replace (b) with `?`-propagation; document (a) with `expect("invariant ...")`."

### §1.3.2 Validate-Before-Trust Violations (Violates "Validate before trust")

| File:Line | Issue | Source |
|---|---|---|
| `handlers.rs:465-482` (`handle_ship_ack`) | Calls `mirror_store.insert_mirrored()` without running `integrity_check` on inbound records | CLAUDE §4.4, OPENCODE §4.5 |
| `handlers.rs:191-237` (`cross_node_verify`) | Uses public key from remote calendar record directly without chain-of-trust verification; DHT entry is itself unauthenticated | OPENCODE §3.3 |
| `probity/gossip_handler.rs:36-51` | Only checks `signature.len() >= 64` — does NOT verify Ed25519 signature against reporter's public key. **Trust model fully bypassed.** | OPENCODE §3.4 |
| `tick.rs:308-317` (genesis path) | If `forward_foretis.len() < 64`, malformed signature treated as Ed25519 component; `forward_genesis` becomes empty; for `tb_version == 0`, empty genesis is accepted as valid | OPENCODE §3.6 |
| `communerd/mod.rs:586-605` (DHT lookup) | `PeerRegistrationRecord` from DHT is consumed without signature verification — TBID not cryptographically bound to peer_id | CLAUDE §4.3, OPENCODE §3.2 |
| `handlers.rs:354-376` (`handle_verify_epoch_snapshot`) | Always returns `valid: true` regardless of signature | CLAUDE §15.1, OPENCODE §3.5 |
| `handlers.rs:125-135` (`handle_verify` local path) | Local verification passes `foretis.signature_algorithm` to `server.verify_with()` without checking `rec.signature_algorithm == foretis.signature_algorithm`. The cross-node path (`cross_node_verify`) does perform this check — an **asymmetric validation gap** between two code paths in the same function. A locally forged Foretis with mismatched algorithm string could be verified against the wrong key type. | **[NEW — Qwen3.6-27B-AWQ-BF16-INT4, 2026-05-20]** — Prior review covered `cross_node_verify` but not the local path within `handle_verify`. |

**Fix pattern:** Adopt the `UnverifiedX` / `VerifiedX` newtype distinction described in `AGENTS.md "Cryptographic and Security-Sensitive Code"`. Only trusted constructors create verified types. Each handler taking inbound bytes from network or DHT must build `UnverifiedFoo`, call `verify(...)`, then unwrap into `VerifiedFoo` before any downstream code consumes it.

**SPEC seed:** "Introduce `UnverifiedChrononRecord`, `UnverifiedProbityReport`, `UnverifiedPeerRegistration`, `UnverifiedEpochSnapshot` newtypes. Refactor all RPC and gossip handlers to construct only the unverified variant from wire bytes. Each variant has exactly one `into_verified(crypto: &dyn CryptoServer) -> Result<Verified..., VerifyError>` method. Downstream code accepts only `Verified...` types."

### §1.3.3 Concurrent Counter via CAS Instead of `fetch_add` (Violates "Simple ownership over shared mutable state")

`chronomatter/mod.rs:302-320, :421-443` — Both `stamp()` and `daemon_tick()` use `compare_exchange` on `current_tick`. CAS is the wrong primitive here: there is no condition being checked, just an unconditional increment. CAS failures under concurrent load convert into `NodeError::Internal("tick counter conflict")` returned to the caller — observable as flaky stamp failures.

`[Sources: CLAUDE §2.4, OPENCODE §2.5, AGENTS.md "concurrent counters" — already mandated]`

`AGENTS.md` does not yet contain a specific rule about CAS vs `fetch_add`. The rule should be added (proposed in §1.5).

**Fix:** Replace with `current_tick.fetch_add(1, SeqCst)` — atomic, no failure mode.

**SPEC seed:** "Audit all uses of `compare_exchange`, `compare_and_swap`, and CAS-loops in the workspace. Categorize as (a) genuine read-modify-write needing the previous value's content for a conditional, (b) unconditional increment misusing CAS. Replace (b) with `fetch_add`/`fetch_sub`/`fetch_or`/etc. Add lint or grep-based pre-commit check."

### §1.3.4 Locks Held Across `.await` (Violates "Avoid holding locks across `.await`")

`chronomatter/mod.rs:140-149` (`generate_and_store_keypair`) — Holds `keypairs` write lock for the duration of `PrivKeyHandle::generate()`, which calls into libsodium and (optionally) the PQC stack. While `PrivKeyHandle::generate()` itself is synchronous, it is on the hot stamping path. The pattern violates the spirit of the rule (don't hold contention-creating locks during slow work).

`[Sources: CLAUDE §2.3, §7.3, OPENCODE §2.5, pre-public-mvp §3.2]`

**Fix pattern (already in AGENTS.md):**
```rust
let kp = PrivKeyHandle::generate(...)?;  // outside lock
let idx = {
    let mut guard = self.keypairs.write();
    guard.push(kp);
    guard.len() - 1
};
```

### §1.3.5 Secrets Reachable via `Debug` Derive (Violates "Do not log secrets")

`p2p/core-engine/src/core/bindings.rs` — Bindgen-generated structs derive `Debug` by default. The following hold raw secret material:

| Struct | Line | Bytes | Severity |
|---|---|---|---|
| `ForetiasPrivKey32` | 103 | 32 (Ed25519 seed) | **CRIT** |
| `ForetiasSecretKeyVar` | 215 | up to 4096 (PQC secret) | **CRIT** |
| `ForetiasTbidV1SecretKey` | 306 | 160 (TBID dual-key secret) | **CRIT** |
| `ForetiasNoiseState` | 413 | session keys, chaining key, hash | **HIGH** |
| `ForetiasKemSecretKey` | 260 | up to 2400 (ML-KEM secret) | **MED** |

`[Sources: OPENCODE §3.1, §3.9]` — Not surfaced in CLAUDE review; this is the highest-impact OpenCode-specific find.

**Additionally (NEW — NoiseSession `unsafe impl Send`):** `p2p/core-engine/src/noise.rs:31` — `NoiseSession` carries `unsafe impl Send` with the justification "no interior mutability in C." The C11 `ForetiasNoiseState` contains mutable counters (`send_nonce: u64`, `recv_nonce: u64`) and keys modified by `foretias_noise_send`/`foretias_noise_recv`. Moving the session across a thread boundary without synchronization means concurrent encryption/decryption can corrupt the noise state, potentially causing nonce reuse in ChaCha20-Poly1305 (catastrophic for encryption). This is a **different and arguably more dangerous unsafety** than the `PrivKeyHandle` Sync issue already cataloged.

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]` — Prior review covered `PrivKeyHandle` `Sync` but not `NoiseSession` `Send`.

**Mitigation:** Remove `unsafe impl Send` unless the session is always protected by `Arc<Mutex<NoiseSession>>` before crossing thread boundaries.

`AGENTS.md` says "Do not log secrets" but does not specifically say "FFI binding types holding secret material must not derive `Debug`." The rule should be added (proposed in §1.5).

**Fix pattern:**
- For bindgen-generated structs: customize bindgen with `.no_debug("ForetiasPrivKey32")` etc., or post-process the generated file
- For hand-written structs: never derive `Debug` on a type containing secret bytes; provide a manual `Debug` impl that emits `"<redacted>"` if logging is needed

**SPEC seed:** "Audit all `#[derive(Debug)]` (manually written and bindgen-generated) for types whose fields hold secret material. Use bindgen's `.no_debug(...)` directive for FFI types; replace manual derives with `Debug` impls that emit `<redacted N bytes>`. Add a clippy lint or pre-commit grep for `#[derive(...Debug...)]` on types named `*Secret*`, `*Priv*`, `*Key*`."

### §1.3.6 FFI Length Not Validated Against Buffer (Violates "FFI must validate lengths")

`p2p/core/src/signing_sphincs.c:53`, `signing_dilithium.c:53` — `sig->len` is passed directly to liboqs without validating `sig->len <= FORETIAS_SIG_MAX_SIG_BYTES`. A caller passing `len` larger than the buffer triggers an out-of-bounds read inside liboqs.

`[Sources: OPENCODE §3.7]` — On the Rust side, the corresponding wrapper in `core-engine/src/core/signing.rs` should add the length check before calling the FFI function, even if the C side also validates. Defense in depth.

**SPEC seed:** "Add length validation at all FFI boundaries: every `*_with_len` C function must check `len <= MAX` first; every Rust FFI wrapper must check the same before the FFI call. Audit existing C functions in `p2p/core/src/` for missing length checks."

### §1.3.7 `Clock` Trait Exists but Not Used (Violates "Inject a clock")

`p2p/core-engine/src/foretias/tick.rs:88-91` — `SystemTime::now()` called directly inside the stamp path. The `Clock` trait is defined elsewhere in core-engine but not threaded through `Chronomatter`. Same pattern at `chronomatter/mod.rs:67`, `tbid_handshake.rs:67-70`.

`[Sources: pre-public-mvp §3.2, CLAUDE §24]`

**Fix:** Plumb `Arc<dyn Clock>` through `Chronomatter::new`, `TbidHandshake::new`, and the daemon. Tests inject a `MockClock`; production uses `SystemClock`. This also resolves the panic potential in §1.3.1.

### §1.3.8 Dependency Injection Inconsistency (Violates "Simple ownership")

`chronomatter/mod.rs:46-47` — `Chronomatter::new` constructs its own `SoftwareCryptoServer` internally, ignoring whatever `CryptoServer` the caller might supply. `Chronomatter::from_calendar` at line 73 does accept an injected `Arc<dyn CryptoServer>`. Inconsistent — and blocks the HSM/enclave backend path.

`[Sources: pre-public-mvp §3.2, surfaced in CLAUDE §34.11]`

**Fix:** Plumb `crypto: Arc<dyn CryptoServer>` through `Chronomatter::new`. Remove the internal `SoftwareCryptoServer::generate()` call.

### §1.3.9 Hardcoded Algorithm Instead of Querying CryptoServer

`chronomatter/mod.rs:259, :283, :355` — `SignatureAlgorithm::Ed25519.to_id_string()` written directly into Foretis/ChrononRecord. If `signature_algorithm()` ever returns Dilithium3 for a future backend, the recorded tag will still say Ed25519, silently breaking verification dispatch.

`[Sources: CLAUDE §3.6]`

**Fix:** `let alg = self.crypto.signature_algorithm().to_id_string();` once at top of `stamp()`/`build_tick_record()`, then use the local binding.

### §1.3.10 `#[serde(default)]` Where Spec Forbids It

`tick.rs:27` — `#[serde(default)] pub stamps_per_tick: u64` directly contradicts `FORETIAS_3 §front-matter`: *"no `#[serde(default)]` for new fields, no legacy migration paths."* Violates `AGENTS.md "Reject malformed, ambiguous, non-canonical, or trailing data unless the format explicitly allows it."`

`[Sources: pre-public-mvp §1.3, CLAUDE §34.4]`

**Fix:** Remove `#[serde(default)]`. A missing field becomes a deserialization error (the desired behavior — it distinguishes legitimate-0 from missing-field).

### §1.3.11 Plain `[u8; 32]` for Secret Material (Violates "secrets protected from logs and errors")

`p2p/foretias-server/src/server/mod.rs:74, :83-84` — `noise_static_priv: [u8; 32]` held as plain `[u8;32]`. No `Zeroizing` wrapper, no `Drop` impl on the server struct.

`[Sources: pre-public-mvp §3.3, CLAUDE §34.12]`

`p2p/core-engine/src/crypto_server/signing_tbid.rs:32-43` — TBID secret extracted to `secret_bytes: Vec<u8>` not wrapped in `Zeroizing`. C-side struct is zeroized but the Rust `Vec<u8>` heap allocation persists until allocator reuse.

`[Sources: CLAUDE §14.1, §19]`

**Fix:** Wrap every in-process secret in `Zeroizing<...>` at the type level. Add a clippy lint that flags `[u8; N]` fields named `*priv*` / `*secret*` / `*seed*` that are not `Zeroizing`.

### §1.3.12 Non-Canonical Serialization for Signed Payload

`probity/mod.rs` — `ProbityReport` canonical signing form uses null-byte field separators. A subject containing a null byte (possible from malformed input) produces ambiguous canonical bytes. Violates `AGENTS.md "Parsing must be strict. Reject malformed, ambiguous, non-canonical, or trailing data."`

`[Sources: CLAUDE §3.4]`

**Fix:** Length-prefix each field (4-byte big-endian length + bytes) or use a deterministic serialization format (CBOR with sorted keys, or protobuf).

### §1.3.13 Test Determinism (Violates "Prefer deterministic tests")

- Many Rust tests use `rand::thread_rng()` then `unwrap()` on dependent results — non-deterministic.
- `calendar_crash_recovery_corrupt_tmp` *asserts buggy behavior* (enshrines a known bug as the test's expected outcome).
- Python tests rely on wall-clock `time.time()` for tick numbers; flaky on slow CI.

`[Sources: pre-public-mvp §6.4, CLAUDE §34.19]`

**Fix:** Seed RNGs deterministically (e.g., `rand_chacha::ChaCha20Rng::seed_from_u64(0xC0FFEE)`); inject `MockClock` for tests; convert tests-that-assert-bugs into `#[should_panic]` or `#[ignore = "FIXME"]` with a tracking issue.

## §1.4 Per-File Rust Style Verdicts

For triage when prioritizing a "Rust hygiene sweep" SPEC:

| File | Verdict | Primary AGENTS.md violation |
|---|---|---|
| `p2p/core-engine/src/foretias/types.rs` | Strong | None — exemplar |
| `p2p/core-engine/src/crypto_server/mod.rs` (trait) | Strong | Stubbed methods return `Unsupported` — okay per AGENTS.md if intentional |
| `p2p/core-engine/src/crypto_server/software.rs` | Good with gaps | Algorithm hardcoding (§1.3.9); eager PQC keygen |
| `p2p/core-engine/src/crypto_server/signing_tbid.rs` | **Needs work** | Plain `Vec<u8>` secret (§1.3.11) |
| `p2p/core-engine/src/chronomatter/mod.rs` | **Needs work** | CAS misuse (§1.3.3), lock-over-slow-work (§1.3.4), DI inconsistency (§1.3.8), algorithm hardcoded (§1.3.9), `SystemTime::now()` (§1.3.7) |
| `p2p/core-engine/src/foretias/tick.rs` | Needs work | `#[serde(default)]` (§1.3.10), `SystemTime::now()` (§1.3.7), `sig_input` built twice (DRY) |
| `p2p/core-engine/src/core/bindings.rs` | **Critical** | Debug derives on secret types (§1.3.5) |
| `p2p/core-engine/src/collision/detector.rs` | Adequate | Count-based nonce window (replay weakness, §1.3.2 spirit) |
| `p2p/foretias-server/src/server/handlers.rs` | **Needs work** | Validate-before-trust violations (§1.3.2), `block_on` in async, `unwrap()` patterns |
| `p2p/foretias-server/src/server/mod.rs` | Needs work | Plain `[u8;32]` noise key (§1.3.11), `unwrap()` on path (§1.3.1), hardcoded `/tmp` path |
| `p2p/foretias-server/src/communerd/mod.rs` | Needs work | 11 `OnceLock` fields — fragile init, no observable state |
| `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs` | Needs work | `unwrap()` on remote payload (§1.3.1); uses `rand::thread_rng()` instead of `crypto.random_bytes()` |
| `p2p/foretias-server/src/probity/gossip_handler.rs` | **Critical** | Signature verification stubbed (§1.3.2) |
| `p2p/foretias-server/src/calendar_store/encrypted_jsonl.rs` | Adequate | `read_to_string` over whole file (memory); block-ID fallback to 0 on I/O error |
| `p2p/foretias-client/src/noise_ptp.rs` | Adequate | New Noise handshake per request (no session reuse) |
| `p2p/core-engine/src/epoch/frost_bridge.rs` | Stub | Returns dummy signature — should return `Unsupported` per AGENTS.md "Do not expose test-only shortcuts in production APIs" |
| `p2p/core-engine/src/probity/store.rs` | Adequate | Expected to grow; O(N) scans tolerable for small N |

## §1.5 Recommended Additions to AGENTS.md "How To Write Rust Code"

The following rules are not yet in `AGENTS.md` but are implied by the violations above. Adding them gives reviewers (and future AI agents) a concrete checklist.

### §1.5.A New Subsection: "Secret Material Handling"

Add after the existing "Cryptographic and Security-Sensitive Code" section:

> **Secret Material Handling**
>
> Any in-process byte array, slice, or `Vec` that holds secret material (private key, seed, session key, derived key, HMAC key, etc.) must be wrapped in `zeroize::Zeroizing<T>` at the type level, or be encapsulated behind an opaque handle that owns the zeroizing logic (e.g., `PrivKeyHandle`).
>
> Forbidden:
> ```rust
> let secret: [u8; 32] = generate();
> let secret: Vec<u8> = extract_from_c_struct();
> struct Server { noise_priv: [u8; 32] }
> ```
>
> Required:
> ```rust
> let secret: Zeroizing<[u8; 32]> = Zeroizing::new(generate());
> let secret: Zeroizing<Vec<u8>> = Zeroizing::new(extract_from_c_struct());
> struct Server { noise_priv: Zeroizing<[u8; 32]> }
> ```
>
> **No type whose fields hold secret bytes may derive `Debug`.** Either omit `Debug` (preferred) or write a manual `Debug` impl that emits `<redacted N bytes>`. For bindgen-generated FFI types, use bindgen's `.no_debug("ForetiasPrivKey32")` builder API to suppress the auto-derive.
>
> A type with secret material should not be `Clone`, `Copy`, or `Serialize` unless cloning, copying, or serializing the secret is genuinely required by the protocol. When cloning is required, the clone must produce a separately-zeroizing copy.
>
> Audit checkpoint: grep for `#[derive(...Debug...)]` on types whose name contains `Secret`, `Priv`, `Key`, or `Seed`. Grep for `[u8; N]` and `Vec<u8>` fields named `*priv*`, `*secret*`, `*seed*`, `*key*` and verify they are wrapped in `Zeroizing` or behind an opaque handle.

### §1.5.B New Subsection: "Atomic Counter Idioms"

Add to "Concurrency and Async":

> **Atomic Counter Idioms**
>
> Use `fetch_add`, `fetch_sub`, `fetch_or`, etc. for unconditional read-modify-write operations on `Atomic*`. Use `compare_exchange` / `compare_exchange_weak` only when the operation depends on the previous value's content (e.g., insert-if-equal, transitioning a state machine).
>
> Forbidden pattern (CAS-as-counter):
> ```rust
> let old = counter.load(SeqCst);
> counter.compare_exchange(old, old + 1, SeqCst, SeqCst)
>     .map_err(|e| NodeError::Internal(format!("counter conflict: {}", e)))?;
> ```
>
> Required pattern:
> ```rust
> let new = counter.fetch_add(1, SeqCst) + 1;
> ```
>
> Reason: CAS-as-counter creates a spurious failure mode under contention. `fetch_add` is unconditional and cannot fail.

### §1.5.C New Subsection: "Unverified vs Verified Type Distinction"

Add to "Cryptographic and Security-Sensitive Code":

> **Unverified vs Verified Type Distinction (Mandatory for Inbound Network/IPC Data)**
>
> Any wire message, gossip payload, RPC parameter set, DHT record, or file loaded from disk where the integrity guarantee comes from a downstream check (signature verification, MAC check, hash match, chain-of-trust traversal) must be modeled as two types:
>
> 1. `Unverified<X>` — Constructed directly from wire bytes. Cannot be passed to any consumer that requires authenticated data.
> 2. `Verified<X>` — Constructed *only* by `Unverified<X>::into_verified(crypto, ...)` after the verification check succeeds.
>
> Pattern:
> ```rust
> pub struct UnverifiedChrononRecord(ChrononRecord);
> pub struct VerifiedChrononRecord(ChrononRecord);
>
> impl UnverifiedChrononRecord {
>     pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> { ... }
>     pub fn into_verified(
>         self,
>         crypto: &dyn CryptoServer,
>         prev: &VerifiedChrononRecord,
>     ) -> Result<VerifiedChrononRecord, VerifyError> { ... }
> }
> ```
>
> Storage, replication, dispatch, and metric counters that act on the record's authenticity must accept only `Verified<X>`. The compiler will then refuse to compile a code path that bypasses verification.

### §1.5.D New Subsection: "FFI Length Validation"

Extend "FFI and C11 Core Boundaries":

> **FFI Length Validation**
>
> Every C function exported via the FFI surface that accepts a length-bearing buffer (`*_with_len` or any function with a `len` parameter) must validate `len <= MAX` before any byte access. The corresponding Rust wrapper in `core-engine/src/core/` must perform the same check before calling the FFI function (defense in depth).
>
> Pattern:
> ```c
> int foretias_x_op(const uint8_t* buf, size_t len) {
>     if (len > FORETIAS_X_MAX_LEN) return FORETIAS_ERR_BAD_INPUT;
>     // ...
> }
> ```
>
> ```rust
> pub fn x_op(buf: &[u8]) -> Result<Output, CoreError> {
>     if buf.len() > FORETIAS_X_MAX_LEN {
>         return Err(CoreError::BadInput);
>     }
>     // SAFETY: length validated above; pointer non-null per slice contract.
>     unsafe { foretias_x_op(buf.as_ptr(), buf.len()) }
> }
> ```

### §1.5.E Extend "Time Handling"

> **Clock Injection Is Mandatory in Protocol Logic**
>
> Any function in `core-engine/`, `foretias-client/`, or `foretias-server/` that takes an action based on the current time (stamping, tick advance, freshness check, replay window, timeout, leasing) must receive its time source via `Arc<dyn Clock>` parameter (or as a struct field set at construction). Direct calls to `std::time::SystemTime::now()` or `std::time::Instant::now()` are forbidden in protocol paths.
>
> Reason: without injection, tests cannot run deterministically, and bugs in tick scheduling cannot be reproduced. The `Clock` trait already exists.
>
> Exception: top-level binary `main.rs` may construct the default `SystemClock`. Internal modules must accept `Arc<dyn Clock>`.

### §1.5.F Extend "Serialization and Parsing"

> **Canonical Signing Forms Must Be Unambiguous**
>
> When constructing the canonical bytes over which a signature, MAC, or hash will be computed, the encoding must be parsing-invariant: any byte sequence decodes to at most one logical message. Field separators (null bytes, commas, delimiters) are forbidden — use length-prefixed encoding or a deterministic schema-based encoding (CBOR canonical form, protobuf with sorted fields).
>
> Forbidden:
> ```rust
> buf.extend_from_slice(subject.as_bytes());
> buf.push(0); // null separator
> buf.extend_from_slice(target.as_bytes());
> ```
>
> Required:
> ```rust
> buf.extend_from_slice(&(subject.len() as u32).to_be_bytes());
> buf.extend_from_slice(subject.as_bytes());
> buf.extend_from_slice(&(target.len() as u32).to_be_bytes());
> buf.extend_from_slice(target.as_bytes());
> ```
>
> Reason: null-byte separation breaks if any field can contain a null byte. Even if validated upstream, the canonical encoding should not depend on upstream validation.

### §1.5.G Extend "Testing Requirements"

> **Tests Must Not Assert Buggy Behavior**
>
> A test that documents a known bug by asserting the bug's output is not a test — it is a future regression accelerant. Such tests must be either:
>
> 1. Rewritten to assert the correct behavior, and marked `#[should_panic]` until fixed, OR
> 2. Marked `#[ignore = "FIXME: link to issue"]` and tracked, OR
> 3. Replaced with a `cargo test --features unfixed-bugs`-gated test that runs in a known-failing CI lane only.
>
> A test named `*_corrupt_*_not_removed_by_current_implementation` is a signal of this anti-pattern.
>
> **Tests Must Be Deterministic**
>
> All tests must seed any RNG explicitly. Use `rand_chacha::ChaCha20Rng::seed_from_u64(constant)` or pass an `Arc<dyn Rng>` mock. Tests that call `rand::thread_rng()` are non-deterministic and forbidden in CI.
>
> Tests sensitive to time must use `MockClock`, not `SystemTime::now()` / `time.time()`.

### §1.5.H Extend "Logging and Observability"

> **Debug Derive on Secret-Holding Types Is Forbidden**
>
> Already covered by §1.5.A but worth restating in the logging context: a stray `format!("{:?}", obj)` or `tracing::debug!(?obj, ...)` on a `Debug`-deriving secret type emits the full secret. The defense is type-level: never derive `Debug` on a type holding secrets.

### §1.5.I Code Review Checklist Additions

Add to the existing "Code Review Checklist for AI Agents":

* Is every in-process secret wrapped in `Zeroizing` or behind an opaque handle?
* Is every inbound network/IPC message wrapped in an `Unverified<X>` newtype before any processing?
* Are atomic counters using `fetch_add` (not CAS)?
* Are all `unwrap()` / `expect()` justified by a documented invariant, or replaced with `?` propagation?
* Are FFI lengths validated on both sides of the boundary?
* Are time sources injected (not direct `SystemTime::now()`)?
* Are canonical signing forms length-prefixed (not separator-delimited)?
* Do tests seed RNGs deterministically?
* Do tests use `MockClock` for time-sensitive assertions?
* Does no `Debug` derive expose secret bytes?

---

## §1.6 Suggested SPECs Generated from Part I

Each of the following can become a `<NAME>_SPEC.md` + `<NAME>_PLAN.md` pair in `specs/`:

| SPEC Name | What it specifies |
|---|---|
| `RUST_HYGIENE_SPEC.md` | Sweep all `unwrap()`/`expect()` (§1.3.1); verify FFI length checks (§1.3.6); remove `#[serde(default)]` violations (§1.3.10) |
| `SECRET_TYPE_DISCIPLINE_SPEC.md` | Wrap all secrets in `Zeroizing` (§1.3.11); remove `Debug` from binding structs (§1.3.5); add bindgen `.no_debug` directives |
| `UNVERIFIED_VERIFIED_TYPES_SPEC.md` | Introduce newtype distinction; refactor handlers (§1.3.2); model used everywhere inbound data is touched |
| `CONCURRENT_PRIMITIVES_SPEC.md` | Replace CAS-as-counter with `fetch_add` (§1.3.3); audit locks across `.await` (§1.3.4); document atomic ordering choices |
| `CLOCK_INJECTION_SPEC.md` | Plumb `Arc<dyn Clock>` through `Chronomatter`, `TbidHandshake`, daemon, collision detector (§1.3.7); add `MockClock` for tests |
| `CRYPTO_SERVER_INJECTION_SPEC.md` | Fix `Chronomatter::new` DI inconsistency (§1.3.8); algorithm-query everywhere (§1.3.9); enable HSM/enclave backend path |
| `CANONICAL_ENCODING_SPEC.md` | Length-prefix all signing payloads (§1.3.12); audit `ProbityReport`, `TbidProofResponse`, auto-attestation blob |
| `AGENTS_MD_RUST_UPDATE_SPEC.md` | Apply additions §1.5.A through §1.5.I to `AGENTS.md` |

---

# Part II — Cybersecurity, Threat Modeling, Formalism, Testing Methodologies

This part is the user's second priority. It consolidates threat analysis, critical security findings, cryptographic protocol concerns, secret lifecycle, formal-methods status, and testing methodology gaps.

## §2.0 Why This Part Comes Second

Foretias is a **timestamping authority** — a system whose entire reason to exist is to make a single security claim: *"a Foretis from chronon N could only have been created during chronon N."* If that claim is breakable, the system has no value. Every other concern in this document (mindshare, ergonomics, performance, even Rust style) is downstream of this property holding.

The good news from both prior reviews: **the core mechanism is sound**. The ephemeral-key-per-chronon design, the forward/backward auto-attestation chain, and the offline verification path are cryptographically defensible.

The work in this part is about the **gaps between the strong core mechanism and the deployed system**: signature checks that are stubbed, secrets that are loggable, P2P trust that is unauthenticated, replay windows that are too short, formal methods that exist as comments without proofs, and CI that doesn't gate any of these on PRs.

## §2.1 Threat Model

The repository has `docs/threat_model_v0_5.md` (referenced in both reviews). This subsection summarizes that document, adds the asset inventory consolidated from both reviews, and identifies adversary classes for which the system currently has weak or missing defenses.

### §2.1.1 Asset Inventory

What an attacker would want to compromise:

| Asset | Where it lives | Value to attacker |
|---|---|---|
| Tick private keys (live chronon) | Process memory, `Chronomatter::keypairs` | Forge stamps for current chronon |
| Tick private keys (past chronons) | In principle: zeroized at tick advance. In practice: unbounded `Vec<TickKeyPair>` retains them | Forge backdated stamps |
| TBID secret (dual-key) | C11 struct + Rust `Vec<u8>` (unzeroized — §1.3.11) | Forge new TBID claims, impersonate this node |
| Noise static private key | `server/mod.rs:83-84` plain `[u8;32]` | Decrypt past Noise sessions |
| PQC secrets (SPHINCS+, Dilithium3, ML-KEM, SLH-DSA-256f) | `SoftwareCryptoServer` `Zeroizing` (safe) | Forge PQC-signed Foretises |
| Calendar (local) | Disk JSON | Modify to claim different chronon ordering; rewrite history if no auto-attestation chain check |
| Calendar (mirrors of other TBIDs) | `MirrorStore` on disk | Serve forged calendars to verifiers (§3.3) |
| DHT routing records | Distributed across peers | Redirect verifiers to attacker servers (§2.3.2) |
| Probity scores | `ProbityStore` in process memory | Influence committee selection, bias verification trust |
| FROST shares (future) | `software.rs:36 frost_shares: HashMap<_, Zeroizing<Vec<u8>>>` (safe today) | Forge epoch snapshots |

### §2.1.2 Adversary Model

| Class | Capability | Foretias defense status |
|---|---|---|
| **Network observer** | Sees all JSON-RPC and libp2p traffic | **Partial** — libp2p uses Noise; JSON-RPC has no TLS; content is hex-encoded plaintext over the wire |
| **Active MITM** | Modifies network traffic | **Partial** — libp2p-noise resists MITM; custom JSON-RPC transport has no integrity protection at protocol layer |
| **Compromised single peer** | Holds peer's keys, can sign anything peer would sign | **Strong** — peer can only forge its own future stamps; past stamps verifiable independently |
| **DHT poisoner** | Registers fake TBID→peer_id mappings | **Broken** — DHT registration is unauthenticated (§2.3.2) |
| **Gossip-flood attacker** | Floods probity reports | **Broken** — gossip signature not verified (§2.3.4) |
| **Replay attacker** | Captures and replays heartbeats / nonces | **Weak** — count-based nonce window, no time freshness check (§2.4.4) |
| **Majority compromise (>50%)** | Controls >N/2 of network nodes | **Past stamps: strong; live/future: weak (FROST stubbed §2.3.5)** |
| **Side-channel attacker** | Timing, power, EM, cache, Spectre | **Unaddressed** — libsodium handles constant-time for primitives; no audit of Rust glue |
| **Memory-extraction attacker** | Cold boot, hypervisor escape, debugger | **Partial** — Zeroizing helps post-use; live tick key is extractable while in use |
| **Nation-state with quantum** | Future RSA/ECC breakage | **Algorithm-agile foundations** — PQC integrated but Ed25519 sigs in TBID handshake still classical (§2.4.5) |
| **Compromised dev environment** | Malicious commits, dependency confusion | **Weak** — no `cargo audit` in CI, `serde_cbor` unmaintained, no SBOM (§14) |

### §2.1.3 Trust Boundaries

Drawn explicitly so SPEC authors know where to harden:

1. **Process boundary** — Anything entering the foretias-server process from outside (JSON-RPC, libp2p, DHT, gossip, file load) is **untrusted** until validated.
2. **C11/Rust FFI boundary** — Foreign data treated as untrusted on both sides; lengths checked at both crossings (§1.5.D).
3. **DHT boundary** — Every record retrieved from DHT is untrusted; today not signed (§2.3.2).
4. **Gossip boundary** — Every message arriving via GossipSub is untrusted; today not signature-verified (§2.3.4).
5. **Calendar mirror boundary** — Every mirrored record is untrusted; today not integrity-checked at ingestion (§2.3.6).
6. **Cross-node verify boundary** — Calendars fetched for verification are untrusted; today used directly (§2.3.3).
7. **Disk boundary** — Local persisted calendar is *less untrusted* (assumes local file integrity) but Calendar load from `.tmp` is not verified (§2.4.6).
8. **Time source boundary** — `SystemTime::now()` is untrustworthy (NTP slew, leap seconds, clock skew). Mitigated by injectable `Clock` trait *when used* (§1.3.7).

## §2.2 The "50% Compromise" Nightmare Test — Direct Answer

Both prior reviews answered this question; the consolidated answer is below.

> *"If I have a nightmare about more than 50% of servers being hacked, will my mind have a reassuring answer?"*

**The short answer: yes for past stamps; partial for live stamps; gap for future stamps until FROST ships.**

### §2.2.1 What is Ironclad

A Foretis is a signature over `(TBID ‖ chronon_number ‖ content)` using the private key of chronon N. That key was algorithmically incapacitated when chronon N+1 was created — the forward and backward auto-attestations bear cryptographic witness to the transition. After tick advance:

- The private key for chronon N no longer exists in any form (Zeroizing on drop; nothing on disk).
- A fresh attacker with access to the *current* compromised server cannot produce a Foretis for chronon N — they would need a key that no longer exists.
- An attempt to produce a Foretis for chronon N with a *different* key produces a signature that fails verification against the published public key. Verifiable offline, no server required.

**This is the architecture's central guarantee.** A calendar received yesterday remains fully self-consistent without any server alive.

### §2.2.2 What Fails During Live Compromise

A compromised server is in an active tick. The attacker *does* hold the current tick's private key. They can issue any Foretis they want with that key until the next tick advance:

- **Damage window** = chronon duration. For the 60-second default, at most 60 seconds of fake stamps per compromised server per tick.
- **Detection** depends on calendar mirroring: if even one honest peer holds the calendar up to chronon N-1, the attacker's divergent chronon N is detectable.
- **Probity is biased** because the gossip signature check is stubbed (§2.3.4) — attackers can flood positive reports for themselves.

### §2.2.3 What is Missing for Future Compromise (the FROST gap)

FROST t-of-n threshold signing on epoch snapshots is specified (`specs/FORETIAS_2_P2P_8_epoch_consensus.md`) and architected (`p2p/core-engine/src/epoch/`) but **triple-stubbed**:

1. `software.rs:284` — `frost_sign_partial` returns `Err(CryptoError::Unsupported("FROST signing not supported in software backend"))`.
2. `frost_bridge.rs:28-46` — `run_frost_round` returns a snapshot with `frost_signature: vec![0x00; 64]` (all-zeros).
3. `handlers.rs:354-376` — `handle_verify_epoch_snapshot` always returns `valid: true`.

When implemented with k > N/2, FROST means an attacker controlling N/2 cannot produce a valid epoch snapshot. Until then, anyone can forge a snapshot and any other node accepts it.

### §2.2.4 The Reassuring Answer to Bring to a Nightmare

> *"My Foretis from last week cannot be forged. A compromised server could issue fake stamps for at most 60 seconds per tick, and those fake stamps are detectable by any honest peer with a mirrored calendar. The cryptographic chain is append-only and chained — inserting a backdated entry requires rewriting all subsequent ticks, which requires all future keys."*

### §2.2.5 Gap-Closing Roadmap (cross-references)

- Implement FROST end-to-end (§14 P0 item)
- Sign DHT registration records to prevent peer impersonation (§2.3.2)
- Sign and verify probity reports (§2.3.4)
- Calendar mirror ingestion runs `integrity_check` before commit (§2.3.6)
- Add "last honest tick" attestation — a signed statement from k-of-n peers that calendar X is valid through tick Y

## §2.3 Critical Security Findings — Consolidated CRIT/HIGH

Findings ordered by severity, then by impact-if-exploited.

### §2.3.1 Debug Derives on Secret Binding Types — CRIT

`bindings.rs:103, :215, :260, :306, :413` derive `Debug` on structs holding raw Ed25519 seeds, PQC secrets, TBID dual-key secrets, KEM secrets, and Noise session state. A stray `format!("{:?}", ...)` or `tracing::debug!(?state, ...)` emits the full secret.

`[Sources: OPENCODE §3.1]` — Highest-impact OpenCode-unique finding. Not in CLAUDE review. **Sometimes a single PR fix.**

**Mitigation:** §1.5.A above. SPEC: `SECRET_TYPE_DISCIPLINE_SPEC.md`.

### §2.3.2 DHT Poisoning via Unauthenticated Peer Registration — CRIT

`communerd/mod.rs:41-51, :586-605` — `PeerRegistrationRecord{peer_id, tbid, multiaddr, json_rpc}` is stored as plain JSON in DHT without signature.

**Attack:** Attacker `A` registers victim `V`'s TBID pointing to `A`'s json_rpc. All cross-node verification requests for V's TBID are redirected to A, who serves forged calendar records.

`[Sources: CLAUDE §4.3, OPENCODE §3.2]`

**Mitigation:** Sign `PeerRegistrationRecord` with the claiming node's TBID signing key. Verifiers check signature before trusting DHT entries. Reject registrations whose `peer_id` doesn't match the signing key (libp2p `peer_id` is derivable from the signing public key).

### §2.3.3 Cross-Node Verify Trusts Remote Calendar — HIGH (CRIT in combination with §2.3.2)

`handlers.rs:191-237` — `cross_node_verify` looks up TBID via DHT (vulnerable per §2.3.2), fetches calendar slice, uses remote record's public key directly. No Merkle proof or chain-of-trust verification of the returned record back to a known root.

`[Sources: OPENCODE §3.3]`

**Combined with §2.3.2:** Full verification bypass for cross-node verify.

**Mitigation:** Require the cross-node response to include an integrity chain back to a trust root (e.g., the most recent FROST-signed epoch snapshot; until FROST ships, the genesis ChrononRecord). Run `integrity_check()` on the returned slice before trusting any public key in it.

### §2.3.4 Probity Signature Verification Is Stubbed — CRIT (Trust Model Failure)

`probity/gossip_handler.rs:36-51` — Only checks `signature.len() >= 64`. Does NOT verify the Ed25519 signature against the reporter's public key.

**Consequence:** Any peer can forge probity reports about any other peer. The probity-based trust model is non-functional. All downstream uses (committee selection, anti-Sybil) are subverted.

`[Sources: OPENCODE §3.4]` — Possibly the most damaging single finding; the system *appears* to have a reputation layer but the layer's trust is broken.

**Mitigation:** Add Ed25519 signature verification in `gossip_handler.rs:handle_gossip_message`. Reject malformed reports rather than ingesting them. Apply the §1.5.C `Unverified<ProbityReport>` / `Verified<ProbityReport>` discipline.

### §2.3.5 FROST Triple-Stubbed and `verify_epoch_snapshot` Always Returns `valid: true` — HIGH

See §2.2.3. The most dangerous aspect: `handle_verify_epoch_snapshot` returning `valid: true` is *worse than returning `Unsupported`* because callers cannot tell the difference between "epoch is valid" and "verification is not implemented."

`[Sources: CLAUDE §15.1, OPENCODE §3.5]`

**Additionally (NEW — sibling stub `handle_get_latest_epoch`):** `handlers.rs:225-242` — `handle_get_latest_epoch` is a second stubbed epoch handler that returns `epoch_number: 0`, empty `peer_scores`, empty `committee`, `threshold: 0`, empty `frost_signature`, and empty `committee_pubkey`. This **sibling handler** misrepresents the current epoch state as "epoch zero with no peers" rather than "not implemented." A client consuming this response cannot distinguish between "network has zero peers" and "handler is not yet implemented."

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]` — Prior review flagged `handle_verify_epoch_snapshot` but not its sibling `handle_get_latest_epoch`.

**Mitigation:** Return a JSON-RPC error with `"FROST epoch data not yet implemented"` rather than all-zero stub data. This forces callers to handle the unimplemented state explicitly.

**Immediate mitigation (cheap):** Change `handle_verify_epoch_snapshot` to return `valid: false` with reason `"FROST epoch verification not yet implemented"`. This is honesty about the current state and prevents any code from acting on a false positive.

**Long mitigation:** Implement FROST `sign`/`verify` in `software.rs` (or feature-gate); implement `run_frost_round` in `frost_bridge.rs` (real protocol); wire `verify_epoch_snapshot` to call real FROST verify.

### §2.3.6 Calendar Mirror Records Accepted Without Integrity Check — HIGH

`handlers.rs:465-482` (`handle_ship_ack`) — `mirror_store.insert_mirrored()` called without running `integrity_check` on inbound records. A malicious peer ships structurally valid but cryptographically forged records.

`[Sources: CLAUDE §4.4, OPENCODE §4.5]`

**Mitigation:** Before insertion, run `verify_pair()` on each consecutive pair in the batch; verify forward/backward auto-attestation chain; reject batch on any failure.

### §2.3.7 Genesis Verification Bypass via Short Foretis — HIGH

`tick.rs:308-317` — Genesis tick path splits `forward_foretis` into Ed25519 (first 64 bytes) and genesis signature (remaining). If `forward.len() < 64`, malformed signature is treated as Ed25519 component; `forward_genesis` becomes empty; for `tb_version == 0`, empty genesis is accepted as valid.

`[Sources: OPENCODE §3.6]` — Crafted calendar with short `forward_foretis` bypasses genesis verification.

**Mitigation:** Require `forward_foretis.len() >= EXPECTED_GENESIS_LEN` before any split; reject otherwise with explicit error.

### §2.3.8 PQC Signature Length Not Validated Against Buffer — HIGH

`signing_sphincs.c:53`, `signing_dilithium.c:53` — `sig->len` passed directly to liboqs without checking `sig->len <= FORETIAS_SIG_MAX_SIG_BYTES`. Out-of-bounds read.

`[Sources: OPENCODE §3.7]`

**Mitigation:** §1.5.D. Add length check at C side; mirror check at Rust wrapper.

### §2.3.9 C11 Nonce Overflow in Noise XX — CRIT

`noise_xx.c:69, :86` — Nonce counter `(*n)++` has no overflow guard. After 2^64 messages on a single session, nonce wraps; reuse with same key catastrophically breaks ChaCha20-Poly1305.

`[Sources: pre-public-mvp §3.1, CLAUDE §32, OPENCODE §3.8]`

**Mitigation:** `if (*n == UINT64_MAX) return FORETIAS_ERR_NONCE_EXHAUSTED;` before increment. Add session-end re-key in the wrapper for very-long-lived sessions.

### §2.3.10 C11 HKDF Stack Overflow in privkey.c — HIGH

`privkey.c:236-240` — `expand_input[64]` buffer filled with `memcpy(expand_input, info, info_len)` then `expand_input[info_len] = 0x01` with no length check on `info_len`. Caller-supplied `info_len > 63` overflows.

`[Sources: pre-public-mvp §3.1, CLAUDE §32, OPENCODE §3.8]`

**Mitigation:** `if (info_len > 63) return FORETIAS_ERR_BAD_INPUT;` at function top.

### §2.3.11 C11 Global Mutable State in privkey.c — HIGH

`privkey.c:74-78, :17-22` — `instance_kek`, `instance_kek_initialized`, `key_gen_counter` are global mutable state. `FORETIAS_1_MVP_SPEC §4.1` forbids this.

`[Sources: pre-public-mvp §3.1, OPENCODE §3.8]`

**Mitigation:** Move to caller-owned context struct, or carve explicit spec exemption with rationale.

### §2.3.12 C11 Dynamic Allocation in Core (calloc/free) — HIGH

`privkey.c:83, :125` — `calloc`/`free` from `<stdlib.h>` violates "no dynamic allocation in core" (§4.1).

`[Sources: pre-public-mvp §3.1]`

**Mitigation:** Use caller-provided buffers (preferred for no-alloc embedded path) or a fixed-size buffer pool.

### §2.3.13 No Authentication on JSON-RPC Endpoints — HIGH

`server/mod.rs` — Any client on the listen port can call `stamp`, `verify`, `get_calendar_slice`, `route_stamp`. Stamps are authoritative proofs; unauthenticated `stamp` access means anyone can issue Foretises under this server's TBID.

`[Sources: CLAUDE §3.1, OPENCODE §8.5]`

**Mitigation:** Token-based auth (shared secret in header, or mTLS) as a configuration option. Default to required auth on non-localhost bind addresses; warn loudly when localhost-only.

### §2.3.14 JSON-RPC Has No Transport Encryption — HIGH

`server/handlers.rs` — All content stamped/queried over the wire is plaintext (after hex-decode). The threat model acknowledges this. Default user experience ("stamp my confidential document hash") sends content unencrypted.

`[Sources: CLAUDE §3.2]`

**Mitigation:** `foretias-client/src/noise_ptp.rs` already implements Noise PtP for client connections. Make Noise the default for client→server, not an alternative. Plain JSON-RPC tolerated only for localhost or with explicit `--insecure-plaintext` flag.

### §2.3.15 libp2p RPC Codec — Unbounded Allocation on Malicious Length — CRIT

`communerd/p2p/rpc_protocol.rs:70-80` — The `ForetiasRpcCodec` implements `libp2p::request_response::Codec`. The `read_length_prefixed` function reads a 4-byte big-endian length prefix and allocates `vec![0u8; len]` with no upper bound. A malicious peer on the P2P network sends `0xFFFFFFFF`, triggering a ~4 GB allocation.

**Critical distinction from existing review:** The prior review flagged `server/mod.rs:383` (server-side TCP JSON-RPC `read_length_prefixed`) which carries a 4 KB bound. The `rpc_protocol.rs` codec is a **separate code path** — it is the framing layer for libp2p request_response over yamux/mplex multiplexed streams. The server-side bound does not protect this path.

**Impact:** Denial of service via memory exhaustion. Any peer on the P2P network can crash a target node with a single malformed RPC request.

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]` — Not surfaced in CLAUDE, OPENCODE, or pre-public-mvp reviews because the prior reviews focused on `foretias-server` TCP paths, not the `communerd` P2P codec.

**Mitigation:** Add `if len > 4 * 1024 * 1024 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "RPC message too large")); }` before `vec![0u8; len]`. Apply the same bound to both `read_request` and `read_response`.

### §2.3.16 Noise PtP Client — Unbounded Response Allocation — CRIT

`foretias-client/src/noise_ptp.rs:72-76` — `noise_json_rpc` reads `resp_len` from the wire (`u32::from_le_bytes(len_buf) as usize`), then allocates `vec![0u8; resp_len]` without any size limit. The server-side `read_len` in `core-engine/src/noise.rs` carries the `NOISE_MAX_MSG` (65535) bound check, but the **client-side path does not**.

**Attack:** Attacker-controlled server sends a length prefix of `0xFFFFFFFF`. The client allocates ~4 GB of memory per request, causing OOM.

**Critical distinction from existing review:** §5.3 notes that "Noise_XX PtP — Functional, with Trade-offs" and flags the "fresh handshake per request" cost. But the **asymmetric length-bound coverage** — server side protected, client side not — was not detected. The prior review examined the server path's bound but did not cross-reference the client path.

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]`

**Mitigation:** Add `if resp_len > foretias_core::noise::NOISE_MAX_MSG as usize { return Err(PtPError::Decode("response exceeds maximum size".into())); }` before the `resp_buf` allocation.

## §2.4 Cryptographic Protocol Correctness Concerns

Findings about the protocol itself (not implementation bugs but design decisions or implementation choices that affect protocol correctness).

### §2.4.1 Python ↔ Rust Auto-Attestation Blob Incompatibility — CRIT (if Python is still canonical)

The Rust auto-attestation blob is `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk ‖ stamps_per_tick (u64 BE) ‖ nonce (16B)`. Python's is `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk` (no `stamps_per_tick`, no `aa_nonce`). Calendars cannot cross-verify.

The Rust shape is strictly stronger (replay-resistant via nonce).

`[Sources: pre-public-mvp §2.2, CLAUDE §34.3, OPENCODE §12.3]`

**Mitigation:** Adopt Rust shape canonically. Update Python `_timebeing.py` to include both fields. Add bidirectional cross-language equivalence test. **OR:** Since Python has been backburnered (`SCOPE_REDUCTION_SPEC.md`), explicitly drop Python as a canonical implementation and update `FORETIAS_0_OVERVIEW.md §0.7` to declare Rust canonical.

### §2.4.2 sign() / signature_algorithm() Mismatch — Verify Still Fixed

The prior P0 from `pre-public-mvp §2.1` was that `SoftwareCryptoServer::sign()` produced Ed25519 bytes while `signature_algorithm()` returned `SPHINCS+`. Current code returns `Ed25519` from `signature_algorithm()`, suggesting fix.

**However:** `chronomatter/mod.rs:259, :283, :355` still hardcodes `SignatureAlgorithm::Ed25519.to_id_string()`. If a future backend (HSM, enclave) returns `Dilithium3` from `signature_algorithm()`, the chronomatter writes Ed25519 into the tag anyway.

`[Sources: pre-public-mvp §2.1, CLAUDE §3.6]`

**Mitigation:** Query algorithm from `self.crypto.signature_algorithm().to_id_string()` once at top of `stamp()`/`build_tick_record()`. Adds regression test:

```rust
let server = make_crypto_with_alg(Dilithium3);
let chronomatter = Chronomatter::new(server.clone(), ...);
let foretis = chronomatter.stamp(content).await?;
assert_eq!(foretis.signature_algorithm, "Dilithium3");
assert!(server.verify_with(&pk, "Dilithium3", &sig_input, &foretis.signature)?);
```

### §2.4.3 Echo Field Not Included in Signature Input

`chronomatter/mod.rs:332-334` — `sig_input` contains `tbid ‖ tick ‖ content`. The `echo` field returned in the Foretis is not signed. MITM servers can modify `echo` without invalidating cryptographic proof.

`[Sources: CLAUDE §2.6]`

**Resolution decision needed:**
- (a) Document `echo` as advisory/unauthenticated (clients must not treat it as protected), OR
- (b) Include `echo` in `sig_input` (wire-breaking change requiring version negotiation).

### §2.4.4 Heartbeat Replay via Count-Based Nonce Window + No Timestamp Freshness — HIGH

`collision/detector.rs:34-39` — Nonce window is `VecDeque` bounded by count, not time. Under heartbeat flood, window evicts quickly, allowing replay of recently-evicted nonces.

`collision/detector.rs:42-64` — `on_heartbeat` accepts heartbeats with arbitrary `timestamp_ns`; no freshness window. Attacker captures yesterday's heartbeat, replays with valid signature → triggers spurious collision → victim node goes dormant. **Dormancy is unrecoverable without restart** (§16.2).

`[Sources: pre-public-mvp §1.7, CLAUDE §3.3, §16.1]`

**Mitigation:** (a) Bound nonce window by time (keep nonces less than `heartbeat_max_age_ns` old). (b) Reject heartbeats whose `timestamp_ns` is more than `heartbeat_max_age_ns` away from local clock. (c) Add operator-override path to revive a dormant node.

### §2.4.5 TBID Handshake Is Not Quantum-Resistant Even Though TBID Is Dual-Key

`tbid_handshake.rs:56-80` — Proof uses `self.crypto.sign()` (Ed25519 only). The 49,856-byte SLH-DSA component of the TBID is used only in the genesis ChrononRecord, not in live handshakes.

`[Sources: CLAUDE §14.1]`

**Trade-off:** Adding SLH-DSA to every handshake costs ~50 KB per handshake — significant for connection establishment. The right design is probably to keep Ed25519 for handshakes and SLH-DSA only for permanent identity proofs (genesis, key-rollover). Document this trade-off.

### §2.4.6 ProbityReport Null-Byte Separators

`probity/mod.rs` — Canonical signing bytes use null-byte field separators. A subject containing a null byte produces ambiguous canonical encoding.

`[Sources: CLAUDE §3.4]`

**Mitigation:** §1.5.F. Length-prefix every field.

### §2.4.7 Calendar Load from .tmp Without integrity_check (Rust path)

`core-engine/src/foretias/calendar.rs:107-141` — `Calendar::load` recovers from `.tmp` if it has more ticks than the main file. Attacker with write access drops fabricated `.tmp` with more (forged) ticks; loader prefers it. Python load path runs `integrity_check` after load; Rust path does not.

`[Sources: pre-public-mvp §4]`

**Mitigation:** Call `integrity_check()` after `.tmp` recovery; refuse to use `.tmp` if any pair fails.

### §2.4.8 Tick Numbers Are Sequential Integers, Not Nanoseconds

Spec `foretias-v1.md §2.2` says chronon_number = nanoseconds since Unix epoch. Implementation increments from 1. Two servers' tick 100 are unrelated points in time. Any use case relying on chronon_number as absolute time anchor is broken.

`[Sources: CLAUDE §2.2]`

**Resolution decision needed** (cross-references `questions.md` item — already resolved there but not propagated).

### §2.4.9 Non-Serialized Mode Not Implemented

Spec defines two modes (`serialized=False` = daemon advances tick, multiple stamps per tick; `serialized=True` = each stamp advances tick). Rust only implements serialized mode. High-throughput batching impossible.

`[Sources: CLAUDE §21]`

**Resolution decision needed.**

## §2.5 Secret Material Lifecycle (Zeroization Inventory)

Combined and corrected table from both reviews:

| Secret | Location | Zeroized? | Method | Action Needed |
|---|---|---|---|---|
| Ed25519 tick private key | `chronomatter.rs: keypairs[i].priv_key` | Yes | `PrivKeyHandle::drop` → C11 memzero | Bound the `Vec` so old keys evict + zeroize promptly (§1.3.3, CLAUDE §2.3) |
| SoftwareCryptoServer seal key | `software.rs:34 Zeroizing<[u8;32]>` | Yes | `Zeroizing` | None |
| SPHINCS+ secret | `software.rs:38 Option<Zeroizing<SignatureBytes>>` | Yes | `Zeroizing` | None |
| Dilithium3 secret | `software.rs:40` | Yes (fixed from prior bug) | `Zeroizing` | None |
| SLH-DSA-256f secret | `software.rs:43` | Yes | `Zeroizing` | None |
| ML-KEM secret | `software.rs:46` | Yes | `Zeroizing` | None |
| FROST shares | `software.rs:36 HashMap<String, Zeroizing<Vec<u8>>>` | Yes | `Zeroizing` | None |
| **TBID secret (Rust Vec)** | `signing_tbid.rs:32-44 Vec<u8>` | **NO** | none | **Wrap in `Zeroizing` (§1.3.11)** |
| **Noise static private key** | `server/mod.rs:83-84 [u8;32]` | **NO** | none | **Wrap in `Zeroizing` + impl Drop (§1.3.11)** |
| **ForetiasPrivKey32 (FFI binding)** | `bindings.rs:103 Copy + Debug` | **NO** | none, also logs via Debug | **Remove Debug + add Drop+zeroize (§1.3.5, §1.5.A)** |
| **ForetiasSecretKeyVar (FFI binding)** | `bindings.rs:215 Debug` | **partial** (C side memzero) | C only | **Remove Debug (§1.5.A)** |
| **ForetiasTbidV1SecretKey (FFI binding)** | `bindings.rs:306 Debug` | **partial** (C side memzero) | C only | **Remove Debug (§1.5.A)** |
| **ForetiasKemSecretKey (FFI binding)** | `bindings.rs:260 Debug` | **partial** (C side memzero) | C only | **Remove Debug (§1.5.A)** |
| **ForetiasNoiseState (FFI binding)** | `bindings.rs:413 Debug` | unknown | unknown | **Remove Debug; audit Drop (§1.5.A)** |
| Noise session keys (Rust side) | libp2p swarm memory | depends on libp2p | libp2p manages | Verify libp2p version uses zeroize |
| TbidProofRequest nonce | `tbid_handshake.rs:52 [u8;32]` | n/a (not secret) | none needed | None |
| TbidProofResponse signature | `tbid_handshake.rs:31 [u8;64]` | n/a (signature) | none needed | None |
| Heartbeat private signing | derives from tick key, not separately stored | via tick key | via tick key | None |

**Aggregate verdict:** Three plain-Rust-`Vec`/`[u8;N]` sites and at least four bindgen-generated `Debug` derives need fixing. All are CRIT or HIGH.

## §2.6 Formal Methods, Verification, and Proofs

### §2.6.1 Frama-C / ACSL Annotations Without Proofs — Misleading

`p2p/core/src/merkle.c:5-9, :18-22, :49-61` carries ACSL annotations (Frama-C verification language) but the `proofs/` directory is empty. The annotations *imply* the code is formally verified to readers/auditors when it is not.

`[Sources: pre-public-mvp §3.1, CLAUDE §34.10, OPENCODE §3.8]`

**Resolution options:**
1. **Drive proofs in CI** — set up Frama-C `wp` plugin to attempt the proofs on every PR; fail CI on regression. This is real work but creates a genuine verified-by-tool surface.
2. **Remove annotations** — if no proof effort is planned, the annotations are misleading. Delete them or relocate to a `notes/` comment so readers understand the status.

A middle path: keep annotations on functions where proofs are intended for v0.5+ hardening; explicitly mark "proof aspiration, not verified" in the source comments.

### §2.6.2 Where Formal Methods Could Be Profitably Applied

Targeted candidates (sorted by impact-per-effort):

1. **`foretias_merkle_verify`** — small, pure function, no side effects, well-defined invariant. Excellent first target for Frama-C/wp.
2. **`foretias_privkey_derive_seal_key`** — KDF chain; provable functional correctness against HKDF spec.
3. **`foretias_noise_step` state machine** — finite state, well-specified transitions. A model-checking approach (TLA+, mCRL2) could verify the handshake.
4. **`foretias_*_verify` (signature verifiers)** — input/output contracts are simple; could prove length validation and constant-time return paths.
5. **Auto-attestation chain verification (Rust side)** — Property: `integrity_check(calendar) == true` ⇔ every consecutive pair's forward and backward foretis is independently valid AND public keys chain correctly. Provable via `proptest` (statistical) or `kani` (bounded model checker for Rust).

### §2.6.3 Kani (Bounded Model Checking for Rust)

Kani can prove Rust safety properties for bounded inputs. Suitable targets:

- `Calendar::append` — invariant: after N appends with strictly-ascending tick numbers, `latest()` returns the last tick number.
- `Chronomatter::stamp` (with mocked crypto) — invariant: every stamp produces a Foretis whose `signature_algorithm` matches the underlying crypto server's reported algorithm (this would catch the prior CRIT bug).
- `Tbid::from_raw` validation — proves no invalid TBID byte sequence can be constructed.

### §2.6.4 Property-Based Testing (proptest)

Already covered as a missing test category. Properties to start with:

1. `stamp(content); verify(content) == true` ∀ content ∈ Bytes (bounded length).
2. `verify(content', stamp(content)) == false` when `content' != content`.
3. For any tampered single byte of `Foretis.signature`, `verify == false`.
4. `calendar.append(t); calendar.latest() == t.tick_number`.
5. Replay: same nonce twice → second is rejected.
6. Tick monotonicity: `c1.append(t1); c1.append(t2); t2.tick_number > t1.tick_number` is always true on success.

### §2.6.5 Differential Testing

Useful because Foretias has multiple implementations (Rust core-engine, soon Python `foretias_p2p` PyO3 binding, future Java JNI). Property: any Foretis produced by implementation A verifies in implementation B and vice versa. This is the cross-language equivalence test category.

### §2.6.6 Fuzzing Status — None

Neither `cargo fuzz` nor libfuzzer harnesses exist in the tree. Highest-value fuzz targets:

| Target | Why |
|---|---|
| `foretias_merkle_verify` (C) | Hash-tree verification on attacker-controlled input |
| `foretias_noise_step` (C) | Handshake state machine on attacker bytes |
| `foretias_*_verify` for each signature alg (C) | Signature verification — attacker controls bytes and signature |
| `foretias_privkey_derive_seal_key` (C) | KDF with attacker-controlled info |
| JSON-RPC envelope decode (Rust) | Malformed envelope, oversized content, deeply nested JSON (serde stack overflow), invalid hex, batch requests |
| `ChrononRecord::deserialize` (Rust) | Wire-format parser on attacker bytes |
| `ProbityReport::deserialize` (Rust) | Gossip payload parser |
| `Calendar::load` (Rust) | Calendar file parser |
| `tbid_handshake::verify_proof` (Rust) | Handshake response parser |

## §2.7 Testing Methodologies — Current State and Required

### §2.7.1 The #1 Process Failure — No PR-Blocking CI

`[Sources: pre-public-mvp §6.1, CLAUDE §4.1 §30.1, OPENCODE §6.1]` — Repeated in all three reviews.

`.github/workflows/release.yml` is the only workflow. Triggers on tag push. **No PR-time tests, no lint, no audit, no cross-language equivalence.**

For a pre-public security-sensitive project, this is the largest single gap. Any fix to a CRIT finding can regress on the next PR without notice.

**Recommended CI matrix (one PR-blocking workflow):**

```yaml
jobs:
  c-core:
    # Linux + clang + gcc; CMake + ctest; -fsanitize=address,undefined
  rust-build:
    # cargo build --workspace + clippy -D warnings
  rust-test:
    # cargo test --workspace
  rust-miri:
    # cargo +nightly miri test for unsafe blocks
  rust-kani:
    # cargo kani for bounded-model-check targets (§2.6.3)
  fuzz-smoke:
    # cargo fuzz run … 30s smoke tests on each target (§2.6.6)
  audit:
    # cargo audit + cargo deny
  cross-lang:
    # if Python re-introduced: maturin build + integration-tests/
  e2e:
    # multi-process serve/stamp/verify roundtrip (§2.7.3.10)
  p2p-smoke:
    # 2-3 node libp2p mesh forms + gossip propagates
```

### §2.7.2 Test Inventory by Category (consolidated)

| Layer | Files | LOC | Coverage feel | Notable gap |
|---|---|---|---|---|
| C11 core | `p2p/core/tests/test_*.c` (one per source) | ~14 files | Patchy | No round-trip Noise XX, no SPHINCS+/Dilithium/ML-KEM tests |
| Rust core-engine | inline `#[cfg(test)]` in 19 files + `integration_tests.rs` (475 LOC) | mid | OK happy path | No adversarial peer, no fuzz, no PQC cross-language |
| Rust foretias-server | inline + `tests/integration.rs` (610 LOC) | mid | Sanity-only | No JSON-RPC fuzz, no oversize-request, no malformed-foretis, no DHT eclipse |
| PyO3 bindings | backburnered | — | — | Re-introduce per SCOPE_REDUCTION roadmap |
| Shell sanity | `p2p/integration_sanity.sh`, `dht_stress_test.sh` | — | Manual only | Not in CI |

### §2.7.3 Missing Test Categories (Prioritized)

Compiled from `pre-public-mvp §6.2`, CLAUDE §6, OPENCODE §6.2:

| # | Test Type | Priority | Why |
|---|---|---|---|
| 1 | **Algorithm regression test** (stamp with each supported alg → serialize → deserialize → verify) | HIGH | Catches the prior `sign()/signature_algorithm()` regression class |
| 2 | **Cross-language PQC round-trip** (when Python re-introduced) | HIGH | Differential testing across implementations |
| 3 | **Calendar tamper resistance** (flip byte in each field: `forward_foretis`, `aa_nonce`, `public_key`, `signature_algorithm`; expect `integrity_check` false) | HIGH | Current coverage only for `forward_foretis` |
| 4 | **Concurrent stamping stress** (N threads × M stamps → calendar `integrity_check == all true`) | HIGH | Catches CAS race and keypair index bugs (§1.3.3, §1.3.5 in CLAUDE) |
| 5 | **Crash recovery: Python `save()`** | HIGH | Python lacks tempfile+rename atomicity |
| 6 | **Crash recovery: Rust `.tmp` integrity check after recovery** | HIGH | Currently skipped (§2.4.7) |
| 7 | **Identity-collision dual termination e2e** (two processes forced to same TBID → both go dormant within heartbeat window) | HIGH | Spec §0 invariant; no automated coverage |
| 8 | **JSON-RPC fuzz** (envelope, oversized, deeply nested, hex, batch) | HIGH | Highest pre-public attack surface |
| 9 | **C-core libfuzzer harnesses** (§2.6.6 targets) | HIGH | Largest unsafe-code surface |
| 10 | **PrivKey memzero verification** (post-tick, scan heap for previous key bytes; assert absent) | HIGH | Would have caught Dilithium drop bug |
| 11 | **End-to-end MVP** (3-process `serve` + `stamp` + `verify` orchestration) | HIGH | `integration_sanity.sh` exists but not CI-gated |
| 12 | **Replay protection** (capture heartbeat, replay after window, expect rejection) | HIGH | Currently unenforced (§2.4.4) |
| 13 | **Probity gossip end-to-end** (2 nodes, signed ProbityReport propagates, receiver score updates correctly) | HIGH | Trust foundation completely untested (§2.3.4) |
| 14 | **DHT eclipse / poisoning test** (attacker registers victim's TBID, verifier rejects) | HIGH | Currently not enforced (§2.3.2) |
| 15 | **FROST stub honesty test** (verify_epoch_snapshot returns false on stub signatures, true only on real ones) | MED | Forces honesty fix (§2.3.5) |
| 16 | **Property tests via proptest** (the 6 invariants in §2.6.4) | MED | Mechanically rigorous |
| 17 | **Multi-node P2P integration** (2-5 node mesh) | HIGH | Currently zero automated coverage |
| 18 | **Kani BMC for `Calendar::append` and `Tbid::from_raw`** (§2.6.3) | MED | Real verification surface |
| 19 | **Cross-version compat** (calendar from v0.1 binary loads in v0.2) | MED | Wire format stability |
| 20 | **Connection-flood resistance** (1000 concurrent JSON-RPC conns, no fd exhaustion → expect bounded accept rate) | MED | DoS resistance |

### §2.7.4 Test Quality Smells (action items)

`[Sources: pre-public-mvp §6.4, CLAUDE §34.19, OPENCODE §6.3]`

- `calendar_crash_recovery_corrupt_tmp` asserts buggy behavior. Convert to `#[should_panic]` with FIXME, or fix the bug. §1.5.G.
- Many tests use `rand::thread_rng()` + `unwrap()`. Non-deterministic. Replace with seeded `ChaCha20Rng`.
- Python tests rely on wall-clock `time.time()` for tick numbers; flaky on slow CI. Inject mock clock.

### §2.7.5 Recommended Test Tooling Stack

| Tool | Purpose | Setup effort |
|---|---|---|
| `cargo test` + `tokio::test` | Unit + integration | Already in place |
| `cargo clippy --all-targets -- -D warnings` | Lint gate | Already available, not enforced |
| `cargo fmt --check` | Format gate | Already available, not enforced |
| `cargo +nightly miri test` | UB detection in unsafe Rust | Add nightly toolchain in CI |
| `cargo fuzz` (libfuzzer) | Rust fuzz harnesses | Add `fuzz/` directory and targets per §2.6.6 |
| `cargo kani` | Bounded model checking | Install kani; add `#[kani::proof]` annotations |
| `cargo audit` | Known-vuln dependencies | Add to CI |
| `cargo deny` | Policy gating (licenses, dupes, advisories) | Add `deny.toml` |
| `proptest` | Property-based testing | Add as dev-dependency |
| `libfuzzer + AFL++` (C) | C-core fuzzing | Add `fuzz_targets/` for C sources |
| `Frama-C wp` | Formal proof for C annotations | Set up CI runner with Frama-C (§2.6.1) |
| `valgrind --leak-check=full` | Memory leak check on C tests | Add to CI |
| ASAN/UBSAN | Sanitizer runs on C tests | CMake target with `-fsanitize=address,undefined` |

## §2.8 Security Properties Summary

For inclusion in the whitepaper and for auditor reference.

### §2.8.1 Properties the System Provides

1. **Backdate impossibility (past chronons):** Once chronon N's key is algorithmically incapacitated by the advance to N+1, no valid Foretis for chronon N can be created. Holds under standard cryptographic assumptions (Ed25519/SHA-256 unbroken).
2. **Calendar integrity:** Forward/backward auto-attestation makes any modification detectable via `integrity_check()`.
3. **Offline verification:** Once obtained, a calendar verifies without network access.
4. **Cross-peer witness:** `external_attestations` provide corroboration from independent parties (when the field is wired through verifiers).

### §2.8.2 Properties the System Does NOT Provide

1. **Wall-clock accuracy:** Sequential integer tick numbers (§2.4.8); the system proves ordering, not absolute time.
2. **Protection against key-in-memory extraction:** Live tick's private key is in process memory and extractable via physical access, hypervisor exploit, or side-channel.
3. **Sybil resistance:** Creating nodes is free; probity scores can be biased (§2.3.4).
4. **Live-period majority-compromise resistance:** Until FROST ships (§2.3.5).
5. **Cryptographic binding of content to real-world events:** Proves content with hash H was *presented* at some moment; says nothing about authorship, correctness, or legal significance.

### §2.8.3 Properties to Add to Whitepaper

- Formal definition of the backdate-impossibility property.
- Security reduction under ROM + Discrete Log assumption (Ed25519 unforgeability) + collision-resistant hash (SHA-256).
- Sybil-resistance discussion (currently: none; future: probity-as-stake).
- Side-channel disclaimer (constant-time primitives from libsodium; Rust glue not audited).

---

# Part III — Other Concerns

The remaining sections cover product, architecture, P2P, spec drift, efficiency, future directions, and prioritized improvements. They are not less important — they are simply downstream of Parts I and II when sequencing SPEC work.

## §3 Product Identity & Mindshare

`[Sources: CLAUDE §1, OPENCODE §1]`

### §3.1 The One-Sentence Pitch Is Missing Everywhere

The core insight expressible in one sentence:

> *"A timestamp that is mathematically impossible to backdate — because the signing key self-destructs after every tick."*

(Note per AGENTS.md MISC §Description: substitute *"is cryptographically disabled"*, *"is algorithmically incapacitated"*, *"is verifiably impaired"* for "self-destructs" in formal docs.)

Appears nowhere in README.md, HOWTO.md, or other public-facing docs. README opens with acronym expansion and dense technical prose.

**Action:** README.md line 1 should be that sentence (or its formal equivalent), followed by install + quick start.

### §3.2 Latin Taxonomy Assessment

*Chronomatter*, *Communerd*, *Foretis*/*Foretias* — memorable and differentiating but require translation. Newcomers parse `Foretias`/`Foretis` as a typo.

**Action:** Add a terminology box at the top of every public-facing document. Consider `pub use Foretis as Timestamp;` aliases for library consumers.

### §3.3 Audience Segmentation

| Audience | Need | Gap |
|---|---|---|
| Application developers | Embed timestamping in their service | No published library crate, no SDK docs |
| Infrastructure operators | Run a timestamping network | CLI works; no monitoring, no auth |
| Auditors/verifiers | Verify a Foretis they received | `verify-with-proof` works; hard to discover |

### §3.4 Competitive Landscape

| Competitor | Strength | Foretias Advantage |
|---|---|---|
| RFC 3161 TSA | Widely deployed, enterprise trust | Decentralized, no trusted third party |
| OpenTimestamps | Public, Bitcoin-backed, free | No blockchain dependency, faster |
| Blockchain timestamps (ETH/BTC) | Decentralized, auditable | No transaction cost, no confirmation wait |
| Certificate Transparency | Public audit | Timestamps not just existence proofs |
| Hyperledger Fabric timestamps | Enterprise permissioned | Open-source, no consortium lock-in |

**Unique position:** Mathematically provable backdate-resistance without a trusted third party or blockchain, achieved via ephemeral key rotation.

## §4 Code Organization & Architecture (Beyond Rust Style)

`[Sources: CLAUDE §2, OPENCODE §2]`

### §4.1 Three-Crate Workspace

`core-engine` → `foretias-client` → `foretias-server`. Domain boundaries respected. Dependency chain logical and enforceable.

### §4.2 Chronomatter Concurrency Issues (consolidated)

Already covered substantively in Part I. To summarize for architectural readers:

- Stamp + daemon both increment `current_tick` via CAS; under load, stamp returns spurious `NodeError::Internal` (§1.3.3 fix: `fetch_add`).
- `keypairs: RwLock<Vec<TickKeyPair>>` unbounded; grows one entry per stamp; never evicted.
- `build_auto_attestation` uses `(tick-1)` as Vec index — fragile under concurrent stampers; should use the index returned by `generate_and_store_keypair()`.
- Write lock held over libsodium keygen (slow path).

### §4.3 Stamp Semantics — Every Stamp Is a New Tick

`chronomatter/mod.rs:302-320` — Every `stamp()` advances the tick. Spec's `serialized=False` mode (multiple stamps per tick, daemon advances) not implemented. Test `two_stamps_share_same_tick_if_no_daemon_advance` actually asserts the opposite of spec intent.

`[Sources: CLAUDE §2.1, OPENCODE §5.2]`

### §4.4 OnceLock Sprawl in Communerd

`communerd/mod.rs:66-80` — 11 `Arc<OnceLock<...>>` fields. P2P init failure is silent (some fields set, others not, no state observable).

`[Sources: CLAUDE §3.7]`

**Fix:** Replace with single state enum: `Uninitialized | Initializing | Ready | Failed(error)`. Provide `state()` accessor for observability.

### §4.5 P2P Event Channel Memory Leak Potential

`communerd/mod.rs:68` — `UnboundedReceiver<NetworkEvent>` in `OnceLock`. If never consumed (e.g., event loop fails to start), events accumulate without bound.

`[Sources: CLAUDE §20]`

**Fix:** Bounded channel with backpressure, or ensure consumer task is always spawned during `serve`.

### §4.6 id Field Read From params, Not Envelope

`server/handlers.rs:31, :74, :130, :240` — Every handler reads `params.get("id")`. JSON-RPC 2.0 places `id` at top of envelope, not inside `params`. Unless dispatcher manually injects, all responses have `id: null`, breaking response correlation.

`[Sources: pre-public-mvp §1.5, CLAUDE §2.7]`

### §4.7 block_on Deadlock in Async Context

`server/handlers.rs:114, :182, :292-296` — `tokio::runtime::Handle::current().block_on(async {...})` from a sync function called inside an async task. Deadlocks on single-thread runtime.

`[Sources: pre-public-mvp §1.5, CLAUDE §2.8, OPENCODE P1#18]`

**Fix:** Refactor handlers to `async fn`; use `.await` directly.

### §4.8 std::sync::Mutex in Async Context — Potential Blocking (NEW)

`communerd/mod.rs:131-136` — `namespace`, `_local_multiaddr_arc`, and `pending_lookups` all use `std::sync::Mutex` behind `Arc`, locked from within `tokio::spawn` tasks and async event loops (e.g., `namespace.lock().unwrap().clone()` at lines 259, 263, 380, 522, 653, 655). A `std::sync::Mutex` held across an `.await` boundary or by a long-running async task can block the entire async runtime thread, defeating the purpose of Tokio's cooperative scheduler.

This is a **different pattern** from §4.7's `block_on` issue. The `block_on` issue blocks an OS thread waiting for async work. The `std::sync::Mutex` issue blocks the async runtime thread while holding a synchronous lock. Under high load, if the mutex is held by a task that yields, the runtime may stall, limiting horizontal scalability and causing jitter in the event loop.

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]`

**Fix:** Replace with `tokio::sync::Mutex` (for async contexts) or `parking_lot::Mutex` (if hold time is proven to be sub-microsecond). For `namespace` and `_local_multiaddr_arc`, consider `Arc<OnceCell<String>>` / `Arc<OnceCell<libp2p::Multiaddr>>` if they are write-once-after-init. For `pending_lookups`, `tokio::sync::Mutex` or `tokio::sync::RwLock` is appropriate.

## §5 P2P Layer Analysis

`[Sources: CLAUDE §4, OPENCODE §4]`

### §5.1 Implementation Status (consolidated)

| Component | Implemented | Verified in CI | Verified manually |
|---|---|---|---|
| DHT peer discovery (Kademlia) | Yes | No | Partially (`dht_stress_test.sh`) |
| GossipSub probity + heartbeat | Yes | No | No |
| libp2p swarm + Noise handshake | Yes | No | `integration_sanity.sh` |
| Noise_XX TCP PtP | Yes | No | Manual only |
| Mutual attestation exchange | Yes | No | Partially |
| Mutual attestation verification | Partial | No | Buried in chronomatter |
| Collision detection + dormancy | Yes | No | No |
| FROST epoch consensus | **Stubbed** | No | No |
| Calendar replication (handlers) | Yes | No | No |
| Calendar replication (orchestration) | **Missing** | No | No |
| Cross-node verify via DHT | Yes (but unauthenticated, §2.3.2-3) | No | No |
| Liege channel | **Missing** | No | No |

### §5.2 Calendar Replication — Handlers Present, Orchestration Missing

`[OPENCODE §4.2 — new finding from live exploration]`

RPC handlers exist for `mirror_request`, `mirror_accept`, `ship_batch`, `ship_ack`, `stream_tick`, `stream_ack`, `mirror_mutual`, `mirror_reconcile`. But:

1. No background task pushes new chronons to mirrors.
2. No reconciliation timer; `handle_mirror_reconcile` exists but is never invoked.
3. No persistent connections; per-call connection cost.
4. `MirrorStore.base_dir` exists but is not used for disk persistence.
5. No retry on connection drop.

### §5.3 Noise_XX PtP — Functional, with Trade-offs

Two implementations: `foretias-client/src/noise_ptp.rs` (library) and `foretias-server/src/communerd/json_rpc_transport.rs` (server side). Both create a fresh Noise handshake per request. No session reuse.

**libp2p transport uses libp2p-noise**, not the C11 Noise_XX implementation. The P2P layer does not use the same cryptographic primitives as the C11 core. Acknowledged in code; should be documented as a C11 deviation.

### §5.4 Probity Gossip Without Spec Authority

`specs/FORETIAS_2_P2P_6_probity_gossip.md` is marked `[x] backburnered` but implementation is fully present in `foretias-server/src/probity/`. This is spec-implementation inversion.

`[Sources: CLAUDE §17]`

**Fix:** Un-backburner or supersede the spec with a new doc describing actual implementation invariants.

### §5.5 No MAX_PEERS in Peer Pool

Threat model recommends `MAX_PEERS = 256` for `DhtPeerSource`. Verify `PeerPool` also has cap; if not, attacker discovering bogus peers via DHT grows pool unboundedly.

`[Sources: CLAUDE §4.5]`

### §5.6 TBID Dual-Key Implications

- **49,920-byte signatures** in genesis tick; bandwidth significant for multi-thousand-node networks.
- **SLH-DSA verification slow** (~2-5 ms vs Ed25519 ~50 µs) — every TBID handshake adds this overhead (if dual-key is used; today only Ed25519 used in handshake — §2.4.5).
- **TBID hex representation** (192 hex chars) is operationally unwieldy; introduce `to_short_hex()` returning first 16 chars for logs.
- **Spec inconsistency:** `foretias-v1.md` says TBID is UUID v4; current Rust is 96-byte dual-key. Spec predates V1 TBID. Update.

`[Sources: CLAUDE §14]`

### §5.7 Mutual Attestation Endpoint — DoS Surface

Mutual attestation is a libp2p request_response. No rate limit per peer per chronon. A malicious peer can spam attestation requests, forcing the responder to do expensive signing on every request.

**Mitigation:** Rate-limit per peer; require evidence of liveness (e.g., a recent heartbeat) before fulfilling attestation.

## §6 Spec-to-Implementation Fidelity

`[Sources: OPENCODE §5, CLAUDE §32 (touches), pre-public-mvp §7]`

### §6.1 Spec Staleness Assessment

| Spec | Staleness | Key Issues |
|---|---|---|
| `FORETIAS_0_OVERVIEW.md` | ~40% stale | Project structure doesn't match 3-crate architecture; references removed Python/Java |
| `FORETIAS_1_MVP_SPEC.md` | ~30% stale | Python/PyO3 references persist after SCOPE_REDUCTION |
| `FORETIAS_2_P2P_SPEC.md` | ~20% stale | Path references; libp2p version mismatch |
| `foretias-v1.md` | 100% stale | Describes Python prototype that no longer exists |
| `FORETIAS_CLI_SPEC.md` | Broken | Truncated to 47 lines |
| `FORETIAS_ENCLAVE_SPEC.md` | **Missing** | Referenced by 3 specs; doesn't exist |

### §6.2 Critical Spec Contradictions

| Contradiction | Spec A | Spec B / Reality | Resolution |
|---|---|---|---|
| `tick_number` semantics | `foretias-v1.md`: nanoseconds since epoch | `questions.md`: sequential counter | Resolved in questions.md, not propagated |
| Config format | `OVERVIEW`: TOML example | `MVP_SPEC`: JSON filename | Code parses TOML with `.json` extension |
| Source of truth | `OVERVIEW §0.7`: Python canonical | Reality: Python deprecated | Not updated |
| Serialized mode | `v1.md §2.1`: two modes | Implementation: only one | Non-serialized mode unimplemented |
| Auto-attestation blob | Python: no nonce/`stamps_per_tick` | Rust: includes both | Cross-language incompatible (§2.4.1) |

### §6.3 Milestone Implementation Status

| Milestone | Spec | Plan | Code | Status |
|---|---|---|---|---|
| v0.1 (MVP) | ✅ | ✅ | ✅ | **Complete** |
| v0.2 (Direct P2P) | ✅ | ✅ | ✅ | **Complete** |
| v0.3 (libp2p Handshake) | ✅ | ✅ | ✅ | **Complete, tagged** |
| v0.4 (DHT Discovery) | ✅ | ❌ backburnered | ⚠️ partial | **Blocked — no plan** |
| v0.5 (Hardening) | ✅ | Backburnered | ❌ | **Backburnered** |
| v0.6 (Probity Gossip) | ✅ | Backburnered | ❌ (code exists, spec not) | **Backburnered (spec)** |
| v0.7 (Collision) | ✅ | Backburnered | ❌ | **Backburnered** |
| v0.8 (Epoch Consensus) | ✅ | Backburnered | ❌ stubbed | **Backburnered** |

**Implementation frontier: v0.4 DHT discovery is the next gate. v0.5-v0.8 chained.**

### §6.4 CLI Conformance

| Command | Spec | `main.rs` | Match |
|---|---|---|---|
| `serve` | ✅ | ✅ | ✅ |
| `stamp` | ✅ | ✅ | ✅ |
| `verify` | ✅ | ✅ | ✅ |
| `verify-with-proof` | `prove-verification` | `verify-with-proof` | ⚠️ Name mismatch |
| `inspect-attestations` | ✅ | ✅ | ✅ |
| `info` | ✅ | ❌ | Missing |

### §6.5 EncryptedJsonlCalendarStore Milestone Violation

`p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs` (347 lines, fully implemented) is a **v0.7 feature** per `FORETIAS_0 §14`. Current target is v0.1/MVP. Either feature-gate (`cargo feature "encrypted-calendar-store"`) or promote to active.

`[Sources: pre-public-mvp §1.6, CLAUDE §34.9]`

## §7 API & Library Usability

`[Sources: CLAUDE §5, OPENCODE §7]`

### §7.1 No Published Library Crate

`foretias-client` exists but is not published. No `cargo add foretias` equivalent. Minimal embed use case (`use foretias::TimeFamily; let tf = TimeFamily::new()?; let stamp = tf.stamp(b"...")?;`) is not the documented entry point.

### §7.2 Three-Level Instantiation Underexplained

`StandaloneConfig` < `PtpConfig` < `P2pConfig` represents progressive adoption but isn't surfaced in any developer guide.

### §7.3 verify() Return Ambiguity

`handlers.rs:173` — `valid: false` returned for both "signature invalid" and "TBID not in calendar." Caller cannot distinguish forgery from "I don't know that calendar."

**Fix:** Add a third state (`method: "unknown_tbid"`) or return JSON-RPC error.

### §7.4 Foretis JSON Has No Schema

No schema document, no OpenAPI definition. Consumers must read Rust struct to know fields. For third-party verification, stable documented schema is essential.

### §7.5 No Wire Format Versioning

JSON-RPC methods have no version prefix (`stamp` not `foretias/v1/stamp`). No version negotiation. `serde(rename = "tick_number")` compat alias is a workaround.

**Fix:** Add `version: u32` field to `Foretis`; version negotiation in stamp RPC.

## §8 Efficiency & Production Readiness

`[Sources: CLAUDE §7, §18, OPENCODE §8]`

### §8.1 Full Calendar Rewrite Per Stamp

`server/handlers.rs:63` — Every stamp triggers full JSON serialization and write of the entire calendar. O(N) bytes per stamp. `EncryptedJsonlCalendarStore` (append-only) is the right solution but not default.

### §8.1a Calendar::get — O(n) Linear Scan (NEW)

`core-engine/src/foretias/calendar.rs:135-141` — `CalendarLookup::get` uses `.filter(|t| t.chronon_number >= chronon_number).take(count).cloned().collect()` — a linear scan over all ticks followed by cloning each match. For large calendars (thousands of ticks), every `verify`, `get_calendar_slice`, and `integrity_check` call triggers a full linear scan + clone of matching records.

This is a **different concern** from §8.1's serialization cost. §8.1 is about the write path (serialize entire calendar on each stamp). §8.1a is about the read path (scan entire calendar on each lookup).

`[Sources: Qwen3.6-27B-AWQ-BF16-INT4 consolidated review, 2026-05-20 — NEW]`

**Fix:** Use `Vec::partition_point` or binary search on `chronon_number` (ticks are monotonically increasing) to find start index in O(log n), then slice for O(1) range access. Cloning is then limited to the actual requested range:
```rust
fn get(&self, chronon_number: u64, count: usize) -> Result<Vec<ChrononRecord>, NodeError> {
    let start = self.ticks.partition_point(|t| t.chronon_number < chronon_number);
    Ok(self.ticks[start..].iter().take(count).cloned().collect())
}
```

### §8.2 Eager PQC Keygen at Every SoftwareCryptoServer Init

`software.rs:51-79` — Generates SPHINCS+, Dilithium3, SLH-DSA-256f, ML-KEM-768 at every server start, even if only Ed25519 used. ~10-20 ms wasted at startup; ~10 KB secret material kept alive process lifetime.

**Fix:** Lazy-init per algorithm on first use.

### §8.3 RwLock Contention on keypairs

Already covered in §1.3.4. Hot stamp path serializes on keygen.

### §8.4 No Prometheus Metrics Endpoint

`metrics.rs` has atomic counters but no Prometheus exporter, Grafana dashboard, or structured logs for ELK/Splunk.

### §8.5 Graceful Shutdown Not Implemented

`chronomatter/mod.rs:413-419` — `handle.abort()` cancels at next `.await` without cleanup. Mid-save = partial write.

### §8.6 No Maximum Concurrent Connections

`server/mod.rs` — Unlimited TCP accept. fd exhaustion under flood.

### §8.7 Hard-Coded Paths and Custom Time Format

- `/tmp/foretias-mirrors` hard-coded (§2.3 risk + config issue).
- `time_being_reference_time` uses custom `UE+{ns}ns` format. Not ISO 8601, not RFC 3339, not Unix epoch number.

### §8.8 Calendar Slice Response Memory Explosion

`MAX_CALENDAR_SLICE_COUNT = 10_000` × SPHINCS+ 7,856 bytes × 2 ≈ ~157 MiB per JSON response.

**Fix:** Lower cap, stream, or move to binary protocol.

### §8.9 sig_input Built Twice

`tick.rs:79-92, :129-140` — Same byte layout built independently in `stamp()` and `verify()`. DRY violation; bug in one won't appear in the other.

**Fix:** Extract `fn build_sig_input(tbid, tick_number, content) -> Vec<u8>`.

### §8.10 Ed25519 Re-derives Keypair From Seed Per Sign

`signing_ed25519.c:12` — ~30 µs overhead per sign. Negligible for one-shot, significant for hot loops. Expose `_with_keypair` variant.

### §8.11 1 GiB Content Cap Pre-Decode

`server/handlers.rs:11, :45` — `MAX_CONTENT_BYTES = 1 GiB` checked after hex decode. Hex string of 2 GiB accepted into memory first. Effective 3× memory usage.

**Fix:** Limit hex-string length before allocation: `MAX_CONTENT_BYTES * 2 + slack`.

## §9 Documentation Architecture & Drift

`[Sources: pre-public-mvp §7, CLAUDE §10, §34.20]`

### §9.1 Documentation Taxonomy Too Complex for Newcomers

`foretias-v1.md`, `FORETIAS_0_OVERVIEW.md`, `FORETIAS_1_MVP_SPEC.md`, ..., `AGENTS.md`, `README.md`, `HOWTO.md`, `docs/threat_model_v0_5.md`, `specs/*.md` — dozens of files. No clear reading order for human contributors.

**Fix:** Create `CONTRIBUTING.md` with onboarding path: `README → HOWTO → foretias-v1.md → FORETIAS_0_OVERVIEW.md → specific feature spec`. Create `docs/architecture.md` summarizing system for contributors.

### §9.2 @human Comment System Carries Forward Unresolved Issues

Unresolved `(@human ...)` comments in specs are silently carried. Should be tracked as issues or spec TODOs. Notable: `FORETIAS_0_OVERVIEW.md §0.1` about dormant TBID query routing.

### §9.3 README Field Name Drift

- README claims `aa_nonce: bytes` on `TickRecord`; `models.py` lacks it.
- README documents `my_content_hash`; Rust uses `content_hash`.
- README lists `pyforetias`; `pyproject.toml` exports `foretias_p2p`.
- README mentions `python -m build`; `pyproject.toml` uses maturin backend.

### §9.4 Spec Paths Stale

`FORETIAS_0_OVERVIEW.md` references `foretias/p2p/node/`, `foretias/p2p/bindings/python/`. Actual: `p2p/core-engine/`, `p2p/foretias-server/`, `p2p/foretias-python/` (backburnered).

## §10 Marketing & Product Lifecycle

`[Sources: CLAUDE §9, §30, OPENCODE §1.4]`

### §10.1 Whitepaper Gap

`specs/WHITEPAPER_SPEC.md`, `specs/WHITEPAPER_PLAN.md` exist; whitepaper not written. Required for academic and enterprise trust.

Should include:
- Formal definition of backdate impossibility
- Security proofs under standard assumptions
- Comparison with prior work (RFC 3161, OpenTimestamps, CT)
- Known limitations (network partition, key-in-memory window)
- The 50%-compromise analysis (§2.2)

### §10.2 Naming Consistency

`Foretias` (project), `Foretis` (artifact), `foretias` (binary), `foretias-{core,client,server}` (crates), `foretias_p2p` (PyO3 module). The singular/plural distinction is elegant but `foretias stamp` returning a `foretis` looks like a typo.

### §10.3 Licensing

BSD 3-Clause Clear License. Consider Apache 2.0 + MIT dual license (Rust ecosystem standard). CLA decision needed before first external contributors.

### §10.4 First-Mover Prior Art Disclosure

arXiv preprint establishing priority on ephemeral-key-rotation timestamping is valuable before public release.

### §10.5 Minimum Viable Public Release Checklist

- [ ] BSD 3-Clause attribution in all source files
- [ ] SECURITY.md disclosure process
- [ ] CI passing on every PR (§2.7.1)
- [ ] `cargo audit` + `cargo deny` passing
- [ ] Memory leak check (valgrind on C tests)
- [ ] ASAN + UBSAN passing on C tests
- [ ] No `TODO`/`FIXME`/`HACK`/`STUB` in public APIs
- [ ] All `@human` comments resolved or tracked
- [ ] Whitepaper or technical note published
- [ ] Clear SemVer policy (CLI flags, wire format, library API)
- [ ] `CHANGELOG.md` for users

## §11 Future Directions

`[Sources: CLAUDE §8, §31, OPENCODE §9]`

### §11.1 Post-Quantum Readiness

- **NIST naming alignment:** `Dilithium3` → `ML-DSA-65` (FIPS 204), `ML-KEM-768` standardized (FIPS 203), SPHINCS+ → `SLH-DSA` (FIPS 205).
- **Hybrid signatures:** Ed25519 + ML-DSA combined (IETF hybrid-sig drafts) for transition period.
- **Algorithm sunset roadmap:** Currently absent. Ed25519 quantum vulnerability timeline matters for long-lived calendars.

### §11.2 Embedded / Resource-Constrained

- `privkey.c` `calloc/free` breaks no-alloc targets.
- liboqs (~200 source files, MB-scale) not embeddable as-is.
- PQC keygen unsuitable for constrained devices.

Keywords: `no_alloc`, `heapless`, `MIPS`, `ARM Cortex-M`, `thumbv7em-none-eabihf`.

### §11.3 Space Flight & High-Reliability

- Clock drift: chronon_ns configurable from external time sources.
- Radiation hardening: SEUs on DRAM key material; need ECC or checksumming.
- Bandwidth: SPHINCS+ 7856B vs Ed25519 64B; epoch-anchor SPHINCS+ instead of per-stamp.
- Asynchronous operation: offline ticking + sync protocol underspecified.

### §11.4 TEE Backends

`CryptoServer` trait is the right abstraction for SGX, SEV, TrustZone, Apple Secure Enclave, Android StrongBox. Requires `Chronomatter::new` DI fix (§1.3.8).

FIDO2/WebAuthn-style hardware-authenticator backend is a natural fit for web use cases.

### §11.5 Key Transparency & External Auditability

Open questions:
- Should calendars be published to global Merkle log (CT-style)?
- Should epoch snapshots be anchored in public ledgers?
- How to prove third parties that a received calendar is authentic?

### §11.6 Interoperability

- **RFC 3161 TSA bridge:** Enterprise PKI workflow integration.
- **OpenTimestamps bridge:** Foretias calendar anchored in Bitcoin/Ethereum for quantum-resistant temporal anchoring.
- **ANSI X9.95:** Financial timestamping compliance.

### §11.7 WASM / Browser Verification

`foretias-verify-wasm` crate exposing `verify_foretis(foretis_json, content, calendar_json) -> bool`. libsodium and liboqs both have WASM builds.

### §11.8 Threshold Timestamping

Extend FROST beyond epoch consensus to the stamp path. k-of-n time beings co-sign a single Foretis. Resists k-1 compromised signers; removes single-point-of-failure from stamping.

### §11.9 Zero-Knowledge Foretis Verification

ZK proof that "I possess a valid Foretis for content C at chronon N" without revealing the Foretis itself. Privacy-preserving verification for confidential content.

### §11.10 Content Commitment Schemes

KZG or Pedersen commitment over content allows batch stamping (1 stamp covers 1M docs) with individual inclusion proofs. High-throughput scenario.

### §11.11 Federated Calendar Networks

Multiple independent Foretias networks (different `dht_namespace`) cross-attesting. "US legal", "EU legal", "financial" networks forming web of trust across jurisdictions.

### §11.12 Latency-Optimized Verification

49,920-byte TBID signatures expensive to fetch. Cache public keys separately from full record; verify with 32-byte Ed25519 PK for Ed25519-signed Foretises.

### §11.13 Nanosecond Timestamp Recovery

Bridge sequential integer ticks to spec's nanosecond design: record server start time + tick duration; derive `start_time + tick_number * chronon_ns`. Approximation but restores absolute-time property.

### §11.14 Post-Compromise Attestation

"I was running fine until time T; after T, I may have been compromised." Mechanism for a node to attest last-known-good state and revoke authority for subsequent stamps.

### §11.15 Recursive Calendar Anchoring

If calendar A anchors calendar B (B's first chronon cross-signed by A), and C anchors A, you get a trust tree. Root calendar serves as PKI-like anchor.

## §12 Open Design Questions

Combined from CLAUDE §27, OPENCODE §10, `specs/questions.md`:

### §12.1 Unresolved from questions.md

| # | Question | Impact |
|---|---|---|
| 1 | FROST stub approach | v0.8 blocked |
| 3 | Noise_XX stub | **HIGH** — PtP depends on Noise_XX |
| 5 | Legacy MD5/SHA-1 library (OpenSSL dependency) | §2.3 OpenSSL concern |
| 8 | Dormant TBID bug | **CRITICAL** — wrong dormant semantics |
| 10 | Config format vs filename | Confusing |

### §12.2 Implicit Open Questions

| Question | Why it matters |
|---|---|
| When is `FORETIAS_ENCLAVE_SPEC.md` written? | v0.9+ backends cannot be specified without it |
| Is v0.4 the next priority? | v0.5-v0.8 are chained on v0.4 |
| Who owns `foretias-v1.md` update? | Python-oriented spec needs "legacy" marking |
| Chronon number: absolute time or sequence? | Cross-node ordering depends on resolution (§2.4.8) |
| Echo field: authenticated or advisory? | MITM can modify if advisory (§2.4.3) |
| Non-serialized mode: implement or document as removed? | High-throughput use cases depend on it (§2.4.9) |
| Echo field signed? | §2.4.3 |
| Python source-of-truth status? | `FORETIAS_0_OVERVIEW.md §0.7` vs reality |
| Source-of-truth canonicalization (Python deprecation) | §6.2 |
| Algorithm sunset planning? | §11.1 |

## §13 Application Use Cases and Their Gaps

`[Sources: CLAUDE §28]`

| Use Case | Current Status | Gap |
|---|---|---|
| Document signing with backdate-proof timestamp | Works (CLI) | No library API; no auth on stamp endpoint |
| Software release attestation | Works | No CI/CD integration; no metadata fields |
| ML experiment timestamping | Works manually | No Python library; no structured metadata |
| Legal document execution | Partial | No RFC 3161 bridge; no jurisdiction compliance docs |
| Supply chain attestation | Not yet | Need structured metadata (envelope around Foretis) |
| Financial trade timestamping | Not yet | Sub-second accuracy; ANSI X9.95 compliance |
| Distributed system causality ordering | Partial | Requires common TimeBeing reference |
| Satellite telemetry authenticity | Not yet | Offline operation + sync; radiation hardening |

Missing: **structured metadata in Foretis** (filename, version, requester, environment). A `SignedStamp` envelope wrapping Foretis with extension fields enables supply chain / legal / ML use cases without packing metadata into raw content bytes.

## §14 Prioritized Improvements — Consolidated P0/P1/P2/P3

Combined and de-duplicated from CLAUDE §12 and OPENCODE §11. Each item annotated with cross-reference. Items marked **NEW** are unique to the consolidated review.

### §14.0 P0 — Ship Blockers (Must Fix Before Any Public Announcement)

| # | Action | Sev | Reference |
|---|---|---|---|
| 1 | Remove `Debug` derive from secret binding types in `bindings.rs` | CRIT | §1.3.5, §2.3.1 |
| 2 | Add Ed25519 signature verification to probity gossip handler | CRIT | §2.3.4 |
| 3 | Add cryptographic binding (signature) to DHT peer registration | CRIT | §2.3.2 |
| 4 | Fix cross-node verify chain-of-trust | HIGH | §2.3.3 |
| 5 | Make `handle_verify_epoch_snapshot` return `false` not `true` (immediate honesty fix) | HIGH | §2.3.5 |
| 6 | Add PR-blocking CI workflow | CRIT | §2.7.1 |
| 7 | Fix C11 nonce overflow in `noise_xx.c` | CRIT | §2.3.9 |
| 8 | Fix C11 HKDF stack overflow in `privkey.c` | HIGH | §2.3.10 |
| 9 | Fix genesis verification bypass on short foretis | HIGH | §2.3.7 |
| 10 | Validate PQC sig->len against buffer (C + Rust wrappers) | HIGH | §2.3.8 |
| 11 | Fix `sign()`/`signature_algorithm()` algorithm-dispatch hardcoding | HIGH | §2.4.2, §1.3.9 |
| 12 | Resolve Python ↔ Rust auto-attestation incompatibility (drop Python OR sync layout) | CRIT (if Python kept) | §2.4.1 |
| 13 | Compile PQC C files in `p2p/core/CMakeLists.txt` (eliminate parallel build path) | CRIT | pre-public-mvp §1.2, CLAUDE §34.1 |
| 14 | Vendor MD5/SHA-1 (remove OpenSSL dep) or remove legacy hash surface | HIGH | pre-public-mvp §1.2, CLAUDE §34.2 |
| 15 | Lower `MAX_CONTENT_BYTES` (1 GiB → 16 MiB) and limit hex-string allocation pre-decode | HIGH | §8.11 |
| 16 | Replace `block_on` in handlers with `async fn` | HIGH | §4.7 |
| 17 | **NEW:** Apply AGENTS.md updates §1.5.A-I so future violations are caught at review time | HIGH | §1.5 |
| 18 | **NEW:** Add length bound to libp2p RPC codec `read_length_prefixed` (prevent 4 GB OOM) | CRIT | §2.3.15 |
| 19 | **NEW:** Add length bound to Noise PtP client `noise_json_rpc` response read (prevent 4 GB OOM) | CRIT | §2.3.16 |

### §14.1 P1 — Before First Public Tag

| # | Action | Sev | Reference |
|---|---|---|---|
| 18 | Bound `keypairs` Vec (LRU eviction + zeroize evicted) | HIGH | §4.2 |
| 19 | Fix CAS race in stamp/daemon (`fetch_add`) | HIGH | §1.3.3 |
| 20 | Add JSON-RPC authentication (token, mTLS) | HIGH | §2.3.13 |
| 21 | Disambiguate `verify()` return (`unknown_tbid` vs forgery) | MED | §7.3 |
| 22 | Calendar mirror integrity check on ingestion | HIGH | §2.3.6 |
| 23 | Implement calendar replication orchestration (push, reconcile timer, disk persistence) | HIGH | §5.2 |
| 24 | Wrap `noise_static_priv` in `Zeroizing`; impl Drop | HIGH | §1.3.11 |
| 25 | Wrap TBID secret `Vec<u8>` in `Zeroizing` | HIGH | §1.3.11 |
| 26 | Heartbeat freshness window + time-based nonce window | HIGH | §2.4.4 |
| 27 | Calendar `.tmp` recovery runs `integrity_check` | MED | §2.4.7 |
| 28 | Bound peer pool size; `MAX_PEERS` enforcement | MED | §5.5 |
| 29 | Fix algorithm hardcoding in `chronomatter` | HIGH | §1.3.9 |
| 30 | Plumb `CryptoServer` into `Chronomatter::new` (DI fix) | HIGH | §1.3.8 |
| 31 | Plumb `Clock` into `Chronomatter`, `TbidHandshake`, daemon | HIGH | §1.3.7 |
| 32 | Length-prefix all canonical signing payloads | MED | §1.5.F, §2.4.6 |
| 33 | Sweep `unwrap()` / `expect()` in protocol paths | HIGH | §1.3.1 |
| 34 | Remove `#[serde(default)]` from `stamps_per_tick` | MED | §1.3.10 |
| 35 | Audit ECDH stubs (`ecdh_ed25519`, `ecdh_p256`) — implement or document Noise XX trait gap | HIGH | §34 from CLAUDE |
| 36 | Implement P-256 or remove from spec | HIGH | pre-public-mvp §1.4 |
| 37 | Echo field decision: document advisory OR sign | MED | §2.4.3 |
| 38 | Tick numbers decision: keep sequential, document; OR migrate to nanoseconds | MED | §2.4.8 |
| 39 | One-sentence pitch + terminology box in README | MED | §3.1 |
| 40 | Whitepaper authored | HIGH | §10.1 |
| 41 | Publish library crate (`foretias-client` to crates.io) | MED | §7.1 |
| 42 | Write missing test categories #1, #3, #4, #8, #11, #12, #13, #14 from §2.7.3 | HIGH | §2.7.3 |
| 43 | Implement `info` CLI command | MED | §6.4 |
| 44 | Rename `verify-with-proof` to `prove-verification` per spec | LOW | §6.4 |
| 45 | **NEW:** Remove `unsafe impl Send` from `NoiseSession` (or wrap in `Arc<Mutex<>>`) | HIGH | §1.3.5 additional |
| 46 | **NEW:** Add algorithm mismatch check to local verify path in `handle_verify` | HIGH | §1.3.2 additional |
| 47 | **NEW:** Replace `std::sync::Mutex` with `tokio::sync::Mutex` in `communerd` async context | MED | §4.8 |
| 48 | **NEW:** Make `handle_get_latest_epoch` return error instead of all-zero stub | MED | §2.3.5 additional |
| 49 | **NEW:** Optimize `Calendar::get` from O(n) linear scan to O(log n) binary search | MED | §8.1a |

### §14.2 P2 — Soon After Public Tag

| # | Action |
|---|---|
| 45 | Implement FROST end-to-end (sign, verify, bridge); enable epoch consensus |
| 46 | Add wire format versioning (`version: u32` in Foretis; method namespacing) |
| 47 | Property tests (`proptest`) for invariants in §2.6.4 |
| 48 | C-core libfuzzer harnesses for targets in §2.6.6 |
| 49 | Multi-node P2P integration tests in CI |
| 50 | OnceLock sprawl → state enum in Communerd |
| 51 | Prometheus metrics endpoint + Grafana dashboard |
| 52 | Graceful shutdown with `CancellationToken` |
| 53 | Max concurrent connection limit |
| 54 | Mirror store path configurable; remove `/tmp` default |
| 55 | Replace `serde_cbor` with `ciborium`; audit `parking_lot`; pin `liboqs` version |
| 56 | OQS_init() one-shot in PQC init path |
| 57 | Lazy PQC keygen (per-algorithm on first use) |
| 58 | Streaming reader for `EncryptedJsonlCalendarStore::read_all` |
| 59 | Replication orchestration: push stream, reconcile timer, persistent connections |
| 60 | Atomic Python `save()` (tempfile+rename) — if Python re-introduced |
| 61 | Sweep early-return un-zeroed key material in `noise_xx.c` |
| 62 | Update spec paths to match 3-crate layout |
| 63 | README field-name reconciliation |

### §14.3 P3 — Strategic / Long Term

| # | Action |
|---|---|
| 64 | RFC 3161 bridge adapter |
| 65 | WASM browser verification crate |
| 66 | Embedded `no_alloc` C11 path |
| 67 | Hybrid classical/PQC signatures (IETF hybrid-sig) |
| 68 | Calendar public auditability (Merkle log, CT-style) |
| 69 | TEE backend (SGX/TrustZone) |
| 70 | Zero-knowledge Foretis verification |
| 71 | Threshold timestamping (FROST on stamp path) |
| 72 | Content commitment schemes (KZG/Pedersen) |
| 73 | Federated calendar networks |
| 74 | ML-DSA / ML-KEM naming alignment (FIPS 203/204/205) |
| 75 | Algorithm sunset roadmap |
| 76 | Frama-C/wp proofs in CI |
| 77 | Kani BMC for `Calendar::append`, `Tbid::from_raw` |
| 78 | Latency-optimized verification (key cache) |
| 79 | Recursive calendar anchoring |
| 80 | Post-compromise attestation mechanism |

---

# Part IV — Per-File Risk Assessment

## §15 Triage Table (consolidated and re-prioritized)

For maintainers prioritizing review attention. Files sorted by combined risk (highest first).

| File | Risk | Primary Concerns | Sources |
|---|---|---|---|
| `p2p/core-engine/src/core/bindings.rs` | **CRIT** | Debug derives on secret types (§2.3.1) | OPENCODE §14 |
| `p2p/core/src/noise_xx.c` | **CRIT** | Nonce overflow (§2.3.9); stack residue; 64KiB stack buffers | All three reviews |
| `p2p/core/src/privkey.c` | **CRIT** | HKDF stack overflow (§2.3.10); global state; dynamic alloc | All three reviews |
| `p2p/foretias-server/src/probity/gossip_handler.rs` | **CRIT** | Signature verification stubbed (§2.3.4) | OPENCODE |
| `p2p/foretias-server/src/communerd/mod.rs` | **HIGH** | DHT poisoning (§2.3.2); OnceLock sprawl; event channel leak | CLAUDE+OPENCODE |
| `p2p/foretias-server/src/server/handlers.rs` | **HIGH** | block_on (§4.7); auth gap; cross-node verify; ship_ack integrity (§2.3.3, §2.3.6, §2.3.13); content limits; id-from-params | All three reviews |
| `p2p/core-engine/src/crypto_server/software.rs` | **HIGH** | Algorithm dispatch hardcoded callers (§1.3.9); ECDH unimplemented; eager PQC keygen | All three reviews |
| `p2p/core-engine/src/chronomatter/mod.rs` | **HIGH** | CAS misuse (§1.3.3); unbounded keypairs; DI inconsistency (§1.3.8); locks over slow work (§1.3.4); algorithm hardcoded (§1.3.9) | CLAUDE+OPENCODE |
| `p2p/core-engine/src/foretias/tick.rs` | **HIGH** | Genesis bypass (§2.3.7); `serde(default)` (§1.3.10); SystemTime panics (§1.3.1); sig_input DRY (§8.9) | All three reviews |
| `p2p/core-engine/src/crypto_server/signing_tbid.rs` | **HIGH** | Rust Vec not zeroized (§1.3.11) | CLAUDE |
| `p2p/core/src/signing_sphincs.c` | **HIGH** | sig->len not validated against buffer (§2.3.8); OQS_MEM_cleanse len=0; no OQS_init | OPENCODE+pre-public-mvp |
| `p2p/core/src/signing_dilithium.c` | **HIGH** | Same as sphincs (§2.3.8) | OPENCODE+pre-public-mvp |
| `p2p/core-engine/src/epoch/frost_bridge.rs` | **HIGH** | Stub signature presented as valid; combined with handler returning `valid: true` | All three reviews |
| `p2p/core/src/nullifier.c` | **HIGH** | HMAC intermediate not zeroed | OPENCODE §14 |
| `p2p/foretias-server/src/server/mod.rs` | **HIGH** | `noise_static_priv` plain `[u8;32]` (§1.3.11); `unwrap()` on path; `/tmp` hardcoded | pre-public-mvp+CLAUDE |
| `p2p/foretias-server/src/communerd/p2p/tbid_handshake.rs` | **MED** | `unwrap()` on remote payload (§1.3.1); uses `rand::thread_rng()` not crypto.random_bytes(); Ed25519 only despite TBID dual-key (§2.4.5) | CLAUDE+session-read |
| `p2p/core-engine/src/collision/detector.rs` | **MED** | Count-based nonce window (§2.4.4); no timestamp freshness; dormancy unrecoverable | All three reviews |
| `p2p/foretias-server/src/calendar/mirror.rs` | **MED** | No disk persistence; no integrity check on insert (cross-ref §5.2, §2.3.6) | OPENCODE |
| `p2p/foretias-server/src/communerd/p2p/swarm.rs` | **MED** | No NAT traversal; no relay | OPENCODE §14 |
| `p2p/foretias-node/src/calendar_store/encrypted_jsonl.rs` | **MED** | v0.7 feature in v0.1 codebase; `read_to_string` over whole file; block-ID fallback to 0 on error | All three reviews |
| `p2p/core/src/memzero.c` | **MED** | Custom memzero vs `sodium_memzero` | OPENCODE §14 |
| `p2p/core/src/hash_legacy_insecure_md5.c`, `_sha1.c` | **MED** | OpenSSL link violates spec (§2.3 supply chain) | pre-public-mvp |
| `p2p/core-engine/src/probity/store.rs` | **LOW** | O(N) scans expected; watch as network grows | CLAUDE |
| `p2p/core/src/signing_ed25519.c` | **LOW** | Seed re-derivation per sign (§8.10) | pre-public-mvp |
| `p2p/core/src/rng_mix.c` | **LOW** | Misleading name; no mixing; `/dev/urandom` direct | pre-public-mvp |
| `p2p/core/src/merkle.c` | **LOW** | ACSL annotations without proofs (§2.6.1) | pre-public-mvp |

---

# Part V — Open Questions, Use Cases, Release Path, Closing

## §16 Open Design Questions Requiring Resolution Before Public

Already enumerated in §12; this section emphasizes those that are blockers vs nice-to-have:

**Blockers for "public-ready" claim:**

1. Algorithm dispatch hardcoded — fixed or documented (§2.4.2)
2. Python source-of-truth status (§6.2)
3. Echo field signed or advisory? (§2.4.3)
4. Chronon number absolute or sequential? (§2.4.8)
5. FROST: implement or feature-gate per §14.1 #45
6. Dormant TBID query routing (`questions.md #8`)

**Nice-to-have resolutions:**

7. Non-serialized mode (§2.4.9)
8. Config format/filename mismatch
9. Algorithm sunset planning (§11.1)
10. Enclave spec authoring (§12.2)

## §17 The Reassuring Architecture Summary

For the record (combined from CLAUDE §13 and OPENCODE §13): The core of Foretias works. The ephemeral key rotation mechanism is cryptographically sound. A calendar produced by this system, once a tick has advanced and the key has been algorithmically incapacitated, provides a verifiable guarantee that no one can produce a valid Foretis for that chronon's content without having been present during that specific window.

The nightmare scenario — more than half the network compromised — is survivable for past stamps because the cryptographic chain is self-contained and verifiable offline. For live and future stamps, FROST epoch consensus (currently stubbed) provides the safety net: with threshold k > N/2, an attacker holding N/2 nodes cannot produce a valid signed epoch snapshot.

**The gap between the architecture's promise and the current implementation's guarantees is bridgeable with the P0-P2 items above. The vision is correct. The implementation needs hardening.**

## §18 Adoption Funnel (for Marketing & Lifecycle SPECs)

For a developer going from "heard about Foretias" to "using in production":

1. **Discover** — GitHub, HN post, blog, conference
2. **Understand** — README in 60s (gap: no one-sentence pitch §3.1)
3. **Try** — `cargo install foretias` → `foretias serve` → `foretias stamp` in 5 min (works today)
4. **Evaluate** — Does it do what I need? (gap: no use-case docs §13)
5. **Integrate** — `cargo add foretias-client`, embed (gap: not published §7.1)
6. **Deploy** — Run a server, connect peers, monitor (gap: no metrics/auth §8.4, §2.3.13)
7. **Trust** — Verify security properties (gap: no whitepaper §10.1, no audit)

Steps 3 and 5 currently require significant manual effort. Steps 2, 4, and 7 not well served.

## §19 Closing — A Project Worth Completing

Foretias addresses a real problem: forgeability of timestamps. The core mechanism is elegant. Verification works offline without trusting any server.

The implementation is in a credible intermediate state. The foundational cryptographic layer (C11 + Rust wrappers) works. The P2P layer is wired but unverified. Product presentation and library usability need significant work before public adoption.

**The most urgent improvements, ranked by impact on public readiness:**

1. **Remove `Debug` from secret binding types** (§2.3.1) — immediate, low effort, high impact
2. **Fix probity signature verification** (§2.3.4) — trust model is broken until done
3. **Add PR-blocking CI** (§2.7.1) — every other fix can regress otherwise
4. **Fix DHT poisoning** (§2.3.2) — peer-impersonation vulnerability
5. **Make `verify_epoch_snapshot` return false** (§2.3.5 cheap fix) — honesty
6. **Fix C11 critical safety** (§2.3.9, §2.3.10, §2.3.12) — pre-public memory safety
7. **Add JSON-RPC auth** (§2.3.13) — anyone can stamp under your TBID today
8. **Update AGENTS.md per §1.5.A-I** — make future violations catchable at review time
9. **One-sentence pitch + whitepaper** (§3.1, §10.1) — adoption prerequisite
10. **Write the missing test categories** (§2.7.3) — protects against regression on all the above

Everything else is important but can be phased. The vision is clear, the architecture is sound, the team discipline (spec-first, terminology enforcement, security-first coding) is excellent. The project deserves to reach its potential.

---

## Appendix A: SPEC/PLAN Candidates Derivable From This Document

For convenience to the next layer of work (per the user's stated goal):

| Source section | Suggested SPEC.md |
|---|---|
| §1.3.1 | `RUST_UNWRAP_SWEEP_SPEC.md` |
| §1.3.2 | `UNVERIFIED_VERIFIED_TYPES_SPEC.md` |
| §1.3.3 + §1.5.B | `ATOMIC_COUNTER_DISCIPLINE_SPEC.md` |
| §1.3.4 + §1.5.B | `LOCK_AWAIT_AUDIT_SPEC.md` |
| §1.3.5 + §2.3.1 + §1.5.A | `SECRET_TYPE_DISCIPLINE_SPEC.md` |
| §1.3.6 + §1.5.D | `FFI_LENGTH_VALIDATION_SPEC.md` |
| §1.3.7 + §1.5.E | `CLOCK_INJECTION_SPEC.md` |
| §1.3.8 | `CRYPTO_SERVER_INJECTION_SPEC.md` |
| §1.3.9 | `ALGORITHM_DISPATCH_SPEC.md` |
| §1.3.10 | `SERDE_STRICTNESS_SPEC.md` |
| §1.3.11 | `ZEROIZATION_SWEEP_SPEC.md` |
| §1.3.12 + §1.5.F | `CANONICAL_ENCODING_SPEC.md` |
| §1.3.13 + §1.5.G | `TEST_DETERMINISM_SPEC.md` |
| §1.5 (all) | `AGENTS_MD_RUST_UPDATE_SPEC.md` |
| §2.1 | `THREAT_MODEL_V0_6_SPEC.md` (supersedes `v0_5`) |
| §2.3.2 | `DHT_REGISTRATION_AUTHENTICATION_SPEC.md` |
| §2.3.3 | `CROSS_NODE_VERIFY_CHAIN_OF_TRUST_SPEC.md` |
| §2.3.4 | `PROBITY_SIGNATURE_VERIFICATION_SPEC.md` |
| §2.3.5 | `FROST_HONESTY_FIX_SPEC.md` + `FROST_IMPLEMENTATION_SPEC.md` |
| §2.3.6 | `MIRROR_INTEGRITY_INGESTION_SPEC.md` |
| §2.3.7 | `GENESIS_LENGTH_VALIDATION_SPEC.md` |
| §2.3.9-10-11-12 | `C11_SAFETY_SWEEP_SPEC.md` |
| §2.3.13-14 | `JSON_RPC_AUTH_AND_TRANSPORT_SPEC.md` |
| §2.4.1 | `PYTHON_RUST_BLOB_RECONCILIATION_SPEC.md` |
| §2.4.4 | `HEARTBEAT_FRESHNESS_SPEC.md` |
| §2.6.1-2 | `FORMAL_VERIFICATION_ROADMAP_SPEC.md` |
| §2.6.6 + §2.7.3 #8-9 | `FUZZ_HARNESS_SPEC.md` |
| §2.7.1 | `CI_PR_BLOCKING_SPEC.md` |
| §2.7.3 | `MISSING_TEST_CATEGORIES_SPEC.md` |
| §3, §10 | `MARKETING_AND_ADOPTION_SPEC.md` |
| §5.2 | `CALENDAR_REPLICATION_ORCHESTRATION_SPEC.md` |
| §6 | `SPEC_STALENESS_REMEDIATION_SPEC.md` |
| §10.1 | `WHITEPAPER_AUTHORING_PLAN.md` |
| §11.x | One spec per future direction (per milestone) |
| §13 | `STRUCTURED_METADATA_ENVELOPE_SPEC.md` |
| §16 | `OPEN_DESIGN_QUESTIONS_RESOLUTION_PLAN.md` |

## Appendix B: Source-Document Cross-Reference

For traceability: where each prior review's findings have been incorporated.

| Prior review | Status |
|---|---|
| `pre-public-mvp-code-review.md §1-§9` | All findings present in this document, primarily in §2.3, §2.4, §6, §8, §14; remaining items in CLAUDE doc §34 are retained by reference |
| `CLAUDE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md §0-§34` | Fully absorbed; reorganized to put §1.x (Rust style audit) and §2.x (security) at top per user request |
| `OPENCODE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md §0-§15` | Fully absorbed; OpenCode-unique findings (Debug-derive on bindings, genesis bypass, probity sig stub, replication orchestration missing) elevated to top-priority security section |

No finding from any prior review has been omitted. The reorganization is structural, not lossy.

---

*End of combined review — Claude Opus 4.7 (max effort), 2026-05-19*
*Synthesizes work by Claude Opus 4.7 (pre-public-mvp), Claude Sonnet 4.6 (CLAUDE), OpenCode 1.14.28 / Qwen3.6-27B-AWQ-BF16-INT4 (OPENCODE)*
