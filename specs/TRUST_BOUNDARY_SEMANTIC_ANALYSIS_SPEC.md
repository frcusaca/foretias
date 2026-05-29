# TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md

**Date:** 2026-05-28
**Application:** Foretias — trust boundary enforcement tests
**Extends:** `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md`

---

## 1. Purpose

Extend the existing trust boundary snapshot test with a second analysis layer that detects
wrapper type usage through the type system, not only through source text. The two layers are
additive: the existing AST layer is not removed or changed.

---

## 2. Background: The Gap in AST Analysis

The existing test (`trust_boundary_type_usage.rs`) uses `syn` to parse source files as text
ASTs and scans `TypePath` nodes for the literal identifiers `Unprocessed`, `CleanAuthenticated`,
`Externalized`. This detects every place the code explicitly wrote one of those names.

It does **not** detect:

- A type alias `type Foo = Unprocessed<Bar>` used as a function parameter or return type —
  `Foo` is a bare identifier in the AST with no angle brackets; the visitor never fires.
- A newtype wrapper `struct Gate(Unprocessed<Bar>)` whose field type is one of the wrappers.
- Any indirection where a name other than the three canonical identifiers carries wrapper
  semantics.

The aliases deleted in May 2026 (`UnprocessedChrononRecord`, etc.) were completely invisible
to the AST scanner. The scanner only began enforcing them after the aliases were removed and
the generic form was written literally. Any future alias reintroduction would silently bypass
enforcement again.

---

## 3. Decision: rustdoc JSON

### 3.1 Options Considered

| Option | Description | Works on stable | Effort |
|--------|-------------|-----------------|--------|
| `rustdoc --output-format json` | Subprocess; parse rustdoc's JSON output which is derived from the compiler's resolved type graph | Yes (stable since ~1.76) | Low–Medium |
| `ra_ap_hir` (rust-analyzer crates) | Embed rust-analyzer's type inference engine | Yes | High |
| `rustc_interface` driver | Write a compiler driver; query `TyCtxt` directly | No (requires `rustc_private`, nightly only) | Very high |

### 3.2 Decision

Use **rustdoc JSON** (option 1). Invoke `cargo rustdoc` as a subprocess from the test,
parse the output JSON with the `rustdoc-types` crate (which mirrors the exact schema rustdoc
emits), and query the resolved type graph.

### 3.3 Limitation Note — as of 2026-05-28

rustdoc JSON is the best available mechanism for stable-Rust type-system analysis without
compiler internals. It operates on the same resolved `TyCtxt` as the compiler, so type aliases
are expanded and trait implementations are enumerated correctly.

However:
- rustdoc JSON's schema is versioned independently of Rust; the `format_version` field in the
  output must be checked and the test should fail with a clear message on a version mismatch
  rather than silently producing incorrect results.
- Cross-crate analysis requires invoking rustdoc separately per crate of interest.
- rustdoc JSON captures the *declared* type graph, not runtime values. It cannot observe
  dynamic dispatch.

If a mechanism with lower friction or more stable versioning becomes available (e.g., a
stabilized `rustc_interface` subset, or a `cargo` plugin API), this spec should be revisited.

---

## 4. Analysis Layers

The test file is restructured so each concern is in its own function with no cross-calling
except at the top-level snapshot test entry points.

```
┌──────────────────────────────────────────────────────────┐
│  collect_ast_usages(workspace)  [existing, unchanged]    │
│    syn parse → visit TypePath nodes                      │
│    returns Vec<AstOccurrence>                            │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  collect_semantic_usages(workspace)  [new]               │
│    invoke cargo rustdoc → parse JSON → walk fn items     │
│    resolve param/return type IDs through alias chain     │
│    returns Vec<SemanticOccurrence>                       │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  merge_into_table(ast, semantic) → Vec<TableRow>         │
│    join on (file, function, position)                    │
│    annotate each wrapper column with detection mode      │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  verify_ast_invariants(ast)  [existing rules, unchanged] │
│  verify_semantic_invariants(semantic)  [new rules]       │
└──────────────────────────────────────────────────────────┘
```

---

## 5. Data Model

