# COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_PLAN.md

**Date:** 2026-05-22 (completed 2026-05-23)
**Application:** Foretias v0.3+
**Paired Spec:** `COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_SPEC.md`
**Supersedes Plan for:** `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md` (Take 2)
**Status:** ✅ MERGED to alpha 2026-05-23 (commits 7fb2549, 0442683). Checkboxes marked 2026-06-02.

---

## Scope

This is a **Major** refactoring of the trust boundary type system. It eliminates concrete newtype
wrappers (`UnprocessedChrononRecord`, `CleanAuthenticatedChrononRecord`, etc.) in favor of direct
use of `Unprocessed<ChrononRecord>` and `CleanAuthenticated<ChrononRecord>`, with `HasVerify` and
`HasExternalize` traits as single, auditable inbound and outbound gates.

**Per Development Rules: do not begin Phase work while any tests are broken.**

---

## Coherence Exceptions

None. `ProbityReport` is moved to `core-engine` as part of this refactoring (Stage 4), which
eliminates the only case where a concrete newtype wrapper was required. After that migration,
all domain types follow the same pattern with zero exceptions.

---

## Worktree

```
FULL_WORKTREE_PATH = ${HOME}/tmp/foretias-worktrees/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_7342
BRANCH_NAME        = group1-type-safety-take3
```

---

## Plan

### Stage 0 — Pre-flight

- [x] Confirm `cargo test --workspace` passes with zero failures
  - Per Development Rules, Phase work must not begin while any test is broken.

### Stage 1 — Worktree Setup

- [x] Create worktree: `git worktree add -b group1-type-safety-take3 ${HOME}/tmp/foretias-worktrees/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_7342`
- [x] Set session working directory to `${HOME}/tmp/foretias-worktrees/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_7342`
- [x] Export build env: `export CARGO_TARGET_DIR="$HOME/.cache/cargo/foretias-group1-take3"` and `export CMAKE_BUILD_PARALLEL_LEVEL=10`

### Stage 2 — (Removed — no coherence exception audit needed)

### Stage 3 — Refactor `core-engine/src/foretias/clean_auth.rs`

All edits in this stage are to `p2p/core-engine/src/foretias/clean_auth.rs` unless otherwise noted.

#### 3a — Generic inherent methods (no delegate needed)

- [x] Add to `impl<T> Unprocessed<T>`:
  - `pub fn from_parsed(inner: T) -> Self`
  - `pub fn inner(&self) -> &T`
  - `pub fn into_inner(self) -> T`
- [x] Add to `impl<T: serde::de::DeserializeOwned> Unprocessed<T>`:
  - `pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError>`
  - `pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError>`
- [x] Add to `impl<T> CleanAuthenticated<T>`:
  - `pub fn from_trusted(inner: T) -> Self`
  - `pub fn inner(&self) -> &T`
  - `pub fn into_inner(self) -> T`

#### 3b — (No traits needed — verify and externalize are direct methods)

No `HasVerify` trait, no `HasExternalize` trait, no `VerifyContext` enum.
The `verify` and `externalize` methods are written directly on concrete impls outside the macro.
See Stage 3d (verify) and Stage 3f (externalize) for those tasks.

#### 3c — `create_type_gated_classes!` macro (aliases only)

- [x] Define macro that generates type aliases only (no logic):
  ```rust
  macro_rules! create_type_gated_classes {
      ($t:ident, $up:ident, $ca:ident) => {
          pub type $up = Unprocessed<$t>;
          pub type $ca = CleanAuthenticated<$t>;
      };
  }
  ```
- [x] Invoke macro for each domain type:
  - `create_type_gated_classes!(ChrononRecord, UnprocessedChrononRecord, CleanAuthenticatedChrononRecord)`
  - `create_type_gated_classes!(Foretis, UnprocessedForetis, CleanAuthenticatedForetis)`
  - `create_type_gated_classes!(EpochSnapshot, UnprocessedEpochSnapshot, CleanAuthenticatedEpochSnapshot)`

#### 3d — `verify` methods (inbound gate, one per domain type, outside the macro)

Each is its own explicit, auditable `impl` block. No shared trait, no `VerifyContext` enum.
Each `verify` takes exactly the parameters its type requires.

- [x] `impl Unprocessed<ChrononRecord>`:
  - `pub fn verify(self, crypto: &dyn CryptoServer, prev: Option<&CleanAuthenticated<ChrononRecord>>) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError>`
    — moves verification logic from former `UnprocessedChrononRecord::into_clean_authenticated`
    — `prev = None` is genesis (tick 1)
  - Field accessors: `chronon_number`, `public_key`, `signature_algorithm`, `forward_foretis`,
    `backward_foretis`, `aa_nonce`, `chronon_stamp_count`, `external_attestations`, `tb_version`, `tbid`
