# Spec: Calendar Proof-of-Storage for Chrononchain Ranges

**Scope:** Feature (cross-cuts v0.4 probity gossip and v0.7 calendar persistence)
**Status:** DEPRECATED — superseded by `COMBINED_GROUP4_SPEC.md` §4 (2026-05-22).
           Design content preserved here for reference. Do not update this file.
           **Further note (2026-05-26):** the successor §4 (Stream 4c) is itself now DEFERRED until Communerdette (Group 7) reaches feature completion. Proof-of-storage implementation will be re-planned alongside the rest of the mirror work after the post-Communerdette spec rewrite.
**Companion:** `CALENDAR_PROOF_OF_STORAGE_PLAN.md`

---

## 1. Problem Statement

A Calendar (Mirror node, or any node storing Chrononchain data for a TBID) can be
asked to prove that it actually has a specific chronon number range persisted for a
given TBID. Without such a mechanism, a Calendar can:

1. **Lie about coverage** — claim to serve chronons 1–1000 but have evicted chronons
   200–400 under its LRU policy, returning nothing on demand.
2. **Gaming probity** — a Mirror node could maintain a high "correctness" probity
   score by never admitting gaps, then silently failing when challenged.
3. **No accountability for storage** — the honor system (Part 0.4 of the overview)
   has no way to penalize a peer that advertises storage it doesn't maintain.

Proof-of-storage closes this gap: a TimeBeing (or any peer) can challenge a Calendar
to cryptographically prove possession of a chronon range, and the response is
verifiable with minimal bandwidth.

(@human — this is the "store or lose reputation" mechanic. A Calendar that can't
prove its claims gets its probity score adjusted. The P2P layer measures and
gossips; the application layer decides consequences.)

---

## 2. Design Goals

| Goal | Rationale |
|------|-----------|
| **Low bandwidth** | Proof must fit in a single gossipsub message (~256 B per leaf) |
| **No new dependencies** | Leverage existing `ForetiasMerkleProof` struct and `foretias_merkle_*` C11 functions |
| **Range proofs** | Support arbitrary `[chronon_start, chronon_end]` queries, not just single-chronon proofs |
| **Backward compatible** | v0.1 calendars without Merkle roots still function; proofs work on v0.7+ sealed blocks |
| **Compositional** | Proof of storage is orthogonal to chain integrity verification (`verify_chain()`) |

---

## 3. Core Concept — Merkle-Rooted CalendarBlocks

### 3.1 Current CalendarBlock (as-is)

```rust
struct CalendarBlock {
    block_id:      u64,
    written_at_ns: u64,
    ticks:         Vec<ChrononChainRecord>,   // soon renamed from TickRecord
}
```

A `CalendarBlock` groups `ChrononChainRecord`s for persistence. The block is sealed
(via `CryptoServer`) and persisted as a line in the encrypted JSONL file.

**Problem:** There is no root hash that binds all `ChrononChainRecord`s in a block
together. A peer cannot prove "I have block N with chronons X–Y" without sending the
entire block.

### 3.2 Extended CalendarBlock

Add a `merkle_root` field computed over the `ChrononChainRecord` leaves:

```rust
struct CalendarBlock {
    block_id:      u64,
    written_at_ns: u64,
    merkle_root:   [u8; 32],     // NEW — SHA-256 Merkle root over tick leaves
    ticks:         Vec<ChrononChainRecord>,
}
```

**Leaf computation** (per `ChrononChainRecord`):

```
leaf[i] = SHA-256( 0x00 || canonical_json(ticks[i]) )
```

This matches the existing `foretias_merkle_leaf()` convention (0x00 prefix for leaf
nodes). The Merkle tree is a standard balanced binary tree over the `ticks` vector.

**Block seal integration:** The `merkle_root` becomes part of the data that the
`CryptoServer` seals when writing a `CalendarBlock`. This means:

- A block with a valid seal implicitly has a valid `merkle_root`
- A proof-of-storage that verifies against the seal also verifies the root

(@human — the seal and the Merkle root are independent: the seal proves the block
was written by the owning TimeBeing. The Merkle root proves the contents of the
block. Together they give "written by X, contains Y".)

### 3.3 Migration Path

| Milestone | Behavior |
|-----------|----------|
| v0.1–v0.6 | CalendarBlocks have no `merkle_root`. Proof-of-storage not available. |
| v0.7 | New blocks carry `merkle_root`. Old blocks are readable (root defaults to `[0; 32]`). |
| v0.7+ | Proof-of-storage protocol is active. |
| Future | Compact old blocks: re-seal with `merkle_root` during background migration. |

---

## 4. Proof-of-Storage Protocol

### 4.1 Challenge — `StorageProofRequest`

A peer challenges a Calendar to prove possession:

```rust
struct StorageProofRequest {
    tbid:            [u8; 32],   // which TimeBeing's chrononchain
    chronon_start:   u64,        // inclusive start chronon number
    chronon_end:     u64,        // inclusive end chronon number
    challenger:      PeerId,     // who is asking (for probity attribution)
    nonce:           u64,        // prevents replay of cached proofs
}
```

**Constraints:**
- `chronon_end >= chronon_start`
- Range size bounded by a configurable `MAX_PROOF_RANGE` (default 1024 chronons)
  to prevent abuse (e.g. requesting the entire chain)
- `nonce` must be unique per challenger per session

### 4.2 Response — `StorageProofResponse`

The Calendar responds with proofs for each `CalendarBlock` that covers the range:

```rust
struct StorageProofResponse {
    tbid:         [u8; 32],
    proofs:       Vec<BlockProof>,         // one per CalendarBlock in range
    signer:       PeerId,                  // the Calendar that responded
    request_nonce: u64,                    // echoes the request nonce
}

struct BlockProof {
    block_id:     u64,
    merkle_root:  [u8; 32],               // root of the calendar block
    merkle_proof: ForetiasMerkleProof,     // siblings for the challenged range
    leaf_indices: Vec<u64>,               // which indices in the block are proven
    block_seal:   Vec<u8>,                // the seal from CryptoServer (optional, see §4.3)
}
```

`ForetiasMerkleProof` is the existing C11 struct:
```c
struct ForetiasMerkleProof {
    uint8_t depth;
    uint8_t siblings[32][32];
    uint8_t direction[32];  // 0=left, 1=right
    uint8_t leaf[32];
};
```

### 4.3 Seal vs Root — Two-Layer Verification

Proof-of-storage supports two verification modes:

**Mode A — Root-only (lightweight):**
- The challenger already knows the `merkle_root` for a block (e.g. from a prior
  gossip message or a cached calendar slice).
- The response provides only the `merkle_proof` and `leaf_indices`.
- Verification: recompute the root from the proof and compare.

**Mode B — Seal-included (full trust):**
- The challenger does NOT know the `merkle_root` for the block.
- The response includes the `block_seal` from the `CryptoServer`.
- Verification: validate the seal against the block's public key, then verify the
  Merkle proof against the unsealed block contents.

(@human — Mode A is the common path during active gossip. Mode B is used for
cold starts, disputes, or when a peer has never seen the block before.)

### 4.4 Range Proof Optimization

When a challenge covers chronons that span multiple `CalendarBlock`s, the response
carries one `BlockProof` per block. Each `BlockProof` proves only the subset of
leaves within that block that overlap the requested range.

For a range that covers the **entire** block, the `BlockProof` carries only the
`merkle_root` and `block_id` (no siblings needed — the root itself is the proof).

---

## 5. Verification Algorithm

The challenger verifies a `StorageProofResponse` in three stages:

```
1. STRUCTURAL VALIDATION
   - request_nonce matches the original challenge
   - tbid matches
   - chronon range is non-empty

2. PER-BLOCK VERIFICATION
   For each BlockProof in proofs:
   a. If block_seal is present (Mode B):
      - Verify seal with CryptoServer using the TBID's public key
      - Extract merkle_root from unsealed block
   b. If block_seal is absent (Mode A):
      - Use the known merkle_root from cache
   c. Verify merkle_proof against merkle_root using foretias_merkle_verify_proof()
   d. Confirm leaf_indices cover the requested chronon range overlap

3. COVERAGE CHECK
   - All chronons in [chronon_start, chronon_end] are covered by at least one BlockProof
   - No gaps in coverage
```

**Result:** `StorageProofResult { success: bool, coverage_ratio: f32, errors: Vec<ProofError> }`

`coverage_ratio` ranges from 0.0 (nothing proven) to 1.0 (full range proven).
Partial coverage is a valid result — it means the Calendar has *some* of the
requested range but not all.

---

## 6. Integration with Probity

Proof-of-storage results feed directly into the probity gossip system (§0.5 of
the overview). Two new opaque attribute names are introduced:

| Attribute | Semantics |
|-----------|-----------|
| `"storage_verified"` | +1.0 when a peer successfully proves storage for a challenged range |
| `"storage_failed"` | -1.0 × `coverage_ratio` when a peer fails to prove storage |
| `"storage_partial"` | -0.5 × `(1.0 - coverage_ratio)` when partial coverage is returned |

**Gossip flow:**

```
Peer A challenges Peer B on [TBID, chronons 100-200]
  → B responds with StorageProofResponse
  → A verifies, gets coverage_ratio = 0.7
  → A publishes ProbityReport("storage_partial", -0.15) on B
  → Network gossips, B's probity dips
```

The application layer (Foretias, not P2P) defines the attribute semantics. The P2P
layer only carries the reports and aggregates scores.

(@human — this is where the honor system bites. A Mirror node that can't prove
storage loses reputation. The U-shape aggregator ensures old failures fade but
recent failures hit hard.)

---

## 7. JSON-RPC Wire Format

Two new JSON-RPC methods are added to the TimeFamilyServer surface:

```typescript
// Challenge a Calendar to prove storage
storage_proof_request(
  tbid: string,           // hex-encoded TBID
  chronon_start: number,
  chronon_end: number,
  nonce: number
) -> StorageProofResponse

// Verify a proof response locally (no network)
storage_proof_verify(
  request: StorageProofRequest,
  response: StorageProofResponse,
  known_roots?: Map<number, string>  // optional, for Mode A
) -> StorageProofResult
```

**Wire format for `BlockProof`:**

```json
{
  "block_id": 42,
  "merkle_root": "hex...",
  "merkle_proof": {
    "depth": 5,
    "siblings": ["hex...", "hex...", ...],
    "directions": [0, 1, 0, 1, 0],
    "leaf": "hex..."
  },
  "leaf_indices": [3, 4, 5, 6, 7],
  "block_seal": "hex..."  // present only in Mode B
}
```

---

## 8. C11 Core Extensions

### 8.1 New Functions

| Function | Purpose |
|----------|---------|
| `foretias_merkle_root_from_leaves()` | Compute Merkle root from a leaf array |
| `foretias_merkle_range_proof()` | Generate proof siblings for a range of leaf indices |
| `foretias_merkle_verify_range_proof()` | Verify a range proof against a known root |

These extend the existing `merkle.c` without modifying `foretias_merkle_leaf()`
or `foretias_merkle_verify_proof()`.

### 8.2 New Error Codes

| Code | Meaning |
|------|---------|
| `FORETIAS_ERR_PROOF_RANGE_EMPTY = -3` | Requested range has no leaves |
| `FORETIAS_ERR_PROOF_RANGE_EXCEEDS = -4` | Range exceeds tree depth |

(`FORETIAS_ERR_BAD_PROOF = -2` is reused for general proof failures.)

---

## 9. Rust Domain Type Extensions

### 9.1 CalendarBlock

```rust
#[derive(Serialize, Deserialize)]
pub struct CalendarBlock {
    pub block_id:      u64,
    pub written_at_ns: u64,
    #[serde(default)]
    pub merkle_root:   [u8; 32],    // [0; 32] for pre-v0.7 blocks
    pub ticks:         Vec<ChrononChainRecord>,  // (renamed from TickRecord)
}

impl CalendarBlock {
    /// Compute the Merkle root over all tick leaves.
    pub fn compute_merkle_root(&self) -> [u8; 32];

    /// Generate a storage proof for a range of chronon indices within this block.
    pub fn prove_range(&self, indices: &[u64]) -> BlockProof;
}
```

### 9.2 Calendar Store

```rust
impl CalendarStore {
    /// Answer a storage proof challenge for a TBID and chronon range.
    /// Returns None if the store has no data for the requested range.
    pub async fn prove_storage(
        &self,
        tbid: &[u8; 32],
        chronon_start: u64,
        chronon_end: u64,
    ) -> Option<StorageProofResponse>;
}
```

---

## 10. Threat Model

| Threat | Mitigation |
|--------|------------|
| **Stale proof replay** | `nonce` in `StorageProofRequest` prevents replay |
| **Block forgery** | Mode B verifies `block_seal` with the TBID's public key |
| **Selective proof** (prove only good chronons) | Range must be contiguous; gaps detected by coverage check |
| **Denial of service** (huge ranges) | `MAX_PROOF_RANGE` cap (default 1024) |
| **Merkle root mismatch** | Root is sealed with the block; tampering invalidates the seal |

---

## 11. Out of Scope (Future Work)

| Item | Reason |
|------|--------|
| **Proof-of-retrieval** (timed download) | Requires network-layer timing; separate spec |
| **Erasure coding proofs** | Adds complexity; Merkle is sufficient for v0.7 |
| **Batch challenges** (multiple TBIDs per request) | Can be done by sending multiple requests; optimize later |
| **Zero-knowledge proofs** | Overkill for Foretias' threat model; Merkle is transparent and sufficient |

---

## 12. Acceptance Criteria

- [ ] `CalendarBlock` carries a `merkle_root` field (default `[0; 32]` for compat)
- [ ] `foretias_merkle_root_from_leaves()`, `foretias_merkle_range_proof()`,
      `foretias_merkle_verify_range_proof()` are implemented in C11 and tested
- [ ] Rust `CalendarBlock::compute_merkle_root()` and `prove_range()` work correctly
- [ ] JSON-RPC `storage_proof_request` and `storage_proof_verify` methods are wired
- [ ] Integration test: node A challenges node B for a range, B proves, A verifies
- [ ] Partial coverage is correctly reported (coverage_ratio < 1.0)
- [ ] Probity attributes `"storage_verified"`, `"storage_failed"`, `"storage_partial"`
      are publishable through the probity gossip handler
- [ ] Pre-v0.7 blocks (no merkle_root) are readable and produce `[0; 32]` gracefully

---

# END OF SPEC
