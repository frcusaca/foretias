# Communerdette - Per-TBID Relationship Handle Specification

**Prefix:** `COMMUNERDETTE`
**Group:** COMBINED_GROUP7
**Pairs with:** `COMBINED_GROUP7_COMMUNERDETTE_PLAN.md`
**Status:** Draft - updated 2026-05-26, pending human review
**Date:** 2026-05-23 (updated 2026-05-26 — authentication model, liveness levels, key model)
**Master Coordination:** `COMBINED_GROUP2_SPEC.md` §2.2

**Downstream dependency (2026-05-26):** This specification is now a
**hard prerequisite** for the remaining mirror work in Group 4 (active
mirroring StartStream / DoAttestation refactor / graceful shutdown +
all of proof-of-storage) and for mutual attestation work in Group 6
(`COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md`). Both of those groups
have been explicitly DEFERRED pending this spec's feature completion.
Treat Communerdette's interface stability — particularly
`MirrorDispatcher`, `CommunerdetteLine`, and the per-TBID lifecycle
state machine — as load-bearing for those downstream specs.

---

## Completed Prerequisites

The following specifications are code-complete and their implementations are
available on `alpha`. The Communerdette design depends on these types and
constructors being present:

- **`COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md`** — Defines
  `Unprocessed<T>`, `CleanAuthenticated<T>`, and `Externalized<T>` as generic
  wrappers in `core-engine/src/foretias/clean_auth.rs`. No concrete newtype
  structs. `CleanAuthenticated<T>` has private constructors:
  `from_trusted()` for locally created data and `verify()` on
  `Unprocessed<T>` for the inbound gate. Type aliases via
  `create_type_gated_classes!` macro (e.g., `CleanAuthenticatedChrononRecord`
  is an alias for `CleanAuthenticated<ChrononRecord>`). `ProbityReport` moved
  to `core-engine`.

---

## Reading Order

1. Re-read `FORETIAS_2_IMPLEMENTATION_PLAN.md` lines describing component
   boundaries: Communerd is the only component with extra-family network access.
2. Re-read `FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` sections 6.2
   and 6.3: Calendar owns mutual-attestation scheduling; remote peers only see
   ordinary `stamp` calls.
3. Re-read `COMMUNERD_LIBP2P_DIRECT_SPEC.md`: libp2p direct transport is the
   preferred RPC pipe when a PeerId is known; custom Noise_XX TCP remains the
   fallback.
4. Read `COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md` to
   understand the `CleanAuthenticated<R>` type used throughout this spec.
5. Read this document end to end before implementing.

---

## 1. Goal

Introduce a **Communerdette** as Communerd's private per-external-TBID
relationship manager, and expose a narrow **CommunerdetteLine** handle to
in-family components.

Communerdette owns the relationship policy for one external TBID: what to do
to maintain communication, when to retry, which route to prefer, how healthy
the relationship looks, and how to serialize requests for that TBID. Communerd
tracks Communerdettes and remains the active network participant: it owns peer
discovery, DHT work, swarm behavior, transport pools, and the node's role in
the wider P2P network.

The line lets Calendar, Chronomatter-adjacent orchestration, and TimeFamily code
ask a single external TBID for network services without knowing transport
details:

- fetch a tick or calendar slice for local verification
- request a stamp for mutual attestation
- initiate or maintain mirror/stream operations
- receive read-only relationship status when needed

Communerd remains the only component with network access. CommunerdetteLine is
only a scoped proxy into Communerd-owned state.

---

## 2. Problem

Communerd already owns the raw pieces needed for per-peer communication:

- DHT lookup and TBID registration records
- libp2p PeerIds and request_response RPC
- custom Noise_XX TCP JSON-RPC fallback
- peer pool and liveness pings
- probity reports
- heartbeat/collision traffic
- request timeouts and transport fallback

Those concerns are currently spread across `Communerd`, `PeerPool`, transport
implementations, server handlers, and call sites. Calendar and verification
logic still have to think in terms of peer records, addresses, PeerIds, or
transport-specific calls.

The desired abstraction is TBID-addressed:

```text
Calendar/TimeFamily:
  "For TBID T, fetch tick N."

CommunerdetteLine(T):
  "I can do that."

Communerdette(T):
  resolves, binds, chooses transport, queues, awaits, retries, records stats, authenticate responses.
```

---

## 3. Terminology

| Term | Meaning |
|------|---------|
| Communerd | Global extra-family communication owner. Owns swarm participation, transports, DHT state, peer discovery, and the Communerdette registry. Routes all inbound traffic from a remote TBID to the owning Communerdette for authentication. |
| Communerdette | Private per-external-TBID relationship manager. Owns the full health and authentication picture for exactly one remote TBID: liveness at all three levels, inbound authentication gate, outbound message wrapping, routing, retries, queues, stats, and binding status. |
| CommunerdetteLine | Public, cloneable, narrow proxy for Calendar/Chronomatter/TimeFamily use. It physically points at a Communerdette but exposes only safe TBID-scoped operations. |
| External TBID | A TBID not owned by the local family. Each Communerdette is scoped to exactly one external TBID. |
| Local TBID owner | The one Rust object that owns a local TBID and its signing capability. Only that object may sign for that TBID. |
| Binding | Evidence that a transport identity, such as libp2p PeerId or direct Noise key/address, is authorized to speak for a TBID. |
| Route | A currently usable way to reach the external TBID: libp2p direct, custom Noise_XX TCP, or unavailable. |
| `CleanAuthenticated<R>` | A remote record `R` that Communerdette has parsed and authenticated against the target external TBID's fast key. Higher layers may consume it as authenticated remote evidence, but not as proof that the remote statement is semantically true. Carries `is_authenticated_quickly() -> bool` (always `true`) as a runtime-checkable guarantee of fast-key authentication. |
| `CleanFullyAuthenticated<R>` | A remote record `R` authenticated against **both** the fast key and the slow key of the target TBID. Subsumes `CleanAuthenticated<R>` — a `CleanFullyAuthenticated<R>` converts to `CleanAuthenticated<R>` and may be used wherever one is accepted. Carries `is_authenticated_quickly() -> bool` (true, fast-key verified) and `is_authenticated_fully() -> bool` (true, slow-key also verified). Required for initial channel-binding: the remote must produce a `CleanFullyAuthenticated<ChannelBinding>` the first time Communerdette establishes a channel to a new TBID. |
| `ChannelBinding` | The message signed during initial channel-binding establishment. Contains the remote TBID, a nonce challenge, and the transport identity (libp2p PeerId, ip/port, or other). Must be dual-key signed (fast + slow) by the remote. Becomes `CleanFullyAuthenticated<ChannelBinding>` after verification. |
| `Externalized<R>` | A wrapper for data that is leaving the local trust boundary. Every message Communerdette sends to the remote TBID must be wrapped in `Externalized<R>` before dispatch. Only locally produced or already-`CleanAuthenticated` data may be externalized. |
| Fast key | The faster of the two signing keys embedded in a TBID (Ed25519 in current implementations). Used to sign every message. All inbound remote messages must be verified against the remote TBID's fast key before being promoted to `CleanAuthenticated<R>`. |
| Slow key | The slower of the two signing keys embedded in a TBID (PQC/SPHINCS+ in future implementations). A message may carry an additional slow-key signature alongside the mandatory fast-key signature. Absence of a slow-key signature is acceptable. An incorrect slow-key signature is a hard rejection. No current protocol operation requires slow-key verification; the use case is deferred. |
| Liveness level | One of three grades of relationship health check, all owned by Communerdette. L1: network stack alive (Noise/TCP transport responsive). L2: TBID identity confirmed (remote signs a challenge with its fast key). L3: Chronomatter responsive (remote stamps content; returned Foretis passes full fast-key verification). |

---

## 4. Design Invariants

1. **Communerd remains the only network authority.** Calendar and Chronomatter
   do not receive raw transport objects, PeerPool mutation access, or swarm
   command channels.
2. **One Communerdette, one external TBID.** A CommunerdetteLine must never send
   a request to a different TBID than the one it was created for.
3. **Communerd owns global discovery.** Communerdette may ask Communerd for help
   resolving or refreshing routes to its target TBID, but it does not own the
   DHT, global peer discovery, swarm, or transport pools.
4. **Communerdette body is private.** Internal methods for liveness, retries,
   route choice, stream health, stats mutation, and binding updates are
   available only to Communerd or the communerd module internals.
5. **CommunerdetteLine is a capability handle.** Holding a line grants only the
   ability to call the exposed TBID-scoped async methods.
6. **Transport identity is not TBID identity.** DHT records and libp2p PeerIds
   are claims until bound to a TBID by an application-level proof. The design
   must represent unverified and verified binding states distinctly.
7. **TBID signing authority is not transport authority.** A TBID belongs to one
   Rust object, and only that object may sign for that TBID. Communerd and
   Communerdette may authenticate transport sessions, verify signed payloads,
   route requests, and retry delivery, but they must not sign for Calendar,
   Chronomatter, or any other TBID-owning object.
8. **Delegated sends carry already-signed payloads.** If Calendar sends a
   message for Chronomatter, Calendar must ask Chronomatter through an internal
   API to sign that message. Calendar then gives the signed payload to
   Communerdette for delivery. Communerdette does not infer authority from the
   caller and does not borrow another object's TBID key.
9. **Remote TBID authentication is Communerdette's job.** Raw remote replies
   enter Communerdette as untrusted data. Communerdette uses Foretias crypto
   verification rules and the current TBID binding state to authenticate the
   remote TBID, reject bad data, or produce `CleanAuthenticated<R>` records.
   Calendar and Chronomatter should not repeat ad hoc remote-authentication
   parsing at call sites.
10. **Clean authenticated is not locally signed.** `CleanAuthenticated<R>`
    proves the remote record passed Communerdette's authentication checks for
    the external TBID relationship. It does not mean Communerdette signed
    anything, and it does not give Communerdette signing authority for local
    TBIDs.
11. **Request/response calls await bounded responses.** Stamp, verify-style
   evidence fetches, and calendar slice requests must have timeouts. No request
   may wait forever.
12. **Local verification remains local.** Prefer fetching signed public evidence
   such as `ChrononRecord` and verifying locally over asking a remote peer for a
   true/false answer.
13. **Fallback is a relationship policy.** Transport fallback is decided using
   the Communerdette's route state and stats, not repeated ad hoc at call sites.
14. **Tasks have owners and shutdown paths.** Per-TBID liveness, queue, and stream
   tasks are owned by the Communerdette and stopped by Communerd shutdown or
   relationship eviction.
15. **Existing transports remain valid.** libp2p direct remains preferred where
    available; custom Noise_XX TCP remains supported as fallback and for direct
    known peer calls.
16. **Communerdette owns all liveness for its relationship.** L1 (transport
    alive), L2 (TBID identity confirmed), and L3 (Chronomatter responsive) are
    all Communerdette's responsibility. Communerd routes inbound messages but
    does not drive liveness loops for individual TBID relationships.
    `PeerPool::start_liveness_pings` is superseded by Communerdette-driven
    liveness and must be deprecated once all three levels are implemented.
17. **Every inbound message is gated through fast-key verification.** Any data
    arriving from the remote TBID enters Communerdette as `Unprocessed<R>`. It
    must pass fast-key signature verification before becoming
    `CleanAuthenticated<R>`. There are no exceptions: liveness responses,
    calendar records, stamp replies, and any future message types all follow
    this invariant.
