# Fortias → Foretias Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename "fortias" → "foretias" and "fortis" → "foretis" (case-preserving) across two repos in `/home/hcbusy/webhash/`.

**Architecture:** Two independent scopes (wdocs prose, code repo) processed in parallel-safe phases. Text replacements before file renames. Build directories and `.venv` excluded (regenerate on next build).

**Tech Stack:** `sed` for text replacements, `git mv` for file renames, `grep` for verification.

---

## Replacement Rules

All replacements are **case-preserving direct string** matches (no word boundaries):
- `Fortias` → `Foretias` (PascalCase prefix, e.g., `FortiasBehaviour` → `ForetiasBehaviour`)
- `fortias` → `foretias` (lowercase prefix, e.g., `fortias_p2p` → `foretias_p2p`, `fortias_core` → `foretias_core`)
- `FORTIAS` → `FORETIAS` (uppercase prefix, e.g., `FORTIAS_ERR_BAD_INPUT` → `FORETIAS_ERR_BAD_INPUT`)
- `Fortis` → `Foretis` (PascalCase prefix)
- `fortis` → `foretis` (lowercase prefix, e.g., `fortis_json` → `foretis_json`)
- `FORTIS` → `FORETIS` (uppercase prefix)

**Why no word boundaries (`\b`)?** The `_` underscore is a word character in regex, so `\bfortias\b` would NOT match `fortias_p2p`, `fortias_core`, `FORTIAS_ERR_BAD_INPUT`, etc. Since `fortias`/`fortis` only appear as project identifiers (not as English words), direct substring replacement is safe with zero false-positive risk. The replacement `s/fortias/foretias/g` catches `fortias`, `fortias_p2p`, `FortiasBehaviour` (via PascalCase rule), `fortias_core`, `FORTIAS_ERR_*`, and `fortias_core.h`.

---

## Exclusions (do NOT touch)

| Path | Reason |
|------|--------|
| `foretias/p2p/target/` | Build artifacts — regenerate |
| `foretias/p2p/core/build/` | CMake build tree — regenerate |
| `foretias/.pytest_cache/` | Test cache — regenerate |
| `foretias/src/fortias/__pycache__/` | Bytecode cache — regenerate |
| `foretias/tests/__pycache__/` | Bytecode cache — regenerate |
| `foretias/.git/` | Git history is historical truth |
| `CAeHius/webhash/.venv/` | Virtual env — regenerate |
| `foretias/.opencode/plans/` | Agent plans — optional, low priority |
| `CAeHius/webhash/.opencode/` | Agent config — optional |

---

## Task 1: Verify current state and create backup branches

**Files:** Both repos

- [ ] **Step 1: Verify both repos are clean**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash && git status
cd /home/hcbusy/webhash/foretias && git status
```

Expected: Clean working trees or known uncommitted changes (document them).

- [ ] **Step 2: Create backup branches**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash && git checkout -b pre-rename-backup
cd /home/hcbusy/webhash/foretias && git checkout -b pre-rename-backup
```

- [ ] **Step 3: Count baseline occurrences**

```bash
# Scope 1: wdocs
grep -r -c -i 'fortias\|fortis' /home/hcbusy/webhash/CAeHius/webhash/ --include='*.md' --include='*.py' 2>/dev/null | grep -v ':0$' | wc -l

# Scope 2: foretias code (excluding build artifacts)
cd /home/hcbusy/webhash/foretias && grep -r -c -i 'fortias\|fortis' --include='*.rs' --include='*.c' --include='*.h' --include='*.py' --include='*.toml' --include='*.md' --include='*.txt' --include='*.cmake' --include='CMakeLists.txt' . 2>/dev/null | grep -v ':0$' | grep -v 'target/' | grep -v 'build/' | grep -v '.pytest_cache/' | grep -v '__pycache__/' | wc -l
```

Expected: ~37 files in wdocs, ~95 files in foretias code.

- [ ] **Step 4: Commit backup**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash && git checkout -
cd /home/hcbusy/webhash/foretias && git checkout -
```

---

## Task 2: Text replacements in wdocs (CAeHius/webhash/)

**Files:** 37 files across `specs/`, `design/`, `docs/`, and root context files.

- [ ] **Step 1: In-place text replacements in prose files**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash

# Process .md and .py files (spec/docs/design files)
# Using \b word boundaries to avoid partial matches
find specs/ design/ docs/ -type f \( -name '*.md' -o -name '*.py' \) ! -path '*/old/*' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 2: In-place text replacements in root context files**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash
sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  README.md AGENTS.md VERSION
```

