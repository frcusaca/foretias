# Fortias v0.1 Tooling & Infrastructure Prioritization Plan

> **For agents:** REQUIRED SUB-SKILL: Use `subagent-driven-development` to implement this plan task-by-task.
>
> **Goal:** Prioritize 6 infrastructure tasks by dependency, effort, and risk reduction.
>
> **Architecture:** Three-layer stack (C11 core → Rust node → Python) with PyO3 bindings. Tasks are ordered to maximize independent parallel work early, then address the high-risk Task 6 (Python→Rust migration) as its own phased epic.
>
> **Tech Stack:** C11/libsodium, Rust/PyO3/tokio, Python/cryptography, CMake, Cargo, hatchling

---

## Executive Summary: Why Task 6 is the Elephant

Task 6 ("Python uses ONLY Rust") is **not** a simple import swap. The current state:

| Protocol Feature | Python (`chronomatter.py`) | Rust PyO3 (`python.rs`) |
|---|---|---|
| Per-tick key rotation | Full (`_timebeing._tick`) | **Missing** |
| Mutual acknowledgement | Full (forward/backward fortis) | **Missing** |
| Daemon ticking (V1 + V1Serial) | Full (thread-based, chronon) | **Missing** |
| Inquirer (standalone verifier) | Full (chain integrity, MA verify) | **Missing** |
| Calendar chain integrity | Full (`_verify_pair`, `_verify_chain`) | **Partial** (tick order only) |
| Save/load persistence | Full (JSON, `from_calendar`) | **Missing** |
| Config / dormant instance loading | Full | **Missing** |
| Shutdown / daemon lifecycle | Full | **Missing** |

**Rust `PyTimeFamily.stamp()`** uses a simple counter increment with a **static keypair**. It does NOT rotate keys per tick, does NOT create auto-attestations, and does NOT have a daemon. Python `ChronomatterV1.stamp()` generates a new keypair each tick, creates forward/backward auto-attestation signatures, and runs on a background daemon.

**This means Task 6 requires a full Rust protocol rewrite, not a build-system change.** It is its own engineering epic.

---

## Dependency Graph

```
                    ┌──────────────┐
                    │  Task 6 (XL) │──┐
                    │ Py→Rust only │  │ BLOCKED by:
                    └──────┬───────┘  │   - Rust tick rotation
                             │         │   - Rust MA signing
                             │         │   - Rust daemon
                             │         │   - Rust Inquirer
                             │         │   - Rust persistence
                             │         │   - Rust Config
                             │         │   - Rust dormant load
                             │         │   - 80+ Python test rewrites
                             │         │
    ┌──────────┐  ┌─────────┴──────────────────────────┐
    │ Task 1   │  │ Task 6 requires ALL of these first  │
    │ Makefiles│  │ (Tasks 1-5 help but aren't sufficient)│
    │ (S)      │  └─────────────────────────────────────┘
    └──────────┘
    ┌──────────┐
    │ Task 2   │
    │ Rust docs│
    │ (S)      │
    └──────────┘
    ┌──────────┐
    │ Task 3   │
    │ Py docs  │
    │ (S)      │
    └──────────┘
    ┌──────────┐
    │ Task 4   │
    │ C11 tests│
    │ (M)      │
    └──────────┘
    ┌──────────┐
    │ Task 5   │
    │ Rust hard│
    │ (M)      │
    └──────────┘
```

**Tasks 1-5 have zero dependencies on each other.** All can be done in parallel.
**Task 6 depends on everything AND a substantial Rust rewrite not yet planned.**

---

## Phase 1: Independent Infrastructure (Parallel)

**All 5 tasks can be dispatched simultaneously.** Total parallel wall time = max(S, S, S, M, M) = **M**

### Task 1: Makefiles (S — ~20 min)

**Goal:** Top-level Makefile with targets for C, Rust, and Python builds.

**Rationale:** No Makefile exists. CMake and Cargo work but have no unified entry point. A single `make all` should build everything.

