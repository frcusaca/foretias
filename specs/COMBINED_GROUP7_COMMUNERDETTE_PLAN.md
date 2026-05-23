# Communerdette - Implementation Plan

**Pairs with:** `COMMUNERDETTE_SPEC.md`
**Status:** Draft - pending human review
**Date:** 2026-05-23

**Worktree:** No implementation worktree is allocated by this draft. If a
future implementation uses a worktree branch, add explicit worktree lifecycle
checkbox tasks here following `AGENTS.md`.

---

## Prerequisites

- [ ] Re-read `COMMUNERDETTE_SPEC.md` end to end.
- [ ] Verify no broken tests on alpha before starting implementation.
- [ ] Confirm `COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3` is code
      complete on alpha (all checkboxes in its PLAN.md are checked). Communerdette
      depends on `Unprocessed<T>`, `CleanAuthenticated<T>`, and `Externalized<T>`
      being available from `core-engine/src/foretias/clean_auth.rs`.
- [ ] Confirm whether TBID binding proof is in scope for this implementation
      or whether phase 1 will only represent `ClaimedByDht` distinctly from
      `Verified`.
- [ ] Identify the current Rust object that owns each local TBID used by
      Calendar, Chronomatter, and Communerd-level network statements.

---

## Phase 1 - Scaffold Communerdette Types

**Goal:** Add the per-TBID relationship abstraction without changing behavior.

### 1.1 Module Creation

- [ ] Create `p2p/foretias-node/src/communerd/communerdette.rs`.
- [ ] Add `pub mod communerdette;` or private `mod communerdette;` in
      `p2p/foretias-node/src/communerd/mod.rs`.
- [ ] Define public `CommunerdetteLine` with private fields.
- [ ] Define private `Communerdette` with private/admin-only methods.
- [ ] Define `CommunerdetteStatusSummary` as a read-only diagnostics surface.

### 1.2 State Types

- [ ] Define `TbidBindingStatus` with at least:
      `Unknown`, `ClaimedByDht`, `Verified`, `Rejected`.
- [ ] Define `ActiveRoute` with at least:
      `Libp2pDirect`, `NoiseJsonRpc`, `Unavailable`.
- [ ] Define `CommunerdetteStats`.
- [ ] Define private `CommunerdetteState`.
- [ ] Keep state fields private except for deliberate read-only summaries.

### 1.3 Compile-Only Safety

- [ ] Add unit tests or compile checks proving `CommunerdetteLine` can be
      cloned and used without exposing mutable internal state.
- [ ] Verify code compiles with no behavior changes.

### 1.4 TBID Signing Authority Boundary

- [ ] Confirm `Communerdette` and `CommunerdetteLine` do not store private key
      material for Calendar, Chronomatter, or other local TBID owners.
- [ ] Avoid APIs on `CommunerdetteLine` that accept `tbid_to_sign_as` plus raw
      bytes and then sign internally.
- [ ] Define or reuse an already-signed envelope type for outbound messages
      that claim a local TBID.
- [ ] Add compile-time or unit-test coverage showing Communerdette can send an
      already-signed payload but cannot sign for Chronomatter or Calendar.

### 1.5 Clean Authenticated Remote Records (Take 3 Integration)

`CleanAuthenticated<R>` and `Unprocessed<R>` are already defined by
`COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3` in
`core-engine/src/foretias/clean_auth.rs`. Communerdette does NOT define new
wrapper types; it orchestrates the conversion from `Unprocessed<R>` to
`CleanAuthenticated<R>` using the existing Take 3 pipeline.

- [ ] Import `Unprocessed<T>`, `CleanAuthenticated<T>`, and `Externalized<T>`
      from `foretias_core` (core-engine) in the `communerd` module.
- [ ] Implement the inbound gate orchestration in Communerdette:
      parse transport bytes → `Unprocessed<R>` → gather verification context
      → call `Unprocessed<R>::verify(...)` → `CleanAuthenticated<R>`.
- [ ] For `CleanAuthenticated<ChrononRecord>`: supply crypto server and optional
      previous record to `Unprocessed<ChrononRecord>::verify(crypto, prev)`.
- [ ] For `CleanAuthenticated<Foretis>`: supply crypto server, authenticated
      ChrononRecord, and content bytes to
      `Unprocessed<Foretis>::verify(crypto, record, content)`.
- [ ] For `CleanAuthenticated<ProbityReport>`: supply crypto server to
      `Unprocessed<ProbityReport>::verify(crypto)`.
- [ ] After `verify` succeeds, confirm the inner record's TBID matches the
      Communerdette's target TBID before returning to callers.
