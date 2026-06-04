# CLIPPY_FIX_SPEC.md

**Spec: Clippy Warning Remediation — Complete Fix Plan**
**Paired Plan: CLIPPY_FIX_PLAN.md**
**Status: PROPOSED**
**Date: 2026-06-04**
**BRANCH_NAME: tooling/clippy-fixes**
**FULL_WORKTREE_PATH=${HOME}/tmp/foretias-worktrees/CLIPPY_FIXES_####**

---

## Overview

This spec documents every clippy warning in the Foretias workspace and provides a test-driven plan to fix them all. The goal is zero clippy warnings so the CI gate (`.github/workflows/clippy.yml`) passes.

### Current State

| Category | Count | Auto-fixable |
|----------|-------|--------------|
| Standard clippy (unique) | ~41 | ~30 (via `cargo clippy --fix`) |
| Standard clippy (total incl. duplicates) | 166 | ~100 |
| Pedantic clippy (unique) | ~1,920 | ~500 |
| Pedantic clippy (total incl. duplicates) | 1,944 | ~1,000 |

### Scope

- **In scope:** All standard clippy warnings (these block the CI gate)
- **In scope:** High-value pedantic warnings (cast_possible_truncation, float_cmp, items_after_statements)
- **Out of scope:** Low-value pedantic warnings (missing_panics_doc, missing_errors_doc, must_use_candidate, doc_markdown, uninlined_format_args) — these are documentation/style only
- **In scope:** `too_many_lines` refactoring — 6 production functions exceeding 100 lines (see Phase D)

---

## Phase A: Auto-Fixable Standard Warnings

These can be resolved with `cargo clippy --fix`. Each still needs a test to verify no regression.

### A1. `clone_on_copy` (26 instances)

**Lint:** Using `.clone()` on types that implement `Copy`.

**Files affected:**
| File | Count | Lines |
|------|-------|-------|
| `communerd/communerdette.rs` | 22 | 1074, 2243, 2251, 2427, 2436, 2465, 2474, 2503, 2512, 2542, 2581, 2602, 2608, 2667, 2674, 3067, 3073, 3377, 3399, 3410, 3984, 4006 |
| `server/mod.rs` | 1 | 489 |
| `tests/integration.rs` | 3 | 388, 729, 809 |

**Fix:** Replace `.clone()` with direct copy (or `*tbid` for references).

**Test plan:**
- No new tests needed — these are mechanical changes that preserve semantics.
- Verify: `cargo test --workspace` passes after fix.

---

### A2. `deprecated ed25519_sign` (15 instances)

**Lint:** Use of deprecated `ed25519_sign`; should use `ed25519_sign_with_handle`.

**Files affected:**
| File | Count | Lines |
|------|-------|-------|
| `core/signing.rs` | 3 | 56, 63, 64 |
| `foretias/calendar.rs` | 6 | 342, 361, 365, 366, 392, 411, 415, 416, 555, 572, 590 |
| `collision/detector.rs` | 1 | 151 |

**Fix:** Replace `ed25519_sign(&priv_key, msg)` with `ed25519_sign_with_handle(handle, msg)`. Requires constructing a `PrivKeyHandle` from the raw key bytes.

**Test plan:**
- **Test:** `test_ed25519_sign_with_handle_roundtrip` — generate keypair, sign with handle, verify matches old behavior
- **Test:** `test_calendar_signatures_still_verify` — verify that calendar attestations still verify after migration
- **Regression:** All existing `ed25519` tests must still pass

---

### A3. `redundant_closure` (13 instances)

**Lint:** `.map(|x| f(x))` where `f` is already a function pointer.

**Files affected:**
| File | Count | Lines |
|------|-------|-------|
| `server/handlers.rs` | 3 | 130, 1155, 1157 |
| `communerd/mod.rs` | 1 | 142 |
| `probity/gossip_handler.rs` | 1 | 148 |
| `tests/toppoli.rs` | 3 | 138, 153, 307, 658, 766, 789 |
| `core-engine` | 4 | various |

**Fix:** Replace `.map(|x| f(x))` with `.map(f)`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A4. `type_complexity` (5 instances)

**Lint:** Very complex types that should be factored into `type` definitions.

**Files affected:**
| File | Count | Lines | Type |
|------|-------|-------|------|
| `communerd/mod.rs` | 2 | 176, 561 | `Arc<parking_lot::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<Vec<u8>>>>>>` |
| `communerd/communerdette.rs` | 1 | 3435 | Test fixture return type |
| `core-engine` | 2 | various | Various complex generics |

**Fix:** Add `type` aliases:
```rust
type PendingFamilyLookup = Arc<parking_lot::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<Vec<u8>>>>>>;
```

**Test plan:**
- No new tests needed — `type` aliases are compile-time only.
- Verify: `cargo build --workspace` passes.

---

### A5. `too_many_arguments` (5 functions)

**Lint:** Functions with more than 7 arguments.

**Functions affected:**
| Function | File | Args | Fix |
|----------|------|------|-----|
| `gossip_event_loop` | `communerd/mod.rs:551` | 11 | Create `GossipLoopConfig` struct |
| `refresh_self_registration` | `communerd/mod.rs:849` | 9 | Create `RegistrationConfig` struct |
| `cmd_serve` | `main.rs:273` | 14 | Already uses `Args` struct — add `#[allow]` with justification |
| `cmd_verify` | `main.rs:486` | 8 | Already uses `Args` struct — add `#[allow]` with justification |
| `cmd_prove_verification` | `main.rs` | 15 | Already uses `Args` struct — add `#[allow]` with justification |