18. **Every outbound message is wrapped as `Externalized<R>`.** Communerdette
    never sends raw bytes to the remote TBID. All outbound payloads are
    constructed from locally produced or already-authenticated data and wrapped
    in `Externalized<R>` before dispatch.
19. **`from_trusted` is for locally produced data only.** It must never be
    called on data that arrived from a remote peer. Using `from_trusted` on
    remote data silently bypasses fast-key verification and is a correctness
    bug. The only correct way to authenticate a remote record is
    `Unprocessed<R>::verify(...)`.
20. **Slow-key rule.** When an inbound message carries a slow-key signature in
    addition to the fast-key signature, both must be verified. A wrong slow-key
    signature is a hard rejection. Absence of a slow-key signature is
    acceptable. The first and currently only operation that requires a slow-key
    signature is channel-binding establishment (see §12.0 and §11.4).
21. **Initial channel-binding requires dual-key proof.** The first time
    Communerdette contacts a remote TBID on any channel (libp2p PeerId, direct
    ip/port, or other), the remote must respond with a `ChannelBinding` message
    dual-signed with both the fast key and the slow key, producing a
    `CleanFullyAuthenticated<ChannelBinding>`. Once the channel is bound,
    subsequent messages on that channel need only fast-key signatures to produce
    `CleanAuthenticated<R>`. Re-binding is required if the channel disconnects
    and a new transport identity is presented.
22. **Communerdette may hold multiple simultaneous channels to the same TBID.**
    Each channel is independently bound. The TBID relationship persists as long
    as at least one fully-bound channel is active. This enables both
    multi-transport operation and transparent reconnection: when a channel drops,
    Communerdette initiates a fresh binding challenge on a new channel without
    tearing down the relationship.

---

## 5. Public Interface: CommunerdetteLine

`CommunerdetteLine` is a **trait** — the restricted interface granted to
Calendar, Chronomatter, and TimeFamily callers. It is not a struct.

Communerd has direct access to the full `Communerdette` object and all its
private methods (liveness loops, binding updates, route management, stats
mutation, and shutdown). Calendar, Chronomatter, and TimeFamily receive only an
`Arc<dyn CommunerdetteLine>` handle, which limits their access to the safe
TBID-scoped operations defined by the trait. `Communerdette` implements
`CommunerdetteLine` (plus its private body); callers never construct or store
`Communerdette` directly.

The trait must support async callers. All methods that contact the network are
async. Communerdette handles queuing, retries, channel selection, and binding
refreshes behind the trait boundary so callers never see transport details.

```rust
#[async_trait]
pub trait CommunerdetteLine: Send + Sync {
    fn target_tbid(&self) -> Tbid;

    async fn stamp(
        &self,
        content: Vec<u8>,
        echo: String,
    ) -> Result<CleanAuthenticated<Foretis>, TransportError>;

    async fn get_calendar_slice(
        &self,
        tick_start: u64,
        count: u64,
    ) -> Result<CleanAuthenticated<Vec<ChrononRecord>>, TransportError>;

    async fn get_tick(
        &self,
        tick_number: u64,
    ) -> Result<CleanAuthenticated<ChrononRecord>, TransportError>;

    async fn start_calendar_stream(
        &self,
        from_tick: u64,
    ) -> Result<CalendarStreamHandle, TransportError>;

    fn status_summary(&self) -> CommunerdetteStatusSummary;
}
```

`start_calendar_stream` may initially return `Unsupported` until the streaming
implementation is wired. The method exists so Calendar mirroring can target the
line abstraction without learning raw transport details. Once implemented, the
stream must yield clean authenticated records or batches; raw remote stream
items must not bypass Communerdette authentication.

### 5.1 Preferred Verification Flow

For cross-node verification, the line should primarily fetch evidence:

```text
1. Caller has content and Foretis { tbid: T, tick_number: N, ... }.
2. Caller obtains line = communerd.line_for_tbid(T).
3. Caller gets tick = line.get_tick(N).await?.
4. Communerdette has already authenticated the remote tick as belonging to T.
5. Caller verifies content hash and Foretis signature locally using the
   authenticated tick's public key.
```

If an implementation chooses to perform discovery before returning a line, it
may add a separate fallible async API such as `try_line_for_tbid(T).await`.
The default handle-returning API should be cheap and should let the
Communerdette perform refresh, binding, and route selection behind the line.

This is preferred over `line.verify(content, foretis)` because the local node
does not need to trust a remote true/false answer.

---

## 6. Private Interface: Communerdette

`Communerdette` is private to `foretias-node/src/communerd/`.

It owns relationship-local state, but calls back into Communerd for global
services such as DHT lookup, swarm commands, peer registry reads, and transport
execution:

```rust
struct Communerdette {
    target_tbid: Tbid,
    state: RwLock<CommunerdetteState>,
    command_tx: mpsc::Sender<CommunerdetteCommand>,
    shutdown: CancellationToken,
    communerd: Weak<CommunerdInner>,
}
```

The final implementation may keep the current `Communerd` struct rather than
introducing `CommunerdInner`; the invariant is that the real state is not
available through `CommunerdetteLine`.

Private/admin-only methods may include:

- `refresh_dht_binding`
- `record_peer_registration`
- `verify_or_update_tbid_binding`
- `choose_route`
- `record_transport_success`
- `record_transport_failure`
- `enqueue_request`
- `spawn_liveness_task`
- `spawn_queue_task`
- `spawn_stream_task`
- `shutdown`

These must not be callable from Calendar or Chronomatter.

### 6.1 Communerd Helper Boundary

Communerdette should not duplicate Communerd's global responsibilities. Instead,
it may depend on a private helper surface supplied by Communerd. This keeps the
Communerdette focused on one relationship while Communerd continues to act as a
member of the P2P network:

```rust
trait CommunerdetteHost {
    async fn lookup_tbid_record(&self, tbid: Tbid) -> Option<PeerRegistrationRecord>;
    async fn call_peer(
        &self,
        peer: PeerAddr,
        method: &'static str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError>;
    fn namespace(&self) -> String;
    fn has_active_libp2p(&self) -> bool;
}
```

This is illustrative, not a required exact trait. The important split is:

- Communerd owns global discovery, DHT, peer pool, swarm, and transport pools.
- Communerdette owns the policy and memory for one TBID relationship.
- CommunerdetteLine exposes only safe operations for that relationship to Calendars and Chronomatters.

---

## 7. Relationship State

```rust
pub enum TbidBindingStatus {
    Unknown,
    ClaimedByDht {
        peer_id: Option<libp2p::PeerId>,
        json_rpc: Option<String>,
        observed_at_ns: u64,
    },
    Verified {
        peer_id: Option<libp2p::PeerId>,
        json_rpc: Option<String>,
        verified_at_ns: u64,
        proof_expires_at_ns: Option<u64>,
    },
    Rejected {
        reason: String,
        observed_at_ns: u64,
    },
}

pub enum ActiveRoute {
    Libp2pDirect,
    NoiseJsonRpc,
    Unavailable,
}

pub struct CommunerdetteStats {
    pub last_success_ns: Option<u64>,
    pub last_failure_ns: Option<u64>,
    pub consecutive_failures: u32,
    pub libp2p_successes: u64,
    pub libp2p_failures: u64,
    pub noise_successes: u64,
    pub noise_failures: u64,
    pub smoothed_rtt_ms: Option<f64>,
    pub queue_depth: usize,
}

struct CommunerdetteState {
    binding: TbidBindingStatus,
    active_route: ActiveRoute,
    stats: CommunerdetteStats,
    backoff_until_ns: Option<u64>,
    stream_state: CalendarStreamState,
}
```

`CommunerdetteStatusSummary` may expose a redacted subset of this state for
diagnostics. It must not expose mutable handles or raw queue internals.

---

## 8. Request Priority

Each Communerdette owns relationship-local scheduling. At minimum, requests
must carry a priority:

| Priority | Examples |
|----------|----------|
| Critical | TBID binding proof, collision/dormancy control, shutdown-sensitive control messages |
| High | Fetch tick needed to verify a Foretis, mutual-attestation evidence lookup |
| Normal | Stamp request, small calendar slice request, mirror negotiation |
| Bulk | History dump, large calendar replication batch |

When resources are constrained, high-priority verification evidence must not be
starved by bulk mirror traffic.

---

## 9. Transport Selection

Route choice is made inside the Communerdette:

1. If a verified libp2p PeerId exists and the swarm can route to it, prefer
   libp2p direct RPC over request_response/yamux.
2. If libp2p direct fails and a `json_rpc` address exists, fall back to custom
   Noise_XX TCP JSON-RPC.
3. If only an unverified route exists, allow discovery/binding operations but do
   not let Calendar store trust-bearing results as verified mutual attestations.
4. Track successes, failures, and RTT per route.
5. Apply backoff per relationship, not globally.

This centralizes the fallback currently repeated by `stamp_peer`,
`route_stamp`, and `get_calendar_slice`.

---

## 10. TBID Binding

DHT records map TBIDs to PeerIds and addresses, but a DHT record alone is not
proof that the transport peer is authorized for the TBID.

The Communerdette must represent this distinction.

Expected future proof shape:

```text
local -> remote:
  challenge = random nonce + local PeerId + remote PeerId + target TBID

remote -> local:
  signature over challenge using TBID-controlled signing capability
  optional current ChrononRecord or TBID public proof material

local:
  verify signature against TBID proof rules
  mark binding Verified
```

This should be fully implemented.

---

## 11. Remote Authentication Product

Communerdette is the **inbound gate orchestrator** for all remote records. It
converts `Unprocessed<R>` into `CleanAuthenticated<R>` by calling the existing
`verify` methods defined in `core-engine/src/foretias/clean_auth.rs` (Take 3).
Communerdette does not invent new verification logic; it gathers the verification
context and invokes the Take 3 pipeline.

### 11.1 Inbound Pipeline

```text
transport bytes
  -> Unprocessed<R>          (Unprocessed::<R>::from_bytes() or from_json_value())
  -> Communerdette gathers context:
       - target TBID (from the Communerdette's scope)
       - crypto server (from Communerd)
       - binding state (from the Communerdette's relationship state)
       - previous record, if chain verification is required
  -> Unprocessed<R>::verify(...)   (Take 3 method, defined in core-engine)
  -> CleanAuthenticated<R>
  -> Calendar / Chronomatter-adjacent code / TimeFamily code
```

### 11.2 Communerdette's Role in Verification

Communerdette is responsible for:

1. **Parsing** — Convert raw transport bytes into `Unprocessed<R>` using the
   Take 3 constructors (`from_bytes`, `from_json_value`, `from_parsed`).
2. **Context assembly** — Gather the verification parameters that
   `Unprocessed<R>::verify(...)` requires:
   - For `Unprocessed<ChrononRecord>::verify(crypto, prev)`: supply the crypto
     server and the previous `CleanAuthenticated<ChrononRecord>` if chain
     verification is needed.
   - For `Unprocessed<Foretis>::verify(crypto, record, content)`: supply the
     crypto server, the authenticated `ChrononRecord`, and the content bytes.
   - For `Unprocessed<ProbityReport>::verify(crypto)`: supply the crypto server.