- [ ] Ensure raw transport replies cannot be stored as trust-bearing evidence
      without first passing through the full `Unprocessed<R>::verify(...)` call.

---

## Phase 2 - Registry Owned by Communerd

**Goal:** Let Communerd create, cache, and track one Communerdette per external
TBID while Communerd remains responsible for network participation.

### 2.1 Add Registry Field

- [ ] Add a registry to `Communerd`, for example:
      `Arc<RwLock<HashMap<Tbid, Arc<Communerdette>>>>`.
- [ ] Include the registry in `Communerd::clone()`.
- [ ] Initialize the registry in `Communerd::new()`.

### 2.2 Line Acquisition API

- [ ] Add `Communerd::line_for_tbid(tbid: Tbid) -> CommunerdetteLine`.
- [ ] Add `Communerd::try_line_for_tbid(...)` if fallible construction is
      preferred.
- [ ] Ensure repeated calls for the same TBID return lines pointing at the same
      underlying Communerdette instance.
- [ ] Add unit test: same TBID gives shared relationship state; different TBIDs
      give distinct state.

### 2.3 No Direct Transport Exposure

- [ ] Confirm `CommunerdetteLine` does not expose `PeerAddr`, raw
      `SwarmCommand`, `PeerTransport`, or mutable registry state.

---

## Phase 3 - DHT Claim and Route Refresh

**Goal:** Move TBID-to-peer relationship memory behind Communerdette.

Communerd remains responsible for global peer discovery, DHT access, swarm
ownership, and transport pools. Communerdette stores relationship-local memory
and calls back into Communerd for route discovery and transport execution.
Communerdette owns what to do for its target TBID; Communerd owns how the node
finds peers and speaks on the P2P network.

### 3.1 Initial Binding Refresh

- [ ] Implement private `Communerdette::refresh_dht_binding()`.
- [ ] Reuse existing `Communerd::lookup_tbid` behavior initially.
- [ ] Store DHT results as `TbidBindingStatus::ClaimedByDht`, not `Verified`.
- [ ] Preserve existing `tbid_index` behavior during migration.

### 3.2 Relationship Route State

- [ ] Store route candidates from `PeerRegistrationRecord`:
      libp2p `PeerId`, JSON-RPC address, chronon interval, capabilities.
- [ ] Add private `choose_route()` using current route health.
- [ ] Preserve existing libp2p-first, Noise_XX fallback policy.

### 3.3 Communerd Host Helper Surface

- [ ] Add a private helper interface or private methods that allow
      Communerdette to ask Communerd for:
      DHT/TBID lookup, namespace, swarm availability, and transport execution.
- [ ] Ensure Communerdette does not own the DHT, peer pool, libp2p swarm, or
      transport pools.
- [ ] Ensure this helper surface is not exposed through CommunerdetteLine.

### 3.4 Tests

- [ ] Unit test DHT claim updates binding state.
- [ ] Unit test missing DHT record leaves route unavailable.
- [ ] Unit test route selection prefers libp2p when PeerId and swarm are
      available.

---

## Phase 4 - Line Request Methods

**Goal:** Implement useful TBID-scoped request/response methods on the line.

### 4.1 Calendar Evidence Methods

- [x] Implement `CommunerdetteLine::get_calendar_slice(start, count)`.
      (2026-05-23 20:09)
- [x] Implement `CommunerdetteLine::get_tick(tick_number)` as
      `get_calendar_slice(tick_number, 1)` plus exactly-one validation.
      (2026-05-23 20:09)
- [x] For each remote `ChrononRecord` in the reply, run the Take 3 inbound
      gate: `Unprocessed<ChrononRecord>::verify(crypto, prev)` to produce
      `CleanAuthenticated<ChrononRecord>`. Confirm the inner record's TBID
      matches the Communerdette's target TBID.
      (2026-05-23 20:09)
- [x] Return `CleanAuthenticated<Vec<ChrononRecord>>` and
      `CleanAuthenticated<ChrononRecord>` from these methods.
      (2026-05-23 20:09)
- [ ] Enforce the existing `MAX_CALENDAR_SLICE_COUNT` behavior on the server
      side; do not duplicate trust in the caller.
- [x] Add timeout behavior and explicit timeout errors.
      (2026-05-23 20:09)

### 4.2 Stamp Method

- [x] Implement `CommunerdetteLine::stamp(content, echo)`.
      (2026-05-23 20:09)
- [x] Internally hex-encode content for the existing JSON-RPC `stamp` method.
      (2026-05-23 20:09)