- [ ] **Step 3: Verify zero remaining matches**

```bash
grep -r -i 'fortias\|fortis' /home/hcbusy/webhash/CAeHius/webhash/ --include='*.md' --include='*.py' --include='*.txt' 2>/dev/null | grep -v '.venv/' | grep -v ':0$'
```

Expected: No output (or only `.venv/` lines which we exclude).

- [ ] **Step 4: Commit**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash
git add -A
git commit -m "rename: fortias→foretias, fortis→foretis in all prose/spec files"
```

---

## Task 3: File renames in wdocs (before text changes above, or after — order doesn't matter for file names since they don't affect content search)

**Files:** Spec files with `FORTIAS_` prefix, `OLD_PYMVP_Fortias_v1.md`

- [ ] **Step 1: Rename spec files**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash/specs/
git mv FORTIAS_0_OVERVIEW.md FORETIAS_0_OVERVIEW.md
git mv FORTIAS_1_MVP_SPEC.md FORETIAS_1_MVP_SPEC.md
git mv FORTIAS_2_IMPLEMENTATION_PLAN.md FORETIAS_2_IMPLEMENTATION_PLAN.md
git mv FORTIAS_2_P2P_SPEC.md FORETIAS_2_P2P_SPEC.md
git mv FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md
git mv FORTIAS_2_P2P_3_libp2p_handshake.md FORETIAS_2_P2P_3_libp2p_handshake.md
git mv FORTIAS_2_P2P_4_dht_discovery.md FORETIAS_2_P2P_4_dht_discovery.md
git mv FORTIAS_2_P2P_5_hardening.md FORETIAS_2_P2P_5_hardening.md
git mv FORTIAS_2_P2P_6_probity_gossip.md FORETIAS_2_P2P_6_probity_gossip.md
git mv FORTIAS_2_P2P_7_collision_detection.md FORETIAS_2_P2P_7_collision_detection.md
git mv FORTIAS_2_P2P_8_epoch_consensus.md FORETIAS_2_P2P_8_epoch_consensus.md
git mv FORTIAS_9_ENCLAVE_SPEC.md FORETIAS_9_ENCLAVE_SPEC.md
git mv OLD_PYMVP_Fortias_v1.md OLD_PYMVP_Foretis_v1.md
```

- [ ] **Step 2: Commit**

```bash
cd /home/hcbusy/webhash/CAeHius/webhash
git add -A
git commit -m "rename: FORTIAS_*.md → FORETIAS_*.md, Fortias→Foretis spec file names"
```

---

## Task 4: Text replacements in foretias/ — C source files

**Files:** `p2p/core/include/fortias_core.h` (126 occurrences), 17 `.c` files, 14 `.c` test files

- [ ] **Step 1: Replace in C header**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  p2p/core/include/fortias_core.h
```

- [ ] **Step 2: Replace in all C source files**

```bash
cd /home/hcbusy/webhash/foretias
find p2p/core/src/ -name '*.c' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  {} +
```

- [ ] **Step 3: Replace in all C test files**

```bash
cd /home/hcbusy/webhash/foretias
find p2p/core/tests/ -type f \( -name '*.c' -o -name '*.h' \) -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  {} +
```

- [ ] **Step 4: Replace in CMakeLists.txt files**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  p2p/core/CMakeLists.txt \
  p2p/core/tests/CMakeLists.txt
```

- [ ] **Step 5: Verify zero remaining matches in C files**

```bash
grep -r -i 'fortias' /home/hcbusy/webhash/foretias/p2p/core/ --include='*.c' --include='*.h' --include='*.txt' --include='CMakeLists.txt' 2>/dev/null | grep -v 'build/'
```

Expected: No output.

- [ ] **Step 6: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: fortias→foretias in C sources, headers, tests, CMakeLists"
```

---

## Task 5: Text replacements in foretias/ — Rust source files

**Files:** 33 `.rs` files across `core-engine/`, `foretias-node/`, `foretias-python/`

- [ ] **Step 1: Replace in all Rust source files**

```bash
cd /home/hcbusy/webhash/foretias
find p2p/ -name '*.rs' -not -path '*/target/*' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 2: Verify**

```bash
grep -r -i 'fortias\|fortis' /home/hcbusy/webhash/foretias/p2p/ --include='*.rs' 2>/dev/null | grep -v 'target/'
```

