# Session Plan — RUST_CLEANUP (L3-L5), Phase 10, v0.3 DHT

**Branch:** `mvp`
**Current state:** 8 commits ahead of `origin/mvp`, 229 tests passing
**Goal:** Ship L3-L5 cleanup, finalize Phase 10 docs, complete v0.3 DHT Peer Discovery tag

---

## Task 1: RUST_CLEANUP_PLAN.md — L3, L4, L5

### L3: Serialization Strictness (`foretias-node/src/main.rs:587`)
**Current:** `serde_json::from_slice(&plaintext)` at line 589 — trusts deserialized JSON directly.
**Fix:** Add a validation wrapper that checks the deserialized JSON-RPC response has a valid structure before using it.

**Files:**
- `p2p/foretias-node/src/main.rs` — wrap `json_rpc_call` response parsing with explicit schema check

**Test:** Existing `test_stamp_and_verify_e2e` covers the happy path; add no new test (this is defensive hardening).

### L4: Error Specificity in Noise I/O (`noise.rs:252-270`)
**Current:** `write_len` and `read_len` map all I/O errors to `CryptoError::Internal(-99)`.
**Fix:** Map to specific variants:
- Connection reset → `Internal(-99)` stays (no better fit)
- Read timeout → new variant or keep generic
- Oversized message → `Internal(-99)` stays

Actually, looking at the code more carefully:
- `write_len` at line 270-273: 3 calls to `CryptoError::Internal(-99)` for write_all/write_all/flush
- `read_len` at line 277-286: `read_exact` → `Internal(-99)`, oversized → `Internal(-99)`, second `read_exact` → `Internal(-99)`