### 5.1 Detection Mode

```rust
pub enum DetectionMode {
    /// The source code literally wrote `Unprocessed<Foo>` (or equivalent wrapper).
    /// Detected by: AST TypePath visitor.
    Literal,

    /// The type at this position resolves through the type system to a wrapper type,
    /// but was not written literally (e.g. written as a type alias or newtype).
    /// Detected by: rustdoc JSON type resolution.
    Resolved,
}
```

A position may carry both modes simultaneously (e.g. a transparent newtype whose field is
also written literally in the same file). Both are recorded independently; the table renders
them as `literal`, `resolved`, or `literal+resolved`.

### 5.2 Semantic Occurrence

```rust
pub struct SemanticOccurrence {
    pub file: String,         // relative path, e.g. "foretias-server/src/communerd/mod.rs"
    pub function: String,     // fn item path, e.g. "Communerdette::execute_stamp"
    pub position: Position,   // Param(index, name) or Return
    pub wrapper: WrapperKind, // Unprocessed | CleanAuthenticated | Externalized
    pub inner: String,        // resolved inner type name, e.g. "ChrononRecord"
    pub written_as: String,   // the name as written in source, e.g. "UnprocessedChrononRecord"
                              // equals inner wrapper form when Literal; differs when Resolved
}

pub enum Position {
    Param { index: usize, name: String },
    Return,
}

pub enum WrapperKind {
    Unprocessed,
    CleanAuthenticated,
    Externalized,
}
```

### 5.3 Table Row

Each row represents one (file, function, position) triple. Wrapper columns hold the detection
mode string or `-`.

```
file | function | position | Unprocessed | CleanAuthenticated | Externalized
```

Column values: `-`, `literal`, `Resolved(ERROR!)`, `literal+Resolved(ERROR!)`.

`Resolved(ERROR!)` means the type system found a wrapper type through alias or newtype
resolution at this position. This is always a violation regardless of file type. The row
exists to show where the indirection is being used; the alias or newtype definition is the
root cause to fix.

---

## 6. Semantic Analysis Function Specification

### 6.1 `invoke_rustdoc_json(workspace, crate_name) -> serde_json::Value`

- Runs: `cargo rustdoc -p <crate_name> -- --output-format json --document-private-items`
  with `CARGO_TARGET_DIR` pointing to a scratch directory to avoid clobbering normal build
  artifacts.
- Reads the output JSON from `target/doc/<crate_name_underscored>.json`.
- Checks `format_version` field; panics with a clear message if the version is not one of the
  known-good versions the parser was written against.
- Returns the raw JSON value for further processing.

### 6.2 `extract_fn_signatures(json) -> Vec<RawFnSignature>`

Walks the `index` map of the rustdoc JSON. For each item with `"kind": "function"` or
`"kind": "method"`:

- Records the item's `name`, its parent path (for methods: the `impl` block's self type).
- Records each input as `(index, name, type_id)`.
- Records the output as `type_id` (may be unit/absent).
- Returns `Vec<RawFnSignature>` — no resolution performed yet.

### 6.3 `resolve_type(type_node, index) -> ResolvedType`

Recursively resolves a rustdoc JSON `Type` node:

- `ResolvedPath` → look up the item in `index`; if the item is a `TypeAlias`, recurse into
  its inner type. If it is a `Struct` with kind `Plain` whose only field is a wrapper type,
  note the newtype relationship.
- `Generic` → record as-is (e.g. the `T` in an `impl<T>` block).
- Any other variant → record as opaque.

Returns:

```rust
pub struct ResolvedType {
    pub wrapper: Option<WrapperKind>,   // Some if the resolved chain ends at a wrapper
    pub inner: Option<String>,          // The resolved inner type name if wrapper is Some
    pub written_as: String,             // The name as it appeared in the source position
}
```

### 6.4 `collect_semantic_usages(workspace) -> Vec<SemanticOccurrence>`

For each crate in `["foretias-core", "foretias-server"]`:

1. Call `invoke_rustdoc_json`.
2. Call `extract_fn_signatures`.
3. For each signature, call `resolve_type` on each parameter and the return type.
4. For each resolved type where `wrapper` is `Some`, emit a `SemanticOccurrence` with
   `written_as` taken from the unresolved position.
5. Filter out occurrences where `written_as` already equals the canonical generic form
   (`Unprocessed<X>`, `CleanAuthenticated<X>`, `Externalized<X>`) — those will be covered
   by the AST layer with the `Literal` mode. Emit them as `Resolved` only.

   (@human: step 5 avoids double-counting. A position that is both literal AND resolved gets
   `Literal` from the AST pass and `Resolved` from the semantic pass; the merge step combines
   them into `literal+resolved`. The semantic pass itself only emits `Resolved`.)

### 6.5 `verify_semantic_invariants(usages: &[SemanticOccurrence]) -> Vec<String>`

Applies the same structural rules as the existing `verify_ast_invariants`, but over the
semantically detected occurrences. Rules mirror the AST rules:

- `Unprocessed` resolved in `core-engine` outside a gate file → violation.
- `Unprocessed` resolved outside communerd, gate files, server/handlers, or test files → violation.

New rule exclusive to the semantic layer:

- Any `Resolved` (non-literal) occurrence of a wrapper type in **any file** → violation,
  including test files.
  Rationale: if the type system resolves a wrapper through an alias or newtype, it means that
  alias or newtype exists somewhere in the codebase. Its existence is the violation regardless
  of where it is used. Test code is not exempt — tests that write `Foo` where `Foo` is an
  alias for `Unprocessed<Bar>` should also write `Unprocessed<Bar>` directly.

  The only permitted exception is synthetic fixture code inside
  `trust_boundary_type_usage.rs` itself, introduced specifically to verify that the
  detection mechanism fires. Such fixtures must be marked with a `#[allow(...)]` attribute
  and a comment explaining their purpose. They are excluded from
  `verify_semantic_invariants` by filtering on the source file path.

  (@human: a `Resolved` detection anywhere outside the test fixture is the signal that an
  alias or indirection was reintroduced. The table renders it as `Resolved(ERROR!)` in every
  such row — the label is not a description of a state to tolerate, it is a lint failure
  rendered inline.)

---

## 7. Snapshot Integration

The enhanced snapshot file gains a second section below the existing AST usage table:

```
=== Semantic Analysis (rustdoc JSON) ===

file | function | position | Unprocessed | CleanAuthenticated | Externalized
...rows...

=== Semantic Cross-Tabulation ===

crate | wrapper | inner | literal | resolved
...rows...
```

`UPDATE_SNAPSHOT=1` regenerates both sections. A mismatch in either section fails the test.

The existing AST table section header is renamed from `=== Trust Boundary Usage Snapshot ===`
to `=== AST Analysis (syn) ===` to make the distinction explicit.

---

## 8. Function Organisation in Test File

```
trust_boundary_type_usage.rs
│
├── // ── AST layer ─────────────────────────────────────────
├── WrapperTypeVisitor (struct + Visit impl)   [existing]
├── collect_ast_usages()                       [existing, renamed from collect_all_type_usages]
├── verify_ast_invariants()                    [existing, renamed from verify_trust_boundary_invariants]
├── check_gate_bodies()                        [new — §9 strawman: field + signatures + verifier]
├── format_ast_table()                         [existing, renamed from format_usage_table]
│
├── // ── Semantic layer ────────────────────────────────────
├── invoke_rustdoc_json()                      [new]
├── extract_fn_signatures()                    [new]
├── resolve_type()                             [new]
├── collect_semantic_usages()                  [new]
├── verify_semantic_invariants()               [new]
├── format_semantic_table()                    [new]
│
├── // ── Merge and snapshot ────────────────────────────────
├── merge_into_table()                         [new]
├── format_merged_table()                      [new]
│
├── // ── Tests ─────────────────────────────────────────────
├── test_wrapper_type_visitor_detects_wrappers [existing]
└── test_trust_boundary_snapshot               [existing, extended to call both layers]
```

No function in the AST layer calls into the semantic layer or vice versa. The test entry
point (`test_trust_boundary_snapshot`) calls both collectors and both verifiers independently,
then merges for snapshot output.