**Fix strategy:**
- For `gossip_event_loop` and `refresh_self_registration`: Create config structs
- For CLI commands: Add `#[allow(clippy::too_many_arguments)]` with comment "CLI command functions derive args from clap Args struct; grouping would add indirection without clarity"

**Test plan:**
- **Test:** `test_gossip_loop_config_construction` — verify struct fields match function params
- **Test:** `test_registration_config_construction` — verify struct fields match function params
- Verify: All existing tests pass after struct extraction

---

### A6. `derivable_impls` (3 instances)

**Lint:** Manual `impl Default` that can be derived.

**Files affected:**
| File | Lines | Type |
|------|-------|------|
| `communerd/communerdette.rs` | 156 | `CommunerdetteStats` |
| `communerd/communerdette.rs` | 449 | `ChannelBindingState` |
| `calendar_store/lru.rs` | 48 | `CalendarStoragePolicy` |

**Fix:** Replace manual `impl Default` with `#[derive(Default)]` and `#[default]` on the default variant.

**Test plan:**
- No new tests needed — derive produces identical behavior.
- Verify: `cargo test --workspace` passes.

---

### A7. `private_interfaces` (6 instances)

**Lint:** Exposed items reference types that are too private.

**Files affected:**
| File | Lines | Issue |
|------|-------|-------|
| `communerd/communerdette.rs` | 970 | `CommunerdetteExecutor` is `pub(self)` but used in `pub(super)` methods |
| `communerd/communerdette.rs` | 465 | `Communerdette` is `pub(super)` but used in `pub(crate)` method |
| `communerd/communerdette.rs` | 51 | `CommunerdetteHost` is `pub(super)` but used in `pub(crate)` method |

**Fix:** Either widen visibility of the private types or narrow visibility of the exposed methods.

**Test plan:**
- No new tests needed — visibility change only.
- Verify: `cargo build --workspace` passes (visibility must be consistent).

---

### A8. `single_match` (2 instances)

**Lint:** `match` with one arm and `_ => {}` should be `if let`.

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/p2p/swarm.rs` | 345 |
| `tests/integration.rs` | 563 |

**Fix:** Replace `match x { A => { ... } _ => {} }` with `if let A = x { ... }`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A9. `unnecessary_lazy_evaluations` (4 instances)

**Lint:** `.or_else(|| x)` where `x` is not a closure; use `.or(x)`. Similarly `.ok_or_else(|| e)` where `e` is not a closure.

**Files affected:**
| File | Lines |
|------|-------|
| `server/config.rs` | 102 |
| `calendar/mirror.rs` | 56 |
| `core-engine` | 2 |

**Fix:** Replace `.or_else(|| x)` with `.or(x)` and `.ok_or_else(|| e)` with `.ok_or(e)`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A10. `unnecessary_cast` (5 instances)

**Lint:** Casting to the same type (e.g., `u64 as u64`).

**Files affected:**
| File | Lines | Cast |
|------|-------|------|
| `communerd/communerdette.rs` | 1439 | `u64 as u64` |
| `communerd/mod.rs` | 499 | `u64 as u64` |
| `core-engine` | 3 | `usize as usize` |

**Fix:** Remove the cast.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A11. `needless_borrow` (2 instances)

**Lint:** `&x` where `x` is already the right type.

**Files affected:**
| File | Lines |
|------|-------|
| `server/mod.rs` | 381 |
| `communerd/mod.rs` | 590 |

**Fix:** Remove the `&`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A12. `useless_conversion` (2 instances)

**Lint:** `.into()` or `NodeError::from()` that converts to the same type.

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/communerdette.rs` | 747, 758 |
| `server/mod.rs` | 400 |

**Fix:** Remove the `.into()` or `NodeError::from()`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A13. `question_mark` (2 instances)

**Lint:** `let Some(x) = y else { return None }` should be `let x = y?`.

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/mod.rs` | 919, 975 |

**Fix:** Replace with `?` operator.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A14. `needless_range_loop` (3 instances)

**Lint:** `for i in 0..n { arr[i] = ... }` should use iterator.

**Files affected:**
| File | Lines |
|------|-------|
| `calendar_store/mod.rs` | 174, 177 |
| `core-engine` | 1 |

**Fix:** Use `.iter_mut().enumerate()` or `.iter_mut().zip()`.

**Test plan:**
- **Test:** `test_merkle_proof_iteration` — verify proof leaves/siblings are populated correctly
- Verify: `cargo test --workspace` passes.

---

### A15. `collapsible_match` (1 instance)

**Lint:** Nested `match` that can be collapsed.

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/p2p/swarm.rs` | 345 |

**Fix:** Collapse inner match into outer pattern.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A16. `large_enum_variant` (1 instance)

**Lint:** `NetworkEvent::Identified` carries `identify::Info` (640 bytes).

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/p2p/events.rs` | 7 |

**Fix:** Box the large variant: `Identified { peer_id: PeerId, info: Box<identify::Info> }`.

**Test plan:**
- **Test:** `test_network_event_sizes` — verify enum size is reasonable after boxing
- Verify: All tests that construct `NetworkEvent::Identified` still pass.

---

### A17. `dead_code` (1 instance)

**Lint:** `verify_report_signature` is never used.

**Files affected:**
| File | Lines |
|------|-------|
| `probity/gossip_handler.rs` | 106 |

**Fix:** Either remove the function or add `#[allow(dead_code)]` with justification if it's intended for future use.

