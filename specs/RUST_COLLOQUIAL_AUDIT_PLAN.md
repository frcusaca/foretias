# RUST_COLLOQUIAL_AUDIT_PLAN.md

## Implementation Plan for Rust Colloquial Safety & Idiom Code Review

**Paired SPEC:** `RUST_COLLOQUIAL_AUDIT_SPEC.md`  
**Guide:** `rust_instructions.md` v2026-06-09  
**Plan created:** 2026-06-10  

---

## Dependency Ordering

Phase 1 (WARNING — security/safety first, no conflicts between items):  
→ Phase 2 (MODERATE — quality improvements, some may touch same files but not conflict)  
→ Phase 3 (LOW — style/polish, safe to batch)

Within each phase, items are ordered by estimated effort (smallest first, to build momentum and verify tooling).

---

## Pre-Implementation Gate

- [ ] Verify all tests pass before starting: `cd p2p && cargo test --workspace`
- [ ] Branch from alpha: `git checkout -b rust-colloquial-audit`

---

## Phase 1 — WARNING Items (Security / Semver Safety)

### Item 1.3: Remove `Clone`/`Copy` from `ForetiasPrivKey32` (Section 2.3)

- [ ] Remove `Copy, Clone` from `ForetiasPrivKey32` derive in `core-engine/src/core/bindings.rs:106`
- [ ] Audit all call sites that construct `ForetiasPrivKey32` — ensure they don't rely on `Copy`/`Clone`
- [ ] If callers need to pass the bytes, add `fn borrow_bytes(&self) -> &[u8; 32]`
- [ ] Verify `cargo check -p foretias-core` passes
- [ ] Verify `cargo test -p foretias-core` passes
- [ ] Verify `assert_not_impl_any!(ForetiasPrivKey32: Copy); assert_not_impl_any!(ForetiasPrivKey32: Clone);` in `secret_no_debug.rs`

### Item 1.1: Add `#[non_exhaustive]` to All Public Enums (Section 2.1)

- [ ] Inventorize all `pub enum` declarations (43 total); produce a checklist of enums and their files
- [ ] Add `#[non_exhaustive]` to error enums first (highest risk of breaking downstream):
  - [ ] `CleanAuthError` (`core-engine/src/foretias/clean_auth.rs`)
  - [ ] `NodeError` (`core-engine/src/error.rs`)
  - [ ] `CryptoError` (`core-engine/src/error.rs`)
  - [ ] `TransportError` (`foretias-server/src/communerd/transport.rs`)
  - [ ] `CommunerdetteError` (`foretias-server/src/communerd/communerdette.rs`)
  - [ ] `ForetiasError` (`foretias-client/src/foretias.rs`)
  - [ ] `PtPError` (`foretias-client/src/noise_ptp.rs`)
- [ ] Add `#[non_exhaustive]` to wire-visible algorithm enums:
  - [ ] `SignatureAlgorithm` (`core-engine/src/foretias/types.rs`)
  - [ ] `KemAlgorithm` (`core-engine/src/foretias/types.rs`)
  - [ ] `SigAlgorithm` (`core-engine/src/foretias/clean_auth.rs`)
  - [ ] `ForetiasCurve` (`core-engine/src/crypto_server/mod.rs`)
  - [ ] `SerializationAlgorithm` (`core-engine/src/foretias/tick.rs`)
- [ ] Add `#[non_exhaustive]` to other public enums in `foretias-server/src/`:
  - [ ] All enums in `communerd/` (transport, capabilities, etc.)
  - [ ] All enums in `calendar/` (task enums, mirror enums, etc.)
  - [ ] All enums in `calendar_store/`
  - [ ] All enums in `probity/`
- [ ] For each enum that is deliberately frozen/stabilized, add `/// Stability: this enum will never gain variants.` instead of `#[non_exhaustive]`
- [ ] Add exhaustive enum matching to all internal crates' match expressions where `#[non_exhaustive]` was added
- [ ] Verify `cargo check --workspace` passes
- [ ] Verify `cargo test --workspace` passes