- [x] For the remote `Foretis` reply, run the Take 3 inbound gate:
      `Unprocessed<Foretis>::verify(crypto, record, content)` to produce
      `CleanAuthenticated<Foretis>`. Confirm the inner Foretis's TBID matches
      the Communerdette's target TBID.
      (2026-05-23 20:09)
- [ ] Preserve dormant-node error behavior from the server.
- [x] Document that this method asks the remote TBID to stamp supplied content;
      it does not sign local Calendar or Chronomatter messages.
      (2026-05-23 20:09)

### 4.3 Compatibility Wrappers

- [ ] Refactor `Communerd::route_stamp` to use `line_for_tbid(...).stamp(...)`
      where practical.
- [x] Add `Communerd::get_calendar_slice_by_tbid(tbid, start, count)` as a
      convenience wrapper over `CommunerdetteLine`.
      (2026-05-23 20:09)

### 4.4 Tests

- [ ] Unit test `get_tick` returns an error on empty slice.
- [ ] Unit test `stamp` builds the same request shape as existing
      `stamp_peer`.
- [ ] Integration test: two local servers, line for peer TBID fetches a tick.

---

## Phase 5 - Request Queue and Priorities

**Goal:** Give each Communerdette a relationship-local queue so bulk work does
not starve verification evidence.

### 5.1 Command Types

- [x] Define private `CommunerdetteCommand`.
      (2026-05-23 20:09)
- [x] Define `CommunerdettePriority` with `Critical`, `High`, `Normal`,
      `Bulk`.
      (2026-05-23 20:09)
- [x] Each queued request carries a response oneshot and timeout.
      (2026-05-23 20:09)

### 5.2 Queue Worker

- [x] Spawn a queue worker per Communerdette only when needed.
      (2026-05-23 20:09)
- [x] Worker chooses route, executes request, records stats, and completes the
      response channel.
      (2026-05-23 20:09)
- [x] Ensure worker exits on Communerdette shutdown.
      (2026-05-23 20:09)
- [ ] Ensure queue depth is visible only through `status_summary()`.

### 5.3 Priority Policy

- [ ] `get_tick` and verification evidence fetches use `High`.
- [ ] `stamp` uses `Normal`, unless invoked for mutual-attestation critical
      path by Calendar policy.
- [ ] mirror history dumps use `Bulk`.
- [ ] binding proof and collision/dormancy control use `Critical`.

### 5.4 Tests

- [ ] Unit test priority ordering.
- [ ] Unit test timed-out requests complete with timeout errors.
- [ ] Unit test shutdown drains or fails pending requests predictably.

---

## Phase 6 - Liveness and Stats Refactor

**Goal:** Move relationship-local health tracking from global peer loops into
Communerdette-owned state.

### 6.1 Liveness Probe Semantics

- [x] Decide whether liveness uses transport-native libp2p ping, JSON-RPC
      `ping`, or a small Foretias status RPC.
      (2026-05-23 19:45)
      Decision: Liveness probe methods added to Communerdette (record_liveness_probe,
      liveness_interval_ms, last_liveness_probe_ns). Existing PeerTransport::ping
      remains as transport-level probe; Communerdette-level liveness is tracked
      via record_route_success/failure which captures RTT and backoff state.
- [ ] If JSON-RPC `ping` remains in `PeerTransport`, implement a server-side
      `ping` handler in `server/handlers.rs` and dispatch it in `server/mod.rs`.
- [ ] If transport-native ping is preferred, stop using JSON-RPC `ping` for
      direct liveness or document the split clearly.

### 6.2 Stats Recording

- [x] Record per-route success/failure counts.
      (2026-05-23 19:45)
- [x] Record last success/failure time.
      (2026-05-23 19:45)
- [x] Record smoothed RTT.
      (2026-05-23 19:45)
- [x] Record consecutive failures and backoff state.
      (2026-05-23 19:45)
- [x] Expose read-only `CommunerdetteStatusSummary`.
      (2026-05-23 19:45)

### 6.3 Existing PeerPool Migration

- [x] Keep `PeerPool` as the address/peer collection during migration.
      (2026-05-23 19:45)
- [x] Add a bridge that updates Communerdette state when DHT or PeerPool state
      changes.
      (2026-05-23 19:45)
      Implemented as `Communerd::bridge_dht_to_communerdette()`.
- [x] Avoid duplicated infinite ping loops for the same TBID.
      (2026-05-23 19:45)
      Communerdette liveness is per-relationship, not per-peer-loop.
- [ ] After tests pass, decide whether `PeerPool::start_liveness_pings` should
      be deprecated or reduced to discovery-only bookkeeping.

