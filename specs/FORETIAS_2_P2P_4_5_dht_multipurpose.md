# Foretias — P2P Sub-Spec 4.5: Multi-Purpose DHT Peer Discovery (v0.4.5)

- [x] backburnered

**Milestone tag:** `v0.4.5-dht-multipurpose`
**Prereq:** `v0.4-dht-discovery` must be tagged.
**Next:** `FORETIAS_2_P2P_5_hardening.md` (v0.5 — threat model, observability).

**Target:** AI Coding Specialist. `(@human ...)` blocks are for human readers.

---

## 1. GOAL

Extend the single-purpose DHT (v0.4 — attest-willing discovery only) to support **multi-purpose peer discovery**:

| Purpose | DHT Provider Key | Description |
|---------|-----------------|-------------|
| Mutual attestation | `/{ns}/foretias/attest-willing/v1` | Already exists in v0.4 |
| Calendar mirroring | `/{ns}/foretias/mirror-willing/v1` | NEW — nodes willing to mirror calendar ticks |
| Verifier services | `/{ns}/foretias/verifier-willing/v1` | NEW — nodes offering independent verification |

A single node may register for **any combination** of capabilities. Discovery queries can target a specific capability or return all peers regardless of capability.

(@human — the rationale for capability-specific provider keys rather than a monolithic record-with-filter is that Kademlia `get_providers()` is inherently key-based. Filtering after retrieval wastes network bandwidth. Capability-keyed provider records let the DHT do the routing for us.)

---

## 2. WHAT v0.4 PROVIDES THAT THIS SPEC EXTENDS

| Symbol | Extension |
|---|---|
| `PeerRegistrationRecord` | Gains `capabilities: Vec<PeerCapability>` field |
| `PeerCapability` enum | NEW — `AttestWilling`, `MirrorWilling`, `VerifierWilling` |
| `DhtPeerSource` | Gains `list_by_capability(cap)` and `upsert` now filters by capability |
| `PeerRegistrationRecord` PUT | Now published under multiple DHT keys — one per capability |
| `Communerd::register_and_discover` | Accepts `capabilities: Vec<PeerCapability>` parameter |
| `SwarmCommand` | Gains `ProvideForCapability` variant |
| `NetworkEvent` | Gains `DhtPeerDiscoveredByCapability { capability, peer_id, addresses }` |

---

## 3. DATA STRUCTURES

### 3.1 `PeerCapability` enum

```rust
/// Capabilities a node can advertise in the DHT for peer discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerCapability {
    /// Willing to cross-attest calendar ticks with other nodes.
    AttestWilling,
    /// Willing to mirror calendar ticks (one-way or mutual replication).
    MirrorWilling,
    /// Offering independent timestamp verification as a service.
    VerifierWilling,
}

impl PeerCapability {
    /// Return the DHT provider key suffix for this capability.
    pub fn provider_key_suffix(&self) -> &'static str {
        match self {
            Self::AttestWilling => "/foretias/attest-willing/v1",
            Self::MirrorWilling => "/foretias/mirror-willing/v1",
            Self::VerifierWilling => "/foretias/verifier-willing/v1",
        }
    }

    /// Return the full DHT provider key for a namespace + capability.
    pub fn provider_key(&self, namespace: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("{namespace}{suffix}", suffix = self.provider_key_suffix()))
    }
}
```

(@human — extending this enum is the ONLY change needed to add a new discovery purpose in the future. No DHT key scheme, no swarm logic, no Communerd dispatch — just a new enum variant + suffix.)