Expected: No output.

- [ ] **Step 3: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: fortias→foretias, fortis→foretis in Rust source files"
```

---

## Task 6: Text replacements in foretias/ — Rust build config files

**Files:** `Cargo.toml`, `build.rs`, `pyproject.toml`, `README.md`

- [ ] **Step 1: Replace in Cargo.toml files**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/fortias/foretias/g' \
  -e 's/Fortias/Foretias/g' \
  p2p/Cargo.toml \
  p2p/core-engine/Cargo.toml \
  p2p/foretias-node/Cargo.toml \
  p2p/foretias-python/Cargo.toml
```

- [ ] **Step 2: Replace in build.rs**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  p2p/core-engine/build.rs
```

- [ ] **Step 3: Replace in Python project files**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/fortias/foretias/g' \
  -e 's/Fortias/Foretias/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  pyproject.toml \
  p2p/foretias-python/pyproject.toml \
  p2p/foretias-python/README.md
```

- [ ] **Step 4: Replace in root README.md**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  README.md
```

- [ ] **Step 5: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: fortias→foretias in Cargo.toml, build.rs, pyproject.toml, README"
```

---

## Task 7: Text replacements in foretias/ — Python source files

**Files:** 12 files in `src/fortias/`, 11 test files, 2 integration test files

- [ ] **Step 1: Replace in Python source files**

```bash
cd /home/hcbusy/webhash/foretias
find src/ -name '*.py' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 2: Replace in test files**

```bash
cd /home/hcbusy/webhash/foretias
find tests/ -name '*.py' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 3: Replace in integration tests**

```bash
cd /home/hcbusy/webhash/foretias
find integration-tests/ -name '*.py' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 4: Replace in foretias-python Python test files**

```bash
cd /home/hcbusy/webhash/foretias
find p2p/foretias-python/tests/python/ -name '*.py' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 5: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: fortias→foretias, fortis→foretis in Python source and test files"
```

---

## Task 8: Text replacements in foretias/ — spec and doc files

**Files:** 14 spec files, 2 doc files, 1 GitHub workflow doc

- [ ] **Step 1: Replace in spec files**

```bash
cd /home/hcbusy/webhash/foretias
find specs/ -name '*.md' -exec sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  {} +
```

- [ ] **Step 2: Replace in doc files**

```bash
cd /home/hcbusy/webhash/foretias
sed -i \
  -e 's/Fortias/Foretias/g' \
  -e 's/fortias/foretias/g' \
  -e 's/FORTIAS/FORETIAS/g' \
  -e 's/Fortis/Foretis/g' \
  -e 's/fortis/foretis/g' \
  -e 's/FORTIS/FORETIS/g' \
  docs/threat_model_v0_5.md \
  docs/superpowers/plans/2026-04-27-infrastructure-prioritization.md \
  .github/workflows/publish-setup.md
```

- [ ] **Step 3: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: fortias→foretias in specs and docs"
```

---

## Task 9: File and directory renames in foretias/

**Important:** Do text replacements FIRST (Tasks 4-8), then rename. The `src/fortias/` directory still exists as a path to reference during text replacement.

**Directory renames:**
| Old | New |
|-----|-----|
| `src/fortias/` | `src/foretias/` |
| `p2p/core/include/fortias_core.h` | `p2p/core/include/foretias_core.h` |

**File renames in specs/:**
| Old | New |
|-----|-----|
| `specs/fortias-v1.md` | `specs/foretias-v1.md` |
| `specs/FORTIAS_0_OVERVIEW.md` | `specs/FORETIAS_0_OVERVIEW.md` |
| `specs/FORTIAS_1_MVP_SPEC.md` | `specs/FORETIAS_1_MVP_SPEC.md` |
| `specs/FORTIAS_2_IMPLEMENTATION_PLAN.md` | `specs/FORETIAS_2_IMPLEMENTATION_PLAN.md` |
| `specs/FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md` | `specs/FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md` |
| `specs/FORTIAS_2_P2P_3_IMPLEMENTATION_PLAN.md` | `specs/FORETIAS_2_P2P_3_IMPLEMENTATION_PLAN.md` |
| `specs/FORTIAS_2_P2P_3_libp2p_handshake.md` | `specs/FORETIAS_2_P2P_3_libp2p_handshake.md` |
| `specs/FORTIAS_2_P2P_4_dht_discovery.md` | `specs/FORETIAS_2_P2P_4_dht_discovery.md` |
| `specs/FORTIAS_2_P2P_5_hardening.md` | `specs/FORETIAS_2_P2P_5_hardening.md` |
| `specs/FORTIAS_2_P2P_6_probity_gossip.md` | `specs/FORETIAS_2_P2P_6_probity_gossip.md` |
| `specs/FORTIAS_2_P2P_7_collision_detection.md` | `specs/FORETIAS_2_P2P_7_collision_detection.md` |
| `specs/FORTIAS_2_P2P_8_epoch_consensus.md` | `specs/FORETIAS_2_P2P_8_epoch_consensus.md` |
| `specs/FORTIAS_2_P2P_SPEC.md` | `specs/FORETIAS_2_P2P_SPEC.md` |