**Test plan:**
- If removed: No test needed.
- If kept: Add `#[allow(dead_code)]` with comment.

---

### A18. `manual_contains` (1 instance)

**Lint:** `!known_roots.iter().any(|r| *r == x)` should be `!known_roots.contains(&x)`.

**Files affected:**
| File | Lines |
|------|-------|
| `calendar_store/mod.rs` | 352 |

**Fix:** Replace with `.contains()`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A19. `new_without_default` (1 instance)

**Lint:** `ProbityStore::new()` exists but no `impl Default`.

**Files affected:**
| File | Lines |
|------|-------|
| `probity/store.rs` | 18 |

**Fix:** Add `impl Default for ProbityStore { fn default() -> Self { Self::new() } }`.

**Test plan:**
- No new tests needed — adds a Default impl.
- Verify: `cargo test --workspace` passes.

---

### A20. `unwrap_or_default` (2 instances)

**Lint:** `.or_insert_with(Vec::new)` should be `.or_default()`.

**Files affected:**
| File | Lines |
|------|-------|
| `calendar/mirror.rs` | 45 |
| `core-engine` | 1 |

**Fix:** Replace with `.or_default()`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A21. `manual_is_multiple_of` (2 instances)

**Lint:** `x % y == 0` should be `x.is_multiple_of(y)`.

**Files affected:**
| File | Lines |
|------|-------|
| `main.rs` | 192, 237 |

**Fix:** Replace with `.is_multiple_of()`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A22. `let_unit_value` (3 instances)

**Lint:** `let _ = expr_that_returns_unit();` is unnecessary.

**Files affected:**
| File | Lines |
|------|-------|
| `tests/integration.rs` | 261, 262, 307 |

**Fix:** Remove the `let _ =`.

**Test plan:**
- No new tests needed — test code only.
- Verify: `cargo test --workspace` passes.

---

### A23. `zombie_processes` (2 instances)

**Lint:** Spawned process not `wait()`ed on all code paths.

**Files affected:**
| File | Lines |
|------|-------|
| `tests/cli_no_hex_leak.rs` | 52, 107 |

**Fix:** Use a guard pattern or ensure `wait()` is called on all paths.

**Test plan:**
- No new tests needed — test code only.
- Verify: `cargo test --workspace` passes.

---

### A24. `unused_variables` / `unused_import` (4 instances)

**Lint:** Unused variables and imports.

**Files affected:**
| File | Lines | Item |
|------|-------|------|
| `communerd/communerdette.rs` | 3110 | `crypto` |
| `tests/toppoli.rs` | 1007 | `peer1_addr` |
| `tests/cli_no_hex_leak.rs` | 134 | `stdout_text` |
| `core-engine/integration_tests.rs` | 3 | `CryptoServer` |

**Fix:** Prefix with `_` or remove.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A25. `empty_line_after_doc_comment` (1 instance)

**Lint:** Empty line between doc comment and the item it documents.

**Files affected:**
| File | Lines |
|------|-------|
| `tests/cli_no_hex_leak.rs` | 4 |

**Fix:** Remove the empty line or convert to inner doc comment.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A26. `needless_question_mark` (1 instance)

**Lint:** `Ok(expr?)` where the `Ok` and `?` cancel out.

**Files affected:**
| File | Lines |
|------|-------|
| `tests/snapshot_tests.rs` | 108 |

**Fix:** Remove the `Ok()` wrapper.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A27. `explicit_auto_deref` (1 instance)

**Lint:** `&*x` where `&x` suffices.

**Files affected:**
| File | Lines |
|------|-------|
| `communerd/mod.rs` | 590 |

**Fix:** Replace `&*x` with `&x`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A28. `if_returns_bool` (1 instance)

**Lint:** `if cond { true } else { false }` should be just `cond`.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 1 |

**Fix:** Replace with the condition directly.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A29. `length_comparison` (2 instances)

**Lint:** `len() == 0` should be `.is_empty()`; `len() == 1` should use dedicated check.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 2 |

**Fix:** Replace with `.is_empty()` or appropriate check.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A30. `iter_copied_collect` (3 instances)

**Lint:** `.iter().copied().collect()` should be `.to_vec()`.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 3 |

**Fix:** Replace with `.to_vec()`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A31. `useless_vec` (2 instances)

**Lint:** `vec![x]` where `x` is already a Vec or unnecessary.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 2 |

**Fix:** Remove unnecessary `vec![]` wrapper.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A32. `map_or_simplify` (2 instances)

**Lint:** `map_or` that can be simplified.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 2 |

**Fix:** Simplify the expression.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A33. `writing_&Vec` / `writing_&PathBuf` (2 instances)

**Lint:** `&Vec<T>` should be `&[T]`; `&PathBuf` should be `&Path`.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 2 |

**Fix:** Change parameter types to slice/Path references.

**Test plan:**
- No new tests needed — ergonomic improvement.
- Verify: `cargo test --workspace` passes.

---

### A34. `push_after_creation` (1 instance)

**Lint:** `vec![]` followed immediately by `.push()`.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 1 |

**Fix:** Use `vec![item]` instead.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### A35. `doc_list_indent` (2 instances)

**Lint:** Doc list items without proper indentation.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 2 |

**Fix:** Fix indentation.

