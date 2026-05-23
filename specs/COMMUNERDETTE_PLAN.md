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

### 1.5 Clean Authenticated Remote Records

- [ ] Define `CleanAuthenticated<R>` or the final equivalent typed wrapper for
      remote records authenticated by Communerdette.
- [ ] Keep `CleanAuthenticated<R>` constructors private to `communerd` or to a
      narrow verifier helper owned by `communerd`.
- [ ] Define the proof/metadata shape needed to explain how the remote TBID was
      authenticated.
- [ ] Ensure raw transport replies cannot be stored as trust-bearing evidence
      without first passing through Communerdette authentication.

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

- [ ] Implement `CommunerdetteLine::get_calendar_slice(start, count)`.
- [ ] Implement `CommunerdetteLine::get_tick(tick_number)` as
      `get_calendar_slice(tick_number, 1)` plus exactly-one validation.
- [ ] Return `CleanAuthenticated<Vec<TickRecord>>` and
      `CleanAuthenticated<TickRecord>` from these methods after verifying the
      remote TBID and record signatures.
- [ ] Enforce the existing `MAX_CALENDAR_SLICE_COUNT` behavior on the server
      side; do not duplicate trust in the caller.
- [ ] Add timeout behavior and explicit timeout errors.

### 4.2 Stamp Method

- [ ] Implement `CommunerdetteLine::stamp(content, echo)`.
- [ ] Internally hex-encode content for the existing JSON-RPC `stamp` method.
- [ ] Return parsed `Foretis` as `CleanAuthenticated<Foretis>` after verifying
      that the remote stamp is authenticated for the target TBID.
- [ ] Preserve dormant-node error behavior from the server.
- [ ] Document that this method asks the remote TBID to stamp supplied content;
      it does not sign local Calendar or Chronomatter messages.

### 4.3 Compatibility Wrappers

- [ ] Refactor `Communerd::route_stamp` to use `line_for_tbid(...).stamp(...)`
      where practical.
- [ ] Add `Communerd::get_calendar_slice_by_tbid(tbid, start, count)` as a
      convenience wrapper over `CommunerdetteLine`.

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

- [ ] Define private `CommunerdetteCommand`.
- [ ] Define `CommunerdettePriority` with `Critical`, `High`, `Normal`,
      `Bulk`.
- [ ] Each queued request carries a response oneshot and timeout.

### 5.2 Queue Worker

- [ ] Spawn a queue worker per Communerdette only when needed.
- [ ] Worker chooses route, executes request, records stats, and completes the
      response channel.
- [ ] Ensure worker exits on Communerdette shutdown.
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

- [ ] Decide whether liveness uses transport-native libp2p ping, JSON-RPC
      `ping`, or a small Foretias status RPC.
- [ ] If JSON-RPC `ping` remains in `PeerTransport`, implement a server-side
      `ping` handler in `server/handlers.rs` and dispatch it in `server/mod.rs`.
- [ ] If transport-native ping is preferred, stop using JSON-RPC `ping` for
      direct liveness or document the split clearly.

### 6.2 Stats Recording

- [ ] Record per-route success/failure counts.
- [ ] Record last success/failure time.
- [ ] Record smoothed RTT.
- [ ] Record consecutive failures and backoff state.
- [ ] Expose read-only `CommunerdetteStatusSummary`.

### 6.3 Existing PeerPool Migration

- [ ] Keep `PeerPool` as the address/peer collection during migration.
- [ ] Add a bridge that updates Communerdette state when DHT or PeerPool state
      changes.
- [ ] Avoid duplicated infinite ping loops for the same TBID.
- [ ] After tests pass, decide whether `PeerPool::start_liveness_pings` should
      be deprecated or reduced to discovery-only bookkeeping.

### 6.4 Tests

- [ ] Unit test stats update after success.
- [ ] Unit test stats update after failure.
- [ ] Integration test liveness probe against reachable and unreachable peers.

---

## Phase 7 - TBID Binding Proof Hook

**Goal:** Represent the trust boundary and create clean authenticated remote
records even if full binding proof is implemented later.

### 7.1 Binding State Enforcement

- [ ] Ensure DHT records produce `ClaimedByDht`, not `Verified`.
- [ ] Ensure trust-bearing operations can inspect binding status.
- [ ] Ensure mutual-attestation storage code can reject or mark unverified
      bindings according to policy.

### 7.2 Proof API Skeleton

- [ ] Add private `Communerdette::request_tbid_binding_proof()`.
- [ ] Add private `Communerdette::verify_tbid_binding_proof(...)`.
- [ ] Return `Unsupported` or leave disabled if proof format is not in scope.
- [ ] Document exact future proof transcript in comments or follow-up spec.

### 7.3 Clean Authentication API

- [ ] Add private helper(s) that transform untrusted parsed remote records into
      `CleanAuthenticated<R>` after verifying TBID, signature/hash material,
      and operation-specific freshness rules.
- [ ] Ensure failed authentication returns an explicit error and does not expose
      a partially trusted value to Calendar or Chronomatter-adjacent code.
- [ ] Ensure `CleanAuthenticated<R>` records retain enough metadata for
      diagnostics without exposing mutable authentication internals.

### 7.4 Tests

- [ ] Unit test DHT-only binding is not accidentally marked verified.
- [ ] Unit test rejected binding disables trust-bearing requests.
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
      `CleanAuthenticated<TickRecord>` before local content verification uses
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

- [ ] Add line methods or wrappers for existing mirror RPCs:
      `mirror_request`, `ship_batch`, `ship_ack`, `stream_tick`,
      `mirror_reconcile`.
- [ ] Ensure stream and mirror outputs expose clean authenticated records or
      batches, never raw remote stream items.
- [ ] Keep bulk history dumps at `Bulk` priority.
- [ ] Keep verification-related tick fetches at `High` priority.

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

- [ ] Add `Communerd::shutdown_relationships()` or integrate with existing
      server shutdown path.
- [ ] Ensure every Communerdette task has a `JoinHandle` or cancellation token.
- [ ] Ensure dropping a line does not kill the underlying relationship while
      Communerd still owns it.
- [ ] Ensure Communerd shutdown stops liveness, queue, and stream tasks.
- [ ] Test shutdown with pending requests.

---

## Phase 10 - Documentation and Cleanup

- [ ] Update `COMMUNERDETTE_SPEC.md` if implementation decisions diverge.
- [ ] Update relevant P2P docs to refer to CommunerdetteLine as the
      TBID-scoped API for Calendar/TimeFamily callers.
- [ ] Document that Communerdette is private and CommunerdetteLine is the
      exposed capability handle.
- [ ] Document the signing boundary: TBID-owning Rust objects sign their own
      messages; Communerdette transports signed payloads and authenticates
      routes but does not sign for other objects.
- [ ] Document the remote authentication boundary: Communerdette authenticates
      the other TBID and emits `CleanAuthenticated<R>` for higher layers.
- [ ] Document the current TBID binding proof status: `ClaimedByDht` only or
      fully `Verified`.
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