- [ ] **Step 1: Rename C header file**

```bash
cd /home/hcbusy/webhash/foretias
git mv p2p/core/include/fortias_core.h p2p/core/include/foretias_core.h
```

- [ ] **Step 2: Update CMakeLists.txt file references**

The CMakeLists.txt still references the old header name internally. After rename, update any `fortias_core.h` → `foretias_core.h` in `include()` directives and link references:

```bash
grep -r 'fortias_core' /home/hcbusy/webhash/foretias/p2p/core/ --include='*.txt' --include='*.cmake' --include='*.c' --include='*.h' --include='*.rs' 2>/dev/null | grep -v 'build/'
```

If any references remain (e.g., `#include "fortias_core.h"`), they should have been caught by Task 4's sed. Verify:

```bash
grep -r 'fortias_core\.h' /home/hcbusy/webhash/foretias/ --include='*.c' --include='*.h' --include='*.rs' 2>/dev/null | grep -v 'target/' | grep -v 'build/'
```

Expected: No output. If output exists, fix remaining `#include` directives:

```bash
sed -i 's/fortias_core\.h/foretias_core.h/g' /home/hcbusy/webhash/foretias/p2p/core-engine/build.rs
```

- [ ] **Step 3: Update build.rs header path reference**

```bash
grep 'fortias_core\|foretias_core' /home/hcbusy/webhash/foretias/p2p/core-engine/build.rs
```

Verify `foretias_core.h` appears (not `fortias_core.h`). If not, the sed in Task 4 already handled it.

- [ ] **Step 4: Rename Python package directory**

```bash
cd /home/hcbusy/webhash/foretias
git mv src/fortias src/foretias
```

- [ ] **Step 5: Rename spec files**

```bash
cd /home/hcbusy/webhash/foretias/specs/
git mv fortias-v1.md foretias-v1.md
git mv FORTIAS_0_OVERVIEW.md FORETIAS_0_OVERVIEW.md
git mv FORTIAS_1_MVP_SPEC.md FORETIAS_1_MVP_SPEC.md
git mv FORTIAS_2_IMPLEMENTATION_PLAN.md FORETIAS_2_IMPLEMENTATION_PLAN.md
git mv FORTIAS_2_P2P_2_direct_p2p_mutual_attestation.md FORETIAS_2_P2P_2_direct_p2p_mutual_attestation.md
git mv FORTIAS_2_P2P_3_IMPLEMENTATION_PLAN.md FORETIAS_2_P2P_3_IMPLEMENTATION_PLAN.md
git mv FORTIAS_2_P2P_3_libp2p_handshake.md FORETIAS_2_P2P_3_libp2p_handshake.md
git mv FORTIAS_2_P2P_4_dht_discovery.md FORETIAS_2_P2P_4_dht_discovery.md
git mv FORTIAS_2_P2P_5_hardening.md FORETIAS_2_P2P_5_hardening.md
git mv FORTIAS_2_P2P_6_probity_gossip.md FORETIAS_2_P2P_6_probity_gossip.md
git mv FORTIAS_2_P2P_7_collision_detection.md FORETIAS_2_P2P_7_collision_detection.md
git mv FORTIAS_2_P2P_8_epoch_consensus.md FORETIAS_2_P2P_8_epoch_consensus.md
git mv FORTIAS_2_P2P_SPEC.md FORETIAS_2_P2P_SPEC.md
```

- [ ] **Step 6: Commit**

```bash
cd /home/hcbusy/webhash/foretias
git add -A
git commit -m "rename: file/directory renames fortias→foretias (C header, Python pkg, spec files)"
```

---

## Task 10: Verify build integrity

- [ ] **Step 1: Clean build artifacts**