**Test plan:**
- No new tests needed — documentation only.
- Verify: `cargo test --workspace` passes.

---

### A36. `replacing_text_with_itself` (8 instances)

**Lint:** String replacement that replaces text with itself (no-op).

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine/build.rs` | 8 |

**Fix:** Remove the no-op replacements.

**Test plan:**
- No new tests needed — build script only.
- Verify: `cargo build --workspace` passes.

---

### A37. `borrowed_expression` (5 instances)

**Lint:** Borrowing an expression that already implements the required traits.

**Files affected:**
| File | Lines |
|------|-------|
| `core-engine` | 5 |

**Fix:** Remove unnecessary borrow.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

## Phase B: High-Value Pedantic Warnings

These are pedantic lints that have security or correctness implications.

### B1. `cast_possible_truncation` (~10 instances)

**Lint:** Casting `u64`/`u128` to `usize`/`u32` may truncate on 32-bit platforms.

**Files affected:**
| File | Lines | Cast |
|------|-------|------|
| `main.rs` | 646 | `u64 as usize` |
| `calendar_store/lru.rs` | 205 | `u128 as u64` |
| `tests/toppoli.rs` | 634 | `usize as u32` |
| `core-engine` | ~5 | Various |

**Fix:** Use `usize::try_from(x)` or `#[allow(clippy::cast_possible_truncation)]` with justification for test code.

**Test plan:**
- **Test:** `test_cast_truncation_safety` — verify that all casts are bounded by reasonable limits
- Verify: `cargo test --workspace` passes.

---

### B2. `float_cmp` (~5 instances)

**Lint:** Direct `assert_eq!` on floats.

**Files affected:**
| File | Lines |
|------|-------|
| `probity/aggregator.rs` | 102, 121 |
| `probity/store.rs` | 251 |
| `probity/mod.rs` | 82, 98, 147 |

**Fix:** Use `assert!((a - b).abs() < f32::EPSILON)` or a float comparison helper.

**Test plan:**
- No new tests needed — test code only.
- Verify: `cargo test --workspace` passes.

---

### B3. `items_after_statements` (~5 instances)

**Lint:** `const` or `fn` defined after statements in a scope.

**Files affected:**
| File | Lines |
|------|-------|
| `calendar_store/encrypted_jsonl.rs` | 292, 454, 474, 495, 517 |

**Fix:** Move `const` definitions to the top of the scope.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

### B4. `match_wildcard_for_single_variants` (~5 instances)

**Lint:** `_` wildcard matches only one remaining variant.

**Files affected:**
| File | Lines |
|------|-------|
| `main.rs` | 711 |
| `probity/gossip_handler.rs` | 186, 300, 341 |
| `core-engine` | ~2 |

**Fix:** Name the specific variant instead of `_`.

**Test plan:**
- No new tests needed — mechanical change.
- Verify: `cargo test --workspace` passes.

---

## Phase D: `too_many_lines` Refactoring (6 Production Functions)

These functions exceed the 100-line threshold from `clippy.toml` (`too-many-lines-threshold = 100`). Each requires extraction of sub-functions with dedicated tests BEFORE refactoring.

---

### D1. `verify_pair` — `core-engine/src/foretias/tick.rs:405` (~120 lines)

**Purpose:** Verify the auto-attestation between two consecutive tick records. Handles both genesis (tick 1) and normal cases.

**Current Structure:**
```
verify_pair(crypto, tbid_str, prev, curr)
  ├─ if curr.chronon_number == 1 && curr.tb_version == 1 (genesis path)
  │   ├─ Validate forward/backward foretis lengths
  │   ├─ Split forward/backward into Ed25519 sig + genesis data
  │   ├─ Verify genesis signature (tbid_verify)
  │   ├─ Build attest_blob (with or without genesis data)
  │   └─ Return (forward_sig, backward_sig, attest_blob, genesis_valid)
  └─ else (normal path)
      └─ Build attest_blob from prev/curr public keys
         Return (prev_sig, curr_sig, attest_blob, true)
```

**Sub-Functions to Extract:**

| New Function | Lines | Purpose |
|--------------|-------|---------|
| `extract_genesis_sigs(forward, backward) -> (Sig, Sig, &[u8], &[u8])` | ~30 | Split forward/backward foretis into Ed25519 sig + genesis data |
| `verify_genesis_signature(crypto, prev_tbid, genesis_blob, forward_genesis) -> bool` | ~15 | Verify the genesis split signature |
| `build_genesis_attest_blob(tbid_str, curr, stamps, nonce, genesis) -> Vec<u8>` | ~20 | Build attestation blob for genesis tick |
| `build_normal_attest_blob(tbid_str, prev, curr, stamps, nonce) -> Vec<u8>` | ~15 | Build attestation blob for normal tick |

**Test Plan (Write BEFORE Refactoring):**

