# Redundancy Elimination Specification

**Generated:** 2026-06-12
**Scope:** `p2p/core-engine/`, `p2p/foretias-client/`, `p2p/foretias-server/`
**Constraint:** 593+ tests must continue to pass. No behavioral changes. Preserve type-enforced trust boundary and signing boundary.

---

## 1. Overview

A comprehensive code review identified 48 redundancy findings across three categories:

| Category | Findings | Estimated lines |
|----------|----------|----------------|
| Naming redundancy | 11 | (cosmetic, no line removal) |
| Mechanism redundancy | 12 | ~500 |
| Code redundancy | 14 | ~970 |
| **Total** | **37 unique** | **~1,470** |

Some findings span multiple categories (e.g., `PeerAddr` is both a naming and mechanism issue). After deduplication, there are **37 unique findings**.

---

## 2. Naming Redundancy

### 2A. Same-name types across crates (HIGH severity)

**Finding N1: `PeerAddr` — 2 definitions**

- **Definition A:** `core-engine/src/foretias/callbacks.rs:26` — `struct PeerAddr { json_rpc: String }`
(@Human: CalendarTask uses `callbacks::PeerAddr` with a raw `json_rpc: String`, but Calendar itself targets TBIDs — the PeerAddr comes from `MirrorDispatcher::known_peers()` which returns transport-level addresses. Calendar shouldn't deal with raw RPC connector strings; that complexity belongs in Communerd(ette). The current design leaks transport details into Calendar via the `MirrorDispatcher` trait return type. This should be refactored so `MirrorDispatcher` returns TBIDs and Communerd handles transport selection internally. This is a deeper architectural issue — flagging for the Calendar refactoring spec (see N4).)

- **Definition B:** `foretias-server/src/communerd/transport.rs:10` — `struct PeerAddr { json_rpc: String, peer_id: Option<PeerId>, last_seen_ns: u64 }`
- **What it is:** A peer's network address. Core version is minimal (callback trait surface). Server version adds libp2p identity and liveness.
- **Used by:** Definition A in `PeerMessenger`, `CalendarTask`, `MirrorDispatcher`. Definition B in `Communerd`, `PeerPool`, all transports, `CommunerdetteLine`.
- **Differences:** Server has 2 extra fields (`peer_id`, `last_seen_ns`). Manual conversion at `communerd/mod.rs:1496-1505`.
- **Recommendation:** For now: add `From` conversion. Long-term: refactor `MirrorDispatcher` to return TBIDs, not raw addresses (Calendar refactoring spec).

**Finding N2: `TransportError` — 2 definitions**

- **Definition A:** `core-engine/src/foretias/callbacks.rs:54` — 4 variants (Connect, Rpc, Timeout, Decode)
- **Definition B:** `foretias-server/src/communerd/transport.rs:28` — 5 variants (same + Unsupported), uses `thiserror`
- **What it is:** Error enum for transport failures. Core version used only by `PeerMessenger` trait (appears dead). Server version actively used.
- **Recommendation:** Core version + `PeerMessenger` may be dead code. Verify and remove if unused. Otherwise rename to `CallbackTransportError`.

**Finding N3: `PublicKeyBytes` — 2 definitions in SAME crate (HIGH)**

- **Definition A:** `core-engine/src/crypto_server/mod.rs:32` — `enum PublicKeyBytes { Ed25519(32), P256Compressed(33) }`
- **Definition B:** `core-engine/src/foretias/types.rs:158` — `struct PublicKeyBytes(Vec<u8>)`
(@Human: The crypto_server enum should be made private — downstream code (communerd, chronomatter, calendar) only needs a blind handle, not algorithm-tagged access. The `PublicKeyBytes` struct in types.rs is the correct public API. Additionally, propose a `TBIDRecord` concept — since TBID data is stored (by calendar), transmitted (by all), participates in verification/authentication/signing chains, and can be embedded in other records, it deserves a proper Record type. Include in Calendar refactoring spec.)

- **What it is:** Definition A is algorithm-tagged fixed-size keys from crypto backend. Definition B is variable-length byte container for wire serialization.
- **Used by:** Both imported in `tbid_handshake.rs` requiring fully-qualified paths.
- **Recommendation:** Make crypto_server enum private. Keep `PublicKeyBytes` struct as public API. Create `TBIDRecord` in the Calendar refactoring spec.

**Finding N4: `Calendar` — 3 definitions**

- **Definition A:** `core-engine/src/foretias/calendar.rs:11` — Core data store (tbid, tbn, ticks)
- **Definition B:** `foretias-client/src/calendar/mod.rs:19` — `Arc<RwLock<CoreCalendar>>` + TickObserver
- **Definition C:** `foretias-server/src/calendar/mod.rs:51` — Adds signing_key, task_tx, worker_pool, mirror_state
- **What it is:** Three-tier layered architecture (data / thin-client / server-orchestrator).
(@Human: This warrants a dedicated refactoring spec. The vision is a 5-component Calendar architecture:
1. **Calendar API** in core-engine — store/retrieve chronon records, produce FB content, implement GNF logic
2. **Calendar Storage API** in core-engine — the storage/search interface the Calendar API depends on
3. **Calendar API implementation** in core-engine — just the logic. The API expects to be given handles such as `CommunerdetteLine` (or a `CommunerdLine` — a constricted set of Communerd API's for finding peers for mirroring?), and Storage API, as well as thread management pools for await/async calls. This way, CLI and Server may all startup calendars configured with different performance characteristics and lifetimes. Also allows expanding Calendar behavior in the future.
4. **In-memory storage** in core-engine — first Calendar Storage API implementation
5. **File-system storage** in foretias-server — implemented after in-memory is tested

This mirrors how crypto_server provides fundamentals. Items and transmitted data should be upgraded to Records. Also includes: TBIDRecord (N3), MirrorStore consolidation (N6), ProbityStore logic extraction (N5).)
- **Recommendation:** OUT OF SCOPE for this redundancy cleanup. Requires dedicated Calendar Refactoring spec.

**Finding N5: `ProbityStore` — 2 definitions**

- **Definition A:** `core-engine/src/probity/store.rs:7` — Simple `HashMap<String, f32>` score store
- **Definition B:** `foretias-server/src/probity/store.rs:10` — Full report ingestion, dedup, U-shape aggregation
- **What it is:** Core version is a primitive score cache for epoch committee. Server version is the real probity system.
(@Human: Core-engine should be logic-only, no storage. The score store should move to Communerd/server. Include in Calendar refactoring spec.)
- **Recommendation:** Move core ProbityStore to server. Core-engine provides only the probity logic/algorithms.

**Finding N6: `MirrorStore` — 2 identical copies (HIGH)**
(@Human: Include MirrorStore consolidation in the Calendar refactoring spec as item #6. For now, merge `latest_record()` into client copy and delete server copy as an interim step.)

- **Definition A:** `foretias-client/src/calendar/mirror.rs:11` — 261 lines
- **Definition B:** `foretias-server/src/calendar/mirror.rs:11` — 276 lines
- **What it is:** Stores replicated chronon records for mirrored TBIDs. Struct, fields, methods, and tests are byte-for-byte identical.
- **Differences:** Server has one extra method (`latest_record()`) and a few extra test assertions.
- **Recommendation:** Interim: merge `latest_record()` into client copy, delete server copy. Long-term: include in Calendar refactoring spec.

### 2B. Inconsistent accessor naming (MEDIUM severity)

**Finding N7: `get_X` vs bare `X` accessor inconsistency**
(@Human: Convention adopted. Use bare `fn name()` for in-memory accessors. Use `get_` prefix ONLY when the function reaches across a time-being boundary or through Communerd (network/disk I/O). Document this in `rust_instructions.md`.)

- `get_tbid()` / `get_tbn()` on `TimeFamilyServer` and `Chronomatter` vs bare `tbid()` / `tbn()` on `Calendar`
- `get_peers()` on `CommunerdServer`, `CommunerdP2P`, `PeerPool`, `Communerd` vs `known_peers()` on `MirrorDispatcher`
- **Recommendation:** Apply convention: `tbid()`, `tbn()` (in-memory), `get_peers()` stays (crosses communerd), `known_peers()` on MirrorDispatcher → `get_known_peers()` (crosses communerd).

**Finding N8: `get_tick` vs `get_chronon` — same concept, different names**
(@Human: Same convention as N7. `CommunerdetteLine::get_tick()` crosses communerd, so `get_` prefix is appropriate. Rename to `get_chronon()` for consistency with `ChrononRecord`.)

- `CommunerdetteLine::get_tick()` (network layer) vs `CalendarStore::get_chronon()` (storage layer)
- Both look up a single `ChrononRecord` by number.
- **Recommendation:** Rename `get_tick()` → `get_chronon()` (crosses communerd, keeps `get_` prefix).

### 2C. Other naming issues (LOW severity)

**Finding N9: `read_message` / `read_foretis` — should be `load_`**

- `main.rs:248,263` — Functions that load from file or pass through string args.
- **Recommendation:** Rename to `load_message` / `load_foretis`.

**Finding N10: `ParseError` — overly generic name**

- `core-engine/src/foretias/clean_auth.rs:556` — `enum ParseError { InvalidJson, TruncatedBytes, InvalidLength, BadFormat, UnknownAlgorithm }`
- Scope is limited to trust-boundary envelope parsing.
- **Recommendation:** Rename to `CleanAuthParseError`.

**Finding N11: `sign_tbid_message` — NOT deprecated**
(@Human: The single-line passthroughs on `TimeFamilyServer` exist to expose Chronomatter's signing capability to the server's JSON-RPC handlers without giving handlers direct access to Chronomatter. `sign_tbid_message` specifically is used by `handle_channel_bind_challenge` and `handle_authenticated_ping` for dual-key TBID signing (Ed25519 + SLH-DSA). These passthroughs are intentional delegation — they keep the server API surface clean.)

- `foretias-server/src/server/mod.rs:196` — delegates to `Chronomatter::sign_tbid_message()`
- **Actively used** at `handlers.rs:526` (`handle_channel_bind_challenge`) and `handlers.rs:577` (`handle_authenticated_ping`).
- **Recommendation:** DO NOT remove. Passthroughs are intentional delegation pattern.

---

## 3. Mechanism Redundancy

### HIGH severity

**Finding M1: `TrustedInner` trait — redundant with inherent methods**

- `core-engine/src/foretias/clean_auth.rs:90-94` — trait with `from_trusted`, `inner`, `into_inner`
- Implemented on `UnverifiedSignatureEnvelope<T>`, `CleanAuthenticated<T>`, `Externalized<T>`
- **All three types already have identical inherent methods.** File comment: "retained for backward compat -- inherent methods are preferred"
- **Zero usage as a trait bound** anywhere in the workspace.
- **Recommendation:** Delete the trait and all impl blocks. Verify tests still pass.

**Finding M2: `NoOpMutualAttest` — defined 3 times inline**

- `foretias-client/src/foretias.rs:173, 222, 510` — identical struct + 3 empty trait methods
- **Recommendation:** Define once at module scope. Verify tests still pass.

**Finding M3: Field accessor duplication on trust-boundary wrappers**
(@Human: Keep the passthrough accessors — they provide easy ergonomic access. Use a macro to generate them from a single definition. The `#![feature(fn_delegation)]` nightly feature was never adopted because the project targets stable Rust. A `macro_rules!` approach is the correct stable-Rust solution.)
(@Human: Add a detailed explanation of this concern in `rust_language_development_upkeep.md` and `INDEX.md` — a continuing task to track up-to-date methods of passing through methods to maximize readability, correctness and efficiency.)

- For each of 3 domain types (ChrononRecord, ForetisRecord, EpochSnapshotRecord), field accessors are duplicated on both `UnverifiedSignatureEnvelope<T>` and `CleanAuthenticated<T>`.
- 48 accessor methods where 24 are exact copies.
- **Recommendation:** `field_accessors!` macro to generate passthrough accessors for both wrappers.

### MEDIUM severity

**Finding M4: `NodeConfig` vs `TimeFamilyCliConfig` vs `TimeFamilyConfig` — triple config overlap**
(@Human: Rename `NodeConfig` → `P2PNodeConfig`. `TimeFamilyCliConfig` should contain a `TimeFamilyConfig` field — CLI overrides go on the outer struct, canonical config lives in the inner. Document any ignored fields in `TimeFamilyCliConfig` doc comments.)
(@Human: Does P2PNodeConfig represent the whole configuration for a fully capable P2P foretias server? If so it would also contain a TimeFamilyConfig, right?)
(@Agent: Yes. `P2PNodeConfig` should be the top-level config for a fully capable P2P server. It would contain a `TimeFamilyConfig` field (the canonical nested config) plus any server-specific fields that don't fit the TimeFamily model (e.g., `persist_path`, `dormant` mode). `TimeFamilyCliConfig` is the CLI bridge — it has flat fields that map to `TimeFamilyConfig` sub-fields via `From`/`Into`. The hierarchy: `P2PNodeConfig` (top-level, contains `TimeFamilyConfig`) → `TimeFamilyConfig` (canonical nested: chronomatter + calendars + communerd + logging) → `TimeFamilyCliConfig` (CLI flat override layer, contains `TimeFamilyConfig`).)

- `NodeConfig` (16 fields) and `TimeFamilyCliConfig` (13 fields) have 12 overlapping fields.
- `TimeFamilyConfig` is the canonical nested structure.
- **Recommendation:** Rename `NodeConfig` → `P2PNodeConfig`. Make `P2PNodeConfig` contain `TimeFamilyConfig`. Make `TimeFamilyCliConfig` contain `TimeFamilyConfig`.

**Finding M5: `CommunerdServer` / `CommunerdP2P` — tier wrappers**
(@Human: Keep the hierarchy. The Server is supposed to stick around and just answer stamp/verify without any P2P discovery. It should be able to perform FB/GNF attestations as configured but not over P2P discovery. Unless we eventually add different implementations on the server and the P2P, keep the tier structure.)

- `foretias-server/src/communerd/tiers.rs` — Every method currently delegates to `Communerd` with zero added logic.
- `CommunerdP2P` has 3-hop delegation (P2P → Server → Communerd).
- **Recommendation:** KEEP the tier hierarchy. The design intent is that `CommunerdServer` answers stamp/verify without P2P discovery, while `CommunerdP2P` adds P2P capabilities. Currently the tiers are pass-through, but they establish the correct abstraction boundary for future differentiation. No changes needed for now.

**Finding M6: `PtPError` vs `TransportError` vs `ForetiasError` — 3 overlapping error types**
(@Human: Nesting makes sense. `ForetiasError::Network(TransportError)` where `TransportError` can carry a `PtPError` as its cause. Each layer appends context as errors escalate. This gives useful stack traces: PtPError("noise handshake failed") → TransportError("connect to peer X failed: [PtPError]") → ForetiasError("network: [TransportError]"). Implement via `#[source]` or `.cause()` chaining.)

- `PtPError` (client noise layer) and `TransportError` (server transport) share 4 of 5 variant concepts.
- `ForetiasError` absorbs both via `From` impls.
- **Recommendation:** Keep all three. Add `cause: Option<Box<PtPError>>` to `TransportError`. Use `#[source]` for error chain escalation.

**Finding M7: `ClientLevel` / `ForetiasInner` / `ForetiasConfig` — triple containment modeling**

- Three enums with identical `Standalone`/`Ptp`/`P2p` variants.
- **Recommendation:** `ClientLevel` can be derived from `ForetiasInner`. Consider eliminating `ClientLevel`.

**Finding M8: Divergent `default_*` values across config files (BUG VECTOR)**
(@Human: Configs should be flat and self-documenting. Settings use descriptive names (`p2p_port_range`) that default to named constants (`DEFAULT_PORT_RANGE`). No `default_` prefix in config field names. The chosen value (`p2p_port`) is selected from the range if not specified. Each config owns its own defaults — do NOT merge intentionally different values.)

- `default_request_timeout_secs`: 5 in node.rs/p2p.rs, **15** in time_family.rs
- `default_p2p_port_range`: `[9900,9999]` in node.rs/p2p.rs, **`[4002,4999]`** in time_family.rs
- `default_dht_namespace`: `"mainnet"` in node.rs/p2p.rs, **`"foretias"`** in time_family.rs
- `default_max_discovered_peers`: 13 in node.rs/p2p.rs, **100** in time_family.rs
- **Recommendation:** Each config defines its own `DEFAULT_*` constants. Rename fields to drop `default_` prefix. Document intentional divergences.

### LOW severity

**Finding M9: `ClockError` standalone**
(@Human: `NodeError` is not obvious — it's the core-engine error type. Keep `NodeError` in core-engine (it's the crate-level error), add `Clock(String)` variant, and add `From<ClockError>` conversion.)
(@Human: Why not just `CoreError`? Added to a collection of dedicated `CalendarError`, `ChronomatterError`, and they can all hold other errors as cause...? I guess recursive error production could be a problem huh?)
(@Agent: A `CoreError` with nested `CalendarError`/`ChronomatterError` is architecturally clean, but yes — recursive error wrapping is a real risk. If `CalendarError` contains `CoreError` and `CoreError` contains `CalendarError`, you get infinite recursion in `Display`/`Debug`. The safe pattern is: one top-level error type (`CoreError` or `NodeError`) with domain-specific variants (`Calendar(String)`, `Chronomatter(String)`, `Clock(String)`, `Crypto(String)`), each carrying a message string rather than a nested error. This is what `NodeError` already does with its 16 variants. Adding `Clock(String)` is the minimal safe change. A full rename to `CoreError` is a separate refactor.)

- `core-engine/src/clock.rs:12` — `struct ClockError(pub String)`, not integrated with `NodeError`.
- **Recommendation:** Add `Clock(String)` variant to `NodeError`. Add `From<ClockError> for NodeError`. Defer rename to `CoreError` as a separate concern.

**Finding M10: `CalendarLookup` — 2 single-record wrappers**

- `CalendarLookupFromCleanRecord` (clean_auth.rs:860) and `FetchedCalendar` (foretias.rs:632) both wrap a single `ChrononRecord` for `CalendarLookup` trait.
- **Recommendation:** Create shared `SingleRecordCalendar` in core-engine.

**Finding M11: Chronomatter + Calendar init duplicated 4 places**

- ~15 lines of shared initialization (create Calendar, create CryptoServer, create Chronomatter, set observer, sync TBID/TBN) in 4 locations.
- **Recommendation:** `create_standalone_state()` should be single source of truth. Initialize on the struct.

**Finding M12: JSON-RPC request construction duplicated 3 places**

- `serde_json::json!({"jsonrpc": "2.0", "method": ..., "params": ..., "id": 1})` identical in 3 transport files.
- **Recommendation:** Extract `jsonrpc_request()` helper.

---

## 4. Code Redundancy

### Highest impact (~190+ lines each)

**Finding C1: Signing algorithm triple duplication (~190 lines)**

- `signing_dilithium.rs`, `signing_sphincs.rs` (SHA2-128s), `signing_sphincs.rs` (SHA2-256f)
- Three `keypair()`/`sign()`/`verify()` functions with identical bodies, differing only in FFI function name called.
- **Recommendation:** `macro_rules!` parameterized by FFI function identifiers. Keep readable.

**Finding C2: `PublicKeyBytes` vs `SignatureBytes` (~130 lines)**

- `core-engine/src/foretias/types.rs` — identical newtype wrappers over `Vec<u8>` with 8 methods + 10 trait impls.
- **Recommendation:** `byte_vec_newtype!` macro.

**Finding C3: Clean auth field accessor duplication (~184 lines)**

- 3 domain types × 2 wrappers × ~8 accessors each = 48 methods, 24 exact copies.
- **Recommendation:** `field_accessors!` macro to generate passthrough accessors.

**Finding C4: JSON-RPC param extraction (~200+ occurrences)**
(@Human: The `require_str()`/`require_u64()` helper approach is the most ergonomic for stable Rust.)

- `handlers.rs` — `params.get("field").and_then(|v| v.as_type())` repeated 200+ times.
- **Recommendation:** `require_str()` / `require_u64()` / `optional_str()` / `optional_u64()` helpers.

### High impact (~40-60 lines each)

**Finding C5: `SoftwareCryptoServer` generate/from_seed (~40 lines)**

- `software.rs` — identical PQC keypair generation + struct construction in both methods.
- **Recommendation:** Extract `from_handle(handle: PrivKeyHandle)` helper.

**Finding C6: `handle_stamp_my_chronon` vs `handle_stamp_my_chronon_block` (~50 lines)**

- `handlers.rs` — identical validation/enqueue logic, differing only in field name and task variant.
- **Recommendation:** Extract `parse_and_enqueue_attestation()` helper.

**Finding C7: Chain verification logic (~50 lines)**

- `handlers.rs` — `verify_ship_ack_chain()` and `verify_history_dump_chunk()` share verification loop.
- **Recommendation:** Extract `chain_verify_records()` with resolver closure.

**Finding C8: Standalone state creation 3× (~60 lines)**

- `foretias-client/src/foretias.rs` — `new()`, `from_persist()`, `create_standalone_state()` share init code.
- **Recommendation:** `new()` should delegate to `create_standalone_state()`.

### Medium impact (~15-36 lines each)

**Finding C9: Noise handshake pair (~36 lines)**

- `noise.rs` — `noise_handshake()` and `noise_handshake_with_handle()` differ only in session construction.
- **Recommendation:** Extract `run_handshake_steps()` helper.

**Finding C10: NoOpMutualAttest 3× (~24 lines)**
(@Human: `NoOpMutualAttest` exists for Standalone mode — a single-node foretias instance with no peers to attest with. PtP and P2P modes use real `MutualAttestObserver` implementations (e.g., `NodeMetrics` on the server). The NoOp is correct for the thin client's standalone path.)

- `foretias-client/src/foretias.rs` — identical 8-line struct+impl defined 3 times.
- **Recommendation:** Define once at module scope.

**Finding C11: TBID secret reconstruction (~14 lines)**

- `signing_tbid.rs` and `types.rs` — identical 240-byte layout reconstruction.
- **Recommendation:** Shared `reconstruct_tbid_secret()` helper.

**Finding C12: First-record resolution (~15 lines)**

- `handlers.rs` — `verify_stream_tick_record` and `verify_history_dump_chunk` share predecessor resolution.
- **Recommendation:** Extract `resolve_predecessor()` helper.

**Finding C13: `JsonRpcResponse` constructors (~44 lines)**

- `jsonrpc.rs` — 4 constructors with identical structure, differing only in option fields and dormant flag.
- **Recommendation:** Use macro to generate constructors.

**Finding C14: Config builder methods (~30 lines)**

- `foretias-client/src/config.rs` — `with_chronon()`/`with_persist_path()` duplicated across containment configs.
- **Recommendation:** Low ROI; inherent to containment pattern.

---

## 5. Additional Findings (from implementation analysis)

### 5A. Tier wrappers — keep hierarchy

`CommunerdServer` and `CommunerdP2P` in `foretias-server/src/communerd/tiers.rs` are currently pass-through, but the hierarchy is intentional: `CommunerdServer` answers stamp/verify without P2P discovery; `CommunerdP2P` adds P2P capabilities. Keep the tier structure for future differentiation.

### 5B. Double crypto instantiation in standalone state

Both `Foretias::new()` and `create_standalone_state()` call `crypto_server::new_software()` TWICE — once for Chronomatter creation, once stored in `StandaloneState`. This may be a waste or a bug. Phase 10 is the right time to investigate.

### 5C. `handlers.rs` has two inconsistent error-return styles

Two patterns coexist:
- `resp_error(server, id, INVALID_PARAMS, "missing 'X'")` — most handlers
- `jsonrpc::JsonRpcResponse::error(id, -32602, "missing X")` — some handlers

Phase 14 should unify these.

### 5D. `TransportError` unification scope

Keep all three error types (`PtPError`, `TransportError`, `ForetiasError`) but nest them with cause chains via `#[source]`. Phase 7 only unifies the TWO `TransportError` definitions (callbacks vs transport). `PtPError` and `ForetiasError` stay separate.

### 5E. `fn_delegation` tracking

`#![feature(fn_delegation)]` is a nightly-only Rust feature for delegating method calls to struct fields. The project targets stable Rust, so `macro_rules!` is used instead. A tracking task should be created in `rust_language_development_upkeep.md` and `INDEX.md` to monitor when `fn_delegation` stabilizes and evaluate migration.

### 5F. Error type architecture

The current `NodeError` in core-engine has 16 variants covering crypto, calendar, chronomatter, and other domains. Adding `Clock(String)` is the minimal safe change. A full rename to `CoreError` with nested `CalendarError`/`ChronomatterError` is architecturally cleaner but risks recursive error wrapping if domain errors contain `CoreError`. Defer as a separate concern.

---

## 6. Items Requiring Dedicated Specs

The following findings are OUT OF SCOPE for this redundancy cleanup but should be tracked as future specs:

### Calendar Refactoring Spec (from N1, N3, N4, N5, N6)

A dedicated spec for the 5-component Calendar architecture:
1. Calendar API in core-engine (store/retrieve, FB, GNF)
2. Calendar Storage API in core-engine (storage/search interface)
3. Calendar API implementation in core-engine (logic only; receives handles: `CommunerdetteLine` or `CommunerdLine`, Storage API, thread pools)
4. In-memory storage in core-engine
5. File-system storage in foretias-server

Also includes:
- TBIDRecord concept (N3) — stored, transmitted, participates in verification chains
- MirrorStore consolidation (N6) — move to core-engine
- ProbityStore logic extraction (N5) — move core storage to server
- `MirrorDispatcher` refactoring (N1) — return TBIDs, not raw addresses

### Error Architecture Spec (from M6, M9)

A dedicated spec for error type hierarchy:
- Nested cause chains: `PtPError` → `TransportError` → `ForetiasError` via `#[source]`
- `NodeError` → `CoreError` rename with domain variants
- `Clock(String)` variant addition
- Guidelines for avoiding recursive error wrapping

### `fn_delegation` Tracking (from M3, C3)

A tracking task in `rust_language_development_upkeep.md` and `INDEX.md`:
- Monitor `#![feature(fn_delegation)]` stabilization
- Evaluate migration from `macro_rules!` passthrough accessors when stable
- Track alternative approaches (Deref-based, proc-macro)

---

## 7. Verification Protocol

After each change:

```bash
cd p2p && cargo check --workspace
cd p2p && cargo test --workspace
cd p2p && cargo clippy --workspace --all-targets
cd p2p && cargo fmt --check

# If trust boundary code changed:
cd p2p && cargo test -p foretias-core -- clean_auth
cd p2p && cargo test -p foretias-core -- externalize
```