```bash
cd /home/hcbusy/webhash/foretias
# Clean CMake build
rm -rf p2p/core/build/
# Clean Rust target
rm -rf p2p/target/
# Clean Python caches
rm -rf src/foretias/__pycache__/
rm -rf tests/__pycache__/
rm -rf .pytest_cache/
```

- [ ] **Step 2: Build C library**

```bash
cd /home/hcbusy/webhash/foretias/p2p/core
mkdir -p build && cd build
cmake ..
make
```

Expected: No errors. If errors about missing `fortias_core.h` — the header rename failed.

- [ ] **Step 3: Build Rust crates**

```bash
cd /home/hcbusy/webhash/foretias/p2p
cargo check --workspace
```

Expected: No errors. Bindgen will regenerate bindings from the renamed header.

- [ ] **Step 4: Run C tests**

```bash
cd /home/hcbusy/webhash/foretias/p2p/core/build
ctest --output-on-failure
```

Expected: All tests pass.

- [ ] **Step 5: Run Python tests**

```bash
cd /home/hcbusy/webhash/foretias
python -m pytest tests/ -v
```

Expected: All tests pass. If `import fortias` errors, check `pyproject.toml` entry points.

- [ ] **Step 6: Commit build verification**

```bash
cd /home/hcbusy/webhash/foretias
git commit --allow-empty -m "verify: build and tests pass after fortias→foretias rename"
```

---

## Task 11: Final global verification

- [ ] **Step 1: Sweep both repos for remaining occurrences**

```bash
# wdocs (excluding .venv)
grep -r -i 'fortias\|fortis' /home/hcbusy/webhash/CAeHius/webhash/ \
  --include='*.md' --include='*.py' --include='*.txt' \
  2>/dev/null | grep -v '.venv/' | grep -v '.opencode/'

# foretias (excluding build artifacts)
cd /home/hcbusy/webhash/foretias
grep -r -i 'fortias\|fortis' . \
  --include='*.rs' --include='*.c' --include='*.h' --include='*.py' \
  --include='*.toml' --include='*.md' --include='*.txt' \
  --include='*.cmake' --include='CMakeLists.txt' \
  2>/dev/null | grep -v 'target/' | grep -v 'build/' | grep -v '.pytest_cache/' | grep -v '__pycache__/' | grep -v '.git/'
```

Expected: No output from either command (or only `.opencode/` files which are optional).

- [ ] **Step 2: Verify no broken file references**

```bash
# Check that all spec file references in docs point to renamed files
grep -r 'FORTIAS_' /home/hcbusy/webhash/foretias/ --include='*.md' 2>/dev/null | grep -v 'FORETIAS_' | grep -v '.git/'
grep -r 'fortias-v1' /home/hcbusy/webhash/foretias/ --include='*.md' --include='*.toml' 2>/dev/null | grep -v 'foretias-v1' | grep -v '.git/'
```

Expected: No output.

- [ ] **Step 3: Final commit**

```bash
cd /home/hcbusy/webhash/foretias
git status
cd /home/hcbusy/webhash/CAeHius/webhash
git status
```

---

## Risk Considerations

1. **Word boundary edge case:** `\b` in sed works for standard word chars (`[a-zA-Z0-9_]`). `Fortias` in `FortiasNode` would match the `Fortias` part. Check for compound identifiers:
   ```bash
   grep -r 'Fortias[A-Z]\|fortias[a-z]\|FORTIAS[A-Z]' /home/hcbusy/webhash/foretias/ --include='*.rs' --include='*.c' --include='*.h' --include='*.py' 2>/dev/null | grep -v 'target/'
   ```
   If compound identifiers exist (e.g., `FortiasNode`), they should become `ForetiasNode` — use `s/Fortias/Foretias/g` (no boundary) for compound words. Direct replacement already handles all cases.

2. **Bindgen regeneration:** `p2p/core-engine/build.rs` uses bindgen to generate Rust bindings from the C header. After rename, the bindgen output file (`core_bindings.rs`) is regenerated. The `build.rs` file must reference `foretias_core.h` not `fortias_core.h`.

3. **Python entry points:** `pyproject.toml` may have `fortias = "fortias.cli:main"` style entry points. Both the package name and the module path need updating.

4. **CMake target names:** CMakeLists.txt may define targets like `fortias_core`. These internal identifiers should also rename.

5. **Cargo package names:** `Cargo.toml` `[package] name = "fortias-core"` should become `"foretias-core"`. Check all workspace members.
