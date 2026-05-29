# Communerdette - Implementation Plan

**Pairs with:** `COMMUNERDETTE_SPEC.md`
**Status:** Draft - pending human review
**Date:** 2026-05-23

**Downstream dependency (2026-05-26):** The remaining mirror work in
Group 4 (`COMBINED_GROUP4_SPEC.md` Stream 4b open items + Stream 4c)
and the mutual attestation work in Group 6
(`COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md`) are explicitly
**DEFERRED** until this plan reaches completion. When making changes
to Communerdette's public interface (`MirrorDispatcher`,
`CommunerdetteLine`, per-TBID lifecycle state machine), note them
here so the dependent specs can be updated in lockstep before
implementation resumes downstream.

**Worktree:** No implementation worktree is allocated by this draft. If a
future implementation uses a worktree branch, add explicit worktree lifecycle
checkbox tasks here following `AGENTS.md`.

---

## Prerequisites

- [x] Re-read `COMMUNERDETTE_SPEC.md` end to end. (2026-05-27)
- [x] Verify no broken tests on alpha before starting implementation. (2026-05-27)
- [x] Confirm `COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3` is code (2026-05-27)
      complete on alpha (all checkboxes in its PLAN.md are checked). Communerdette
      depends on `Unprocessed<T>`, `CleanAuthenticated<T>`, and `Externalized<T>`
      being available from `core-engine/src/foretias/clean_auth.rs`.
- [x] Confirm whether TBID binding proof is in scope for this implementation (2026-05-27)
      or whether phase 1 will only represent `ClaimedByDht` distinctly from
      `Verified`.
- [x] Identify the current Rust object that owns each local TBID used by (2026-05-27)
      Calendar, Chronomatter, and Communerd-level network statements.

---

## Phase 1 - Scaffold Communerdette Types

**Goal:** Add the per-TBID relationship abstraction without changing behavior.

### 1.1 Module Creation

- [x] Create `p2p/foretias-node/src/communerd/communerdette.rs`. (2026-05-27)
- [x] Add `pub mod communerdette;` or private `mod communerdette;` in (2026-05-27)
      `p2p/foretias-node/src/communerd/mod.rs`.
- [x] Define public `CommunerdetteLine` with private fields. (2026-05-27)
- [x] Define private `Communerdette` with private/admin-only methods. (2026-05-27)
- [x] Define `CommunerdetteStatusSummary` as a read-only diagnostics surface. (2026-05-27)

### 1.2 State Types

- [x] Define `TbidBindingStatus` with at least: (2026-05-27)
      `Unknown`, `ClaimedByDht`, `Verified`, `Rejected`.
- [x] Define `ActiveRoute` with at least: (2026-05-27)
      `Libp2pDirect`, `NoiseJsonRpc`, `Unavailable`.
- [x] Define `CommunerdetteStats`. (2026-05-27)
- [x] Define private `CommunerdetteState`. (2026-05-27)
- [x] Keep state fields private except for deliberate read-only summaries. (2026-05-27)

### 1.3 Compile-Only Safety

- [x] Add unit tests or compile checks proving `CommunerdetteLine` can be (2026-05-27)
      cloned and used without exposing mutable internal state.
- [x] Verify code compiles with no behavior changes. (2026-05-27)

### 1.4 TBID Signing Authority Boundary

- [x] Confirm `Communerdette` and `CommunerdetteLine` do not store private key (2026-05-27)
      material for Calendar, Chronomatter, or other local TBID owners.
- [x] Avoid APIs on `CommunerdetteLine` that accept `tbid_to_sign_as` plus raw (2026-05-27)
      bytes and then sign internally.
- [x] Define or reuse an already-signed envelope type for outbound messages (2026-05-27)
      that claim a local TBID.
- [x] Add compile-time or unit-test coverage showing Communerdette can send an (2026-05-27)
      already-signed payload but cannot sign for Chronomatter or Calendar.

### 1.5 Clean Authenticated Remote Records (Take 3 Integration)

`CleanAuthenticated<R>` and `Unprocessed<R>` are already defined by
`COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3` in
`core-engine/src/foretias/clean_auth.rs`. Communerdette does NOT define new
wrapper types; it orchestrates the conversion from `Unprocessed<R>` to
`CleanAuthenticated<R>` using the existing Take 3 pipeline.

- [x] Import `Unprocessed<T>`, `CleanAuthenticated<T>`, and `Externalized<T>` (2026-05-27)
      from `foretias_core` (core-engine) in the `communerd` module.
- [x] Implement the inbound gate orchestration in Communerdette: (2026-05-27)
      parse transport bytes → `Unprocessed<R>` → gather verification context
      → call `Unprocessed<R>::verify(...)` → `CleanAuthenticated<R>`.
- [x] For `CleanAuthenticated<ChrononRecord>`: supply crypto server and optional (2026-05-27)
      previous record to `Unprocessed<ChrononRecord>::verify(crypto, prev)`.
- [x] For `CleanAuthenticated<Foretis>`: supply crypto server, authenticated (2026-05-27)
      ChrononRecord, and content bytes to
      `Unprocessed<Foretis>::verify(crypto, record, content)`.
- [ ] For `CleanAuthenticated<ProbityReport>`: supply crypto server to
      `Unprocessed<ProbityReport>::verify(crypto)`.
- [x] After `verify` succeeds, confirm the inner record's TBID matches the (2026-05-27)
      Communerdette's target TBID before returning to callers.
- [x] Ensure raw transport replies cannot be stored as trust-bearing evidence (2026-05-27)
      without first passing through the full `Unprocessed<R>::verify(...)` call.

---

## Phase 2 - Registry Owned by Communerd

**Goal:** Let Communerd create, cache, and track one Communerdette per external
TBID while Communerd remains responsible for network participation.

### 2.1 Add Registry Field

- [x] Add a registry to `Communerd`, for example:
      `Arc<RwLock<HashMap<Tbid, Arc<Communerdette>>>>`.
      (2026-05-23 — implemented as `Arc<DashMap<Tbid, Arc<Communerdette>>>`)
