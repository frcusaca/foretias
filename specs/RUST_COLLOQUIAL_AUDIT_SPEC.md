# RUST_COLLOQUIAL_AUDIT_SPEC.md

## Rust Colloquial Safety & Idiom Code Review

**Audit scope:** `p2p/` Rust workspace (core-engine, foretias-server, foretias-client)  
**Guide evaluated against:** `rust_instructions.md` (2026-06-09)  
**Audit date:** 2026-06-10  
**Methodology:** Full-codebase grep audits across all `.rs` files, excluding `#[cfg(test)]` modules unless noted otherwise. Automated enforcement test (`trust_boundary_type_usage.rs`) verified at rest.

---

## SECTION 1 — POSITIVE PRACTICES & DISCIPLINES

These practices are effective, consistent, and well-aligned with the Rust guide. They represent a strong foundation and should be preserved as improvements roll in.

### 1.1 Error Architecture — Matchable, Thiserror-Based, No Stringly-Typed Errors

Every crate uses `thiserror` for matchable error enums with `#[from]` / `#[error(transparent)]` chaining. Library code contains zero `Box<dyn Error>`. CLI binary entry points (`main.rs`) use `Box<dyn Error>` only in `main()` — the standard Rust pattern.

- **`NodeError`** (19 variants) and **`CryptoError`** (8 variants) in `core-engine/src/error.rs`
- **`CommunerdetteError`, `TransportError`, `ForetiasError`, `CleanAuthError`, `ParseError`** — each domain has its own matchable error enum
- Callers can branch on failure mode; opaque propagation uses `anyhow` only in application layers

### 1.2 Type-Enforced Trust Boundaries

The `Unprocessed<T>` → `CleanAuthenticated<T>` → `Externalized<T>` three-stage type progression in `foretias/clean_auth.rs` is a standout security pattern. Automated enforcement exists via `trust_boundary_type_usage.rs` (497 lines) that scans each crate to verify these types appear only in approved locations. This directly implements the AGENTS.md trust boundary requirements and rust_instructions.md §1b.4 ("Make illegal states unrepresentable").

### 1.3 DHT Record Signing — Canonical Payload + Consumer Verification

`PeerRegistrationRecord::canonical_payload()` produces deterministic canonical bytes (signature field excluded), signed with Ed25519, and verified by every DHT consumer. `dht_record_signature.rs` has comprehensive tests for valid signatures, tampered records, wrong-pubkey records, and legacy unsigned compatibility. This aligns with rust_instructions.md §6 ("DHT and discovery records must be signed").

### 1.4 CommunerdetteLine Isolation

Calendar and Chronomatter have zero direct references to `Communerd`, `SwarmHandle`, `PeerPool`, or any communerd transport internals. All network access flows through `CommunerdetteLine` — a narrow, per-TBID-scoped handle that returns `CleanAuthenticated<R>`. The AGENTS.md invariant ("Communerdette is private to the `communerd` module") is upheld throughout.

### 1.5 Unsafe Discipline

All 107 `unsafe` blocks in production code carry `// SAFETY:` comments explaining invariant preconditions. `NoiseSession` correctly lacks `unsafe impl Send` (enforced by `assert_not_impl_any!(NoiseSession: Send)`). `PrivKeyHandle`'s `unsafe impl Send + Sync` is justified by a SAFETY comment covering KEK-encrypted C memory and libsodium thread safety.

### 1.6 Secret Key Material Handling

`Zeroizing<T>` wraps secret key material from allocation through hand-off. `TbidSecret` fields use `Zeroizing<[u8; 48]>` / `Zeroizing<[u8; 24]>`. `SoftwareCryptoServer` wraps seal keys, FROST shares, and PQC keys. `assert_not_impl_any` compile-time checks enforce `!Debug` on `ForetiasPrivKey32`, `ForetiasSecretKeyVar`, `ForetiasKemSecretKey`, `ForetiasTbidV1SecretKey`, `ForetiasNoiseState`, and `ForetiasFrostRound1`. This aligns with rust_instructions.md §6 ("Secret material handling").

### 1.7 Signing Key Ownership

