# Spec: Centralize P2P Record Key Definitions and Calendar Supplier Discovery

- [x] backburnered

**Scope:** Major (two phases — key cleanup, then calendar supplier feature)
**Status:** Draft

---

## Background and Problem

P2P DHT record keys and provider keys are defined inline, scattered across
multiple source files, and contain at least one verified inconsistency that
causes silent key-space collisions at runtime. There is no single authoritative
source listing every key in use, its format, its wire value type, and its
lifecycle.

Additionally, the current cross-node verify path has a single point of failure:
it always fetches the calendar from the one node that owns a given TBID. If that
node is offline or slow, verification fails. A second mechanism — discovering
*any* node that holds calendar data for a given TBID — would make verification
more resilient and distribute the load.

---

## Part 1 — Known Issues in the Current Implementation

### Issue 1: Namespace placement inconsistency (bug)

Value records (GET/PUT) correctly use `/foretias/{namespace}/...`:
```
/foretias/{ns}/peers/v1
/foretias/{ns}/tbid/{tbid_hex}/v1
```

Provider records (start_providing / get_providers) use two different conflicting
patterns in the same codebase:

| Location | Key produced |
|----------|--------------|
| `communerd/mod.rs::publish_attest_willing()` | `{ns}/foretias/attest-willing/v1` |
| `communerd/capabilities.rs::provider_key()` | `{ns}foretias/attest-willing/v1` (no `/` separator) |

These produce different bytes in the DHT keyspace. A node registering under one
pattern is invisible to a node querying under the other.

### Issue 2: Dead capability provider path

`SwarmCommand::ProvideForCapability` and `SwarmCommand::GetProvidersForCapability`
are handled in the swarm loop but never sent from `Communerd`. The event
`DhtPeerDiscoveredByCapability` is defined but never emitted. `DhtPeerSource`
with its capability index is fully implemented and tested but never populated
from live swarm events. This entire path is unreachable at runtime.

### Issue 3: Inline key construction

Record keys are constructed with `format!()` strings spread across three source
files. A typo, case change, or future rename produces a silently different key
with no compile-time or test-time warning.

### Issue 4: `GetProviders` result discards addresses

The swarm's `GetProviders` handler emits `DhtPeerDiscovered` with an empty
address list:
```rust
// communerd/p2p/swarm.rs
let addresses = vec![];   // always empty — provider addresses not forwarded
let _ = tx.send(NetworkEvent::DhtPeerDiscovered { peer_id, addresses });
```
Any feature that relies on `get_providers` must have a strategy for resolving
peer IDs to usable addresses independently.

---

## Part 2 — Canonical DHT Key Inventory (Target State)

All keys follow the same root: `/foretias/{namespace}/`, then a type segment,
then a version suffix. The namespace always comes second, immediately after
`/foretias/`, so that the protocol name is always the root of any key tree.

### Value Records (Kademlia PUT / GET)

These are standard Kademlia records: one authoritative writer, readers retrieve
by exact key. Last-write-wins semantics.

| Name | Key pattern | Value type | Writer | Reader |
|------|-------------|------------|--------|--------|
| `PEERS_KEY` | `/foretias/{ns}/peers/v1` | `PeerRegistrationRecord` (JSON) | Every node on startup; refreshed every 60 s | Every node on bootstrap to seed the peer pool |
| `TBID_KEY` | `/foretias/{ns}/tbid/{tbid_hex}/v1` | `PeerRegistrationRecord` (JSON) | The node that owns this TBID | Any node doing TBID lookup or cross-node verify |

### Provider Records (Kademlia start_providing / get_providers)

These are Kademlia provider advertisements: many nodes may independently
register as providers of the same key. A query returns a set of peer IDs, not a
value. Resolving a peer ID to a usable address requires a secondary step (see
address resolution below).

