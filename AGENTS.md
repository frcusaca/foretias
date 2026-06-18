# Notes to AI coding agents:

Please read the entire document word for word do not skip any thing.
  - This is the code repository for foretias
  - The active and working specifications are in the `foretias/specs` directory.
  - The architectures, including project language and dependencies are stated here.
  - Sometimes new specifications come from user, and they may be placed in a new file in foretias/specs/
  - Working specs may be updated while coding.
  - When it is detected that specs diverge while coding, we maintain the reliance on `foretias/specs`.

---

## TERMINOLOGY SHORTHANDS (Mandatory)

| Shorthand | Meaning | NEVER use |
|-----------|---------|-----------|
| **PtP** | Point-to-point (direct channel between two parties) | "point-to-point", "point to point", "point2point", "PTP" (capital T means something else) |
| **P2P** | Peer-to-peer (network of distributed nodes) | "peer-to-peer", "peer to peer", "peer2peer" |

**In prose (English):** The shorthand must be precisely capitalized as `PtP` and `P2P`. No other capitalization is permitted.

**In code identifiers:** The three-character sequence must appear in either exact-case or all-lowercase. Acceptable examples: `PtPClient`, `P2PClient`, `make_ptp_connection`, `make_p2p_connection`, `ptp_transport`, `p2p_mesh`, `ptp_client`. Forbidden examples: `PtpClient`, `P2pClient`, `PTPCLIENT`, `P2PCLIENT` (mixed-case or all-caps variants are not allowed — only `ptp`/`PtP` for the three chars, only `p2p`/`P2P` for the three chars).

These shorthands are enforced across all specs, plans, code, comments, and documentation. No exceptions.

---

## SPECS DIRECTORY CONVENTIONS (`foretias/specs/`)

The `specs/` directory contains two kinds of files that work together:

### SPEC.md Files — "What is needed"

A `*_SPEC.md` file provides a **detailed description of functionality or internal design**. It specifies requirements, behavior, data structures, protocols, and constraints. A spec answers: *what must the system do, and how should it behave?*

### PLAN.md Files — "How to build it"

A `*_PLAN.md` file contains a **serialized or parallelized plan for actually implementing the corresponding spec**. It breaks the spec down into concrete, actionable tasks with dependencies and ordering. A plan answers: *what do we build first, next, and in parallel?*

### Naming Convention

Spec and plan files should always be paired by prefix:
- `X_SPEC.md` pairs with `X_PLAN.md`
- Example: `FORETIAS_2_P2P_SPEC.md` ↔ `FORETIAS_2_P2P_3_IMPLEMENTATION_PLAN.md`

### Checkbox Format in PLAN.md

Checkboxes in PLAN.md files track progress. When an item is checked off, **always place a timestamp (to the minute) on the next line with indent into the bulleted list**:

```markdown
- [ ] Task not yet done
- [x] Task completed                    ← bad (no timestamp)
- [x] Task completed                    ← good it is
      (2026-05-06 13:11)                ← timestamped properly
```

This gives both agents and humans a clear idea of how work is progressing over time.

### Specify, Plan, Backburnered and Deprecated PLAN.md

Before a specification is complete, and some times before a plan is complete, we may have intermediate steps. In these situations a plan file may be created with corresponding open items:
```markdown
- [ ] Specify
- [ ] Plan
```
Most of time this is not needed, but when specification or planning takes on multi-turn iteration, it is useful to track these separately and check them off.

When a specification is considered VERY important but interfering with current highest priorities, it is marked with `[x] backburnered`. To be revived by removing the `[x] backburnered` marker. These plans are to be excluded when agent or human asks for plans that are: ready, pending, iterating, in progress, developing, active, etc. backburnered plans can only be found and addressed directly by using the words "backburnered plan(s)".

```markdown
- [x] Backburnered
      (2026-05-06 14:00)
- [ ] Do this or system will break
- [ ] And fix that bug
- [ ] ...
```
Canceled features shall be marked as "not to be done" using the marker "[-] don't do this". An entirely deprecated plans hall have a "[x] canceled" box at the top. The AGENT should first add the canceled check item, then mark all todo's with per-item cancelation "[-] each one". The deprecation can have elaboration regarding the reasons and context on the same line after the initial "[x] Canceled." text. Here is the example of properly canceled spec
```markdown
- [x] Canceled. Optionally explain there's a new spec see ABCD_PLAN.md
      (2026-05-06 14:00)
- [-] Do this or system will break
- [-] And fix that bug
- [-] ...
```

### Worktree Branch Tracking in PLAN.md

If a worktree branch is used for implementation, the PLAN.md **must** document the lifecycle of that worktree as explicit, separate checkbox tasks placed at appropriate points in the plan. The workpath shall always be `FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/{SPEC_NAME_WITHOUT_MARKDOWN_EXTENSION}_####`, with '####' replaced by random digts once, then the name stays consistent throughout the plan file.
```markdown
- [ ] Create worktree `git worktree add -b ${BRANCH_NAME} ${FULL_WORKTREE_PATH}}`
- [ ] `cd ${FULL_WORKTREE_PATH}}`; reset current session work directory to be the full worktree path.
...
  (implementation tasks go here)
...
- [ ] Verify all work is complete in ${FULL_WORKTREE_PATH} and committed to ${BRANCH_NAME}
- [ ] Merge ${BRANCH_NAME} to alpha
```

These tasks ensure the worktree lifecycle is tracked alongside the implementation work itself. Replace `${FULL_WORKTREE_PATH}` and `${BRANCH_NAME}` with actual values when writing the plan.

### INDEX.md — Spec/Plan Tracking

`specs/INDEX.md` tracks which specs and plans are being worked on, organized by **priority** and **dependency order** (highest priority + highest dependency first). It lists:

- All plans/specs with open tasks
- Their status (active, paused, blocked, backburnered)
- Their dependencies (what must finish before this can start)
- Deferred and backburnered features at the end

