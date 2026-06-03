# Foretias — Open Work Index

**Generated:** 2026-06-02
**Purpose:** Single reference for all plans/specs with open tasks, organized by urgency and dependency order. Deferred and backburnered features at the end.

---

## How to Read This Index

- **Urgency** = how soon the work blocks other work or is needed
- **Dependency** = what must finish before this can start
- Each entry lists: spec/plan pair, open task count, status, dependencies, and a brief description

---

## TIER 1 — URGENT (active work, immediate blockers)

### 1. POST7 Audit & Cleanup

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/POST7_AUDIT_CLEANUP_PLAN.md` | 0 | 45 | **COMPLETE** — all 5 phases done |
| `specs/POST7_AUDIT_CLEANUP_SPEC.md` | — | — | Spec (paired) |

**Completed 2026-06-02:**
- Phase 1: ✅ 3 unwrap/expect fixes (P0-P1)
- Phase 2: ✅ ~65 mutex poison migration calls (std → parking_lot)
- Phase 3: ✅ 15 dead code stubs addressed
- Phase 4: ✅ Pre-P2P finding resolved (already correct)
- Phase 5: ✅ Final verification — all tests green

---

### 2. Group 7 — Communerdette (SIGNING ARCHITECTURE + REMNANTS)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md` | 0 | 318 | **COMPLETE** — all open items resolved |
| `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` | — | — | Spec (paired) |

**Dependencies:** None (self-contained)
**Blocks:** Group 4 (mirror), Group 6 (mutual attestation) — **unblocked**

**Completed 2026-06-01:**
- Phase 4.4: ✅ Verified `handle_route_stamp` response shape — **BROKEN**, then **FIXED** (added `signature_bytes()`/`signature_algorithm()` accessors to `CleanAuthenticated<Foretis>`)
- Phase 8.2: ✅ Calendar `DoAttestation` explored — 5 blockers identified. All resolved by Group 4 Streams 4d/4e.
- Phase 10: ✅ Removed obsolete direct peer-call paths (`stamp_peer`, `PeerTransport::stamp`, tier wrappers). 571 tests pass.
- Phase 18.6: ✅ Unwrap/expect audit complete — `docs/security/unwrap-audit.md` with 10 ranked items, 1 Critical, 2 Moderate

**Completed 2026-06-02:**
- Signing boundary enforced: ✅ Calendar now owns its own Ed25519 signing key (per AGENTS.md § Signing Boundary)
- Documentation: ✅ SAFETY.md § Per-Component Key Ownership + AGENTS.md implementation status note
- Phase 10: ✅ Removed obsolete direct peer-call paths (`stamp_peer`, `PeerTransport::stamp`, tier wrappers). 571 tests pass.
- Phase 18.6: ✅ Unwrap/expect audit complete — `docs/security/unwrap-audit.md` with 10 ranked items, 1 Critical, 2 Moderate

**Key design decisions (resolved 2026-05-31):**
- Signing boundary: internal calls don't need signing; external boundary does
- Signing initiation: Communerdette requests signing when external transmission needed
- Key ownership: signing key stays with the time being having the TBID
- `sign_response` parameter pattern replaces `sign_tbid_message`
- `Externalized<T>` is the canonical outbound type (concrete types removed)
- `stamp_chronon` API gives Communerd serialization control

---

## TIER 2 — HIGH PRIORITY (blocked on Group 7 completion)

