# Foretias — Pre-P2P Product & Code Review (OpenCode)

**Reviewer:** OpenCode 1.14.28; vllm/Qwen3.6-27B-AWQ-BF16-INT4
**Date:** 2026-05-19
**Scope:** Holistic multi-perspective review — code organization, correctness, security, product design, mindshare, lifecycle, future directions, spec-to-implementation fidelity
**Prior art read:**
- `specs/pre-public-mvp-code-review.md` (Claude Opus 4.7, 2026-05) — 409 lines, technical review
- `specs/CLAUDE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` (Claude Sonnet 4.6, 2026-05) — 1753 lines, product/code review
**Live exploration:** 4 parallel explore agents (architecture, security, P2P, specs/plans)
**Status of P2P:** Implemented but not integration-verified against live multi-node networks

---

## 0. Executive Summary

Foretias is a cryptographically sound timestamping system with a genuinely novel core mechanism: per-chronon ephemeral keypairs with self-documenting chain transitions. The architecture is elegant, the spec discipline is excellent, and the codebase shows clear engineering intent.

**However, the implementation has reached an inflection point where accumulated technical debt, spec drift, and missing verification infrastructure threaten public readiness.** The P2P layer is wired but unverified. Critical security findings from prior reviews remain unaddressed. The spec ecosystem is 40% stale. No PR-blocking CI exists.

### The Nightmare Test — Direct Answer

> *"If more than 50% of servers are hacked, will my mind have a reassuring answer?"*

**Yes for past stamps. Partially for live stamps. No safety net yet for future stamps.**

- **Past stamps:** Once chronon N's private key is algorithmically incapacitated by the advance to chronon N+1, no valid Foretis for chronon N can be created. This is verifiable offline, forever, without any server alive. This is the ironclad guarantee.
- **Live stamps:** A compromised server holds the current tick's private key. Damage window is bounded by chronon duration (default 60 seconds). Detectable by any honest peer with a mirrored calendar.
- **Future stamps:** FROST epoch consensus (v0.6) would provide k-of-n threshold signing — with k > N/2, a majority compromise cannot forge epoch snapshots. **FROST is currently triple-stubbed** (sign, verify, and epoch handlers all return fabricated data).

**The gap to close:** Implement FROST, add calendar mirroring verification, add "last honest tick" attestation. These are tracked in the improvement recommendations below.

---

## 1. Product Identity & Mindshare

### 1.1 The One-Sentence Pitch Is Missing

The core insight deserves to be the first thing any reader sees:

> "A timestamp that is mathematically impossible to backdate — because the signing key self-destructs after every tick."

This sentence appears nowhere in README.md or any public-facing documentation. The README opens with the acronym expansion and dense technical prose. Most potential users stop reading before reaching the value proposition.

**Keywords:** `one-sentence pitch`, `value proposition`, `README optimization`, `elevator pitch`.

### 1.2 The Latin Taxonomy — Asset or Liability?

The Latin naming (*Chronos fidelius authenticus*, *Chronos fidelius grapha*, etc.) is memorable and differentiating. It signals deliberate system design. However:

- Requires translation for every new reader
- `Foretis` (singular artifact) vs `Foretias` (project name) looks like a typo to newcomers
- `Communerd` may not translate across cultures

**Recommendation:** Keep the names; add a terminology glossary at the top of every public document. Consider `pub use Foretis as Timestamp;` aliases for library consumers.

### 1.3 Audience Segmentation

| Audience | Primary Need | Current Gap |
|---|---|---|
| Application developers | Embed timestamping | No published library crate, no SDK docs |
| Infrastructure operators | Run a network | CLI works; no monitoring, no auth |
| Auditors/verifiers | Verify a Foretis | `verify-with-proof` works; hard to discover |

**Keywords:** `library-first API`, `SDK documentation`, `verifier persona`, `operator persona`.

### 1.4 Competitive Landscape

| Competitor | Strength | Foretias Advantage |
|---|---|---|
| RFC 3161 TSA | Enterprise trust | Decentralized, no trusted third party |
| OpenTimestamps | Bitcoin-backed, free | No blockchain dependency, faster |
| Blockchain timestamps | Decentralized | No transaction cost, no confirmation wait |
| Certificate Transparency | Public audit | Timestamps, not just existence proofs |

**Unique position:** Mathematically provable backdate-resistance without a trusted third party or blockchain. No other system offers this through ephemeral key rotation.

---

## 2. Code Organization & Architecture

### 2.1 Three-Crate Architecture (Current State)

The workspace has settled into a clean three-crate structure:

```
p2p/
├── core-engine/       # Safe Rust wrappers over C11 FFI + domain logic
├── foretias-client/   # Thin client library (Standalone, PtP, P2P levels)
└── foretias-server/   # Server + CLI binary + Communerd (libp2p)
```

This is well-organized. Module boundaries are clear. The dependency chain (C11 → core-engine → client → server) is logical and enforceable.

### 2.2 Domain Types — Well-Structured

Core types (`Foretis`, `ChrononRecord`, `Calendar`, `ExternalAttestation`, `Tbid`, `TbidSecret`) are properly defined with strong types, newtype wrappers, and checked constructors. `TbidSecret` correctly avoids `Debug` derive. `SignatureBytes` and `PublicKeyBytes` prevent accidental byte array confusion.

