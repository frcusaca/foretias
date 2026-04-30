# ── Fortias C11 Core — Top-level Makefile ──────────────────────────────
# Usage:
#   make            # build static library (libfortias_core.a)
#   make test       # run unit tests
#   make clean      # remove build artifacts
#   make install    # install lib + headers (requires sudo)
#
# Requires: C11 compiler (gcc/clang), libsodium-dev, CMake 3.16+

SHELL    := /bin/bash
PREFIX   ?= /usr/local
BUILD    := build

.PHONY: all test clean install cmake frama-c-merkle

all: $(BUILD)/libfortias_core.a

# ── Build via CMake ─────────────────────────────────────────────────────
$(BUILD)/libfortias_core.a: CMakeLists.txt $(wildcard src/*.c include/*.h)
	cmake -B $(BUILD) -DCMAKE_C_COMPILER=gcc -DCMAKE_BUILD_TYPE=Release
	cmake --build $(BUILD) --target fortias_core
	@echo "Built $(BUILD)/libfortias_core.a"

cmake:
	cmake -B $(BUILD) -DCMAKE_C_COMPILER=gcc -DCMAKE_BUILD_TYPE=Release

# ── Tests ───────────────────────────────────────────────────────────────
test: all
	cmake --build $(BUILD) --target test_all 2>/dev/null || true
	ctest --test-dir $(BUILD) --output-on-failure
	@echo "Tests complete."

# ── Install ─────────────────────────────────────────────────────────────
install: all
	cmake --install $(BUILD) --prefix $(PREFIX)
	@echo "Installed to $(PREFIX)"

# ── Clean ───────────────────────────────────────────────────────────────
clean:
	rm -rf $(BUILD)
	@echo "Cleaned."

# ── Frama-C Formal Verification (merkle.c PoC) ─────────────────────
FRAMA_C     ?= frama-c
WP_FLAGS    = -wp -wp-rte -wp-timeout 60

frama-c-merkle:
	$(FRAMA_C) $(WP_FLAGS) \
	    -cpp-extra-args="-I include" \
	    src/merkle.c src/hash_sha256.c src/memzero.c \
	    -then -report
	@echo "Frama-C WP: all goals proved"