### 2. Group 4 — Calendar Active Mirroring + Proof of Storage + GNF/FB

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP4_PLAN.md` | 0 | 121 | **COMPLETE** — all 5 streams implemented |
| `specs/COMBINED_GROUP4_SPEC.md` | — | — | Spec (paired, revised) |
| `specs/CALENDAR_ACTIVE_MIRRORING_PLAN.md` | 18 | 0 | **DEPRECATED** → superseded by Group 4 |
| `specs/CALENDAR_PROOF_OF_STORAGE_PLAN.md` | 26 | 0 | **DEPRECATED** → superseded by Group 4 |

**Completed 2026-06-02:**
- **4a:** ✅ Libp2p unit tests
- **4b:** ✅ Calendar Active Mirroring (4b.1-4b.5 merged, StartStream placeholder deferred)
- **4c:** ✅ Calendar Proof of Storage (C11 merkle, FFI, CalendarBlock, prove_storage, handlers, probity, integration tests)
- **4d:** ✅ GNF/FB Mutual Attestation (DoChronon/DoEpochAttestation, stamp_my_chronon/block, FB verification, integration tests)
- **4e:** ✅ Chronon Retrieval + FB Verification (get_chronon/chain, VerifyFbRecorded, integration tests)
- **Calendar signing key:** ✅ Calendar now owns its own Ed25519 signing key (per AGENTS.md signing boundary)

**Calendar signing key implementation:**
- `Calendar::sign_foretis()` — signs with Calendar's own key
- `Calendar::calendar_public_key()` — exposes public key
- DoChrononAttestation/DoEpochAttestation handlers sign before transmission
- Documented in SAFETY.md § Per-Component Key Ownership

---

### 3. Group 6 — Mutual Attestation

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md` | — | — | **SUPERSEDED** — mutual attestation now specified in Group 4 Stream 4d |

**Dependencies:** N/A — superseded
**Blocks:** Nothing

**Note:** The mutual attestation protocol (GNF/FB) is now specified in `COMBINED_GROUP4_SPEC.md` §5 (Stream 4d) and planned in `COMBINED_GROUP4_PLAN.md` (Phases 4d.1–4d.8). This Group 6 spec is superseded and can be archived.

---

## TIER 3 — MEDIUM PRIORITY (foundational work, partially complete)

### 4. Group 3 — P2P Major Features

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP3_PLAN.md` | 45 | 0 | Needs review — may be stale |

**Dependencies:** Group 2 (pre-flight)
**Blocks:** Group 4 (some items)

**Note:** 45 open items. Needs review to determine which are still relevant vs superseded by later work.

---

### 5. Group 5 — Architectural Foundations

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP5_PLAN.md` | 30 | 20 | Partially complete |

**Dependencies:** Group 2 (pre-flight)
**Blocks:** Group 4 (Clock injection prerequisite)

**Status:**
- g5-b (crash recovery): merged
- g5-d (AGENTS.md): merged
- g5-a (clock injection): merged
- g5-c (JSON-RPC auth): **blocked on human decision** (Option B — mTLS, needs dedicated spec)

---

### 6. Group 1 — Type-Based Safety Enforcement (Take 3)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_PLAN.md` | 66 | 22 | Needs verification |

**Dependencies:** None (foundational)
**Blocks:** Group 7 (depends on Take 3 types)

**Note:** 66 open items. The core types (`UnverifiedSignatureEnvelope`, `CleanAuthenticated`, `Externalized`) are implemented and used throughout. Needs verification of which items are stale vs genuinely open.

---

### 7. Type-Enforced Cleansing and Authentication (Legacy)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/TYPE_ENFORCED_CLEANSING_AND_AUTHENTICATION_PLAN.md` | 58 | 10 | Likely stale — superseded by Group 1 Take 3 |

**Dependencies:** None
**Blocks:** None (superseded)

**Note:** This plan predates the Group 1 Take 3 work. Most items are likely stale. Needs review to confirm.

---

## TIER 4 — LOW PRIORITY (improvements, tooling)

### 8. Peering v1 (Auto-Port, Self-Registration, DHT Discovery)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/PEERING_V1_PLAN.md` | 8 | 17 | Phase 2 & 3 verified, Phase 4 partial, Phase 5 pending |

**Dependencies:** DHT discovery (done)
**Blocks:** Nothing critical

**Open items (8):**
- Python/Java bindings for auto-port (4 items) — blocked on bindings reintroduction
- Cross-language DHT discovery tests (4 items) — blocked on bindings reintroduction

---

### 9. Code Quality Tooling

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/CODE_QUALITY_TOOLING_PLAN.md` | 71 | 0 | **PROPOSED** — not started |

**Dependencies:** None
**Blocks:** Nothing

**Description:** Clippy configuration, cargo-geiger, formatting standards. Low urgency but good hygiene.

---

### 10. Group 2 — Workspace Hygiene (Verification Only)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP2_PLAN.md` | 9 | 7 | Verification-only — all code done |