When starting work on a new spec/plan, check `specs/INDEX.md` to understand the current priority order and what blocks what. Update it as work progresses.

---

### Sub-Tasks

If scoping was incorrect for any task, and and it became many tasks, It is possible to write additional sub-tasks. These are indented in markdown
```markdown
...
- [ ] Merge ${BRANCH_NAME} to alpha
  - [x] Detected complex merge situation # Note, here we used a completed task to justify these subtasks
        (2026-05-06 14:00)
  - [ ] Update ${BRANCH_NAME} to follow new coding style
  - [ ] Update ${BRANCH_NAME} to use new API call convention
  - [x] Merged breaking alpha # Again, this is needed to document a clear need for more work as subtasks
        (2026-05-06 14:31)
  - [ ] Repair ALL tests in alpha
  - [ ] Finalize:
    - [ ] Check that _PLAN.md has all but Cleanup checkboxes completed
    - [ ] This is the last checkbox to be checked in my _PLAN.md
```
This example also illustrates that because foretias uses git merge and not rebase this situation where a fix to merge on alpha may be required. The final stage, to finalize, means to cleanup the sanity check everything before completely wiping the work directory and marking the plan complete.

## PROJECT STRUCTURE (Permanent Reference — Do Not Rescan)


```
foretias/                               # Repository root
│
├── specs/                              # All working specifications (SPEC.md & PLAN.md — see below)
│   ├── FORETIAS_0_OVERVIEW.md          # Design invariants, roadmap, milestones
│   ├── FORETIAS_1_MVP_SPEC.md          # v0.1 local-server stack (C11+Rust+PyO3)
│   ├── FORETIAS_2_IMPLEMENTATION_PLAN.md
│   ├── FORETIAS_2_P2P_SPEC.md          # v0.2-v0.8 network layers
│   ├── FORETIAS_2_P2P_*.md             # P2P sub-specs (handshake, DHT, hardening, probity, etc.)
│   ├── FORETIAS_3_PQC_INTEGRATION.md   # Post-quantum crypto (liboqs integration)
│   ├── FORETIAS_4_GPU_CRYPTO_ACCELERATION.md
│   ├── FORETIAS_CLI_SPEC.md            # CLI unified specification
│   ├── CLI_SPECIFIED.md                # CLI specification (supplementary)
│   ├── CALENDAR_REPLICATION_SPEC.md    # Calendar replication over P2P
│   ├── foretias-v1.md                  # Product & technical specification
│   ├── *_PLAN.md                       # Implementation plans (paired with SPEC.md files)
│   └── questions.md                    # Open design questions
│
├── src/foretias/                       # [BACKBURNERED] Python shim - removed from active scope
│   ├── __init__.py                     # Re-exports from foretias_p2p (Rust PyO3)
│   ├── cli.py                          # CLI #1 (Python): "foretis" entry point (calls Rust via PyO3)
│   ├── thin_client.py                  # Convenience client wrapper (calls Rust via PyO3)
│   └── _version.py                     # Version string
│
├── tests/                              # [BACKBURNERED] Python shim integration tests - removed from active scope
│
├── pyproject.toml                      # [BACKBURNERED] Top-level: hatchling build - removed from active scope
│                                       # Runtime dep: foretias-p2p (the Rust bindings)
│
├── p2p/                                # Rust + C11 workspace root
│   ├── Cargo.toml                      # Workspace: core-engine, foretias-client, foretias-server (foretias-python/foretias-java backburnered)
│   │
│   ├── core/                           # C11 verified cryptographic primitives
│   │   ├── include/
│   │   │   └── foretias_core.h         # Single public header (Ed25519, P-256, SHA-256, BLAKE3, Noise, Merkle, FROST, etc.)
│   │   ├── src/                        # 15 .c files (identity, signing, hashing, noise, merkle, frost, nullifier, rng, memzero, etc.)
│   │   ├── tests/                      # 14 test files (test_ed25519.c, test_sha256.c, test_noise.c, etc.)
│   │   ├── CMakeLists.txt              # Builds static lib (foretias_core), links libsodium + OpenSSL
│   │   └── Makefile                    # Convenience targets (make, make test, make install)
│   │
│   ├── core-engine/                    # Rust crate — safe wrappers over C11 FFI + domain logic
│   │   ├── Cargo.toml                  # foretias-core (links C11 via build.rs + bindgen)
│   │   ├── build.rs                    # Compiles C11 core, runs bindgen over foretias_core.h
│   │   └── src/
│   │       ├── lib.rs                  # Re-exports: core, crypto_server, foretias, config, collision, epoch, noise, probity
│   │       ├── core/                   # Safe Rust wrappers (bindings.rs, identity.rs, signing.rs, hash.rs, etc.)
│   │       ├── crypto_server/          # CryptoServer trait + SoftwareCryptoServer backend
│   │       ├── foretias/               # Domain types: tick.rs, foretis.rs, calendar.rs, external_attestation.rs
│   │       ├── chronomatter/           # Time-being mutable logic (stamp, tick, auto-attestation, daemon)
│   │       ├── config/                 # NodeConfig, TimeFamilyConfig
│   │       ├── collision/              # Heartbeat, collision detection
│   │       ├── epoch/                  # Epoch scheduler, snapshot, committee
│   │       ├── probity/                # ProbityReport, aggregator, store
│   │       ├── noise.rs                # Noise_XX handshake (Rust-side integration)
│   │       └── error.rs                # NodeError, CryptoError
│   │
│   ├── foretias-client/                # Rust crate — thin client library (CLI #2)
│   │   ├── Cargo.toml                  # foretias-client (depends on foretias-core)
│   │   ├── src/
│   │   │   ├── lib.rs                  # Re-exports: foretias, calendar, config
│   │   │   ├── foretias/               # Foretias OOP struct, ForetiasError, ForetiasStatus
│   │   │   ├── calendar/               # Calendar, mirror, PtP calendar retrieval
│   │   │   ├── noise_ptp.rs            # Noise_XX PtP client (Standalone/PtP/P2P levels)
│   │   │   └── config/                 # Containment configs (StandaloneConfig < PtpConfig < P2pConfig)
│   │   └── tests/                      # Client tests (unit, PtP integration)
│   │
│   ├── foretias-server/                # Rust crate — server + CLI binary
│   │   ├── Cargo.toml                  # foretias-server (depends on foretias-core, foretias-client, libp2p)
│   │   ├── src/
│   │   │   ├── lib.rs                  # Re-exports: server, communerd, calendar, calendar_store, metrics, probity
│   │   │   ├── main.rs                 # CLI (Rust): "foretias" binary — serve/stamp/verify/prove-verification/inspect-attestations
│   │   │   ├── server/                 # TimeFamilyServer (stamp, verify, integrity_check, daemon, JSON-RPC, HTTP handlers)
│   │   │   ├── communerd/              # P2P layer: Communerd tiers (Reader/Server/P2P), libp2p swarm, DHT, gossipsub, peer pool, mutual attestation
│   │   │   ├── calendar/               # Calendar wrapper
│   │   │   ├── calendar_store/         # LRU policy, encrypted JSONL persistence
│   │   │   ├── probity/                # Probity gossip handler
│   │   │   ├── metrics.rs              # Node metrics
│   │   │   └── replication_logger.rs   # Calendar replication logging
│   │   └── tests/                      # Integration tests (e2e stamp/verify, P2P connect, probity gossip)
│   │
│   ├── foretias-python/                # [BACKBURNERED] PyO3 Python bindings — removed from workspace
│   │   └── (see foretias/specs/SCOPE_REDUCTION_SPEC.md for future reintroduction)
│   │
│   └── foretias-java/                  # [BACKBURNERED] JNI bindings — removed from workspace
│       └── (see foretias/specs/SCOPE_REDUCTION_SPEC.md for future reintroduction)

**CLI Implementations (one active, Python/Java backburnered):**

| # | CLI | Language | Entry | Binary/Command | Location |
|---|-----|----------|-------|----------------|----------|
| 1 | Rust "foretias" | Rust (native) | `p2p/foretias-server/src/main.rs` | `foretias` | p2p/ Cargo workspace |

> **Note:** Python (`foretis`) and Java CLIs are removed from active scope. See `foretias/specs/SCOPE_REDUCTION_SPEC.md` for rationale.

### Architecture Layers

```
                    ┌─────────────────┐
                    │   Application   │   (foretias-server CLI)
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  foretias-server │   Server, JSON-RPC, calendar store
                    └────────┬────────┘
            ┌─────────────────┼──────────────────┐
            │                 │
 ┌──────────▼──────┐
 │   communerd     │  (libp2p P2P)
 │                 │
 │ - DHT           │   ┌───────────────────────────────────┐
 │ - GossipSub     │   │ Communerdette (per-TBID)          │
 │ - Peer pool     │ ──│ - relationship state + stats      │
 │ - Mutual attest │   │ - binding status + route choice   │
 │ - DHT signing   │   │ - Take 3 inbound gate per TBID    │
 │   (per g3-d)    │   │ - CommunerdetteLine = the only    │
 │                 │   │   handle Calendar/Chronomatter use│
 └────────┬────────┘   └───────────────────────────────────┘
          │
          │        ┌─────────▼───────────────────────────┐
          │        │       foretias-core                 │
          │        │  Safe Rust wrappers, CryptoServer,  │
          │        │  domain types                       │
          │        └─────────┬───────────────────────────┘
          │                  │  (FFI via bindgen)
          │        ┌─────────▼───────────────────────────┐
          │        │       C11 core                      │
          │        │  libsodium + OpenSSL + liboqs       │
          │        └─────────┬───────────────────────────┘
          │                  │
 ┌────────▼────────┐   ┌─────▼────────┐   ┌────────────┐
 │   libp2p        │   │  libsodium   │   │  liboqs    │
 │  (Rust crate)   │   │ (Ed25519,    │   │ (PQC:      │
 │                 │   │  hash, AEAD) │   │  Kyber,    │
 │ - noise protocols│  └──────────────┘   │  Dilithium, │
 │ - yamux/mplex   │                      │  etc.)      │
 │ - identify      │                      └────────────┘
 │ - ping          │
 │ - dht           │
 └─────────────────┘