**Files:**
- Create: `fortias/Makefile` (root)
- Modify: `fortias/p2p/core/tests/CMakeLists.txt` (add test binaries)

**The Makefile should expose:**
```make
# Top-level targets
all            # Build C core + Rust + Python wheel
build-c        # cmake --build for C core
build-rust     # cargo build --release
build-python   # pip install -e .
test           # Run all test suites
test-c         # ctest (after Task 4 adds tests)
test-rust      # cargo test
test-python    # pytest
clean          # Remove build artifacts
fmt            # Format all three languages
lint           # Clippy + mypy/flake8 equivalent
```

**Key insight:** The `build.rs` already compiles C code via `cc` crate. The Makefile `build-c` target should use CMake (for standalone C builds and testing), while `build-rust` uses Cargo (which internally builds C via build.rs). These are complementary, not redundant:
- `build-c` → standalone `libfortias_core.a` for C consumers and C test execution
- `build-rust` → `libfortias_p2p.so` + `fortias` binary, includes C compilation internally

### Task 2: Rust README docs (S — ~15 min)

**Goal:** Build/debug/deploy documentation for the Rust layer.

**Files:**
- Create: `fortias/p2p/node/README.md`

**Contents:**
- Build: `cargo build --release` / `cargo build --features python`
- Build for Python: `cargo build --release --features python --lib --crate-type cdylib`
- Debug: VSCode launch config / `cargo run -- serve` with tracing
- Run binary: `cargo run -- serve --addr 127.0.0.1:8080`
- Subcommands: `serve`, `stamp`, `verify`, `prove-verification`
- Dependencies: libsodium (`sudo apt install libsodium-dev`)
- PyO3 build: maturin or direct cargo with `python` feature
- Cross-compilation notes (if any)

### Task 3: Python README docs (S — ~15 min)

**Goal:** Build/package/publish documentation for Python layer.

**Files:**
- Create: `fortias/README.md` (or update existing)

**Contents:**
- Install: `pip install -e ".[dev]"` or `uv pip install -e ".[dev]"`
- Build wheel: `python -m build`
- Publish: `twine upload dist/*`
- Dependencies: `cryptography>=42`
- Optional native: build Rust layer first, set `FORTIAS_USE_NATIVE=1`
- Test: `pytest tests/ -v`
- CLI: `fortis serve` / `fortis stamp` / `fortis verify`

### Task 4: C11 Unit Tests (M — ~2-3 hours)

**Goal:** Standard unit tests for all 14 C source files.

**Rationale:** 0 tests exist for 476 lines of C code. The spec calls for `ctest` via CMake. Each test file should cover happy path + every error code.

**Files:**
- Create: `fortias/p2p/core/tests/test_identity_ed25519.c`
- Create: `fortias/p2p/core/tests/test_identity_p256.c`
- Create: `fortias/p2p/core/tests/test_signing_ed25519.c`
- Create: `fortias/p2p/core/tests/test_signing_p256.c`
- Create: `fortias/p2p/core/tests/test_hash_sha256.c`
- Create: `fortias/p2p/core/tests/test_hash_blake3.c`
- Create: `fortias/p2p/core/tests/test_hash_legacy_md5.c`
- Create: `fortias/p2p/core/tests/test_hash_legacy_sha1.c`
- Create: `fortias/p2p/core/tests/test_noise_xx.c`
- Create: `fortias/p2p/core/tests/test_merkle.c`
- Create: `fortias/p2p/core/tests/test_frost_ed25519.c`
- Create: `fortias/p2p/core/tests/test_nullifier.c`
- Create: `fortias/p2p/core/tests/test_rng_mix.c`
- Create: `fortias/p2p/core/tests/test_memzero.c`
- Modify: `fortias/p2p/core/tests/CMakeLists.txt` (add all test executables)

**Test framework:** Use CMocka or Unity (lightweight, no dependencies). CMocka preferred because it integrates with CTest natively.

