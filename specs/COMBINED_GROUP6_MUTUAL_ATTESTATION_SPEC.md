# COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md
# Mutual Attestation — Fast Buddies (scheduled, bilateral) and GanzNeueFreundschaft (random, one-way)

**Prefix:** `COMBINED_GROUP6_MUTUAL_ATTESTATION`
**Group:** COMBINED_GROUP6
**Pairs with:** `COMBINED_GROUP6_MUTUAL_ATTESTATION_PLAN.md` (to be written after this spec is approved)
**Status:** Draft — second revision incorporating human review feedback. **Execution DEFERRED until Communerdette (`COMBINED_GROUP7_COMMUNERDETTE_SPEC.md`) reaches feature completion.** This spec depends load-bearingly on `MirrorDispatcher`, `CommunerdetteLine`, and the per-TBID lifecycle state machine that Group 7 owns; finalizing FB/GNF before Communerdette is settled risks rework. Spec authoring continues in parallel with Communerdette; implementation does not begin until Group 7 lands AND any required interface updates land in this spec.
**Date:** 2026-05-24 (deferral notice 2026-05-26)
**Master Coordination:** `COMBINED_GROUP2_SPEC.md` §2.2

---

## Completed Prerequisites

The following specifications are code-complete and merged to `alpha`. This
design depends on the types, traits, and behaviors they introduce:

- **`COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md`** —
  `Unprocessed<R>` → `CleanAuthenticated<R>` → `Externalized<R>` triple with
  private constructors and the inbound `verify()` gate. All remote records
  consumed by Mutual Attestation enter through this gate.
- **`COMBINED_GROUP7_COMMUNERDETTE_SPEC.md`** — Communerdette + CommunerdetteLine
  as the per-TBID relationship handle. FB cadence reuses the per-TBID line
  for transport; GNF uses the lighter `what_time_do_you_have` /
  `witness_attestation` pair without holding a Communerdette open.
- **`COMBINED_GROUP4_SPEC.md` §3 (Calendar Active Mirroring, Phases 4b.1-4b.5
  merged)** — Calendar task queue with `MirrorState`, `MirrorDispatcher`,
  and worker pool. FB and GNF tasks extend the same queue.
- **`ExternalAttestation` (in `core-engine/src/foretias/external_attestation.rs`)** —
  the record type produced by a successful attestation, attached to a
  `ChrononRecord.external_attestations`. This spec extends it with an
  `AttestationKind` field but does not redefine the existing fields.

---

## Reading Order

1. Read `FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` (backburnered
   v0.2 spec) — the original two-node demo this spec generalizes.
2. Read `COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` §6 (private interface) and
   §7 (relationship state) for the model FB cadence extends.
3. Read `COMBINED_GROUP4_SPEC.md` §3.2 (Calendar Priority Hierarchy) for
   where mutual-attestation tasks live in Calendar's queue.
4. Read this document end to end before implementing or estimating.

---

## 1. Goal

Make mutual attestation **the load-bearing reliability mechanism** of the
Foretias network. A Calendar's confidence in a remote TBID's chrononchain
should grow primarily through repeated, observed mutual attestations across
epochs — not through any single high-stakes ceremony.

Two distinct cadences run in parallel (when enabled), each with its **own
dedicated wire API surface** so an observer can distinguish them by RPC
method name. The cadences are differentiated by **the initiator's
motive** — which determines content-flow direction, which calendar's
attestation store grows, and which side carries the freshness obligation:

| Cadence | Initiator's motive | Content flow | Whose calendar grows | Wire API (protocol-specific) | Discovery | Trigger | Trust signal |
|---------|--------------------|--------------|----------------------|------------------------------|-----------|---------|--------------|
| **Fast Buddies (FB)** | "I want stamps for MY chronon, NOW." | **Push**: A pushes its own content (a ChrononRecord to be stamped) to its FBs. | The initiator's (A's) calendar — A's `ChrononRecord.external_attestations` grows. | `bruderschaft_init`, `fb_stamp_demand`, `is_fast_buddy` | Derived from mirroring + Brüderschaft initiation (P2P mode); preconfigured TBID list (Server mode) | End of each (local) epoch | "We have a maintained, bilaterally-declared relationship; my FBs respond to my demand with high alacrity." |
| **GanzNeueFreundschaft (GNF)** | "I want to bear witness to a stranger." (German: "Eine ganz neue Freundschaft" — "a completely new friendship.") | **Pull, then push-stamp**: A pulls B's current chronon (via wtdyh), stamps it locally, then pushes the stamp to B. | The responder's (B's) calendar — B's `ChrononRecord.external_attestations` grows. A's outbound-attestation store grows in parallel. | `witness_attestation` (GNF-specific) + base `wtdyh` (shared) | Random sample from known peer pool | Jittered scheduler across the epoch | "I (and others doing GNF) am willing to bear witness for, and be witnessed by, strangers. This particular exchange is a one-off." |

**Base (non-protocol-specific) API.** Three primitives are open to any
caller and are NOT FB- or GNF-flavored:

- `stamp(content)` — produce a Foretis stamping the caller-supplied content.
  The existing v0.1 primitive.
- `verify(foretis, content)` — verify a Foretis against content. v0.1.
- `what_time_do_you_have()` (`wtdyh`) — return the responder's current
  chronon info (plus an optional `not_accepting_attestations` flag for
  dormancy hinting). Added by this spec.

FB and GNF compose on top of these. GNF uses `wtdyh` to learn what to
stamp; FB does not need `wtdyh` because the initiator already has the
content it wants stamped. Both protocols use the underlying signature
mechanism that `stamp`/`verify` exposes, even though FB invokes it via
the FB-specific `fb_stamp_demand` (which is `stamp` + an FB-membership
gate).

Both cadences MUST fire reliably when enabled (FB more so — it is
deterministic; GNF is rate-limited but its non-occurrence is itself a
probity signal).

**Operating modes** (§5.1):

| Mode | FB | GNF | Typical use |
|------|----|----|-------------|
| **Server** | Yes, against a preconfigured TBID list | Disabled | Closed deployments where peers are prearranged — e.g., a company's private time zone, a federation with explicit peering agreements. Verification depth typically dialed down because everyone is known-good. |
| **P2P** | Yes, derived from mirroring + Brüderschaft | Yes | Open network. Verification typically at Full level to defend against unknown peers. |

Both modes use the SAME wire protocols and the SAME storage paths.
The differences are: (a) where FB candidates come from, (b) whether
GNF runs, (c) how deeply we verify (configurable per-mode).

---

## 2. Problem

The Foretias chrononchain is locally consistent (own ticks chain via
`aa_nonce` and Foretis links). Local consistency tells an outside observer
nothing about whether the Calendar's progression matches the network's: a
Calendar could run its ticks at any rate it likes in private.

Mutual attestation closes the gap by producing **bilateral evidence**: peer
B stamps A's chronon at A's chronon number N with B's signature at B's own
chronon M. The pair `(A's tick N, B's stamp at M)` is checkable offline by
any third party who can verify the two signatures.

For this to be a usable network-level health signal:

1. It must be **reliable** — predictable cadence, not best-effort.
2. It must be **abundant** — enough exchanges that missing one is a
   detectable anomaly.
3. It must include **diverse counterparties** — repeated attestation only
   with one peer becomes a single point of trust failure.
4. It must **distinguish** stable relationships (Fast Buddies) from
   single-encounter exchanges (GanzNeueFreundschaft) so downstream consumers can
   weight them appropriately, both in wire traffic and in stored evidence.

Today the codebase has the wire primitives (`stamp_peer`, `route_stamp`,
mutual attest observer hooks) and a static-peer `every_n_chronons` cadence
inherited from v0.2. This spec lifts that into the two relationship-aware
cadences above.

---

## 3. Terminology

| Term | Meaning |
|------|---------|
| **Chronon** | The native time unit. One chronon = one tick on a Calendar's chrononchain. There is no wall-clock cadence in Foretias semantics; all timing is measured in chronons of the local Calendar. |
| **Epoch (local)** | A fixed number of consecutive chronons defined per Calendar via `epoch_length_chronons`. Epoch N spans chronons `[N * epoch_length_chronons + 1, (N+1) * epoch_length_chronons]`. Each Calendar's epochs are entirely local — there is no network-wide global epoch in this spec. |
| **FROST EpochSnapshot (orthogonal)** | A separately-defined network-level concept in `p2p/core-engine/src/epoch/snapshot.rs` for FROST probity scoring. It uses its own wall-clock-driven scheduler and is unrelated to FB/GNF cadence. Operators MAY align the two cadences in configuration but the two are independent. |
| **Mutual Attestation Exchange (MAE)** | A protocol exchange where one Calendar produces and stores an attestation of another Calendar's chronon. The FB MAE is intrinsically bilateral (both sides exchange stamps per epoch); the GNF "MAE" is intrinsically one-way per exchange (initiator stamps responder; responder stores via Witness API). |
| **Fast Buddy (FB)** | A remote TBID T that the local Calendar has explicitly entered into via the Brüderschaft protocol AND continues to back up to / back up from across epochs. FB status is bilaterally declared, maintained per epoch, and stored. Calendar exposes a yes/no API. **Initiator motive**: A Calendar pushes content it wants stamped to its FBs and expects high-alacrity stamping in return. FBs are committed responders. |
| **Brüderschaft protocol** | The initiation handshake that promotes a peer pair into FB. Requires four cross-stamps (origin chronon + last-completed-epoch chronon, each direction). |
| **GanzNeueFreundschaft (GNF)** | Literally "a completely new friendship" in German — the name describes the *relationship state* of an exchange treated as never-before-seen, not a person ("Freundschaft" is the state-noun "friendship," not the agent-noun "friend"). A GNF exchange involves a remote TBID treated as never-before-seen for the purposes of a single attestation, even if we have history with it. **Initiator motive**: a Calendar that wants to bear witness for a stranger pulls the stranger's current chronon (via `wtdyh`), stamps it, and pushes the stamp into the stranger's Witness API. GNF MAEs are one-way per exchange, fire-and-forget, not promoted into FB. |
| **Witness API** | The receiver-side API of GNF: an open `witness_attestation` RPC that accepts an inbound attestation, verifies it, and stores it on the local Calendar's `ChrononRecord.external_attestations`. |
| **`wtdyh` (what_time_do_you_have)** | **Base** (non-protocol-specific) universal time-probe RPC. Always returns the responder's current chronon info; may additionally signal dormancy and attestation acceptance status. Used by GNF to learn what to stamp, but also available to any caller for any purpose. Not gated by FB-membership. |
| **Push (FB)** | The initiator brings its own content to the responder for stamping. "Push of content." |
| **Pull-then-stamp (GNF)** | The initiator first pulls the responder's content (via `wtdyh`), stamps it locally, then pushes the stamp back to the responder. The data direction is asymmetric in two phases. |
| **Outbound attestation store** | A per-Calendar record of attestations the local node has **given to others** (i.e., foretis we signed about someone else's chronon, dispatched via FB or GNF). Distinct from the existing `ChrononRecord.external_attestations`, which records attestations **received from others** about our own chronons. |

---

## 4. Design Invariants

1. **Local Calendar owns initiation.** Only Calendar may decide to enqueue
   any MAE — never Communerd, Chronomatter, or the server transport layer.
2. **CommunerdetteLine carries FB exchanges; lightweight RPC carries GNF.**
   FB-MAE goes through `CommunerdetteLine` because we have an ongoing
   relationship to track. GNF goes through stateless `wtdyh` +
   `witness_attestation` RPCs because each exchange is one-off.
3. **FB and GNF protocol-specific wire methods are strictly
   non-overlapping.** A node observing RPC traffic can tell which cadence
   is running by the protocol-specific method name alone. FB-specific:
   `bruderschaft_init`, `fb_stamp_demand`, `is_fast_buddy`. GNF-specific:
   `witness_attestation`. The **base API layer** (`stamp`, `verify`,
   `wtdyh`) is shared and may be invoked outside either cadence. GNF uses
   `wtdyh` to learn what to stamp; FB does not need `wtdyh` because the
   initiator already holds the content. Observing a `wtdyh` call in
   isolation does not identify the cadence; observing a subsequent
   `witness_attestation` does (→ GNF). Observing `fb_stamp_demand` does
   (→ FB).
4. **MAE failure is normal.** Network and peer faults are expected. A
   single MAE failure must never block other MAEs, must never crash the
   task worker, and SHOULD produce a `ProbityReport` that other peers can
   observe.
5. **FB cadence is deterministic at the local epoch boundary.** For every
   local epoch E and every Fast Buddy F, the local node MUST enqueue
   `DoFastBuddyMAE { peer_tbid: F, epoch: E }` before processing any
   chronon belonging to epoch E+2. (The window is "you have one full
   epoch to attempt" — Invariant 5 in the abandonment rule below.)
6. **GNF cadence is jittered and bounded — and only runs in P2P mode.**
   Per local epoch, GNF initiates at most `gnf_peers_per_epoch`
   attestations against randomly selected non-FB peers. The actual launch
   instants are spread across the epoch by a jittered scheduler. The GNF
   scheduler does NOT start in Server mode.
7. **GNF treats every encounter as stateless.** Even if a peer has been
   GNF-attested by us before, the GNF cadence MUST NOT pre-load any
   reputation. (FB is where prior history becomes load-bearing.)
8. **GNF never promotes to FB.** The only path into FB is `bruderschaft_init`
   (§6.1). A long history of GNF attestations with the same peer does NOT
   make them an FB.
9. **No mutual attestation with self.** A node MUST NOT initiate an MAE
   with its own TBID, nor accept one. Self-attestation is meaningless and
   corrupts the attestation graph.
10. **Stored attestations are tagged.** Every `ExternalAttestation` carries
    an `AttestationKind` enum distinguishing `FastBuddy` from
    `GanzNeueFreundschaft`. A third party reading a Calendar's stored records can
    tell which cadence produced each attestation.
11. **No attestation may bypass the Take 3 inbound gate.** Records stored
    in `ChrononRecord.external_attestations` or in the outbound store come
    from `CleanAuthenticated<R>` only, never from raw `Unprocessed<R>`.
11a. **Beneficial-attestation gate.** Take 3 verifies cryptographic
    integrity. The beneficial-attestation gate verifies that the attester
    is the kind of peer whose attestations are worth storing — recoverable
    (signed DHT record exists, peer is locatable later) and genuine
    (their chrononchain is internally consistent). See §6.1.5 (Brüderschaft
    deep verification), §6.7 (FB sporadic re-verification), and §7.3 step
    2a (GNF per-attestation verification). An attestation that passes
    Take 3 but fails the beneficial gate is REFUSED — not stored, and
    the failure is logged in probity.
12. **Dormancy fully suppresses outbound attestation.** While the local
    node is dormant (per `CollisionDetector`), it initiates NO FB MAE and
    NO GNF attestation. This is non-configurable — outbound during
    dormancy can deepen a collision.
13. **Dormancy inbound is operator-configured.** wtdyh always returns
    time. The `accepting_attestations` field in the wtdyh response, and
    whether the Witness API actually accepts incoming attestations during
    dormancy, are both controlled by a single operator policy
    (`dormant_inbound: Strict | Permissive`). Strict refuses (default);
    Permissive accepts and grows Calendar with witnesses.
14. **No wall-clock cadence anywhere in this spec.** All timing is in
    chronons. FROST's wall-clock scheduler is a separate concern.

---

## 5. Public Interfaces

### 5.1 Configuration

Lives in `core-engine/src/config/node.rs`:

```rust
/// Mutual-attestation cadence configuration, per Calendar.
pub struct AttestationConfig {
    /// Operating mode. Drives FB candidate discovery and whether GNF
    /// runs at all. Default: `OperatingMode::P2P`.
    pub mode: OperatingMode,

    /// Length of a local epoch, in chronons.
    /// Default: 60.
    pub epoch_length_chronons: u64,

    /// Maximum simultaneous FB count.
    /// Default: 16.
    pub max_fast_buddies: usize,

    /// Number of GNF attestations to initiate per local epoch.
    /// Ignored when `mode == Server` (GNF disabled in Server mode).
    /// Default: 8.
    pub gnf_peers_per_epoch: u32,

    /// Cache TTL for Communerd's wtdyh answer, in chronons.
    /// Default: 1 (refresh on every tick advance).
    pub wtdyh_cache_ttl_chronons: u64,

    /// Inbound attestation behavior while dormant.
    /// Default: Strict.
    pub dormant_inbound: DormantInbound,

    /// Whether FB cadence is enabled (set to false during tests or for
    /// archival-only nodes). Default: true.
    pub fb_enabled: bool,

    /// Beneficial-attestation verification settings.
    pub verification: VerificationConfig,
}

/// Two operating modes — see §1.
pub enum OperatingMode {
    /// Closed deployment. FB candidates come from a preconfigured TBID
    /// list (e.g., a company's set of in-house time families). GNF is
    /// disabled because there is no "random open network" to sample.
    /// Brüderschaft still runs against the prearranged TBIDs as the
    /// standard initiation protocol; verification depth is controlled
    /// by `VerificationConfig::level` (typically `Minimal` or `None`).
    Server {
        /// TBIDs the local node will automatically initiate Brüderschaft
        /// against at startup, in order. The list is the SOLE source of
        /// FB candidates in this mode.
        prearranged_fast_buddies: Vec<Tbid>,
    },

    /// Open peer-to-peer network. FB candidates are derived from
    /// mirroring relationships per §6.2. GNF runs on a jittered
    /// cadence per §7.1. Verification typically at Full level.
    P2P,
}

/// Tuning knobs for the beneficial-attestation gate (§6.1.5, §6.7, §7.3).
/// All checks default to enabled at `VerificationLevel::Full`; operators
/// can relax them for closed testbeds where every peer is known-good.
pub struct VerificationConfig {
    /// Master verification depth. Per-check fields below are honored
    /// only when `level == Full` (or `Minimal` for the per-check
    /// listed in `Minimal`'s description).
    pub level: VerificationLevel,

    /// How many full epochs of the peer's chrononchain to fetch and
    /// verify during Brüderschaft initiation. Each epoch = at least
    /// `epoch_length_chronons` records to fetch + chain-verify.
    /// Honored at `Full` and `Minimal`. Default: 2.
    pub bruderschaft_verify_epochs_back: u32,

    /// Re-run §6.7 deep verification once every N FB-MAEs against a
    /// given peer. Honored at `Full`. Default: 10.
    pub fb_reverify_every_n_maes: u32,

    /// Require GNF attesters to have a signed DHT registration record
    /// (g3-d) before the Witness API stores their attestation. Honored
    /// at `Full`. Default: true.
    pub gnf_require_dht_record: bool,

    /// Maximum time the Witness API will spend on a DHT lookup for an
    /// unknown GNF attester's registration record. Honored at `Full`.
    /// Default: 2000 ms.
    pub gnf_dht_lookup_timeout_ms: u32,

    /// Refuse Brüderschaft when the candidate's probity score is
    /// strictly below this threshold. Honored at `Full`. Default: 0.0.
    pub bruderschaft_min_probity: f32,
}

pub enum VerificationLevel {
    /// All §6.1.5, §6.7, §7.3 checks active. The default for P2P mode
    /// because peers are presumed unknown.
    Full,
    /// Chain-integrity check on Brüderschaft (§6.1.5 step 3) remains
    /// active to catch outright garbage chrononchains. Other checks
    /// (DHT recoverability, probity floor, periodic FB re-verify) are
    /// skipped. Reasonable middle ground for a federation of
    /// somewhat-trusted operators.
    Minimal,
    /// Skip the beneficial-attestation gate entirely. Take 3 crypto
    /// checks still run — signatures, TBID match, chain links inside
    /// records still verify — but no extra fetches, no DHT lookup, no
    /// probity check, no slice-fetch on Brüderschaft.
    /// Intended for `OperatingMode::Server` with prearranged FBs where
    /// every peer is already known-good. Setting this in P2P mode is
    /// possible but invites accepting garbage attestations from
    /// unknown peers.
    None,
}

/// Tuning knobs for the beneficial-attestation gate (§6.1.5, §6.7, §7.3).
/// All checks default to enabled; operators can relax them for closed
/// testbeds where every peer is known-good.
pub struct VerificationConfig {
    /// How many full epochs of the peer's chrononchain to fetch and
    /// verify during Brüderschaft initiation. Each epoch = at least
    /// `epoch_length_chronons` records to fetch + chain-verify.
    /// Default: 2.
    pub bruderschaft_verify_epochs_back: u32,

    /// Re-run §6.7 deep verification once every N FB-MAEs against a
    /// given peer. 1 = every MAE (expensive), 10 = roughly every 10
    /// epochs. Default: 10.
    pub fb_reverify_every_n_maes: u32,

    /// Require GNF attesters to have a signed DHT registration record
    /// (g3-d) before the Witness API stores their attestation.
    /// Default: true. Set false only on closed testbeds.
    pub gnf_require_dht_record: bool,

    /// Maximum time the Witness API will spend on a DHT lookup for an
    /// unknown GNF attester's registration record. After this, the
    /// attestation is refused. Default: 2000 ms.
    pub gnf_dht_lookup_timeout_ms: u32,

    /// Refuse Brüderschaft when the candidate's probity score is
    /// strictly below this threshold. Default: 0.0 (don't accept
    /// negative-probity peers as FB).
    pub bruderschaft_min_probity: f32,
}

pub enum DormantInbound {
    /// wtdyh returns `accepting_attestations: false`; Witness API rejects
    /// incoming `witness_attestation` with a `dormant` error.
    Strict,
    /// wtdyh returns `accepting_attestations: true` regardless of
    /// dormancy; Witness API accepts and stores. Calendar grows during
    /// dormancy.
    Permissive,
}
```

### 5.2 Calendar Hooks

Calendar gains four new public methods:

```rust
impl Calendar {
    /// Yes/no query: is the given TBID a Fast Buddy of this Calendar?
    /// This is the authoritative source of truth. Communerd, the
    /// Witness API, and remote peers (via the `is_fast_buddy` RPC) all
    /// route through this.
    pub fn is_fast_buddy(&self, tbid: Tbid) -> bool;

    /// Snapshot of the current FB set for tests and diagnostics.
    pub fn fast_buddies(&self) -> Vec<Tbid>;

    /// Snapshot of outbound attestations this Calendar has dispatched.
    /// Append-only.
    pub fn outbound_attestations(&self) -> Vec<OutboundAttestation>;

    /// Run the FB-recompute pass — typically fired at end-of-epoch and
    /// on PeerChangeCallback. Idempotent.
    pub fn recompute_fast_buddies(&self);
}
```

### 5.3 Task Queue Variants

The existing `CalendarTask::DoAttestation` (placeholder in Phase 4b.4) is
removed in favor of:

```rust
pub enum CalendarTask {
    // ...existing 6 variants from Group 4b...

    /// Initiate Brüderschaft with a candidate peer, promoting them to
    /// Fast Buddy iff both sides successfully cross-stamp origin and
    /// last-completed-epoch chronons. Idempotent — re-initiation against
    /// an existing FB is a no-op.
    InitiateBruderschaft {
        peer_tbid: Tbid,
        triggered_at_chronon: u64,
    },

    /// Fire the per-epoch FB MAE against a known Fast Buddy.
    DoFastBuddyMAE {
        peer_tbid: Tbid,
        epoch_number: u64,
        triggered_at_chronon: u64,
    },

    /// Fire a single GNF attestation against a randomly-selected
    /// non-FB peer. Discovery happens inside the handler; the task
    /// itself carries no specific target.
    DoGanzNeueFreundschaft {
        scheduled_for_chronon: u64,
    },
}
```

### 5.4 Witness API

Hosted at the TimeFamily level (not on Calendar directly — Calendar is the
storage backend; TimeFamily owns the network-facing surface):

```rust
impl TimeFamilyServer {
    /// Called from the JSON-RPC dispatch when a remote node submits a
    /// `witness_attestation` RPC. Verifies the incoming foretis,
    /// matches it to a chronon in the local chrononchain, and (if
    /// dormancy policy allows) stores it.
    pub fn witness_attestation(&self, raw: serde_json::Value)
        -> Result<WitnessOutcome, WitnessError>;
}

pub enum WitnessOutcome {
    /// Verified and stored on ChrononRecord N's external_attestations.
    Stored { local_chronon_number: u64 },
}

pub enum WitnessError {
    /// Caller's foretis didn't pass Take 3 inbound gate (bad sig,
    /// wrong tbid, etc.).
    BadAttestation(String),
    /// Caller stamped a chronon we don't have (e.g., future chronon).
    UnknownChronon(u64),
    /// Local node is dormant with DormantInbound::Strict.
    Dormant,
}
```

### 5.5 Extended ExternalAttestation

```rust
pub struct ExternalAttestation {
    // ... existing fields ...
    pub attester_tbid:        String,
    pub foretis:              Foretis,
    pub attester_tick_record: ChrononRecord,
    pub received_at_ns:       u64,            // existing; retained for backward compat

    /// NEW: which cadence produced this attestation.
    #[serde(default = "default_attestation_kind")]
    pub kind: AttestationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationKind {
    FastBuddy,
    GanzNeueFreundschaft,
}

fn default_attestation_kind() -> AttestationKind {
    // Records persisted before this field existed are treated as GNF
    // (the more conservative interpretation — no FB relationship is
    // inferred from missing tags).
    AttestationKind::GanzNeueFreundschaft
}
```

### 5.6 Outbound Attestation Record

```rust
pub struct OutboundAttestation {
    /// TBID we attested TO.
    pub peer_tbid: String,
    /// Foretis we generated about the peer's chronon.
    pub foretis: Foretis,
    /// The peer's chronon_number we stamped.
    pub attested_peer_chronon: u64,
    /// Our own chronon at the time we dispatched the attestation.
    pub dispatched_at_chronon: u64,
    /// Which cadence dispatched this.
    pub kind: AttestationKind,
}
```

Stored on Calendar in an append-only `outbound_attestations: Vec<OutboundAttestation>`
list. Cap to the most recent N (default 65536) with FIFO eviction beyond
that — the store is for short-term auditing and probity, not permanent
ledger.

---

## 6. Fast Buddy Cadence

### 6.1 Brüderschaft Protocol (Initiation)

FB status is entered ONLY via Brüderschaft. The protocol is bilateral,
explicit, and requires four cross-stamps to succeed:

**Wire surface:** `bruderschaft_init(BruderschaftInitRequest)
-> BruderschaftInitResponse`.

```rust
pub struct BruderschaftInitRequest {
    /// Caller's TBID hex.
    pub initiator_tbid: String,
    /// Caller's origin chronon (chronon #1) as ChrononRecord.
    pub initiator_origin_chronon: ChrononRecord,
    /// Caller's last-completed-epoch chronon as ChrononRecord (the
    /// chronon at `floor(current_chronon / epoch_length) * epoch_length`).
    pub initiator_last_epoch_chronon: ChrononRecord,
    /// Caller's stamp (Foretis) of the responder's claimed origin
    /// chronon, retrieved by the caller via wtdyh + a prior origin-fetch.
    pub stamp_of_responder_origin: Foretis,
    /// Caller's stamp of responder's last-completed-epoch chronon.
    pub stamp_of_responder_last_epoch: Foretis,
}

pub struct BruderschaftInitResponse {
    /// Responder's stamp of initiator's origin chronon (must match the
    /// origin in the request).
    pub stamp_of_initiator_origin: Foretis,
    /// Responder's stamp of initiator's last-completed-epoch chronon.
    pub stamp_of_initiator_last_epoch: Foretis,
    /// Did the responder accept the relationship?
    pub accepted: bool,
    /// If !accepted, free-form reason ("at_capacity", "dormant", "tbid_unknown").
    pub reason: Option<String>,
}
```

**Successful Brüderschaft flow:**

1. Initiator A picks candidate B (typically from the mirror set after a
   successful mirror dump or by operator/test injection — see §6.2).
2. A queries `wtdyh` on B → gets B's `most_recent_chronon_record` and
   `last_epoch_bounds` ([first chronon of last completed epoch, last
   chronon of last completed epoch]).
3. A fetches B's origin chronon via `CommunerdetteLine::get_tick(1)`.
4. A produces two Foretis stamps: one over B's origin chronon, one over
   B's last-completed-epoch chronon.
5. A sends `bruderschaft_init` to B with its own origin chronon, last
   epoch chronon, and the two stamps.
6. B verifies both stamps; if either fails, return `accepted: false,
   reason: "bad_stamp"` and no state changes on either side.
7. B's policy gate: must have capacity (`fast_buddies.len() <
   max_fast_buddies`), must not be dormant under `Strict`, must already
   have a mirroring relationship with A (B mirrors A or A mirrors B).
   If any gate fails, return `accepted: false, reason: <gate>`.
8. B produces its two stamps over A's origin and last-epoch chronons.
9. B verifies its OWN policy first, then stores A in B's `fast_buddies`
   set. Returns `accepted: true` with both stamps.
10. A verifies B's two stamps; on success, A stores B in A's
    `fast_buddies` set.
11. Both sides record an `ExternalAttestation { kind: FastBuddy, ... }`
    for each stamp received (two each → four `ExternalAttestation`
    records produced in total, two on each Calendar).

**Failure modes:** any verification failure aborts the protocol with NO
state changes on either side. Brüderschaft is all-or-nothing.

### 6.1.5 Brüderschaft Beneficial-Attestation Verification (Deep)

Beyond the Take 3 signature checks on the four cross-stamps (§6.1 steps
6, 8, 10), both sides run the verification below before flipping
`is_fast_buddy` to true. Either side failing aborts the protocol with no
state change.

**Active per `VerificationConfig::level`:**
- `Full`: all five steps run.
- `Minimal`: only step 3 (chain integrity) runs.
- `None`: all five steps SKIPPED. Brüderschaft consists only of the four
  cross-stamps; their Take 3 crypto validity is the sole criterion. The
  four `ExternalAttestation` records are still produced and stored
  (audit value preserved). Intended for `OperatingMode::Server` with
  prearranged FBs.

**On the responder side (B), after receiving `BruderschaftInitRequest`
and before deciding to accept:**

1. **Recoverability check.** B confirms initiator A's TBID has a
   signed DHT registration record (g3-d). B consults its local
   `tbid_index` cache; on miss, B does a single bounded DHT lookup
   (`gnf_dht_lookup_timeout_ms`). If A has no signed DHT record,
   refuse with `reason: "no_dht_record"`.

2. **Origin authenticity check.** B verifies the supplied
   `initiator_origin_chronon` is genuinely the genesis tick of A's
   chrononchain:
   - `chronon_number == 1`.
   - Forward-foretis signature verifies against A's TBID.
   - The chronon record itself passes Take 3 gate as a genesis-eligible
     record.

3. **Chain integrity check.** B fetches the most recent
   `bruderschaft_verify_epochs_back` epochs (default 2) of A's
   chrononchain via `CommunerdetteLine::get_calendar_slice(...)`. B
   runs the existing `integrity_check` over the slice — each chronon
   must chain-link correctly to its predecessor (forward_foretis
   verification, aa_nonce continuity, etc.). Any chain break refuses
   with `reason: "chain_break"`.

4. **Last-epoch consistency.** The `initiator_last_epoch_chronon` in
   the request MUST appear in the chain fetched in step 3 (or be the
   tip of it). Mismatch refuses with `reason: "epoch_inconsistent"`.

5. **Probity floor.** B looks up A's current probity score from local
   probity store. If the score is strictly less than
   `bruderschaft_min_probity` (default 0.0), refuse with
   `reason: "below_probity_floor"`.

**On the initiator side (A), after receiving `BruderschaftInitResponse`
with `accepted: true`:**

A runs the same five checks against B (substitute B for A in steps 1-5).
If any fail, A does NOT add B to its `fast_buddies` and emits
`bruderschaft_refused` with the failure reason — even though B accepted
on its side. This bilateral re-check covers the case where B's responder
side is buggy or compromised.

**Cost note.** Brüderschaft initiation involves:
- 1 wtdyh RPC + 1 origin-fetch + 1 last-epoch-fetch (already in §6.1
  setup).
- 1 `bruderschaft_init` round-trip.
- 1 `get_calendar_slice` per side covering `bruderschaft_verify_epochs_back`
  epochs (typically 2 epochs × 60 chronons = 120 records per direction).
- 1 DHT lookup (if not cached).

This is expensive ON PURPOSE — Brüderschaft is a long-term commitment.
The cost is amortized across however many epochs the FB relationship
lasts.

### 6.2 Promotion Trigger

The Calendar enqueues `InitiateBruderschaft` on the following triggers,
which differ by `OperatingMode`:

**Server mode (`OperatingMode::Server { prearranged_fast_buddies }`):**
- At Calendar startup (after `start_task_queue_with_dispatcher`), enqueue
  `InitiateBruderschaft` for every TBID in `prearranged_fast_buddies`,
  in declaration order.
- On periodic retry interval (default: once per epoch) for any
  prearranged TBID that has not yet successfully completed Brüderschaft
  — Server-mode operators expect prearranged peers to eventually
  succeed, so we keep trying.

**P2P mode (`OperatingMode::P2P`):**
- A new mirroring relationship is established (Phase 4b's
  `PeerChangeCallback` fires AND the peer is now in
  `MirrorState::mirrors` AND we have not previously initiated
  Brüderschaft with them this epoch).
- Operator manual command (CLI: `foretias bruderschaft-init <tbid>`).
- Reciprocal initiation: if we receive a successful `bruderschaft_init`
  from B and accept it, no further action — B's invocation completes
  both halves.

**Both modes:**
- There is no automatic promotion via repeated GNF attestations — see
  Invariant 8.

### 6.3 FB End-of-Epoch MAE (Push)

**Motive recap.** A Calendar wants stamps for its own just-completed
epoch chronon — A wants ITS calendar grown with `ExternalAttestation`s
from FBs. The protocol is **a push of content** from A to each FB:
"stamp this, NOW; I am your FB and you committed to alacrity."

When the local Calendar's chronon advances such that
`floor(prev_chronon / epoch_length) < floor(current_chronon / epoch_length)`,
the just-completed epoch's `epoch_number =
floor(prev_chronon / epoch_length)`. For every TBID T in
`fast_buddies`, Calendar enqueues:

```rust
CalendarTask::DoFastBuddyMAE {
    peer_tbid: T,
    epoch_number,
    triggered_at_chronon: current_chronon,
}
```

**Wire surface:** `fb_stamp_demand(FbStampDemandRequest)
-> FbStampDemandResponse`.

```rust
pub struct FbStampDemandRequest {
    pub initiator_tbid: String,
    pub epoch_number:   u64,
    /// The content the initiator wants stamped — typically the
    /// initiator's just-completed-epoch ChrononRecord. The responder
    /// (FB) does not need to fetch this from the initiator; it arrived
    /// in this request.
    pub content_to_stamp: ChrononRecord,
}

pub struct FbStampDemandResponse {
    /// Responder's stamp (Foretis) over `content_to_stamp`. The
    /// chronon_number embedded in this Foretis is the responder's own
    /// current chronon at the moment of stamping — that's how the
    /// initiator learns when the FB witnessed it.
    pub stamp: Foretis,
    /// Responder's confirmation that it still considers the initiator
    /// an FB. If false, the relationship is over from responder's side
    /// (Invariant 5 abandonment — see §6.4). The responder MAY still
    /// produce a stamp out of courtesy even when this flag is false,
    /// or it MAY refuse and return an error instead — implementation
    /// choice.
    pub still_fast_buddy: bool,
}
```

**Worker handler `handle_do_fast_buddy_mae` (initiator side, A):**

```text
1. Check is_fast_buddy(T) is still yes (Calendar may have flipped during
   the latency between enqueue and worker pickup). If no → drop task.
2. Obtain CommunerdetteLine for T.
3. Resolve A's just-completed-epoch ChrononRecord via the local
   chrononchain. Call it CR_a_epoch.
4. Push: line.fb_stamp_demand({ content_to_stamp: CR_a_epoch,
                                 epoch_number,
                                 initiator_tbid: A's TBID }).
5. Receive the response. Verify the stamp via Take 3 inbound gate
   (signature over CR_a_epoch, signed by T's TBID).
6. If verification ok: append ExternalAttestation { kind: FastBuddy,
   attester_tbid: T, foretis: response.stamp,
   attester_tick_record: <fetched lazily via line.get_tick(stamp.chronon_number)
                          OR included in response in a future revision>,
   ... } to A's CR_a_epoch.external_attestations; append our own
   no-op outbound entry (A did not produce a stamp in this exchange,
   but the request itself is auditable).
7. If response.still_fast_buddy == false: Calendar flips
   is_fast_buddy(T) -> false locally too (mirror the unilateral
   abandonment from §6.4).
8. Record FB cycle outcome in MirrorState::fb_state.
```

**Responder handler (FB side, B):**

```text
1. Verify caller's TBID == request.initiator_tbid via transport auth.
2. is_fast_buddy(caller_tbid): if false, return `still_fast_buddy: false`
   AND refuse the stamp (return error) — we're not their FB.
3. Verify request.content_to_stamp passes Take 3 inbound gate as a
   well-formed ChrononRecord signed by caller's TBID.
4. Sign Foretis over content_to_stamp using B's TBID at B's current chronon.
5. Return { stamp, still_fast_buddy: true }.
6. Record outbound entry in B's OutboundAttestation log
   (B provided a stamp; this is B's outbound, not B's inbound).
7. Update fb_state for caller: this counts as a successful FB-MAE
   from B's perspective (B serviced an FB demand).
```

**Storage outcome:**
- A's calendar grew: new `ExternalAttestation { kind: FastBuddy }` on
  A's just-completed-epoch ChrononRecord.
- B's outbound store grew: new `OutboundAttestation { kind: FastBuddy }`
  recording the stamp B provided to A.
- Notably, B's calendar does NOT grow in this exchange. For B's calendar
  to grow on this same epoch, B must initiate its OWN
  `fb_stamp_demand` to A (typically also at epoch end). That second
  exchange is INDEPENDENT — the "mutual" in mutual attestation emerges
  because each side independently demands stamps from the other at end
  of their own epochs.

Errors at any step: record as a failed FB-cycle in MirrorState. Probity
report attribute `"fb_mae_failed"`.

### 6.4 Abandonment Criterion

At end of every local epoch, Calendar runs `recompute_fast_buddies()`.
For each T in `fast_buddies`:

- Examine the just-completed epoch's `fb_state` records for T.
- T remains a Fast Buddy iff during the just-completed epoch the local
  node had **at least one successful interaction in either direction**:
  - We successfully completed an FB-MAE with T (we stamped each other), OR
  - We successfully completed any mirror-dump step with T (Group 4b's
    `history_dump_chunk` succeeded, OR T's dump to us was accepted), OR
  - Both: success in either dimension counts.
- If T had **only failed attempts** during the just-completed epoch (no
  successes in either direction), `is_fast_buddy(T)` flips to false.
  T is removed from `fast_buddies` AND the next attempt by T to call
  `fb_stamp_demand` on us will receive `still_fast_buddy: false`.
- If T had **no attempts at all** (quiet epoch — neither side initiated
  any work), FB status is preserved. Absence of failure ≠ failure.

**Counterparty discovery of abandonment:** When the abandoned counterparty's
worker next tries `fb_stamp_demand`, our handler:
- Detects `is_fast_buddy(caller_tbid) == false`.
- Returns success/failure of THIS stamp normally, but with
  `still_fast_buddy: false`.
- Initiates no further action. Counterparty's worker sees the false flag
  and updates its own `is_fast_buddy(us)` accordingly.

The counterparty can then either:
- Re-initiate Brüderschaft (§6.1) to re-establish the relationship, OR
- Accept termination (no action; their next epoch will not enqueue an
  FB-MAE for us).

### 6.5 The `is_fast_buddy` RPC

Either party may query the other to ascertain FB status:

```rust
fn is_fast_buddy(my_tbid: String) -> bool
```

(Trivial RPC; responder's TBID is implicit from the connection.) The
responder consults its local `Calendar::is_fast_buddy(my_tbid)` and
returns the boolean. No state mutation.

**Use cases:**
- A counterparty unsure of its FB status can poll without forcing an MAE.
- Diagnostics and integration tests.
- After a unilateral abandonment, the abandoned peer can verify the new
  state and respond accordingly.

### 6.6 Reliability Guarantee (FB)

For a local epoch E that the local node fully observed (was not dormant
when E started or ended):

- The local node MUST enqueue `DoFastBuddyMAE` for every TBID that was a
  Fast Buddy at the moment epoch E rolled over.
- The local node MAY enqueue retries within epoch E+1 if early attempts
  in E+1 failed; retry policy is operator-tunable and not load-bearing
  for this invariant.

### 6.7 Sporadic FB Re-Verification

FB-MAEs are cheap (one stamp each direction) but they only verify the
relationship is alive — not that the peer's chrononchain is still
internally consistent across the broader history. A peer who got Brüderschafted
months ago and has since stopped chaining correctly would still produce
valid per-epoch stamps even as their chrononchain rots.

To catch this, the FB-MAE handler runs a **light re-verification** on a
schedule: every `fb_reverify_every_n_maes` (default 10) FB-MAEs against
a given peer, the handler additionally:

1. Increments a per-peer counter `MirrorState::fb_mae_count.get(T)`.
2. If `count % fb_reverify_every_n_maes == 0`:
   - Fetches **the most recent epoch's worth** of T's chrononchain (the
     epoch we are CURRENTLY about to attest). This is typically
     `epoch_length_chronons` records.
   - Runs `integrity_check` on the slice (chain-link + signatures).
   - On chain break: emit `fb_reverify_failed` probity report; record
     in `fb_state` as a critical failure for THIS epoch (counts as a
     "no success" toward the §6.4 abandonment criterion).
3. Otherwise: skip the deep check; the normal FB-MAE stamp suffices.

**Why deep but rare instead of light but always.** A failing-integrity
peer is a serious event (their chronon discipline broke) but a
relatively rare condition. Catching it once per ~10 epochs is fast
enough for operational response while keeping the per-MAE cost bounded
to the stamp-exchange.

**Probity:**
- `fb_reverify_ok` (+0.5) on success.
- `fb_reverify_failed` (-2.0) on chain break — same magnitude as
  `fb_abandoned` because a failing-integrity FB is going to be abandoned
  anyway at end-of-epoch under §6.4.

---

## 7. GanzNeueFreundschaft Cadence

**Motive recap.** A Calendar wants to **bear witness for a stranger** —
A's goal is to grow some random peer B's calendar with a fresh
`ExternalAttestation` and, in passing, to record an outbound stamp in
A's own outbound-attestation store. Crucially, A's own
`ChrononRecord.external_attestations` is NOT what GNF grows from A's
side as initiator; that growth happens when some OTHER node randomly
picks A as ITS GNF target and arrives at A's Witness API. The
network-wide effect of every Calendar independently running GNF is
that each Calendar's external_attestations accretes random witnesses
over time — mutuality emerges across the population, not within each
exchange.

The protocol is **pull-then-stamp**: A pulls B's current chronon (via
the base `wtdyh` RPC, which is open to any caller for any purpose),
stamps it locally, then pushes the stamp to B via the GNF-specific
Witness API.

### 7.1 Scheduler

Calendar spawns a GNF scheduler at `start_task_queue_with_dispatcher`
time. The scheduler:

- Tracks the current local epoch E (derived from
  `current_chronon / epoch_length_chronons`).
- At epoch rollover (E → E+1), enqueues `gnf_peers_per_epoch` instances
  of `CalendarTask::DoGanzNeueFreundschaft` with `scheduled_for_chronon` values
  spread (jittered) across epoch E+1's chronon range.
- The worker pulls each task when its `scheduled_for_chronon` is reached
  (or shortly after).

This gives a steady drip of GNF attestations across each epoch rather
than a burst at epoch start.

### 7.2 Candidate Selection

When a `DoGanzNeueFreundschaft` task is dequeued, the worker selects a
candidate TBID:

1. Gather candidates: peers from `Communerd::known_peers()` and
   `Communerd::tbid_index` (filtered by `validate_peer_registration`
   per g3-d — only signed DHT records).
2. Subtract Fast Buddies — they have their own deterministic cadence;
   don't double-attest.
3. Subtract peers already GNF-attested by us within the current epoch
   (Invariant 6 dedupe).
4. Subtract self TBID (Invariant 9).
5. Pick uniformly at random from the remaining pool. Empty pool →
   debug-log and return.

### 7.3 The GNF Wire Path

**Step 1 — Probe time and acceptance:**

```rust
fn what_time_do_you_have() -> WhatTimeDoYouHaveResponse

pub struct WhatTimeDoYouHaveResponse {
    /// The responder's most recent ChrononRecord (from Chronomatter,
    /// cached by Communerd up to `wtdyh_cache_ttl_chronons`). REQUIRED;
    /// always populated.
    pub most_recent_chronon_record: ChrononRecord,

    /// First and last chronons of the responder's most recently
    /// completed epoch. `None` if the responder has not yet completed a
    /// full epoch; omitted from wire entirely when None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_epoch_bounds: Option<(ChrononRecord, ChrononRecord)>,

    /// Negative-sense flag — `true` means "I am NOT accepting
    /// attestations right now." Defaults to `false` (i.e., accepting);
    /// omitted from wire when default. Driven by dormancy + the
    /// `DormantInbound` policy on the responder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_accepting_attestations: bool,
}
```

**Wire economics:** A normal-operation response from a non-dormant node
serializes to just the `most_recent_chronon_record` (plus `last_epoch_bounds`
if at least one epoch has rolled over). The `not_accepting_attestations`
field is only on the wire when the responder is explicitly refusing —
keeping the common-case response minimal and making refusal an
unambiguous, positive signal.

**Initiator's read:**
- Field absent or `false` → proceed with `witness_attestation`.
- Field present and `true` → abort the GNF exchange (no
  `witness_attestation` call); record the attempt as `Refused` in
  bookkeeping; consult `mutual_attest.gnf_refused_total` metric for
  observability.

**Step 2 — Submit attestation (only if accepting):**

```rust
fn witness_attestation(WitnessAttestationRequest) -> WitnessAttestationResponse

pub struct WitnessAttestationRequest {
    pub attester_tbid: String,
    /// Foretis the attester signed over responder's
    /// most_recent_chronon_record.
    pub foretis: Foretis,
    /// Attester's most recent ChrononRecord (so responder can store
    /// the attester's context for offline re-verification).
    pub attester_tick_record: ChrononRecord,
}

pub struct WitnessAttestationResponse {
    /// Receiver's reported outcome.
    pub outcome: WitnessOutcomeWire,
    /// Receiver's current chronon at the time of acceptance (so the
    /// attester knows the exact storage moment).
    pub stored_at_chronon: Option<u64>,
}

pub enum WitnessOutcomeWire {
    Stored,
    Dormant,            // refused under Strict
    BadAttestation,
    UnknownChronon,     // we don't have this chronon
}
```

**Step 2a — Witness-side beneficial-attestation verification (every
GNF attestation, before storing).**

Take 3 inbound gate runs first as always: signature on the foretis,
TBID match, etc. After Take 3 succeeds and before the Witness API
stores the attestation, the following per-attestation verification
runs at depth determined by `VerificationConfig::level`:

- `Full`: all four steps below run.
- `Minimal`: only step 2 (attester chronon self-consistency) and step 4
  (anti-replay) run.
- `None`: no steps run; storage proceeds immediately after Take 3.

1. **Recoverability check.** Confirm the attester's TBID has a signed
   DHT registration record (g3-d). Consult local `tbid_index` cache;
   on miss, do at most one bounded DHT lookup
   (`gnf_dht_lookup_timeout_ms`). If `gnf_require_dht_record` is true
   (default) AND no signed record can be found, refuse with
   `WitnessOutcomeWire::BadAttestation` and probity
   `witness_rejected` with reason `no_dht_record`.

2. **Attester chronon self-consistency.** The `attester_tick_record`
   in the request passes Take 3 gate as a freshly-parsed
   `Unprocessed<ChrononRecord>`. Specifically: its forward_foretis
   signature verifies against the attester's TBID, and its
   chronon_number is non-zero. (We do NOT chain-fetch the attester's
   prior chronons — that would make every GNF call do extra round-trips.
   The single record's self-consistency is enough to ensure it's not
   garbage.)

3. **Foretis-tick consistency.** The `foretis` in the request must
   reference the attester's TBID AND a chronon_number that is `<=`
   the `attester_tick_record.chronon_number`. (An attester cannot
   stamp from a future chronon.)

4. **Anti-replay window.** If we have ALREADY stored an
   `ExternalAttestation` from this attester for the same local
   chronon, refuse with reason `duplicate`. (One attestation per
   (local_chronon, attester_tbid) pair.)

On all checks passing: proceed to Step 3 (storage). On any failure:
return the appropriate `WitnessOutcomeWire` and emit
`witness_rejected` with the specific reason. Do NOT store anything.

**Step 3 — Both sides update their stores:**

- Initiator A: append to its outbound attestations log
  (`OutboundAttestation { kind: GanzNeueFreundschaft, ... }`).
- Responder B (Witness API): append to its
  `ChrononRecord.external_attestations` for the chronon A stamped
  (`ExternalAttestation { kind: GanzNeueFreundschaft, ... }`).

### 7.4 GNF Asymmetry

A GNF exchange is one-way per call: A attests to B, but B does NOT
reciprocate within the same exchange. This is structurally correct
because:

- GNF is sampling, not pairing — A picked B at random, B has no reason
  to immediately stamp back.
- Mutuality emerges at the network level — B's own GNF scheduler will
  randomly pick someone (possibly A, more likely not) on B's own
  cadence, producing the reverse-direction attestation independently.

This is why GNF has no `still_fast_buddy` flag, no `is_fast_buddy`
query, and no reciprocal stamp in the response. The Witness API is
intentionally one-shot.

### 7.5 Reliability Guarantee (GNF)

GNF is NOT a strict reliability guarantee. The cadence is best-effort
and rate-limited. The operational expectation is that during any
epoch the local node will dispatch `gnf_peers_per_epoch` attestation
attempts (default 8). Significant under-shoot (e.g., 0 attempts due to
empty candidate pool over multiple epochs) is itself a probity signal
that SHOULD be observable via metrics (§10).

---

## 8. Component Responsibilities

| Component | Owns | Reads from | Writes to |
|-----------|------|------------|-----------|
| **Chronomatter** | Local chrononchain advance; `most_recent_chronon_record` source-of-truth. | Local crypto for signing. | Calendar (via TickObserver). |
| **Calendar** | FB set; `epoch_length_chronons` config; outbound attestation log; `is_fast_buddy` API; FB recompute; FB and GNF task enqueue. | Chronomatter (via lookup); `MirrorState` (Group 4b); peer change callback. | Task queue; `ChrononRecord.external_attestations` (inbound, via TimeFamily Witness API); `OutboundAttestation` log. |
| **Communerd** | Caches Chronomatter's "what time" answer (TTL = `wtdyh_cache_ttl_chronons`); serves `wtdyh` RPC; routes FB exchanges through CommunerdetteLine. | Chronomatter (on cache miss); Calendar (for `is_fast_buddy` policy when a remote node calls). | Cache; Communerdette per-TBID state. |
| **TimeFamily server** | Hosts Witness API (`witness_attestation`); routes FB RPCs (`bruderschaft_init`, `fb_stamp_demand`, `is_fast_buddy`) to handlers; enforces dormancy policy on inbound. | Calendar (via `is_fast_buddy`, `Dormancy::strict`); Chronomatter (to validate referenced chronon exists). | Calendar (via Witness API → `ChrononRecord.external_attestations`). |

The trust boundary discipline (Group 1 Take 3) applies to every inbound
record: `Unprocessed<R>` → `verify()` → `CleanAuthenticated<R>` before
storage.

---

## 9. State Tracking

### 9.1 MirrorState Extensions

`MirrorState` (in `calendar::task_queue`) gains:

```rust
pub struct MirrorState {
    // ...existing fields from Group 4b...

    /// FB set; recomputed at end-of-epoch and on PeerChangeCallback.
    pub fast_buddies: parking_lot::RwLock<std::collections::BTreeSet<Tbid>>,

    /// Most recent epoch number this Calendar fully processed.
    pub last_processed_epoch: parking_lot::RwLock<u64>,

    /// Per-FB activity record for the just-completed epoch, used by
    /// the abandonment check (§6.4).
    pub fb_state: parking_lot::RwLock<HashMap<Tbid, FbEpochActivity>>,

    /// GNF per-epoch dedupe set; pruned on epoch rollover.
    pub gnf_attempted_this_epoch: parking_lot::RwLock<HashSet<Tbid>>,
}

pub struct FbEpochActivity {
    /// Number of successful FB-MAEs with this peer in the just-completed epoch.
    pub fb_mae_successes: u32,
    /// Number of failed FB-MAEs.
    pub fb_mae_failures: u32,
    /// Did we successfully dump records to (or receive from) this peer this epoch?
    pub mirror_traffic_ok: bool,
    /// Did we attempt any mirror traffic with this peer this epoch?
    pub mirror_traffic_attempted: bool,
}

impl FbEpochActivity {
    /// §6.4 abandonment rule: any success counts.
    pub fn keep_as_fb(&self) -> bool {
        self.fb_mae_successes > 0 || self.mirror_traffic_ok || self.is_quiet()
    }
    pub fn is_quiet(&self) -> bool {
        self.fb_mae_successes == 0
            && self.fb_mae_failures == 0
            && !self.mirror_traffic_attempted
    }
}
```

### 9.2 Probity Attribute Names

- `"fb_mae_ok"` — FB MAE succeeded (value +1.0).
- `"fb_mae_failed"` — FB MAE failed (value -1.0).
- `"fb_abandoned"` — FB abandoned this peer at end of epoch (value -2.0).
- `"bruderschaft_accepted"` — successful Brüderschaft (value +2.0).
- `"bruderschaft_refused"` — Brüderschaft refused (we asked, peer said no)
  (value -0.5; refusal can be benign — at capacity, etc.).
- `"gnf_attested"` — outbound GNF attestation succeeded (value +0.5).
- `"gnf_refused"` — peer's wtdyh said `accepting_attestations: false`
  (value 0.0 — informational, not punitive).
- `"witnessed"` — Witness API stored an incoming attestation from peer
  (value +0.5).

Per-(peer, attribute) per-epoch rate limit applies (at most one report
per pair per local epoch).

---

## 10. Observability

Metrics (in `foretias-server/src/metrics.rs`):

- `mutual_attest.fb_count` (Gauge) — current FB set size.
- `mutual_attest.bruderschaft_initiated_total` (Counter).
- `mutual_attest.bruderschaft_accepted_total` (Counter).
- `mutual_attest.fb_mae_attempts_total` (Counter, label peer_tbid).
- `mutual_attest.fb_mae_successes_total` (Counter, label peer_tbid).
- `mutual_attest.fb_mae_failures_total` (Counter, label peer_tbid, reason).
- `mutual_attest.fb_abandonments_total` (Counter).
- `mutual_attest.gnf_attempts_total` (Counter).
- `mutual_attest.gnf_successes_total` (Counter).
- `mutual_attest.gnf_refused_total` (Counter) — accepted at network level,
  refused by responder's dormancy.
- `mutual_attest.witness_stored_total` (Counter, label attester_tbid).
- `mutual_attest.witness_rejected_total` (Counter, reason).
- `mutual_attest.epoch_eoe_enqueue_lag_chronons` (Histogram) — chronons
  between epoch boundary and last FB MAE enqueued. Indicates scheduler
  health.
- `mutual_attest.outbound_store_size` (Gauge) — current count of
  OutboundAttestation records.

CLI `inspect-attestations` extended:
- Display `AttestationKind` per record (FB / GNF tags).
- New subcommand `outbound-attestations` lists the outbound store.
- New subcommand `fast-buddies` lists the current FB set.

---

## 11. Failure Modes

| Failure | Detection | Recovery | Probity report |
|---------|-----------|----------|----------------|
| FB peer unreachable | CommunerdetteLine timeout | Record failure in `fb_state`; abandonment check at end-of-epoch (§6.4) | `fb_mae_failed` |
| Brüderschaft signature mismatch | Take 3 inbound gate rejection | Abort protocol; no state change either side | `bruderschaft_refused` |
| Brüderschaft policy gate fails (at capacity, dormant, no mirroring) | Responder local check | Return `accepted: false, reason: <gate>` | `bruderschaft_refused` (informational; not all refusals are adversarial) |
| GNF candidate pool empty | Selection (§7.2) | Skip; log at debug | (none — local condition) |
| Witness API receives attestation for unknown chronon | TimeFamily Witness API | Return `UnknownChronon`; don't store | `witness_rejected` |
| Witness API receives attestation while dormant (Strict mode) | TimeFamily Witness API | Return `Dormant`; don't store | `witness_rejected` |
| Counterparty stamps with wrong key | Take 3 inbound gate | Reject; no storage | `fb_mae_failed` or `gnf_refused` depending on which path |
| Self-attestation attempt | Caller-side check + worker-side check | Refuse with error; log | (none — local bug) |

---

## 12. Bootstrapping & Empty State

**Fresh node, no peers known.** Both cadences degrade gracefully:
- FB: `fast_buddies` is empty → §6.3 enqueues nothing. Once mirroring
  forms (Group 4b) AND Brüderschaft succeeds with at least one peer,
  FB starts producing attestations.
- GNF: candidate pool empty → §7.2 selection returns None. Scheduler
  continues; the first time the DHT/swarm produces a candidate, GNF
  proceeds.

**First epoch ever.** The "last completed epoch" doesn't exist yet. FB
EoE recompute on the first crossing of an epoch boundary skips (no prior
epoch to attest); the assignment to `last_processed_epoch` still
advances, so the second crossing fires normally.

**Calendar restart from scratch.** A node whose Calendar is wiped (no FB
history) cannot prove to existing FBs that it is the "same Calendar."
Existing FBs will see `is_fast_buddy(restarted_node) == false`. The
restarted node MUST re-run Brüderschaft to regain FB status with each
former counterparty.

**Dormancy.** Outbound: zero work (§4 Invariant 12). Inbound: per
`DormantInbound` policy.

---

## 13. Acceptance Criteria

1. **Brüderschaft round-trip test.** Two in-process nodes A and B with
   established mirroring. Trigger `InitiateBruderschaft` on A. After
   the worker completes: both `A.is_fast_buddy(B)` and `B.is_fast_buddy(A)`
   return true; both Calendars have four new `ExternalAttestation`
   records with `kind: FastBuddy`.
2. **FB end-of-epoch test.** Two nodes with established FB-hood;
   advance chronons across an epoch boundary; verify both nodes
   enqueued and completed `DoFastBuddyMAE` for the just-completed
   epoch; verify both Calendars accreted the new attestations.
3. **FB abandonment test.** Two FB nodes; simulate one full epoch where
   B refuses every fb_stamp_demand call from A AND refuses every
   mirror chunk; at end of epoch A's `is_fast_buddy(B)` flips to false;
   `fb_abandoned` probity report emitted.
4. **FB quiet-epoch test.** Two FB nodes; one full epoch with NO
   activity (neither side initiates anything); at end of epoch
   `is_fast_buddy` remains true on both sides (Invariant: absence ≠
   failure).
5. **GNF outbound + Witness inbound test.** Three in-process nodes A,
   B, C; A initiates a GNF attestation that lands on C (selected
   randomly under fixed test seed); C's Calendar accretes an
   `ExternalAttestation { kind: GanzNeueFreundschaft, attester: A }`; A's
   outbound store has the corresponding `OutboundAttestation { kind:
   GanzNeueFreundschaft, peer: C }`.
6. **GNF dedupe-per-epoch test.** Within one epoch, two GNF cadence
   ticks against a candidate pool of size 2 select DIFFERENT peers
   (Invariant 6).
7. **Self-attestation refusal test.** Enqueueing any task with
   `peer_tbid == local TBID` is refused at the worker entry.
   `bruderschaft_init` and `fb_stamp_demand` handlers refuse when
   `initiator_tbid == local TBID`.
8. **Dormancy Strict test.** Local node is dormant under
   `DormantInbound::Strict`. wtdyh returns `accepting_attestations:
   false`. `witness_attestation` calls return `WitnessOutcomeWire::Dormant`
   without storage.
9. **Dormancy Permissive test.** Local node is dormant under
   `DormantInbound::Permissive`. wtdyh returns
   `accepting_attestations: true`. Incoming `witness_attestation`
   stores the attestation in Calendar normally.
10. **Outbound attestation cap test.** Push more than the configured
    cap of OutboundAttestations through the store; FIFO eviction
    keeps the most recent N.
11. **API-distinct test.** Wire-level assertion (mock transport) that
    FB tasks invoke only `bruderschaft_init` / `fb_stamp_demand`
    / `is_fast_buddy`, and GNF tasks invoke only `wtdyh` /
    `witness_attestation`. The two namespaces are non-overlapping.
12. `cargo test --workspace` passes.

---

## 14. Out of Scope

- **Cross-epoch retry queue.** If an FB-MAE fails late in epoch E, the
  next FB-MAE attempt is at the end of E+1. No mid-epoch retry. This is
  intentional — the cadence signal should be legible. Operators can add
  retry policy as a follow-up.
- **Aggregating GNF outcomes into trust scores.** GNF attestations
  land in the Calendar as evidence; no consumer (e.g., FROST scoring)
  is required to weight them in v1.
- **Operator-declared FBs.** FB is derived from Brüderschaft. No CLI
  flag pins a TBID as FB without running the handshake. If demand
  emerges, a follow-up may add a pinned-FB override.
- **GNF guaranteed-distinct counterparties across epochs.** GNF dedupes
  per-epoch but does not track long-term coverage; the same peer may
  be GNF-attested in epoch E and then in epoch E+2 by chance.
- **Multi-party group attestation.** Only bilateral or one-way; group
  primitives are left to FROST.
- **Replication of attestations across mirrors.** A mirror replicating
  A's Calendar receives `ExternalAttestation` records because they're
  embedded in the ChrononRecords; the existing dump path (Group 4b)
  covers it.
- **Wire-format upgrade path.** This spec assumes JSON-RPC for all new
  methods. Migration to a binary codec is a separate concern.

---

## 15. Tunable Defaults (Summary)

| Setting | Default | Rationale |
|---------|---------|-----------|
| `epoch_length_chronons` | 60 | One epoch per 60 chronons. If chronon ≈ 1s, this is one-per-minute, fast enough to be observable in dev/test but coarse enough to keep FB MAE load reasonable. |
| `max_fast_buddies` | 16 | Cap matching typical small-network mirror set; prevents runaway promotion. |
| `gnf_peers_per_epoch` | 8 | Modest spread; well within the worker pool throughput. |
| `wtdyh_cache_ttl_chronons` | 1 | Refresh on every tick advance — cheap and keeps responses fresh. |
| `dormant_inbound` | `Strict` | Conservative default; contested-TBID nodes don't grow the network's view of them while dormant. |
| Outbound attestation store cap | 65536 | Generous bound; FIFO eviction. |
| Probity report rate limit | 1 per (peer, attribute) per epoch | Prevents flooding. |

---

## 16. Notes on Existing Code

- **`EpochScheduler` (in `core-engine/src/epoch/scheduler.rs`)** is the
  FROST committee scheduler, wall-clock-based, used by probity scoring.
  Mutual Attestation does NOT use it. The `epoch_length_chronons` config
  is read by Calendar only.
- **`mutual_attest_observer: Option<Arc<dyn MutualAttestObserver>>`** in
  Chronomatter is the v0.2 metrics hook for `stamp_peer`. It remains
  for backward compat but is NOT load-bearing for FB/GNF cadence —
  FB and GNF go through Calendar's task queue, not through Chronomatter.
- **`every_n_chronons` in `MutualAttestConfig`** is the v0.2 cadence
  config. It is superseded by `AttestationConfig::epoch_length_chronons`
  and `fb_enabled`. The v0.2 field is preserved for one release with a
  deprecation log; remove in a follow-up.

---

## 17. Open Questions (Pending Human Review)

1. **Brüderschaft retry on failure.** Should a Calendar that initiated
   Brüderschaft and got a refusal (e.g., responder at capacity) retry
   automatically in a later epoch, or wait for operator/mirroring-event
   trigger? Recommend: no auto-retry, surface the refusal in probity, let
   the natural mirroring-event trigger try again.
2. **`is_fast_buddy` honesty under dormancy.** Should a dormant node
   answer `is_fast_buddy(T)` truthfully (yes if T is in the FB set even
   though we won't attest with them right now), or always return false
   during dormancy (forcing all counterparties to drop us)? Recommend:
   answer truthfully. Dormancy is temporary; the relationship persists.
3. **GNF rate when candidate pool > target.** If the local node knows
   many more peers than `gnf_peers_per_epoch`, the same peers may go
   for many epochs without GNF coverage. Should we layer a "least-recently-
   GNF-attested" preference? Recommend: no, keep uniform sampling — over
   enough epochs the law of large numbers covers everyone. If operationally
   we observe persistent gaps, revisit in v2.
4. **`OutboundAttestation` durability.** Is the in-memory cap-65536 FIFO
   store sufficient, or should this persist to disk for forensic audits
   across restarts? Recommend: in-memory for v1; persist later if
   demand emerges.
5. **Witness API authentication.** The current spec lets any peer call
   `witness_attestation`. Should this require Noise_XX handshake (likely
   yes — it already does via the existing TCP transport) AND/OR check
   that the attester's TBID exists in the local Communerd's
   `tbid_index`? Recommend: TCP/Noise handshake is enough for v1; future
   audit may add a TBID-known filter.