Each component owns its own signing key. Chronomatter signs with its TBID key. Calendar signs with its own `PrivKeyHandle`. The deprecated `sign_tbid_message` on `TimeFamilyServer` is a pass-through to Chronomatter — not a global signing authority. This aligns with AGENTS.md ("Each component… owns its own signing key and signs only what it produced").

### 1.8 Concurrency Safety

No `panic!` in production code paths. No CAS-as-counter anti-pattern (`compare_exchange` — zero matches). Lock guards are dropped before `.await` points; where guards must cross `.await`, `tokio::sync::Mutex` is used correctly (verified in `task_queue.rs:297`).

### 1.9 Module Organization

Code is organized by responsibility — `core/`, `crypto_server/`, `foretias/`, `chronomatter/`, `communerd/`, `calendar/`, `calendar_store/`, `probity/`. No sprawling `utils` modules.

### 1.10 Dependencies & Proc-Macros

No gratuitous proc-macro dependencies. No `lazy_static!` / `once_cell` usage — MSRV ≥ 1.85 with edition 2024. The only proc-macro dependency (`bon` for `#[derive(Builder)]`) is well-justified for the `BaseRecord` builder pattern.

---

## SECTION 2 — FINDINGS REQUIRING CORRECTION

Findings are grouped by severity: **WARNING** (breaking API issue or security concern), **MODERATE** (quality/idiom deficit), **LOW** (style/minor). Each finding includes the rust_instructions.md rule reference and a concrete correction target.

### 2.1 WARNING — `#[non_exhaustive]` Missing on All Public Enums

**Rule:** rust_instructions.md §2 — "When a public enum or struct may gain variants/fields later, mark it `#[non_exhaustive]`."  
**Finding:** Zero `#[non_exhaustive]` annotations on any of 43 public enums. Adding a variant to `CleanAuthError`, `NodeError`, `SignatureAlgorithm`, `KemAlgorithm`, `TransportError`, or any error/wire enum is currently a semver-breaking change. External consumers (including test code in other crates) may match exhaustively and break.  
**Files affected:** All `pub enum` declarations across the workspace. Priority targets:

| Enum | File | Risk |
|------|------|------|
| `CleanAuthError` | `core-engine/src/foretias/clean_auth.rs:453` | Failure mode extension must not break API |
| `NodeError` | `core-engine/src/error.rs:7` | Core error type |
| `CryptoError` | `core-engine/src/error.rs:69` | Core error type |
| `SignatureAlgorithm` | `core-engine/src/foretias/types.rs:443` | Wire-visible algorithm enum |
| `KemAlgorithm` | `core-engine/src/foretias/types.rs:521` | Wire-visible algorithm enum |
| `SigAlgorithm` | `core-engine/src/foretias/clean_auth.rs:37` | Internal trust boundary enum |
| `TransportError` | `foretias-server/src/communerd/transport.rs:28` | Public API error |
| `ForetiasCurve` | `core-engine/src/crypto_server/mod.rs:12` | Public crypto configuration |
| All public enums in `communerd/`, `calendar/`, `calendar_store/` | Multiple files | -- |

**Correction:** Add `#[non_exhaustive]` to every `pub enum` that may gain variants. Exceptions: FFI C-compatible enums (if any) cannot use `#[non_exhaustive]` but should be explicitly documented as stable. Enums that are deliberately closed/frozen (none identified in this audit) may be left without `#[non_exhaustive]` with a `/// Stability: this enum will never gain variants.` doc comment.

### 2.2 WARNING — `#[must_use]` Missing on Result-Returning Functions

**Rule:** rust_instructions.md §2 — "When a function returns a `Result` or a value pointless to discard, mark it `#[must_use]`."  
**Finding:** Zero `#[must_use]` annotations across the entire `p2p/` codebase. Any public function returning `Result<T, E>` can have its error silently discarded by a caller. This affects:

- All `CryptoServer` methods (`sign`, `verify`, `public_key`, `encrypt`, `decrypt`)  
- `PrivKeyHandle::generate()`, `sign()`, `public_key()`  
- `NoiseSession::new()`, `write_message()`, `read_message()`  
- `TimeFamilyServer::stamp()`, `verify()`, `prove_verification()`  
- `Foretias::stamp()`, `Foretias::verify()` (client API)  
- Calendar `append()`, `validate_chain()`, etc.  
- All `CommunerdetteLine` methods  
- JSON-RPC handler response builders  

**Correction:** Add `#[must_use = "reason"]` to every public function returning `Result<T, E>`. Internal `pub(crate)` functions returning `Result` should also get `#[must_use]` unless the caller's logic intentionally ignores the result (rare — add an explicit `let _ =` to document the intent).

### 2.3 WARNING — `ForetiasPrivKey32` Derives `Clone` on Raw Private Key Bytes

**Rule:** rust_instructions.md §6 — "Do not `#[derive(Clone)]` on secret types unless the protocol requires it, and then each clone must itself be `Zeroizing`."  
**Finding:** `ForetiasPrivKey32` in `core-engine/src/core/bindings.rs:106` derives `Copy, Clone` on `[u8; 32]` raw private key bytes. While the code constructs this type ephemerally from `Zeroizing`-wrapped data, the type itself allows accidental duplication of raw secret bytes.  
**Correction:** Remove `Clone` and `Copy` from `ForetiasPrivKey32`. If copy semantics are needed for FFI calls, provide an explicit `fn borrow_bytes(&self) -> &[u8; 32]` accessor and require callers to construct the type with an explicit acknowledgment.

---

### 2.4 MODERATE — `CleanAuthenticated<T>` and `UnverifiedSignatureEnvelope<T>` Derive `Debug` Generically

**Rule:** rust_instructions.md §6 — "Do not log secrets, private keys, raw credentials, sensitive peer material, or unreduced protocol internals."  
**Finding:** `CleanAuthenticated<T>` (`clean_auth.rs:247`) and `UnverifiedSignatureEnvelope<T>` (`clean_auth.rs:95`) derive `Debug` on `T: Debug`. If `T` contains sensitive data (peer scores, raw attestation bytes, identity material), logging these types would leak it.  
**Correction:** Implement `Debug` manually to show only structural fields (wrapping status, length) without delegating to `T: Debug`. Alternatively, use a conditional `Debug` bound that requires `T: foretias_safe_debug::SafeDebug` (a new marker trait). Minimum: add `/// SAFETY: Debug prints only structural metadata, not inner T.` doc comment on the manual impl.

### 2.5 MODERATE — `&Vec<T>` Return Types (14+ Instances)

**Rule:** rust_instructions.md §5 — "Don't write `&Vec<T>` / `&String` parameters → `&[T]` / `&str`."  
**Finding:** While `&Vec<T>` is not used as function parameters (good), 14+ functions return `&Vec<T>` instead of `&[T]`, making callers unable to pass slices and requiring owned `Vec` access. Primary locations:

- `core-engine/src/foretias/clean_auth.rs` — `committee()`, `peer_scores()`, `external_attestations()` (6 occurrences)  
- `core-engine/src/epoch/snapshot.rs` — `peer_scores()`, `committee()`  
- `core-engine/src/probity/report.rs` — `signature()`, `slow_signature()`  
- `core-engine/src/foretias/tick.rs` — `external_attestations()`  

**Correction:** Change all `&Vec<T>` return types to `&[T]`. This is a non-breaking change for most callers (slices auto-coerce from Vec references) and broadens the API.

### 2.6 MODERATE — Enum Exhaustiveness: Distinguishing External vs Internal Enums

**Rule:** rust_instructions.md §2 — "When matching on your own enum, enumerate variants — avoid a catch-all `_` so new variants force a compile error"; §5 — "Don't catch-all `_` on your own enums. → enumerate variants."  

**Background:** In a distributed system receiving external data, the communerd's inbound handler *must* have a `_ =>` branch for external enums like libp2p `SwarmEvent` — those come from an external crate whose future versions may add new variants. No matter how many variants we handle today, a libp2p upgrade tomorrow could introduce one we haven't seen. The catch-all is **required** for safety.