### 2.3 CryptoServer Trait — Correct Abstraction, Incomplete Implementation

The `CryptoServer` trait (`p2p/core-engine/src/crypto_server/mod.rs`) is the right abstraction for algorithm agility and hardware backends. However:

- **Stubbed methods:** `frost_sign_partial`, `ecdh_ed25519`, `ecdh_p256` return `Unsupported`
- **P-256:** All three operations (sign, verify, keypair) are stubs in both Rust and C11
- **Algorithm hardcoding:** `chronomatter/mod.rs:355` hardcodes `Ed25519` instead of querying `crypto.signature_algorithm()`

**[REPEAT from prior reviews]** `sign()` / `signature_algorithm()` mismatch was identified as CRIT in `pre-public-mvp-code-review.md §2.1`. May be fixed now — verify current state.

### 2.4 Error Handling — Adequate Structure

`NodeError` and `CryptoError` (`p2p/core-engine/src/error.rs`) are well-structured domain errors. `c_result_to_error` maps C11 return codes to Rust errors. However, some paths swallow errors (e.g., `handlers.rs:138` — `serde_json::from_value(v.clone()).ok()` discards parse errors).

### 2.5 Chronomatter Concurrency — Latent Bugs

**[REPEAT from CLAUDE review §2.1-2.5]** The `stamp()` and `daemon_tick()` code paths both increment `current_tick` via CAS. Under concurrent load:

1. CAS failures cause `stamp()` to return errors to callers
2. `fetch_add` would be more appropriate than CAS
3. `keypairs` Vec is unbounded — grows one entry per stamp, never shrinks
4. `build_auto_attestation` index arithmetic (`keypairs[tick-1]`) assumes sequential indexing that breaks under concurrency

**Keywords:** `CAS race`, `fetch_add`, `unbounded keypairs`, `concurrent stamping`, `chronomatter/mod.rs:302-320`.

---

## 3. Security Analysis

### 3.1 CRITICAL — Bindings.rs Debug Derives on Secret Types

**File:** `p2p/core-engine/src/core/bindings.rs`

The following FFI binding structs derive `Debug`, making raw secret material loggable:

| Struct | Line | Secret Size | Severity |
|---|---|---|---|
| `ForetiasPrivKey32` | 103 | 32 bytes (Ed25519 seed) | CRITICAL |
| `ForetiasSecretKeyVar` | 215 | Up to 4096 bytes (PQC secret) | CRITICAL |
| `ForetiasTbidV1SecretKey` | 306 | 160 bytes (dual-key TBID secret) | CRITICAL |
| `ForetiasNoiseState` | 413 | Full session state (keys, chaining key, hash) | HIGH |
| `ForetiasKemSecretKey` | 260 | Up to 2400 bytes (ML-KEM secret) | MED |

Any `format!("{:?}", ...)` on these types emits raw key material. While `PrivKeyHandle` (the safer wrapper) blocks this, `generate_ed25519_keypair()` returns `ForetiasPrivKey32` directly and is used in Noise handshakes.

**Fix:** Remove `Debug` derive. Use `zeroize::Zeroize` derive or manual impl.

### 3.2 CRITICAL — DHT Poisoning (No Cryptographic Binding)

**File:** `p2p/foretias-server/src/communerd/mod.rs:41-51, 586-605`

`PeerRegistrationRecord` stores `peer_id`, `tbid`, `multiaddr`, `json_rpc` as plain JSON with no signature. Any node can register any TBID under their own address in the DHT.

**Attack:** Attacker registers victim's TBID pointing to their own RPC address. All cross-node verification requests for that TBID are redirected to the attacker, who serves forged calendar records.

**Fix:** Sign `PeerRegistrationRecord` with the claiming node's TBID signing key. Verifiers check the signature before trusting DHT entries.

### 3.3 HIGH — Cross-Node Verification Trusts Remote Calendar Without Verification

**File:** `p2p/foretias-server/src/server/handlers.rs:191-237`

`cross_node_verify` looks up TBID via DHT (vulnerable to poisoning above), fetches calendar slice, and uses the remote record's public key directly. No Merkle proof or chain-of-trust verification.

**Combined with §3.2:** Full verification bypass for cross-node verify.

### 3.4 HIGH — Probity Signature Verification Is Stubbed

**File:** `p2p/foretias-server/src/probity/gossip_handler.rs:36-51`

The gossip handler only checks `signature.len() >= 64`. Does NOT verify the Ed25519 signature against the reporter's public key. Any peer can forge probity reports to manipulate reputation scores.

**This is a complete trust model failure.** The probity system is wired but its trust foundation is broken.

### 3.5 HIGH — FROST Epoch Consensus Is Triple-Stubbed

**Files:**
- `software.rs:284` — `frost_sign_partial` returns `Unsupported`
- `frost_bridge.rs:28-46` — `run_frost_round` returns all-zeros signature
- `handlers.rs:354-376` — `handle_verify_epoch_snapshot` always returns `valid: true`

