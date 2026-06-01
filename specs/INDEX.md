# Foretias — Open Work Index

**Generated:** 2026-06-01
**Purpose:** Single reference for all plans/specs with open tasks, organized by urgency and dependency order. Deferred and backburnered features at the end.

---

## How to Read This Index

- **Urgency** = how soon the work blocks other work or is needed
- **Dependency** = what must finish before this can start
- Each entry lists: spec/plan pair, open task count, status, dependencies, and a brief description

---

## TIER 1 — URGENT (active work, immediate blockers)

### 1. Group 7 — Communerdette (SIGNING ARCHITECTURE + REMNANTS)

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP7_COMMUNERDETTE_PLAN.md` | 0 | 318 | **COMPLETE** — all open items resolved |
| `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` | — | — | Spec (paired) |

**Dependencies:** None (self-contained)
**Blocks:** Group 4 (mirror), Group 6 (mutual attestation) — **unblocked**

**Completed 2026-06-01:**
- Phase 4.4: ✅ Verified `handle_route_stamp` response shape — **BROKEN**, then **FIXED** (added `signature_bytes()`/`signature_algorithm()` accessors to `CleanAuthenticated<Foretis>`)
- Phase 8.2: ✅ Calendar `DoAttestation` explored — 5 blockers identified, 7-10h estimated effort. Item 3 already satisfied.
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

### 2. Group 4 — Calendar Active Mirroring + Proof of Storage

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP4_PLAN.md` | 68 | 0 | **PAUSED** — blocked on Group 7 + Group 6 |
| `specs/COMBINED_GROUP4_SPEC.md` | — | — | Spec (paired) |
| `specs/CALENDAR_ACTIVE_MIRRORING_PLAN.md` | 18 | 0 | **DEPRECATED** → superseded by Group 4 |
| `specs/CALENDAR_PROOF_OF_STORAGE_PLAN.md` | 26 | 0 | **DEPRECATED** → superseded by Group 4 |

**Dependencies:**
1. Group 7 (Communerdette) reaches feature completion ← **current blocker**
2. Group 6 (Mutual Attestation) spec approved and interfaces stable
3. Plan rewritten to use Communerdette as mirror-traffic carrier

**Blocks:** Stream 4c (Proof of Storage), calendar replication

**Resumption preconditions (all must be true):**
1. Group 7 spec reaches feature completion
2. Group 6 spec approved and interfaces stable
3. Plan rewritten to reflect Communerdette architecture

---

### 3. Group 6 — Mutual Attestation

| File | Open | Done | Status |
|------|------|------|--------|
| `specs/COMBINED_GROUP6_MUTUAL_ATTESTATION_SPEC.md` | — | — | **BACKBURNERED** — not yet started |

**Dependencies:** Group 7 (Communerdette) completion
**Blocks:** Group 4 resumption

**Note:** This spec is backburnered. No plan file exists yet. The spec needs to be reviewed and a plan created before work can begin.

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
                 └── Group 7 (Communerdette) ◄──┘
                      │
                      ├──► Group 6 (Mutual Attestation) [BACKBURNERED]
                      │
                      └──► Group 4 (Mirror + Proof of Storage) [PAUSED]
                           ├── Stream 4b: Active Mirroring
                           └── Stream 4c: Proof of Storage
```

---

## Quick Reference: What to Work On Next

| Priority | Item | Action |
|----------|------|--------|
| **1** | Group 7 Phase 18.6 | Run unwrap/expect audit (informational) |
| **2** | Group 7 Phase 8.2 | Implement Calendar `DoAttestation` with CommunerdetteLine |
| **3** | Group 6 | Review spec, create plan, implement mutual attestation |
| **4** | Group 4 | Rewrite plan for Communerdette architecture, resume mirror work |
| **5** | Group 1 | Verify which of 66 open items are stale vs genuine |
| **6** | Group 3 | Review 45 open items for relevance |
| **7** | Group 5 g5-c | Human decision on JSON-RPC auth (mTLS) |