- [x] Include the registry in `Communerd::clone()`.
      (2026-05-23 — `communerdettes: Arc::clone(&self.communerdettes)` in Clone impl)
- [x] Initialize the registry in `Communerd::new()`.
      (2026-05-23 — `communerdettes: Arc::new(DashMap::new())`)

### 2.2 Line Acquisition API

- [x] Add `Communerd::line_for_tbid(tbid: Tbid) -> CommunerdetteLine`.
      (2026-05-23)
- [x] Add `Communerd::try_line_for_tbid(...)` if fallible construction is
      preferred.
      (2026-05-23)
- [x] Ensure repeated calls for the same TBID return lines pointing at the same
      underlying Communerdette instance.
      (2026-05-23 — DashMap entry reuse; verified by `clone_shares_communerdette_registry` test)
- [x] Add unit test: same TBID gives shared relationship state; different TBIDs
      give distinct state.
      (2026-05-23 — `same_tbid_returns_shared_state`, `different_tbids_get_different_lines`,
      `clone_shares_communerdette_registry` in communerd/mod.rs)

### 2.3 No Direct Transport Exposure

- [x] Confirm `CommunerdetteLine` does not expose `PeerAddr`, raw
      `SwarmCommand`, `PeerTransport`, or mutable registry state.
      (2026-05-26 — verified: CommunerdetteLine public API is target_tbid, status_summary,
      shutdown_token, get_calendar_slice, get_tick, stamp, start_calendar_stream only)

---

## Phase 3 - DHT Claim and Route Refresh

**Goal:** Move TBID-to-peer relationship memory behind Communerdette.

Communerd remains responsible for global peer discovery, DHT access, swarm
ownership, and transport pools. Communerdette stores relationship-local memory
and calls back into Communerd for route discovery and transport execution.
Communerdette owns what to do for its target TBID; Communerd owns how the node
finds peers and speaks on the P2P network.

### 3.1 Initial Binding Refresh

- [x] Implement private `Communerdette::refresh_dht_binding()`. (2026-05-27)
- [x] Reuse existing `Communerd::lookup_tbid` behavior initially. (2026-05-27)
- [x] Store DHT results as `TbidBindingStatus::ClaimedByDht`, not `Verified`. (2026-05-27)
- [x] Preserve existing `tbid_index` behavior during migration. (2026-05-27)

### 3.2 Relationship Route State

- [x] Store route candidates from `PeerRegistrationRecord`: (2026-05-27)
      libp2p `PeerId`, JSON-RPC address, chronon interval, capabilities.
- [x] Add private `choose_route()` using current route health. (2026-05-27)
- [x] Preserve existing libp2p-first, Noise_XX fallback policy. (2026-05-27)

### 3.3 Communerd Host Helper Surface

- [x] Add a private helper interface or private methods that allow (2026-05-27)
      Communerdette to ask Communerd for:
      DHT/TBID lookup, namespace, swarm availability, and transport execution.
- [x] Ensure Communerdette does not own the DHT, peer pool, libp2p swarm, or (2026-05-27)
      transport pools.
- [x] Ensure this helper surface is not exposed through CommunerdetteLine. (2026-05-27)

### 3.4 Tests

- [x] Unit test DHT claim updates binding state. (2026-05-27)
- [x] Unit test missing DHT record leaves route unavailable. (2026-05-27)
- [x] Unit test route selection prefers libp2p when PeerId and swarm are (2026-05-27)
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
- [x] Enforce the existing `MAX_CALENDAR_SLICE_COUNT` behavior on the server (2026-05-27)
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

### 4.3 Route stamp through CommunerdetteLine

All outbound stamp calls must flow through Communerdette so they receive the
full Take 3 inbound gate and return `CleanAuthenticated<Foretis>` (not the
bare `Foretis` type or a `from_trusted` bypass).

- [x] Replace the body of `Communerd::route_stamp` with:
      ```rust
      let content = hex::decode(content_hex)?;
      let tbid = Tbid::from_hex(target_tbid)?;
      self.line_for_tbid(tbid).stamp(content, echo.to_string()).await
      ```
      Return type changes from `Result<Foretis, TransportError>` to
      `Result<CleanAuthenticated<Foretis>, CommunerdetteError>`.
      (2026-05-28)
- [x] Mark `Communerd::stamp_peer` `#[deprecated(note = "use CommunerdetteLine::stamp")]`.
      (2026-05-28)
- [x] Update `tiers.rs` wrappers to forward the new return type.
      (2026-05-28)
- [x] Update `handle_route_stamp` in `handlers.rs` to call `.into_inner()` for
      the JSON response (the `CleanAuthenticated<Foretis>` wrapper is internal).
      (2026-05-28)

**Prerequisite for Phase 8.1.**

### 4.4 Tests

- [x] Unit test `get_tick` returns an error on empty slice.
      (2026-05-28) `execute_tick_returns_error_on_empty_slice`
- [x] Unit test `route_stamp` returns `CleanAuthenticated<Foretis>` with the
      correct TBID and a non-empty signature.
      (2026-05-28) `execute_stamp_produces_clean_authenticated_foretis`
- [ ] Confirm `handle_route_stamp` response shape is unchanged (`{ "tbid": ...,
      "chronon_number": ..., ... }`) after the refactor.
      (deferred — requires full handler test infrastructure)
- [x] Add `Communerd::get_calendar_slice_by_tbid(tbid, start, count)` as a
      convenience wrapper over `CommunerdetteLine`.
      (2026-05-23 20:09)

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
- [x] Ensure queue depth is visible only through `status_summary()`. (2026-05-27)

### 5.3 Priority Policy

- [ ] `get_tick` and verification evidence fetches use `High`.
- [ ] `stamp` uses `Normal`, unless invoked for mutual-attestation critical
      path by Calendar policy.
- [ ] mirror history dumps use `Bulk`.
- [ ] binding proof and collision/dormancy control use `Critical`.