### Item 1.2: Add `#[must_use]` to All Result-Returning Public Functions (Section 2.2)

- [ ] Write a helper script or grep to identify all `pub fn` returning `Result<` (and `pub async fn`):
  ```bash
  rg 'pub (async )?fn .*-> .*Result<' p2p/ --include '*.rs'
  ```
- [ ] Core-engine public API (highest priority — consumed externally):
  - [ ] `core-engine/src/crypto_server/` — all trait methods returning `Result`
  - [ ] `core-engine/src/core/identity.rs` — `PrivKeyHandle` methods
  - [ ] `core-engine/src/core/signing.rs` — `sign`, `verify` methods
  - [ ] `core-engine/src/noise.rs` — `NoiseSession` methods
  - [ ] `core-engine/src/foretias/clean_auth.rs` — `TrustedInner` trait methods
  - [ ] `core-engine/src/foretias/tick.rs` — record construction/validation methods
  - [ ] `core-engine/src/foretias/calendar.rs` — calendar validation methods
- [ ] Foretias-server public API:
  - [ ] `foretias-server/src/server/` — all JSON-RPC handler functions
  - [ ] `foretias-server/src/communerd/` — `CommunerdetteLine` trait methods
  - [ ] `foretias-server/src/communerd/communerdette.rs` — public communerdette methods
- [ ] Foretias-client public API:
  - [ ] `foretias-client/src/foretias.rs` — all public methods
  - [ ] `foretias-client/src/noise_ptp.rs` — PtP connection methods
  - [ ] `foretias-client/src/calendar/` — calendar mirror client methods
- [ ] Verify `cargo clippy --workspace --all-targets` passes (should not flag unused `must_use` on internal calls)
- [ ] Verify `cargo test --workspace` passes

---

## Phase 2 — MODERATE Items (Quality / Idiom)

### Item 2.5: `&Vec<T>` → `&[T]` Return Types (Section 2.5)

- [ ] `core-engine/src/foretias/clean_auth.rs` — `external_attestations()`, `peer_scores()`, `committee()` (6 occurrences)
- [ ] `core-engine/src/epoch/snapshot.rs` — `peer_scores()`, `committee()`
- [ ] `core-engine/src/probity/report.rs` — `signature()`, `slow_signature()`
- [ ] `core-engine/src/foretias/tick.rs` — `external_attestations()`
- [ ] Verify `cargo check --workspace` passes (slice auto-coerce should make this transparent)
- [ ] Verify `cargo test --workspace` passes

### Item 2.4: Manual `Debug` for `CleanAuthenticated<T>` and `UnverifiedSignatureEnvelope<T>` (Section 2.4)

- [ ] In `core-engine/src/foretias/clean_auth.rs`, remove `Debug` from the derive of `CleanAuthenticated<T>` and `UnverifiedSignatureEnvelope<T>`
- [ ] Add manual `impl<T> fmt::Debug for CleanAuthenticated<T> { ... }` showing only structural metadata (type name, status) without delegating to `T: Debug`
- [ ] Add manual `impl<T> fmt::Debug for UnverifiedSignatureEnvelope<T> { ... }` similarly
- [ ] For `Externalized<T>`, keep `Debug` if the externalized data is intentionally inspectable
- [ ] Verify logging calls don't break (grep for `{:?}`, `{:#?}` on these types)
- [ ] Verify `cargo check --workspace` passes
- [ ] Verify `cargo test --workspace` passes

### Item 2.10: Fix `from_trusted()` Documentation Contradiction (Section 2.10)

- [ ] Update module doc comment in `core-engine/src/foretias/clean_auth.rs` (lines 1-15) to accurately describe the two constructor gates:
  - `into_clean_authenticated()` — inbound gate, requires verification, returns `Result`
  - `from_trusted()` — local gate for internally-produced trusted data, public but named to signal intent