| Name | Key pattern | Meaning | Who provides | Who queries |
|------|-------------|---------|--------------|-------------|
| `CAP_ATTEST_KEY` | `/foretias/{ns}/cap/attest-willing/v1` | Node will cross-attest | Nodes accepting attestation work | Nodes seeking attestation partners |
| `CAP_MIRROR_KEY` | `/foretias/{ns}/cap/mirror-willing/v1` | Node will mirror calendars | Nodes mirroring calendars | Nodes seeking mirror partners |
| `CAP_VERIFY_KEY` | `/foretias/{ns}/cap/verifier-willing/v1` | Node offers timestamp verification | Verifier nodes | Clients seeking a verifier |
| `CAL_FOR_TBID_KEY` | `/foretias/{ns}/cal/{tbid_hex}/v1` | Node holds calendar data for this TBID | Owner node + any mirror/verifier holding ticks for this TBID | Any node seeking a calendar supplier for verification |

### Provider Address Resolution

When `get_providers` returns a set of peer IDs, a json_rpc address is required
to actually issue an RPC. Resolution proceeds in priority order:

1. **Peer pool hit.** Check `DhtPeerSource` / `PeerPool` for a cached
   `PeerAddr` matching the peer ID. This is the common case for nodes that have
   been active and registered at `/foretias/{ns}/peers/v1`.

2. **TBID value record.** If the provider is the owner of the TBID being
   queried (i.e., querying `CAL_FOR_TBID_KEY` and the provider's peer ID matches
   the peer_id field in the TBID value record), the json_rpc address is already
   in that record.

3. **Skip.** If neither source yields an address, that provider is not currently
   usable and is removed from the candidate set for this request. No blocking
   dial is attempted.

Providers that PUT to `/foretias/{ns}/peers/v1` on startup (all current nodes
do this) will naturally be resolvable via path 1 for any peer that bootstrapped
against the same network.

---

## Part 3 — Proposed Work

### Phase A — Key Centralization and Bug Fixes

#### Stage A1 — Create `dht_keys.rs`

Create `communerd/p2p/dht_keys.rs` as the single authoritative source for every
DHT key pattern.

```rust
/// Single source of truth for all Foretias DHT key patterns.
/// All keys follow: /foretias/{namespace}/<type>/.../<version>
pub struct DhtKeys;

impl DhtKeys {
    // ── Value records ─────────────────────────────────────────────────────

    /// Shared peer registration pool. Every node writes here; every node
    /// reads here on bootstrap.
    pub fn peers(ns: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/peers/v1", ns))
    }

    /// TBID-indexed peer registration. Maps a specific TBID to its owning
    /// node's address and capabilities.
    pub fn tbid(ns: &str, tbid_hex: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/tbid/{}/v1", ns, tbid_hex))
    }

    // ── Capability provider records ───────────────────────────────────────

    /// Provider key for nodes willing to cross-attest.
    pub fn cap_attest(ns: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/cap/attest-willing/v1", ns))
    }

    /// Provider key for nodes willing to mirror calendars.
    pub fn cap_mirror(ns: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/cap/mirror-willing/v1", ns))
    }

    /// Provider key for nodes offering verification as a service.
    pub fn cap_verifier(ns: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/cap/verifier-willing/v1", ns))
    }

    // ── Calendar supplier provider records ────────────────────────────────

    /// Provider key for nodes that hold calendar data for a specific TBID.
    /// Multiple independent nodes (owner, mirrors, verifiers) may all
    /// register as providers for the same TBID's calendar.
    pub fn cal_for_tbid(ns: &str, tbid_hex: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("/foretias/{}/cal/{}/v1", ns, tbid_hex))
    }
}
```

**Tests in `dht_keys.rs`:**
- Every key is distinct for distinct inputs.
- Every key contains the namespace as an isolated path segment (not a prefix).
- Same inputs always produce the same bytes.
- No key is a prefix of any other key.
- `tbid` and `cal_for_tbid` keys for the same TBID are distinct (they must not
  collide even though both are keyed by `tbid_hex`).

#### Stage A2 — Replace all inline key formats

Every `kad::RecordKey::new(&format!(...))` in `communerd/mod.rs`,
`communerd/capabilities.rs`, and `communerd/p2p/swarm.rs` is replaced with the
corresponding `DhtKeys::*` call. The old `PeerCapability::provider_key()` and
`PeerCapability::provider_key_suffix()` methods are removed.

#### Stage A3 — Fix the namespace bug