```

> **Note:** Python (`foretias-py`) and Java (`foretias-java`) bindings are removed from active scope. See `foretias/specs/SCOPE_REDUCTION_SPEC.md`.

### Peer Transport Comparison

| Layer | Protocol | Encryption | Purpose |
|-------|----------|-----------|---------|
| libp2p Direct (new) | libp2p request_response over yamux | libp2p-noise (C11 deviation) | Stamp, verify, calendar replication, ping (preferred) |
| Noise_XX TCP (custom) | Direct TCP + Noise_XX + JSON-RPC | C11 Noise_XX | Stamp, verify, calendar replication, ping (fallback) |
| libp2p swarm | Kademlia DHT + GossipSub | libp2p-noise | Peer discovery, DHT, probity gossip, heartbeat |

---

## BUILD, TEST, RUN COMMANDS

### Prerequisites (System Dependencies)

```bash
# Required
sudo apt install build-essential cmake clang libsodium-dev libssl-dev
# For BLAKE3 in C11 core
# For P-256 support
rustup install stable  # Rust toolchain
```

> **Note:** Python (`maturin`) and Java bindings are removed from active scope. See `foretias/specs/SCOPE_REDUCTION_SPEC.md`.

### Build Environment Variables

Set these environment variables BEFORE any cargo build/check/test commands to accelerate compilation:

```bash
# Parallel CMake builds for oqs-sys (liboqs C library) — defaults to CPU count if unset
export CMAKE_BUILD_PARALLEL_LEVEL=10
```

This variable is consumed by `core-engine/build.rs` which passes it to `cmake --build --parallel`. Without it, CMake falls back to single-threaded compilation of the ~200 post-quantum signature scheme source files, making foretias-core builds take 15-30 minutes instead of 2-5 minutes.

### Build Caching Strategies

The `foretias-core` crate (oqs-sys/liboqs C library) dominates build time. The following strategies reduce repeated build costs:

**1. Enable only required libp2p features** — biggest win (20-25% reduction in initial build time):
```toml
# In foretias-server/Cargo.toml
libp2p = { version = "0.56", default-features = false, features = [
    "tcp", "noise", "yamux", "gossipsub", "kad", "identify", "ping", "request-response"
] }
```
libp2p has 38 feature flags, none enabled by default. Enabling only what's used avoids compiling unused protocols (QUIC, WebRTC, WebSocket, TLS, etc.).

**2. Use sccache for Rust compilation** — caches external crates (libp2p, etc.) across builds:
```bash
cargo install sccache # should already be installed
export RUSTC_WRAPPER=sccache
# First build populates cache; subsequent rebuilds hit cache for unchanged deps
sccache -s  # Check cache stats
```
Note: sccache cannot cache incrementally-compiled workspace members or proc-macros, but it effectively caches the libp2p dependency tree.

**3. Per-worktree target directories** — DO NOT share `target/` across worktrees (cargo locking will corrupt artifacts). Use separate target dirs per worktree:
```bash
export CARGO_TARGET_DIR="$HOME/.cache/cargo/foretias-$WORKTREE_NAME"
```
The shared `CARGO_HOME` registry (`~/.cargo/registry`) is already reused across worktrees for source downloads.

**4. Pre-build heavy dependencies** — warm the cache before starting work:
```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10
# Build foretias-core first (takes longest), then only check foretias-server
cd p2p && cargo build -p foretias-core
cd p2p && cargo check -p foretias-server  # Fast after core is cached
```

**5. Profile optimization for dev builds** — faster incremental compilation:
```toml
[profile.dev.build-override]
opt-level = 0
codegen-units = 256
```

**What does NOT help for local development:**
- `cargo-chef` — designed for Docker layer caching, not local incremental builds
- Shared target directories — unsafe (race conditions on `.rmeta`/`.rlib` files)
- `cargo-biscuit` — not widely adopted, limited benefit over native cargo caching

### 1. Build C11 dependencies

All builds require this first step:
```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10
# Step 1: Build C11 core library (static lib)
cd p2p/core && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build
```

### 2. Build Everything

```bash
# Do step 1, then