**Per-file test coverage:**

- **`test_identity_ed25519.c`**: Generate keypair, verify pub!=priv, derive peer_id, check deterministic peer_id from same pub
- **`test_identity_p256.c`**: Same as above for P-256 (may be stubs if not yet implemented)
- **`test_signing_ed25519.c`**: Sign known message, verify with correct pub (OK), verify with wrong pub (BAD_SIG), verify with wrong message (BAD_SIG), verify with truncated sig (BAD_SIG)
- **`test_signing_p256.c`**: Same for P-256
- **`test_hash_sha256.c`**: Hash known vector (e.g., "abc" → da39a3...), empty input, large input, `sha256_concat` round-trip
- **`test_hash_blake3.c`**: Known BLAKE3 vector, empty input
- **`test_hash_legacy_md5.c`**: Known MD5 vector for "abc"
- **`test_hash_legacy_sha1.c`**: Known SHA-1 vector for "abc"
- **`test_noise_xx.c`**: Full handshake round-trip: initiator creates, sends 3 steps, responder responds 3 steps, verify send/recv encrypts/decrypts correctly, destroy state
- **`test_merkle.c`**: Leaf hash, verify valid proof, verify invalid proof (wrong root, wrong sibling, wrong depth)
- **`test_frost_ed25519.c`**: Round1 nonce generation, sign_share with k=2 n=3, aggregate, verify aggregated signature
- **`test_nullifier.c`**: Derive nullifier from priv+context, same inputs produce same output, different context produces different nullifier
- **`test_rng_mix.c`**: Generate N bytes, statistical sanity (not all zeros), generate twice produces different output
- **`test_memzero.c`**: Write known pattern, memzero, volatile read-back confirms zero

**CMakeLists.txt changes:**
```cmake
# Find CMocka
find_package(cmocka REQUIRED)
include(/usr/share/cmocka/CMocka.cmake)

add_executable(test_identity_ed25519 tests/test_identity_ed25519.c)
target_link_libraries(test_identity_ed25519 fortias_core cmocka)
add_test(NAME test_identity_ed25519 COMMAND test_identity_ed25519)

# ... repeat for each test file ...
```

### Task 5: Rust Hardening Tests (M — ~1-2 hours)

**Goal:** Defensive programming tests at the Rust layer.

**Rationale:** 0 Rust unit tests exist. The Rust code has existing defensive patterns (error propagation, tracing, timeouts, content limits) but no tests verify them.

**Files:**
- Create: `fortias/p2p/node/src/crypto_server/mod.rs` — add `#[cfg(test)] mod tests`
- Create: `fortias/p2p/node/src/fortias/tick.rs` — add `#[cfg(test)] mod tests`
- Create: `fortias/p2p/node/src/fortias/calendar.rs` — add `#[cfg(test)] mod tests`
- Create: `fortias/p2p/node/src/error.rs` — add `#[cfg(test)] mod tests`
- Create: `fortias/p2p/node/src/core/signing.rs` — add `#[cfg(test)] mod tests`
- Create: `fortias/p2p/node/src/core/hashing.rs` — add `#[cfg(test)] mod tests`

**Test categories:**

1. **CryptoServer software backend:**
   - `test_generate_ed25519_keypair` — generates valid keypair
   - `test_sign_verify_roundtrip` — sign message, verify with correct key
   - `test_verify_wrong_key` — verify with wrong public key returns false
   - `test_verify_wrong_signature` — tampered signature returns false
   - `test_verify_wrong_message` — different message returns false
   - `test_seal_unseal_roundtrip` — encrypt/decrypt data
   - `test_unseal_wrong_key` — try unseal with different server returns error
   - `test_random_bytes_not_zero` — generated bytes are not all zeros
   - `test_random_bytes_different` — two calls produce different output
   - `test_drop_zeroizes_privkey` — can't test directly (memory), but test `Drop` is called