```rust
// In core-engine/src/foretias/tick.rs (or tests/)

#[cfg(test)]
mod verify_pair_refactor_tests {
    use super::*;

    // --- extract_genesis_sigs tests ---

    #[test]
    fn test_extract_genesis_sigs_normal_split() {
        // Forward = 64-byte sig + 32-byte genesis
        // Backward = 64-byte sig + 32-byte genesis (same genesis)
        let forward = vec![1u8; 96];
        let backward = vec![2u8; 64];
        backward.extend_from_slice(&forward[64..]); // same genesis
        let (f_sig, f_gen, b_sig, b_gen) = extract_genesis_sigs(&forward, &backward);
        assert_eq!(f_sig.len(), 64);
        assert_eq!(f_gen.len(), 32);
        assert_eq!(f_gen, b_gen); // genesis must match
    }

    #[test]
    fn test_extract_genesis_sigs_short_forward() {
        // Forward < 64 bytes — should still work (entire forward = sig)
        let forward = vec![1u8; 50];
        let backward = vec![2u8; 64];
        let (f_sig, f_gen, _b_sig, _b_gen) = extract_genesis_sigs(&forward, &backward);
        assert_eq!(f_sig.len(), 50);
        assert!(f_gen.is_empty());
    }

    #[test]
    fn test_extract_genesis_sigs_mismatched_genesis() {
        let forward = vec![1u8; 96];
        let backward = vec![2u8; 96]; // different genesis data
        let (_f_sig, f_gen, _b_sig, b_gen) = extract_genesis_sigs(&forward, &backward);
        assert_ne!(f_gen, b_gen);
    }

    // --- verify_genesis_signature tests ---

    #[test]
    fn test_verify_genesis_signature_valid() {
        let crypto = SoftwareCryptoServer::new();
        let (pubkey, privkey) = crypto.generate_keypair("Ed25519").unwrap();
        let blob = b"test genesis blob".to_vec();
        let sig = crypto.sign_with(&privkey, "Ed25519", &blob).unwrap();
        assert!(verify_genesis_signature(&crypto, &pubkey, &blob, &sig));
    }

    #[test]
    fn test_verify_genesis_signature_tampered() {
        let crypto = SoftwareCryptoServer::new();
        let (pubkey, privkey) = crypto.generate_keypair("Ed25519").unwrap();
        let blob = b"original".to_vec();
        let sig = crypto.sign_with(&privkey, "Ed25519", &blob).unwrap();
        let tampered_blob = b"tampered".to_vec();
        assert!(!verify_genesis_signature(&crypto, &pubkey, &tampered_blob, &sig));
    }

    // --- build_genesis_attest_blob tests ---

    #[test]
    fn test_build_genesis_attest_blob_with_genesis() {
        // Verify blob structure: tbid + chronon + pubkey + chronon + pubkey + genesis + stamps + nonce
        let blob = build_genesis_attest_blob("test_tbid", &mock_curr(), 5, &[0u8; 16], Some(&[1u8; 32]));
        assert!(blob.starts_with(b"test_tbid"));
        assert!(blob.len() > 100); // includes genesis data
    }

    #[test]
    fn test_build_genesis_attest_blob_without_genesis() {
        let blob = build_genesis_attest_blob("test_tbid", &mock_curr(), 5, &[0u8; 16], None);
        assert!(blob.starts_with(b"test_tbid"));
        // Shorter than with genesis
        let blob_with = build_genesis_attest_blob("test_tbid", &mock_curr(), 5, &[0u8; 16], Some(&[1u8; 32]));
        assert!(blob.len() < blob_with.len());
    }

    // --- build_normal_attest_blob tests ---

    #[test]
    fn test_build_normal_attest_blob_structure() {
        let prev = mock_prev(1);
        let curr = mock_curr();
        let blob = build_normal_attest_blob("test_tbid", &prev, &curr, 5, &[0u8; 16]);
        assert!(blob.starts_with(b"test_tbid"));
        // Contains both prev and curr chronon numbers
        assert!(blob.contains(&1u64.to_be_bytes()[..]));
    }

    // --- Integration: verify_pair after refactoring ---

    #[test]
    fn test_verify_pair_genesis_path_unchanged() {
        // Run verify_pair on genesis data, compare result with pre-refactoring baseline
        // This is a regression test — the extracted functions must produce identical results
    }

    #[test]
    fn test_verify_pair_normal_path_unchanged() {
        // Same for normal (non-genesis) path
    }
}
```

**Refactoring Steps:**
1. Write ALL tests above against current `verify_pair` implementation
2. Extract `extract_genesis_sigs()` — run tests
3. Extract `verify_genesis_signature()` — run tests
4. Extract `build_genesis_attest_blob()` — run tests
5. Extract `build_normal_attest_blob()` — run tests
6. Rewrite `verify_pair` to use extracted functions — run ALL tests
7. Verify `verify_pair` is now < 100 lines

---

### D2. `handle_storage_proof_verify` — `foretias-server/src/server/handlers.rs:1177` (~130 lines)

**Purpose:** JSON-RPC handler for `storage_proof_verify`. Parses request/response/known_roots from JSON, verifies storage proof.

**Current Structure:**
```
handle_storage_proof_verify(server, params)
  ├─ Parse 'request' (tbid, chronon_start, chronon_end)
  ├─ Parse 'response' (coverage_ratio, blocks[])
  │   └─ For each block: parse block_id, merkle_root, leaves[], siblings[], n
  ├─ Parse 'known_roots'[]
  ├─ Call verify_storage_proof(req, resp, known_roots)
  └─ Return JSON result
```

**Sub-Functions to Extract:**

| New Function | Lines | Purpose |
|--------------|-------|---------|
| `parse_storage_proof_request(params) -> Result<Req, JsonRpcResponse>` | ~20 | Extract request fields from JSON |
| `parse_storage_proof_response(params) -> Result<Resp, JsonRpcResponse>` | ~40 | Extract response + blocks from JSON |
| `parse_single_block(bv: &Value, i: usize) -> Result<BlockProof, String>` | ~30 | Parse one block proof from JSON |
| `parse_known_roots(params) -> Vec<[u8; 32]>` | ~15 | Extract known_roots array |