Any node can forge an epoch snapshot with arbitrary data, and it will be accepted as valid. Not an attack vector today (nobody acts on epoch snapshots), but catastrophic if code depends on epoch validity before FROST is implemented.

**[REPEAT from CLAUDE review §15.1]**

### 3.6 HIGH — Genesis Verification Bypass via Short Foretis

**File:** `p2p/core-engine/src/foretias/tick.rs:308-317`

The genesis tick path splits `forward_foretis` into Ed25519 (first 64 bytes) and genesis signature (remaining bytes). If `forward.len() < 64`, the entire malformed signature is treated as the Ed25519 component and `forward_genesis` is empty. For `tb_version == 0`, empty genesis is accepted as valid.

**Attack:** Crafted calendar with short `forward_foretis` bypasses genesis verification.

### 3.7 HIGH — PQC Signature Length Not Validated Against Buffer

**File:** `p2p/core/src/signing_sphincs.c:53`, `signing_dilithium.c:53`

`sig->len` is passed directly to liboqs without validating `sig->len <= FORETIAS_SIG_MAX_SIG_BYTES`. A malicious caller can set `len` larger than the buffer, causing out-of-bounds reads.

### 3.8 HIGH — C11 Core Safety Issues (from pre-public review)

**[REPEAT from pre-public-mvp-code-review.md §3.1]**

| Issue | File | Severity |
|---|---|---|
| Nonce overflow (2^64 wrap) | `noise_xx.c:69,86` | CRITICAL |
| HKDF stack overflow (info_len > 63) | `privkey.c:236-240` | HIGH |
| Global mutable state | `privkey.c:74-78` | HIGH |
| Dynamic allocation (calloc/free) | `privkey.c:83,125` | HIGH |
| Stack residue on early returns | `noise_xx.c:241-247, ...` | MED |
| Missing OQS_init() | `signing_sphincs.c`, etc. | MED |

### 3.9 Zeroization Inventory

| Secret | Location | Zeroized? | Method |
|---|---|---|---|
| Ed25519 tick private key | `chronomatter.rs: keypairs[i].priv_key` | Yes | `PrivKeyHandle::drop` → C11 memzero |
| Seal key | `software.rs:34` | Yes | `Zeroizing<[u8;32]>` |
| SPHINCS+ secret | `software.rs:38` | Yes | `Zeroizing` |
| Dilithium3 secret | `software.rs:40` | Yes | `Zeroizing` |
| SLH-DSA-256f secret | `software.rs:43` | Yes | `Zeroizing` |
| ML-KEM secret | `software.rs:46` | Yes | `Zeroizing` |
| FROST shares | `software.rs:36` | Yes | `Zeroizing<Vec<u8>>` |
| **TBID secret (Rust Vec)** | `signing_tbid.rs:32-44` | **NO** | Plain `Vec<u8>` |
| **Noise static private key** | `server/mod.rs:83-84` | **NO** | Plain `[u8;32]`, no Drop |
| **ForetiasPrivKey32 (bindings)** | `bindings.rs:103` | **NO** | `Copy + Debug` |

**Keywords:** `zeroization`, `secret lifecycle`, `memzero`, `Zeroizing`, `bindings.rs Debug`.

---

## 4. P2P Layer Analysis

### 4.1 Implementation Status Matrix

| Component | Status | Verified? |
|---|---|---|
| DHT peer discovery (Kademlia) | Complete | No |
| GossipSub (probity + heartbeat) | Complete | No |
| libp2p swarm + Noise handshake | Complete | Manual only |
| Noise_XX TCP PtP | Complete | Manual only |
| Mutual attestation exchange | Complete | Integration test passes |
| Mutual attestation verification | Partial | Buried in chronomatter |
| Collision detection + dormancy | Complete | No |
| Calendar replication (RPC handlers) | Complete | No |
| Calendar replication (orchestration) | **Missing** | No |
| Probity gossip pipeline | Complete | No |
| Probity aggregation (U-shape) | Complete | No |
| Probity trust model (sig verification) | **STUBBED** | No |
| FROST epoch consensus | **STUBBED** | No |
| Liege channel | **Missing** | No |

### 4.2 Calendar Replication — Handlers Exist, Orchestration Missing

**[NEW finding from live exploration]**

The RPC handlers for mirror_request, mirror_accept, ship_batch, ship_ack, stream_tick, stream_ack, mirror_mutual, and mirror_reconcile are all implemented. However:

1. **No push stream:** No background task pushes new chronons to mirrors
2. **No reconciliation timer:** `handle_mirror_reconcile` exists but is never called by a background timer
3. **No persistent connections:** Current implementation uses per-call connections
4. **No disk persistence:** `MirrorStore.base_dir` exists but is never used
5. **No error recovery:** No retry logic on connection drop

**Keywords:** `CALENDAR_REPLICATION_SPEC.md`, `mirror.rs`, `handlers.rs:378-613`.

### 4.3 Noise_XX PtP — Functional but with Caveats

Two implementations exist:
1. `foretias-client/src/noise_ptp.rs` — Library-scoped, generates ephemeral keypair per request
2. `foretias-server/src/communerd/json_rpc_transport.rs` — Server-side, used for mutual attestation

