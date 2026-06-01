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

## Development Rules

**NEVER** start file changes for project Phase or larger WHEN any tests are broken.
**NEVER** start large project segment work WHEN ANY tests are broken even if there're notes indicating those breakage are known. The test has to be manually disabled by human OR repaired and committed.

## TYPE-ENFORCED TRUST BOUNDARIES (Mandatory)

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

### Transition Points

1. **Inbound Gate (Communerd):** Raw bytes → `Unprocessed<X>` → `CleanAuthenticated<X>` (via `into_clean_authenticated()`)
2. **Intra-Family:** Only `CleanAuthenticated<X>` flows between Chronomatter, Calendar, and server logic
3. **Outbound Gate (Communerd/Calendar):** `CleanAuthenticated<X>` → `Externalized<X>` (via `externalize()`) for wire/disk

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

This chapter applies to Rust code in both projects:

Optimize in this order:

1. **Correctness**
2. **Readability and maintainability**
3. **Testability**
4. **Efficiency**
5. **Style principles**

Do not sacrifice correctness for cleverness, abstraction, minimalism, or performance. Do not sacrifice readability unless there is a measured, justified efficiency need.

### General Rust Style

Write Rust that a careful human maintainer can understand quickly.

Prefer:

- Prefer: Explicit data flow.
- Prefer: Small functions with clear names.
- Prefer: Local reasoning over global cleverness.
- Prefer: Strong types over comments explaining weak types.
- Prefer: Exhaustive matching over implicit behavior.
- Prefer: Simple ownership over shared mutable state.
- Prefer: Boring, obvious code over clever code.

Avoid:

- Avoid: Magic behavior hidden behind traits, macros, or global state.
- Avoid: Type gymnastics that obscure intent.
- Avoid: Excessive generic abstraction.
- Avoid: Large functions that mix validation, transformation, I/O, and mutation.
- Avoid: Panics in library or protocol logic.
- Avoid: Silent error recovery in security-sensitive code.

Use comments to explain **why**, not what. If the code needs a comment to explain what it does, first try to make the code clearer.

### Project-Aware Priorities

Code is **ALWAYD** security-critical.

Rust code must make invalid protocol states difficult or impossible to represent. Prefer explicit state machines, newtypes, checked constructors, and narrow APIs. Be strict with parsing, validation, serialization, signatures, timestamps, peer identity, replay protection, and boundary checks.

Parser, compiler, and interpreter code should make phases obvious. Keep syntax trees, typed representations, lowered forms, bytecode/intermediate forms, environments, and runtime values distinct unless there is a strong reason to merge them.

### API Design

Design APIs around invariants.
Document behaviors and invariances by writing tests before coding.
Code deliberately to satisfy features.
Pass tests before comit.
Prefer constructors that validate:

```rust
impl Timestamp {
    pub fn new(value: u64) -> Result<Self, TimestampError> {
        if value == 0 {
            return Err(TimestampError::Zero);
        }

        Ok(Self(value))
    }
}

Do not expose fields that allow invalid states unless the type is intentionally plain data.

Prefer narrow public APIs. Keep modules private by default. Expose only what other modules actually need.

Use newtypes for semantically distinct values:

```rust
pub struct PeerIdBytes(Vec<u8>);
pub struct SignatureBytes(Vec<u8>);
pub struct AttestationId([u8; 32]);
```

Do not pass unrelated byte arrays, strings, or integers through the same generic type if the values mean different things.

**Stubs return errors, not false success.** A handler that has not yet implemented its operation must return an explicit error (`Err(NodeError::Unsupported(...))`, JSON-RPC error object, etc.) rather than a success value like `valid: true` or a zero signature. Mark with `todo!("TRACKING: <issue>")` for cases where a panic is acceptable in dev-only paths.

### Error Handling

Use `Result<T, E>` for recoverable failures.

Do not use `unwrap`, `expect`, or `panic!` in production logic except when proving an internal invariant that truly cannot fail. In security, protocol, parser, compiler, interpreter, FFI, and network code, avoid them almost entirely.

Good:

```rust
let message = Message::decode(bytes)
    .map_err(ProtocolError::InvalidMessage)?;