**Dependencies:** None
**Blocks:** Nothing (pre-flight checks)

**Note:** The 9 open items are pre-flight verification checks (grep for issues, run tests). All code changes are already shipped. These are "run and confirm" tasks, not implementation.

---

## TIER 5 — DEFERRED / BACKBURNERED

These features are explicitly out of active scope. They remain as specifications for future reintroduction.

### Python Bindings (Thin Client)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/PYTHON_BINDINGS_THIN_PLAN.md` | 33 | 1 | **BACKBURNERED** |
| `specs/PYTHON_BINDINGS_THIN_SPEC.md` | — | — | Spec (paired) |
| `specs/THIN_CLIENT_SPEC.md` | — | — | Thin client specification |
| `specs/PYTHON_REMOVAL_PLAN.md` | — | — | Removal plan (executed) |

**Reason:** Removed from active scope per `SCOPE_REDUCTION_SPEC.md`. Python shim (`src/foretias/`), PyO3 bindings (`p2p/foretias-python/`), and tests (`tests/`) are not built or maintained.

**Reintroduction path:** See `SCOPE_REDUCTION_SPEC.md`. The Rust library API (`foretias-server`) exposes all protocol logic. Python bindings are a thin wrapper over this stack.

---

### Java Bindings

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/PEERING_V1_PLAN.md` (lines 534-541) | — | — | **BACKBURNERED** |

**Reason:** Removed from active scope per `SCOPE_REDUCTION_SPEC.md`. Java JNI crate (`p2p/foretias-java/`) is not built or maintained.

---

### Whitepaper

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/WHITEPAPER_PLAN.md` | 1 | 1 | **BACKBURNERED** |
| `specs/WHITEPAPER_SPEC.md` | — | — | Spec (paired) |

---

### HTTPS Transport

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/FORETIAS_6_HTTPS_TRANSPORT_PLAN.md` | 66 | 22 | **BACKBURNERED** |
| `specs/FORETIAS_6_HTTPS_TRANSPORT_SPEC.md` | — | — | Spec (paired) |

---

### Centralize P2P Documentation

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/CENTRALIZE_DEFINITION_DOCUMENTATION_OF_P2P_PLAN.md` | 0 | 0 | **BACKBURNERED** |

---

### DHT Stress Test Analysis

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/DHT_STRESS_TEST_ANALYSIS_PLAN.md` | 0 | 1 | **BACKBURNERED** |

---

### Code Quality Tooling Spec

| File | Status |
|------|--------|
| `specs/CODE_QUALITY_TOOLING_SPEC.md` | **BACKBURNERED** |

---

## Dependency Graph (Simplified)

```
Group 2 (pre-flight)
  └── Group 1 (Take 3 types) ──────────────────┐
       └── Group 5 (arch foundations) ──────────┤
            └── Group 3 (P2P features) ─────────┤
                 └── Group 7 (Communerdette) ◄──┘  [COMPLETE ✅]
                      │
                      ├──► Group 4 (Mirror + PoS + GNF/FB) [COMPLETE ✅]
                      │    ├── Stream 4a: Libp2p unit tests ✅
                      │    ├── Stream 4b: Active Mirroring ✅
                      │    ├── Stream 4c: Proof of Storage ✅
                      │    ├── Stream 4d: GNF/FB Mutual Attestation ✅
                      │    └── Stream 4e: Chronon Retrieval + FB Verification ✅
                      │
                      └──► POST7 Audit & Cleanup [COMPLETE ✅]
```

---

## Quick Reference: What to Work On Next

| Priority | Item | Action |
|----------|------|--------|
| **1** | Group 1 | Verify which of 66 open items are stale vs genuine |
| **2** | Group 3 | Review 45 open items for relevance |
| **3** | Group 5 g5-c | Human decision on JSON-RPC auth (mTLS) |
| **4** | Group 4 StartStream | Implement live calendar streaming to mirrors (deferred) |
| **5** | Group 4 periodic scheduling | Randomized FB verification scheduling via TickObserver |