Both create a NEW Noise handshake per request (no session reuse). This means:
- High overhead (3-way handshake per request)
- No cross-session replay protection at the protocol layer

**C11 deviation:** libp2p transport uses libp2p-noise, not C11 Noise_XX. Acknowledged in code but means the P2P layer doesn't use the same cryptographic primitives as the C11 core.

### 4.4 DHT Registration — Unauthenticated

**[REPEAT from §3.2]** DHT peer registration has no cryptographic binding. TBID → peer_id mapping is not signed. DHT poisoning is possible.

### 4.5 Mirror Records — No Integrity Check

**File:** `p2p/foretias-server/src/server/handlers.rs:465-482`

`handle_ship_ack` calls `mirror_store.insert_mirrored()` without running `integrity_check` on received records. A malicious peer can ship structurally valid but cryptographically forged calendar records.

---

## 5. Spec-to-Implementation Fidelity

### 5.1 Spec Staleness Assessment

| Spec | Staleness | Key Issues |
|---|---|---|
| `FORETIAS_0_OVERVIEW.md` | ~40% stale | Project structure doesn't match 3-crate architecture; references removed Python/Java |
| `FORETIAS_1_MVP_SPEC.md` | ~30% stale | Python/PyO3 references persist after SCOPE_REDUCTION |
| `FORETIAS_2_P2P_SPEC.md` | ~20% stale | Path references (`src/network/` → `communerd/`); libp2p version mismatch |
| `foretias-v1.md` | 100% stale | Describes Python prototype that no longer exists |
| `FORETIAS_CLI_SPEC.md` | Broken | Truncated to 47 lines; cuts off mid-sentence |
| `FORETIAS_ENCLAVE_SPEC.md` | **Missing** | Referenced by 3 specs; doesn't exist |

### 5.2 Critical Spec Contradictions

| Contradiction | Spec A | Spec B | Resolution |
|---|---|---|---|
| `tick_number` semantics | `foretias-v1.md`: nanoseconds since epoch | `questions.md`: sequential counter | Resolved in questions.md, not propagated to specs |
| Config format | `OVERVIEW`: TOML example | `MVP_SPEC`: JSON filename | Code uses TOML parsing with `.json` extension |
| Source of truth | `OVERVIEW §0.7`: Python is canonical | Reality: Python deprecated | Not updated |
| Serialized mode | `v1.md §2.1`: two modes (serialized/non-serialized) | Implementation: only serialized | Non-serialized mode not implemented |

### 5.3 Implementation Progress by Milestone

| Milestone | Spec | Plan | Code | Status |
|---|---|---|---|---|
| v0.1 (MVP) | ✅ | ✅ | ✅ | **Complete** |
| v0.2 (Direct P2P) | ✅ | ✅ | ✅ | **Complete** |
| v0.3 (libp2p Handshake) | ✅ | ✅ | ✅ | **Complete, tagged** |
| v0.4 (DHT Discovery) | ✅ | ❌ (backburnered) | ⚠️ (partial) | **Blocked — no plan** |
| v0.5 (Hardening) | ✅ | Backburnered | ❌ | **Backburnered** |
| v0.6 (Probity Gossip) | ✅ | Backburnered | ❌ | **Backburnered** |
| v0.7 (Collision) | ✅ | Backburnered | ❌ | **Backburnered** |
| v0.8 (Epoch Consensus) | ✅ | Backburnered | ❌ | **Backburnered** |

**Assessment:** v0.4 DHT discovery is the implementation frontier. v0.5-v0.8 are chained on v0.4 completion.

### 5.4 CLI Conformance

| Command | Spec (`CLI_SPECIFIED.md`) | `main.rs` | Match? |
|---|---|---|---|
| `serve` | ✅ | ✅ | ✅ |
| `stamp` | ✅ | ✅ | ✅ |
| `verify` | ✅ | ✅ | ✅ |
| `verify-with-proof` | `prove-verification` | `verify-with-proof` | ⚠️ Name mismatch |
| `inspect-attestations` | ✅ | ✅ | ✅ |
| `info` | ✅ | ❌ | **Missing** |

---

## 6. Testing & CI Infrastructure

### 6.1 The #1 Process Failure: No PR-Blocking CI

**[REPEAT from both prior reviews]** `.github/workflows/release.yml` is the only workflow. It runs on tag push. There is no PR-time testing, linting, audit, or cross-language equivalence check.

For a pre-public, security-sensitive project, this is the single largest gap.

**Recommended CI matrix:**
```
c-core:        Linux + clang/gcc; CMake + ctest; ASAN/UBSAN
rust-build:    cargo build --workspace + clippy -D warnings
rust-test:     cargo test --workspace
rust-miri:     cargo +nightly miri test (unsafe blocks)
cross-lang:    Cross-language equivalence tests
audit:         cargo audit + cargo deny
```

### 6.2 Missing Test Categories

| Test Type | Status | Priority |
|---|---|---|
| PQC cross-language round-trip | Missing | HIGH |
| Calendar tamper resistance (all fields) | Partial | HIGH |
| Concurrent stamping stress test | Missing | HIGH |
| JSON-RPC fuzz | Missing | HIGH |
| C-core fuzz (libfuzzer) | Missing | HIGH |
| PrivKey memzero verification | Missing | HIGH |
| Property tests (proptest) | Missing | MED |
| Multi-node P2P integration | Missing | HIGH |
| DHT eclipse attack test | Missing | MED |
| Probity gossip propagation test | Missing | HIGH |

