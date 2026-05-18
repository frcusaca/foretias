# Foretias — Overview, Invariants, and Roadmap

**Project:** Foretias (Free and Open-source Resilient Time Integrity Attestation Service)
**Primary Latin names:** Time Being = *Chronos fidelis* · Time Family = *Chronos adunatrix* · Chronomatter = *Chronos authenticus* · Calendar = *Chronos graphus*
**This document:** Cross-cutting design invariants, project structure, milestone roadmap, configuration, build, and development protocol.
**Companion documents:**
- `FORETIAS_MVP_SPEC.md` — v0.1 local-server stack (C11 verified core, Rust node layer, crypto-server abstraction with software backend, Foretias domain types in Rust, Python bindings).
- `FORETIAS_P2P_SPEC.md` — v0.2–v0.8 network layers (libp2p, probity, identity collision, epoch consensus, encrypted calendar persistence).
- `FORETIAS_ENCLAVE_SPEC.md` — v0.9+ custom-plugin backends. (Restricted distribution; do not reference its details outside that file.)

**Target:** AI Coding Specialist for execution. Comments to human reader in parenthesis `(@human ...)`.

---

## READING ORDER — MANDATORY FIRST STEPS

**Before writing any code, AI Coding Specialist MUST:**

1. **Read `foretias-v1.md`** in the project root (the Foretias v1 Product and Technical Specification). This defines Time Being, Chronon, Calendar, Chronomatter, Foretis, and TimeFamily.

2. **Read this document completely.** It defines the cross-cutting invariants and the milestone roadmap.

3. **Read `FORETIAS_MVP_SPEC.md` completely.** It defines the v0.1 local-server target.

4. **Read `FORETIAS_P2P_SPEC.md` completely.** It defines the gossip, DHT, probity, collision, epoch, and persistence layers (v0.2–v0.8).

5. **Scan the existing Python prototype** under `foretias/` for the current shape of:
   - Source code in `src/foretias/*.py`
   - All tests under `tests/`

6. **Confirm to the user** what you found and propose an execution plan before starting v0.1. The plan must respect the milestone ordering below.

---

## PART 0 — DESIGN INVARIANTS (non-negotiable)

These shape every subsequent decision. Do not deviate without asking the user.

### 0.1 Per-Run Entity Lifecycle

```
Process start = brand new identity
Process exit  = identity destroyed forever
Restart       = completely new, unrelated identity
```

The existing Python prototype's "dormant" state (load calendar from disk, verify only) is preserved for offline verification, but an **active** instance always generates a fresh identity. There is no persistent identity continuity across runs.

(human: this is how human made post-final additions to this document. design system identified an issue that a dormant time being needs to start with a tbid in it's constructor, which is contrary to the current design statement that time beings do not restart. The answer is that we need to update communication with calendric data by adding "tbid" to the query, so a new dormant time being with t's own TBID, it can can serve queries about one or many previous and other simultaneous TBID. This is appliation logic we need to update about the expanded spec)

### 0.2 Identity Collision = Dual Termination

If two running entities ever hold the same `tbid`/PeerID (near-impossible under good RNG, possible under VM-clone / RNG-failure), both terminate.

**"Terminate" means:** Becomes dormant--answers requests but do not stamp or create new chronons. The dormant agent always will flag in it's response that the responding time agent is dormant.

Detection uses signed heartbeats (see `FORETIAS_P2P_SPEC.md` Part 10). Escalation to the liege is stubbed as `mail_liege("help")` — liege infrastructure comes from FOSITAS application logic (stable-marriage matching between peers).

### 0.3 What Persists, What Does Not

```
PERSISTED (disk):
  - Calendar entries
    Existing Python prototype: plaintext JSON file (one file per time being).
    New C11/Rust/PyO3 implementation, target format (v0.7+):
      single encrypted JSONL file, each line = calendar block,
      encrypted under own public key.
  - Foretis records produced by this instance (same file or sibling)
  - Relics: Non-authoritative DHT routing tables for next time being to leverage.

NEVER PERSISTED (memory only, destroyed with process):
  - Private keys (held inside a custom plugin when one is loaded)
  - Peer records, probity scores, bonded peer lists
  - Noise session state
  - FROST shares
  - Vivaldi coordinates
  - Epoch snapshots (re-fetched from peers on restart)
```