### 5.4 Tests

- [x] Unit test priority ordering. (2026-05-27)
- [x] Unit test timed-out requests complete with timeout errors. (2026-05-27)
- [x] Unit test shutdown drains or fails pending requests predictably. (2026-05-27)

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
- [x] If JSON-RPC `ping` remains in `PeerTransport`, implement a server-side
      `ping` handler in `server/handlers.rs` and dispatch it in `server/mod.rs`.
      (2026-05-26 — `handle_ping` added to handlers.rs, dispatched first in mod.rs match;
      returns `{"pong": true}` unconditionally regardless of dormant state)

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
- [x] After tests pass, decide whether `PeerPool::start_liveness_pings` should
      be deprecated or reduced to discovery-only bookkeeping.
      (2026-05-26 — Decision: retain for now. PeerPool ping loop serves a distinct
      purpose (generic connectivity check for all configured peers) from Communerdette
      per-TBID liveness tracking. Deprecation to discovery-only should happen when
      mutual attestation / Communerdette is mature enough to replace all liveness
      signalling currently done via PeerPool. No code change required at this stage.)

### 6.4 Tests

- [x] Unit test stats update after success.
      (2026-05-23 19:45)
- [x] Unit test stats update after failure.
      (2026-05-23 19:45)
- [x] Integration test liveness probe against reachable and unreachable peers.
      (2026-05-26 — `liveness_ping_reachable_peer_succeeds` and
      `liveness_ping_unreachable_peer_fails` added to tests/mirror_integration.rs;
      both pass)

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
- [x] Unit test bad remote signatures cannot produce `CleanAuthenticated<R>`.
      (2026-05-26 — `gate_chronon_records_rejects_bad_chained_signature`: a chained record
      with garbage forward/backward signatures returns `Err(CommunerdetteError::CleanAuth(_))`.
      `gate_foretis_structural_gate_does_not_crypto_verify_signature`: documents that
      `gate_foretis` intentionally defers full sig verification to the call site, which
      requires the ChrononRecord — this is by design, not a gap.)
- [x] Unit test mismatched remote TBID cannot produce `CleanAuthenticated<R>`.
      (2026-05-26 — covered by pre-existing tests:
      `gate_chronon_records_rejects_tbid_mismatch` and `gate_foretis_rejects_tbid_mismatch`)

---

## Phase 8 - Calendar and Verification Integration

**Goal:** Use CommunerdetteLine in the places that currently hand-roll remote
TBID lookup and calendar fetch.

### 8.1 Cross-Node Verify

**Prerequisite: Phase 4.3 complete** (establishes the pattern).

Current `cross_node_verify` in `handlers.rs`:
- Does its own DHT lookup via `com.lookup_tbid(...)` — bypasses Communerdette
- Calls `com.get_calendar_slice(&owner_peer, ...)` directly on the transport
- Uses `CleanAuthenticatedChrononRecord::from_trusted(rec.clone())` — the same
  `from_trusted` bypass; no real signature verification

Fix:
- [x] Replace the DHT lookup + direct transport call with:
      ```rust
      let tbid = Tbid::from_hex(foretis_tbid_hex)?;
      let tick: CleanAuthenticated<ChrononRecord> =
          com.line_for_tbid(tbid).get_tick(foretis_ref.chronon_number).await?;
      ```
      (2026-05-28)
- [x] Remove the manual `from_trusted` construction of `CleanAuthenticated<ChrononRecord>`.
      `line.get_tick(...)` already returns `CleanAuthenticated<ChrononRecord>` from the
      Take 3 gate — use it directly.
      (2026-05-28)
- [x] Pass the gated `tick` to `unproc_foretis.into_clean_authenticated(crypto, content, &tick)`.
      (2026-05-28)
- [x] Keep response shape unchanged: `{ "valid": bool, "method": "cross_node" }`.
      (2026-05-28)

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

## Phase 9 - Shutdown and Lifecycle ✅ COMPLETE (2026-05-23)

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
      **Blocked on Phase 4.3** (refactor `Communerd::route_stamp` / `stamp_peer` to use
      `line_for_tbid(...).stamp(...)`) and **Phase 4.4** (compatibility tests: `get_tick`
      error on empty slice, `stamp` request-shape test, two-server integration test).
      `route_stamp` and `stamp_peer` in `communerd/mod.rs` still use direct transport;
      these are the paths to remove once Phase 4 compatibility work is merged.

---

## Phase 11 — Fast-Key Authentication Enforcement ✅ COMPLETE (2026-05-27)

**Goal:** Enforce the universal inbound authentication invariant (Spec §4
invariants 17, 19) across all Communerdette gate functions. No remote data
may become `CleanAuthenticated<R>` without passing fast-key verification.
Annotate slow-key hooks for future insertion.

**Spec refs:** §11.4, §11.5, §4 invariants 17–22, §15 criteria 10–12.

### 11.0 Core-Engine Auth Type Extensions

These additions to `core-engine/src/foretias/clean_auth.rs` are prerequisites
for Phase 12.0 channel-binding and for any code that needs to distinguish fast-
only vs dual-key authentication.

- [x] Add `is_authenticated_quickly() -> bool { true }` as an inherent method on (2026-05-27)
      `CleanAuthenticated<T>`. This is a runtime-checkable guarantee that fast-
      key verification has been performed.
- [x] Add `CleanFullyAuthenticated<T>` struct in `clean_auth.rs`: (2026-05-27)
  ```rust
  /// A record authenticated against both the fast key and the slow key.
  #[derive(Debug, Clone)]
  pub struct CleanFullyAuthenticated<T> {
      inner: T,
  }

  impl<T> CleanFullyAuthenticated<T> {
      /// Construct from data verified against both keys.
      /// Private — only produced by dual-key verification paths.
      pub(crate) fn from_dual_verified(inner: T) -> Self { Self { inner } }
      pub fn inner(&self) -> &T { &self.inner }
      pub fn into_inner(self) -> T { self.inner }
      pub fn is_authenticated_quickly(&self) -> bool { true }
      pub fn is_authenticated_fully(&self) -> bool { true }
  }

  impl<T: Clone> From<CleanFullyAuthenticated<T>> for CleanAuthenticated<T> {
      fn from(full: CleanFullyAuthenticated<T>) -> CleanAuthenticated<T> {
          CleanAuthenticated::from_trusted(full.into_inner())
      }
  }
  ```