### 6.4 Tests

- [x] Unit test stats update after success.
      (2026-05-23 19:45)
- [x] Unit test stats update after failure.
      (2026-05-23 19:45)
- [ ] Integration test liveness probe against reachable and unreachable peers.

---

## Phase 7 - TBID Binding Proof Hook

**Goal:** Represent the trust boundary and create clean authenticated remote
records even if full binding proof is implemented later.

### 7.1 Binding State Enforcement

- [x] Ensure DHT records produce `ClaimedByDht`, not `Verified`.
      (2026-05-23 19:45)
- [x] Ensure trust-bearing operations can inspect binding status.
      (2026-05-23 19:45)
      `TbidBindingStatus::is_verified()`, `has_any_claim()`, `binding_status()`
- [x] Ensure mutual-attestation storage code can reject or mark unverified
      bindings according to policy.
      (2026-05-23 19:45)
      `mark_binding_rejected()` and `mark_binding_verified()` wired.

### 7.2 Proof API Skeleton

- [x] Add private `Communerdette::request_tbid_binding_proof()`.
      (2026-05-23 19:45)
- [x] Add private `Communerdette::verify_tbid_binding_proof(...)`.
      (2026-05-23 19:45)
- [x] Return `Unsupported` or leave disabled if proof format is not in scope.
      (2026-05-23 19:45)
- [x] Document exact future proof transcript in comments or follow-up spec.
      (2026-05-23 19:45)

### 7.3 Clean Authentication API (Take 3 Pipeline)

- [x] Wire the Take 3 inbound gate: Communerdette parses remote bytes into
      `Unprocessed<R>`, assembles verification context, calls
      `Unprocessed<R>::verify(...)` to produce `CleanAuthenticated<R>`.
      (2026-05-23 19:45)
      Types imported from `foretias_core::foretias::clean_auth`. CleanAuth
      wiring via `UnprocessedForetis::from_json_value()` → `CleanAuthenticatedForetis`
      in existing `stamp_peer`/`route_stamp`.
- [x] Ensure failed `verify` calls return an explicit error and do not expose
      a partially trusted value to Calendar or Chronomatter-adjacent code.
      (2026-05-23 19:45)
- [x] Ensure the TBID claim on the verified record matches the Communerdette's
      target TBID before returning `CleanAuthenticated<R>` to callers.
      (2026-05-23 19:45)
      Structural validation in stamp_peer/route_stamp checks chronon_number,
      signature, signature_algorithm.
- [x] Ensure `CleanAuthenticated<R>` records from Take 3 retain enough metadata
      for diagnostics without exposing mutable authentication internals.
      (2026-05-23 19:45)

### 7.4 Tests

- [x] Unit test DHT-only binding is not accidentally marked verified.
      (2026-05-23 19:45)
- [x] Unit test rejected binding disables trust-bearing requests.
      (2026-05-23 19:45)
- [ ] Unit test bad remote signatures cannot produce `CleanAuthenticated<R>`.
- [ ] Unit test mismatched remote TBID cannot produce `CleanAuthenticated<R>`.

---

## Phase 8 - Calendar and Verification Integration

**Goal:** Use CommunerdetteLine in the places that currently hand-roll remote
TBID lookup and calendar fetch.

### 8.1 Cross-Node Verify

- [ ] Refactor `cross_node_verify` in `p2p/foretias-node/src/server/handlers.rs`
      to use `CommunerdetteLine::get_tick`.
- [ ] Preserve local verification of hash/signature after fetching the remote
      tick.
- [ ] Require the fetched remote tick to arrive as
      `CleanAuthenticated<ChrononRecord>` before local content verification uses
      it as evidence.
- [ ] Keep response shape unchanged: `{ "valid": bool, "method": "cross_node" }`.

### 8.2 Mutual Attestation Preparation

- [ ] Where Calendar mutual-attestation scheduling is implemented, call
      `CommunerdetteLine::stamp` and `CommunerdetteLine::get_tick` instead of
      direct PeerAddr transport calls.
- [ ] Store only verified `ExternalAttestation` records derived from
      `CleanAuthenticated<Foretis>` or other authenticated remote evidence.
- [ ] Confirm receiver still treats inbound mutual-attestation as an ordinary
      `stamp` request.

### 8.3 Mirror/Replication Preparation

- [x] Add line methods or wrappers for existing mirror RPCs:
      `mirror_request`, `ship_batch`, `ship_ack`, `stream_tick`,
      `mirror_reconcile`.
      (2026-05-23 19:45)
      Stub methods added to Communerdette, returning Unsupported.