3. **Binding state enforcement** — Before calling `verify`, check that the
   Communerdette's `TbidBindingStatus` is sufficient for the operation's trust
   level. A `ClaimedByDht` binding may be sufficient for evidence fetch but not
   for trust-bearing mutual attestation storage.
4. **TBID claim validation** — After `verify` succeeds, confirm that the
   `CleanAuthenticated<R>` inner record's TBID matches the Communerdette's
   target TBID. If not, reject with an explicit error.

Communerdette does NOT:

- Define new `verify` methods — those live in `core-engine` per Take 3.
- Construct `CleanAuthenticated<R>` directly — only `Unprocessed<R>::verify(...)`
  and `CleanAuthenticated::<R>::from_trusted()` can do that (private constructors).
- Bypass the Take 3 pipeline for any record type.

### 11.3 What CleanAuthenticated<R> Means

A `CleanAuthenticated<R>` value returned by Communerdette means:

- the remote bytes were parsed using the expected schema for `R`
- the claimed remote TBID matches the Communerdette's target TBID
- `Unprocessed<R>::verify(...)` passed (signatures, hashes, chain links, etc.)
- the fast-key signature was verified against the remote TBID's current fast
  public key
- the route/binding state was strong enough for the operation's trust level
- protocol freshness or replay checks were applied where the protocol defines
  them

A `CleanAuthenticated<R>` value does not mean:

- the remote statement is morally or semantically true
- the local node agrees with the remote statement
- Communerdette signed anything
- Communerdette may sign for any local TBID
- the slow-key signature (if present) has been verified — that is a separate
  step if the operation requires it

Calendar may store trust-bearing external-attestation material only after the
relevant remote record has become `CleanAuthenticated<R>` and any
Calendar-specific policy checks have also passed.

### 11.4 TBID Key Model

Every TBID embeds two signing keys:

- **Fast key** (Ed25519 in current implementations): signs every message.
  Communerdette verifies the fast-key signature on every inbound message from
  the remote TBID. This is the mandatory authentication step.

- **Slow key** (PQC/SPHINCS+ in future implementations): may optionally
  co-sign a message alongside the fast key for high-assurance operations.
  Rules:
  - A message that carries no slow-key signature is accepted after fast-key
    verification passes.
  - A message that carries a slow-key signature where the signature is
    incorrect is rejected, regardless of whether the fast-key signature is
    valid.
  - Channel-binding establishment is the first and currently only protocol
    operation that requires a slow-key signature. All other protocol messages
    use fast-key only.

Implementation note: the slow-key verification path must exist in the gate
functions even if it only fires during channel-binding establishment. The path
must be explicit rather than silently absent.

#### 11.4.1 Channel-Binding Establishment Protocol

When Communerdette receives a new channel from Communerd (libp2p PeerId,
direct ip/port, or future transport), it initiates a one-time binding challenge
before accepting any application messages on that channel:

```text
1. Communerd notifies Communerdette of a new channel (transport identity T for target TBID X).
2. Communerdette generates a 32-byte random nonce N.
3. Communerdette sends a channel_bind_challenge { nonce: N, channel_id: T, requester_tbid: local_tbid } to the remote.
4. Remote constructs ChannelBinding { responder_tbid: X, nonce_echo: N, channel_id: T }.
5. Remote signs ChannelBinding bytes with fast key → fast_sig.
6. Remote signs ChannelBinding bytes with slow key → slow_sig.
7. Remote returns channel_bind_response { channel_binding: ..., fast_sig, slow_sig }.
8. Communerdette parses → Unprocessed<ChannelBinding>.
9. Verifies: TBID matches target, nonce_echo matches N, channel_id matches T.
10. Verifies fast_sig against TBID's fast key.
11. Verifies slow_sig against TBID's slow key.
12. On success → CleanFullyAuthenticated<ChannelBinding>; channel state → FullyBound.
13. On any failure → channel state → Rejected; channel is not used.
```

Once a channel is `FullyBound`, all subsequent messages on that channel pass
through the normal fast-key gate (`Unprocessed<R>` → `CleanAuthenticated<R>`).
The slow key is not re-verified per message.

Multi-channel: Communerdette may hold N fully-bound channels to the same TBID
simultaneously. Each channel has its own binding state. The TBID relationship
is healthy as long as at least one fully-bound channel is active.

### 11.5 Known Gap: gate_foretis Uses from_trusted

**This is a correctness bug to be fixed in Phase 11.**

The current `CommunerdetteExecutor::gate_foretis` implementation calls
`CleanAuthenticated::from_trusted(unprocessed.into_inner())` after structural
and TBID checks pass. `from_trusted` bypasses fast-key verification. A remote
`Foretis` is not locally produced data; it is an inbound remote record that
must pass the fast-key gate.

The correct implementation:

```text
1. Parse raw JSON → Unprocessed<Foretis>
2. Structural check (chronon_number != 0, signature non-empty, algorithm set)
3. TBID match check (foretis.tbid == communerdette.target_tbid)
4. Obtain the authenticated ChrononRecord for foretis.chronon_number
   (either passed in by the caller, or fetched via get_tick)
5. Call Unprocessed<Foretis>::verify(crypto, chronon_record, content_bytes)
6. On success → CleanAuthenticated<Foretis>
```

Step 4 is the reason `from_trusted` was used as a shortcut: the caller
does not always have the `ChrononRecord` available at the time `gate_foretis`
is called. The fix is to require callers to supply the authenticated record,
or to chain a `get_tick` call into the gate.

Until Phase 11 lands, `CleanAuthenticated<Foretis>` values produced by
`gate_foretis` have NOT been fast-key verified. Code that stores or acts on
those values is accepting a structurally-valid but cryptographically unverified
remote statement.

### 11.6 Crypto Call-Site Snapshot Audit

To prevent silent introduction of new signing or verification operations —
particularly as Communerdette adds channel-binding, liveness, and gate functions
— the project maintains a committed **crypto call-site snapshot file**.

**Snapshot file location:** `p2p/tests/snapshots/crypto_call_sites.txt`

**What the snapshot captures:**

1. **Crypto primitive call sites** — every place in the Rust source that calls
   through the crypto plugin/FFI layer (e.g., calls to `CryptoServer` methods
   that dispatch to C++ primitives: sign, verify, hash, key generation). This
   maps the Rust → FFI → C++ boundary exhaustively.

2. **Signature call sites** — every place in the Rust source where a signature
   is generated (`sign(...)` or equivalent) or verified (`verify(...)` or
   equivalent), regardless of transport layer. Each line in the snapshot is:
   ```
   <crate>/<file>:<line>  <function_name>  <call_type: sign|verify|other>
   ```

**How the snapshot is produced and checked:**

```bash
# Regenerate the snapshot (run after any crypto call site changes):
cd p2p && cargo test --test crypto_callsite_snapshot -- --nocapture --include-ignored > /dev/null
# Or via the dedicated script:
scripts/gen_crypto_snapshot.sh > p2p/tests/snapshots/crypto_call_sites.txt
```

The snapshot test (`cargo test crypto_callsite_snapshot`) runs the grep/AST
pass and fails if the current output differs from the committed snapshot. Any
new crypto call site requires:
1. A deliberate code review of the new site.
2. Updating the snapshot file in the same commit.
3. A comment at the call site annotating the signing authority and authentication
   level (e.g., `// SIGN(local-tbid, fast-key)` or `// VERIFY(remote-tbid, fast-key)`).

**Why this matters for Communerdette:** Communerdette is the primary consumer of
remote crypto verification (gate functions) and the trigger for local signing
(channel-binding response, authenticated-ping response). Every new gate function
or signing handoff that lands in `communerd/` must appear in the snapshot.
Reviewers checking the snapshot diff can immediately see whether new crypto
operations are expected or accidental.

---

## 12. Three Liveness Levels

Communerdette owns all liveness for its relationship (Invariant 16). All three
levels are driven by Communerdette and are distinct in what they prove.

Channel-binding establishment (§12.0) is a prerequisite for L2 and L3.
L1 may run before binding is established to confirm reachability.

### 12.0 Channel-Binding Establishment

This is not a periodic health check — it is a one-time protocol that runs when
Communerdette first acquires a channel to the target TBID.

**What it proves:** the remote party holds BOTH the fast key and the slow key
for the target TBID. The transport identity (PeerId, ip/port, etc.) is bound to
the TBID with strong dual-key proof.

**Result type:** `CleanFullyAuthenticated<ChannelBinding>`. The channel state
transitions from `Unbound` to `FullyBound`. Only a fully-bound channel may carry
application messages.

**Wire:** new `channel_bind_challenge` / `channel_bind_response` JSON-RPC method pair.

Challenge request:
```json
{
  "method": "channel_bind_challenge",
  "params": {
    "nonce": "<hex-encoded 32-byte random>",
    "channel_id": "<transport identity string>",
    "requester_tbid": "<hex>"
  }
}
```

Response:
```json
{
  "responder_tbid": "<hex>",
  "nonce_echo": "<hex — must match request nonce>",
  "channel_id": "<hex — must match request channel_id>",
  "fast_sig": "<hex — fast-key Ed25519 sig over (nonce || channel_id || responder_tbid_hex)>",
  "slow_sig": "<hex — slow-key SPHINCS+ sig over same bytes>"
}
```

**Binding state machine per channel:**
- `Unbound` → send challenge → receive dual-signed response → `verify()` succeeds → `FullyBound`
- `Unbound` → verify failure → `Rejected` (channel not used; log binding violation)
- `FullyBound` → channel disconnect → `Unbound` for that channel (other channels unaffected)

**Re-binding:** if a channel disconnects and a new connection arrives from the
same transport address, a fresh challenge is required. No proof from a previous
session is reused.

### 12.1 Level 1 — Network Stack Alive

**What it proves:** the Noise/TCP transport to the remote address is reachable
and a connection can be established.

**Wire:** existing `ping` JSON-RPC method → `{ "pong": true }` response.

**Authentication:** none at the application level. The Noise_XX handshake
provides transport-layer mutual key proof, but the remote's Noise keypair is
not bound to a TBID at this level. L1 proves connectivity only.

**Owner:** Communerdette drives the L1 loop. Communerd provides the transport;
Communerdette owns the liveness state and decides whether to ping and how to
record the outcome. `PeerPool::start_liveness_pings` is superseded by
Communerdette L1 and must be deprecated (see Phase 12 in the plan).

**Response handling:** `{ "pong": true }` is a trusted-local response since it
is evidence only of transport availability, not of remote TBID identity. It
does not need to pass through the `CleanAuthenticated<R>` gate. Record L1
success/failure in `CommunerdetteRouteStats`.

### 12.2 Level 2 — TBID Identity Confirmed (Ongoing Health)

**Prerequisite:** channel binding (§12.0) must be `FullyBound` before L2 health
checks begin. If binding has not been established or was rejected, L2 is skipped.

**What it proves:** the remote party continues to hold the fast signing key for
the target TBID on the currently-bound channel. This is the ongoing health
check after binding is established — it does **not** re-verify the slow key.

**Wire:** new `authenticated_ping` JSON-RPC method.

Request:
```json
{
  "method": "authenticated_ping",
  "params": {
    "challenge": "<hex-encoded random nonce, 32 bytes>",
    "sender_tbid": "<hex>"
  }
}
```