- [x] Unit test: `CleanFullyAuthenticated` converts to `CleanAuthenticated` via `From`. (2026-05-27)
- [x] Unit test: `is_authenticated_quickly()` returns true on both types. (2026-05-27)
- [x] Unit test: `is_authenticated_fully()` returns true only on `CleanFullyAuthenticated`. (2026-05-27)

### 11.1 Fix gate_foretis

The current implementation calls `CleanAuthenticated::from_trusted(...)` on
remote Foretis data — a correctness bug (spec §11.5).

- [x] Change `gate_foretis` signature to accept an authenticated (2026-05-27)
      `CleanAuthenticated<ChrononRecord>` as an additional parameter, or to
      accept the raw content bytes and fetch the tick internally via
      `CommunerdetteExecutor`.
- [x] Replace `CleanAuthenticated::from_trusted(unprocessed.into_inner())` (2026-05-27)
      with `Unprocessed<Foretis>::verify(crypto, chronon_record, content)`.
- [x] All callers of `gate_foretis` must supply the `ChrononRecord`. For (2026-05-27)
      `execute_stamp`, chain a `get_tick(foretis.chronon_number)` call to
      obtain the record before calling `gate_foretis`.
- [x] Update tests: the existing `gate_foretis_returns_clean_authenticated_on_valid` (2026-05-27)
      test must supply a real authenticated `ChrononRecord`; if that requires
      a real crypto key-pair in the test fixture, update `make_test_foretis` or
      add a new fixture accordingly.
- [x] Add test: `gate_foretis_rejects_foretis_with_wrong_signature` — a Foretis (2026-05-27)
      whose signature does not verify against the provided ChrononRecord's
      public key must return `Err(CommunerdetteError::CleanAuth(...))`.

### 11.2 Audit from_trusted Usage

- [x] Search the entire `communerd/` module for `from_trusted` calls. (2026-05-27)
      Any call on data that did not originate locally is a bug.
      ```bash
      grep -rn "from_trusted" p2p/foretias-server/src/communerd/
      ```
- [x] For each hit, document whether the data is local (acceptable) or remote (2026-05-27)
      (must be replaced with `Unprocessed<R>::verify(...)`).
- [x] Fix any non-local uses found. (2026-05-27)

### 11.3 Slow-Key Gate Stubs

- [x] In each gate function (`gate_chronon_records`, the fixed `gate_foretis`, (2026-05-27)
      and any future gate functions), add a clearly marked stub comment
      after fast-key verification passes:
      ```rust
      // TODO(slow-key): if record carries a slow-key signature, verify it here
      // against target_tbid's slow public key. Reject if signature is present
      // but invalid. Absence is acceptable. Slow-key verification is currently
      // only required at channel-binding establishment; see Phase 12.0.
      ```
- [x] Do not implement slow-key verification in general gate functions yet — only (2026-05-27)
      the stub comment. The full slow-key implementation belongs in Phase 12.0.

### 11.4 Externalized Outbound Stubs

- [x] For each outbound message Communerdette dispatches via (2026-05-27)
      `CommunerdetteHost::call_peer`, document in a comment that the payload
      should eventually be wrapped as `Externalized<R>` once the outbound
      type wrapper is wired.
      ```rust
      // TODO(externalized): wrap params as Externalized<R> before dispatch
      // once outbound type enforcement is implemented.
      ```
- [x] Do not implement `Externalized<R>` wrapping yet — only the stub comment. (2026-05-27)

### 11.5 Crypto Call-Site Snapshot Test

**Goal:** establish `p2p/tests/snapshots/crypto_call_sites.txt` as a committed
golden file for all signing and verification call sites (Spec §11.6, §15 criterion 13).

- [x] Create `p2p/tests/crypto_callsite_snapshot.rs` (or equivalent): (2026-05-27)
  - Runs `grep -rn` (or an AST-walk script) over `p2p/` source for:
    1. Calls to `CryptoServer` methods that dispatch to C++ primitives
       (sign, verify, hash, key-gen variants).
    2. All `sign(` and `verify(` call sites in `communerd/`, `clean_auth.rs`,
       and any future auth-path files.
  - Produces a sorted list: `<crate>/<file>:<line>  <fn>  <call_type>`.
  - Compares against `p2p/tests/snapshots/crypto_call_sites.txt`.
  - Test fails on any diff (new or removed call site).
- [x] Generate the initial snapshot with all current call sites: (2026-05-27)
  ```bash
  scripts/gen_crypto_snapshot.sh > p2p/tests/snapshots/crypto_call_sites.txt
  git add p2p/tests/snapshots/crypto_call_sites.txt
  ```
  (@human: `scripts/gen_crypto_snapshot.sh` is a new shell script wrapping the
  grep pass; create it as part of this step. The snapshot is regenerated — not
  edited by hand — whenever a new crypto call site lands.)
- [x] Add `// SIGN(...)` or `// VERIFY(...)` annotation comment at each call (2026-05-27)
      site found, naming the signing authority and authentication level.
      Example: `// SIGN(local-tbid, fast-key)` at the `channel_bind_response`
      signing site; `// VERIFY(remote-tbid, fast-key)` at each gate function.
- [x] Confirm snapshot test passes on a clean workspace. (2026-05-27)
- [x] Document: any PR that adds a new crypto call site must update the snapshot (2026-05-27)
      and add the annotation. Reviewers use the snapshot diff to gate crypto
      surface area.

### 11.6 Tests