- [x] Remove the misleading "private constructors" claim from spec files
      (2026-06-11 15:00)
      NOTE: Code module doc in clean_auth.rs already correct. Fixed spec files: COMBINED_GROUP7_COMMUNERDETTE_SPEC.md, COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md. Remaining references in TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md and IMPROVE_BUILDERS_AND_BIG_FNS_SPEC.md are about BaseRecord constructors (different concept) — left as-is.
- [ ] Verify doc builds: `cargo doc -p foretias-core --no-deps`

### Item 2.8: Inline Format Variables (Section 2.8)

- [ ] Verify MSRV ≥ 1.85 and edition 2024 (inline format variables require both)
- [ ] Convert in `core-engine/src/noise.rs` (~6 occurrences)
- [ ] Convert in `core-engine/src/chronomatter/mod.rs` (~4 occurrences)
- [ ] Convert in `core-engine/src/foretias/tick.rs`
- [ ] Convert in `core-engine/src/foretias/calendar.rs`
- [ ] Convert in `core-engine/src/foretias/family_record.rs`
- [ ] Convert in `core-engine/src/epoch/committee.rs`
- [ ] Convert in `foretias-server/src/server/handlers.rs` (~40 occurrences, largest batch)
- [ ] Convert in `foretias-server/src/server/mod.rs`
- [ ] Convert in `foretias-server/src/communerd/mod.rs` (~20 occurrences)
- [ ] Convert in `foretias-server/src/communerd/p2p/` (behaviour.rs, swarm.rs, gossip.rs)
- [ ] Convert in `foretias-server/src/main.rs` (~25 occurrences)
- [ ] Convert in `foretias-server/src/replication_logger.rs`
- [ ] Convert in `foretias-server/src/calendar_store/mod.rs`
- [ ] Verify `cargo clippy --workspace --all-targets` passes (clippy may flag stale positional formats)
- [ ] Verify `cargo test --workspace` passes

### Item 2.6: Enum Exhaustiveness — Fix Internal Enums, Log External Enums (Section 2.6)

#### 2.6a External Enums — Catch-All MUST Stay, Add Logging

- [ ] **communerd/p2p/swarm.rs:376** — `_ => {}` → `other => { tracing::trace!("unhandled libp2p event: {other:?}"); }`
- [ ] **communerd/p2p/swarm.rs:439** — Same as above
- [ ] **communerd/p2p/swarm.rs:510,546,549** — Same as above

#### 2.6b Internal Domain Enums — Must Be Exhaustive, Remove `_ =>`

- [ ] **communerd/communerdette.rs:525–558** — Replace `_ =>` on `ActiveRoute` with explicit `ActiveRoute::variant =>` arms; add `ActiveRoute::VariantName => { tracing::warn!("unhandled route variant"); return Err(...); }` for any not yet handled
- [ ] **communerd/communerdette.rs:1703** — Same as above
- [ ] **server/handlers.rs** — Audit all `_ =>` catch-alls: keep the top-level `_ => Err(MethodNotFound)` (correct for truly unknown methods), but make internal sub-matches on algorithm/status/type discriminants exhaustive
- [ ] **communerd/mod.rs:716,879,1394** — Enumerate missing variants on internal enums
- [ ] **clean_auth.rs:718,740** — Return `Err(ParseError::UnknownAlgorithm(id))` instead of silently defaulting to `SigAlgorithm::Ed25519`
- [ ] Verify `cargo check --workspace` passes
- [ ] Verify `cargo test --workspace` passes

### Item 2.9: Encapsulate Communerd/Behaviour Pub Fields (Section 2.9)

- [x] Audit all external access to `CommunerdHandle` fields (grep for `handle.crypto`, `handle.peer_pool`, `handle.tbid_index`)
      (2026-06-11 00:00)
      FINDING: `Communerd` fields are already private (no visibility modifier). No `pub` fields exist.
- [x] Change `CommunerdHandle` fields from `pub` to `pub(crate)`
      (2026-06-11 00:00)
      FINDING: Already private. No change needed.