Response:
```json
{
  "responder_tbid": "<hex>",
  "challenge_echo": "<hex — must match request challenge>",
  "signature": "<hex — fast-key sig over (challenge || responder_tbid_hex_bytes)>",
  "signature_algorithm": "Ed25519"
}
```

**Authentication:** the response is `Unprocessed<AuthenticatedPong>` and must
pass fast-key verification before Communerdette records L2 success. Verify:
1. `responder_tbid` matches `communerdette.target_tbid`.
2. `challenge_echo` matches the sent challenge (replay/mismatch guard).
3. Signature verifies over `(challenge || responder_tbid_hex_bytes)` using the
   remote TBID's fast public key from the current DHT/binding record.
4. Promote to `CleanAuthenticated<AuthenticatedPong>` only after all checks
   pass.

A failed L2 check (wrong TBID, wrong signature, challenge mismatch) is a
binding violation and must be recorded as `TbidBindingStatus::Rejected` with
reason.

**Server-side handler:** `handle_authenticated_ping` in `server/handlers.rs`.
The server signs the response with its own TBID's fast key (via Chronomatter's
signing API — Communerdette does not sign on behalf of the local TBID).

### 12.3 Level 3 — Chronomatter Responsive

**What it proves:** the remote node's Chronomatter is alive and capable of
stamping content. The returned `Foretis` must pass full fast-key verification.

**Wire:** existing `stamp` JSON-RPC method. Level 3 liveness is a `stamp` call
with a known short test payload, followed by full `CleanAuthenticated<Foretis>`
verification through the corrected Take 3 gate (Phase 11 fix required first).

**Authentication:** the `Foretis` response enters as `Unprocessed<Foretis>` and
must pass full `Unprocessed<Foretis>::verify(crypto, chronon_record, content)`
(see §11.5 for the gate_foretis fix). This requires:
1. The caller knows the content bytes (it sent them).
2. After receiving the `Foretis`, obtain `CleanAuthenticated<ChrononRecord>`
   for `foretis.chronon_number` via `get_tick`.
3. Call `Unprocessed<Foretis>::verify(crypto, chronon_record, content)`.
4. Verify `foretis.tbid == communerdette.target_tbid`.
5. Promote to `CleanAuthenticated<Foretis>`.

A failed L3 check means Chronomatter is not responding correctly and must be
recorded in `CommunerdetteStats`. The binding status remains unchanged (L3
failure is a health signal, not a binding violation).

### 12.4 Liveness Loop Policy

Communerdette runs a per-relationship liveness background task that cycles
through L1 → L2 → L3 on a configurable interval. The levels are ordered:

- Run L1 first. If L1 fails, skip L2 and L3 and record transport failure.
- Run L2 after L1 passes. If L2 fails, skip L3 and record a binding violation.
- Run L3 after L2 passes. L3 failure is a health signal only.

The liveness interval and which levels to run are relationship policy
(stored in `CommunerdetteState`) and may be configured differently per
use case (e.g. mutual-attestation peers run all three; bootstrap peers
run L1 only).

---

## 13. Local TBID Signing Authority

(@human — this was §12 before the liveness section was inserted)

Communerdette is not a time-being and does not own the signing authority of
Calendar, Chronomatter, or any other local TBID-bearing object. A local TBID
belongs to exactly one Rust object. That object is the only code path allowed
to produce signatures for that TBID.

There are two separate security jobs:

1. **Transport/session authentication.** Communerd and its communicator tasks
   authenticate the connection, maintain encrypted channels, bind or challenge
   remote transport identities, and decide whether a route is usable.
2. **Application/TBID signing.** Calendar, Chronomatter, or another local TBID
   owner signs its own statements through its own internal API.

Communerdette may carry, queue, retry, and prioritize signed envelopes. It may
verify signatures when routing or policy requires it. It must not create a
signature for a TBID it does not own.

When one local object sends for another, the sender must obtain an explicitly
signed payload from the owner:

```text
Calendar wants to send a Chronomatter statement to remote TBID R.

1. Calendar asks Chronomatter, through an internal API, to sign the statement.
2. Chronomatter returns a signed payload whose signer TBID is Chronomatter's TBID.
3. Calendar passes that signed payload to CommunerdetteLine(R).
4. Communerdette authenticates/uses the transport route and sends the payload.
5. The remote side verifies the payload against the claimed signer TBID.
```

If Communerd has its own TBID for network-level statements, that TBID authorizes
only Communerd's own statements. It does not grant Communerd permission to sign
for Calendar or Chronomatter.

Implementation rule: avoid API shapes that accept `tbid_to_sign_as` plus raw
bytes inside Communerdette or CommunerdetteLine. Prefer API shapes that accept
an already-signed envelope, or call a narrow internal signing trait implemented
only by the TBID-owning object.

---

## 14. Calendar and Chronomatter Usage

Calendar is the first intended consumer:

- mutual attestation: `line.stamp(serialized_tick, echo).await`
- verification evidence: `line.get_tick(foretis.tick_number).await`
- mirroring: `line.get_calendar_slice(...)` or `line.start_calendar_stream(...)`

For any Calendar-originated message that claims a Calendar TBID, Calendar signs
before calling CommunerdetteLine. For any Calendar-originated message that
claims a Chronomatter TBID, Calendar must first obtain a Chronomatter-signed
payload through Chronomatter's internal API, then hand that signed payload to
CommunerdetteLine for delivery.

Chronomatter lives in `core-engine`, while Communerd lives in `foretias-node`.
If Chronomatter needs direct access to remote lines in a future phase, define a
small trait in `core-engine` and implement it in `foretias-node`. Do not create
a dependency from `core-engine` back to `foretias-node`.

---

## 15. Acceptance Criteria

1. Calendar or TimeFamily code can obtain a `CommunerdetteLine` for a TBID and
   fetch a tick without manually handling `PeerRegistrationRecord`, `PeerAddr`,
   `PeerId`, or transport fallback.
2. `Communerdette` internal state and admin methods are not accessible outside
   the `communerd` module.
3. `get_tick` and `get_calendar_slice` await bounded responses and return
   explicit timeout errors.
4. Existing direct Communerd calls continue to work during migration.
5. Cross-node verification can be refactored to use `CommunerdetteLine` while
   preserving local verification of signatures.
6. Per-TBID stats record liveness and route health without exposing mutable
   internals to Calendar or Chronomatter.
7. Communerdette implements all three liveness levels (L1 transport, L2
   TBID identity via fast key, L3 Chronomatter via stamp+verify). Liveness
   loops are driven by Communerdette. `PeerPool::start_liveness_pings` is
   deprecated and removed.
8. No Communerdette or CommunerdetteLine API can sign as Calendar,
   Chronomatter, or another local TBID owner. Outbound messages that claim a
   local TBID are signed by the owning Rust object before handoff to
   Communerdette.
9. Remote records returned to Calendar or Chronomatter-adjacent code for
   trust-bearing use are wrapped in `CleanAuthenticated<R>` after
   Communerdette verifies the remote TBID and fast-key signature.
10. `from_trusted` is never called on data that arrived from a remote peer.
    The `gate_foretis` function uses full `Unprocessed<Foretis>::verify(...)`
    with an authenticated `ChrononRecord` supplied by the caller or fetched
    via `get_tick`.
11. All outbound payloads dispatched by Communerdette are wrapped in
    `Externalized<R>`.
12. Inbound messages that carry a slow-key signature and fail slow-key
    verification are rejected even if the fast-key signature is valid.
    Absent slow-key signatures are not treated as failures.
13. A committed snapshot file `p2p/tests/snapshots/crypto_call_sites.txt`
    enumerates every Rust call site that invokes a crypto primitive (sign,
    verify, hash, key generation via the crypto plugin) and every location
    where a signature is generated or checked. The snapshot test passes on a
    clean workspace. Any new crypto call site requires updating the snapshot
    in the same commit with an explanatory annotation.
14. Channel-binding establishment (§12.0) requires dual-key (fast + slow)
    proof from the remote TBID. A channel with a missing or incorrect slow-key
    binding proof must not reach `FullyBound` state. L2 and L3 liveness checks
    must not run on unbound channels.

---

## 16. Non-Goals

- Do not remove custom Noise_XX TCP JSON-RPC.
- Do not require libp2p direct for all peers.
- Do not make Calendar or Chronomatter own network transport state.
- Do not make Communerd or Communerdette a shared signing service for other
  local TBID owners.
- Do not let Calendar store raw remote replies as trust-bearing evidence before
  Communerdette has converted them into `CleanAuthenticated<R>`.
- Do not outsource final application verification to a remote true/false answer
  when local evidence fetch is enough.
- Do not make `CommunerdetteLine` a general JSON-RPC escape hatch for arbitrary
  unreviewed methods.
- Do not implement full TBID binding proof in the first phase unless scoped
  separately by the implementation plan.
- Do not implement slow-key verification in these phases; annotate the gate
  functions with a clear TODO marking where slow-key verification will be
  inserted.
- Do not use `from_trusted` on any data arriving from a remote peer.

---

## 17. Fully-Bound Gossip (Phase 13)

### 17.1 Purpose

When a Communerdette reaches `FullyBound` state (§12.0), or when a `FullyBound`
relationship is lost, the event is broadcast as a signed `ProbityReport` on the
gossipsub probity topic. This gives the broader network observable evidence of
who is mirroring whom without requiring a central registry.

Because channel-binding is bilateral but independent, two gossip messages are
emitted per FB establishment: one from each party when it locally reaches
`FullyBound`. Loss may be unilateral.

### 17.2 Report Shape

The `ProbityReport` fields for an FB event:

| Field | Established | Lost |
|-------|-------------|------|
| `reporter` | Local Calendar's TBID hex | same |
| `subject` | Remote Chronomatter's TBID hex | same |
| `attribute` | `"fb"` | `"fb"` |
| `value` | `1.0` | `-1.0` |
| `timestamp_ns` | current time (ns since epoch) | same |
| `curve` | `1` (Ed25519) | same |
| `signature` | full TbidSecret dual-key signature | same |

The `attribute = "fb"` value is the type discriminator. Sign is the state
discriminator (`positive = established`, `negative = lost`).

**Why full TbidSecret signing:** FB establishment is a meaningful network-level
commitment (Calendar is declaring it is mirroring a specific Chronomatter). The
signature must be verifiable by anyone who has the reporter's TBID, not just by
peers who already know the generic Ed25519 key. Full TbidSecret signing makes
this verifiable without prior key exchange. FB events do not happen frequently,
so the cost of a slow-key (SPHINCS+) signature is acceptable.

### 17.3 Signing Authority

The signing authority is the **local Calendar's TbidSecret**. Communerdette does
not hold signing keys. The emission path is:

```
Communerdette state → FullyBound
  → call CommunerdetteHost::host_emit_fb_gossip(reporter_tbid, subject_tbid, value)
  → Communerd asks Calendar/Chronomatter to sign via internal signing API
  → signed ProbityReport returned
  → published to gossipsub probity topic via publish_probity_report()
```

Two new methods on `CommunerdetteHost`:

```rust
/// Sign a ProbityReport with the local Calendar's TbidSecret and return it.
async fn host_sign_probity_report(
    &self,
    report: ProbityReport,
) -> Result<ProbityReport, NodeError>;

/// Publish a signed ProbityReport to the gossipsub probity topic.
async fn host_publish_probity_report(
    &self,
    report: ProbityReport,
) -> Result<(), NodeError>;
```