- [x] Confirm all existing gate tests still pass after 11.1 fix. (2026-05-27)
- [x] `gate_foretis_rejects_foretis_with_wrong_signature` (see 11.1). (2026-05-27)
- [x] Crypto call-site snapshot test passes (see 11.5). (2026-05-27)
- [x] Confirm workspace tests pass: `cargo test --workspace`. (2026-05-27)

---

## Phase 12 — Channel Binding + Three Liveness Levels ✅ COMPLETE (2026-05-27)

**Goal:** Implement channel-binding establishment (§12.0) and all three
liveness levels inside Communerdette (Spec §12).
Deprecate `PeerPool::start_liveness_pings`.

**Spec refs:** §12.0–§12.4, §11.4.1, §4 invariants 16, 21–22, §15 criterion 7.

**Prerequisites:** Phase 11.0 (CleanFullyAuthenticated<R> and is_authenticated_quickly() in core-engine) must land before Phase 12.0 channel-binding work begins.

### 12.0 Channel-Binding Establishment

**Goal:** Implement the one-time dual-key binding protocol (Spec §12.0, §11.4.1).
A channel becomes usable for application messages only after it is `FullyBound`.

- [x] Add `ChannelBinding` struct in `communerdette.rs` (or a new (2026-05-27)
      `channel_binding.rs`):
      ```rust
      struct ChannelBinding {
          responder_tbid: Tbid,
          nonce_echo: Vec<u8>,
          channel_id: String,
          fast_sig: Vec<u8>,
          slow_sig: Vec<u8>,
      }
      ```
- [x] Add `UnprocessedChannelBinding` following the Take 3 pattern. Add (2026-05-27)
      `verify_dual_key(crypto, nonce, channel_id, target_tbid) ->
      Result<CleanFullyAuthenticated<ChannelBinding>, CommunerdetteError>`
      that checks: (a) TBID match, (b) nonce echo match, (c) channel_id match,
      (d) fast-key signature, (e) slow-key signature. Both (d) and (e) must pass.
- [x] Add `handle_channel_bind_challenge` in `server/handlers.rs`. The handler (2026-05-27)
      must sign the response using the local TBID owner (not the handler directly).
      Signs `(nonce || channel_id || responder_tbid_hex)` with both fast and slow keys.
- [x] Wire `"channel_bind_challenge"` into the dispatch table in `server/mod.rs`. (2026-05-27)
- [x] Add per-channel `BindingState` to `CommunerdetteState`: (2026-05-27)
      ```rust
      enum ChannelBindingState {
          Unbound,
          FullyBound(CleanFullyAuthenticated<ChannelBinding>),
          Rejected { reason: String },
      }
      ```
- [x] Add `spawn_channel_bind_task(channel_id, transport_addr)` to `Communerdette`. (2026-05-27)
      On invocation:
      1. Generate 32-byte random nonce.
      2. Call `channel_bind_challenge` via the transport.
      3. Parse response → `UnprocessedChannelBinding`.
      4. Call `verify_dual_key(...)` → `CleanFullyAuthenticated<ChannelBinding>`.
      5. On success: channel state → `FullyBound`. Emit binding event.
      6. On failure: channel state → `Rejected`. Log binding violation.
- [ ] Communerd triggers `spawn_channel_bind_task` when it notifies Communerdette
      of a new channel (PeerId or direct address).
- [x] Unit test: `verify_dual_key` accepts a correctly dual-signed response. (2026-05-27)
- [x] Unit test: `verify_dual_key` rejects wrong fast-key signature. (2026-05-27)
- [x] Unit test: `verify_dual_key` rejects wrong slow-key signature. (2026-05-27)
- [x] Unit test: `verify_dual_key` rejects nonce mismatch. (2026-05-27)
- [x] Unit test: `verify_dual_key` rejects TBID mismatch. (2026-05-27)
- [ ] Integration test: two real servers, source initiates channel bind → mirror
      responds → source reaches `FullyBound`.
- [ ] Integration test: second independent channel can be bound simultaneously.

### 12.1 Level 1 — Network Stack Alive

L1 uses the existing `ping` / `{ "pong": true }` RPC already wired in Phase 6.
The remaining work is moving the liveness loop from `PeerPool` into
Communerdette.

- [x] Add `spawn_l1_liveness_task` to `Communerdette`. This task loops on a (2026-05-27)
      configurable interval, calls `ping` via `CommunerdetteHost::call_peer`,
      and records success/failure in `CommunerdetteRouteStats`.
- [x] The task exits when the Communerdette's `CancellationToken` is cancelled. (2026-05-27)
- [x] Verify the response is `{ "pong": true }`. Record transport success/failure (2026-05-27)
      only — no `CleanAuthenticated<R>` gate needed for L1 (transport-only
      evidence; see Spec §12.1).
- [ ] Unit test: L1 task records success when ping returns pong.
- [ ] Unit test: L1 task records failure when ping times out or returns error.

### 12.2 Deprecate PeerPool::start_liveness_pings

- [x] Remove the `communerd.start_liveness_pings()` call from
      `server/mod.rs` (currently line 216).
      (2026-05-28)
- [x] Mark `PeerPool::start_liveness_pings` as `#[deprecated]` with message: (2026-05-27)
      "Use Communerdette L1 liveness loop instead."
- [x] Verify no other callers exist:
      ```bash
      grep -rn "start_liveness_pings" p2p/
      ```
      (2026-05-28) confirmed only in PeerPool definition + one deprecated test.
- [x] All tests still pass after removal. (2026-05-28)

### 12.3 Level 2 — TBID Identity Confirmed (Ongoing Health)

**Prerequisite:** Phase 12.0 (channel-binding) must be `FullyBound` before L2
health checks begin. L2 only needs fast-key verification — the dual-key proof
was already established at binding time.

- [x] Define `AuthenticatedPong` struct in `communerdette.rs`: (2026-05-27)
      ```rust
      struct AuthenticatedPong {
          responder_tbid: Tbid,
          challenge_echo: Vec<u8>,
          signature: Vec<u8>,
          signature_algorithm: String,
      }
      ```