- [x] `impl CleanAuthenticated<ChrononRecord>`:
  - Field accessors (same list)
- [x] `impl Unprocessed<Foretis>`:
  - `pub fn verify(self, crypto: &dyn CryptoServer, record: &CleanAuthenticated<ChrononRecord>, content: &[u8]) -> Result<CleanAuthenticated<Foretis>, CleanAuthError>`
    — moves verification logic from former `UnprocessedForetis::into_clean_authenticated`
  - Field accessors for Foretis
- [x] `impl CleanAuthenticated<Foretis>`:
  - Field accessors
- [x] `impl Unprocessed<EpochSnapshot>`:
  - `pub fn verify(self, crypto: &dyn CryptoServer, committee_pubkeys: &[SignatureBytes], threshold: usize) -> Result<CleanAuthenticated<EpochSnapshot>, CleanAuthError>`
    — retain `NotYetImplemented` stub behavior from former impl
  - Field accessors
- [x] `impl CleanAuthenticated<EpochSnapshot>`:
  - Field accessors
- [x] Decide whether to keep ergonomic aliases (`into_clean_authenticated`, `into_clean_authenticated_genesis`)
  as thin wrappers calling `verify`, or update all callers to use `verify` directly

#### 3e — (Removed — was the ergonomic wrapper step; folded into 3d above)

#### 3f — `externalize` methods (outbound gate, one per domain type, outside the macro)

Each is its own explicit, auditable `impl` block. No `HasExternalize` trait.

- [x] `impl CleanAuthenticated<ChrononRecord>`:
  - `pub fn externalize(self) -> ExternalizedChrononRecord`
    — moves logic from former `CleanAuthenticatedChrononRecord::externalize`
- [x] `impl ExternalizedChrononRecord`:
  - `pub fn reconstruct(self) -> Result<Unprocessed<ChrononRecord>, ParseError>`
- [x] `impl CleanAuthenticated<Foretis>`:
  - `pub fn externalize(self) -> ExternalizedForetis`
- [x] `impl ExternalizedForetis`:
  - `pub fn reconstruct(self) -> Result<Unprocessed<Foretis>, ParseError>`
- [x] `impl CleanAuthenticated<EpochSnapshot>`:
  - `pub fn externalize(self) -> ExternalizedEpochSnapshot` (stub or full impl)

#### 3g — Remove concrete newtypes and delegate macro

- [x] Delete `struct UnprocessedChrononRecord`
- [x] Delete `struct CleanAuthenticatedChrononRecord`
- [x] Delete `struct UnprocessedForetis`
- [x] Delete `struct CleanAuthenticatedForetis`
- [x] Delete `struct UnprocessedEpochSnapshot`
- [x] Delete `struct CleanAuthenticatedEpochSnapshot`
- [x] Remove all `delegate::delegate!` macro invocations from `clean_auth.rs`
- [x] Remove `TrustedInner` trait if now unused (or retain if still referenced elsewhere — check before deleting)
- [x] Remove `delegate` from `core-engine/Cargo.toml` if no longer referenced anywhere in `core-engine`

### Stage 4 — Move `ProbityReport` to `core-engine`

- [x] Move `ProbityReport` struct definition to `core-engine/src/probity/` (or `core-engine/src/foretias/probity.rs`)
  - Fields only — no server-specific logic moves
- [x] Move or define `ExternalizedProbityReport` struct in `core-engine`
- [x] Add type aliases via the macro: `create_type_gated_classes!(ProbityReport, UnprocessedProbityReport, CleanAuthenticatedProbityReport)`
- [x] Write `impl Unprocessed<ProbityReport>`:
  - `pub fn verify(self, crypto: &dyn CryptoServer) -> Result<CleanAuthenticated<ProbityReport>, CleanAuthError>`
    — move verification logic from `foretias-server/src/probity/clean_auth.rs` here
  - Field accessors
- [x] Write `impl CleanAuthenticated<ProbityReport>`:
  - `pub fn externalize(self) -> ExternalizedProbityReport`
  - Field accessors
- [x] Re-export `ProbityReport` from `core-engine/src/lib.rs` so `foretias-server` import paths stay stable
- [x] Delete `foretias-server/src/probity/clean_auth.rs` concrete newtypes:
  - `pub struct UnprocessedProbityReport(pub Unprocessed<ProbityReport>)`
  - `pub struct CleanAuthenticatedProbityReport(CleanAuthenticated<ProbityReport>)`
- [x] Update all `foretias-server` call sites to use `Unprocessed<ProbityReport>` directly
- [x] Verify zero coherence exceptions remain in workspace

### Stage 5 — Update Call Sites