### 17.4 Reception and Processing

When a `ProbityReport` arrives on the probity gossipsub topic:

1. Attempt `Unprocessed<ProbityReport>::verify(crypto)` — authenticates the
   reporter's Ed25519 signature using `pub_key_from_tbid_hex(&report.reporter)`.
2. If `attribute == "fb"`:
   - Log at `DEBUG`: `"[probity] fb report: reporter={} subject={} value={}"`.
   - Ingest into `ProbityStore` for local aggregation.
3. Unauthenticated or malformed reports are silently dropped (no panic, no error
   propagation to caller). Log at `TRACE` only.

(@human: only fast-key verification is done on reception — the full TbidSecret
slow-key signature is what makes the report authoritative for external auditors,
but the receiving Communerd only has the Ed25519 public key from the TBID. Full
slow-key verification is deferred until `tbid_verify` C function is wired, same
as channel-binding.)

### 17.5 State Transition Wiring

In `spawn_channel_bind_task` (Phase 12.0):
- When state transitions to `FullyBound`: call `host_emit_fb_gossip` with
  `value = 1.0`.

In Communerdette state update paths (shutdown, rejection, disconnect):
- When state transitions out of `FullyBound`: call `host_emit_fb_gossip` with
  `value = -1.0`.

---

## 18. Toppoli — Test Of P2P and PTP On Local Integration (Phase 14)

### 18.1 Purpose and Scope

Toppoli is the multi-peer local integration test harness for foretias. It manages
up to ~24 `TimeFamilyServer` instances within a single test binary, supporting
peer lifecycle (start, stop, restart), flexible network topology (full mesh,
ring, custom), uniform or per-peer configuration, and direct Rust struct
introspection.

`TimeFamilyServer` runs in-process. There is no forking. All servers share the
same tokio runtime. Tests can directly inspect any `pub` method on any server —
including `communerd()`, `line_for_tbid()`, `status_summary()`, and
`ProbityStore` — without any external probing mechanism.

Intended test classes:
- **Liveness (Phase 12)**: L1/L2/L3 round-trips across real TCP sockets.
- **FB gossip (Phase 13)**: peer A reaches `FullyBound`, gossip arrives at B.
- **GNF** (Gossip and Node Failure): gossip propagation under churn; DHT
  formation and recovery when peers leave and rejoin.
- **General P2P/PTP**: any test requiring realistic multi-hop message flow.

### 18.2 Key Design Decisions