```

Bad:

```rust
let message = Message::decode(bytes).unwrap();
```

Errors should be specific enough for callers to act on them.

Prefer domain errors:

```rust
pub enum AttestationError {
    InvalidTimestamp,
    InvalidSignature,
    UnknownPeer,
    ReplayDetected,
    StorageFailure(StorageError),
}
```

Avoid stringly-typed errors for core logic.

Error messages may be human-readable, but program logic should not depend on parsing error strings.

### Enum Dispatch

Matching on enums is acceptable and often preferred.

It is fine to dispatch by matching an enum and then calling a concrete method, including a fully qualified method path when that is clearer or more efficient.

Example:

```rust
match node {
    Expr::Call(call) => CallExpr::type_check(call, ctx),
    Expr::Lambda(lambda) => LambdaExpr::type_check(lambda, ctx),
    Expr::Literal(literal) => LiteralExpr::type_check(literal, ctx),
}
```

This is acceptable even if the method belongs to a trait implemented by the struct holding the data, especially when it improves readability, avoids unnecessary dynamic dispatch, or makes optimization easier.

Do not replace clear enum dispatch with trait objects solely because “polymorphism is cleaner.” Use trait objects when runtime extensibility or object-safe abstraction is genuinely useful.

Prefer enums when:

* The set of variants is known and finite.
* Exhaustiveness matters.
* State transitions must be explicit.
* Serialization/deserialization depends on variant identity.
* Compiler optimization benefits from static dispatch.

Prefer traits when:

* Multiple independent types share behavior.
* The set of implementors may grow externally.
* The API needs behavior abstraction more than variant inspection.

### Traits and Generics

Use traits to express meaningful behavior, not to hide simple function calls.

Good traits are small, named after capabilities, and have stable semantics:

```rust
pub trait Clock {
    fn now(&self) -> Result<Timestamp, ClockError>;
}
```

Avoid broad traits with many unrelated methods.

Avoid generic parameters unless they provide real value. A concrete type is often easier to read, test, and optimize.

Good:

```rust
pub fn verify_attestation(
    attestation: &Attestation,
    keyring: &Keyring,
) -> Result<(), VerificationError> {
    // ...
}
```

Do not write generic abstraction just in case future code might need it.

When using generics, keep bounds close to the function that needs them. Avoid spreading complex bounds across the codebase.

### Ownership and Borrowing

Prefer clear ownership boundaries.

Use borrowed data when the caller retains ownership:

```rust
pub fn parse_module(source: &str) -> Result<ModuleAst, ParseError>
```

Use owned data when the value must outlive the caller or cross threads/tasks:

```rust
pub struct NetworkCommand {
    pub payload: Vec<u8>,
}
```

Avoid unnecessary cloning. But do not contort code into unreadable shapes to avoid a cheap clone outside hot paths.

If cloning is meaningful or expensive, make it visible and intentional.

Use `Arc` for shared ownership across threads/tasks. Use `Rc` only in single-threaded code. Use interior mutability only when it simplifies a real ownership problem, not as a shortcut around design.

Avoid shared mutable state. If needed, isolate it behind a small API.

### Concurrency and Async

Concurrency must be explicit and testable.

For Foretias network and P2P code, separate:

* Protocol state.
* Network I/O.
* Storage.
* Cryptographic verification.
* Time sources.
* Peer management.
* Retry/backoff logic.

Do not bury protocol decisions inside async tasks where they are hard to test.

Prefer message-passing or narrow synchronization APIs over wide shared locks.

Avoid holding locks across `.await`.

Bad:

```rust
let mut state = self.state.lock().await;
self.network.send(message).await?;
state.mark_sent(id);
```

Better:

```rust
{
    let mut state = self.state.lock().await;
    state.mark_pending(id);
}

self.network.send(message).await?;