# Step 2: Build Rust workspace (core-engine, foretias-client, foretias-server)
cd p2p && cargo build --workspace
```

### 1a. Build Individual Crates (Incremental)

Use these when only one crate has changed — faster than rebuilding the full workspace. Always run from `p2p/`.

```bash
# Do step 1, then

# Build only core-engine (safe Rust wrappers over C11 FFI)
cd p2p && cargo build -p foretias-core

# Build only foretias-client (thin client library)
cd p2p && cargo build -p foretias-client

# Build only foretias-server (server + CLI binary)
cd p2p && cargo build -p foretias-server

# Build a single crate in release mode
cd p2p && cargo build -p foretias-server --release

# Check a single crate without full compilation (fastest validation)
cd p2p && cargo check -p foretias-core
cd p2p && cargo check -p foretias-client
cd p2p && cargo check -p foretias-server
```

**Dependency chain** (build order when multiple crates change):
1. C11 core (`p2p/core`) — static lib, linked by all Rust crates
2. `foretias-core` (core-engine) — depends on C11 core via bindgen
3. `foretias-client` — depends on core-engine
4. `foretias-server` — depends on core-engine + foretias-client

If build stalls, and does not resolve after repeating an attempt, you may look at build process using verbose flag.
This flag is very verbose, so use a subagent to run it and check on progressing output.
```bash
cd p2p && cargo build -vv
```
### 2. Run All Tests

```bash
# C11 core tests (ctest)
cd p2p/core/build && ctest --output-on-failure

# Rust workspace tests (unit + integration)
cd p2p && cargo test --workspace

# All tests at once
cd p2p/core/build && ctest --output-on-failure && cd ../.. && cargo test --workspace
```

### 3. Run Individual / Targeted Tests

```bash
export CMAKE_BUILD_PARALLEL_LEVEL=10
# --- C11 tests ---

# Run a specific test file (CMake builds all into test_all, use -V to list):
cd p2p/core/build && ctest -R foretias_core_tests --verbose

# Or rebuild and run the test binary directly:
cd p2p/core && cmake --build build --target test_all && ./build/test_all

# --- Rust tests ---

# Run all tests in one crate:
cd p2p && cargo test -p foretias-core
cd p2p && cargo test -p foretias-server
cd p2p && cargo test -p foretias-client

# Run a specific test by name (substring match):
cd p2p && cargo test -p foretias-server -- test_stamp_and_verify_e2e
cd p2p && cargo test -p foretias-core -- ed25519
cd p2p && cargo test -- calendar_store

# Run only unit tests (exclude integration):
cd p2p && cargo test -p foretias-server --lib

# Run only integration tests:
cd p2p && cargo test -p foretias-server --test integration

# Run with output (don't capture stdout):
cd p2p && cargo test -- --nocapture
```

### 4. Build for Release

```bash
# Release build — optimized, LTO, stripped
cd p2p && cargo build --workspace --release

# Release binary: p2p/target/release/foretias
# Release C11 lib: p2p/core/build/libforetias_core.a
```

### 5. Package & Publish

```bash
# --- C11: install system-wide ---
cd p2p/core && make install PREFIX=/usr/local

# --- Python/Java bindings removed from active scope ---
# See foretias/specs/SCOPE_REDUCTION_SPEC.md for future reintroduction
```

### 6. CLI Usage — Rust Binary (`foretias`)

```bash
# Build first:
cd p2p && cargo build --release