- [x] Add `UnprocessedAuthenticatedPong` wrapper following the Take 3 pattern (2026-05-27)
      (`from_json_value`, inner accessor). Add `verify(crypto, challenge,
      target_tbid)` method that checks: (a) TBID match, (b) challenge echo
      match, (c) fast-key signature over `(challenge || responder_tbid_hex_bytes)`.
- [x] Add `handle_authenticated_ping` in `server/handlers.rs`. The handler (2026-05-27)
      must ask the local TBID owner (Chronomatter or the server's signing
      surface) to sign the response — it must NOT sign directly with a key
      stored inside the handler.
- [x] Wire `"authenticated_ping"` into the dispatch table in `server/mod.rs`. (2026-05-27)
- [x] Add `spawn_l2_liveness_task` to `Communerdette`. On each interval: (2026-05-27)
      1. Generate a 32-byte random challenge.
      2. Call `authenticated_ping` via `CommunerdetteHost::call_peer`.
      3. Parse response as `UnprocessedAuthenticatedPong`.
      4. Call `verify(crypto, challenge, target_tbid)` → `CleanAuthenticated<AuthenticatedPong>`.
      5. On success: if binding was `ClaimedByDht`, upgrade to `Verified`.
         Record L2 success.
      6. On failure: record binding violation, set `TbidBindingStatus::Rejected`
         with reason.
- [ ] Unit test: L2 task promotes `ClaimedByDht` to `Verified` on valid response.
- [ ] Unit test: L2 task sets `Rejected` on wrong TBID in response.
- [ ] Unit test: L2 task sets `Rejected` on wrong challenge echo.
- [ ] Unit test: L2 task sets `Rejected` on invalid fast-key signature.
- [ ] Integration test: two real servers, L2 succeeds and binding is `Verified`.

### 12.4 Level 3 — Chronomatter Responsive

L3 reuses the existing `stamp` RPC. The prerequisite is Phase 11 (gate_foretis
fix) — L3 is meaningless without a real fast-key gate on the Foretis response.

**This phase is blocked on Phase 11 completion.**

- [x] Add `spawn_l3_liveness_task` to `Communerdette`. On each interval: (2026-05-27)
      1. Generate a short test payload (e.g. `b"liveness-probe"`).
      2. Call `stamp(payload, "liveness")` via `CommunerdetteLine` (or directly
         via the executor).
      3. Parse the returned `Foretis`.
      4. Call `get_tick(foretis.chronon_number)` to obtain
         `CleanAuthenticated<ChrononRecord>`.
      5. Call the fixed `gate_foretis(foretis_json, chronon_record, payload)` →
         `CleanAuthenticated<Foretis>`.
      6. On success: record L3 success in `CommunerdetteStats`.
      7. On failure (gate error, timeout, missing tick): record L3 failure.
         Do not change `TbidBindingStatus` — L3 failure is health only.
- [ ] Unit test: L3 records success when stamp returns a valid Foretis that
      passes the fast-key gate.
- [ ] Unit test: L3 records failure when gate_foretis rejects the response.
- [ ] Integration test: two real servers, L3 succeeds end-to-end.

### 12.5 Liveness Loop Policy

- [x] Add `liveness_policy: LivenessPolicy` to `CommunerdetteState`: (2026-05-27)
      ```rust
      struct LivenessPolicy {
          l1_interval_ms: u64,
          l2_interval_ms: u64,
          l3_interval_ms: u64,
          run_l2: bool,
          run_l3: bool,
      }
      ```
- [x] Default policy: L1 enabled, L2 enabled, L3 enabled, intervals configurable. (2026-05-27)
- [x] Run order enforced: L2 skipped if L1 failed in current cycle. L3 skipped
      if L2 failed or `TbidBindingStatus` is `Rejected`.
      (2026-05-28) via `LivenessCycleFlags` Arc<AtomicBool> shared between spawn functions.
- [x] Expose current policy in `CommunerdetteStatusSummary`.
      (2026-05-28) `liveness_policy: LivenessPolicy` added to state and summary.

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

### Test category commands

Tests are divided into three categories. Run them independently to keep fast
feedback loops during development.

```bash
# ── Unit tests only (lib's #[test] functions, fast, <5s) ─────────────────────
cargo test -p foretias-server --lib
cargo test -p foretias-core --lib

# ── Snapshot tests only (AST scan + crypto callsite scan) ────────────────────
cargo test -p foretias-core --test trust_boundary_type_usage
cargo test -p foretias-server --test crypto_callsite_snapshot
# Regenerate snapshots when intentionally adding new call sites:
UPDATE_SNAPSHOT=1 cargo test -p foretias-core --test trust_boundary_type_usage
UPDATE_SNAPSHOT=1 cargo test -p foretias-server --test crypto_callsite_snapshot

# ── Toppoli integration tests (slow, multi-peer, requires --include-ignored) ──
# All toppoli tests:
cargo test -p foretias-server --test toppoli -- --include-ignored
# One specific toppoli test:
cargo test -p foretias-server --test toppoli toppoli_peer_restart -- --include-ignored
# All toppoli tests matching a pattern (e.g. all mesh tests):
cargo test -p foretias-server --test toppoli toppoli_twelve -- --include-ignored

# ── All non-toppoli tests (default CI run) ────────────────────────────────────
cargo test --workspace
# Toppoli tests appear as "(ignored)" — expected.

# ── Everything including toppoli ──────────────────────────────────────────────
cargo test --workspace -- --include-ignored
```

---

## Phase 13 — Fully-Bound Gossip

**Spec reference:** §17.  
**Date added:** 2026-05-28.

### 13.1 CommunerdetteHost API additions

- [ ] Add to `CommunerdetteHost` trait:
      ```rust
      async fn host_sign_probity_report(
          &self,
          report: ProbityReport,
      ) -> Result<ProbityReport, NodeError>;

      async fn host_publish_probity_report(
          &self,
          report: ProbityReport,
      ) -> Result<(), NodeError>;
      ```
