# IMPROVE_BUILDERS_AND_BIG_FNS_SPEC.md

**Spec: Improve Builders and Big Functions**
**Paired Plan: IMPROVE_BUILDERS_AND_BIG_FNS_PLAN.md**
**Status: PROPOSED**
**Date: 2026-06-05**
**Experiment: `experiments/bon-serde-poc/` (19/19 tests pass)**

---

## Overview

Introduce `bon` builders for domain types and convert large multi-parameter functions to named-parameter function builders. No serde or trust boundary changes.

### Motivation

1. **Large constructors are error-prone.** `ChrononRecord::new()` takes 7 positional params. Chronomatter constructs via struct literals with 10 fields. Builders with named methods eliminate positional confusion.

2. **Large function signatures are hard to read.** `gossip_event_loop` takes 11 params. `cmd_serve` takes 14. `#[builder] fn` gives named, optional parameters.

### What Changes

| Component | Before | After |
|-----------|--------|-------|
| `ChrononRecord` construction (Chronomatter) | Struct literal with 10 fields | `ChrononRecord::builder().chronon_number(1)...build()` |
| `ChrononRecord::new()` | 7 positional params | `ChrononRecord::builder()...build()` |
| `Foretis::new()` | 6 positional params | `Foretis::builder()...build()` |
| `gossip_event_loop` | 11 params | `#[builder] fn` with named params |
| `refresh_self_registration` | 9 params | `#[builder] fn` with named params |
| `cmd_serve` | 14 params | `#[builder] fn` with named params |
| `TimeFamilyCliConfig` | 15 fields, manual construction | `#[derive(Builder)]` |
| `VerifyConfig` | 8 fields, manual construction | `#[derive(Builder)]` |

### What Does NOT Change

- `ChrononRecord` struct fields — all stay required (no `Option<T>`)
- `#[derive(Deserialize)]` on domain types — stays strict, missing field = serde error
- `UnverifiedSignatureEnvelope<T>` — stays as-is
- `CleanAuthenticated<T>` — stays as-is
- `Externalized<T>` — stays as-is
- Trust boundary flow — unchanged
- Wire format — unchanged
- `bon = "3.9"` added to `core-engine` and `foretias-server`

---

## Detailed Design

### 1. Domain Type Builders

Each domain type gets `#[derive(Builder)]` with a fallible `build()`:

```rust
use bon::Builder;

#[derive(Debug, Clone, Serialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ChrononRecord {
    #[serde(rename = "tick_number")]
    pub chronon_number: u64,
    pub public_key: FTByteVector,
    #[builder(default = "Ed25519".to_string())]
    pub signature_algorithm: String,
    pub forward_foretis: FTByteVector,
    pub backward_foretis: FTByteVector,
    pub aa_nonce: FTByteArray<16>,
    #[builder(default)]
    pub chronon_stamp_count: u64,
    #[builder(default)]
    pub external_attestations: Vec<ExternalAttestation>,
    #[builder(default = 1)]
    pub tb_version: u32,
    #[builder(default)]
    pub tbid: Tbid,
}

impl<S: chronon_record_builder::IsComplete> ChrononRecordBuilder<S> {
    /// Fallible build — validates invariants.
    pub fn build(self) -> Result<ChrononRecord, NodeError> {
        let record = self.build_internal();
        if record.chronon_number == 0 {
            return Err(NodeError::InvalidInput("chronon_number must be > 0".into()));
        }
        if record.public_key.is_empty() {
            return Err(NodeError::InvalidInput("public_key must not be empty".into()));
        }
        Ok(record)
    }
}
```

**Compile-time enforcement:** Non-`Option` fields are required by default in bon. Omitting a required field produces a compile error.

**Chronomatter usage (before/after):**

```rust
// Before: struct literal — 10 fields, positional
Ok(ChrononRecord {
    chronon_number: self.chronon_number,
    public_key: self.current_public_key.clone(),
    signature_algorithm: "Ed25519".to_string(),
    forward_foretis: vec![].into(),
    backward_foretis: prev_foretis_bytes.into(),
    aa_nonce: nonce,
    chronon_stamp_count: 0,
    external_attestations: Vec::new(),
    tb_version: 1,
    tbid: self.tbid.clone(),
})

// After: named builder — each field is named, optional fields defaulted
Ok(ChrononRecord::builder()
    .chronon_number(self.chronon_number)
    .public_key(self.current_public_key.clone())
    .forward_foretis(vec![].into())
    .backward_foretis(prev_foretis_bytes.into())
    .aa_nonce(nonce)
    .tbid(self.tbid.clone())
    .build()?)
```

### 2. Function Builders

Large functions become `#[builder] fn`:

```rust
// Before: 11 positional params
async fn gossip_event_loop(config: GossipLoopConfig) { ... }

// After: named builder, optional fields can be omitted
#[bon::builder]
async fn gossip_event_loop(
    events: UnboundedReceiver<NetworkEvent>,
    cmd_tx: Option<UnboundedSender<SwarmCommand>>,
    probity_store: Arc<ProbityStore>,
    crypto: Arc<dyn CryptoServer>,
    clock: Arc<dyn Clock>,
    detector: Option<Arc<CollisionDetector>>,
    peer_pool: PeerPool,
    tbid_index: Arc<RwLock<HashMap<String, PeerRegistrationRecord>>>,
    pending_lookups: PendingLookupMap,
    pending_family_lookups: PendingFamilyLookupMap,
    communerd: Communerd,
) { ... }

// Call site:
Self::gossip_event_loop()
    .events(events)
    .cmd_tx(cmd_tx)
    .probity_store(probity_store)
    .crypto(crypto)
    .clock(clock)
    .detector(Some(det))
    .peer_pool(peer_pool)
    .tbid_index(tbid_index)
    .pending_lookups(pending_lookups)
    .pending_family_lookups(pending_family_lookups)
    .communerd(communerd_ref)
    .call()
    .await;
```

**Candidates (all constructed atomically at the call site):**

| Function | Params | File |
|----------|--------|------|
| `gossip_event_loop` | 11 | `communerd/mod.rs` |
| `refresh_self_registration` | 9 | `communerd/mod.rs` |
| `cmd_serve` | 14 | `main.rs` |

### 3. Struct Builders for Config Types

Config types that are built gradually get `#[derive(Builder)]`:

| Struct | Fields | File | When built |
|--------|--------|------|------------|
| `TimeFamilyCliConfig` | 15 | `config/time_family.rs` | From config file + CLI overrides |
| `VerifyConfig` | 8 | `main.rs` | From multiple input sources |

---

## Scope

### In Scope

- `ChrononRecord` — bon builder with fallible build
- `Foretis` — bon builder with fallible build
- `ExternalAttestation` — bon builder with fallible build
- `gossip_event_loop` — bon function builder
- `refresh_self_registration` — bon function builder
- `cmd_serve` — bon function builder
- `TimeFamilyCliConfig` — bon struct builder
- `VerifyConfig` — bon struct builder
- Chronomatter: replace struct literals with builder calls
- `bon = "3.9"` dependency

### Out of Scope

- Serde changes (no permissive deserialization, no passthrough)
- Trust boundary changes (UnverifiedSignatureEnvelope, CleanAuthenticated stay as-is)
- Wire format changes
- `ChrononRecord::new()` — kept alongside builder for backward compat (or removed)