### 3.2 Extended `PeerRegistrationRecord`

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerRegistrationRecord {
    pub peer_id: String,
    pub tbid: String,
    pub multiaddr: String,
    pub json_rpc: String,
    pub chronon_ns: u64,
    pub registered_at_ns: v0.4,  // unchanged
    /// Capabilities this node advertises for DHT-based peer discovery.
    pub capabilities: Vec<PeerCapability>,
}
```

**Backward compatibility:** Existing v0.4 records without a `capabilities` field are treated as `{ AttestWilling }` only. `serde(default)` ensures deserialization of legacy records never fails.

### 3.3 `DhtPeerSource` — now capability-aware

```rust
pub struct DhtPeerSource {
    /// All known peers, indexed by PeerId.
    peers: Arc<RwLock<HashMap<PeerId, PeerAddr>>>,
    /// Capability → set of PeerIds that advertised this capability.
    capability_index: Arc<RwLock<HashMap<PeerCapability, HashSet<PeerId>>>>,
}
```

New methods:
- `upsert_with_capabilities(peer_id, addr, caps)` — inserts peer AND updates capability index
- `list_by_capability(cap)` — returns peers matching a specific capability
- `list()` — unchanged, returns all peers (backward compatible)
- `remove(peer_id)` — removes from both main map AND all capability indices

---

## 4. ARCHITECTURE

### 4.1 Registration Flow (per capability)

On `register_and_discover`, for **each capability** the node advertises:

```rust
for cap in &capabilities {
    let key = cap.provider_key(&namespace);
    swarm.behaviour_mut().kad.start_providing(key)?;
}
```

The `PeerRegistrationRecord` PUT to `/foretias/{ns}/peers/v1` now includes the full `capabilities` vector so any peer that retrieves the record knows what this node can do.

### 4.2 Discovery Flow (per capability)

To find peers for a specific purpose:

```rust
// Find mirror-willing peers
let key = PeerCapability::MirrorWilling.provider_key(&namespace);
swarm.behaviour_mut().kad.get_providers(key);
// Results arrive via DhtPeerDiscoveredByCapability { capability: MirrorWilling, ... }
```

### 4.3 `NetworkEvent` additions

```rust
pub enum NetworkEvent {
    // ... existing variants ...
    /// A peer was discovered via a capability-specific provider query.
    DhtPeerDiscoveredByCapability {
        capability: PeerCapability,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    /// A peer registration record was retrieved from the DHT (with capabilities).
    PeerRegistrationRetrieved {
        record: PeerRegistrationRecord,
    },
}
```

### 4.4 `SwarmCommand` additions

```rust
pub enum SwarmCommand {
    // ... existing variants ...
    /// Start providing a capability-specific provider key.
    ProvideForCapability {
        capability: PeerCapability,
        namespace: String,
    },
    /// Query providers for a specific capability.
    GetProvidersForCapability {
        capability: PeerCapability,
        namespace: String,
    },
}
```

### 4.5 `Communerd` dispatch

The `gossip_event_loop` now handles the new events:

```rust
NetworkEvent::DhtPeerDiscoveredByCapability { capability, peer_id, addresses } => {
    peer_pool.add_peer_with_capability(peer_addr, capability).await;
}
NetworkEvent::PeerRegistrationRetrieved { record } => {
    // Upsert into peer pool with ALL capabilities from the record
    for cap in &record.capabilities {
        dht_source.upsert_with_capability(record.peer_id, peer_addr.clone(), *cap);
    }
}
```

---

## 5. CONFIGURATION

```json
{
  "network": {
    "p2p_listen":             "/ip4/0.0.0.0/tcp/4101",
    "dht_namespace":          "mainnet",
    "dht_bootstrap":          ["/ip4/bootstrap.foretias.example/tcp/4101/p2p/12D3KooW..."],
    "dht_reprovide_interval_secs": 1200,
    "dht_capabilities":       ["attest_willing", "mirror_willing"]
  }
}
```

CLI: `--dht-capability <cap>` (repeatable). Default: `["attest_willing"]` for backward compatibility.

---

## 6. TEST PLAN

- `capability_provider_key_deterministic` — same namespace + capability always produces same key.
- `capability_provider_key_distinct` — attest_willing ≠ mirror_willing ≠ verifier_willing keys.
- `peer_registration_record_roundtrip` — serialize/deserialize with capabilities.
- `peer_registration_record_backward_compat` — legacy record without capabilities deserializes as `{ attest_willing }`.
- `dht_peer_source_upsert_with_capabilities` — peer appears in list() AND list_by_capability(cap).
- `dht_peer_source_list_by_capability_filters` — only matching peers returned.
- `dht_peer_source_remove_clears_capability_index` — removed peer gone from all indices.
- `register_and_discover_multiple_capabilities` — node registers for attest + mirror, both provider keys published.
- Integration: `two_nodes_discover_by_capability` — A registers as mirror_willing, B queries mirror_willing, discovers A within 15s.
- All v0.1–v0.4 tests pass.

---

## 7. NON-GOALS FOR v0.4.5

- ❌ Capability negotiation protocol (nodes accept or reject based on capability) — future work.
- ❌ Capability-based transport selection (e.g., mirror traffic always uses libp2p streams) — v0.5.
- ❌ Probity scoring weighted by capability — v0.6.
- ❌ Dynamic capability addition/removal at runtime — future work.

---

## 8. MILESTONE CHECKLIST

```
[ ] v0.4.5.1  Add PeerCapability enum to communerd module.
[ ] v0.4.5.2  Extend PeerRegistrationRecord with capabilities field (serde default).
[ ] v0.4.5.3  Extend DhtPeerSource with capability_index, list_by_capability.
[ ] v0.4.5.4  Add NetworkEvent variants (DhtPeerDiscoveredByCapability, PeerRegistrationRetrieved).
[ ] v0.4.5.5  Add SwarmCommand variants (ProvideForCapability, GetProvidersForCapability).
[ ] v0.4.5.6  Wire swarm.rs to handle new SwarmCommands.
[ ] v0.4.5.7  Wire Communerd gossip_event_loop to handle new NetworkEvents.
[ ] v0.4.5.8  Update register_and_discover to accept capabilities param.
[ ] v0.4.5.9  Unit tests for capability enum, record, DhtPeerSource.
[ ] v0.4.5.10 Integration test: two nodes discover by capability.
[ ] v0.4.5.11 All prior tests pass (229+ Rust tests).
[ ] v0.4.5.12 TAG: v0.4.5-dht-multipurpose.
```

---
# END OF SUB-SPEC 4.5 (v0.4.5)