`publish_attest_willing()` in `communerd/mod.rs` currently constructs
`{ns}/foretias/attest-willing/v1`. It is replaced with `DhtKeys::cap_attest(ns)`
which produces `/foretias/{ns}/cap/attest-willing/v1`. After this change all
provider key construction goes through `DhtKeys` and produces consistent bytes.

#### Stage A4 — Resolve the dead capability path

**Decision required from human before implementation begins.**

**Option A — Wire it.** Extend `Communerd` startup to send
`SwarmCommand::ProvideForCapability` for each capability the node supports.
Handle `GetProvidersForCapability` results in the event loop and populate
`DhtPeerSource` with discovered peers. Emit `DhtPeerDiscoveredByCapability`
events. This makes `DhtPeerSource` live for the first time.

**Option B — Remove it.** Delete `SwarmCommand::ProvideForCapability`,
`SwarmCommand::GetProvidersForCapability`, `NetworkEvent::DhtPeerDiscoveredByCapability`,
and `DhtPeerSource`. The code is not yet needed and its presence creates false
confidence that capability discovery is operational. Re-introduce when actually
built.

Both options are valid; the spec documents them and defers the decision.

---

### Phase B — Calendar Supplier Discovery

This phase implements `CAL_FOR_TBID_KEY`: a mechanism by which any node
holding calendar data for a given TBID can advertise that fact, and any node
seeking to verify a Foretis can discover multiple potential suppliers rather than
depending on the TBID owner alone.

#### Stage B1 — Advertisement (providing)

Nodes call `start_providing(DhtKeys::cal_for_tbid(ns, tbid_hex))` for every
TBID whose calendar ticks they hold:

- **Owner node:** on startup, after DHT bootstrap completes, for its own TBID.
- **Mirror node:** when a new mirror relationship is accepted, for the mirrored
  TBID. Also on startup, for every TBID currently mirrored.
- **Dormant node:** on startup, for every TBID present in its loaded calendar
  store.

Kademlia provider records expire; nodes must re-announce periodically. The
re-announcement interval follows the same 60-second cycle as the existing
`PeerRegistrationRecord` refresh, or the Kademlia default TTL, whichever is
shorter.

**New `SwarmCommand` variant (or reuse `Provide`):**

```rust
ProvideCalendarForTbid { tbid_hex: String, namespace: String },
```

The swarm loop handles this by calling:
```rust
let key = DhtKeys::cal_for_tbid(&namespace, &tbid_hex);
let _ = swarm.behaviour_mut().kad.start_providing(key);
```

#### Stage B2 — Discovery (querying)

A new `Communerd` method returns a random sample of peers known to hold
calendar data for a given TBID:

```rust
/// Find peers that advertise holding calendar data for `tbid_hex`.
/// Returns up to `max` usable peers, selected at random from the provider set.
/// A peer is "usable" if its json_rpc address can be resolved from the
/// peer pool or the TBID value record.
pub async fn find_calendar_suppliers(
    &self,
    tbid_hex: &str,
    namespace: &str,
    max: usize,
) -> Vec<PeerAddr>
```

Internally this method:

1. Sends `SwarmCommand::GetProviders { key: DhtKeys::cal_for_tbid(ns, tbid_hex) }`.
2. Waits for `NetworkEvent::DhtPeerDiscovered` events carrying provider peer IDs
   (with a configurable timeout, default 5 s, matching the existing TBID lookup
   timeout).
3. For each discovered peer ID, attempts address resolution using the priority
   order defined in Part 2.
4. Collects all resolved `PeerAddr` values, shuffles them, and returns up to
   `max` of them.

If no providers are found or none are resolvable, the method returns an empty
vec. Callers must handle this gracefully.

#### Stage B3 — Integrate with cross-node verify

`cross_node_verify` in `server/handlers.rs` currently:

1. Looks up the TBID owner via `lookup_tbid` (TBID value record).
2. Fetches the calendar slice from the owner.
3. Verifies locally.

With this stage, the fallback chain becomes:

1. Try the TBID owner (existing path). If successful, done.
2. If the owner is unreachable (connection error or timeout), call
   `find_calendar_suppliers(tbid_hex, ns, 3)`.