However, for **internal domain enums** that this project defines and controls (`ActiveRoute`, `SigAlgorithm`, handler dispatch tables, etc.), the catch-all `_ =>` is a **bug** — it means adding a new variant silently falls through without a compile error.

#### 2.6a External Enums — Catch-All REQUIRED but Must Log, Not Silently Drop

| Location | Enum Source | Current Behavior | Fix |
|----------|-------------|-----------------|-----|
| `communerd/p2p/swarm.rs:376` | `libp2p::swarm::SwarmEvent` (external) | `_ => {}` (silent drop) | `other => { tracing::trace!("unhandled libp2p event: {other:?}"); }` |
| `communerd/p2p/swarm.rs:439` | `libp2p::swarm::SwarmEvent` | `_ => {}` (silent drop) | Same as above |
| `communerd/p2p/swarm.rs:510,546,549` | `libp2p::swarm::SwarmEvent` | `_ => {}` (silent drop) | Same as above |

**Correction:** Keep the `_ =>` branch (it must stay), but replace `{}` with a `tracing::trace!` log so that when libp2p adds events, we see them in logs instead of them vanishing silently. This also makes them discoverable during development.

#### 2.6b Internal Domain Enums — Must Be Exhaustive, Remove `_ =>`

| Location | Enum | Current Behavior | Fix |
|----------|------|-----------------|-----|
| `communerd/communerdette.rs:525,529,554,558,1703` | `ActiveRoute` (internal) | `_ =>` on own enum | Enumerate every `ActiveRoute` variant explicitly; add a compile-fail fallback for any truly not-yet-implemented: `ActiveRoute::UnsupportedVariant => { return Err(CommunerdetteError::UnsupportedRoute); }` |
| `server/handlers.rs` (10+ locations) | JSON-RPC dispatch tables (internal) | `_ =>` catch-alls | Match every known method explicitly; the catch-all for truly unknown methods is `_ => Err(MethodNotFound)` which is correct — but the internal sub-matches on things like algorithm, status, and type discriminants should be exhaustive |
| `communerd/mod.rs:716,879,1394` | Internal enums | `_ =>` on own types | Enumerate missing variants; add explicit error for unhandled |
| `clean_auth.rs:718,740` | `SigAlgorithm` deserialization (internal) | `_ => SigAlgorithm::Ed25519` (silent default) | Return `Err(ParseError::UnknownAlgorithm(id))` instead of silently defaulting to Ed25519 |

**Principle:** 
- **External data (libp2p events, peer wire messages):** `_ =>` is mandatory and correct. The arm must log, not silently discard.
- **Internal enums (ActiveRoute, SigAlgorithm, handler dispatch):** `_ =>` is forbidden. Enumeration forces the compiler to tell you when a new variant needs handling.

### 2.7 MODERATE — Large Functions (>80 Lines)

**Rule:** rust_instructions.md §5 — "Don't write large functions that mix validation, transformation, I/O, and mutation. → split by responsibility."  
**Finding:**

| Function | File | Lines |
|----------|------|-------|
| `handle_verify` | `server/handlers.rs:216` | 156 |
| `handle_get_chronon_chain` | `server/handlers.rs:1867` | 117 |
| `handle_route_stamp` | `server/handlers.rs:108` | 107 |
| `handle_ship_ack` | `server/handlers.rs:789` | 101 |
| `handle_history_dump_chunk` | `server/handlers.rs:1255` | 97 |
| `handle_storage_proof_request` | `server/handlers.rs:1527` | 93 |
| `handle_stream_tick` | `server/handlers.rs:891` | 91 |
| `cmd_serve` | `main.rs:334` | 187 |
| `main` | `main.rs:852` | 100 |
| `stamp_and_sign_chronon_attestation` | `calendar/task_queue.rs:431` | 196 |
| `log_verify_coverage` | `calendar/task_queue.rs:971` | 219 |
| `prove_storage` | `calendar_store/mod.rs:114` | 97 |
| `build_tick_record` | `chronomatter/mod.rs:243` | 105 |

**Correction:** Split each function into focused helper functions: validation phase, transformation, I/O, response building. `handlers.rs` functions follow a common pattern (parse params → validate → perform operation → build response) that lends itself to extraction. `cmd_serve` in `main.rs` can be decomposed into sub-cli modules.