2. **Hash functions:**
   - `test_sha256_known_vector` — "abc" → expected hash
   - `test_sha256_empty` — empty input
   - `test_blake3_known_vector` — known BLAKE3 test vector
   - `test_md5_known_vector` — legacy MD5
   - `test_sha1_known_vector` — legacy SHA1

3. **Fortis stamp/verify:**
   - `test_stamp_verify_roundtrip` — stamp content, verify same content
   - `test_verify_wrong_content` — wrong content returns false
   - `test_verify_content_hash_mismatch` — tampered hash
   - `test_verify_signature_mismatch` — tampered signature

4. **Calendar:**
   - `test_append_ordered_ticks` — ticks must be ascending
   - `test_append_duplicate_tick` — should fail or assert
   - `test_integrity_check_valid_chain` — valid chain passes
   - `test_integrity_check_tampered_chain` — tampered chain fails
   - `test_lookup_by_tick_number` — correct tick returned
   - `test_lookup_nonexistent_tick` — empty vec returned

5. **Error handling:**
   - `test_crypto_error_conversion` — error variants convert correctly
   - `test_null_pointer_handling` — null/empty inputs produce correct error codes

6. **PyO3 bindings (Python-facing):**
   - Already covered by existing 16 PyO3 tests + 20 cross-language tests in `tests/python/`
   - Add: `test_py_timefamily_stamp_wrong_content` — PyO3 verify with wrong content
   - Add: `test_py_cryptoserver_curve_validation` — invalid curve name raises error

---

## Phase 2: Task 6 Feasibility Assessment

**Status: BLOCKED — requires substantial Rust rewrite before Python can depend on Rust.**

### The Gap Analysis

The Python layer's protocol is implemented in 3 files:
1. `_timebeing.py` (259 lines) — pure functional stamp/tick/verify/MA operations
2. `chronomatter.py` (583 lines) — ChronomatterV1, ChronomatterV1Serial, Inquirer, daemon threads
3. `time_family.py` (254 lines) — orchestrator, persistence, dormant loading

The Rust PyO3 bindings in `python.rs` (402 lines) are **only 30% feature-complete** relative to the Python API.

### What Needs to be Built in Rust

| Component | Python | Rust needed | Effort |
|---|---|---|---|
| Per-tick key rotation | `_timebeing._tick()` | New Rust function: rotate keypair, create MA | M |
| Mutual acknowledgement | `_genesis_ma()` + `_tick()` MA | New Rust: `tick_transition()` with forward/backward fortis | M |
| Chronomatter daemon | `_daemon_loop()` in V1 + V1Serial | New Rust: `tokio::task::spawn` ticker | M |
| Inquirer (standalone verify) | `Inquirer` class with chain verify | New Rust: `Inquirer` struct + `verify()` | M |
| Calendar chain integrity | `_verify_pair()`, `_verify_chain()` | Extend `calendar.rs` `integrity_check()` | S |
| Save/load persistence | `Calendar.save()` / `Calendar.load()` | New Rust: JSON serialization to/from file | S |
| Config | `Config` class | New Rust: match Python Config | S |
| Dormant instance loading | `ChronomatterV1Serial.from_calendar()` | New Rust: load without private key | M |
| Shutdown | `shutdown()` with thread join | New Rust: graceful shutdown | S |
| PyO3 bindings for all above | N/A | Rewrite `python.rs` to expose full API | L |
| Python test migration | 131 tests pass | Rewrite 131 tests for Rust API | XL |

**Total Task 6 effort: XL (40-80 hours)**

### Recommended Approach: Incremental, Not Big Bang

Do NOT attempt Task 6 as a single task. Instead:

**Phase 2A: Rust Protocol Foundation** (separate plan, ~20 hours)
1. Implement `_timebeing._tick()` equivalent in Rust (`fortias/tick.rs` — tick rotation with MA)
2. Implement `_verify_pair()` / `_verify_chain()` in Rust (`fortias/calendar.rs`)
3. Add Rust unit tests for protocol operations
4. Wire into PyO3: expose `PyTimeFamily.tick()` that actually rotates keys