**Fix:** Add 2 new `CryptoError` variants:
- `IoWrite(String)` — for write failures
- `IoRead(String)` — for read failures  
- Keep `Internal(-99)` for the oversized guard (it's a protocol violation, not I/O)

**Files:**
- `p2p/core-engine/src/error.rs` — add `IoWrite`, `IoRead` variants to `CryptoError`
- `p2p/core-engine/src/noise.rs` — replace `Internal(-99)` with specific variants in `write_len` / `read_len`

**Test:** `noise_handshake` unit tests still pass; existing integration test `test_stamp_and_verify_e2e` covers happy path.

### L5: Lint Policy Alignment (`foretias-python/src/lib.rs`)
**Current:** `core-engine/src/lib.rs` has `#![deny(unsafe_op_in_unsafe_fn)]`. `foretias-python/src/lib.rs` has `#![allow(unsafe_op_in_unsafe_fn)]`.
**Analysis:** These are intentionally different — PyO3 bindings inherently require `unsafe` within `unsafe fn`, and the deny policy would cause hundreds of warnings. The current split is correct.
**Decision:** **NO CHANGE NEEDED.** The lint policy is already appropriately scoped per crate. Document this decision in RUST_CLEANUP_PLAN.md.

---

## Task 2: PYTHON_REMOVAL_PLAN.md — SKIP

User directive: skip Python removal. Maintain the Rust-library-backed Python CLI (`foretias_p2p` via PyO3) as-is. No changes.

---

## Task 3: FORETIAS_2_IMPLEMENTATION_PLAN.md — Phase 10

### 3.1: Update `README.md`
**Current state:** README documents `foretias serve`, `stamp`, `verify`, `inspect-attestations` CLI. Missing:
- DHT-related flags (`--dht-namespace`, `--known-servers`, `--p2p-listen`, `--p2p-dial`)
- P2P concepts (libp2p, DHT, gossipsub)
- Communerd component
- Python bindings usage via `foretias_p2p`

**Fix:** Update README.md to document:
- Full serve command with P2P flags
- Brief architecture section (Chronomatter, Calendar, Communerd)
- Current feature list (what's implemented: v0.1 stamp/verify, v0.2 mutual attestation, v0.3 DHT discovery)

### 3.2: Update `FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md`
**Current state:** Spec uses `PeerConnectivity` in places; actual code uses `Communerd`.
**Fix:** Already reviewed — the spec consistently uses `Communerd` now. No changes needed.
**Decision:** **NO CHANGE NEEDED.**

### 3.3: `wdocs/specs/foretias-v1.md`
**Current state:** `wdocs/` directory does not exist. `foretias-v1.md` lives in `specs/` and `specs/foretias-v1.md`.
**Fix:** Update `specs/foretias-v1.md` to document mutual attestation semantics if not already present.
**Decision:** Check `specs/foretias-v1.md` — if mutual attestation is already documented, **NO CHANGE NEEDED**.

### 3.4: Tag `v0.2-direct-p2p-mutual-attestation`
**Fix:** Git tag `v0.2-direct-p2p-mutual-attestation` on current HEAD (or the commit that completes Phase 10).

---

## Task 4: v0.3 DHT Peer Discovery

### What's Already Done (Audit)
| Spec Item | Status | Evidence |
|-----------|--------|----------|
| Kademlia with private namespace | ✅ DONE | `behaviour.rs` — `kad_protocol = "/foretias/kad/{namespace}/1.0.0"` |
| DHT bootstrap | ✅ DONE | `communerd/mod.rs:bootstrap_dht()` + `SwarmCommand::Bootstrap` |
| Self-registration (PUT) | ✅ DONE | `communerd/mod.rs:register_and_discover()` |
| Peer discovery (GET) | ✅ DONE | `communerd/mod.rs:register_and_discover()` + `lookup_tbid()` |
| TBID → Peer mapping | ✅ DONE | `DhtPeerSource` + `tbid_index` cache in Communerd |
| Config: namespace, bootstrap | ✅ DONE | `DHTConfig { namespace, bootstrap }` in `config/p2p.rs` |
| CLI: `--dht-namespace` | ✅ DONE | `main.rs:67` |
| CLI: `--known-servers` | ✅ DONE | `main.rs:65` |
| CLI: `--dht-bootstrap` | ✅ DONE | `main.rs:71` |
| Two swarms connect + identify | ✅ DONE | Integration test `two_swarms_connect_and_identify` |
| Three nodes connect | ✅ DONE | Integration test `three_nodes_discover_and_attest` |
| Gossip propagation | ✅ DONE | Integration test `gossip_probity_propagation` |

### What's Missing
| Spec Item | Status | Effort |
|-----------|--------|--------|
| Three-node DHT discovery test (A announces, B bootstraps, C finds A via B) | ❌ NOT DONE | 1 new integration test |
| v0.3 milestone tag | ❌ NOT DONE | 1 git tag |

### Implementation: DHT Discovery Integration Test
**Goal:** Prove that a node can discover peers through the DHT without knowing their addresses directly.

**Test: `dht_discovery_three_nodes`**
```
Setup:
- Node A: starts, listens, self-registers to DHT
- Node B: starts, listens, dials A, self-registers to DHT
- Node C: starts, listens, dials B only (does NOT know A)
- C issues GetRecord for A's peer registration key
- C discovers A via DHT through B

Verification:
- C receives RecordRetrieved event with A's PeerRegistrationRecord
- C can subsequently dial A using the discovered address
```

**File:** `p2p/foretias-node/tests/integration.rs` — add new test function
**Lines:** ~100

### v0.3 Tag
**Tag:** `v0.3-dht-discovery`

---

## Execution Order

```
1. L3: Serialization strictness          (~30 min)
2. L4: Error specificity in Noise I/O    (~30 min)
3. L5: Lint policy alignment             (NO CHANGE — document decision)
4. cargo test --workspace                 (verify 229+ pass)
5. Commit L3-L5                           (1 commit)
6. README.md update                       (~1 hour)
7. v0.3 DHT integration test              (~1 hour)
8. cargo test --workspace                 (verify all pass)
9. Commit Phase 10 + v0.3 test            (1 commit)
10. Tag: v0.2-direct-p2p-mutual-attestation
11. Tag: v0.3-dht-discovery
```

**Total estimated effort: 3-4 hours**

---

## Risks
| Risk | Mitigation |
|------|-----------|
| L4 changes break noise handshake | Existing tests cover handshake; add targeted test if needed |
| DHT discovery test is flaky on CI | Use generous timeouts (15s), retry logic |
| README.md update introduces inaccuracies | Cross-reference against actual CLI (`--help`) |
