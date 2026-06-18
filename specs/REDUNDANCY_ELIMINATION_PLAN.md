# Redundancy Elimination Plan

**Spec:** `specs/REDUNDANCY_ELIMINATION_SPEC.md`
**Generated:** 2026-06-12 (updated with implementation analysis)
**Branch:** `redundancy-elimination`
**Worktree:** `${HOME}/tmp/foretias-worktrees/REDUNDANCY_ELIMINATION_PLAN_4821`

---

## Worktree Lifecycle

- [x] Create worktree `git worktree add -b redundancy-elimination ${HOME}/tmp/foretias-worktrees/REDUNDANCY_ELIMINATION_PLAN_4821`
      (2026-06-12 10:15)
- [x] `cd ${HOME}/tmp/foretias-worktrees/REDUNDANCY_ELIMINATION_PLAN_4821`; reset current session work directory to be the full worktree path.
      (2026-06-12 10:15)

---

## Phase 1: MirrorStore — Delete server copy

**Risk:** LOW | **Lines removed:** ~276 | **Files changed:** 5-6

### Steps

- [x] Add `latest_record()` to `foretias-client/src/calendar/mirror.rs` (copy from server `mirror.rs:79-84`)
      (2026-06-12 10:20)
- [x] Port the extra assertion from server's `insert_and_get_mirrored` test (server line 169 checks `records[1]`) to client test
      (2026-06-12 10:20)
- [x] Delete `foretias-server/src/calendar/mirror.rs` (276 lines)
      (2026-06-12 10:21)
- [x] Update `foretias-server/src/calendar/mod.rs:40` — change `pub use mirror::{compute_hash_sanity, MirrorStore};` to re-export from `foretias_client::calendar::mirror`
      (2026-06-12 10:21)
- [x] Update imports in `foretias-server/src/server/mod.rs:23,51,114,149,220`
      (2026-06-12 10:21)
- [x] Update imports in `foretias-server/src/server/handlers.rs:853,935,939,1299,1308`
      (2026-06-12 10:21)
- [x] Verify: `cargo test -p foretias-client && cargo test -p foretias-server`
      (2026-06-12 10:22)

### Tests affected

| Test | Status |
|------|--------|
| 16 tests in `foretias-server/src/calendar/mirror.rs` | DELETED (duplicates of client tests) |
| `foretias-server/tests/mirror_integration.rs` | Import path update needed |
| `foretias-client/src/calendar/mirror.rs` tests | UNCHANGED (canonical copy) |

### Blockers

- `latest_record()` must be added to client BEFORE deleting server (called from `handlers.rs:939,1308`)
- Server's `insert_and_get_mirrored` test has one extra assertion vs client — port it

---

## Phase 2: Byte-wrapper macro — `PublicKeyBytes` / `SignatureBytes`

**Risk:** LOW | **Lines removed:** ~130 | **Files changed:** 1

### Steps

- [x] Define `macro_rules! byte_vec_newtype` in `core-engine/src/foretias/types.rs`
      (2026-06-12 11:00)
- [x] Generate: `new`, `empty`, `from_slice`, `as_bytes`, `as_slice`, `len`, `is_empty`, `into_inner`, `Deref`, `DerefMut`, `AsRef`, `From<Vec<u8>>`, `From<[u8; N]>`, `Index` (4 variants), `IndexMut`
      (2026-06-12 11:00)
- [x] Apply to `PublicKeyBytes` and `SignatureBytes`
      (2026-06-12 11:00)
- [x] Keep `SignatureBytes`-specific impls outside macro: `Zeroize` (line 411), `From<FTByteVector>` (line 352)
      (2026-06-12 11:00)
- [x] Verify: `cargo test -p foretias-core`
      (2026-06-12 11:00)

### Tests affected

No direct unit tests for these types. Indirect exercised by:
- `core-engine/src/foretias/tick.rs` — `SignatureBytes::from()` in test builders
- `foretias-server/tests/dht_record_signature.rs` — `PublicKeyBytes::Ed25519` pattern match

### @Human concern

`PublicKeyBytes` lacks `Zeroize` and `From<FTByteVector>` (intentional — public keys are not secret). The macro should NOT add these to `PublicKeyBytes`. Keep them `SignatureBytes`-only.

---

## Phase 3: Signing algorithm macro — Dilithium / SPHINCS+

**Risk:** MEDIUM | **Lines removed:** ~170 | **Files changed:** 2