---

## 9. Gate Check — A Gate Must Touch the Field, the Signatures, and the Verifier

A focused AST-layer lint (a **strawman** — syntactic, not a proof, but a check
nonetheless): a **gate function** must reference all three things a real gate
necessarily uses. Each is very weak on its own; their *absence* is a definite bug.

**Definition of a gate function:** a `fn` whose signature has

- at least one **parameter** whose type is `UnverifiedSignatureEnvelope<_>` or
  its alias `DontUse<_>`, **and**
- a **return type** of `CleanAuthenticated<_>` or `CleanFullyAuthenticated<_>`
  (directly, or wrapped in `Result<…>` / `Option<…>`).

**Rule — the body of every gate function must contain all three:**

1. **The full-signature field** — a call to `always_require_full_signature`
   (`RecordBase`, Communerdette spec §21.5). Catches: gate forgot the field and
   may silently emit `CleanAuthenticated<R>` for a record that demanded full.
2. **The signatures** — access to the signature data: a reference to the
   `signatures` field / a `signatures()` accessor (or `matrix` for FamilyRecord).
   Catches: a gate that returns `Clean*` without ever looking at any signature —
   i.e. fabricating authentication from nothing.
3. **The verification library** — a call to a known verification primitive. The
   allowlist of identifiers is fixed in the test:
   `{ verify, verify_with, tbid_verify }` (the project's crypto/verify surface).
   Catches: a gate that reads signatures but never actually verifies them.

If a gate is missing **any** of the three, the lint **fails**, naming which.

**Why strawman:** each clause confirms a reference *exists*, not that it is used
correctly — a body could touch signatures and call `verify` on the wrong bytes.
The lint cannot catch logical misuse; it catches the empty or hollow gate
(returns a trusted type while skipping the field, the signatures, or the
verifier). Correct enforcement is additionally covered by the negative unit
tests in Plan Phases 13/15/16.

**Implementation (AST layer, `syn`):**

```
const VERIFY_PRIMITIVES = {"verify", "verify_with", "tbid_verify"};

fn check_gate_bodies(ast) -> Vec<String>:
  for each ItemFn / ImplItemFn:
    let takes_unverified = sig.inputs.any(|p| type_name(p) in
        {"UnverifiedSignatureEnvelope", "DontUse"})
    let returns_clean = return_type_mentions(sig.output,
        {"CleanAuthenticated", "CleanFullyAuthenticated"})  // unwrap Result/Option
    if !(takes_unverified && returns_clean): continue

    let calls_field  = body has ExprMethodCall/ExprCall ident "always_require_full_signature"
    let touches_sigs = body has field/method access ident in {"signatures", "matrix"}
    let calls_verify = body has ExprMethodCall/ExprCall ident in VERIFY_PRIMITIVES

    if !calls_field:  violations.push("gate {fn}: never consults always_require_full_signature()")
    if !touches_sigs: violations.push("gate {fn}: never accesses signatures/matrix")
    if !calls_verify: violations.push("gate {fn}: never calls a verification primitive")
```

- Lives in the **AST layer** alongside `verify_ast_invariants` (it needs the
  function body, which rustdoc JSON does not expose — only `syn` sees bodies).
- Add as its own function `check_gate_bodies(&ast)` and call it from
  `test_trust_boundary_snapshot`; any violation fails the test.
- The `VERIFY_PRIMITIVES` set is maintained alongside the crypto verify surface;
  adding a new verification entry point requires adding it here (a deliberate,
  reviewed coupling).
- Allow an explicit, auditable opt-out marker (e.g.
  `// gate-strawman-exempt: <reason>`) for the rare legitimate gate that provably
  needs no such check — e.g. one that delegates verification to a helper it calls.
  The marker names the reason so the exception is visible in review.

## 10. Dependencies to Add (core-engine Cargo.toml, dev-dependencies)

```toml
rustdoc-types = "0.32"   # mirrors rustdoc JSON schema; pin to a specific version
```

The `format_version` check in `invoke_rustdoc_json` must assert against the version constant
exported by this crate.