### 2.8 MODERATE — `format!("{}", x)` Instead of Inline `format!("{x}")`

**Rule:** rust_instructions.md §5 — "Don't write `format!("{}", x)`. → `format!("{x}")`."  
**Finding:** Hundreds of `format!("{}", x)` across all three crates. Edition 2024 + MSRV ≥ 1.85 supports inline variable interpolation. Worst-hit files: `server/handlers.rs` (~40 occurrences), `main.rs` (~25 occurrences), `communerd/mod.rs` (~20 occurrences), `calendar_store/mod.rs` (~10+), `noise.rs` (~6), `chronomatter/mod.rs` (~4).  
**Correction:** Convert all `format!("{}", x)` to `format!("{x}")`, all `format!("... {} ...", expr)` to `format!("... {expr} ...")`. Use sed or an automated refactor (rustfix may handle this via clippy).

### 2.9 MODERATE — Pub Fields on `Communerd`, `CommunerdHandle`, `ForetiasBehaviour`

**Rule:** rust_instructions.md §1b.3 — "Encapsulation over exposure. Private fields and behavior-based APIs over public fields and raw state."  
**Finding:** `CommunerdHandle` (`communerd/mod.rs:74-94`) exposes `pub crypto`, `pub peer_pool`, `pub tbid_index`. `ForetiasBehaviour` (`communerd/p2p/behaviour.rs:11-15`) exposes all libp2p behaviour components as pub fields. These allow callers to bypass the intended API and mutate internals.  
**Correction:** Make fields `pub(crate)` and expose intentional accessor methods. For `ForetiasBehaviour`, consider a `libp2p::swarm::NetworkBehaviour` re-export if external code needs access to the behaviour itself (not its fields).

### 2.10 MODERATE — Known `from_trusted()` Documentation Contradiction

**Rule:** rust_instructions.md §3 — "Do comment to explain *why*, not *what*."  
**Finding:** `CleanAuthenticated::from_trusted()` at `clean_auth.rs:258` is `pub` despite the module doc comment at lines 8-9 claiming "private constructors." The design may be intentional (a "local gate" for data produced within the trust boundary), but the doc contradicts reality and could mislead reviewers.  
**Correction:** Update the module doc to accurately describe the two constructor gates: `into_clean_authenticated()` (inbound gate, requires verification) and `from_trusted()` (local gate, for data created locally within the trust boundary). Clarify that `from_trusted()` is `pub` because local code in other modules may legitimately create trusted data.

---

### 2.11 LOW — `mod.rs` Legacy Pattern (15 Files)

**Rule:** rust_instructions.md §5 — "Don't add `mod.rs` files. → path-based modules." Rust 2018+ prefers `$modname.rs` alongside `$modname/` directory.  
**Finding:** 15 `mod.rs` files exist. This is the older convention but non-breaking. New modules should use `$modname.rs` + `$modname/` directory.  
**Correction:** Rename `mod.rs` → `$basename.rs` in each directory. Example: `server/mod.rs` → `server.rs`. Update all `#[path]` attributes if any exist. This is mechanical.

### 2.12 LOW — `SignatureBytes` Lacks `ZeroizeOnDrop`

**Rule:** rust_instructions.md §6 — "`SignatureBytes` implements `Zeroize` but **not** `ZeroizeOnDrop` — any `SignatureBytes` holding secret material must be explicitly zeroized."  
**Finding:** This is a documented TODO (`signing_tbid.rs:34-37`). The code already explicitly zeroizes `SignatureBytes` when it holds secret material, but automatic zeroing on drop would reduce the risk of forgetting to `.zeroize()`.  
**Correction:** Either implement `ZeroizeOnDrop` for `SignatureBytes` or add a separate `SecretSignatureBytes` newtype that does, to avoid zeroizing public signatures unnecessarily.

### 2.13 LOW — Deprecated `new()` Constructors Still Public