### Steps

- [x] Define `macro_rules! pq_signing_suite` parameterized by FFI function identifiers
      (2026-06-12 11:00)
- [x] The 9 functions (3 algorithms × 3 operations) have byte-identical bodies differing only in FFI symbol:
      (2026-06-12 11:00)
  - `foretias_dilithium3_keypair/sign/verify`
  - `foretias_sphincs_sha2_128s_keypair/sign/verify`
  - `foretias_sphincs_sha2_256f_keypair/sign/verify`
- [x] Apply macro for all 3 algorithms
      (2026-06-12 11:00)
- [x] Keep legacy `sphincs_keypair/sign/verify` aliases as thin wrappers to SHA2-128s
      (2026-06-12 11:00)
- [x] Verify: `cargo test -p foretias-core`
      (2026-06-12 11:00)

### Tests affected

No dedicated unit tests for individual signing functions. Exercised indirectly by:
- `core-engine/src/probity/report.rs:395-415` (sign/verify on ProbityReport)
- `core-engine/src/integration_tests.rs` (end-to-end signing)

### Blockers

- Rust stable doesn't support `concat_idents!`. Workaround: explicit function names passed to macro, or module-per-variant.

---

## Phase 4: Remove `TrustedInner` trait

**Risk:** LOW | **Lines removed:** ~60 | **Files changed:** 1

### Steps

- [x] Delete trait definition at `clean_auth.rs:90-94`
      (2026-06-12 10:25)
- [x] Delete `impl TrustedInner<T> for UnverifiedSignatureEnvelope<T>` at `clean_auth.rs:146-159`
      (2026-06-12 10:25)
- [x] Delete `impl TrustedInner<T> for CleanAuthenticated<T>` at `clean_auth.rs:300-313`
      (2026-06-12 10:25)
- [x] Delete `impl TrustedInner<T> for Externalized<T>` at `clean_auth.rs:398-408`
      (2026-06-12 10:25)
- [x] Grep for `TrustedInner` imports — remove any `use ...::TrustedInner`
      (2026-06-12 10:25)
- [x] Verify: `cargo test --workspace`
      (2026-06-12 10:26)

### Tests affected

**ZERO tests use TrustedInner as a trait bound.** All call sites use inherent methods (`from_trusted()`, `inner()`, `into_inner()`) which exist independently on each type.

Confirmed safe: grep for `: TrustedInner|dyn TrustedInner|where.*TrustedInner` returns zero results outside the trait's own definition/impls.

---

## Phase 5: `NoOpMutualAttest` — define once

**Risk:** LOW | **Lines removed:** ~16 | **Files changed:** 1

### Steps

- [x] Define `struct NoOpMutualAttest;` + `impl MutualAttestObserver` at module scope in `foretias-client/src/foretias.rs` (visibility: `pub(crate)`)
      (2026-06-12 10:28)
- [x] Remove 3 identical inline definitions at lines 173-180, 222-229, 510-517
      (2026-06-12 10:28)
- [x] Update all 3 usage sites to reference the module-level definition
      (2026-06-12 10:28)
- [x] Verify: `cargo test -p foretias-client`
      (2026-06-12 10:29)

### Tests affected

15 tests in `foretias-client/src/foretias.rs::tests` transitively exercise NoOpMutualAttest via `new()`, `from_persist()`, `create_standalone_state()`.

### @Human concern

`NoOpMutualAttest` is used ONLY for Standalone mode (single node, no peers). PtP/P2P modes use real `MutualAttestObserver` implementations (e.g., `NodeMetrics` on the server). The NoOp is correct for the thin client's standalone path.

---

## Phase 6: PeerAddr — add `From` conversion

**Risk:** LOW | **Lines changed:** ~10 | **Files changed:** 2

### Steps

- [x] Add `impl From<transport::PeerAddr> for callbacks::PeerAddr` in `foretias-server/src/communerd/transport.rs` (lossy: drops `peer_id` + `last_seen_ns`)
      (2026-06-12 11:00)
- [x] Replace manual conversion at `communerd/mod.rs:1496-1505` with `.into()`
      (2026-06-12 11:00)
- [x] Verify: `cargo test --workspace`
      (2026-06-12 11:00)

### Tests affected

| File | Notes |
|------|-------|
| `foretias-server/tests/mirror_integration.rs` | Uses both PeerAddr types |
| `foretias-server/tests/toppoli.rs:729` | Uses transport::PeerAddr |