### 6.3 Test Quality Smells

- `calendar_crash_recovery_corrupt_tmp` asserts buggy behavior (enshrines a known bug)
- Many tests use `unwrap()` on `random_bytes()` results (non-deterministic)
- Python tests rely on wall-clock `time.time()` (flaky on slow CI)

---

## 7. API & Library Usability

### 7.1 No Published Library Crate

Foretias ships a CLI binary. For embedding, there is no published library interface. `foretias-client` has a `Foretias` struct but:
- Not published to crates.io
- Sparse documentation
- No `cargo add foretias` equivalent

**Minimal embed use case should be:**
```rust
use foretias::TimeFamily;
let tf = TimeFamily::new()?;
let stamp = tf.stamp(b"my document hash")?;
let valid = tf.verify(b"my document hash", &stamp)?;
```

### 7.2 Foretis JSON Has No Schema

A `Foretis` JSON object has no schema document or OpenAPI definition. Consumers must read the Rust struct to know expected fields. For third-party verification, a stable, documented JSON schema is essential.

**Keywords:** `JSON schema`, `OpenAPI`, `schema stability`, `wire format versioning`.

### 7.3 No Wire Format Versioning

JSON-RPC methods have no version prefix (`stamp` not `foretias/v1/stamp`). If parameters or response fields change, there's no version negotiation.

### 7.4 verify() Return Value Is Ambiguous

**[REPEAT from CLAUDE review §5.3]** `valid: false` is returned for both "signature invalid" and "TBID not in calendar." Caller cannot distinguish forgery from "I don't know that calendar."

---

## 8. Efficiency & Production Readiness

### 8.1 Full Calendar Rewrite Per Stamp

**[REPEAT from CLAUDE review §7.1]** Each stamp triggers a full JSON serialization and write of the entire calendar. O(N) bytes per stamp. The `EncryptedJsonlCalendarStore` (append-only) exists but is not the default backend.

### 8.2 No Prometheus Metrics

`metrics.rs` has atomic counters but no Prometheus exporter, no Grafana dashboard, no structured logging for ELK/Splunk.

**Keywords:** `Prometheus`, `Grafana`, `OpenTelemetry`, `structured logging`.

### 8.3 Graceful Shutdown Not Implemented

`stop_daemon()` calls `handle.abort()` — cancels at next await point without cleanup. If the daemon was mid-save, the calendar may be partially written.

### 8.4 No Maximum Concurrent Connections

The JSON-RPC server accepts unlimited TCP connections. Under a connection flood, file descriptors are exhausted.

### 8.5 Hard-Coded Paths

- `/tmp/foretias-mirrors` (server/mod.rs:78,105) — world-readable, not configurable, cleared on reboot
- `time_being_reference_time` uses custom format `UE+{ns}ns` — not ISO 8601, not RFC 3339

---

## 9. Future Directions

### 9.1 Post-Quantum Readiness

PQC foundations are present (liboqs, SPHINCS+, Dilithium3, ML-KEM-768). Considerations:
- **NIST naming:** `Dilithium3` → `ML-DSA-65` (FIPS 204); `ML-KEM-768` → `ML-KEM-768` (FIPS 203)
- **Hybrid signatures:** Ed25519 + ML-DSA combined (IETF hybrid-sig drafts)
- **Algorithm sunset:** No roadmap for Ed25519 deprecation

**Keywords:** `FIPS 203`, `FIPS 204`, `FIPS 205`, `hybrid-sig`, `algorithm sunset`.

### 9.2 Embedded & Resource-Constrained

- `privkey.c` uses `calloc/free` — breaks no-alloc embedded targets
- liboqs (200+ source files) is not embeddable as-is
- PQC keygen unsuitable for constrained devices

**Keywords:** `no_alloc`, `embedded Rust`, `heapless`, `MIPS`, `ARM Cortex-M`.

### 9.3 Space Flight & Extreme Reliability

- Clock drift: chronon_ns should be configurable from external time sources
- Radiation hardening: key material in DRAM susceptible to SEUs
- Bandwidth: SPHINCS+ (7856 bytes) vs Ed25519 (64 bytes) per stamp
- Asynchronous operation: offline ticking + sync protocol underspecified

**Keywords:** `space flight`, `radiation hardening`, `ECC memory`, `SEU mitigation`, `offline operation`.

### 9.4 Trusted Execution Environments

The `CryptoServer` trait is the right abstraction for TEE backends:
- Intel SGX / AMD SEV: key operations inside enclave
- ARM TrustZone: mobile timestamping
- Apple Secure Enclave / Android StrongBox: hardware key lifecycle
- FIDO2/WebAuthn alignment for web use cases

**Keywords:** `Intel SGX`, `AMD SEV`, `ARM TrustZone`, `FIDO2`, `WebAuthn`.

### 9.5 Key Transparency & External Auditability

Open questions:
- Should calendars be published to a global Merkle log?
- Should epoch snapshots be anchored in public ledgers?
- How to prove to third-party auditors that a received calendar is authentic?