# Start server (daemon, stamps every chronon):
target/release/foretias serve --addr 127.0.0.1:4001 --chronon_ns 60000000000

# Start server with persistence:
target/release/foretias serve --addr 127.0.0.1:4001 --persist-path /tmp/cal

# Start server in dormant mode (verify-only, loads persisted calendar):
target/release/foretias serve --addr 127.0.0.1:4001 --start-dormant --persist-path /tmp/cal

# Start server with P2P peers:
target/release/foretias serve --addr 127.0.0.1:4001 --peer 127.0.0.1:4002 --mutually_attest_every_chronons 10

# Stamp a message (sends to running server via Noise+JSON-RPC):
target/release/foretias stamp --message "hello world" --server 127.0.0.1:4001

# Stamp from file:
target/release/foretias stamp --message-file myfile.txt --server 127.0.0.1:4001 --stamp-output stamp.json

# Verify a stamp (server-side verification):
target/release/foretias verify --message "hello world" --foretis-file stamp.json --server 127.0.0.1:4001

# Prove verification (fetch calendar slice, verify locally):
target/release/foretias prove-verification --message "hello world" --foretis-file stamp.json --server 127.0.0.1:4001

# Inspect external attestations in a persisted calendar:
target/release/foretias inspect-attestations --calendar /tmp/cal/calendar.json
```

### 7. CLI Usage — Python (backburnered)

> **Note:** Python CLI (`foretis`) and Java CLI are removed from active scope. See `foretias/specs/SCOPE_REDUCTION_SPEC.md`.
> The Python directories (`src/foretias/`, `p2p/foretias-python/`) and Java directory (`p2p/foretias-java/`)
> remain as reference but are not built or tested as part of the workspace.

---

## Project Segmentation

Software projects May be large or small. Their complexity and diffiulty may also vary. Generally speaking we use these terms for disjoint components of softare:
  - Major
    - This is a noun, That "specification file is for a major", or an adjective "that is a major specification"
    - This is a very large feature, that may break many existing functionalities while implementing
    - Some extensive exchange with human may be required.
    - Some multi-modal analysis, including web-searches, prototyping, analysis, etc.
    - aka Major Feature, Major release, Major upgrade, etc.
    - Example: "Centralize and fully sepcification of CLI interface by gathering features from all the existing implementations. Resolve any conflicts or redundancies. Then update all implementation to follow new specification."
    - Example: "DHT for discovering peers for different purposes: mutual attestation, calendar replication, capability-matching, etc."
  - Phase
    - a Major feature may be implemented in many phases
    - Example: Research, Discuss and Q&A with Human, Design and implement tests, Implementation feature, Code Review, Security Review, Fresh-eye review, merge to alpha, etc.
  - Stage
    - each phase may contain many stages
    - Example for Research: Analyze code, web search, pose research questions, combination and synthesis, etc.
  - Step
    - Each stage may be several steps.
    - Example: Search Arxiv, Search Google Schollar, Search wikipedia, Search reddit, Search Google Groups,
    - Example: Change the entire project name from "Fortias" to "Foretias".
  - Task
    - Each step may be several tasks.
    - Tasks are smaller very well defined jobs, typically using tool or simple updates.
    - Example: Alter spelling of "Fortias" to "Foretias" in all file names
    - Example: Alter spelling of "Fortias" to "Foretias" in C11 code.
    - Example: Alter spelling of "Fortias" to "Foretias" in rs code.
It is very important, given a request from user that correspond to a feature request or software change, to set a scope size. After scoping, perhaps the new request may be placed into an existing larger sized poject, or cause a split of existing project to form similar sized projects. Ultimately correctness and implementation efficiency is the goal achieved through organization, consideration and communication.

When request is small, you may combine Major/Phase/Stage into 

## Development tools

Please use plugins and mcp's for performing disk operations, file searches and file edits. Use fully specified regular expressions (covering various cases), through mcp or using `sed` directly. These means of editing are much faster than regenerating the entire document. Each time regexp is used to for updates, please reread updated document before replacing original document.  Use Github mcp to perform git related actions.

When commiting to Git, always state project segment and software version and model version:
```git
Major: Refactor CLI, Phase: Discussion with Human--complete
opencode 1.14.39, Qwen3.6-27B-AWQ-BF16-INT4
```

### Code Quality Tooling

The project uses the following code quality tools and configurations:

| Tool | Config | Baseline | CI |
|------|--------|----------|----|
| **Clippy** | `p2p/clippy.toml` | `docs/security/clippy-baseline.md` | `.github/workflows/clippy.yml` |
| **rustfmt** | `p2p/rustfmt.toml` | — | `.github/workflows/rustfmt.yml` |
| **cargo-geiger** | — | `docs/security/cargo-geiger-baseline.md` | — |
| **.editorconfig** | `.editorconfig` | — | — |

**Before committing**, run:
```bash
cd p2p && cargo clippy --workspace --all-targets  # Must pass (zero warnings)
cd p2p && cargo fmt --check                       # Must pass (no formatting changes)
```

**Clippy configuration:** `too-many-lines-threshold = 100`, `too-many-arguments-threshold = 7`
**rustfmt configuration:** `max_width = 100`, `tab_spaces = 4`, `edition = "2021"`
**cargo-geiger baseline:** foretias-server has 302/1,075 unsafe functions (expected — libp2p + FFI). foretias-core and foretias-client own code has 0/0 unsafe.

**Known flaky tests:**
- `snapshot_suite::tests::compare_mismatch_when_different` — PID collision in parallel runs. Passes when run individually.

## Development Rules

**NEVER** start file changes for project Phase or larger WHEN any tests are broken.
**NEVER** start large project segment work WHEN ANY tests are broken even if there're notes indicating those breakage are known. The test has to be manually disabled by human OR repaired and committed.

## TYPE-ENFORCED TRUST BOUNDARIES (Mandatory)

**Source of truth for all security requirements: `SAFETY.md`.** This section provides the programming essentials only.

Foretias enforces a three-stage type progression for all inbound data:

```
Unprocessed<X>  →  CleanAuthenticated<X>  →  Externalized<X>
(parsed,        →  (authenticated +       →  (wire/disk format,
 untrusted)         cleansed, trusted)          minimal fields)
