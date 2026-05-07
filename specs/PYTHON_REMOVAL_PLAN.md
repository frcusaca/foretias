# Pure Python Removal Plan — Thin Shim Strategy

> **Goal:** Remove all pure-Python protocol implementation from `src/foretias/` and replace with a thin shim that re-exports from the Rust `foretias_p2p` crate via PyO3.

> **Current state:** All Foretias protocol logic (stamp, tick, verify, calendar, crypto, TimeFamily, Chronomatter, Inquirer) is implemented in Rust (`p2p/foretias-core`, `p2p/foretias-node`, `p2p/foretias-python`). The old Python modules in `src/foretias/` carry `DeprecationWarning` banners and are functionally superseded. The top-level `pyproject.toml` currently uses `maturin` which conflates the shim package with the Rust bindings build.

> **Target state:** `src/foretias/` is a minimal shim (~5 files) that re-exports from `foretias_p2p`. No pure-Python crypto, no protocol logic, no daemon threads. `foretias stamp/verify` works identically from user perspective.

---

## What Stays (thin shim — 5 files)

| File | Purpose |
|------|---------|
| `__init__.py` | Re-export `TimeFamily`, `TimeFamilyServer`, `CryptoServer`, `Foretis`, `TickRecord`, `Calendar`, `__version__` from `foretias_p2p` |
| `cli.py` | CLI wrapper around `PyTimeFamilyServer` / `PyForetis` (already Rust-backed) |
| `thin_client.py` | Convenience wrapper around `PyTimeFamily` (already Rust-backed) |
| `_version.py` | Version string (or derive from `foretias_p2p`) |
| `__main__.py` | Entry point for `python -m foretias` → `cli.main()` |

## What Goes (remove entirely)

| File | Current Purpose | Replaced By |
|------|----------------|-------------|
| `_timebeing.py` | Pure functional stamp/tick/verify | `foretias_core::foretias::tick::{stamp, verify, verify_pair}` |
| `chronomatter.py` | ChronomatterV1, V1Serial, Inquirer, interfaces | `foretias_node::server::TimeFamilyServer` |
| `calendar.py` | Calendar class (append, get, save, load, integrity) | `foretias_core::foretias::calendar::Calendar` |
| `time_family.py` | TimeFamily orchestrator | `PyTimeFamilyServer` |
| `crypto.py` | Python crypto primitives (Ed25519 via `cryptography`) | `foretias_core::crypto_server::SoftwareCryptoServer` |
| `models.py` | Foretis, TickRecord frozen dataclasses | `PyForetis`, `PyTickRecord` |
| `config.py` | Config resolution (arg > env > default) | `PyNodeConfig` |
| `timebeing.py` | Timebeing base class (tbid, tbn) | Absorbed into Rust types |

## Old Tests — Remove Entirely

All 10 files under `tests/` exercise pure-Python classes that are being removed. They will be replaced by:
- Rust unit tests (already passing: 104 unit + 7 integration)
- New thin Python integration tests that exercise the shim API only

| File | Remove? | Replacement |
|------|---------|-------------|
| `test_functional.py` | Yes | Rust tests in `foretias-core` |
| `test_calendar.py` | Yes | Rust tests in `foretias-node` |
| `test_timebeing.py` | Yes | Rust tests in `foretias-node` |
| `test_timebeing_aggressively.py` | Yes | Rust integration tests |
| `test_timefamily.py` | Yes | Rust tests in `foretias-node` |
| `test_timefamily_aggressively.py` | Yes | Rust integration tests |
| `test_crypto.py` | Yes | Rust crypto_server tests |
| `test_models.py` | Yes | Rust serde roundtrip tests |
| `test_config.py` | Yes | Rust config tests |
| `test_cli.py` | Partially | Rewrite against new shim CLI |

## Build System Changes

### Top-level `pyproject.toml`