### Import sites (for reference)

`callbacks::PeerAddr`: `communerd/mod.rs:34` (aliased as `CorePeerAddr`), `calendar/task_queue.rs:16`, `calendar/mod.rs:445`, `tests/mirror_integration.rs:19`
`transport::PeerAddr`: `communerd/tiers.rs:13`, `dht_peer_source.rs:11`, `libp2p_transport.rs:12`, `json_rpc_transport.rs:10`, `communerdette.rs:38`

### Blockers

None. No circular dependency (core-engine doesn't depend on foretias-server).

---

## Phase 7: TransportError — unify

**Risk:** MEDIUM | **Lines changed:** ~130+ | **Files changed:** 2 + 10 consumers

### Steps

- [x] Add `Unsupported(String)` variant to `core-engine/src/foretias/callbacks.rs:54` TransportError
      (2026-06-12 11:00)
- [x] Add `#[derive(thiserror::Error)]` to core TransportError (`thiserror = "2"` already in core-engine/Cargo.toml:32)
      (2026-06-12 11:00)
- [x] Replace `foretias-server/src/communerd/transport.rs:28` TransportError with `pub use foretias_core::foretias::callbacks::TransportError;`
      (2026-06-12 11:00)
- [x] Update ~130 call sites across: `communerd/mod.rs`, `tiers.rs`, `peer_pool.rs`, `swarm.rs`, `libp2p_transport.rs`, `json_rpc_transport.rs`, `communerdette.rs` + 5 test files
      (2026-06-12 11:00)
- [x] Verify: `cargo test --workspace`
      (2026-06-12 11:00)

### Tests affected

~130+ usages across `foretias-server/src/communerd/` and 5 test files. All mechanical import-path changes.

### Blockers

- `Unsupported` variant is missing from core — must add it (semantic change to core's public API)
- Existing `match` on core's `TransportError` in `communerd/mod.rs:1770-1815` (3 sites) need new `Unsupported` arm

### @Human concern

Per your feedback: keep all three error types nested with cause chains. `ForetiasError::Network(TransportError)` where `TransportError` carries `PtPError` as cause via `#[source]`. This phase only unifies the TWO `TransportError` definitions — `PtPError` and `ForetiasError` stay separate.

---

## Phase 8: Clean auth accessor macro

**Risk:** LOW | **Lines removed:** ~184 | **Files changed:** 1

### Steps

- [x] Define `macro_rules! field_accessors` generating per-field `&self.inner.FIELD` delegation
      (2026-06-12 11:00)
- [x] Apply to `UnverifiedSignatureEnvelope<ChrononRecord>` (10 accessors, lines 587-617)
      (2026-06-12 11:00)
- [x] Apply to `CleanAuthenticated<ChrononRecord>` (10 accessors, lines 677-707)
      (2026-06-12 11:00)
- [x] Apply to `UnverifiedSignatureEnvelope<ForetisRecord>` (6 accessors, lines 721-739)
      (2026-06-12 11:00)
- [x] Apply to `CleanAuthenticated<ForetisRecord>` (6 accessors, lines 876-893) — NOTE: 2 extra accessors (`signature_bytes`, `signature_algorithm`) access `self.signatures`, not `self.inner` — keep manual
      (2026-06-12 11:00)
- [x] Apply to `UnverifiedSignatureEnvelope<EpochSnapshotRecord>` (8 accessors, lines 926-950)
      (2026-06-12 11:00)
- [x] Apply to `CleanAuthenticated<EpochSnapshotRecord>` (8 accessors, lines 975-999)
      (2026-06-12 11:00)
- [x] Verify: `cargo test -p foretias-core`
      (2026-06-12 11:00)

### Tests affected

389 call sites use these accessors. API is unchanged — all method names stay the same. No test modifications needed.

### @Human concern

`#![feature(fn_delegation)]` is nightly-only. The project targets stable Rust. `macro_rules!` is the correct stable-Rust approach. The passthrough accessors are kept for easy ergonomic access — the macro just eliminates the copy-paste.

---

## Phase 9: Config defaults — extract shared constants

**Risk:** LOW | **Lines removed:** ~10 | **Files changed:** 4

### Steps

- [x] Analyzed: only 2 of 7 defaults are truly identical (`listen_addr`, `auto_attest_every_n`). ROI too low for extraction. Skipping.
      (2026-06-12 10:30)

### Tests affected

Indirect only — config defaults are tested through serialization/deserialization roundtrips.

### @Human concern

Only 2 of 7 defaults are truly identical across the 3 config files. The rest are intentionally different deployment profiles:
- `p2p_port_range`: `[9900,9999]` (node/p2p) vs `[4002,4999]` (time_family)
- `request_timeout_secs`: `5` (node/p2p) vs `15` (time_family)
- `dht_namespace`: `"mainnet"` (node/p2p) vs `"foretias"` (time_family)
- `max_discovered_peers`: `13` (node/p2p) vs `100` (time_family)

Config fields should use descriptive names (`p2p_port_range`) with named constants (`DEFAULT_PORT_RANGE`). No `default_` prefix in field names.

---

## Phase 10: Standalone state creation dedup

**Risk:** LOW | **Lines removed:** ~30 | **Files changed:** 1

### Steps

- [x] Refactor `Foretias::new()` (lines 163-201) to delegate to `create_standalone_state()` (lines 501-530)
      (2026-06-12 11:00)
- [x] `from_persist()` CANNOT delegate (uses `Chronomatter::from_calendar` + `Calendar::from_persisted` — different path)
      (2026-06-12 11:00)
- [x] Verify: `cargo test -p foretias-client`
      (2026-06-12 11:00)

### Tests affected

11 tests in `foretias-client/src/foretias.rs::tests` — all use `Foretias::new()`.

### @Human concern

**Possible bug found:** Both `new()` and `create_standalone_state()` call `crypto_server::new_software()` TWICE — once for Chronomatter creation, once stored in `StandaloneState`. Is this intentional? The dedup is the right time to investigate.

---

## Phase 11: Tier hierarchy — KEEP (no changes)

**Risk:** NONE | **Lines removed:** 0 | **Files changed:** 0

### Decision

Per @Human: Keep the tier hierarchy (`CommunerdServer` → `CommunerdP2P` → `Communerd`). The Server is supposed to stick around and just answer stamp/verify without any P2P discovery. It should be able to perform FB/GNF attestations as configured but not over P2P discovery. The tiers are currently pass-through but establish the correct abstraction boundary for future differentiation.

### Steps

- [x] No code changes needed. Document the design intent in `tiers.rs` doc comments.
      (2026-06-12 11:00)

---

## Phase 12: `PublicKeyBytes` name collision

**Risk:** LOW-MEDIUM | **Occurrence count:** 56 across 12 files | **Files changed:** 12

### Steps

- [x] Rename `crypto_server::PublicKeyBytes` enum → `CryptoPublicKey` in `core-engine/src/crypto_server/mod.rs:32`
      (2026-06-12 11:00)
- [x] Update 30 pattern match sites across 10 files (all are `Ed25519(pk) => pk.bytes.to_vec()`)
      (2026-06-12 11:00)
- [x] Verify: `cargo test --workspace`
      (2026-06-12 11:00)

### Files to update

| File | Occurrences |
|------|------------|
| `core-engine/src/crypto_server/software.rs` | 2 |
| `core-engine/src/foretias/tick.rs` | 6 |
| `core-engine/src/probity/report.rs` | 1 |
| `core-engine/src/integration_tests.rs` | 4 |
| `foretias-server/src/communerd/mod.rs` | 1 |
| `foretias-server/src/communerd/communerdette.rs` | 10 |
| `foretias-server/src/communerd/p2p/tbid_handshake.rs` | 2 |
| `foretias-server/src/probity/gossip_handler.rs` | 3 |
| `foretias-server/tests/dht_record_signature.rs` | 1 |

### Tests affected

`foretias-server/tests/dht_record_signature.rs` — uses `PublicKeyBytes::Ed25519(pk) => pk.bytes`

---

## Phase 13: Naming consistency

**Risk:** HIGH (get_tbid: 311+ occurrences) | **Files changed:** 30+

### Steps

- [x] Rename `get_tbid()` → `tbid()` — **311+ occurrences across ~30 files** (largest single rename)
      (2026-06-12 11:00)
- [x] Rename `get_tbn()` → `tbn()` — 11 occurrences, 6 files
      (2026-06-12 11:00)
- [x] Rename `get_peers()` → `known_peers()` — 22 occurrences, 4 files
      (2026-06-12 11:00)
- [x] Rename `CommunerdetteLine::get_tick()` → `get_chronon()` — 5 occurrences, 1 file
      (2026-06-12 11:00)
- [x] Rename `ParseError` → `CleanAuthParseError` — 25 occurrences, 2 files
      (2026-06-12 11:00)
- [x] Rename `read_message` → `load_message` — 4 refs, 1 file (main.rs)
      (2026-06-12 11:00)
- [x] Rename `read_foretis` → `load_foretis` — 3 refs, 1 file (main.rs)
      (2026-06-12 11:00)
- [x] Document accessor naming convention in `rust_instructions.md`: bare `fn name()` for in-memory, `get_` prefix for cross-time-being/communerd calls
      (2026-06-12 11:00)
- [x] Verify: `cargo test --workspace && cargo clippy --workspace --all-targets`
      (2026-06-12 11:00)

### Convention (from @Human)

- Bare `fn name()` — in-memory accessors (e.g., `tbid()`, `tbn()`, `calendar()`)
- `get_` prefix — functions that reach across time-being or through Communerd (e.g., `get_chronon()`, `get_peers()`)

### Tests affected

Virtually every integration test calls `get_tbid()`. Top files:
- `communerdette.rs` ~80 occurrences
- `handlers.rs` ~15
- `toppoli.rs` ~16
- `communerd/mod.rs` ~10
- `server/mod.rs` ~5

### @Human concern

`get_tbid` at 311+ occurrences is the single largest rename. Consider batching with another change touching the same files to reduce churn. All other renames are small and safe.

---

## Phase 14: Defer — JSON-RPC param extraction

**Risk:** LOW | **Occurrence count:** 54 `params.get()` calls | **Files changed:** 1

### Steps

- [ ] Define helpers in `foretias-server/src/server/handlers.rs`:
  - `fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String>` — covers 12 sites
  - `fn required_u64(params: &Value, key: &str) -> Result<u64, String>` — covers 8 sites
  - `fn optional_str<'a>(params: &'a Value, key: &str) -> Option<&'a str>` — covers ~5 sites
  - `fn optional_u64(params: &Value, key: &str, default: u64) -> u64` — covers ~3 sites
  - `fn extract_id(params: &Value) -> Option<Value>` — covers all 33 handlers
- [ ] Migrate handlers incrementally (one at a time)
- [ ] Also unify error return style: `resp_error(server, id, code, msg)` vs `JsonRpcResponse::error(id, code, msg)` — currently inconsistent
- [ ] Verify: `cargo test -p foretias-server`

### Tests affected

- `handlers.rs:2485+` inline `#[cfg(test)]` module
- `foretias-server/tests/integration.rs`
- `foretias-server/tests/toppoli.rs`
- `foretias-server/tests/mirror_integration.rs`

### @Human concern

Two error-return styles exist in handlers.rs: `resp_error(server, id, code, msg)` and `jsonrpc::JsonRpcResponse::error(id, code, msg)`. This refactor is a natural time to unify them.

---

## Phase 15: Create Calendar Refactoring spec

- [ ] Create `specs/CALENDAR_REFACTORING_SPEC.md` describing the 5-component Calendar architecture:
  - Calendar API in core-engine (store/retrieve, FB, GNF)
  - Calendar Storage API in core-engine (storage/search interface)
  - Calendar API implementation in core-engine (logic only; receives handles: CommunerdetteLine or CommunerdLine, Storage API, thread pools)
  - In-memory storage in core-engine
  - File-system storage in foretias-server
- [ ] Include TBIDRecord concept (from N3) — stored, transmitted, participates in verification chains
- [ ] Include MirrorStore consolidation (from N6) — move to core-engine
- [ ] Include ProbityStore logic extraction (from N5) — move core storage to server
- [ ] Include MirrorDispatcher refactoring (from N1) — return TBIDs, not raw addresses
- [ ] Include PeerAddr leakage fix (from N1) — Calendar shouldn't see raw RPC connector strings
- [ ] Create `specs/CALENDAR_REFACTORING_PLAN.md` with implementation phases

---

## Phase 16: Create Error Architecture spec

- [ ] Create `specs/ERROR_ARCHITECTURE_SPEC.md` describing:
  - Nested cause chains: PtPError → TransportError → ForetiasError via `#[source]`
  - `NodeError` rename evaluation (to `CoreError`) with domain variants
  - `Clock(String)` variant addition to NodeError
  - Guidelines for avoiding recursive error wrapping
  - Per-component error types (CalendarError, ChronomatterError) as `From`-convertible into NodeError
- [ ] Create `specs/ERROR_ARCHITECTURE_PLAN.md` with implementation phases

---

## Phase 17: fn_delegation tracking task

- [ ] Add entry to `specs/INDEX.md` under Tier 5 (Deferred): `fn_delegation` nightly feature tracking
- [ ] Add section to `rust_language_development_upkeep.md` documenting:
  - Current approach: `macro_rules!` for passthrough accessors (stable Rust)
  - Target: `#![feature(fn_delegation)]` when it stabilizes
  - Evaluation criteria: readability, correctness, efficiency vs macro approach
  - Tracking link: Rust tracking issue for `fn_delegation`

---

## Phase ordering

```
Phase 1  (MirrorStore)           ─── simple, zero deps ──────────────┐
Phase 2  (byte_wrapper macro)    ─── simple, zero deps ──────────────┤
Phase 3  (signing macro)         ─── medium, zero deps ──────────────┤
Phase 4  (TrustedInner removal)  ─── simple, zero deps ──────────────┤
Phase 5  (NoOpMutualAttest)      ─── simple, zero deps ──────────────┤
Phase 9  (config defaults)       ─── simple, zero deps ──────────────┤
Phase 12 (PublicKeyBytes rename) ─── simple, zero deps ──────────────┘
                                                                PARALLEL
Phase 6  (PeerAddr From)         ─── medium, zero deps ──────────────┐
Phase 7  (TransportError unify)  ─── medium, after Phase 6 ──────────┤
Phase 10 (standalone state)      ─── simple, after Phase 5 ──────────┤
Phase 8  (clean_auth accessors)  ─── medium, after Phase 4 ──────────┘
                                                                SEQUENTIAL
Phase 11 (tier hierarchy)        ─── doc-only, no code changes ──────┐
Phase 13 (naming consistency)    ─── high-touch, after all above ─────┤
Phase 14 (JSON-RPC helpers)      ─── deferred ────────────────────────┘
                                                                SPECS
Phase 15 (create Calendar Refactoring spec)  ─── planning ───────────┐
Phase 16 (create Error Architecture spec)    ─── planning ───────────┤
Phase 17 (fn_delegation tracking task)       ─── doc-only ───────────┘
```

## Summary

| Phase | Risk | Lines removed | Files | Tests impacted | Effort |
|-------|------|---------------|-------|----------------|--------|
| 1. MirrorStore | LOW | ~281 | 5-6 | 16 deleted + 1 path update | 30 min |
| 2. byte_wrapper | LOW | ~234 | 1 | Indirect only | 30 min |
| 3. signing macro | MEDIUM | ~188 | 2 | Indirect only | 1 hr |
| 4. TrustedInner | LOW | ~53 | 1 | ZERO | 15 min |
| 5. NoOpMutualAttest | LOW | ~18 | 1 | 15 transitive | 15 min |
| 6. PeerAddr From | LOW | ~9 | 2 | 2 test files | 20 min |
| 7. TransportError | MEDIUM | ~17 | 2+10 | 5+ test files | 1.5 hr |
| 8. clean_auth macro | LOW | ~144 | 1 | 389 call sites (API unchanged) | 1 hr |
| 9. config defaults | LOW | 0 (skipped) | 4 | Indirect only | 20 min |
| 10. standalone dedup | LOW | ~25 | 1 | 11 tests | 20 min |
| 11. tier hierarchy | NONE | 0 (doc-only) | 0 | ZERO | 5 min |
| 12. PublicKeyBytes rename | LOW-MEDIUM | ~42 (rename) | 12 | 1 test file | 1 hr |
| 13. naming consistency | HIGH | 0 (rename) | 30+ | All integration tests | 3 hr |
| 14. JSON-RPC helpers | LOW | ~200+ | 1 | 4 test files | 2 hr |
| 15. Calendar Refactoring spec | NONE | 0 | 0 | ZERO | 2 hr |
| 16. Error Architecture spec | NONE | 0 | 0 | ZERO | 1 hr |
| 17. fn_delegation tracking | NONE | 0 | 2 | ZERO | 15 min |
| **Total** | | **~1,011+** | **~40** | | **~16 hr** |

---

## Worktree Closeout

- [x] Verify all work is complete in `${HOME}/tmp/foretias-worktrees/REDUNDANCY_ELIMINATION_PLAN_4821` and committed to `redundancy-elimination`
      (2026-06-12 11:15)
- [x] Merge `redundancy-elimination` to alpha
      (2026-06-12 11:15)