3. Try each returned supplier in order until one succeeds.
4. If all fail, return an error indicating the calendar is currently unavailable.

The verification logic itself (signature check against the fetched public key)
is unchanged.

```
cross_node_verify flow:

  lookup_tbid(tbid_hex)
      → owner PeerAddr
          → get_calendar_slice(owner, tick, 1)  ──── success → verify locally
                                                 ↘ error
                                                  find_calendar_suppliers(tbid_hex, max=3)
                                                      → [supplier_1, supplier_2, ...]
                                                          → get_calendar_slice(supplier_1, tick, 1)  ── success → verify locally
                                                          → get_calendar_slice(supplier_2, tick, 1)  ── success → verify locally
                                                          → all failed → return error
```

#### Stage B4 — New `NetworkEvent` variant

To distinguish calendar-supplier discovery from general peer discovery, add a
dedicated event:

```rust
/// Provider query for a specific TBID's calendar returned results.
CalendarSuppliersFound {
    tbid_hex: String,
    providers: Vec<PeerId>,
},
```

The swarm loop emits this event when a `GetProviders` result is received for a
key matching the `cal_for_tbid` pattern. The swarm must track in-flight queries
by query ID and key to know which event to emit; this mirrors the existing
`pending_get_record` pattern already in `swarm_loop`.

---

## Part 4 — Acceptance Criteria

### Phase A

- [ ] `dht_keys.rs` exists and is the only place key patterns are defined.
- [ ] All key patterns follow `/foretias/{ns}/...` consistently.
- [ ] `DhtKeys::tbid` and `DhtKeys::cal_for_tbid` for the same `tbid_hex` produce
      distinct keys.
- [ ] No inline `format!()` key constructions remain in `communerd/mod.rs`,
      `communerd/capabilities.rs`, or `communerd/p2p/swarm.rs`.
- [ ] Unit tests assert distinctness, determinism, and namespace isolation.
- [ ] Namespace bug in `publish_attest_willing` is resolved.
- [ ] Capability provider path is either fully wired or cleanly removed.
- [ ] All existing tests pass.

### Phase B

- [ ] Nodes call `start_providing(DhtKeys::cal_for_tbid(...))` on startup for
      their own TBID.
- [ ] `Communerd::find_calendar_suppliers` returns a randomly ordered list of
      usable `PeerAddr` values.
- [ ] `cross_node_verify` falls back to supplier discovery when the TBID owner
      is unreachable.
- [ ] `CalendarSuppliersFound` event is emitted correctly by the swarm loop.
- [ ] Integration test: three nodes, node C's TBID owner is stopped, node A
      still verifies a Foretis from C using node B as a supplier.
- [ ] All existing tests pass.

---

## Part 5 — Files Affected

### Phase A

| File | Change |
|------|--------|
| `communerd/p2p/dht_keys.rs` | New — canonical key definitions |
| `communerd/p2p/mod.rs` | Add `pub mod dht_keys` |
| `communerd/capabilities.rs` | Remove `provider_key()`, `provider_key_suffix()`; retain `PeerCapability` enum if kept for capability path |
| `communerd/mod.rs` | Replace inline key formats; fix `publish_attest_willing` |
| `communerd/p2p/swarm.rs` | Replace inline key formats |

### Phase B

| File | Change |
|------|--------|
| `communerd/p2p/events.rs` | Add `CalendarSuppliersFound` variant |
| `communerd/p2p/swarm.rs` | Track provider query IDs; emit `CalendarSuppliersFound`; add `ProvideCalendarForTbid` handler |
| `communerd/p2p/swarm.rs` (`SwarmCommand`) | Add `ProvideCalendarForTbid` variant |
| `communerd/mod.rs` | Add `find_calendar_suppliers`; update `start_p2p` to advertise own TBID; handle `CalendarSuppliersFound` events |
| `server/handlers.rs` (`cross_node_verify`) | Add supplier fallback after owner failure |
| `communerd/dht_peer_source.rs` | Populate from `CalendarSuppliersFound` events (if capability path is wired in Phase A Option A) |