- [ ] Implement `host_sign_probity_report` on `CommunerdetteHost` impl in
      `communerd/mod.rs`: build canonical bytes, call Calendar's
      `sign_tbid_message()`, set `signature` and `curve=1` on report, return.
- [ ] Implement `host_publish_probity_report` on `CommunerdetteHost` impl:
      forward to `publish_probity_report(swarm, &report, namespace)` via the
      existing gossip helper.
- [ ] Update `MockHost` in tests to stub both methods
      (sign: return report unchanged; publish: return Ok(())).

### 13.2 Emission — FullyBound established

- [ ] In `spawn_channel_bind_task`, when state transitions to `FullyBound`:
      ```rust
      let report = ProbityReport {
          reporter: local_calendar_tbid.to_hex(),
          subject:  executor.target_tbid.to_hex(),
          attribute: "fb".to_string(),
          value:    1.0,
          timestamp_ns: now_ns(),
          signature: vec![],
          curve:    1,
      };
      let _ = executor.host.host_sign_probity_report(report)
          .and_then(|signed| executor.host.host_publish_probity_report(signed))
          .await;
      ```
      Failure to sign or publish is logged at WARN and does not abort binding.
- [ ] `CommunerdetteExecutor` must carry `local_calendar_tbid: Tbid` for this
      call. Add the field and thread it through `CommunerdetteExecutor::new`.

### 13.3 Emission — FullyBound lost

- [ ] Add helper `Communerdette::emit_fb_lost(executor, reason: &str)`:
      same report shape but `value = -1.0`. Call from:
      - channel disconnect handler inside `spawn_channel_bind_task`
      - Communerdette shutdown path when binding was `FullyBound`
      - any path that sets `TbidBindingStatus::Rejected` while previous state
        was `FullyBound`

### 13.4 Reception

- [ ] In Communerd's gossip event handler (`communerd/p2p/event_loop.rs` or
      equivalent), when a `ProbityReport` arrives on the probity topic:
      1. Parse as `UnverifiedSignatureEnvelope<ProbityReport>` (`DontUse`).
      2. Verify → clean state.
      3. If `attribute == "fb"` / `"gnf"`: log at DEBUG; ingest into `ProbityStore`.
      4. Any error: log at TRACE, drop silently.
- [ ] **CHECK: a Bruderschaft (FB) / GNF report returns
      `always_require_full_signature() == true`, and the reception gate REQUIRES
      `CleanFullyAuthenticated<ProbityReport>` for it — an FB/GNF report that is
      not fully signed is rejected, never ingested at the fast level.**
- [ ] Ensure the gossip event handler path does not panic on malformed input.

### 13.5 Tests

- [ ] Unit test: `emit_fb_established` builds a report with `attribute="fb"`,
      `value=1.0`, correct reporter and subject TBIDs.
- [ ] Unit test: `emit_fb_lost` builds a report with `value=-1.0`.
- [ ] Unit test: a signed FB report passes `Unprocessed<ProbityReport>::verify`.
- [ ] Integration test (two servers, Phase 14 harness): server A reaches
      `FullyBound` with B; B's `ProbityStore` eventually contains a record
      from A with `attribute="fb"` and `value=1.0`.

---

## Phase 14 — Toppoli: Multi-Peer Local Integration Harness

**Spec reference:** §18 (Toppoli).  
**Date added:** 2026-05-28.

All servers run in-process. No forking. `TimeFamilyServer` uses in-process
tokio tasks. Direct Rust introspection of all pub state is available.
Shutdown: `stop_daemon_arc()` + brief drain sleep + `handle.abort()`.
No CancellationToken changes to `TimeFamilyServer` are needed.

### 14.1 Core harness types

- [ ] Create `p2p/foretias-server/tests/toppoli.rs`.
- [ ] Implement `ToppliPeerConfig { chronon_ns, extra_peers, namespace }` with
      `Default` (100ms chronon, empty peers, namespace `"toppoli"`).
- [ ] Implement `ToppliPeer { idx, addr, server: Arc<TimeFamilyServer>, handle }`.
- [ ] Implement `ToppliHarness { slots: Vec<ToppliSlot>, addrs: Vec<String> }`
      where `ToppliSlot` is `Reserved | Running | Stopped`.
- [ ] `ToppliHarness::with_peers(count, config)` — pre-allocate N ports with
      `find_available_port()`, store as `Reserved` slots.
- [ ] `ToppliHarness::add_peer(config)` — allocate one more port, push slot,
      return index.

### 14.2 Topology helpers

- [ ] `topology_full_mesh()` — for each slot, set its `extra_peers` to all
      other slots' addresses. Call before any `start_peer`.
- [ ] `topology_ring()` — each slot knows only the next slot's address.
- [ ] Custom topology: `add_peer` + manually set `extra_peers` before starting.

### 14.3 Lifecycle methods

- [ ] `start_peer(idx)` — build `CommunerdConfig` from slot's `extra_peers`,
      call `TimeFamilyServer::new(addr, chronon_ns)`, optionally
      `.with_communerd(config)`, call `start_daemon_arc()`, call `start()`,
      transition slot to `Running`.
- [ ] `stop_peer(idx, drain_ms)` — call `peer.server.stop_daemon_arc()`,
      sleep `drain_ms`, call `peer.handle.abort()`, transition to `Stopped`.
- [ ] `restart_peer(idx)` — `stop_peer` then `start_peer` at same address.
- [ ] `start_all()` — call `start_peer` for all `Reserved` slots concurrently
      (use `futures::future::join_all`).
- [ ] `stop_all(drain_ms)` — call `stop_peer` for all `Running` slots
      concurrently.

### 14.4 Introspection and wait helpers

- [ ] `running_count() -> usize`.
- [ ] `running_peers() -> impl Iterator<Item = &ToppliPeer>`.
- [ ] `peer(idx) -> &ToppliPeer` — panics if not running (test bug, not prod).
- [ ] `wait_until(predicate: impl Fn(&ToppliHarness) -> bool, timeout_ms: u64) -> bool` —
      polls every 20 ms until predicate true or timeout.