**Port pre-allocation.** All ports are reserved with `bind-then-release` before
any server is constructed, so full-mesh `CommunerdConfig` (which needs every
peer's address) can be built before startup. There is a small TOCTOU window; for
local tests on a controlled machine this is acceptable.

**No `request_shutdown()` plumbing required.** `start_tcp` returns a
`JoinHandle<()>` that loops forever with no cancellation path. Shutdown is:
`stop_daemon_arc()` + `handle.abort()`. A brief configurable drain sleep
(default 20 ms) gives in-flight requests time to complete. No CancellationToken
changes to `TimeFamilyServer` are needed.

**Direct introspection.** Peers are stored as `Arc<TimeFamilyServer>`. Tests
call public methods directly. No mock adapters or external probing.

### 18.3 ToppliHarness API

File: `p2p/foretias-server/tests/toppoli.rs`

```rust
/// Configuration for one peer in the toppoli network.
pub struct ToppliPeerConfig {
    pub chronon_ns: u64,
    /// Additional peer addresses to include in CommunerdConfig.
    /// Full-mesh addresses are added automatically by topology helpers.
    pub extra_peers: Vec<String>,
    pub namespace:   String,
}

impl Default for ToppliPeerConfig { ... }  // 100ms chronon, empty peers, "toppoli"

/// A live peer managed by ToppliHarness.
pub struct ToppliPeer {
    pub idx:    usize,
    pub addr:   String,         // "127.0.0.1:<port>"
    pub server: Arc<TimeFamilyServer>,
    handle:     JoinHandle<()>,
}

impl ToppliPeer {
    /// Introspect: direct access to the running server.
    pub fn server(&self) -> &Arc<TimeFamilyServer> { &self.server }
}

/// Multi-peer local integration harness.
pub struct ToppliHarness {
    /// Configs indexed by slot. A slot may be unstarted (no server yet).
    slots:    Vec<ToppliSlot>,
    /// Pre-allocated addresses (addr = "127.0.0.1:<port>").
    addrs:    Vec<String>,
}

enum ToppliSlot {
    Reserved { config: ToppliPeerConfig },
    Running  { peer: ToppliPeer },
    Stopped  { config: ToppliPeerConfig, addr: String },
}

impl ToppliHarness {
    /// Reserve N slots with the same config. Does not start servers.
    pub fn with_peers(count: usize, config: ToppliPeerConfig) -> Self;

    /// Reserve one more slot with a custom config. Returns its index.
    pub fn add_peer(&mut self, config: ToppliPeerConfig) -> usize;

    /// Wire all reserved/stopped peers as a full mesh in their configs.
    /// Call before starting peers.
    pub fn topology_full_mesh(&mut self);

    /// Wire peers as a unidirectional ring: 0→1→2→…→N-1→0.
    pub fn topology_ring(&mut self);

    /// Start a specific peer (build server, bind TCP, start daemon).
    pub async fn start_peer(&mut self, idx: usize);

    /// Stop a specific peer: stop_daemon_arc + brief drain + handle.abort.
    pub async fn stop_peer(&mut self, idx: usize, drain_ms: u64);

    /// Restart a stopped peer at the same address with the same config.
    pub async fn restart_peer(&mut self, idx: usize);

    /// Start all reserved peers concurrently.
    pub async fn start_all(&mut self);

    /// Stop all running peers concurrently with the given drain time.
    pub async fn stop_all(&mut self, drain_ms: u64);

    /// Number of currently running peers.
    pub fn running_count(&self) -> usize;

    /// Iterate over all running peers.
    pub fn running_peers(&self) -> impl Iterator<Item = &ToppliPeer>;

    /// Get a running peer by index (panics if not running).
    pub fn peer(&self, idx: usize) -> &ToppliPeer;

    /// Poll predicate until it returns true or timeout_ms elapses.
    /// Polls every 20 ms. Returns true if predicate satisfied, false on timeout.
    pub async fn wait_until(
        &self,
        predicate: impl Fn(&ToppliHarness) -> bool,
        timeout_ms: u64,
    ) -> bool;
}
```

### 18.4 Introspection Examples

Since all servers are in-process `Arc<TimeFamilyServer>`:

```rust
// Check binding state for a specific TBID:
let communerd = harness.peer(0).server().communerd().unwrap();
let tbid = harness.peer(1).server().get_tbid();
let line = communerd.line_for_tbid(tbid);
let summary = line.status_summary();
assert!(summary.binding.is_verified());

// Check probity store:
let store = harness.peer(1).server().probity_store();
let reports = store.reports_for_subject(&tbid.to_hex());
assert!(reports.iter().any(|r| r.attribute == "fb" && r.value > 0.0));

// Check liveness stats directly:
assert_eq!(summary.stats.l1_successes, 3);
```

### 18.5 Test Template

```rust
#[tokio::test]
async fn test_name() {
    let mut h = ToppliHarness::with_peers(4, ToppliPeerConfig::default());
    h.topology_full_mesh();
    h.start_all().await;
    tokio::time::sleep(Duration::from_millis(100)).await;  // settle

    // ... exercise the feature ...
    let reached = h.wait_until(|h| /* condition */, 5_000).await;
    assert!(reached, "condition not met within 5s");

    h.stop_all(50).await;
}
```

### 18.6 Scalability Note

24 peers is the target upper bound for toppoli tests. Each peer requires one TCP
socket, one tokio task for the listener, and one chronomatter daemon task. At
100 ms chronon intervals and no external I/O, 24 peers impose negligible CPU.
DHT and gossipsub tests that need realistic peer counts (≥12) for protocol
formation should use toppoli.

Tests must not rely on wall-clock timing for correctness. Use `tokio::time::sleep`
only for port-binding settling (≤ 100 ms). Feature-specific readiness should be
checked via polling a server status method with a bounded retry loop.

---

## 19. Time Family Model (Phase 15)

### 19.1 Concept

A **Time Family** is the unit Communerdette tracks. A family consists of one
Communerd and its associated time beings (one or more Calendars, one or more
Chronomatter instances). The Communerd's TBID is the **family identifier** —
its role is analogous to a shared surname.

```
TimeFamily "Smith":
  Communerd-Smith    TBID = 0xABCD…  (family identifier, transport authenticator)
  Calendar-Smith     TBID = 0x1234…  (responsible for Chronomatter's chrononchain)
  Chronomatter-Smith TBID = 0x5678…  (time authority; produces ticks)
```

**Calendar's official role within the family.** Calendar is responsible for
**storing and replicating Chronomatter's chrononchain**. This is not incidental
— it is Calendar's declared purpose within the family. The `TimeFamily`
declaration *establishes* this responsibility: by signing the family manifest,
Calendar commits to owning and replicating the chrononchain of the named
Chronomatter instances.

**Calendar is authoritative for chrononchain blocks it replicates.** When
Calendar sends a ChrononRecord (or a slice of the chrononchain) to another
peer, it signs the block with its own TBID. The receiver trusts this block
because the `FamilyRecord` establishes Calendar as the replication
authority for that Chronomatter. The trust chain is:

```
FamilyRecord (k-way signed):
  "Calendar-Smith is responsible for Chronomatter-Smith's chrononchain"
       ↓
Calendar-Smith sends ChrononRecord(n) signed with Calendar-Smith TBID
       ↓
Receiver verifies Calendar-Smith TBID signature
  → authoritative because TimeFamily declares Calendar-Smith responsible
       ↓
Receiver can further verify the block's internal chain signature
  (Chronomatter's per-tick Ed25519 key) for depth-of-trust escalation
```

Practically, Calendar adds an `ExternalAttestation` (signed with Calendar's
TbidSecret) to each ChrononRecord it replicates. This is the authoritative
proof. The underlying Chronomatter per-tick key signature remains for chain
integrity, but the Calendar TBID signature is what Communerdette (and other
peers) verify for application-layer trust.

This explains why Calendar signs FB and GNF messages: Calendar is the entity
that holds the replication commitment, so Calendar's signature on an FB report
means "I, Calendar, have established a verified replication relationship with
the remote family's Chronomatter." The Chronomatter need not co-sign FB/GNF
because the relationship is Calendar-to-Calendar/Calendar-to-Family, not
Chronomatter-to-Chronomatter.

**Implication for `get_tick`.** When Communerdette requests a tick from a remote
family via `CommunerdetteLine::get_tick(n)`, the returned `ChrononRecord` must
carry a Calendar TBID attestation (from a Calendar listed in the remote family's
`FamilyRecord`). Communerdette verifies this attestation. The current
`from_trusted` shortcut used for locally-held calendar data is only valid for
the local family's own Calendar and must never be applied to remote calendar
data.

**Registry key change.** `Communerdette` is keyed on the remote **Communerd
TBID** (family identifier), not on a Calendar or Chronomatter TBID. One
Communerdette per remote family, regardless of how many time beings that family
has.

**Authentication model:**

| Layer | Key | Applies to |
|-------|-----|-----------|
| Transport | Communerd fast key (Ed25519) | Every message on a bound channel |
| Application: stamp/tick | Calendar/Chronomatter TBID (in the payload) | Foretis, ChrononRecord content |
| Application: FB/GNF | Calendar TBID signature + Communerd transport auth | FB gossip reports, GNF reports |

Transport auth is verified once per channel during binding (Phase 12.0) and
fast-checked on subsequent messages. The Foretis payload carries the signer's
TBID inline and is verified at the application layer — same as today. FB/GNF
carry a Calendar TBID signature in addition to the transport auth.

### 19.2 Design Principle: Thin DHT, Fat Record on Connection

The DHT holds only **thin pointers**. The full, cross-signed `FamilyRecord`
travels over the **direct connection** to the peer — it is never stored in the
DHT. This keeps DHT storage minimal (the holder nodes carry small TBID→peer_id
entries, not multi-KB k-way-signed manifests) and means a `FamilyRecord` is
always fetched fresh from its authoritative source.

DHT records (both thin, both per-key directed lookups):

| Key | Value | Use case |
|-----|-------|----------|
| `/foretias/{ns}/tbid/{TBID}` | `peer_id` | "I know this TBID — what libp2p peer do I dial?" |
| `/foretias/{ns}/attest/{TBID}` | `peer_id` (set; provider record) | "Who can verify stamps made by this TBID?" |

`FamilyRecord` (fat, k-way signed, Foretis-anchored) is obtained by **dialing
the peer** after resolving its `peer_id`, then cached locally in the Communerd's
**Family Cache**.

### 19.3 Connection and Family-Resolution Flow

```
1. A holds a target TBID (a Communerd TBID, or any member TBID).
2. DHT lookup /tbid/{TBID} → peer_id.
3. A dials peer_id (libp2p direct, or Noise TCP fallback).
4. A requests the peer's FamilyRecord over the connection.
5. A verifies the FamilyRecord:
     - k-way attestations (each member signed the manifest)
     - Foretis temporal anchor (optional / policy)
6. A associates the verified FamilyRecord with peer_id in its Family Cache.
7. A starts a Communerdette keyed on the family's Communerd TBID.
8. Channel binding (Phase 12.0) opens full communication.
9. The two families can now stamp, verify, and mirror with each other.
```

The Family Cache (in Communerd) holds:
- `communerd_tbid → FamilyRecord` (the verified manifest)
- `member_tbid → communerd_tbid` (reverse pointer, derived from the manifest's
  `calendar_tbids` / `chronomatter_tbids`)
- `communerd_tbid → peer_id` (transport association)

Once cached, subsequent encounters of any family member resolve locally with no
further DHT lookup. The reverse pointer means a Foretis carrying a Calendar TBID
resolves to its family via the cache.

### 19.4 FamilyRecord

`FamilyRecord` is the renamed and extended successor to `PeerRegistrationRecord`.
It carries the peer's reachability **and** the cross-signed family manifest. It
is transmitted over a connection (in response to a family-request RPC), not
stored in the DHT.

Its familial attestation is a **k×k cross-signing matrix**: every member fully
(dual-key) signs **every member's TBID**, including its own. A **Foretis**
temporally anchors the declaration.

**Why a cross-signing matrix (not just each-signs-manifest):** entry `[i][j]` is
member *i* endorsing member *j*'s identity. The full matrix is a complete
web-of-trust over the family — every member has personally attested to every
other member. A compromised Communerd cannot fabricate members (it cannot
produce other members' signatures), and the diagonal `[i][i]` is each member's
self-attestation (proof of key possession). "Completing a row" = one member
signing all member TBIDs.

**Why a Foretis:** stamps the manifest at a specific chronon — prevents backdating
and gives an independently verifiable timestamp.

```rust
/// FamilyRecord — the manifest. SIGNATURE-FREE with respect to itself: it carries
/// no signature *over the FamilyRecord*. The matrix signs individual member TBIDs
/// (sub-components), not the record, so this is a valid payload `R` — the Communerd
/// transport envelope signs the record itself (§21).
///
/// INVARIANTS (all enforced in the verifying constructor):
///   - member_tbids non-empty
///   - member_tbids unique, each well-formed (correct length, parses as Tbid)
///   - created_at_ns > 0
///   - matrix is exactly k×k where k == member_tbids.len()
///   - every matrix[i][j] is a valid dual-key signature by member_tbids[i]
///     over member_tbids[j]'s TBID bytes
///   - communerd_tbid ∈ member_tbids; every calendar/chronomatter TBID ∈ member_tbids
pub struct FamilyRecord {
    // ── Reachability ──
    pub communerd_tbid:     String,   // family identifier
    pub peer_id:            String,   // libp2p peer id of the Communerd
    pub multiaddr:          String,   // optional; libp2p may resolve from peer_id
    pub json_rpc:           String,   // Noise TCP endpoint (libp2p can't resolve)
    pub chronon_ns:         u64,
    pub created_at_ns:      u64,      // family creation time; checked > 0

    // ── Membership ──
    pub member_tbids:       Vec<String>,  // canonical ordered list; non-empty, unique
    pub calendar_tbids:     Vec<String>,  // subset of member_tbids
    pub chronomatter_tbids: Vec<String>,  // subset of member_tbids
    pub version:            u64,          // bumped on membership change

    // ── k×k cross-signing matrix ──
    /// matrix[i][j] = member_tbids[i] dual-key signs member_tbids[j] (49,920 B each).
    /// Strictly k×k; verified entry-by-entry at construction.
    pub matrix:             Vec<Vec<Vec<u8>>>,

    // ── Temporal anchor ──
    /// Foretis stamping postcard(manifest); itself a fully-signed Foretis record.
    pub foretis:            Foretis,
}
```

**Verify at construction (Rust convention; defensive).** `FamilyRecord` has **no
public field constructor** — it is built only via a fallible verifying
constructor that enforces every invariant above and returns
`Result<FamilyRecord, FamilyError>`. Once a `FamilyRecord` value exists, it is
structurally sound and its matrix is cryptographically valid; downstream code
never re-checks. This is "parse, don't validate" — the type *is* the proof. The
verifying constructor is the gate that yields `CleanFullyAuthenticated<FamilyRecord>`
(§21) once the Communerd transport envelope is also verified.

The constructor must be **defensively coded**: reject (never panic on) malformed
input — mismatched matrix dimensions, out-of-range indices, wrong signature
lengths, duplicate/empty/oversized member lists, non-member TBIDs in the
calendar/chronomatter subsets — each a distinct `FamilyError` variant. Bound the
member count (a hostile record could claim a huge `k` to force k² SLH-DSA
verifications as a DoS); enforce a `MAX_FAMILY_MEMBERS` cap before verifying any
signature.

**Canonical encoding** covers all fields except `attestations` and `foretis`,
in fixed declaration order with `calendar_tbids`/`chronomatter_tbids` sorted,
length-prefixed strings. Any change is wire-breaking.

**Verification levels:**

A `FamilyRecord` is **always required to reach full matrix verification before it
is used or cached** — it is the root of family trust, so there is no
"act on the envelope alone" shortcut (see §19.10). Each matrix entry is a **full
dual-key signature** (Ed25519 ‖ SLH-DSA), and the matrix is **k×k**, so full
verification costs **k² SLH-DSA verifications**. This cost is acceptable because
FamilyRecords are **rare** — produced only on family formation or membership
change — and never on the request hot path. (A member count cap, `MAX_FAMILY_MEMBERS`,
bounds k² and prevents a hostile oversized record from forcing a verification-DoS.)

**Lifecycle — sign once, cache for the time being's lifetime.** A FamilyRecord is
signed/stamped once at family formation and then **cached for essentially the
entire life of the time being**; it is re-signed only when membership changes
(a member joins or leaves, bumping `version`). The expensive k² dual-key signing
therefore amortizes to near-zero over the time being's lifetime — both the
producing family (signs once) and verifying peers (verify once, then cache the
resulting `CleanFullyAuthenticated<FamilyRecord>`) pay the slow-key cost a single
time per record version.

| Level | What is checked | When |
|-------|-----------------|------|
| Full matrix (mandatory) | All k² entries: `matrix[i][j]` = dual-key sig by member i over member j's TBID | Before any use, both sender and receiver |
| Temporal | Foretis vs Chronomatter ChrononRecord | When anchoring required |

### 19.5 Attestation Provider Record: `/attest/{TBID}`

`/foretias/{ns}/attest/{stamper_tbid}` is a **Kademlia provider record** — a
one-to-many set of `peer_id`s that can verify stamps made by `stamper_tbid`.

Provider records store only peer IDs (pointers), so a chain with many mirrors
costs many tiny entries, not many copies. New verifiers `start_providing`; dead
ones age out via provider TTL.

**Before mirroring (initial state):** the stamper's own family Calendar
publishes itself as the sole provider for its Chronomatter's TBID:

```
Calendar-Smith → start_providing(/attest/{Chronomatter-Smith TBID})
  meaning: "dial the Smith family to verify any stamp by Chronomatter-Smith"
```

**After FB mirroring (full state):** every family that has established an FB
relationship and replicated the chain also publishes itself as a provider:

```
get_providers(/attest/{Chronomatter-Smith TBID})
  → [Smith family, Jones family, Doe family, …]   (all verified replicas)
```

This publication is **part of the FB (Bruderschaft) protocol**: when family B
completes FB with family A and replicates A's chain, B `start_providing`s
`/attest/{A's Chronomatter TBIDs}`. Loss of FB (GNF) stops the re-announce, so
B ages out of the provider set.

**Origin vs. mirror.** The `/attest/{stamper_tbid}` provider set does not
distinguish origin from mirror — both publish there. The distinction lives in
the `FamilyRecord`:

- The **origin** family's `FamilyRecord` lists `stamper_tbid` in its own
  `chronomatter_tbids` (the Chronomatter is a member of that family).
- A **mirror** family's `FamilyRecord` does **not** list `stamper_tbid` as its
  own Chronomatter; the mirror merely replicates and attests that chain.

A pedantic client that needs the origin (not merely a valid replica) resolves
each provider's `FamilyRecord` and selects the one whose `chronomatter_tbids`
contains `stamper_tbid`. For ordinary verification this is unnecessary — any
provider in the set serves a verified replica.

**Key is the stamper (Chronomatter) TBID.** A `/verify` request carries a
Foretis, and the Foretis carries the stamper TBID. The verifier lookup is
therefore keyed directly on that stamper TBID:

```
verify(foretis):
  stamper = foretis.tbid                     // the Chronomatter that stamped
  providers = get_providers(/attest/{stamper})
  pick a provider → fetch tick(foretis.chronon_number) → check signature
```

There is one `/attest/` key per **Chronomatter TBID**, not per family. A family
with three Chronomatters publishes three provider keys (and re-publishes the
three keys for every remote chain it mirrors).

**Cross-witness verification (deferred; design noted).** With multiple providers
available, a verifier may ask **several capable time families to each perform
`/verify`** on the same Foretis and return a **signed verification result**. The
cross-witness value comes from collecting N independent *signed attestations*
that the Foretis is valid — not from re-downloading and comparing chronon blocks.

Two distinct, complementary capabilities:

- **Retrieve the chronon block** (`get_tick` / calendar slice): the verifier
  fetches the ChrononRecord and checks the signature itself. Useful when the
  verifier wants the underlying evidence in hand.
- **Request signed `/verify`** (cross-witness): each queried family runs its own
  verification and returns a result signed by its **Chronomatter fast key**
  (inner) inside its **Communerd envelope** (outer) — see §19.9. The verifier
  collects several such signed results. This produces a bundle of independent
  attestations — each family is on record (signed) as having verified the
  Foretis. Useful when the verifier wants corroboration from multiple parties
  without holding the chain itself.

`/verify` returning a **signed** result is what makes cross-witness meaningful:
a collection of signed "I verified this" statements from distinct families is
itself a durable, forwardable proof.

> Default for now: **one provider, one `/verify`, is sufficient.** K-of-N
> cross-witness (how many families to ask, how many must agree, how the signed
> results are bundled) is deferred to a future phase and revisited once
> mirroring is implemented. The `/verify` response must be signed regardless, so
> the cross-witness capability is available without a wire change later.

**Implementation note — `/verify` response must gain a signature.** The current
`/verify` (cross_node) response is `{ "valid": bool, "method": "cross_node" }` —
**unsigned**. To support cross-witness, the response must carry the verifying
**Chronomatter's TBID** and a Chronomatter-fast-key signature over
`(foretis_id ‖ valid ‖ chronomatter_tbid)`, then be wrapped in the standard
Communerd envelope (§19.9) on transmit:

```json
{ "valid": true, "method": "cross_node",
  "chronomatter_tbid": "0x…", "signature": "…" }
```

This is a wire change to the verify handler, scheduled with the cross-witness
phase. Until then the unsigned shape remains (single-provider trust only).

### 19.6 Communerdette Line Semantics Under the Family Model

`CommunerdetteLine` is keyed on the remote **Communerd TBID**. Its state holds
the resolved family (from the Family Cache):

```rust
struct CommunerdetteState {
    // ... existing fields ...
    family: Option<ResolvedFamily>,  // None until FamilyRecord fetched + verified
}

struct ResolvedFamily {
    calendar_tbids:     Vec<Tbid>,
    chronomatter_tbids: Vec<Tbid>,
    record_version:     u64,
}
```

Routing:
- `get_tick` / `get_calendar_slice` → a Calendar in `family.calendar_tbids`
- `stamp` → a Chronomatter in `family.chronomatter_tbids`
- FB/GNF reception → verify payload Calendar TBID ∈ `family.calendar_tbids`

If `family` is `None`, operations degrade gracefully and schedule a
FamilyRecord fetch.

### 19.7 Signing for FB/GNF Under the Family Model

`CommunerdetteHost::host_sign_probity_report` (Phase 13, §17.3) signs with the
local **Calendar's** TbidSecret — unchanged. The `reporter` field carries the
local Calendar TBID. The receiver confirms the reporter ∈ the sender family's
`calendar_tbids` (from the cached FamilyRecord) before ingesting.

### 19.8 Family Membership Is Mandatory (In-System)

For the current implementation, **every reachable node belongs to a family**.
Family-less registration is not supported — there is no implementation for it,
and a node without a published `FamilyRecord` cannot be contacted through the
system. Within the running system, time beings are always reached through their
family's Communerd.

This is deliberate, not a temporary gap to paper over: the family is the unit of
identity, reachability, and trust. A "solo" time being is simply a family of one
Communerd + one Calendar + one Chronomatter.

**Offline verification is a separate, out-of-band path.** Offline tools may read
calendar files on disk and verify TBIDs and chrononchain signatures directly,
with no running time being and no family contact. This is a legitimate audit
capability and is explicitly *not* part of the in-system reachability model. It
neither requires nor consults the DHT, the Family Cache, or any Communerdette.

### 19.9 Two-Layer Outbound Signing: Communerd Envelope over Time-Being Content

Every message a family transmits carries **two signature layers**:

**1. Inner — application authority.** The time being that performs the operation
signs the content with its own fast signing key. The signer depends on which
time being is the **active agent**:

| Operation | Inner signer | Rationale |
|-----------|--------------|-----------|
| `/stamp` (Foretis) | **Chronomatter** fast key | Chronomatter produces the stamp |
| `/verify` (result) | **Chronomatter** fast key | Chronomatter performs the verification (reads chain from its own Calendar) |
| `get_tick` / `get_calendar_slice` (ChrononRecord block) | **Calendar** key | A chrononchain block leaving the family; Calendar is its replication authority (§19.1) |
| chrononchain block replication | **Calendar** key | Calendar owns the replicated chain (§19.1) |
| FB / GNF | **Calendar** key | Relationship/replication commitment (§17, §19.7) |

**The principle — active agent signs.** Chronomatter is the active agent for
`/stamp` and `/verify`. To verify a Foretis, Chronomatter retrieves the
chrononchain data it needs **from its own Calendar**. That intra-family handoff
is **unsigned**: Calendar does not sign data it gives to its own Chronomatter,
because the k-way `FamilyRecord` already establishes mutual trust within the
family. Chronomatter therefore signs the verify result on its own authority.

Calendar signs only when chronon data or a relationship statement **crosses the
family boundary**: a ChrononRecord block leaving via `get_tick`, replication to
another family, or FB/GNF. Note the contrast — `get_tick` returns a *block*
(Calendar-signed, it is the chain authority) while `/verify` returns a signed
*yes/no result* (Chronomatter-signed, it is the verifying agent). Different
operations, different signers, no conflict.

This also resolves the mirror case: when a mirror family verifies a Foretis
stamped by another family's Chronomatter, the mirror's **own** Chronomatter
performs the check by reading the replica held by the mirror's **own** Calendar
(intra-family, unsigned), and signs the result. The result is trusted because
the mirror's `FamilyRecord` establishes that Chronomatter as a legitimate family
member and the Communerd envelope authenticates the family.

**Why this factoring (concurrency).** Assigning `/stamp` and `/verify` to
Chronomatter is primarily an anticipation of **high concurrency** on those two
operations — they are the request hot path and must scale independently.
Chronomatter is the component built to handle that throughput. Calendar is
responsible for the other concerns (durable chrononchain storage, replication,
FB/GNF relationships, serving chronon blocks), which have different concurrency
and durability profiles. Splitting the signing authority along the same line as
the concurrency boundary keeps each component's hot path self-contained.

**2. Outer — family transport envelope.** Before transmitting, **Communerd
signs the whole record** — the entire payload including the inner time-being
signature — with its own fast TBID key. This is the family envelope: "this
message left this family over this channel."

```
/stamp outbound:
  Foretis content
    └─ signed by Chronomatter fast key        ← inner (durable application proof)
  wrapped in Communerd-signed envelope         ← outer (family transport auth)
```

**Receiver order:** check the Communerd envelope first (fast transport auth, on
every message), then the inner time-being signature (application authority). The
inner signature is the durable proof that survives beyond the transport hop; the
Communerd envelope authenticates the hop and the family origin.

(@human — `/verify` results are signed by Chronomatter, not Calendar: Chronomatter
is the active agent that performs verification, reading chain data from its own
Calendar intra-family without a signature. §19.5 reflects this.)

### 19.10 Records Through the Trust Boundary Pipeline

All inbound records flow through the Take 3 wrappers
(`Unprocessed<R>` → `CleanAuthenticated<R>` → optionally
`CleanFullyAuthenticated<R>` → `Externalized<R>` outbound). This section maps
the family-model signatures onto those wrappers and states one invariant.

**Invariant — Communerd signs last, is verified first.** When a family produces
an `Externalized<R>` for transmission, the **Communerd envelope is the outermost
and final signature applied** (§19.9). On receipt, the Communerd envelope is the
**first** signature checked. This ordering is what lets the fast gate
(`Unprocessed<R>` → `CleanAuthenticated<R>`) be a single Ed25519 check before any
deeper, more expensive verification.

**The two clean wrappers encode Communerd's signing depth.** The distinction
between `CleanAuthenticated<R>` and `CleanFullyAuthenticated<R>` is, at the
Communerd layer, exactly the difference between Communerd's fast and full
signature over the record:

| Wrapper | Communerd signature over the record | Plus (record-specific) |
|---------|-------------------------------------|------------------------|
| `CleanAuthenticated<R>` | **fast** Ed25519 envelope | inner time-being fast sig |
| `CleanFullyAuthenticated<R>` | **full dual-key** (Ed25519 ‖ SLH-DSA) over the full record | inner full proofs (e.g. k-way for FamilyRecord) |

So `CleanFullyAuthenticated<R>` always **requires** that Communerd has fully
(dual-key) signed the complete record — not merely the fast envelope. For records
that also carry inner multi-party proofs (FamilyRecord's k-way attestations),
those full inner proofs are required as well. "Fully" therefore means *Communerd's
full signature over the full record*, and (where applicable) full inner proofs.

**FamilyRecord is always `CleanFullyAuthenticated` — both sender and receiver.**
Unlike stamp/verify/tick (where a single-Ed25519 `CleanAuthenticated` fast gate
suffices for the hot path), a `FamilyRecord` is the **root of family trust** and
must reach **full k-way verification before it is used or cached**. There is no
acting on a merely envelope-authenticated FamilyRecord.

| Wrapper | Applies to FamilyRecord? | What has been verified |
|---------|--------------------------|------------------------|
| `Unprocessed<FamilyRecord>` | transient only | nothing — parsed from the connection |
| `CleanAuthenticated<FamilyRecord>` | **not used** | (skipped — no envelope-only trust for family records) |
| `CleanFullyAuthenticated<FamilyRecord>` | **required** | Communerd's full dual-key signature over the record **and** all k-way member dual-key attestations (Ed25519 ‖ SLH-DSA) |
| (policy) temporal | escalation | Foretis vs Chronomatter ChrononRecord |

- **Sender side:** a family produces a `FamilyRecord` only once the full k×k
  matrix is complete; it holds and serves it as `CleanFullyAuthenticated<FamilyRecord>`.
  A half-built (incomplete matrix) record is never emitted.
- **Receiver side:** an `UnverifiedSignatureEnvelope<FamilyRecord>` must reach
  `CleanFullyAuthenticated<FamilyRecord>` (entire matrix verified, via the
  verifying constructor) before it is cached or used to start a Communerdette.

The matrix entries are **full dual-key signatures** (Ed25519 ‖ SLH-DSA each), and
the matrix is k×k, so reaching `CleanFullyAuthenticated<FamilyRecord>` is
**expensive** (k² SLH-DSA verifications, bounded by `MAX_FAMILY_MEMBERS`). This is
the deliberate cost of establishing the root of family trust, acceptable because
FamilyRecords are rare and off the hot path. This is the same
`CleanFullyAuthenticated` tier introduced for channel-binding dual-key proof —
here it denotes full matrix proof (plus the Communerd transport envelope).

**Consolidated signing review (all record/operation types):**

| Record / op | Inner signer(s) | Outer envelope | Fast gate (→ CleanAuthenticated) | Deeper (→ CleanFullyAuthenticated / policy) |
|-------------|-----------------|----------------|----------------------------------|---------------------------------------------|
| `FamilyRecord` | k-way: all members | Communerd (last) | Communerd envelope | all member attestations; slow-key; Foretis |
| `/stamp` Foretis | Chronomatter | Communerd (last) | Communerd envelope | Chronomatter sig vs ChrononRecord |
| `/verify` result | Chronomatter | Communerd (last) | Communerd envelope | Chronomatter sig |
| `get_tick` / slice | Calendar | Communerd (last) | Communerd envelope | Calendar attestation in family `calendar_tbids` |
| FB / GNF (ProbityReport) | Calendar | Communerd (last) | Communerd envelope | reporter ∈ sender family `calendar_tbids` |

**Communerd signs FamilyRecord twice — by design (decided).** FamilyRecord
carries the Communerd's signature in two distinct places over distinct bytes:
(a) as a **member attestation** inside the k-way set (Communerd consents to the
manifest, as a family member), and (b) as the **transport envelope** wrapping
the delivery. These are kept separate permanently: (a) is durable family proof
that travels with the record; (b) is per-hop transport auth that does not.

---

## 20. FamilyRecord in Toppoli Tests

Each toppoli peer is a `TimeFamilyServer` — the name now correctly describes the
concept. With `with_communerd = true`, each server publishes its thin DHT
pointers and serves its `FamilyRecord` on connection. Tests inspect the Family
Cache directly:

```rust
let family = harness.peer(0).server()
    .communerd().unwrap()
    .family_cache_lookup(&remote_communerd_tbid);
assert!(family.calendar_tbids.contains(&expected_calendar_tbid));
```

---

## 21. Canonical Encoding and Signature Wrappers (Phase 16)

This standardizes **how records become bytes for signing** and **how signatures
attach to payloads**, replacing the current per-record hand-rolled `canonical()`
functions (which diverge in endianness and framing — see the divergence between
`PeerRegistrationRecord`, `ProbityReport`, and `Foretis` `sig_input`).

### 21.1 Canonical Serializer

Signing bytes are produced by a **single deterministic binary serializer**, not
per-record hand-rolled encoders and **not** `serde_json`.

- **Recommended: `postcard`** — deterministic (fields in declaration order, fixed
  integer encoding, no map-key ambiguity), maintained, `no_std`, compact. One
  call — `postcard::to_allocvec(&payload)` — replaces every hand-rolled
  `canonical()`.
- `serde_cbor` stays where it is (encrypted storage blobs, `encrypted_jsonl.rs`);
  it is **not** used for signing (unmaintained; CBOR-canonical needs care).
- `serde_json` is never used for signing bytes.

(@human — crate choice (`postcard` vs `bincode` v2) is the one open decision in
this section. Both are deterministic; `postcard` is recommended for stability +
`no_std`. Confirm before implementation.)

A record is signable iff it derives `Serialize` and contains **no** signature
field (signatures live in the wrappers, §21.2). This makes "exclude the
signature when signing" **structural** rather than a runtime blanking step that
can be forgotten.

### 21.2 Signatures Are Part of the Trust-Boundary Type System

There is **no standalone `Signed<P>` type**. The ordered signature list is a
field carried *through* the existing trust-boundary wrappers — signing is rolled
into the same type-checked authorization system as cleansing/authentication.

Every wrapper holds a **signature-free payload `R`** plus an **ordered list** of
signatures. Each signature covers the payload **plus every signature that
precedes it** in the list (the flattened equivalent of nested signing: each
signer appends an entry attesting to the accumulated prefix).

```rust
pub struct SignatureEntry {
    pub role:      SignerRole,    // Chronomatter | Calendar | Member | CommunerdEnvelope
    pub tbid:      String,        // signer TBID hex
    pub algorithm: SigAlgorithm,  // Ed25519 (fast) | DualKey (Ed25519 ‖ SLH-DSA)
    pub sig:       Vec<u8>,
}
```

**The wrapper states (renamed and extended):**

```rust
/// Parsed from the wire; payload + signatures present but NONE verified.
/// Renamed from `Unprocessed<R>` — everything inbound is a signature envelope,
/// so the name says what it is. The alias screams its danger.
pub struct UnverifiedSignatureEnvelope<R> {
    payload:    R,                     // signature-free
    signatures: Vec<SignatureEntry>,   // unverified
}
pub type DontUse<R> = UnverifiedSignatureEnvelope<R>;

/// Fast gate passed: the last (Communerd envelope) signature is verified.
pub struct CleanAuthenticated<R> { /* payload R + signatures */ }

/// Full gate passed: every signature verified, last is a dual-key envelope.
pub struct CleanFullyAuthenticated<R> { /* payload R + signatures */ }
```

`CleanAuthenticated<R>` and `CleanFullyAuthenticated<R>` keep their prior meaning
and private-constructor discipline; they now **additionally carry the signature
list** alongside the delegated fields of `R`. Consequently **`R` must itself be
signature-free** — a type that already contains signatures cannot be used as the
payload, because signatures live only in the wrapper.

**Signing rule.** `signatures[i].sig` is computed over
`postcard(payload) ‖ postcard(&signatures[0..i])`:

- `signatures[0]` signs `postcard(payload)`.
- each subsequent signer signs the payload **and** all signatures already present.
- the **last** entry — always the **Communerd envelope** — signs over everything.

**Verification.** To check entry *i*, recompute
`postcard(payload) ‖ postcard(&signatures[0..i])` and verify `signatures[i].sig`.

- **Fast gate (→ `CleanAuthenticated`)** = verify **only the last entry** (the
  Communerd envelope) over payload + all preceding signature bytes (all present
  in the record). A single Ed25519 check; sufficient to route.
- **Full gate (→ `CleanFullyAuthenticated`)** = verify **every** entry, and
  require the last entry to be a full **dual-key** Communerd envelope (§19.10).

**Properties:**
- **Order is locked.** Each entry covers all preceding entries, so the final
  envelope fixes the order and content of the whole chain.
- **No silent downgrade.** Stripping the envelope leaves a record with no final
  Communerd entry, which fails the required envelope check.
- **"Communerd signs twice" is natural** (§19.10): for a FamilyRecord, Communerd
  signs once **inside the payload** (its row of the k×k matrix — endorsing each
  member's TBID) and once as the **envelope** entry in this list (transport).
  Two signatures, different bytes, different purposes.

### 21.3 Composition Per Record

One container; the difference between records is just the `signatures` list:

| Record | `signatures` list (in order) | Inner attestation |
|--------|------------------------------|-------------------|
| Foretis (stamp) | `[Chronomatter(fast), CommunerdEnvelope(fast)]` | — |
| `/verify` result | `[Chronomatter(fast), CommunerdEnvelope(fast)]` | — |
| ChrononRecord block (`get_tick`) | `[Calendar(fast), CommunerdEnvelope(fast)]` | — |
| ProbityReport (FB/GNF) | `[Calendar(fast), CommunerdEnvelope(fast)]` | — |
| FamilyRecord | `[CommunerdEnvelope(dual)]` | **k×k matrix in the payload** |

For most records the `signatures` list carries both the inner application
signature and the Communerd envelope. **FamilyRecord is the exception:** its
familial attestation is **not** in the envelope `signatures` list — it is the
**k×k cross-signing matrix inside the payload** (§19.4), verified at construction.
The wrapper's `signatures` list for a FamilyRecord therefore holds only the
single Communerd transport envelope entry. The two are deliberately distinct
(§19.10): the matrix is durable family content (member-to-member endorsement);
the envelope is per-hop transport. The Foretis anchor is likewise a payload field
(itself a fully-signed Foretis carrying `[Chronomatter, CommunerdEnvelope]`).

### 21.4 The Unified Verification State Machine

There is no separate wire wrapper to compose with — the signature list lives in
the trust-boundary states themselves:

```
bytes on wire → parse → UnverifiedSignatureEnvelope<R>   (alias DontUse<R>)
                        │  payload + signatures, NONE verified
                        │  verify LAST entry (Communerd envelope), fast
                        ▼
                     CleanAuthenticated<R>               (fast gate)
                        │  verify ALL entries; last must be dual-key envelope
                        ▼
                     CleanFullyAuthenticated<R>          (full gate)
                        │  outbound
                        ▼
                     Externalized<R>
```

The `DontUse<R>` alias is deliberate: any code holding one is holding unverified
bytes and must move it through a gate before reading `R` for trust purposes.

### 21.5 Externalization and the Signature Builder

Producing an `Externalized<R>` is **not** an unwrap of an inbound wrapper. Inbound
signatures authenticated the *inbound* hop and the *inbound* signers; an outbound
record needs **its own** signatures. Externalization therefore:

1. copies the signature-free payload fields of `R` into a fresh outbound payload,
2. drops the inbound signature list, and
3. generates a **new** ordered signature list for this transmission (inner agent
   first, Communerd envelope last).

This is expressed with a **builder**, so the ordered signing rule (§21.2) is
enforced step by step and the half-built record is never a usable `Externalized`:

```rust
let out: Externalized<Foretis> =
    Externalized::<Foretis>::builder_from(clean_auth_foretis) // or from a raw R
        .add_signature(SignerRole::Chronomatter, &chronomatter_key)? // entry 0
        .add_signature(SignerRole::CommunerdEnvelope, &communerd_key)? // last
        .build()?;
```

Builder rules:

- `builder_from(src)` accepts a raw signature-free `R` **or** a
  `CleanAuthenticated<R>` / `CleanFullyAuthenticated<R>` (it copies only the
  payload fields; inbound signatures are not carried into the outbound record).
- each `add_signature(role, key)` appends an entry signing
  `postcard(payload) ‖ postcard(&signatures_so_far)` — the ordered rule is
  enforced by construction (you cannot sign out of order).
- `build()` validates that the **last** entry is a `CommunerdEnvelope` and
  returns `Externalized<R>`; it errors otherwise. There is no way to obtain an
  `Externalized<R>` without a terminal Communerd envelope.

(@human — re-transmission of durable third-party proof, e.g. a mirror serving a
ChrononRecord, is a *different* path: there the original inner attestation may be
**retained** as durable evidence while a fresh Communerd envelope is added. If
that path is needed, the builder gains a `carry_signature(entry)` step. Not in
scope until mirroring; noted so the builder API leaves room.)

### 21.6 Migration Notes

- **Rename `Unprocessed<R>` → `UnverifiedSignatureEnvelope<R>`** (alias
  `DontUse<R>`) across `clean_auth.rs` and every call site. This is a large but
  mechanical rename of the existing Take 3 type. Elsewhere in this spec, prose
  that still reads `Unprocessed<R>` denotes the renamed type.
- `CleanAuthenticated<R>` / `CleanFullyAuthenticated<R>` gain a `signatures`
  field; enforce that `R` is signature-free (no `signature`/`attestations`
  fields on payload types — those move into the wrapper).
- Replace `PeerRegistrationRecord::canonical_payload()`, `ProbityReport::canonical()`,
  and the `Foretis` `sig_input` concatenation with `postcard(payload)` over
  signature-free payload structs.
- **`Foretis` is wire-breaking**: its current `sig_input` uses big-endian and
  bare concatenation. Moving to `postcard` changes the signed bytes, so it needs
  a `signature_algorithm`/version bump and a cutover, not a silent swap.
- Existing dual-key signing (`TbidSecret::sign` / `Chronomatter::sign_tbid_message`)
  is reused as the `DualKey` algorithm; no new primitive.