**Keywords:** `certificate transparency`, `RFC 6962`, `Merkle tree`, `public append-only log`.

### 9.6 Interoperability with Existing Standards

- **RFC 3161 (TSA):** Bridge adapter for enterprise PKI workflows
- **OpenTimestamps:** Foretias calendar anchored in Bitcoin/Ethereum
- **ANSI X9.95:** Financial timestamping compliance

**Keywords:** `RFC 3161`, `TSA`, `OpenTimestamps`, `ANSI X9.95`, `bridge adapter`.

### 9.7 WASM/Browser Verification

A WASM build of the verification path (no stamping, no P2P) would enable browser-based Foretis verification without any server infrastructure. libsodium and liboqs both have WASM builds.

**Keywords:** `wasm-pack`, `wasm-bindgen`, `libsodium-wasm`, `browser verification`.

---

## 10. Open Design Questions

### 10.1 From questions.md (Unresolved)

| # | Question | Impact |
|---|----------|--------|
| 1 | FROST stub approach | v0.8 epoch consensus blocked |
| 3 | Noise_XX stub | **HIGH** — PtP depends on Noise_XX |
| 5 | Legacy MD5/SHA-1 library | Low impact |
| 8 | Dormant TBID bug | **CRITICAL** — wrong dormant semantics |
| 10 | Config format vs filename | Confusing to users |

### 10.2 Implicit Open Questions

| Question | Why It Matters |
|---|---|
| When is `FORETIAS_ENCLAVE_SPEC.md` written? | v0.9+ backends cannot be specified without it |
| Is v0.4 the next priority? | v0.5-v0.8 are chained on v0.4 |
| Who owns `foretias-v1.md` update? | Python-oriented spec needs "legacy" marking |
| Chronon number: absolute time or sequence? | Cross-node ordering depends on resolution |
| Echo field: authenticated or advisory? | MITM can modify if advisory |
| Non-serialized mode: implement or document as removed? | High-throughput use cases depend on it |

---

## 11. Prioritized Improvements

### P0 — Before Any Public Announcement

| # | Finding | Severity | File:Line |
|---|---------|----------|-----------|
| 1 | Remove `Debug` derive from secret binding types | CRITICAL | `bindings.rs:103,215,306,413` |
| 2 | Fix probity signature verification (stubbed) | CRITICAL | `gossip_handler.rs:36-51` |
| 3 | Add cryptographic binding to DHT peer registration | HIGH | `communerd/mod.rs:41-51` |
| 4 | Fix cross-node verify (trusts remote calendar) | HIGH | `handlers.rs:191-237` |
| 5 | Fix FROST epoch handlers (return fabricated data) | HIGH | `handlers.rs:354-376` |
| 6 | Add PR-blocking CI workflow | CRITICAL | `.github/workflows/` |
| 7 | Fix C11 nonce overflow (`noise_xx.c`) | CRITICAL | `noise_xx.c:69,86` |
| 8 | Fix C11 HKDF stack overflow (`privkey.c`) | HIGH | `privkey.c:236-240` |
| 9 | Fix genesis verification bypass (short foretis) | HIGH | `tick.rs:308-317` |
| 10 | Validate PQC sig->len vs buffer | HIGH | `signing_sphincs.c:53` |

### P1 — Before First Public Tag

| # | Finding | Severity |
|---|---------|----------|
| 11 | Bound `keypairs` Vec (LRU eviction) | HIGH |
| 12 | Fix CAS race in `stamp()` / `daemon_tick()` | HIGH |
| 13 | Add JSON-RPC authentication | HIGH |
| 14 | Fix verify() ambiguity (forgery vs unknown TBID) | MED |
| 15 | Add calendar mirror integrity check | HIGH |
| 16 | Implement calendar replication orchestration | HIGH |
| 17 | Fix `noise_static_priv` zeroization | HIGH |
| 18 | Replace `block_on` with async handlers | HIGH |
| 19 | Lower `MAX_CONTENT_BYTES` (1 GiB → 16 MiB) | HIGH |
| 20 | Write the whitepaper | HIGH |

### P2 — Near Term

| # | Finding | Severity |
|---|---------|----------|
| 21 | Implement P-256 or remove from spec | HIGH |
| 22 | Implement ECDH for Noise XX | HIGH |
| 23 | Add wire format versioning | MED |
| 24 | Publish library crate to crates.io | MED |
| 25 | Add Prometheus metrics endpoint | MED |
| 26 | Implement graceful shutdown | MED |
| 27 | Property tests (proptest) | MED |
| 28 | C-core fuzz targets | HIGH |
| 29 | Multi-node P2P integration tests | HIGH |
| 30 | Fix config filename/format mismatch | MED |

### P3 — Strategic / Long Term

| # | Finding |
|---|---------|
| 31 | RFC 3161 bridge adapter |
| 32 | WASM/browser verification |
| 33 | Embedded/no-alloc C11 path |
| 34 | Hybrid classical/PQC signatures |
| 35 | Calendar public auditability (Merkle log) |
| 36 | TEE backend (SGX/TrustZone) |
| 37 | Zero-knowledge Foretis verification |
| 38 | Threshold timestamping (FROST on stamp path) |
| 39 | Content commitment schemes (KZG/Pedersen) |
| 40 | Federated calendar networks |