**Finding:** `ChrononRecord::new()` (`tick.rs:57-60`) and `ForetisRecord::new()` are marked `#[deprecated]` but remain `pub`. The `bon` builder API is the recommended path.  
**Correction:** Either remove these constructors (breaking, semver-major) or leave them deprecated with a clear migration note. Current state (deprecated + pub) is acceptable during transition.

### 2.14 LOW — Production `.unwrap()` on Calendar Append

**Rule:** rust_instructions.md §5 — "Don't `.unwrap()` in library, protocol, parser, interpreter, FFI, or production paths."  
**Finding:** `core-engine/src/chronomatter/mod.rs:610` — `self.calendar.write().append(tick_record.clone()).unwrap()`. Calendar append can fail (disk I/O, capacity). Unwrap in the daemon's hot path will panic.  
**Correction:** Propagate the error with `?` or handle it with a retry/log strategy appropriate to the daemon context. This is the only production-path `.unwrap()` not justified by a compile-time invariant.

### 2.15 LOW — Test Code Uses `// Safe:` (Lowercase) for Unsafe Comments

**Finding:** `core-engine/tests/privkey_encrypt_decrypt.rs:11` uses `// Safe:` instead of `// SAFETY:`. All production code uses the uppercase convention.  
**Correction:** Standardize to `// SAFETY:` in test code as well, for consistency and greppability.

### 2.16 LOW — `format!("trust_boundary_{}", i)` Pattern in Test Code

**Finding:** Test file `core-engine/tests/trust_boundary_type_usage.rs` uses `format!("trust_boundary_{}", i)` for loop variable construction.  
**Correction:** Convert to `format!("trust_boundary_{i}")`.

---

## SECTION 3 — DEFERRED OR INTENTIONAL CHOICES (No Action)

These items were found during the audit but are intentional design decisions or already tracked separately:

| Finding | Reason Deferred |
|---------|----------------|
| `parking_lot::Mutex` usage in Calendar (cannot poison) | Fine — `parking_lot::Mutex::lock()` never returns `Err`, so `.unwrap()` is noise-free. |
| `expect()` on libsodium availability (23 instances in communerdette) | True invariant — libsodium is a required system dependency. |
| `postcard::to_allocvec().expect()` for DHT record serialization | True invariant — struct is known-serializable, failure indicates a bug, not runtime input. |
| `clone()` in swarm event handler hot path | Calls `Arc::clone()` on libp2p types — minimal cost. Documented for future measurement. |
| `CleanAuthenticated::from_trusted()` being `pub` | Intended — local gate for internally-produced trusted data. Doc contradiction (2.10) to be resolved. |

---

## SECTION 4 — SUMMARY OF CORRECTION ITEMS BY PRIORITY

| # | Severity | Section | Item | Est. Effort |
|---|----------|---------|------|-------------|
| 1 | WARNING | 2.1 | Add `#[non_exhaustive]` to all public enums | Medium (43 enums, each needs review) |
| 2 | WARNING | 2.2 | Add `#[must_use]` to all Result-returning public functions | Large (audit every pub fn → Result) |
| 3 | WARNING | 2.3 | Remove `Clone`/`Copy` from `ForetiasPrivKey32` | Small |
| 4 | MODERATE | 2.4 | Manual Debug for `CleanAuthenticated<T>` | Small |
| 5 | MODERATE | 2.5 | `&Vec<T>` → `&[T]` return types | Small (14 locations) |
| 6 | MODERATE | 2.6 | Enumerate enum variants, remove `_ =>` | Medium (5 files) |
| 7 | MODERATE | 2.7 | Split large functions | Large (13 functions) |
| 8 | MODERATE | 2.8 | Inline format variables | Medium (automated, then review) |
| 9 | MODERATE | 2.9 | Encapsulate Communerd/Behaviour pub fields | Medium |
| 10 | MODERATE | 2.10 | Fix `from_trusted()` documentation | Trivial |
| 11 | LOW | 2.11 | Rename `mod.rs` → `$basename.rs` | Small (15 files) |
| 12 | LOW | 2.12 | `SignatureBytes` ZeroizeOnDrop (or newtype) | Small |
| 13 | LOW | 2.13 | Remove or finalize deprecated `new()` constructors | Trivial |
| 14 | LOW | 2.14 | Propagate error from calendar append unwrap | Small |
| 15 | LOW | 2.15 | Standardize `// SAFETY:` in test code | Trivial |
| 16 | LOW | 2.16 | Inline format in trust_boundary_type_usage test | Trivial |