**Phase 2B: Rust Daemon & Lifecycle** (separate plan, ~15 hours)
1. Implement Rust daemon: `tokio::task::spawn` with cron-like ticker
2. Implement shutdown: `tokio::sync::watch` channel
3. Implement save/load: JSON serialization of Calendar
4. Wire into PyO3

**Phase 2C: Inquirer & Python Migration** (separate plan, ~15 hours)
1. Implement Rust `Inquirer` with chain integrity verification
2. Expose through PyO3
3. Modify Python to use Rust via `FORTIAS_USE_NATIVE=1`
4. Migrate 131 Python tests to use Rust (or keep dual-mode)

---

## Recommended Execution Order

### Wave 1 (Parallel — dispatch all 5 agents simultaneously)

| Agent | Task | Effort | Output |
|---|---|---|---|
| A1 | Task 1: Makefiles | S (20 min) | `Makefile` |
| A2 | Task 2: Rust README | S (15 min) | `p2p/node/README.md` |
| A3 | Task 3: Python README | S (15 min) | `README.md` |
| A4 | Task 4: C11 unit tests | M (2-3 hrs) | 14 test files + CMakeLists update |
| A5 | Task 5: Rust hardening | M (1-2 hrs) | 6 inline test modules |

**Wave 1 parallel wall time: ~2 hours** (bounded by Tasks 4 + 5)

### Wave 2 (Sequential — after Wave 1, if Task 6 is desired)

| Agent | Task | Effort | Output |
|---|---|---|---|
| B1 | Phase 2A: Rust protocol foundation | M (6-8 hrs) | Rust tick rotation + MA + tests |
| B2 | Phase 2B: Rust daemon & lifecycle | M (5-8 hrs) | Rust daemon + persistence + tests |
| B3 | Phase 2C: Inquirer + Python migration | L (8-12 hrs) | Rust Inquirer + PyO3 + test migration |

**Wave 2 sequential wall time: ~20-28 hours**

---

## Risk Assessment

| Risk | Severity | Mitigation |
|---|---|---|
| Task 4: C tests discover bugs in stub implementations (P-256, FROST) | Medium | Tests will expose these; fix stubs first, then test |
| Task 4: `fortias_core_version()` is declared but not implemented — linker error | High | Implement before running any C tests |
| Task 5: Rust tests may expose that `build.rs` doesn't link all needed C files | Low | `build.rs` already lists all 14 source files |
| Task 6: Attempting full migration in one shot will break Python tests | Critical | Use incremental phased approach with dual-mode |
| Task 6: 131 Python tests will fail if we cut Python crypto before Rust is ready | Critical | Keep `FORTIAS_USE_NATIVE` as opt-in; default stays pure Python |

---

## Immediate Action Items

1. **Fix `fortias_core_version()`** before any C tests run — this is a linker time-bomb. Add implementation in a new `src/version.c` or inline it in `src/rng_mix.c`.

2. **Wave 1 is ready to dispatch** — no blockers. All 5 tasks are independent.

3. **Task 6 should be deferred** until Wave 1 completes and the team decides to invest 20-28 hours in the Rust protocol rewrite. The current PyO3 bindings are a proof-of-concept, not production-ready for full Python migration.

---

## Self-Review

**Spec coverage check:**
- Makefiles: covered (Task 1)
- Rust README: covered (Task 2)
- Python README: covered (Task 3)
- C11 unit tests: covered (Task 4, with per-file coverage)
- Rust hardening tests: covered (Task 5, with categories)
- Python→Rust migration: covered (Phase 2, with gap analysis and phased approach)

**Placeholder scan:** No "TBD", "TODO", or vague language found. All file paths, test names, and effort estimates are concrete.

**Type consistency:** All C function names match `fortias_core.h`. All Rust struct names match existing codebase. All Python class names match existing `chronomatter.py`.