```

### Where Each Type May Appear

| Type | Allowed Locations | Forbidden Locations |
|------|------------------|---------------------|
| **`Unprocessed<X>`** | Communerd inbound handlers ONLY (`communerd/mod.rs`, `handlers.rs`, `gossip_handler.rs`), unit tests | Everywhere else — Chronomatter, Calendar, core-engine domain logic |
| **`CleanAuthenticated<X>`** | Chronomatter, Calendar, core-engine domain logic, intra-family communication | Never at wire/disk boundaries |
| **`Externalized<X>`** | Communerd outbound (wire transmission), Calendar storage (disk persistence) | Never in domain logic or intra-family communication |

### Code Review Checklist

Before approving any Rust changes, verify:
- **No `Unprocessed<X>`** escapes Communerd or test code
- **No `Externalized<X>`** appears in domain logic (Chronomatter, core-engine)
- **No direct `CleanAuthenticated<X>` construction** outside `clean_auth.rs` (private constructors)
- **No raw domain types** (`ChrononRecord`, `Foretis`, etc.) at trust boundaries — always wrapped

### Constructor Discipline

`CleanAuthenticated<X>` has **private constructors**. The only two gates are:
- `into_clean_authenticated()` — inbound gate (Unprocessed → CleanAuthenticated, requires verification)
- `from_trusted()` — local gate (domain type → CleanAuthenticated, for data created locally)

Never construct `CleanAuthenticated<X>` directly. The compiler enforces this.

### Signing Boundary (Mandatory)

Each component (Chronomatter, Calendar, Communerd) owns its own signing key and signs only what it produced. No component signs for another.

- **Internal calls** (Calendar → Chronomatter) do NOT need signing — they're within the same trust boundary.
- **Signing happens at the external boundary:** just before transmitting to `CommunerdetteLine` for external transmission.
- **Signing key ownership:** the signing key stays with the time being (component) that has the corresponding TBID.
- **Signing initiation:** Communerdette requests signing when external transmission is needed. The producing component signs with its own key at that point.

```
Calendar calls Chronomatter internally → no signing needed
Calendar hands result to CommunerdetteLine → Calendar signs with Calendar's key
Chronomatter hands result to CommunerdetteLine → Chronomatter signs with Chronomatter's key
```

**Never use `sign_tbid_message` on `TimeFamilyServer`** — each component's own signing API replaces it.

**Implementation status:** Chronomatter, Calendar, and Communerd each now own their own signing key and manage their own signing lifecycle independently. See `SAFETY.md` for the full signing key ownership table and per-component key management details.

---

## COMMUNERDETTE AND PER-TBID RELATIONSHIPS (Mandatory)

Communerd is the only component with extra-family network access. Within Communerd, **`Communerdette`** is the private per-external-TBID relationship manager, and **`CommunerdetteLine`** is the narrow capability handle that Calendar / Chronomatter-adjacent orchestration / TimeFamily code uses for all TBID-scoped network services.

### Invariants

1. **Communerd remains the only network authority.** Calendar and Chronomatter never receive raw transport objects, PeerPool mutation access, swarm command channels, or `&Communerd` directly. They obtain `CommunerdetteLine` handles via `Communerd::line_for_tbid(tbid)`.
2. **One Communerdette, one external TBID.** A `CommunerdetteLine` must never send a request to a different TBID than the one it was created for. Enforced by the line's `target_tbid` field being copied into every request.
3. **Communerdette is private to the `communerd` module.** Internal admin methods (`mark_binding_*`, `record_route_*`, `set_active_route`, `add_route_candidate`, `shutdown`, queue spawning, etc.) are `pub(super)` and never reachable from Calendar/Chronomatter.
4. **Take 3 inbound gate runs inside Communerdette.** Raw remote replies enter as untrusted bytes and exit as `CleanAuthenticated<R>` only after `Unprocessed<R>::verify(...)` and a TBID-match check. Calendar may NOT store remote replies as trust-bearing evidence except via this path.
5. **TBID signing authority is not transport authority.** Communerd may authenticate connections, verify signed payloads, and route requests, but it must never produce a signature for Calendar, Chronomatter, or any other local TBID owner. Calendar that needs a Chronomatter-signed payload obtains it through Chronomatter's internal signing API and hands the already-signed bytes to `CommunerdetteLine` for delivery.
6. **Binding status is not transport identity.** `TbidBindingStatus::ClaimedByDht` is a discovery hint (transport identity claimed but not proven). `Verified` requires an application-level binding proof. Until binding proof is implemented, trust-bearing code must not treat a DHT claim as a verified TBID binding.
7. **All Communerdette requests are bounded.** `get_calendar_slice`, `get_tick`, and `stamp` carry per-call timeouts (default 15s) and return `TransportError::Timeout` rather than waiting forever.

### When to Use `CommunerdetteLine`

| Calendar / Chronomatter wants to … | Use … |
|------------------------------------|-------|
| Fetch a tick for local verification | `line.get_tick(n).await` — returns `CleanAuthenticated<ChrononRecord>` |
| Fetch a calendar slice | `line.get_calendar_slice(start, count).await` |
| Request a stamp from a remote TBID (mutual attestation) | `line.stamp(content, echo).await` — returns `CleanAuthenticated<Foretis>` |
| Start a calendar stream (mirroring, see Group 4b) | `line.start_calendar_stream(from_tick).await` |
| Inspect relationship status (read-only) | `line.status_summary()` |

### Do NOT

- Call `Communerd::stamp_peer` / `get_calendar_slice` directly from Calendar — go through a `CommunerdetteLine`.
- Construct `Communerdette` or pass `Arc<Communerdette>` outside the `communerd` module.
- Store raw remote replies as trust-bearing evidence; insist on `CleanAuthenticated<R>`.
- Add new methods to `CommunerdetteLine` without scoping them to one TBID and returning either `CleanAuthenticated<R>` or `Unsupported` for not-yet-implemented capabilities.

### Spec References

See `specs/COMBINED_GROUP7_COMMUNERDETTE_SPEC.md` for the full design (invariants, transport selection, request priority, binding state, remote authentication product).

---

## CALENDAR ACTIVE MIRRORING (Group 4b — Mandatory for mirror work)

> **Status note (2026-05-26):** Group 4b Phases 4b.1–4b.5 are merged on alpha and continue to compile/test cleanly. **Further mirror work — Stream 4b.4c (StartStream), 4b.4d (DoAttestation refactor), 4b.6 (graceful shutdown), and all of Stream 4c (Proof of Storage) — is DEFERRED.** Mirror specifications and plans will be updated and remaining tasks executed AFTER Communerdette (`COMBINED_GROUP7_COMMUNERDETTE_SPEC.md`) reaches feature completion AND the `COMBINED_GROUP4_*` files are revised to integrate with Communerdette's `MirrorDispatcher` / `CommunerdetteLine` / per-TBID lifecycle. Do not start work on the open mirror items until both conditions hold. The content below documents the architecture as it stands today; the architecture WILL evolve through the post-Communerdette rewrite.

Calendar is a **task-driven orchestrator** with a documented five-priority hierarchy. Mirror replication is priority 4 (persist family's calendar via mirrors); mirroring other calendars is priority 5 (starvable). Priorities 1–3 (record ticks, support local verify, mutual attestation) are synchronous and cannot be blocked by mirror back-pressure.

### Priority Invariant

| # | Priority    | Responsibility                                                  |
|---|-------------|-----------------------------------------------------------------|
| 1 | Critical    | Record every tick for the family's chronomatter                 |
| 2 | High        | Support local verify requests (look up ticks, validate chains)  |
| 3 | Medium-High | Mutual attestation with peers                                   |
| 4 | Medium      | Persist family's calendar in the P2P network (find mirrors)     |
| 5 | Low         | Mirror other calendars' ticks (starvable)                       |

This ordering is reproduced in `p2p/foretias-server/src/calendar/mod.rs`'s module doc comment. Any change to it requires updating both that doc and this section.

### Task Queue + MirrorDispatcher

Mirror work flows through Calendar's task queue (`calendar::task_queue`):

- `CalendarTask` enum has six variants (DoAttestation, FindNewMirror, InitiateDump, StartStream, ExploreMirror, ExpireMirror).
- Worker pool (4 workers by default) pulls tasks from a `tokio::sync::mpsc::UnboundedReceiver` and dispatches to per-variant handlers.
- Handlers route network calls through the narrow `MirrorDispatcher` async trait. **Communerd implements this trait; Calendar holds an `Arc<dyn MirrorDispatcher>`.** Do not give Calendar a direct reference to Communerd or its transports.

### MirrorDispatcher Surface

```rust
#[async_trait]
pub trait MirrorDispatcher: Send + Sync {
    async fn known_peers(&self) -> Vec<PeerAddr>;
    async fn mirror_announce(&self, peer: &PeerAddr, local_tbid_hex: &str)
        -> Result<bool, String>;
    async fn history_dump_chunk(&self, peer: &PeerAddr, local_tbid_hex: &str,
        records: Vec<ChrononRecord>) -> Result<u64, String>;
    async fn history_dump_complete(&self, peer: &PeerAddr, local_tbid_hex: &str,
        total_records: u64) -> Result<u64, String>;
    async fn mirror_health_check(&self, peer: &PeerAddr, local_tbid_hex: &str)
        -> Result<u64, String>;
}
```

Add new mirror RPCs by extending this trait, not by reaching into Communerd internals from worker handlers.

### Wire Methods (Server-Side Handlers)

Six new JSON-RPC methods are registered in `server/jsonrpc.rs` dispatch table:

- `mirror_announce` — Source → Candidate. Mirror accepts iff `MirrorStore::can_accept_mirror`.
- `history_dump_request` — Source → Mirror. Validate range and capacity.
- `history_dump_ack` — Mirror → Source. Symmetric echo.
- `history_dump_chunk` — Source → Mirror. Each record goes through the Take 3 inbound gate (`Unprocessed<ChrononRecord>::verify` + chain link) before insertion into the MirrorStore.
- `history_dump_complete` — Source → Mirror. End-of-stream marker.
- `mirror_health_check` — Source → Mirror. Liveness + tick_count probe.

All chunk-handling preserves the type-enforced trust boundary — no raw `ChrononRecord` enters the mirror's store except via `into_clean_authenticated_*`.

### Known Limitations

- `Unprocessed<ChrononRecord>::verify` in `core-engine/src/foretias/clean_auth.rs:269` returns `CleanAuthError::NotYetImplemented` for `tb_version >= 1`. Until PQC genesis verification ships, the full source→mirror dump cannot complete end-to-end; the wire path works but the receiver's chain verifier stubs out. The blocked integration test (`tests/mirror_integration.rs::source_dumps_history_to_mirror`) is `#[ignore]` with that reason.
- `DUMP_CHUNK_SIZE` is currently 1 (the spec target is 64 records or 1 MB). Raised once the verifier ships and chunk-size sweeps are measured.