---

## 12. Augmentation from `pre-public-mvp-code-review.md` — Unaddressed Findings

This section maps findings from `pre-public-mvp-code-review.md` (Claude Opus 4.7) that were NOT addressed in `CLAUDE_PRE_P2P_PRODUCT_AND_CODE_REVIEW.md` and remain open.

### 12.1 Duplicated C Build Path (PQC Files Not in CMakeLists.txt) — [CRIT]

**[REPEAT — still unaddressed]**

`p2p/core/CMakeLists.txt:23-40` does NOT include `signing_sphincs.c`, `signing_dilithium.c`, or `kem_mlkem.c` in `FORETIAS_CORE_SOURCES`. The Rust `build.rs` compiles its own copies from `p2p/core-engine/src/core/algorithms.{c,h}` — a parallel C build that bypasses the strict C11 compile flags (`-Wpedantic -Werror -fstack-protector -fvisibility=hidden`).

**Consequence:** PQC code is compiled without verified-core invariants.

### 12.2 OpenSSL Dependency via Legacy Hash Files — [HIGH]

**[REPEAT — still unaddressed]**

`p2p/core/src/hash_legacy_insecure_md5.c` and `hash_legacy_insecure_sha1.c` link OpenSSL EVP. `FORETIAS_1_MVP_SPEC §4.3` explicitly forbids this. Expands supply-chain surface.

### 12.3 Python ↔ Rust Auto-Attestation Blob Incompatibility — [CRIT]

**[REPEAT — still unaddressed]**

Python: `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk` (no `stamps_per_tick`, no `aa_nonce`)
Rust: `tbid ‖ A.tick ‖ A.pk ‖ B.tick ‖ B.pk ‖ stamps_per_tick ‖ nonce`

Calendars cannot interoperate cross-language.

### 12.4 `#[serde(default)]` on `stamps_per_tick` — [MED]

**[REPEAT — still unaddressed]**

`tick.rs:27` — `#[serde(default)] pub stamps_per_tick: u64` contradicts `FORETIAS_3 §front-matter`: "no `#[serde(default)]` for new fields."

### 12.5 Undocumented Fields in Foretis and TickRecord — [MED]

**[REPEAT — still unaddressed]**

- `signature_algorithm: String` and `time_being_reference_time: String` — absent from `FORETIAS_1_MVP_SPEC §7.1`
- `external_attestations: Vec<ExternalAttestation>` — undocumented addition

### 12.6 Python Field Name Divergence — [HIGH]

**[REPEAT — still unaddressed]**

- Python `my_content_hash` vs Rust `content_hash`
- Python `TickRecord` has no `aa_nonce` field
- Python `save()` does NOT serialize `aa_nonce` or `signature_algorithm`

### 12.7 ECDH Functions Unimplemented — [HIGH]

**[REPEAT — still unaddressed]**

`software.rs:162-168` — `ecdh_ed25519` and `ecdh_p256` return `Unsupported`. Noise XX is unusable through the trait.

### 12.8 Calendar Slice Response Memory Explosion — [MED]

**[REPEAT — still unaddressed]**

`MAX_CALENDAR_SLICE_COUNT = 10_000` × SPHINCS+ 7,856 bytes × 2 ≈ ~157 MiB per JSON response.

### 12.9 Calendar Persistence Drift — [MED]

**[REPEAT — still unaddressed]**

- `EncryptedJsonlCalendarStore` is a v0.7 feature present in v0.1 codebase
- Python `save()` is non-atomic (no tempfile+rename)
- Python `save()` silently truncates `aa_nonce` and `signature_algorithm`

### 12.10 Additional C11 Findings — [MED]

**[REPEAT — still unaddressed]**

- `privkey.c:122-156` — `from_seed` does NOT zero caller's seed (despite doc-comment)
- `noise_xx.c` — early-return paths leave stack-local key material un-zeroed
- `signing_sphincs.c:39,17` — `OQS_MEM_cleanse` uses caller-supplied len
- Missing `OQS_init()` in PQC files
- `rng_mix.c` — misleading name; no mixing, reads `/dev/urandom` directly
- `merkle.c` — ACSL annotations exist but `proofs/` directory is empty

### 12.11 Chronomatter Dependency Injection Inconsistency — [HIGH]

**[REPEAT — still unaddressed]**

`Chronomatter::new` constructs its own `SoftwareCryptoServer`, ignoring injected crypto server. `from_calendar` path accepts injection. Inconsistent.

### 12.12 Hard-Coded `/tmp/foretias-mirrors` — [MED]

**[REPEAT — still unaddressed]**

`server/mod.rs:78,105` — mirror store path hard-coded to `/tmp/foretias-mirrors`.

### 12.13 `EncryptedJsonlCalendarStore::read_all` Unbounded Memory — [MED]

**[REPEAT — still unaddressed]**

`encrypted_jsonl.rs:107-112` — `read_to_string` over entire file.

### 12.14 Test Quality Issues — [MED]

**[REPEAT — still unaddressed]**