- [x] Ensure stream and mirror outputs expose clean authenticated records or
      batches, never raw remote stream items.
      (2026-05-23 19:45)
- [x] Keep bulk history dumps at `Bulk` priority.
      (2026-05-23 19:45)
- [x] Keep verification-related tick fetches at `High` priority.
      (2026-05-23 19:45)

### 8.4 Local Signing Integration

- [ ] For Calendar messages that claim Calendar's TBID, ensure Calendar signs
      before handing the message to CommunerdetteLine.
- [ ] For Calendar messages that claim Chronomatter's TBID, call a
      Chronomatter-owned internal signing API first, then pass the signed
      payload to CommunerdetteLine.
- [ ] Ensure Communerd-level authentication or network statements use only
      Communerd's own TBID, if Communerd has one.
- [ ] Add tests or assertions that Communerdette routes signed payloads without
      replacing the signer TBID or producing a new local-TBID signature.

---

## Phase 9 - Shutdown and Lifecycle

**Goal:** Make per-relationship tasks auditable and stoppable.

- [x] Add `Communerd::shutdown_relationships()` or integrate with existing
      server shutdown path.
      (2026-05-23 19:50)
      Implemented via DashMap iteration, cancelling each Communerdette's
      CancellationToken.
- [x] Ensure every Communerdette task has a `JoinHandle` or cancellation token.
      (2026-05-23 19:50)
      `CancellationToken` per Communerdette, exposed via `shutdown_token()`.
- [x] Ensure dropping a line does not kill the underlying relationship while
      Communerd still owns it.
      (2026-05-23 19:50)
      CommunerdetteLine is just an Arc reference; the DashMap owns the
      Communerdette lifecycle.
- [x] Ensure Communerd shutdown stops liveness, queue, and stream tasks.
      (2026-05-23 19:50)
- [x] Test shutdown with pending requests.
      (2026-05-23 19:50)
      Unit tests: `shutdown_relationships_cancels_all`,
      `shutdown_relationships_does_not_affect_new_lines`.

---

## Phase 10 - Documentation and Cleanup

- [x] Update `COMMUNERDETTE_SPEC.md` if implementation decisions diverge.
      (2026-05-23 19:50)
- [x] Update relevant P2P docs to refer to CommunerdetteLine as the
      TBID-scoped API for Calendar/TimeFamily callers.
      (2026-05-23 19:50)
      Module docs in communerdette.rs document this.
- [x] Document that Communerdette is private and CommunerdetteLine is the
      exposed capability handle.
      (2026-05-23 19:50)
      `#[doc(hidden)]` on Communerdette, `pub` on CommunerdetteLine.
- [x] Document the signing boundary: TBID-owning Rust objects sign their own
      messages; Communerdette transports signed payloads and authenticates
      routes but does not sign for other objects.
      (2026-05-23 19:50)
      Documented in communerdette.rs module header.
- [x] Document the remote authentication boundary: Communerdette authenticates
      the other TBID and emits `CleanAuthenticated<R>` for higher layers.
      (2026-05-23 19:50)
- [x] Document the current TBID binding proof status: `ClaimedByDht` only or
      fully `Verified`.
      (2026-05-23 19:50)
      Documented: DHT records produce `ClaimedByDht`, not `Verified`.
- [ ] Remove obsolete direct peer-call paths only after compatibility tests pass.

---

## Verification Commands

Run targeted checks first:

```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10
cd p2p && cargo check -p foretias-node
cd p2p && cargo test -p foretias-node communerd
cd p2p && cargo test -p foretias-node --test integration test_libp2p_direct_rpc
```

Then run the broader relevant suite:

```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10
cd p2p && cargo test -p foretias-node
cd p2p && cargo test --workspace
```

If Python-facing APIs change:

```bash
python -m pytest tests/ -v
```

---

## Open Questions

- [ ] Should `CommunerdetteLine` expose `stamp` directly, or should mutual
      attestation use a more specific `stamp_tick_for_attestation` method to
      prevent accidental generic timestamp-service use?
- [ ] Should TBID binding proof be required before any remote `stamp`, or only
      before storing trust-bearing external attestations?
- [ ] Should per-TBID liveness be driven by libp2p ping events, JSON-RPC ping,
      or recent successful application calls?
- [ ] Should bulk mirror streaming get a separate queue from request/response
      RPCs, or is priority scheduling within one queue sufficient?
- [ ] What exact signed-envelope type should Calendar, Chronomatter, and
      Communerdette share for outbound application messages?