---

## SECTION 5 — OPEN INVESTIGATION AREAS (Requires User Discussion)

These areas were identified during the audit but lie outside the scope of mechanical fixes. Each needs investigation and a user decision before becoming concrete action items. Corresponding PLAN tasks are in Phase 4.

### 5.1 Miri Safety — Undefined Behavior Detection

**What:** The codebase has 107 `unsafe` blocks (all documented), but Miri (the Rust UB detector) has never been run. FFI-heavy code is especially prone to stacked-borrows violations, use-after-free from C memory, and alignment issues. The `noise.rs` `ptr::read` + `mem::forget` pattern on `Zeroizing` wrappers is particularly worth Miri scrutiny.

**Question for user:** Should Miri be added to CI (e.g., `cargo +nightly miri test -p foretias-core --lib`)? If findings surface, what's the priority for fixing them vs deferring?

### 5.2 Edition 2024 Feature Adoption

**What:** The project is on edition 2024 but the audit didn't measure adoption of newer language features. Let-chains (Rust 1.88+) could flatten deeply nested conditionals in `handlers.rs` and `clean_auth.rs`. RPITIT (async fn in traits, Rust 1.75+) could replace `#[async_trait]` usage on `CryptoServer` and `MirrorDispatcher` traits.

**Question for user:** Aggressively adopt let-chains and RPITIT now, or defer to a separate modernization phase?

### 5.3 Fuzz & Property Test Coverage

**What:** The parsers exposed to untrusted external bytes (JSON-RPC params, DHT postcard deserialization, Noise framing, calendar JSONL) have no fuzz harness. A single malformed byte from a peer enters these parsers — property-based or fuzz testing would catch edge cases that static analysis cannot.

**Question for user:** Which parser is most exposed to untrusted remote input and should be fuzzed first? Is `cargo-fuzz` acceptable as a new dev dependency?

### 5.4 `parking_lot` vs `tokio::sync` Policy

**What:** The codebase uses both `parking_lot::Mutex` (Calendar, communerd, calendar_store) and `tokio::sync::Mutex` (task_queue). Both are correct in their current contexts, but no project-level policy documents when to use which. A `.await` inside a `parking_lot` scope would be a runtime deadlock that the compiler doesn't catch.

**Question for user:** Should we standardize: `tokio::sync` for async code, `parking_lot` for sync-only FFI paths, and add a clippy lint or CI check for `parking_lot` guards across `.await`?

### 5.5 Toppoli Test Coverage Gaps

**What:** The Toppoli integration harness tests multi-peer behavior, but the coverage boundaries aren't documented. Which P2P behaviors (mutual attestation, calendar mirroring, probity gossip, DHT discovery, peer churn) have Toppoli tests, and which don't?

**Question for user:** Which P2P behavior most critically needs a Toppoli test that it lacks today?

### 5.6 Cargo Audit / Dependency Health

**What:** No automated advisory or license check runs on the workspace. Given the security-critical nature of the project, `cargo deny check advisories` should run regularly.

**Question for user:** Should `cargo deny` be added to CI? Are there any license restrictions that would block adding it?

### 5.7 `from_trusted()` Visibility — `pub` vs `pub(crate)` vs Token-Guarded

**What:** `CleanAuthenticated::from_trusted()` is `pub fn` — any code in any crate (including external consumers of `foretias-core`) can construct a trusted wrapper. If external crates don't need the local gate, `pub(crate)` would make the trust boundary mechanically stronger. A `TrustedToken` pattern would make it compiler-enforced.

**Question for user:** Should `from_trusted()` be `pub(crate)` to prevent external crates from constructing trusted data? Or is the current `pub` visibility needed for current or planned external consumers?

---

## Last Updated

**Date:** 2026-06-10  
**Audit performed by:** Opencode 1.14.28; deepseek/deepseek-v4-pro  
**Guide:** `rust_instructions.md` v2026-06-09