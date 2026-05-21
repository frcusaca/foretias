# FORETIAS_SNAPSUITE_SPEC.md

## Overview

Integrate a cryptographically signed snapshot testing suite (`snapsuite`) into foretias, adapted from the pattern in `foolish-rust/foolish/foolish-core/src/snapshot_suite.rs`.

The core idea: approval-based snapshot tests where each snapshot is **progressively signed** with Ed25519, so tampering with any block (input, output, or comments) invalidates downstream signatures.

This is `insta` plus cryptographic provenance.

---

## Key Differences from foolish-rust

| Aspect | foolish-rust | foretias |
|--------|-------------|----------|
| **Test name source** | `input/*.foo` file stem | Test function name / implementation-defined string |
| **Evaluator** | `Compiler` + `run_to_completion` on Foolish source | `TimeFamilyServer` / `Chronomatter` — stamp & verify operations |
| **Input** | `.foo` source code files | No input files. The "input" is the test scenario description (messages to stamp, server config, etc.) |
| **Output** | FIR evaluation output (HSSnap format) | Cryptographically signed `Foretis` JSON + verification results |
| **Signing key** | Argon2id-derived Ed25519 (passphrase → key) | Same Argon2id approach, using `ed25519-dalek` (already a dependency) |
| **Snapshot storage** | `snapshot_tests/approved/*.foo.snap` | `snapshot_tests/approved/*.snap` |
| **First test** | N/A | Fixed-seed server stamp/verify — **intentionally failing** (C11 RNG is uncontrollable from Rust) |

---

## Architecture

### Module Placement

```
p2p/core-engine/src/
├── snapshot_suite.rs       # SnapshotSuite struct, Evaluator trait, discovery, comparison
├── snapshot_signature.rs   # Progressive Ed25519 signing (adapted from foolish-rust signature.rs)
└── (lib.rs re-exports)
```

Placed in `core-engine` (foretias-core) because:
- The signing infrastructure is domain-agnostic and reusable
- `foretias-server` tests will depend on it as a dev-dependency
- Avoids circular dependencies

### Dependencies

**New `[dev-dependencies]` for `core-engine/Cargo.toml`:**
- `insta` — snapshot assertion framework (already used in foolish-rust)
- `argon2` — deterministic key derivation from passphrase (already in foolish-rust)

**No new production dependencies.** `ed25519-dalek`, `hex`, `base64`, `serde`, `serde_json` are already in `core-engine`.

---

## Core Types

### `Evaluator` Trait

```rust
/// Trait for test scenarios that produce a signed snapshot output.
///
/// Unlike foolish-rust where an evaluator compiles source code,
/// a foretias evaluator executes a stamp/verify scenario against a server.
pub trait Evaluator {
    /// Execute the test scenario and return the formatted output.
    ///
    /// The output is a multi-block string:
    ///   - INPUT block: description of what was stamped/verified
    ///   - RESULT block(s): Foretis JSON, verification results
    ///   - COMMENTS block: test name + metadata
    ///
    /// Returns the signed snapshot text (including SIGNATURES footer).
    fn evaluate(&self, test_name: &str) -> Result<String, String>;
}
```

Key difference: The `test_name` is passed in from the implementation, not derived from a file path.

### `SnapshotSuite`

```rust
pub struct SnapshotSuite {
    approved_dir: PathBuf,
    passphrase:    String,  // For deterministic signing key derivation
}
```

Responsibilities:
- Discover approved snapshots in `approved_dir`
- Compare new output against approved snapshots
- Report `Pending` (no approved snapshot), `Mismatch`, or `Pass`
- **No input directory** — tests are code-driven, not file-driven

### `SnapshotSignature` (adapted from foolish-rust)

Progressive triple-signing:
```
input_sig      = sign(canon_input)
result_sig     = sign(canon_input + canon_result)
comments_sig   = sign(canon_input + canon_result + canon_comments)
```

Footer format:
```
SIGNATURES:
Public key: <hex>
Input signature: <base64>
Result signature: <base64>
Comments signature: <base64>
```

### `SnapshotVerification`

```rust
pub struct SnapshotVerification {
    pub key_match:    bool,
    pub input_ok:     bool,    // renamed from foolish_ok
    pub result_ok:    bool,    // renamed from hs_ok
    pub comments_ok:  bool,
}
```

---

## Snapshot File Format

Each approved snapshot (`.snap` file) contains:

```
---
input: |
  <canonicalized input description>

result: |
  <canonicalized Foretis JSON / verification output>

comments: |
  <test_name>
  <optional metadata: chronon_number, tbid, etc.>

SIGNATURES:
Public key: <hex-encoded verifying key>
Input signature: <base64>
Result signature: <base64>
Comments signature: <base64>
```

The `insta` framework handles the `.snap` file format natively. The SIGNATURES footer is appended after the insta-generated content.

---

## Test Output Format

The `evaluate()` method for a foretias test produces output in this structure:

```
INPUT:
```json
{
  "test_name": "server_stamp_verify_fixed_seed",
  "messages": ["hello world", "second message"],
  "server_config": {
    "chronon_ns": 60000000000,
    "fixed_seed": true
  }
}
```

RESULT:
```json
{
  "stamps": [
    {
      "chronon_number": 1,
      "content_hash": "sha256:...",
      "signature": "ed25519:...",
      "tbid": "...",
      "echo": "hello world",
      "tbn": 1,
      "time_being_reference_time": 60000000000
    }
  ],
  "verifications": [
    {
      "valid": true,
      "message": "hello world"
    }
  ]
}
```

COMMENTS:
```
server_stamp_verify_fixed_seed
```

SIGNATURES:
Public key: <hex>
Input signature: <base64>
Result signature: <base64>
Comments signature: <base64>
```

