# Communerdette - Per-TBID Relationship Handle Specification

**Prefix:** `COMMUNERDETTE`
**Pairs with:** `COMMUNERDETTE_PLAN.md`
**Status:** Draft - pending human review
**Date:** 2026-05-23

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
4. Read this document end to end before implementing.

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
  resolves, binds, chooses transport, queues, awaits, retries, records stats.
```

---

## 3. Terminology

| Term | Meaning |
|------|---------|
| Communerd | Global extra-family communication owner. Owns swarm participation, transports, DHT state, peer discovery, and the Communerdette registry. |
| Communerdette | Private per-external-TBID relationship manager. Owns what to do to maintain and use communication with exactly one remote TBID: state, tasks, queues, stats, transport preference, retry policy, and binding status. |
| CommunerdetteLine | Public, cloneable, narrow proxy for Calendar/Chronomatter/TimeFamily use. It physically points at a Communerdette but exposes only safe TBID-scoped operations. |
| External TBID | A TBID not owned by the local family. Each Communerdette is scoped to exactly one external TBID. |
| Local TBID owner | The one Rust object that owns a local TBID and its signing capability. Only that object may sign for that TBID. |
| Binding | Evidence that a transport identity, such as libp2p PeerId or direct Noise key/address, is authorized to speak for a TBID. |
| Route | A currently usable way to reach the external TBID: libp2p direct, custom Noise_XX TCP, or unavailable. |
| `CleanAuthenticated<R>` | A remote record `R` that Communerdette has parsed, checked, and authenticated against the target external TBID. Higher layers may consume it as authenticated remote evidence, but not as proof that the remote statement is semantically true. |

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
   such as `TickRecord` and verifying locally over asking a remote peer for a
   true/false answer.
13. **Fallback is a relationship policy.** Transport fallback is decided using
   the Communerdette's route state and stats, not repeated ad hoc at call sites.
14. **Tasks have owners and shutdown paths.** Per-TBID liveness, queue, and stream
   tasks are owned by the Communerdette and stopped by Communerd shutdown or
   relationship eviction.
15. **Existing transports remain valid.** libp2p direct remains preferred where
    available; custom Noise_XX TCP remains supported as fallback and for direct
    known peer calls.

---

## 5. Public Interface: CommunerdetteLine

`CommunerdetteLine` is the only type intended for Calendar/Chronomatter/TimeFamily
callers.

```rust
#[derive(Clone)]
pub struct CommunerdetteLine {
    target_tbid: Tbid,
    inner: Arc<Communerdette>,
}
```

The actual fields should stay private. Public methods should be narrow and
TBID-scoped:

```rust
impl CommunerdetteLine {
    pub fn target_tbid(&self) -> Tbid;

    pub async fn stamp(
        &self,
        content: Vec<u8>,
        echo: String,
    ) -> Result<CleanAuthenticated<Foretis>, TransportError>;

    pub async fn get_calendar_slice(
        &self,
        tick_start: u64,
        count: u64,
    ) -> Result<CleanAuthenticated<Vec<TickRecord>>, TransportError>;

    pub async fn get_tick(
        &self,
        tick_number: u64,
    ) -> Result<CleanAuthenticated<TickRecord>, TransportError>;

    pub async fn start_calendar_stream(
        &self,
        from_tick: u64,
    ) -> Result<CalendarStreamHandle, TransportError>;

    pub fn status_summary(&self) -> CommunerdetteStatusSummary;
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
- CommunerdetteLine exposes only safe operations for that relationship.

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

The Communerdette must represent this distinction even if the first
implementation only supports `ClaimedByDht`.

Expected future proof shape:

```text
local -> remote:
  challenge = random nonce + local PeerId + remote PeerId + target TBID

remote -> local:
  signature over challenge using TBID-controlled signing capability
  optional current TickRecord or TBID public proof material

local:
  verify signature against TBID proof rules
  mark binding Verified
```

Until this is fully implemented, trust-bearing code must avoid pretending that
a DHT claim is equivalent to a verified TBID binding.

---

## 11. Remote Authentication Product

Communerdette is responsible for authenticating the other TBID. It may use the
Foretias crypto libraries, verified tick material, binding proofs, transport
identity, DHT claims, and protocol-specific signatures to decide whether a
remote record is clean enough for higher layers.

The intended type-level shape is:

```rust
pub struct CleanAuthenticated<R> {
    target_tbid: Tbid,
    record: R,
    proof: RemoteAuthenticationProof,
    authenticated_at_ns: u64,
}
```

The exact fields may differ, but the fields and constructors must stay narrow.
Only Communerdette or a small verifier helper owned by the `communerd` module
should be able to construct `CleanAuthenticated<R>` from untrusted remote data.

A `CleanAuthenticated<R>` value means:

- the remote bytes were parsed using the expected schema for `R`
- the claimed remote TBID matches the Communerdette's target TBID
- required signatures, hashes, tick references, or binding proofs were checked
- the route/binding state was strong enough for the operation's trust level
- protocol freshness or replay checks were applied where the protocol defines
  them

A `CleanAuthenticated<R>` value does not mean:

- the remote statement is morally or semantically true
- the local node agrees with the remote statement
- Communerdette signed anything
- Communerdette may sign for any local TBID

This creates a clean inbound pipeline:

```text
transport bytes
  -> untrusted parsed record
  -> Communerdette authentication checks
  -> CleanAuthenticated<R>
  -> Calendar / Chronomatter-adjacent code / TimeFamily code
```

Calendar may store trust-bearing external-attestation material only after the
relevant remote record has become `CleanAuthenticated<R>` and any
Calendar-specific policy checks have also passed.

---

## 12. Local TBID Signing Authority

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

## 13. Calendar and Chronomatter Usage

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

## 14. Acceptance Criteria

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
7. The plan includes an explicit step to address current liveness ping routing:
   either implement JSON-RPC `ping` on the server or switch liveness to
   transport-native probes.
8. No Communerdette or CommunerdetteLine API can sign as Calendar,
   Chronomatter, or another local TBID owner. Outbound messages that claim a
   local TBID are signed by the owning Rust object before handoff to
   Communerdette.
9. Remote records returned to Calendar or Chronomatter-adjacent code for
   trust-bearing use are wrapped in `CleanAuthenticated<R>` after
   Communerdette verifies the remote TBID and record.

---

## 15. Non-Goals

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