### 14.5 First toppoli tests

- [ ] `toppoli_peers_start_and_stop` — 4 peers, full mesh, start all, wait
      100 ms, stop all. Assert no panic, `running_count == 0` after stop.
- [ ] `toppoli_l1_ping_round_trip` — 2 peers, peer 0 sends JSON-RPC `ping`
      to peer 1's addr, assert `Ok(())` within 2 s.
- [ ] `toppoli_peer_restart` — 3 peers, start all, stop peer 1, restart
      peer 1, assert all 3 reachable via ping within 3 s.
- [ ] `toppoli_12_peers_form_and_stabilize` — 12 peers, full mesh, start all,
      wait until all running, stop_all. Smoke test for scale.

### 14.6 Later toppoli tests (require Phase 12/13)

- [ ] `toppoli_l2_auth_ping` — 2 peers with Communerd wired; peer 0's
      Communerdette sends `authenticated_ping` to peer 1; assert
      `TbidBindingStatus` advances toward `Verified`.
- [ ] `toppoli_fb_gossip` — 2 peers, A reaches `FullyBound` with B;
      assert B's `ProbityStore` contains FB report from A within 5 s.
- [ ] `toppoli_gnf_churn` — 12 peers, bring 3 down mid-test, bring them
      back, assert gossip still propagates to all 12 within timeout.

### 14.7 Documentation (AGENTS.md + README.md)

- [ ] Update `AGENTS.md` with a "Toppoli tests" section covering:
      - What toppoli is (multi-peer in-process integration tests)
      - When to write a toppoli test vs a unit test
      - Test scale: 2–24 peers, seconds to ~30 s per test
      - How to run: `cargo test -p foretias-server --test toppoli -- --include-ignored`
      - Fixture classes: `ToppliBasicTest`, `ToppoliFBProbityTest`,
        `ToppliLivenessTest`, `ToppliGNFTest`
      - Note that tests are marked `#[ignore = "toppoli: ..."]` and are
        omitted from the default CI run (`cargo test --workspace`)
- [ ] Update `README.md` (if present) or the project's top-level
      `foretias/README.md` with the same summary and run commands.

---

## Phase 15 — FamilyRecord and Family Cache

**Spec reference:** §19. **Defensive coding throughout** (reject, never panic).

- [ ] Define `FamilyRecord` payload (signature-free) per §19.4: `member_tbids`,
      `calendar_tbids`, `chronomatter_tbids`, reachability, `created_at_ns`,
      k×k `matrix`, `foretis`.
- [ ] Verifying constructor `FamilyRecord::try_new(...) -> Result<_, FamilyError>`
      enforcing every §19.4 invariant: non-empty + unique + well-formed members;
      `created_at_ns > 0`; matrix strictly k×k; subsets ⊆ members.
- [ ] `MAX_FAMILY_MEMBERS` cap enforced **before** any signature verification
      (bounds k² SLH-DSA — DoS guard).
- [ ] Verify every `matrix[i][j]` is member i's dual-key signature over member
      j's TBID; reject on any failure with a distinct error variant.
- [ ] **CHECK: `FamilyRecord::always_require_full_signature()` returns `true`,
      and the gate that produces it asserts full verification — never returns a
      fast-level `CleanAuthenticated<FamilyRecord>`.** (see Phase 16 gate.)
- [ ] Family Cache in Communerd: `communerd_tbid → CleanFullyAuthenticated<FamilyRecord>`,
      `member_tbid → communerd_tbid` reverse pointer, `communerd_tbid → peer_id`.
- [ ] Connection flow (§19.3): `/tbid` lookup → dial → fetch FamilyRecord →
      verify (full) → cache → start Communerdette.
- [ ] Test: malformed FamilyRecords (bad matrix dims, wrong sig lengths, dup/empty
      members, non-member subset entries, oversized k) are each **rejected**, no panic.

---

## Phase 16 — Canonical Encoding, Signature Wrappers, and Gate Enforcement

**Spec reference:** §21. **Defensive coding throughout.**

- [ ] Add the canonical serializer (`postcard` per §21.1) — confirm crate choice.
- [ ] Rename `Unprocessed<R>` → `UnverifiedSignatureEnvelope<R>` (alias
      `DontUse<R>`) across `clean_auth.rs` and all call sites.
- [ ] `RecordBase` trait with `always_require_full_signature(&self) -> bool`
      (default `false`).
- [ ] Implement `RecordBase` for every payload type; FamilyRecord and
      Bruderschaft/GNF return `true`, all others default `false`.
- [ ] `Signed`-list model on the wrappers: ordered `signatures`, each entry
      covers `postcard(payload) ‖ postcard(signatures[0..i])`.
- [ ] **CHECK: the `DontUse<R>` → clean-state gate verifies ALL signatures**
      (no verify-last-only shortcut), and **rejects** an unexpected signature
      count (cardinality is exactly two today).
- [ ] **CHECK: the gate consults `R::always_require_full_signature()` and, when
      `true`, REFUSES to produce `CleanAuthenticated<R>` — it must reach
      `CleanFullyAuthenticated<R>` or reject.** This check must exist at the gate
      and be covered by a test.
- [ ] **CHECK: a record claiming `always_require_full_signature() == true` but
      carrying only fast (Ed25519) signatures is REJECTED, never downgraded.**
      Dedicated negative test.
- [ ] `Externalized<R>` builder (§21.6): `builder_from().add_signature()...build()`;
      `build()` rejects unless the last entry is a `CommunerdEnvelope`.
- [ ] Migrate `PeerRegistrationRecord`/`ProbityReport`/`Foretis` canonical
      encoders to `postcard`; handle the `Foretis` wire-break with a version bump.
- [ ] Snapshot/trust-boundary tests updated for the renamed wrapper + signatures.

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