---

## First Test: Fixed-Seed Server (Intentionally Failing)

### Purpose

Establish the snapshot_suite integration infrastructure. The test itself will **always fail** because the random seed that controls key generation is deeply embedded inside the C11 library (libsodium) and cannot be overridden from the Rust side.

This is intentional — the test serves as:
1. Infrastructure validation (snapsuite compiles, runs, produces signed snapshots)
2. A permanent regression marker documenting that C11 RNG is not externally controllable
3. A template for future tests that use controllable seeds

### Test Design

```rust
#[test]
fn server_stamp_verify_fixed_seed() {
    // Create a TimeFamilyServer with a fixed chronon period
    // Attempt to stamp "hello world" and verify the result
    // Compare against approved snapshot
    // THIS TEST IS EXPECTED TO FAIL — C11 RNG cannot be seeded
}
```

The approved snapshot will be generated with `INSTA_UPDATE=always` during initial integration, capturing whatever output the server produces. On subsequent runs, the test will fail because the output differs (different keys, different signatures each run).

**The test must be marked with `#[ignore]` or have a clear comment explaining why it fails.** The approved snapshot is kept as a reference for the *structure* of the output, not the *values*.

---

## Signature Module Design (`snapshot_signature.rs`)

Adapted from `foolish-rust/foolish/foolish-core/src/signature.rs` with foretias-specific naming.

### Functions

| Function | Purpose |
|----------|---------|
| `derive_keypair(passphrase: &str) -> (SigningKey, VerifyingKey)` | Argon2id → Ed25519 keypair (deterministic) |
| `sign_content(&SigningKey, &str) -> (VerifyingKey, Vec<u8>)` | Sign UTF-8 content |
| `verify_signature(&VerifyingKey, &str, &[u8]) -> bool` | Verify Ed25519 signature |
| `canonicalize_block(&str) -> String` | Trim + append `\n` |
| `sign_snapshot(passphrase, input, result, comments) -> SnapshotSignature` | Progressive triple-sign |
| `verify_snapshot(passphrase, input, result, comments, &sig) -> SnapshotVerification` | Verify all three progressive sigs |
| `parse_snapshot_footer(&str) -> Option<SnapshotSignature>` | Extract signature block from snapshot text |

### Salt

Change the Argon2id salt from `"foolish-rust:snapshot-sig:v1"` to `"foretias:snapsuite-sig:v1"` to ensure keys are domain-specific.

---

## Integration with Existing Tests

The snapshot_suite is **additive** — it does not replace existing integration tests. It coexists with:

- `p2p/foretias-server/tests/integration.rs` — CLI E2E, P2P connectivity
- `p2p/foretias-server/tests/e2e_verification.rs` — Type discipline, integrity checks
- `p2p/foretias-server/tests/client_ptp.rs` — PtP client tests

The snapsuite tests live in a new file:
```
p2p/foretias-server/tests/snapshot_tests.rs
```

This file:
1. Imports `SnapshotSuite` and `Evaluator` from `foretias-core`
2. Defines test scenarios (each as a function implementing `Evaluator`)
3. Uses `insta::assert_snapshot!` for comparison, wrapped by the suite

---

## Directory Structure

```
p2p/foretias-server/
├── snapshot_tests/
│   └── approved/
│       └── server_stamp_verify_fixed_seed.snap    (generated, will always mismatch)
├── tests/
│   └── snapshot_tests.rs                          (test entry point)
└── Cargo.toml                                     (insta as dev-dependency)
```

No `input/` directory — tests are code-driven.

---

## Implementation Plan Outline

(To be detailed in `FORETIAS_SNAPSUITE_PLAN.md`)

1. **Port signature module** — `snapshot_signature.rs` into `core-engine`
2. **Port snapshot_suite** — `snapshot_suite.rs` into `core-engine`, adapted for code-driven tests
3. **Wire up re-exports** — `lib.rs` exposes `SnapshotSuite`, `Evaluator`, `SnapshotSignature`
4. **Add dev-dependencies** — `insta`, `argon2` to `core-engine/Cargo.toml`
5. **Create snapshot test harness** — `foretias-server/tests/snapshot_tests.rs`
6. **Implement first test** — `server_stamp_verify_fixed_seed` (intentionally failing)
7. **Generate initial approved snapshot** — `INSTA_UPDATE=always` to capture structure
8. **Verify signature chain** — ensure progressive signing works end-to-end

---

## Constraints

- **No new production dependencies.** Signing uses `ed25519-dalek` (already present).
- **No changes to C11 core.** The RNG limitation is a known constraint, not a bug to fix.
- **No changes to existing tests.** Snapsuite is purely additive.
- **Test name from implementation.** Never derived from file paths.
- **Signature salt is foretias-specific.** `"foretias:snapsuite-sig:v1"`.
- **First test is `#[ignore]` or clearly documented as expected-failure.**

---

## Verification Criteria

The integration is complete when:
1. `cargo test -p foretias-core` passes (signature module tests)
2. `cargo test -p foretias-server --test snapshot_tests` runs and the first test produces a signed snapshot
3. The first test fails on re-run (proving non-determinism due to C11 RNG)
4. The SIGNATURES footer in the `.snap` file passes `verify_snapshot()` when re-verified against the captured content
5. `cargo clippy` is clean on new code