The existing Python prototype currently stores the calendar in plaintext JSON, and the new C11/Rust/PyO3 v0.1 MVP keeps that same plaintext format unchanged (it is the source of truth for Foretias semantics — see §0.7). The encrypted JSONL format is a target of the new implementation only, scheduled for v0.7 (see `FORETIAS_P2P_SPEC.md` Part 12 and Part 14 below).

### 0.4 Enforcement Is Social, Not Cryptographic

The P2P layer **measures, gossips, and reaches epoch consensus**. It never forces the application to act on consensus.

Consequences are emergent:

```
Peer A lies → network gossips, A's probity drops
Peer B ignores A's drop, still serves A → B is observed doing so
Peer B's behavior observed by peers C, D, E
  → peers C, D, E reduce service to B to preserve own standing
  → B's probity drops because C, D and E rates B lower for not following the honor system.

No central enforcement. The hierarchy of honor self-organizes.
```

(@human — this is the insight that makes the whole system work. AI Coding Specialist: the P2P layer's job is measure → gossip → aggregate. The separate individual application instances decides what to do with the aggregated signal. A running application that ignores the honor system as reported by the p2p consensus is obviously observable, which is itself a probity signal.)

### 0.5 Probity Is One Number; Attributes Are Opaque

Every peer computes a **single probity score** (f32, range `[-100.0, +100.0]`) for every other peer it knows about. This score is the U-shape time-weighted aggregate of signed probity reports from the network.

Reports carry opaque attribute names. The P2P layer does not know what `"correctness"` or `"propriety"` or `"answers_hard_questions"` means. The Foretias application layer defines attribute semantics.

Report tuple the P2P layer carries:

```rust
struct ProbityReport {
    subject:    PeerId,       // who is being reported on
    reporter:   PeerId,       // who is reporting
    attribute:  String,       // opaque name, e.g. "correctness"
    value:      f32,          // signed magnitude, e.g. -1.0
    timestamp:  u64,          // observation time (nanoseconds)
    signature:  [u8; 64],     // reporter signs the above
}
```

Aggregation:

```
score(subject) = Σ over all measurements:
    u_shape_weight(now - timestamp) × 
    reporter_credibility(reporter) × 
    value
```

`u_shape_weight` — high for very recent, dips for medium-age, rises again for ancient-but-uncontradicted.

(@human — details of the U-shape parameters come from an old FOSITAS design; they are configurable. We just need the framework. Reporter credibility = reporter's own probity score — this creates the recursive structure where low-probity reporters have low report weight.)

### 0.6 Curve Support — Ed25519 and P-256

Every Foretias node ships software implementations of both Ed25519 and P-256. Software is always the fallback. Custom plugins may use either curve depending on what the plugin supports.

Peers can disable and enable cryptographic protocols a part of future software upgrade. By default both are supported so arbitrary peers can always handshake.

Existing Python prototype uses Ed25519 throughout. This is preserved. P-256 support is added to the crypto-server backend but does not affect the Python application API until a later tag.

### 0.7 Existing Python Code Is the Source of Truth for Foretias Semantics

The existing Python prototype defines:

- **Chronon semantics** (forward_foretis, backward_foretis, auto-attestation)
- **Foretis format** (chronon_number, content_hash, signature, tbid, echo, tbn)
- **Calendar structure** (append-only, integrity check via `_verify_pair`)
- **TimeFamily orchestration** (Chronomatter + Calendar)
- **Verification flow**

**AI Coding Specialist must preserve all of these semantics exactly.** Do not redesign them. The Rust/C11 work in `FORETIAS_MVP_SPEC.md` is infrastructure — it replaces the crypto primitive layer and adds the networking/P2P layers, but the Foretias chronon/foretis/calendar semantics are unchanged.

---

## PART 1 — PROJECT STRUCTURE

Integrate the new code into the existing `foretias/` directory. Final structure:

```
foretias/                                  EXISTING — Python package root
├── __init__.py                           existing, modify to export new API
├── chronomatter.py                       existing
├── calendar.py                           existing
├── timefamily.py                         existing — becomes an orchestrator
├── _timebeing.py                         existing — pure functional
├── crypto.py                             existing — becomes thin wrapper over crypto-server
├── models.py                             existing
├── config.py                             existing
├── cli.py                                existing
│
├── p2p/                                  NEW — Rust/C11 library imported via PyO3
│   ├── core/                             C11 verified cryptographic primitives
│   │   ├── include/
│   │   │   └── foretias_core.h            single public header
│   │   ├── src/
│   │   │   ├── platform.h                internal C11 baseline + static asserts
│   │   │   ├── identity_ed25519.c        Ed25519 keypair, peer ID
│   │   │   ├── identity_p256.c           P-256 keypair, peer ID
│   │   │   ├── signing_ed25519.c         Ed25519 sign/verify
│   │   │   ├── signing_p256.c            P-256 ECDSA sign/verify
│   │   │   ├── hash_sha256.c             SHA-256 — primary secure hash
│   │   │   ├── hash_blake3.c             BLAKE3 — fast hash for merkle/DHT
│   │   │   ├── hash_legacy_insecure_md5.c   MD5 — noncrypto legacy only
│   │   │   ├── hash_legacy_insecure_sha1.c  SHA-1 — noncrypto legacy only
│   │   │   ├── noise_xx.c                Noise_XX state machine (dual curve)
│   │   │   ├── merkle.c                  sparse merkle tree
│   │   │   ├── frost_ed25519.c           FROST primitives over Ed25519
│   │   │   ├── nullifier.c               nullifier derivation
│   │   │   ├── rng_mix.c                 HKDF mixing of multiple entropy sources
│   │   │   └── memzero.c                 volatile secure zero
│   │   ├── tests/
│   │   │   └── test_*.c                  one per source
│   │   ├── CMakeLists.txt
│   │   └── README.md
│   │
│   ├── node/                             Rust node — everything above C11
│   │   ├── Cargo.toml
│   │   ├── build.rs                      compiles C11, runs bindgen
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── config.rs
│   │       ├── runtime.rs                shared tokio runtime
│   │       │
│   │       ├── core/                     safe Rust wrappers over C11 FFI
│   │       │   ├── mod.rs
│   │       │   ├── bindings.rs
│   │       │   ├── identity.rs
│   │       │   ├── signing.rs
│   │       │   ├── hash.rs
│   │       │   ├── noise.rs
│   │       │   ├── merkle.rs
│   │       │   ├── frost.rs
│   │       │   └── nullifier.rs
│   │       │
│   │       ├── crypto_server/            crypto-server abstraction + backends
│   │       │   ├── mod.rs                CryptoServer trait
│   │       │   ├── software.rs           pure-software backend (C11 + OS RNG)
│   │       │   ├── custom_plugin/        custom-plugin backends (v0.9+)
│   │       │   │                          — see FORETIAS_ENCLAVE_SPEC.md
│   │       │   └── capabilities.rs
│   │       │
│   │       ├── foretias/                  Foretias domain types — bridge to Python
│   │       │   ├── mod.rs
│   │       │   ├── tick.rs               ChrononRecord, stamp, tick, verify
│   │       │   ├── foretis.rs             Foretis type
│   │       │   ├── calendar.rs           Calendar (plaintext v0.1; encrypted v0.7+)
│   │       │   └── timefamily.rs         TimeFamily orchestrator (Rust side)
│   │       │
│   │       ├── network/                  libp2p layer
│   │       │   ├── mod.rs
│   │       │   ├── swarm.rs
│   │       │   ├── transport.rs
│   │       │   ├── dht.rs
│   │       │   ├── gossip.rs
│   │       │   ├── discovery.rs
│   │       │   └── presence.rs
│   │       │
│   │       ├── probity/                  probity gossip + aggregation
│   │       │   ├── mod.rs
│   │       │   ├── report.rs             ProbityReport type
│   │       │   ├── aggregator.rs         U-shape time-weighted aggregation
│   │       │   ├── store.rs              per-peer probity state
│   │       │   ├── gossip_handler.rs     gossipsub integration
│   │       │   └── vivaldi.rs            Vivaldi network coordinates
│   │       │
│   │       ├── bonding/
│   │       │   ├── mod.rs
│   │       │   └── manager.rs            bonded peer long-lived connections
│   │       │
│   │       ├── collision/
│   │       │   ├── mod.rs
│   │       │   └── detector.rs           identity collision detection
│   │       │
│   │       ├── epoch/
│   │       │   ├── mod.rs
│   │       │   ├── scheduler.rs          epoch timing
│   │       │   ├── committee.rs          FROST-signed epoch snapshot committees
│   │       │   └── snapshot.rs           signed epoch digests
│   │       │
│   │       ├── liege/
│   │       │   ├── mod.rs
│   │       │   └── stub.rs               liege channel stub (mail_liege)
│   │       │
│   │       ├── calendar_store/
│   │       │   ├── mod.rs
│   │       │   ├── policy.rs             Everything | MyOwn | MyOwnPlusLru
│   │       │   ├── lru.rs                bin-based LRU (my_own|used|unused)
│   │       │   └── encrypted_jsonl.rs    v0.7+ persistence format
│   │       │
│   │       └── api/                      public API surfaces
│   │           ├── mod.rs
│   │           ├── types.rs              NodeInfo, PeerInfo, NodeEvent
│   │           ├── node.rs               top-level Node struct
│   │           ├── python.rs             PyO3 bindings
│   │           └── c_ffi.rs              C ABI for Go / others
│   │
│   └── bindings/
│       ├── python/
│       │   ├── pyproject.toml
│       │   └── foretias_p2p/
│       │       └── __init__.py           re-exports from _foretias_p2p native
│       └── go/
│           ├── go.mod
│           └── foretias.go
│
├── tools/                                admin/dev tooling
│   ├── keygen.py
│   ├── inspect_peer.py
│   └── monitor.py
│
├── config/
│   ├── default.toml                      node defaults
│   └── local_example.toml
│
├── tests/                                existing Python tests — extended
│   ├── test_functional.py                existing
│   ├── test_calendar.py                  existing
│   ├── test_timebeing.py                 existing
│   ├── test_persistence.py               existing
│   ├── test_stress.py                    existing
│   └── test_mvp_remote.py                NEW — MVP end-to-end test
│
├── Makefile
├── foretias-v1.md                         existing — Foretias v1 spec
├── FORETIAS_OVERVIEW.md                   THIS DOCUMENT
├── FORETIAS_MVP_SPEC.md                   v0.1 local-server stack
├── FORETIAS_P2P_SPEC.md                   v0.2–v0.8 network layers
└── README.md
```

---

## PART 2 — EARLY MVP TARGET

**Before any of the heavy P2P machinery, you must hit this MVP:**

> A user on machine A can reach out to a running Foretias Time Family on machine B
> (or same machine, different process) and ask for a stamp, receive a Foretis,
> send content + Foretis to a third process C and have C verify it — all three
> using the Foretias library.

### MVP Acceptance Criteria

1. **Time Family Server Process.** Runs a Foretias `TimeFamily` in a standalone process, listens on a local TCP ports, answering to JSON-RPC 2.0 requests:
   - `stamp(content_bytes) -> StampResponse`
   - `verify(content_bytes, Foretis) -> VerifyResponse`
   - `get_calendar_slice(cal_tbid, cal_tick_start, count) -> [ChrononRecord,ChrononRecord]` # Recall this can return 1 or two chronon records depending on whether cal_tick_start's chronon duration has elapsed or not.

2. ** Rust UI ** 
   - CLI server: `foretias serve` reading configurations from a configurable path such as `/.config/foretias/foretias.settings.json` port, version, various default configurations will be written to this configuration file.
   - CLI stamp for sending content or a file, and writing foretis to screen or disk.
   - CLI verify for sending content or a file, along with a foretis file, print out verification result.
   - This instance of time being is alive for only one query.
   - This instance of time being has to be given the time family server contact information, this is pre-p2p-dht discovery.

3. **Test:** 
   - An automated integration test spawns 3 processes (server, CLI stamp, CLI verify) and confirms end-to-end success.
   - More extensive tests in the unit tests can be replicated here as well with all 3 programs running and talking on the same test machine. These can be called "Integration sanity tests."

4. ** Python UI ** 
   - Replicate the Rust UI behavior using a python server calling rust foretias library through PyO3

6. ** Python UI tests **
   - Replicate step 3 testing to run and pass on python application as well,

### MVP Scope Boundaries

- No libp2p, no DHT, no gossip. Just direct TCP.
- No custom plugin. Software-only CryptoServer using the existing Python `cryptography` package.
- No probity. No honor system. No epoch consensus.
- No identity collision detection.
- Calendar persistence: plaintext JSON (inherited from the existing Python prototype, unchanged for v0.1).

(@human — this MVP is explicitly a stepping stone. It proves the decomposition of "time family owns stamping" vs "client wants stamps" works end-to-end before we invest in the P2P machinery. Once this tag lands, all subsequent work adds network layers underneath this same application surface.)

### MVP Milestone Tags

```
v0.1.1 — Review existing code; verify all existing Python tests pass
v0.1.2 — Extract core functional behavior and implement in C11
v0.1.3 — Implemnt time beings and time family persistence/mutable logic in rust.
v0.1.4 — Implement time famil server (TimeFamilyServer) application with CLI in rust
v0.1.5 — Test the rust cli
v0.1.6 — implement the save CLI in python calling rust through PyO3 
v0.1.7 — Test the python cli
v0.1.8 — TAG: v0.1-local-server-mvp
```

(@human — this gives you a working demo at v0.1. Once you can stamp remotely and verify across processes, we have a baseline to preserve through every subsequent refactoring. Every later tag must continue to pass the v0.1 integration test.)

Detailed implementation guidance for the v0.1 stack lives in `FORETIAS_MVP_SPEC.md`.

---

## PART 3 — DESIGN INVARIANT MAPPING TO EXISTING PYTHON

The existing Python prototype conflates several concerns that we need to separate for the P2P / custom-plugin work. Map:

| Existing concept           | New home                                      | Notes                                        |
|----------------------------|-----------------------------------------------|----------------------------------------------|
| `TimeFamily`               | Stays in `foretias/timefamily.py` (orchestrator) | API-compatible; delegates to crypto-server    |
| `Chronomatter`             | Stays — becomes a CryptoServer consumer        | Key lifecycle becomes CryptoServer ops        |
| `Calendar`                 | Stays — gains encrypted-JSONL persistence option | Old plaintext JSON supported for back-compat |
| `_timebeing.py` pure functions | Stays — but calls crypto-server for ops    | Pure functional signatures preserved          |
| `crypto.py`                | Becomes a thin wrapper over `foretias_p2p`      | Directly imports the Rust crypto-server       |
| `models.py` (ChrononRecord, Foretis) | Stays                                     | Binary layout matches Rust-side types         |
| Persistence                | Encrypted JSONL in `calendar_store/` (v0.7+)    | Old path kept as a compat shim                |
| Identity (`tbid`)          | Generated by CryptoServer on startup           | No longer accepts user-supplied tbid in prod  |

(@human — the existing Python API stays the same from the user's perspective:
`tbf = TimeFamily(); foretis = tbf.stamp("hello"); tbf.verify("hello", foretis)`
continues to work throughout every tag.)

---

## PART 14 — MILESTONE TAGS

**Every tag must keep every previous tag's tests passing.** The v0.1 MVP
integration test is the durable acceptance bar — every subsequent tag
re-runs it.

### v0.0 — Baseline Verification

- Task: verify all existing Python tests pass as-is.
- No new code written.
- Tag: `v0.0-baseline`

### v0.1 — Local Server MVP (C11 + Rust + Python via PyO3)

This milestone hits the MVP target defined in Part 2: three processes
(server, stamp client, verify client) communicate over local TCP JSON-RPC.
Implementation work spans all three languages — C11 verified core, Rust
crate, Python CLI through PyO3 — folded into one milestone so the v0.1
demo exercises the whole software-only stack end-to-end. Old standalone
tags for the C11 core (v0.2), CryptoServer (v0.3), Rust Foretias types
(v0.4), and PyO3 bindings (v0.11) are absorbed here.

Sub-milestones (see Part 2 for detailed acceptance criteria; full implementation guidance in `FORETIAS_MVP_SPEC.md`):

- **v0.1.1** — Review existing Python code; confirm all existing Python tests pass as-is. No new code.
- **v0.1.2** — Extract core functional behavior into C11 under `foretias/p2p/core/`. Skeleton with CMakeLists.txt, then Ed25519 + P-256 identity/signing, SHA-256 + BLAKE3 hashing, Noise_XX (both curves), Merkle, FROST, nullifier, RNG, memzero, legacy MD5/SHA-1 with warning banners. Every test in `ctest` passes.
- **v0.1.3** — Implement time-being and time-family persistence and mutable logic in Rust under `foretias/p2p/node/src/`: `Cargo.toml` + `build.rs` compile the C11 core; safe Rust wrappers with `Drop` impls; `SoftwareCryptoServer` implementing the `CryptoServer` trait; Rust `ChrononRecord` / `Foretis` / `Calendar` / `TimeFamily` mirroring Python semantics. Cross-verification test: Python stamp → Rust verify and Rust stamp → Python verify.
- **v0.1.4** — Implement `TimeFamilyServer` application with Rust CLI: `foretias serve` (reads `~/.config/foretias/foretias.settings.json`), `foretias stamp`, `foretias verify`. JSON-RPC 2.0 surface: `stamp(content) -> StampResponse`, `verify(content, Foretis) -> VerifyResponse`, `get_calendar_slice(cal_tbid, cal_tick_start, count) -> [ChrononRecord, ChrononRecord]`. Each client invocation is a fresh, single-query time being; client receives time-family contact info up front (pre-DHT). Responses include a `dormant` flag per Part 0.2.
- **v0.1.5** — Test the Rust CLI: 3-process integration test (server + stamp client + verify client). Replicate the unit-test surface as integration sanity tests on the same machine.
- **v0.1.6** — Implement the Python CLI mirroring the Rust one, calling the Rust `foretias_p2p` crate through PyO3 (via maturin). `FORETIAS_USE_NATIVE=1` switches the existing `foretias.TimeFamily` Python API to delegate to the native crate.
- **v0.1.7** — Test the Python CLI: replicate v0.1.5's 3-process integration test through the Python CLI; run the existing Python unit-test suite under both pure-Python and native modes.
- **v0.1.8** — Tag `v0.1-local-server-mvp`. User can demo: stamp content from process A through a server on process B, then verify the artifact in process C — using either the Rust or Python CLI.

**(@human — this is the first natural stopping point. The full software-only
stack (C11 → Rust → Python via PyO3) is wired together; the only thing
missing from production is the P2P / probity / epoch layers. Every
subsequent tag must keep the v0.1 integration test passing.)**

### v0.2 — libp2p Two-Node Networking (No DHT Yet)

- **v0.2.1** — `src/network/swarm.rs` builds a swarm using `CryptoServer` for identity
- **v0.2.2** — Two-node integration test: dial, Noise handshake, direct stream send
- **v0.2.3** — `identify::Behaviour` and `ping::Behaviour` working
- **v0.2** — Tag `v0.2-libp2p-handshake`.

### v0.3 — DHT Peer Discovery

- **v0.3.1** — Kademlia DHT configured with private namespace
- **v0.3.2** — Bootstrap from static list of seed peers
- **v0.3.3** — Peer discovery via DHT query
- **v0.3.4** — Three-node integration test: node A announces, node C finds A via node B
- **v0.3** — Tag `v0.3-dht-discovery`.

### v0.4 — GossipSub + Probity Gossip + U-Shape Aggregation

- **v0.4.1** — GossipSub with private topic IDs
- **v0.4.2** — `ProbityReport` type, canonical serialization, signing
- **v0.4.3** — U-shape aggregator with default config
- **v0.4.4** — Probity gossip handler; two-pass credibility
- **v0.4.5** — Integration test: node A reports on B, node C sees A's report via gossip
- **v0.4** — Tag `v0.4-probity-gossip`.

### v0.5 — Collision Detection + Liege Stub

- **v0.5.1** — Signed heartbeats on gossipsub topic
- **v0.5.2** — `CollisionDetector` validates heartbeats with own pub key
- **v0.5.3** — Liege channel stub emits events to application
- **v0.5.4** — On confirmed collision: stop P2P, continue locally; surface dormant state in JSON-RPC responses (per Part 0.2)
- **v0.5.5** — Test: two processes intentionally colliding terminate correctly
- **v0.5** — Tag `v0.5-collision-detection`.

### v0.6 — Epoch Consensus Framework

- **v0.6.1** — Epoch scheduler (phases: gossip / freeze / consensus / publish)
- **v0.6.2** — Committee selector (top-probity for MVP)
- **v0.6.3** — FROST signing of epoch snapshots using `frost-ed25519` crate
- **v0.6.4** — Snapshot adoption on receive
- **v0.6.5** — Special election triggers (collision, threshold of malfeasance)
- **v0.6** — Tag `v0.6-epoch-consensus`.

### v0.7 — Calendar Persistence Upgrade

- **v0.7.1** — `EncryptedJsonlCalendarStore` implementation (replaces v0.1's plaintext JSON)
- **v0.7.2** — Bin-based LRU
- **v0.7.3** — Migration path from plaintext JSON (read old, write new)
- **v0.7.4** — Storage policy switches (Everything | MyOwn | MyOwnPlusLru)
- **v0.7** — Tag `v0.7-encrypted-calendar`.

### v0.8 — End-to-End P2P Demo

- **v0.8.1** — Three-node P2P network: stamp/verify across all, probity gossip visible
- **v0.8.2** — Multi-tbid `get_calendar_slice` queries against dormant verifier nodes (per Part 0.1)
- **v0.8.3** — All existing Python unit tests pass under both pure-Python and native modes against the live P2P stack
- **v0.8** — Tag `v0.8-p2p-demo`.

---

### v0.9+ — Custom Plugin Backends

At `v0.8-p2p-demo` the entire software-only stack is complete. Subsequent
milestones replace the software CryptoServer with custom plugin backends
that protect private key material through alternative means.

**Milestone schedule, decision points, and implementation details for v0.9 and
beyond live in `FORETIAS_ENCLAVE_SPEC.md`.** That document is intentionally
maintained separately and should be consulted only when work on v0.9+ begins.

---

## PART 15 — CONFIGURATION

### 15.1 `config/default.toml`

```toml
[identity]
# Per-run identity — fresh key generated each process start.
# This path stores ONLY the encrypted calendar, not the key.
calendar_path = "~/.foretias/calendars/"

[curve]
# "ed25519" or "p256". Defaults to ed25519 for historical Foretias compat.
preferred = "ed25519"
# Whether to advertise both curves or just preferred.
advertise_both = true

[network]
listen_addrs    = ["/ip4/0.0.0.0/tcp/4001", "/ip4/0.0.0.0/udp/4001/quic-v1"]
bootstrap_peers = []
enable_relay    = true
enable_autonat  = true
enable_upnp     = true
max_connections = 50

[dht]
mode              = "client"          # "client" | "server"
app_namespace     = "/foretias/v1"
namespace_secret  = "CHANGE_ME_BEFORE_DEPLOY"

[gossip]
mesh_n           = 6
mesh_n_low       = 4
mesh_n_high      = 12
heartbeat_secs   = 1

[probity]
hot_window_ns    = 3_600_000_000_000
valley_center_ns = 604_800_000_000_000
ancient_start_ns = 2_592_000_000_000_000
valley_floor     = 0.1
max_weight       = 1.0
recompute_secs   = 60
max_reports_per_subject = 256

[collision]
heartbeat_secs       = 30
liege_response_timeout_secs = 30

[epoch]
duration_secs           = 3600
freeze_offset_secs      = 300
consensus_offset_secs   = 120
publish_offset_secs     = 60
committee_size          = 7
threshold_k             = 5

[calendar_storage]
# "everything" | "my_own" | "my_own_plus_lru"
policy         = "my_own_plus_lru"
max_bytes_total = 1_073_741_824     # 1 GiB — for MyOwnPlusLru

[crypto_server]
# "software" | "auto" | "custom"
# "auto" prefers a custom plugin if one is available, else falls back to software.
backend = "auto"
```

### 15.2 `src/config.rs`

Standard serde deserialization. Each section maps to a struct.

---

## PART 16 — MAKEFILE

```makefile
.PHONY: all mvp c11-core rust python go test clean

# ── MVP ────────────────────────────────────────────────
# First target — exercises the v0.1 local-server-mvp stack
# (C11 core + Rust crate + Python via PyO3). Each layer must
# be built before the integration test runs end-to-end.
mvp: c11-core rust python-native
	python -m pytest tests/test_mvp_remote.py -v

# ── C11 core ───────────────────────────────────────────
c11-core:
	cd foretias/p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
	cd foretias/p2p/core/build && ctest --output-on-failure

# ── Rust node ──────────────────────────────────────────
rust: c11-core
	cd foretias/p2p/node && cargo build --release

rust-test: c11-core
	cd foretias/p2p/node && cargo test --release

# ── Python native bindings ─────────────────────────────
python-native:
	cd foretias/p2p/bindings/python && maturin develop --release

# ── Full integration test ──────────────────────────────
test: c11-core rust-test
	cd foretias && python -m pytest tests/ -v
	cd foretias && FORETIAS_USE_NATIVE=1 python -m pytest tests/ -v

# ── Clean ──────────────────────────────────────────────
clean:
	rm -rf foretias/p2p/core/build foretias/p2p/node/target
```

---

## PART 17 — DEVELOPMENT PROTOCOL FOR CLAUDE CODE

1. **Read first.** Before touching any file, read `foretias-v1.md` and the four spec files (`FORETIAS_OVERVIEW.md`, `FORETIAS_MVP_SPEC.md`, `FORETIAS_P2P_SPEC.md`, and — only when v0.9+ work is authorized — `FORETIAS_ENCLAVE_SPEC.md`) end to end. Scan the existing `foretias/` directory to understand current code.

2. **Propose before acting.** Produce a short plan that identifies:
   - Current state of the repo
   - Which milestone you'll attempt first (must be `v0.0-baseline` or `v0.1.1`)
   - Any files you plan to create or modify
   - Questions if anything is unclear
   
   Wait for user approval.

3. **One milestone at a time.** Do not attempt to execute multiple major milestones in one session. Stop at each tag for review.

4. **Tests are the contract.** Before tagging, run the full test suite. Every tag's acceptance criteria is: every previous tag's tests still pass.

5. **Preserve Python API.** Users calling `tbf = TimeFamily(); tbf.stamp("...")` must see identical behavior from v0.1 through v0.9 and beyond. Changes happen underneath.

6. **Ask about human decision points.** The `@human` comments in this doc flag decisions that need user input. Do not guess at those; ask.

7. **Use the existing Foretias terminology.** TimeBeing, TimeFamily, Chronomatter, Calendar, Foretis, chronon. Do not invent new vocabulary.

8. **Feature flags for custom plugins.** All custom-plugin code must be behind cargo features and compile-gated. The software backend always compiles.

9. **Security hygiene.**
   - Every `unsafe` block has a justifying comment.
   - Every key-material `Drop` zeroes memory.
   - Every C11 function that touches keys calls `foretias_memzero` before error return.
   - Legacy hashes only exposed via `legacy_insecure_*` names.

10. **No premature P2P.** Do not implement `FORETIAS_P2P_SPEC.md` content before the MVP milestones (v0.1) are stable. The MVP is the priority.

---

## APPENDIX A — OPEN QUESTIONS DEFERRED

Things intentionally not designed yet, to be revisited in later spec versions:

- **FOSITAS nobility level assignment.** How a peer moves from level N to N+1. Committee-based promotion as per `hierarchical_identity_v2.docx`; wire into epoch consensus later.
- **Challenge protocols (Type A, Type B).** Formal protocol design for probity challenges.
- **Retribution mechanics.** How bad reports consume prover resources.
- **Stable-marriage liege matching.** Full liege/vassal bonding protocol.
- **Cross-calendar verification.** P2P verification of Foretises issued by third parties.
- **Succession protocols.** Orderly identity handoff when a peer wants to retire gracefully instead of just terminating.
- **Application-specific probity attributes.** The Foretias application layer defines these; the P2P layer is ready whenever the application wants to publish them.

---

## APPENDIX B — REQUIRED TOOLING ON BUILD MACHINE

```
rust stable (1.75+)
clang 14+
gcc (for MIPS cross-compile later)
libsodium-dev
libmbedtls-dev (for P-256)
cmake 3.16+
maturin (pip install maturin)
python 3.9+
go 1.21+ (optional, for Go bindings)

rustup targets for future cross-compile:
  rustup target add aarch64-unknown-linux-gnu
  rustup target add mipsel-unknown-linux-gnu
  rustup target add thumbv7em-none-eabihf
  rustup target add x86_64-apple-darwin
  rustup target add aarch64-apple-darwin
```

---

# END OF OVERVIEW

(@human — hand this plus `foretias-v1.md`, `FORETIAS_MVP_SPEC.md`, and `FORETIAS_P2P_SPEC.md` to AI Coding Specialist and watch it execute. It will stop at each tag for review; the first real demo milestone is v0.1 where you can reach out to a running time family over the network and stamp content. Everything below that tag is invisible infrastructure work that the user-facing Python API keeps abstract. `FORETIAS_ENCLAVE_SPEC.md` is held back until v0.8 lands.)