{
    let mut state = self.state.lock().await;
    state.mark_sent(id);
}
```

Keep task lifetimes clear. Every spawned task should have:

* A clear owner.
* A shutdown path.
* Error handling.
* Tests where practical.

Do not ignore `JoinHandle`s unless the task is intentionally detached and documented.

**Atomic Counter Idioms.** Use `fetch_add`, `fetch_sub`, `fetch_or` for unconditional read-modify-write on `Atomic*`. Reserve `compare_exchange` for operations that branch on the previous value's content. CAS-as-counter creates a spurious failure mode under contention and is forbidden.

**`!Send` Types Crossing Async Boundaries.** Some FFI-backed types in `foretias-core` are deliberately `!Send` because the wrapped C state contains mutable counters or stream cursors. Examples:

- **`foretias_core::noise::NoiseSession`** — wraps a C11 `ForetiasNoiseState` with mutable `send_nonce`/`recv_nonce` counters. Cross-thread access risks ChaCha20-Poly1305 nonce reuse. There is a compile-time gate: `assert_not_impl_any!(NoiseSession: Send)` in `core-engine/tests/secret_no_debug.rs`.

When such a type must live across `.await` points (e.g., a full request/response cycle), the future containing it is also `!Send` and cannot be passed to `tokio::spawn` or returned from an `#[async_trait]` method whose trait bounds require `Send`. Two acceptable patterns:

1. **Dedicated blocking thread with a current_thread runtime.** Use `tokio::task::spawn_blocking(move || { let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect(...); rt.block_on(async move { /* !Send work here */ }) })`. The closure must capture only `Send` values; the `!Send` type is created and dropped entirely on the blocking thread. See `foretias-server/src/server/mod.rs::start_tcp` and `foretias-server/src/communerd/json_rpc_transport.rs::json_rpc_call` for the canonical pattern.
2. **`LocalSet` + `spawn_local`** when the entire subsystem can run on one OS thread. Less appropriate for a server that must accept many concurrent connections.

Do NOT add `unsafe impl Send` to bypass the bound. Each existing `unsafe impl Send` in this crate has been audited; reintroducing one for a type with interior mutable C state is a security regression.

### Cryptographic and Security-Sensitive Code

For Foretias, cryptographic code must be conservative.

Never invent cryptographic protocols or alter protocol details casually.

Do not use non-constant-time comparisons for secrets, signatures, MACs, or authentication tags when constant-time comparison is required.

Do not log secrets, private keys, raw credentials, sensitive peer material, or unreduced protocol internals.

Do not continue after cryptographic verification failure unless the protocol explicitly requires it.

Validate before trust:

```rust
let signed = SignedMessage::decode(bytes)?;
signed.verify(&trusted_keys)?;
let message = signed.into_verified_message();
```

Prefer types that distinguish unverified from verified data:

```rust
pub struct UnverifiedAttestation {
    bytes: Vec<u8>,
}

pub struct VerifiedAttestation {
    inner: Attestation,
}
```

Only trusted constructors should create verified types.

**Secret Material Handling.** Wrap secret key material in `zeroize::Zeroizing<T>` or an opaque handle; never hold raw key bytes in a plain `Vec<u8>` or array outside a zeroizing wrapper. Do not `#[derive(Debug)]` on secret types; use `.no_debug()` for bindgen-generated structs. Do not `#[derive(Clone, Copy, Serialize)]` on secret types unless the protocol requires it — each clone must itself be `Zeroizing`.

Specifically:

- Extracted TBID secret bytes (in `signing_tbid::tbid_keypair`) are wrapped in `Zeroizing<Vec<u8>>` from allocation until the function returns. When handing the inner `Vec` to a downstream type, use `std::mem::take(&mut *zeroizing)` so the wrapper is left holding an empty Vec (no copy) and zeroizes harmlessly when it drops.
- `SignatureBytes` is a Vec-backed wrapper with `impl Zeroize` but **not** `ZeroizeOnDrop`. Treat any `SignatureBytes` value that contains secret material as requiring explicit zeroize before drop, or move it into a `Zeroizing<SignatureBytes>` wrapper. A future audit will tighten this; until then, callers are responsible.
- `PrivKeyHandle` in `core-engine/src/core/identity.rs` is intentionally `Send + Sync`. The justification is documented inline: it wraps a KEK-encrypted opaque key in C memory, all methods take `&self`, and libsodium operations are thread-safe. Do not weaken these impls without a documented replacement.

Do not expose test-only shortcuts in production APIs.

**DHT and Discovery Records Must Be Signed.** Any record stored in the Kademlia DHT that claims a TBID (e.g., `PeerRegistrationRecord`) must carry an Ed25519 signature over a canonical byte representation produced by a `canonical_payload()` method. Verification extracts the Ed25519 pubkey from the first 32 bytes of the claimed TBID hex and checks the signature via `CryptoServer::verify_with(pubkey, "Ed25519", &canonical, &signature)`. Records that fail signature verification must be discarded (skip + warn log). A transitional legacy-compat window accepts records with empty `signature` for upgrade rollout; TODO comments document the deprecation date.