**Test Plan (Write BEFORE Refactoring):**

```rust
#[cfg(test)]
mod storage_proof_handler_tests {
    // --- parse_storage_proof_request tests ---

    #[test]
    fn test_parse_request_valid() {
        let params = json!({ "request": { "tbid": "abc", "chronon_start": 1, "chronon_end": 10 } });
        let req = parse_storage_proof_request(&params).unwrap();
        assert_eq!(req.tbid, "abc");
        assert_eq!(req.chronon_start, 1);
        assert_eq!(req.chronon_end, 10);
    }

    #[test]
    fn test_parse_request_missing_tbid() {
        let params = json!({ "request": { "chronon_start": 1 } });
        assert!(parse_storage_proof_request(&params).is_err());
    }

    #[test]
    fn test_parse_request_empty_tbid() {
        let params = json!({ "request": { "tbid": "", "chronon_start": 1, "chronon_end": 10 } });
        assert!(parse_storage_proof_request(&params).is_err());
    }

    // --- parse_single_block tests ---

    #[test]
    fn test_parse_block_valid() {
        let bv = json!({
            "block_id": 1, "merkle_root": "a".repeat(64),
            "leaves": [], "leaf_count": 0, "siblings": [], "sibling_count": 0, "n": 1
        });
        let block = parse_single_block(&bv, 0).unwrap();
        assert_eq!(block.block_id, 1);
    }

    #[test]
    fn test_parse_block_invalid_merkle_root() {
        let bv = json!({ "block_id": 1, "merkle_root": "short" });
        assert!(parse_single_block(&bv, 0).is_err());
    }

    #[test]
    fn test_parse_block_with_leaves() {
        let leaves: Vec<String> = (0..4).map(|_| "a".repeat(64)).collect();
        let bv = json!({
            "block_id": 1, "merkle_root": "a".repeat(64),
            "leaves": leaves, "leaf_count": 4, "siblings": [], "sibling_count": 0, "n": 4
        });
        let block = parse_single_block(&bv, 0).unwrap();
        assert_eq!(block.leaf_count, 4);
    }

    // --- parse_known_roots tests ---

    #[test]
    fn test_parse_known_roots_valid() {
        let roots: Vec<String> = vec!["a".repeat(64), "b".repeat(64)];
        let params = json!({ "known_roots": roots });
        let result = parse_known_roots(&params);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_known_roots_invalid_length() {
        let params = json!({ "known_roots": ["short"] });
        let result = parse_known_roots(&params);
        assert!(result.is_empty()); // invalid roots skipped
    }

    // --- Integration ---

    #[test]
    fn test_handle_storage_proof_verify_end_to_end() {
        // Full handler with valid request/response/known_roots
        // Verify result.verified and result.coverage_ratio
    }
}
```

**Refactoring Steps:**
1. Write tests against current implementation
2. Extract `parse_storage_proof_request()` — run tests
3. Extract `parse_single_block()` — run tests
4. Extract `parse_storage_proof_response()` — run tests
5. Extract `parse_known_roots()` — run tests
6. Rewrite handler to use extracted functions — run ALL tests

---

### D3. `handle_do_chronon_attestation` — `foretias-server/src/calendar/task_queue.rs:305` (~155 lines)

**Purpose:** Execute chronon-level mutual attestation with a target TBID.

**Current Structure:**
```
handle_do_chronon_attestation(worker_id, target_tbid, ctx)
  ├─ Resource gate (communerd, chronomatter)
  ├─ Parse TBID from hex
  ├─ Step 2: Get target's latest tick
  ├─ Step 3: Stamp via Chronomatter
  ├─ Step 4: Sign with Calendar key
  ├─ Step 5: Transmit via line.stamp()
  └─ Step 6: Best-effort FB verification
```

**Sub-Functions to Extract:**

| New Function | Lines | Purpose |
|--------------|-------|---------|
| `gate_attestation_resources(ctx) -> Result<(Arc<Communerd>, Arc<Chronomatter>), ()` | ~15 | Check Communerd + Chronomatter availability |
| `fetch_target_latest_tick(line, target_tbid) -> Result<CleanAuthenticated<ChrononRecord>, ()` | ~15 | Get target's latest tick |
| `stamp_and_sign_chronon(chronomatter, ctx, target_tbid) -> Result<StampedContent, ()` | ~20 | Stamp + sign with Calendar key |
| `transmit_chronon_attestation(line, foretis_bytes, echo) -> Result<..., ()` | ~15 | Send attestation via line.stamp() |
| `verify_attestation_recorded(line, target_chronon, echo) -> bool` | ~30 | Best-effort FB verification |

**Test Plan (Write BEFORE Refactoring):**