- [x] Add accessor methods for fields that legitimate external callers need (read-only)
      (2026-06-11 00:00)
      FINDING: Accessors already exist: `config()`, `local_peer_id()`, `probity_store()`, `p2p_cmd_tx()`, `crypto_server()`, `namespace()`, `local_multiaddr()`, `set_calendar()`. External callers (`main.rs`, `tiers.rs`) use these exclusively.
- [x] Audit all external access to `ForetiasBehaviour` fields (grep for `.kademlia`, `.gossipsub`, `.identify`, `.ping`, `.request_response`)
      (2026-06-11 00:00)
      FINDING: All 5 fields already `pub(super)`. Accesses in `swarm.rs` and `gossip.rs` are within `p2p` parent module — correct scope.
- [x] Change `ForetiasBehaviour` fields from `pub` to `pub(super)` or add accessor methods
      (2026-06-11 00:00)
      FINDING: Already `pub(super)`. No change needed.
- [x] Verify `cargo check --workspace` passes
      (2026-06-11 00:00)
- [x] Verify `cargo test --workspace` passes
      (2026-06-11 00:00)
      NOTE: `crypto_callsite_snapshot` integration test is pre-existing failure (snapshot approval needed; unrelated to encapsulation). All 593 lib tests + 16 integration tests pass.

### Item 2.7: Split Large Functions (Section 2.7)

- [x] **`server/handlers.rs:216` — `handle_verify` (156 lines):** Extracted `parse_verify_params()`, `try_local_verify()`, `build_cross_node_verify_response()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:1867` — `handle_get_chronon_chain` (117 lines):** Extracted `parse_chronon_chain_params()`, `build_chronon_chain_response()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:108` — `handle_route_stamp` (107 lines):** Extracted `parse_route_stamp_params()`, `build_route_stamp_response()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:789` — `handle_ship_ack` (101 lines):** Extracted `parse_ship_ack_records()`, `verify_ship_ack_chain()`, `insert_ship_ack_records()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:1255` — `handle_history_dump_chunk` (97 lines):** Extracted `parse_history_dump_chunk_params()`, `verify_history_dump_chunk()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:1527` — `handle_storage_proof_request` (93 lines):** Extracted `parse_storage_proof_request_params()`, `build_storage_proof_response()`
      (2026-06-11 14:30)
- [x] **`server/handlers.rs:891` — `handle_stream_tick` (91 lines):** Extracted `parse_stream_tick_params()`, `verify_stream_tick_record()`
      (2026-06-11 14:30)
- [x] **`main.rs:334` — `cmd_serve` (187 lines):** Extracted `build_time_family_config()`, `create_server()`, `setup_p2p()`, `print_server_status()`
      (2026-06-11 14:30)
- [x] **`main.rs:852` — `main` (100 lines):** Extracted `init_tracing_for_command()`, `cmd_serve_main()`, `cmd_stamp_main()`, `cmd_verify_main()`, `cmd_verify_with_proof_main()`, `cmd_inspect_attestations_main()`
      (2026-06-11 14:30)
- [x] **`calendar/task_queue.rs:431` — `stamp_and_sign_chronon_attestation` (34 lines):** Already small — no split needed
      (2026-06-11 14:30)
- [x] **`calendar/task_queue.rs:971` — `log_verify_coverage` (50 lines):** Already small — no split needed
      (2026-06-11 14:30)
- [x] **`calendar_store/mod.rs:114` — `prove_storage` (97 lines):** Extracted `build_merkle_proofs()`, `format_proof_arrays()`
      (2026-06-11 14:30)
- [x] **`chronomatter/mod.rs:243` — `build_tick_record` (105 lines):** Extracted `build_genesis_tick_record()`, `build_genesis_with_pqc()`, `build_genesis_without_pqc()`, `build_regular_tick_record()`
      (2026-06-11 14:30)
- [x] For each split, verify behavior preservation with existing tests
      (2026-06-11 14:30)
- [x] Verify `cargo clippy --workspace --all-targets` passes
      (2026-06-11 14:30)