### Spec References

See `specs/COMBINED_GROUP4_SPEC.md` §3 and `specs/COMBINED_GROUP4_PLAN.md` §4b for the full design and phase plan.

---

## How To Write Rust Code

> # ⛔ STOP — READ `rust_instructions.md` BEFORE TOUCHING ANY RUST ⛔
>
> **Every Rust coding instruction for this repository now lives in
> [`rust_instructions.md`](./rust_instructions.md). It is the single source of
> truth. The rules are NOT duplicated here.**
>
> This is **mandatory, not advisory.** Before you read or write a single line of
> Rust — any `.rs` file, `Cargo.toml`, `build.rs`, FFI shim, or doc example — you
> **MUST** open `rust_instructions.md` and follow it **word for word, in full.**
>
> Do **NOT** skim it. Do **NOT** assume you remember it. Do **NOT** substitute
> general Rust knowledge or habits from other codebases. Its rules — priorities,
> ownership & borrowing, encapsulation, error handling, concurrency & async,
> cryptographic and secret-material handling, signed DHT records, FFI boundaries,
> testing (including Toppoli), and the Foretias security review checklist —
> **override** default behavior and any conflicting instinct.
>
> If you are about to touch Rust and have **not** already read
> `rust_instructions.md` in this session, **read it now.** No exceptions.