```rust
#[cfg(test)]
mod chronon_attestation_tests {
    // --- gate_attestation_resources tests ---

    #[test]
    fn test_gate_returns_err_when_no_communerd() {
        let ctx = WorkerContext { communerd: None, .. };
        assert!(gate_attestation_resources(&ctx).is_err());
    }

    #[test]
    fn test_gate_returns_err_when_no_chronomatter() {
        let ctx = WorkerContext { chronomatter: None, .. };
        assert!(gate_attestation_resources(&ctx).is_err());
    }

    #[test]
    fn test_gate_returns_ok_when_both_present() {
        let ctx = WorkerContext { communerd: Some(...), chronomatter: Some(...), .. };
        assert!(gate_attestation_resources(&ctx).is_ok());
    }

    // --- stamp_and_sign_chronon tests ---

    #[test]
    fn test_stamp_content_format() {
        // Verify stamp content is "chronon-attest-{target_tbid}"
    }

    #[test]
    fn test_stamp_and_sign_returns_sig_input() {
        // Verify the stamped content contains valid sig_input_bytes
    }

    // --- verify_attestation_recorded tests ---

    #[test]
    fn test_verify_attestation_found() {
        // Mock a record with matching echo + content_hash
        assert!(verify_attestation_recorded(...));
    }

    #[test]
    fn test_verify_attestation_not_found() {
        // Mock a record without matching attestation
        assert!(!verify_attestation_recorded(...));
    }

    // --- Integration ---

    #[test]
    fn test_handle_do_chronon_attestation_full_flow() {
        // Mock all dependencies, verify full attestation flow
    }
}
```

---

### D4. `handle_do_epoch_attestation` — `foretias-server/src/calendar/task_queue.rs:470` (~120 lines)

**Purpose:** Execute epoch-level mutual attestation. Similar to D3 but uses `get_calendar_slice` instead of `get_tick`.

**Current Structure:**
```
handle_do_epoch_attestation(worker_id, target_tbid, ctx)
  ├─ Resource gate (same as D3)
  ├─ Parse TBID from hex
  ├─ Step 2: Fetch target's latest epoch via get_calendar_slice
  ├─ Step 3: Stamp via Chronomatter
  ├─ Step 4: Sign with Calendar key
  └─ Step 5: Transmit via line.stamp()
```

**Sub-Functions to Extract:**

| New Function | Lines | Purpose |
|--------------|-------|---------|
| `fetch_target_latest_epoch(line, target_tbid) -> Result<ChrononRecord, ()` | ~20 | Get latest epoch via calendar slice |
| `stamp_and_sign_epoch(chronomatter, ctx, target_tbid) -> Result<StampedContent, ()` | ~20 | Stamp + sign for epoch attestation |

**Note:** `gate_attestation_resources` and `transmit_chronon_attestation` can be shared with D3.

**Test Plan:** Same pattern as D3, focused on epoch-specific differences.

---

### D5. `handle_verify_fb_recorded` — `foretias-server/src/calendar/task_queue.rs:781` (~135 lines)

**Purpose:** Verify that FullyBound attestations are recorded on a target TBID.

**Current Structure:**
```
handle_verify_fb_recorded(worker_id, target_tbid, ctx)
  ├─ Resource gate (communerd only)
  ├─ Parse TBID from hex
  ├─ Step 2: Fetch target's latest chronon
  ├─ Step 3: Pick random chronon range
  ├─ Step 4: Fetch chronon slice
  └─ Step 5: Log coverage results
```

**Sub-Functions to Extract:**

| New Function | Lines | Purpose |
|--------------|-------|---------|
| `gate_communerd_resource(ctx) -> Result<Arc<Communerd>, ()` | ~10 | Check Communerd availability |
| `pick_random_chronon_range(latest_chronon) -> (u64, u64)` | ~15 | Select random start + count |
| `fetch_and_analyze_coverage(line, start, count) -> CoverageResult` | ~30 | Fetch slice, compute coverage |
| `log_coverage_warnings(target_tbid, coverage: &CoverageResult)` | ~20 | Log poor coverage / missing attestations |

**Test Plan:**

```rust
#[cfg(test)]
mod verify_fb_recorded_tests {
    #[test]
    fn test_pick_random_range_single_chronon() {
        let (start, count) = pick_random_chronon_range(1);
        assert_eq!(start, 1);
        assert!(count >= 1);
    }

    #[test]
    fn test_pick_random_range_large_chronon() {
        let (start, count) = pick_random_chronon_range(1000);
        assert!(start <= 1000);
        assert!(count <= 100);
    }

    #[test]
    fn test_coverage_ratio_calculation() {
        // Verify coverage_ratio = returned / requested
    }

    #[test]
    fn test_log_warnings_on_poor_coverage() {
        // Mock coverage < 50%, verify warning logged
    }
}
```

---

### D6. `Communerd` struct fields — `foretias-server/src/communerd/mod.rs:176` (type_complexity)

**Purpose:** The `pending_family_lookups` field has a very complex type that needs a `type` alias.

**Current Type:**
```rust
Arc<parking_lot::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<Vec<u8>>>>>>
```

**Fix:** Add type alias at module level:
```rust
type PendingFamilyLookupMap = Arc<parking_lot::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<Vec<u8>>>>>>;
type PendingLookupMap = Arc<parking_lot::Mutex<HashMap<kad::RecordKey, tokio::sync::oneshot::Sender<Option<PeerRegistrationRecord>>>>>;
```

**Test Plan:** No new tests needed — `type` aliases are compile-time only.

---

### D7. `gossip_event_loop` — `foretias-server/src/communerd/mod.rs:551` (too_many_arguments + too_many_lines)

**Purpose:** Main gossip event loop that processes NetworkEvents.

**Current Args (11):** events, cmd_tx, probity_store, crypto, clock, namespace, tbid, communerd, collision_config, heartbeat_config, recompute_interval

**Fix:** Create `GossipLoopConfig` struct to bundle the 11 parameters.

**Test Plan:** Same as A5 above.