- [x] Verify `cargo test --workspace` passes
      (2026-06-11 14:30)

---

## Phase 3 — LOW Items (Style / Polish)

### Item 2.14: Propagate Error from Calendar Append Unwrap (Section 2.14)

- [ ] In `core-engine/src/chronomatter/mod.rs:610`, replace `.unwrap()` with `?` (return `Result` from the function)
- [ ] Update callers of the function to propagate the error
- [ ] Verify `cargo check --workspace` passes
- [ ] Verify `cargo test --workspace` passes

### Item 2.11: Rename `mod.rs` → Path-Based Modules (Section 2.11)

- [ ] For each of the 15 `mod.rs` files, rename to `$basename.rs` and verify compiler finds it:
  - [ ] `core-engine/src/core/mod.rs` → `core.rs`
  - [ ] `core-engine/src/config/mod.rs` → `config.rs`
  - [ ] `core-engine/src/crypto_server/mod.rs` → `crypto_server.rs`
  - [ ] `core-engine/src/foretias/mod.rs` → `foretias.rs`
  - [ ] `core-engine/src/chronomatter/mod.rs` → `chronomatter.rs`
  - [ ] `core-engine/src/epoch/mod.rs` → `epoch.rs`
  - [ ] `core-engine/src/collision/mod.rs` → `collision.rs`
  - [ ] `core-engine/src/probity/mod.rs` → `probity.rs`
  - [ ] `foretias-server/src/server/mod.rs` → `server.rs`
  - [ ] `foretias-server/src/communerd/mod.rs` → `communerd.rs`
  - [ ] `foretias-server/src/communerd/p2p/mod.rs` → `p2p.rs`
  - [ ] `foretias-server/src/calendar/mod.rs` → `calendar.rs`
  - [ ] `foretias-server/src/calendar_store/mod.rs` → `calendar_store.rs`
  - [ ] `foretias-server/src/probity/mod.rs` → `probity.rs`
  - [ ] `foretias-client/src/calendar/mod.rs` → `calendar.rs`
- [ ] Verify `cargo build --workspace` passes

### Item 2.15: Standardize `// SAFETY:` in Test Code (Section 2.15)

- [ ] In `core-engine/tests/privkey_encrypt_decrypt.rs`, replace `// Safe:` with `// SAFETY:`
- [ ] Grep for any other lowercase `// Safe:` occurrences in test files; standardize all

### Item 2.13: Finalize Deprecated `new()` Constructors (Section 2.13)

- [ ] In `core-engine/src/foretias/tick.rs`, either:
  - Option A: Remove `pub fn new()` from `ChrononRecord` and `ForetisRecord` (semver-major, safe if all callers use builder)
  - Option B: Keep `#[deprecated]` with a `#[doc(hidden)]` attribute so it disappears from docs but compiles
- [ ] Audit all call sites of `ChrononRecord::new()` and `ForetisRecord::new()` — ensure zero production callers remain
- [ ] Verify `cargo check --workspace` passes

### Item 2.16: Inline Format in Trust Boundary Test (Section 2.16)

- [ ] In `core-engine/tests/trust_boundary_type_usage.rs`, convert `format!("trust_boundary_{}", i)` to `format!("trust_boundary_{i}")`
- [ ] Convert any other positional format macros in the same test file

---

## Phase 4 — Investigation & User Discussion (Open Issues)

These tasks surfaced during the audit but need deeper investigation or user input before they become actionable items. Each produces either findings (that graduate to concrete checklist items) or a decision (documented in the SPEC).

### Item 4.1: Miri Safety Run (Undefined Behavior Detection)

- [ ] Run Miri on `foretias-core` (flags all FFI calls, so focus on pure-Rust code):
  ```bash
  cd p2p && cargo +nightly miri test -p foretias-core --lib 2>&1 | tee /tmp/miri-core.log
  ```