---

## MISC

### Description is Important

The AI agent is to refrain from simplifying a key behavior of Chronochain by saying "destroy private key". This project aspires to do this well. That description should not restrict the mindset of readers to what we are using now. The words that should replace destroy in that description so as to not lock human mind into a fixed mindset. The list to randomly select a term from include: disable/disablement, impair/impairment, incapacitate/incapacitation, debilitate/debilitation. Those descriptors could be further modified with intensifiers including: cryptographically, algorithmically, provably, verifiably, etc. Never use: destroy, delete, remove, forget. Never downtoned: simple, naive, etc.

Do write: "The expring chronon is cryptographically disabled from making additional stamp after producing a forward_fortis"
DO NOT write: "The Chronomatter simply forgets the private key so it can't be used again."

It is paramount that we do not restrict thinking to the implementation. The Foretias project, foremost, is a human expression of human desire and human need, for some minimum amount of constant integrity.

### Embedded Communications

If any file, other than this example in the AGENTS.md, contain a parenthetical comment, anywhere, it is a request for agent to comment based on the context surrounding that comment.
```markdown
Blah blah, some texxt (@Agent, do you think that word is mispelled?)
```

or
```python
def fibonacii(x):
	# @agents, errrr, terminal case? spelling? did you even run this?
	return fibonacii(x-1) + fibonacii(x-2)
```

or even not in a comment
```python
def add (x):
@AGENT, this is just plain wrong!
	return x+y;
```
The expectation is for agent to consider, discuss, and resolve the concern
that follows various capitalizations of `@agent` or `@agents`. Resolution, once achieved, also means the parenthetical comment can be completely removed.

If this form of embedded communication is discussed while performing another task, determin if it is relevant or interferes with current task. In some cases, this causes an immediately actionable response, other times, the encounterance results in an extra '[ ] TODO:human concern at file FILENAME line LINE_NUMBER' added to current task list to investigate. In some cases, if it is clear that the situation is too complex or require too much context, it may become a "[ ] TODO: write a specification and plan to address human concern at file FILENAME line LINE_NUMBER"

In some conversations, the resonses can be nested:

```python
def add (x):
(@AGENT, this is just plain wrong!)
	return x+y;
```

can become:

```python
def add (x):
(@AGENT, this is just plain wrong! (@Human, this is just an example))
	return x+y;
```

to which human may respond

```python
def add (x):
(@AGENT, this is just plain wrong! 
 (@Human, this is just an example
  (@Agents, okay, let's keep this example ONLY in this one section of the project's AGENTS.md
     Everywhere else, this entire conversation should be removed once all changes are made and an
     LGTM or SGTM signal is unmistakably typed by a human to agents. Once issues are resolved,
     the original python file will be completely normal functioning code, because this discussion
     would have resolved any and all concerns.
)))
	return x+y;
```

Plese keep the above example in the AGENTS.md; while conversing and resolving human or agent concerns, it is very important to keep the conversational context, so that both agent and user can quickly recover context of what they were discussing. I rare situations one may even need to add makers such as "see above @human comment M1m considering your @Agents comments M2m, and M3m below'.(And those markers would be searcheable to find easily) In other situations, the next step is to remove the entire conversation when resolution is reached and leave the file looking like this:

```python
def add (x):
	return x+y;
```

Or, perhaps:

```python
def add (x):
   # @human, you indicated this example is to be kept in this file for demonstration purposes on 2026-06-15.
	return x+y;
```

### Uncertainty and Other Utterances in Conversing with Human

Expressions of uncertainty and hypotheticals, such as "perhaps", "maybe", "possible", "what if", "in case". These words does not mean a firm directive from human to either pause work, or make large changes. It means human wants a todo task enqueued, perhaps to be done immediately, to explore options regarding the statement. In the last sentence, the perhaps suggests an option that can be explored, and it also highlight the possibility of the task not at the top of the todo list. More than anything else, the statement suggests human is thinking about the issue and you can help that thinking process.

"Wait!" is almost always typed when humans are reading the previous output and found something objectionable. "Wait!" meant stop that, something was wrong. This also implies whatever they ask about, it is highly unlikely they read through the reast of the response. Good or bad, that is human nature, please accomodate this behavior as a supportive agent. After addressing the concern following "wait!", the you can summarize what you meant to say after the output that the human said "Wait!" to--where it is is inferred based on the question or comment after "WAit!", when in doubt, summarize the whole response in the context of having addressed the human's concern.

"Continue." is uttered when the humans sees output on the screen that they think is incomplete. The best course of action, irrespective of actual status, is to summarize the progress made in the most recent few turns of conversation. If indeed the progress was ended or blocked by nonresponsive sub-agents, then take approrpiate action. If the short term task is truely complete, still output the summary, but also present outstanding todo items as well as other possible next steps for human to decide. Human may decide previous task is not complete and needs more work, or they may agree previous task was complete and move on to one of the options for next steps.


### When in Doubt

When uncertain, choose the design that is easiest to prove correct, easiest to test, and easiest for the next human to understand.

Correctness first. Then readability and maintainability. Then efficiency. Then principles and asethetics.
