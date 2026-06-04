# Cargo-Geiger Unsafe Code Baseline Report

**Date:** 2026-06-04
**Tool:** cargo-geiger v0.13.0
**Workspace:** Foretias Rust workspace (`p2p/`)
**Branch:** CODE_QUALITY_TOOLING_4821 worktree

---

## What is cargo-geiger?

[cargo-geiger](https://github.com/Julian/cargo-geiger) scans Rust crates for `unsafe` code usage. It reports:

- **Functions** — unsafe functions used by the build
- **Expressions** — unsafe expressions used by the build
- **Impls** — unsafe impl blocks used by the build
- **Traits** — unsafe trait definitions used by the build
- **Methods** — unsafe methods used by the build

Metric format `x/y` means: `x` = unsafe code **used** by the build, `y` = total unsafe code found in the crate.

Symbols:
- `:)` — No `unsafe` usage, declares `#![forbid(unsafe_code)]`
- `?` — No `unsafe` usage, missing `#![forbid(unsafe_code)]`
- `!` — `unsafe` usage found

---

## Summary Totals

| Crate | Functions | Expressions | Impls | Traits | Methods |
|-------|-----------|-------------|-------|--------|---------|
| **foretias-core** | 194/419 | 16,063/26,383 | 388/439 | 59/61 | 502/761 |
| **foretias-client** | 194/419 | 16,063/26,383 | 388/439 | 59/61 | 502/761 |
| **foretias-server** | 302/1,075 | 28,130/69,244 | 691/903 | 74/87 | 937/3,675 |

Note: `foretias-client` shows the same totals as `foretias-core` because client's own code has `0/0` unsafe — all unsafe comes from shared dependencies (core-engine). `foretias-server` has higher counts due to libp2p and additional dependencies.

---

## Dependencies with Highest Unsafe Usage (foretias-server)

These are the top-level dependencies contributing the most unsafe code to the build:

| Dependency | Functions | Expressions | Impls | Traits | Methods | Why |
|------------|-----------|-------------|-------|--------|---------|-----|
| **tokio** 1.52.3 | 26/30 | 2,303/2,906 | 110/119 | 3/3 | 108/138 | Async runtime, syscalls, epoll |
| **memchr** 2.8.1 | 27/41 | 1,981/2,429 | 2/2 | 0/0 | 110/148 | SIMD-optimized byte search |
| **zerocopy** 0.8.50 | 8/12 | 462/466 | 74/74 | 29/29 | 37/37 | Zero-copy deserialization |
| **hashbrown** 0.14.5 | 1/1 | 1,403/1,578 | 21/24 | 1/1 | 76/88 | AHash-based HashMap |
| **heapless** 0.7.17 | 2/2 | 955/961 | 13/13 | 0/0 | 29/29 | Stack-allocated collections |
| **parking_lot** 0.12.5 | 1/1 | 279/279 | 17/17 | 0/0 | 25/25 | Mutex/RwLock primitives |
| **bytes** 1.11.1 | 40/40 | 799/853 | 12/14 | 1/1 | 16/20 | Byte buffer utilities |
| **blake3** 1.8.5 | 11/84 | 77/4,365 | 0/0 | 0/0 | 0/0 | BLAKE3 hash (SIMD intrinsics) |
| **poly1305** 0.8.0 | 5/5 | 972/975 | 0/0 | 0/0 | 12/12 | Poly1305 MAC (constant-time) |
| **ring** 0.17.14 | 8/8 | 432/444 | 0/0 | 0/0 | 8/8 | TLS/crypto primitives |
| **chacha20** 0.9.1 | 12/13 | 371/842 | 0/0 | 0/0 | 0/0 | ChaCha20 cipher |
| **curve25519-dalek** 4.1.3 | 0/2 | 153/806 | 0/0 | 0/0 | 0/0 | Ed25519/X25519 curves |
| **libc** 0.2.186 | 0/92 | 34/725 | 0/2 | 0/0 | 8/101 | FFI bindings for libc |
| **mio** 1.2.1 | 1/2 | 171/687 | 0/15 | 0/0 | 9/23 | I/O notification (epoll/kqueue) |
| **lock_api** 0.4.14 | 0/0 | 669/669 | 32/32 | 14/14 | 24/24 | Lock abstractions |
| **generic-array** 0.14.7 | 1/1 | 285/285 | 20/20 | 8/8 | 5/5 | Fixed-size crypto arrays |
| **spin** 0.9.8 | 0/0 | 185/227 | 23/31 | 0/0 | 21/25 | Lock-free spinlocks |
| **oqs** 0.11.0 | 0/0 | 170/170 | 4/4 | 0/0 | 0/0 | liboqs Rust bindings (PQC) |
| **tracing-subscriber** 0.3.23 | 0/0 | 142/142 | 0/0 | 0/0 | 7/7 | Structured logging |
| **parking_lot_core** 0.9.12 | 16/16 | 912/1,343 | 0/0 | 0/0 | 8/58 | Parking lot primitives |

---

## foretias-core Unsafe — Expected (C11 FFI)

The `foretias-core` crate (core-engine) has significant unsafe code, which is **expected and intentional**:

1. **C11 FFI via bindgen** — `core-engine/build.rs` compiles the C11 core library and runs bindgen over `foretias_core.h`. The generated bindings contain `unsafe extern "C"` function declarations for all C11 primitives (Ed25519, SHA-256, BLAKE3, Noise protocol, Merkle trees, FROST, etc.).

2. **Safe wrappers** — The `core-engine/src/core/` module provides safe Rust wrappers around the FFI bindings, validating inputs before crossing the boundary and handling errors from C return codes.

3. **oqs-sys / liboqs** — The post-quantum crypto library (Kyber, Dilithium, etc.) is linked via C FFI through `oqs-sys`, contributing additional unsafe FFI bindings.

This unsafe code is the **designated trust boundary** for cryptographic operations. It is audited and documented per the project's SAFETY.md.

---

## foretias-server Own Code

`foretias-server` reports `0/0` unsafe in its own code — all unsafe comes from dependencies. This is the desired state: the server layer should not contain raw unsafe code.

---

## foretias-client Own Code

`foretias-client` reports `0/0` unsafe in its own code — all unsafe comes from shared dependencies (foretias-core). This is the desired state.

---

## Crates with `#![forbid(unsafe_code)]`

These dependencies explicitly forbid unsafe code (marked `:)`):

- `digest` 0.10.7
- `typenum` 1.20.1
- `crypto-common` 0.1.7
- `base64` 0.21.7
- `aead` 0.5.2
- `ed25519` 2.2.3
- `signature` 2.2.0
- `hkdf` 0.12.4
- `hmac` 0.12.1
- `regex-syntax` 0.8.10
- `unsigned-varint` 0.8.0
- `yamux` 0.12.1 / 0.13.10
- `parking` 2.2.1
- `fastrand` 2.4.1
- `zeroize_derive` 1.4.3
- `heck` 0.5.0
- `ctr` 0.9.2
- `base64` 0.22.1
- `async-channel` 2.5.0
- `unsigned-varint` 0.7.2

---

## Interpretation

This report is **informational only** — it is not an enforcement gate. The purpose is to:

1. **Establish a baseline** for future comparison when dependencies are updated or new ones are added.
2. **Identify dependencies** with notably high unsafe code usage for potential review.
3. **Confirm** that foretias's own code (server, client) does not contain raw unsafe code.

### Key observations:

- **foretias-core** unsafe is expected (C11 FFI via bindgen + oqs-sys/liboqs PQC bindings).
- **tokio** and **memchr** are the largest sources of unsafe in the dependency tree — both are well-audited, widely-used crates.
- **zerocopy** has significant unsafe (74/74 impls, 29/29 traits) — this is expected for zero-copy deserialization and is a well-maintained crate.
- **blake3** has 11/84 unsafe functions but 77/4,365 unsafe expressions — the high expression count comes from SIMD intrinsics, which is expected for a high-performance hash implementation.
- **ring** (used by libp2p-noise) has 8/8 unsafe functions and 432/444 expressions — expected for a TLS/crypto library with assembly implementations.

---

## Re-running This Report

```bash
cd p2p
cargo geiger --manifest-path core-engine/Cargo.toml
cargo geiger --manifest-path foretias-client/Cargo.toml
cargo geiger --manifest-path foretias-server/Cargo.toml
```