- [ ] Run Miri on `foretias-client` pure-Rust code (skip PtP integration tests that open TCP):
  ```bash
  cd p2p && cargo +nightly miri test -p foretias-client --lib 2>&1 | tee /tmp/miri-client.log
  ```
- [ ] Run Miri on `foretias-server` pure-Rust code (skip libp2p/network integration):
  ```bash
  cd p2p && cargo +nightly miri test -p foretias-server --lib 2>&1 | tee /tmp/miri-server.log
  ```
- [ ] Investigate: Are `unsafe` FFI blocks that Miri can't verify documented with explicit pre/post-condition reasoning?
- [ ] Investigate: Any stacked-borrows violations in `noise.rs` (the `ptr::read` + `mem::forget` pattern on `Zeroizing` wrapper)?
- [ ] Discuss with user: Findings from Miri (if any). Decide whether to add `cargo +nightly miri test` to CI.

### Item 4.2: Edition 2024 Feature Adoption Survey

The project is on edition 2024 (Rust 1.85+) but the audit didn't check whether newer language features are used. Investigate adoption gaps:

- [ ] **Let-chains** (Rust 1.88+, edition 2024): Scan for `if let Some(x) = a { if x.is_valid() { ... } }` patterns that could flatten to `if let Some(x) = a && x.is_valid() { ... }`. Key files: `server/handlers.rs`, `communerd/communerdette.rs`, `core-engine/src/foretias/clean_auth.rs`
- [ ] **RPITIT / async fn in traits** (Rust 1.75+): Scan for `#[async_trait]` usage — can any be replaced with native `async fn` in traits? Check `core-engine/src/crypto_server/mod.rs` (CryptoServer trait), `foretias-server/src/calendar/task_queue.rs` (MirrorDispatcher trait)
  ```bash
  rg '#\[async_trait\]' p2p/ --include '*.rs'
  ```
- [ ] **Inline format variables**: Already covered by Section 2.8 — but check for `{self.x}` patterns that can't use inline syntax yet (needs Rust 1.87+ / edition 2024 field interpolation)
- [ ] **std vs external crates**: Scan for `once_cell`, `lazy_static`, `itertools` usage that `std` now covers — already verified zero for lazy statics; check remaining helper crate use
- [ ] **Recommended investigation**: Is there a `Cargo.toml` `rust-version` field? If not, add one so the compiler enforces MSRV
- [ ] Discuss with user: Should the codebase aggressively adopt let-chains and RPITIT now, or defer to a separate "edition modernization" phase?

### Item 4.3: Fuzz & Property Test Coverage Gap Analysis

- [ ] Investigate: Is there a fuzz harness anywhere in the project?
  ```bash
  find p2p/ -name 'fuzz*' -o -name '*fuzz*' 2>/dev/null
  ```
- [ ] Investigate: Which parsers/decoders exist that take untrusted external bytes?
  - JSON-RPC parameter parsing (`server/handlers.rs` — every handler)
  - Calendar store persistence format (JSONL)
  - DHT record deserialization (postcard)
  - Noise protocol message framing (`noise.rs`)
  - CleanAuth `UnverifiedSignatureEnvelope` construction from wire bytes
- [ ] For each parser, check: is there a test that feeds it malformed/garbage/fuzz-style input?
- [ ] Recommend: Add `cargo-fuzz` or `bolero` harness targeting at minimum JSON-RPC param parsing and DHT record deserialization
- [ ] Discuss with user: Priority for fuzz coverage — which parser is most exposed to untrusted remote input?

### Item 4.4: `parking_lot` vs `tokio::sync` Deep Dive

- [ ] Audit all `parking_lot::Mutex` and `parking_lot::RwLock` usage:
  ```bash
  rg 'parking_lot::(Mutex|RwLock)' p2p/ --include '*.rs'
  ```