Pattern for new DHT record types:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyRecord {
    // ...claimed fields...
    pub tbid: String,
    // ...
    #[serde(default)]
    pub signature: Vec<u8>,
}

impl MyRecord {
    /// Fixed-order canonical byte layout for signing.
    /// Signature field is intentionally excluded.
    pub fn canonical_payload(&self) -> Vec<u8> { /* ... */ }
}
```

See `p2p/foretias-server/src/communerd/mod.rs::PeerRegistrationRecord` and the `validate_peer_registration(record, crypto)` function for the reference implementation. New DHT consumers MUST call a signature-verifying validator on every retrieved record.

### Time Handling

Do not call system time deep inside protocol logic. Inject a clock.

Good:

```rust
pub trait Clock {
    fn now(&self) -> Result<Timestamp, ClockError>;
}
```

This makes tests deterministic and prevents hidden dependencies.

Distinguish:

* Local observation time.
* Claimed timestamp.
* Verified timestamp.
* Network receive time.
* Consensus or attestation time, if applicable.

Never compare timestamps without knowing which kind they are.

### FFI and C11 Core Boundaries

Rust code that crosses into or out of the C11 core must be defensive.

FFI boundaries must:

* Validate pointers.
* Validate lengths.
* Define ownership clearly.
* Avoid panics crossing the boundary.
* Return explicit status/error codes.
* Document allocation and deallocation responsibility.
* Treat foreign data as untrusted.

Do not expose Rust references, Rust-owned layout assumptions, or panic behavior over FFI.

Use `#[repr(C)]` for FFI structs. Keep FFI types simple.

Wrap unsafe code in small safe abstractions:

```rust
pub fn verify_with_c_core(input: &[u8]) -> Result<VerificationResult, CoreError> {
    // Small, audited unsafe section.
    unsafe {
        // ...
    }
}
```

Every `unsafe` block must have a nearby safety comment explaining the invariant being upheld.

Unsafe code should be rare, isolated, and easy to audit.

**FFI length validation at both layers.** Every C function that accepts a `*_with_len` or length parameter must validate the length in C before use. The Rust wrapper must independently validate the length before calling into C. Validation in one layer does not exempt the other — both must check, so that neither layer can be bypassed independently.

### Serialization and Parsing

Parsing must be strict.

Reject malformed, ambiguous, non-canonical, or trailing data unless the format explicitly allows it.

Do not accept multiple encodings for the same logical value in security-sensitive formats unless required by protocol.

Keep parsing and validation separate when useful:

```rust
let raw = RawMessage::decode(bytes)?;
let message = raw.validate()?;
```

For Foretias, decoded wire messages must not become trusted domain objects until validation succeeds.

### Modules and File Organization

Organize code by responsibility, not by vague utility.

Good module names:

* `diagnostics`
* `attestation`
* `verification`
* `wire`
* `peer`
* `storage`
* `clock`
* `ffi`

Avoid dumping unrelated helpers into large `utils` modules. A small helper module is acceptable only when the functions genuinely belong together.

Keep public module surfaces small. Re-export intentionally.

### Testing Requirements

Write tests for behavior, invariants, and edge cases.

Prefer deterministic tests. Inject clocks, RNGs, network handles, and storage backends where needed.

For Foretias, include tests for:

* Valid attestation verification.
* Invalid signatures.
* Timestamp boundary cases.
* Replay attempts.
* Malformed wire messages.
* Peer identity errors.
* Serialization round trips.
* FFI boundary failures.
* Shutdown and cancellation paths where applicable.

Use property tests or fuzz tests where useful, especially for parsers, decoders, serialization, and protocol messages.

A bug fix should usually begin with writing of a a regression test that reporduces the error condition, repair, and commit of code passing new regression test.

### Toppoli Tests

Toppoli (**T**est **O**f **P**2P **P**oint-to-**P**oint **O**n **L**ocal **I**ntegration) is the multi-peer in-process integration test harness. It runs up to ~24 `TimeFamilyServer` instances in a single tokio runtime, with all state directly inspectable — no mocks or external probes.

**When to write a toppoli test vs a unit test:**