---

## Phase C: Suppressions (With Justification)

These warnings are intentional and should be suppressed with documented justifications.

### C1. `too_many_arguments` — CLI Commands

**Functions:** `cmd_serve` (14 args), `cmd_verify` (8 args), `cmd_prove_verification` (15 args)

**Justification:** CLI command functions derive their arguments from the clap `Args` struct. Grouping them into a config struct would add indirection without clarity, since the args are destructured immediately from the CLI parser.

**Suppression:** `#[allow(clippy::too_many_arguments)]` with comment.

---

### C2. `large_enum_variant` — `NetworkEvent`

**Type:** `NetworkEvent::Identified { info: identify::Info }` (640 bytes)

**Justification:** Boxing `identify::Info` adds heap allocation on every identification event. The enum is used in an mpsc channel where the overhead is negligible compared to network I/O.

**Decision:** Box the variant (Phase A16) rather than suppress — the fix is clean and the size reduction is meaningful.

---

### C3. `private_interfaces` — Communerdette

**Types:** `CommunerdetteExecutor`, `Communerdette`, `CommunerdetteHost`

**Justification:** The visibility hierarchy reflects the module architecture: `CommunerdetteLine` is the public handle, `Communerdette` is the internal manager, `CommunerdetteExecutor` is the private worker. The `pub(super)` methods are only called from within the `communerd` module.

**Fix:** Narrow `CommunerdetteLine::new` to `pub(super)` or widen `CommunerdetteExecutor` to `pub(super)`.

---

## Test Strategy

### Per-Phase Testing

| Phase | Test Approach | Verification |
|-------|--------------|--------------|
| A1-A4 | No new tests; workspace test suite | `cargo test --workspace` |
| A5 | New unit tests for config structs | `cargo test -p foretias-server -- gossip_loop_config` |
| A6-A15 | No new tests; workspace test suite | `cargo test --workspace` |
| A16 | New size test | `cargo test -p foretias-server -- network_event_sizes` |
| A17 | N/A (removal or suppression) | `cargo build --workspace` |
| A18-A37 | No new tests; workspace test suite | `cargo test --workspace` |
| B1 | New cast safety test | `cargo test -p foretias-server -- cast_truncation` |
| B2-B4 | No new tests; workspace test suite | `cargo test --workspace` |

### CI Gate Verification

After all fixes:
```bash
cd p2p && cargo clippy --workspace --all-targets -- -D warnings
```

This must produce zero warnings.

---

## Implementation Order

### Wave 1: Auto-Fixable (No Tests Needed)
1. **Phase A1, A3, A6, A8-A13, A15, A18-A37** — Mechanical fixes via `cargo clippy --fix` or direct edits
2. Verify: `cargo test --workspace` passes

### Wave 2: Struct Extraction (Tests First)
3. **Phase A5** — `GossipLoopConfig`, `RegistrationConfig` structs
4. Write tests → extract structs → verify tests pass

### Wave 3: Type Aliases + Visibility
5. **Phase A4, A7** — `type` aliases, visibility adjustments
6. Verify: `cargo build --workspace` passes

### Wave 4: `too_many_lines` Refactoring (Tests First, Then Extract)
7. **Phase D6** — Type aliases for Communerd fields (no tests needed)
8. **Phase D1** — `verify_pair`: Write tests → extract 4 sub-functions → verify
9. **Phase D2** — `handle_storage_proof_verify`: Write tests → extract 4 sub-functions → verify
10. **Phase D3** — `handle_do_chronon_attestation`: Write tests → extract 5 sub-functions → verify
11. **Phase D4** — `handle_do_epoch_attestation`: Write tests → extract 2 sub-functions (share with D3) → verify
12. **Phase D5** — `handle_verify_fb_recorded`: Write tests → extract 4 sub-functions → verify
13. **Phase D7** — `gossip_event_loop`: Bundle into `GossipLoopConfig` (same as A5)

### Wave 5: High-Value Pedantic
14. **Phase B1-B4** — Cast truncation, float cmp, items after statements, wildcard matches

### Wave 6: Suppressions
15. **Phase C** — Add `#[allow]` with justifications

### Wave 7: Final Verification
16. `cargo clippy --workspace --all-targets -- -D warnings` → zero warnings
17. `cargo test --workspace` → all tests pass

---

## Success Criteria

| Criteria | Verification |
|----------|--------------|
| Zero standard clippy warnings | `cargo clippy --workspace --all-targets 2>&1 \| grep -c "^warning" = 0` |
| Zero high-value pedantic warnings | Same command with `-W clippy::pedantic` for targeted lints |
| All tests pass | `cargo test --workspace` exits 0 |
| CI gate passes | `.github/workflows/clippy.yml` shows green |
| No behavior changes | All existing tests pass; new tests verify struct extraction |

---

## Notes

- This spec covers ALL standard clippy warnings, high-value pedantic warnings, AND `too_many_lines` refactoring
- Low-value pedantic warnings (uninlined_format_args, missing_panics_doc, etc.) are out of scope
- Phase D (`too_many_lines`) requires tests BEFORE refactoring — each sub-function is tested independently
- Shared sub-functions between D3/D4 (attestation handlers) reduce duplication
- Each fix is verified by the workspace test suite; new tests are added for struct extraction, enum boxing, and function refactoring
- The goal is a clean CI gate (`cargo clippy --workspace --all-targets -- -D warnings` produces zero warnings)