- `calendar_crash_recovery_corrupt_tmp` asserts buggy behavior
- Non-deterministic tests with `random_bytes()` + `unwrap()`
- Python tests rely on wall-clock time

### 12.15 Documentation Field Name Drift — [HIGH]

**[REPEAT — still unaddressed]**

- README claims `aa_nonce: bytes` on `TickRecord` (doesn't exist in Python)
- README documents `my_content_hash` (Rust uses `content_hash`)
- README lists `pyforetias` (actual: `foretias_p2p`)

### 12.16 Dependency & Supply-Chain Gaps — [MED]

**[REPEAT — still unaddressed]**

- No `cargo audit` in CI
- `liboqs` version pinning not verified
- `serde_cbor` unmaintained (replace with `ciborium`)

---

## 13. The Reassuring Architecture Summary

The core of Foretias works. The ephemeral key rotation mechanism is cryptographically sound. A calendar produced by this system, once a tick has advanced and the key has been algorithmically incapacitated, provides a verifiable guarantee that no one can produce a valid Foretis for that chronon's content without having been present during that specific window.

The nightmare scenario — where more than half the network is compromised — is survivable for past stamps because the cryptographic chain is self-contained and verifiable offline. For live and future stamps, the FROST epoch consensus mechanism (currently stubbed) provides the safety net: with threshold k > N/2, an attacker holding N/2 nodes cannot produce a valid signed epoch snapshot.

**The gap between the architecture's promise and the current implementation's guarantees is bridgeable with the P0-P2 items above. The vision is correct. The implementation needs hardening.**

---

## 14. File-By-File Risk Assessment (Triage Priority)

| File | Risk | Primary Concern |
|------|------|-----------------|
| `p2p/core-engine/src/core/bindings.rs` | CRITICAL | Debug derives on secret types |
| `p2p/core/src/noise_xx.c` | CRITICAL | Nonce overflow, stack buffers |
| `p2p/core/src/privkey.c` | CRITICAL | HKDF stack overflow, global state, dynamic alloc |
| `p2p/core-engine/src/crypto_server/software.rs` | HIGH | Algorithm dispatch, stubbed methods, zeroization |
| `p2p/core-engine/src/chronomatter/mod.rs` | HIGH | CAS race, unbounded keypairs, tick semantics |
| `p2p/foretias-server/src/server/handlers.rs` | HIGH | block_on, auth gap, cross-node verify, content limits |
| `p2p/foretias-server/src/probity/gossip_handler.rs` | HIGH | Signature verification stubbed |
| `p2p/foretias-server/src/communerd/mod.rs` | HIGH | DHT poisoning, OnceLock silent failure |
| `p2p/core-engine/src/foretias/tick.rs` | HIGH | Genesis bypass, serde defaults |
| `p2p/core-engine/src/crypto_server/signing_tbid.rs` | HIGH | Rust Vec not zeroized |
| `p2p/core-engine/src/collision/detector.rs` | MED | Count-based nonce window, no timestamp freshness |
| `p2p/core-engine/src/epoch/frost_bridge.rs` | HIGH | Stub signature accepted as valid |
| `p2p/core/src/nullifier.c` | HIGH | HMAC intermediate not zeroed |
| `p2p/core/src/signing_sphincs.c` | HIGH | sig->len not validated |
| `p2p/foretias-server/src/calendar/mirror.rs` | MED | No disk persistence, no integrity check on insert |
| `p2p/foretias-server/src/communerd/p2p/swarm.rs` | MED | No NAT traversal, no relay |
| `p2p/core/src/memzero.c` | MED | Custom memzero vs sodium_memzero |
| `p2p/core/src/signing_ed25519.c` | LOW | Seed re-derivation per sign |

---

## 15. In Summary — A Project Worth Completing

Foretias addresses a real and important problem: the forgeability of timestamps in digital systems. The core mechanism — per-chronon ephemeral key pairs with self-documenting chain transitions — is cryptographically elegant, and the fact that verification works offline without trusting any server is genuinely powerful.

The implementation is in a credible intermediate state. The foundational cryptographic layer (C11 core + Rust wrappers) works. The P2P layer is wired but unverified. The product presentation and library usability need significant work before public adoption.

**The most urgent improvements, ranked by impact on public readiness:**

1. Fix the Debug derive on secret binding types (immediate, low effort, high impact)
2. Fix probity signature verification (trust model is broken)
3. Add PR-blocking CI (the #1 process failure)
4. Fix DHT poisoning vulnerability (cryptographic binding for peer registration)
5. Fix FROST epoch handlers (return honest "not implemented" instead of fabricated data)
6. Write and publish the one-page pitch and whitepaper (adoption prerequisite)
7. Fix the three critical C11 safety issues (nonce overflow, HKDF stack overflow, dynamic allocation)
8. Add JSON-RPC authentication (security correctness)

Everything else is important but can be phased. The vision is clear, the architecture is sound, and the team discipline (spec-first, terminology enforcement, security-first coding guidelines) is excellent. The project deserves to reach its potential.

---

**Major: Pre-P2P Product & Code Review, Phase: Document Creation—complete**
**OpenCode 1.14.28; vllm/Qwen3.6-27B-AWQ-BF16-INT4**