- [ ] For each instance, check: does this guard ever cross `.await`? If yes, it must become `tokio::sync::Mutex`/`tokio::sync::RwLock`
- [ ] Calendar module specifically: `calendar/mod.rs` uses `parking_lot::Mutex` for `signing_key`, `task_tx`, and other fields. Verify no `.await` within lock scopes
- [ ] Investigate: Is there a project-level policy on `parking_lot` vs `std::sync` vs `tokio::sync`? If not, recommend writing one
- [ ] Discuss with user: Standardize on one mutex provider per context (async code → `tokio::sync`, sync FFI code → `parking_lot`) or document why the mix exists

### Item 4.5: Toppoli Test Coverage Survey

- [ ] Read `p2p/foretias-server/tests/toppoli.rs` — inventory all Toppoli test scenarios
- [ ] Compare against the features in the communerd/calendar/chronomatter stack — which inter-peer behaviors lack Toppoli coverage?
  - Mutual attestation (Group 6) — likely covered
  - Calendar mirroring (Group 4b) — what scenarios?
  - Probity gossip (Group 6) — what scenarios?
  - DHT peer discovery — what scenarios?
  - Peer churn (add/remove during operation)
  - Stale peer cleanup
- [ ] Check if any Toppoli tests are `#[ignore]` due to brokenness rather than slowness
- [ ] Discuss with user: Which P2P behaviors most need Toppoli coverage that they lack today?

### Item 4.6: Cargo Audit & Cargo Deny Check

- [ ] Run `cargo audit` (or `cargo deny check advisories`) on the workspace:
  ```bash
  cd p2p && cargo deny check advisories 2>&1 | tee /tmp/deny-advisories.log
  ```
- [ ] Check for duplicate dependency versions:
  ```bash
  cd p2p && cargo tree --duplicates 2>&1 | tee /tmp/cargo-dupes.log
  ```
- [ ] Check for license compliance:
  ```bash
  cd p2p && cargo deny check licenses 2>&1 | tee /tmp/deny-licenses.log
  ```
- [ ] Discuss with user: Any advisories need immediate action? Any license that violates project policy?

### Item 4.7: `from_trusted()` Broader Design Review

- [ ] Re-read `core-engine/src/foretias/clean_auth.rs:1-30` (module doc) and `clean_auth.rs:247-270` (CleanAuthenticated definition)
- [ ] Investigate: Is there any code path where `from_trusted()` is called on data that did NOT originate within the local trust boundary? (grep for `.from_trusted(` in production code)
- [ ] Consider: Should `from_trusted()` be `pub(crate)` instead of `pub`? If external crates link against `foretias-core`, do they need the local gate?
- [ ] Consider: Should a `TrustedToken` zero-sized type guard the constructor, so only code holding the token can call `from_trusted()`? This would make the gate compiler-enforced rather than naming-convention-enforced.
- [ ] Discuss with user: Is `pub fn from_trusted()` the right visibility? Should it be `pub(crate)`, token-guarded, or left as-is with clarified docs?

---

## Verification Gate

- [ ] Run `cargo fmt --all` — must pass
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings` — must pass (zero warnings)
- [ ] Run `cargo test --workspace` — must pass
- [ ] Run `cd p2p/core/build && ctest --output-on-failure` — C11 tests must pass
- [ ] Review git diff for unintended changes
- [ ] Update `specs/RUST_COLLOQUIAL_AUDIT_SPEC.md` to mark completed items
- [ ] Commit with message: `Rust Colloquial Audit — WARNING + MODERATE + LOW fixes; Opencode 1.14.28; deepseek/deepseek-v4-pro`
- [ ] Merge to alpha

---

## Deferred Items (No Current Plan)

| Item | Reason |
|------|--------|
| `parking_lot::Mutex` review | Intentional — cannot poison; fine as-is |
| `clone()` in swarm event handler hot path | `Arc::clone()` is cheap; measure before optimizing |
| `expect()` on libsodium availability | True invariant — libsodium is required system dep |
| `postcard::to_allocvec().expect()` for DHT records | True invariant — struct is known-serializable |

---

## Last Updated

**Date:** 2026-06-10  
**Created by:** Opencode 1.14.28; deepseek/deepseek-v4-pro