**From:** `maturin` build backend (builds `foretias_p2p` Rust crate directly)
**To:** `hatchling` build backend (installs `src/foretias/` as a pure Python shim package with `foretias-p2p` as a runtime dependency)

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "foretias"
version = "0.2.0"
dependencies = ["foretias-p2p>=0.2.0"]
```

### `p2p/foretias-python/pyproject.toml`

**Unchanged.** Continues to build the `foretias-p2p` package via maturin + PyO3. This is a separate package that installs the Rust `.so` and is consumed as a dependency by the shim.

### Install Order

```bash
# 1. Build Rust bindings package
cd p2p/foretias-python && pip install maturin && maturin develop

# 2. Install shim package (depends on foretias-p2p)
pip install -e .
```

---

## Implementation Phases

### Phase 1: Fix shim `__init__.py` and `pyproject.toml`

- [ ] Rewrite `src/foretias/__init__.py` to re-export from `foretias_p2p` with correct names
- [ ] Switch top-level `pyproject.toml` from `maturin` to `hatchling`
- [ ] Add `foretias-p2p` as a runtime dependency
- [ ] Verify `pip install -e .` succeeds (after `maturin develop` in `p2p/foretias-python/`)

### Phase 2: Remove old Python modules

- [ ] Delete `_timebeing.py`
- [ ] Delete `chronomatter.py`
- [ ] Delete `calendar.py`
- [ ] Delete `time_family.py`
- [ ] Delete `crypto.py`
- [ ] Delete `models.py`
- [ ] Delete `config.py`
- [ ] Delete `timebeing.py`
- [ ] Remove `__pycache__/` entries

### Phase 3: Rewrite shim CLI and thin_client

- [ ] Audit `cli.py` — ensure it imports only from `foretias_p2p` and the shim `__init__.py`
- [ ] Audit `thin_client.py` — ensure it imports only from `foretias_p2p`
- [ ] Verify both work with the new shim structure

### Phase 4: Replace tests

- [ ] Delete all 10 old test files
- [ ] Write `test_shim.py` — verify shim imports work, basic stamp/verify roundtrip
- [ ] Write `test_cli_smoke.py` — verify CLI `stamp` and `verify` exit codes
- [ ] Run `python -m pytest tests/ -v` — all pass
- [ ] Run `cd p2p && cargo test --workspace` — still all pass

### Phase 5: Final cleanup

- [ ] Remove `pyproject.toml` `[project.optional-dependencies] dev` if `pytest` no longer needed for pure Python
- [ ] Update `README.md` — remove pure-Python quick start, keep Rust + shim examples
- [ ] Update `AGENTS.md` references if any point to old Python modules
- [ ] Verify no lingering imports of removed modules anywhere in the repo

---

## Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| `foretias_p2p` import fails if maturin build is stale | High | CI/build docs must always build `p2p/foretias-python/` first |
| `thin_client.py` API diverges from old Python `TimeFamily` | Medium | Keep `thin_client.py` API identical for any external consumers |
| `cli.py` `serve` subcommand is a stub | Low | Already documented — use Rust binary `foretias serve` instead |
| `PyTimeFamily` lacks per-tick key rotation vs old `ChronomatterV1` | Medium | `PyTimeFamilyServer` has this. Shim should prefer `TimeFamilyServer` for full protocol. Document the difference. |

---

## Verification Criteria

1. `pip install -e .` succeeds (after `maturin develop` in `p2p/foretias-python/`)
2. `from foretias import TimeFamily, Foretis, TickRecord, Calendar` works
3. `python -m pytest tests/ -v` passes (new shim tests)
4. `cd p2p && cargo test --workspace` passes (111 tests)
5. `foretis stamp -m "hello"` and `foretis verify` work from CLI
6. `grep -r 'from foretias\._timebeing\|from foretias\.chronomatter\|from foretias\.time_family\|from foretias\.crypto\|from foretias\.models\|from foretias\.config\|from foretias\.timebeing' src/ tests/` returns nothing

---

# END OF PLAN