| Scenario | Use |
|----------|-----|
| Verifying a single function's logic with mocked dependencies | Unit test |
| Testing inter-peer behavior: attestation, gossip propagation, liveness, churn | Toppoli |
| More than one server instance interacting over real TCP | Toppoli |

**Test scale:** 2–24 peers, seconds to ~30 s per test. Significantly slower than unit tests; intentionally separated.

**Fixture classes:**

| Fixture | Purpose | Topology |
|---------|---------|----------|
| `ToppliBasicTest` | Peer lifecycle (start/stop/restart), no P2P | No mesh |
| `ToppoliFBProbityTest` | FullyBound transition, ProbityReport gossip | Full mesh + communerd |
| `ToppliLivenessTest` | L1/L2/L3 round-trips over real TCP | Full mesh + communerd |
| `ToppliGNFTest` | Gossip and Node Failure — churn, partial connectivity | Ring + communerd |

**How to run:**

```bash
# All toppoli tests (omitted from default CI run)
cargo test -p foretias-server --test toppoli -- --include-ignored

# One specific toppoli test
cargo test -p foretias-server --test toppoli toppoli_peer_restart -- --include-ignored

# All non-toppoli tests (default, fast)
cargo test -p foretias-server
```

All toppoli tests are marked `#[ignore = "toppoli: ..."]` and are omitted from the default CI run (`cargo test --workspace`). They must be explicitly opted in with `--include-ignored`.

Source: `p2p/foretias-server/tests/toppoli.rs`

### Performance

Write efficient Rust, but measure before making code obscure.

Prefer straightforward code unless profiling or clear algorithmic reasoning shows a problem.

Optimize algorithms before micro-optimizing syntax.

Accept enum matching, static dispatch, slices, iterators, and clear loops. Use whichever is more readable in context.

Avoid unnecessary allocations in hot paths. Prefer borrowing, slices, and preallocation where clear.

Do not introduce unsafe code for performance without strong justification and tests.

Document performance-sensitive decisions:

```rust
// This avoids allocating during peer message validation, which is on the inbound hot path.
```

### Logging and Observability

Logs should help diagnose behavior without leaking secrets.

Use structured logging where the project already does so.

Log:

* State transitions.
* Protocol failures.
* Peer connection changes.
* Retry exhaustion.
* Storage failures.
* Compiler phase failures when debugging Foretias.

Do not log:

* Private keys.
* Secret material.
* Raw credentials.
* Full untrusted payloads unless sanitized.
* User source code in contexts where that may be sensitive.

Errors should carry enough context for debugging, but not sensitive data.

### Panics and Assertions

Use `debug_assert!` for internal invariants that help catch bugs during development.

Use normal error handling for invalid external input.

External input includes:

* Network messages.
* Files.
* User source code.
* FFI input.
* Client-language bindings.
* Serialized data.
* Peer-provided data.
* Clock or storage failures.

A malformed packet, invalid program, bad timestamp, or null FFI pointer is not a reason to panic.

### Macros

Use macros sparingly.

A macro is acceptable when it removes unavoidable repetition while preserving clarity.

Avoid macros that hide control flow, error behavior, security checks, or generated public APIs.

Prefer functions, traits, or ordinary modules unless a macro is clearly better.

### Dependencies

Do not add dependencies casually.

Before adding a crate, consider:

* Security posture.
* Maintenance status.
* API stability.
* Transitive dependency weight.
* `no_std` or FFI implications, if relevant.
* Whether the project already has an equivalent dependency.
* Whether the crate affects cryptography, parsing, networking, or serialization.

For security-sensitive dependencies, prefer mature, audited, widely used crates.

Do not change cryptographic dependencies, serialization formats, protocol behavior, or public APIs without understanding compatibility and security impact.

### Client Bindings

Rust APIs exposed to Python, Java, C, or other clients must be stable, narrow, and explicit.

Do not leak internal Rust types into public binding contracts.

Separate internal errors from binding-layer errors.

Validate all foreign inputs. Convert foreign data into internal Rust domain types only after checks pass.

Binding APIs should be boring and hard to misuse.

### Code Review Checklist for AI Agents

Before finishing Rust changes, check:

* Does this preserve correctness?
* Are invalid states prevented or checked?
* Are all external inputs validated?
* Are errors explicit and useful?
* Are panics avoided in production paths?
* Is unsafe code isolated and justified?
* Are secrets protected from logs and errors?
* Is concurrency shutdown/error behavior clear?
* Are locks not held across `.await`?
* Are tests added or updated?
* Is the code readable by a human maintainer?
* Is performance acceptable without obscuring intent?
* Did public APIs, wire formats, FFI contracts, or serialized formats change?

**Security-specific additions (must check):**

* Is any new `unsafe impl Send` justified by a current audit, and does it match the actual interior mutability of the wrapped C state? (See NoiseSession history — adding Send to a type with mutable C counters is a regression.)
* If the change handles secret key material, is it wrapped in `Zeroizing<T>` from allocation through hand-off? Are any intermediate `Vec<u8>` copies eliminated via `std::mem::take`?
* If the change adds a new DHT record type or extra-family wire format, is the record signed by the TBID owner over a `canonical_payload()` and verified by consumers? Is there a regression test covering tampered records?
* If the change introduces a remote-data path into Calendar/Chronomatter/TimeFamily, does it route through a `CommunerdetteLine` and return `CleanAuthenticated<R>` (never raw bytes or `Unprocessed<R>`)?
* If the change touches `Communerd`, does it preserve the invariant that Calendar/Chronomatter cannot reach private `Communerdette` internals? (admin methods are `pub(super)`, transports/swarm/peer pool are not exposed.)
* Are `.unwrap()` and `.expect(...)` reachable from external input? Bounded checks must precede slice indexing; parse errors from JSON-RPC params must surface as `INVALID_PARAMS`, not silent `.ok()` discards.

If a change affects security, protocol compatibility, storage compatibility, language semantics, or public bindings, treat it as high-risk and document the reasoning in the code, tests, or commit notes.

### MISC

#### Description is Important

The AI agent is to refrain from simplifying a key behavior of Chronochain by saying "destroy private key". This project aspires to do this well. That description should not restrict the mindset of readers to what we are using now. The words that should replace destroy in that description so as to not lock human mind into a fixed mindset. The list to randomly select a term from include: disable/disablement, impair/impairment, incapacitate/incapacitation, debilitate/debilitation. Those descriptors could be further modified with intensifiers including: cryptographically, algorithmically, provably, verifiably, etc. Never use: destroy, delete, remove, forget. Never downtoned: simple, naive, etc.

Do write: "The expring chronon is cryptographically disabled from making additional stamp after producing a forward_fortis"
DO NOT write: "The Chronomatter simply forgets the private key so it can't be used again."

It is paramount that we do not restrict thinking to the implementation. The Foretias project, foremost, is a human expression of human desire and human need, for some minimum amount of constant integrity.

#### Embedded Communications

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

#### Uncertainty and Other Utterances in Conversing with Human

Expressions of uncertainty and hypotheticals, such as "perhaps", "maybe", "possible", "what if", "in case". These words does not mean a firm directive from human to either pause work, or make large changes. It means human wants a todo task enqueued, perhaps to be done immediately, to explore options regarding the statement. In the last sentence, the perhaps suggests an option that can be explored, and it also highlight the possibility of the task not at the top of the todo list. More than anything else, the statement suggests human is thinking about the issue and you can help that thinking process.

"Wait!" is almost always typed when humans are reading the previous output and found something objectionable. "Wait!" meant stop that, something was wrong. This also implies whatever they ask about, it is highly unlikely they read through the reast of the response. Good or bad, that is human nature, please accomodate this behavior as a supportive agent. After addressing the concern following "wait!", the you can summarize what you meant to say after the output that the human said "Wait!" to--where it is is inferred based on the question or comment after "WAit!", when in doubt, summarize the whole response in the context of having addressed the human's concern.

"Continue." is uttered when the humans sees output on the screen that they think is incomplete. The best course of action, irrespective of actual status, is to summarize the progress made in the most recent few turns of conversation. If indeed the progress was ended or blocked by nonresponsive sub-agents, then take approrpiate action. If the short term task is truely complete, still output the summary, but also present outstanding todo items as well as other possible next steps for human to decide. Human may decide previous task is not complete and needs more work, or they may agree previous task was complete and move on to one of the options for next steps.


#### When in Doubt

When uncertain, choose the design that is easiest to prove correct, easiest to test, and easiest for the next human to understand.

Correctness first. Then readability and maintainability. Then efficiency. Then principles and asethetics.