- [x] Update `p2p/foretias-server/tests/e2e_verification.rs`:
  - `test_e2e_type_discipline_enforced`: replace `UnprocessedChrononRecord(Unprocessed::from_parsed(record))`
    with `Unprocessed::<ChrononRecord>::from_parsed(record)` — type aliases cannot be used as tuple constructors
  - Remove import of `UnprocessedChrononRecord` from `e2e_verification.rs` if no longer needed
    (the type alias still exists, but constructing via alias is impossible; use the generic directly)
- [x] Audit `foretias-server/src/` for any remaining uses of old concrete newtype constructors
  (grep: `UnprocessedChrononRecord(`, `CleanAuthenticatedChrononRecord(`, etc.)
- [x] Audit `foretias-client/src/` for the same
- [x] Audit `core-engine/src/chronomatter/` for the same
- [x] Resolve all compilation errors in changed call sites

### Stage 6 — Update Snapshot Test (trust_boundary_type_usage.rs)

- [x] Read current `p2p/core-engine/tests/trust_boundary_type_usage.rs` in full
- [x] Update `syn` visitor to emit per-function-signature-position rows:
  - Visit `ItemFn`, `ImplItemFn`, `TraitItemFn`
  - For each `FnArg::Typed` parameter, record role as `param_0`, `param_1`, ... `param_N`
  - For `ReturnType::Type`, record role as `return`
  - For `FnArg::Receiver` (`self`/`&self`/`&mut self`), skip (not a trust boundary crossing)
  - Emit a row only when the type contains `Unprocessed<`, `CleanAuthenticated<`, or `Externalized<`
  - Column order: `file | struct_or_impl | method | role | inner_type | Unprocessed | CleanAuthenticated | Externalized`
  - `struct_or_impl` = enclosing impl type name (e.g., `ChrononRecord`) or `—` for free functions
  - `inner_type` = the `T` extracted from `Unprocessed<T>` / `CleanAuthenticated<T>` / `Externalized<T>`
  - Boolean columns: `Y` if that wrapper appears, `-` if not
- [x] Regenerate snapshot: `UPDATE_SNAPSHOT=1 cargo test -p foretias-core trust_boundary_type_usage`
- [x] Review regenerated snapshot: confirm inbound gate functions show `Unprocessed` → `CleanAuthenticated`,
  outbound gate shows `CleanAuthenticated` → `Externalized`, no `Unprocessed` in domain-logic methods

### Stage 7 — Verification Checklist (Spec Section 7)

- [x] No `struct UnprocessedXxx` or `struct CleanAuthenticatedXxx` in `core-engine/src/foretias/clean_auth.rs`
- [x] `Unprocessed<T>` has generic inherent methods: `from_parsed`, `inner`, `into_inner`, `from_bytes`, `from_json_value`
- [x] `CleanAuthenticated<T>` has generic inherent methods: `from_trusted`, `inner`, `into_inner`
- [x] No `HasVerify` trait, no `HasExternalize` trait, no `VerifyContext` enum
- [x] `impl Unprocessed<ChrononRecord>`, `impl Unprocessed<Foretis>`, `impl Unprocessed<EpochSnapshot>` each have a concrete `verify(...)` method (outside the macro)
- [x] `impl CleanAuthenticated<ChrononRecord>`, `impl CleanAuthenticated<Foretis>`, `impl CleanAuthenticated<EpochSnapshot>` each have a concrete `externalize(...)` method (outside the macro)
- [x] `create_type_gated_classes!` macro generates type aliases only (no logic)
- [x] `delegate::delegate!` removed from `core-engine/src/foretias/clean_auth.rs`
- [x] `delegate` removed from `core-engine/Cargo.toml` if not used elsewhere
- [x] All existing call sites in `foretias-server`, `foretias-client`, `chronomatter` compile unchanged
- [x] `e2e_verification.rs` test uses `Unprocessed::<ChrononRecord>::from_parsed(record)` not old tuple constructor
- [x] Trust boundary snapshot test updated to per-signature-position row format
- [x] `ProbityReport` in `core-engine`; `UnprocessedProbityReport` and `CleanAuthenticatedProbityReport` concrete newtypes gone from `foretias-server`
- [x] Zero coherence exceptions remain in the workspace
- [x] `cargo test --workspace` passes with zero test failures

### Stage 8 — Commit and Merge

- [x] Verify all work is complete in `${HOME}/tmp/foretias-worktrees/COMBINED_GROUP1_TYPE_BASED_SAFETY_ENFORCEMENT_TAKE_3_7342` and committed to `group1-type-safety-take3`
- [x] Merge `group1-type-safety-take3` to alpha
  - Resolve any merge conflicts; do NOT use force-push or `--no-verify`
- [x] Finalize:
  - [x] Check that this PLAN.md has all but Cleanup checkboxes completed
  - [x] `cargo test --workspace` passes on alpha after merge
  - [x] This is the last checkbox to be checked in this PLAN.md
