# TRUST_BOUNDARY_SEMANTIC_ANALYSIS_SPEC.md

**Date:** 2026-05-28 (revised 2026-06-05: teaching + catalogue refactor)
**Application:** Foretias — trust boundary enforcement tests
**Extends:** `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md`

---

## 1. Purpose

The trust-boundary snapshot test (`p2p/core-engine/tests/trust_boundary_type_usage.rs`)
enforces that the project's trust-state wrapper types are used only where they are allowed,
and that authentication gates actually do the work they claim. It does this with **two
analysis layers** that look at the same code two different ways:

- **StrawmanSuite** — syntactic, AST-based (`syn`). Reads source *text*.
- **TinmanSuite** — type-resolved, rustdoc-JSON-based. Reads the compiler's *resolved* type graph.

The two layers are additive: the AST layer is never removed or weakened when the semantic
layer is added.

This document is **both** an implementation spec for those two suites **and** a teaching /
evaluation reference for the underlying check mechanism:

1. It teaches the difference between *syntactic / AST* checking and *compiled / type-resolved*
   analysis, and surveys the mature alternative technologies in each tier (§3), so a future
   maintainer can judge what to run.
2. It catalogues, prose-first, exactly **what each suite verifies today** and what each suite
   **could verify next** (§6, §7) — a copyable template for adding checks, and the checklist any
   replacement engine must reproduce.
3. It frames both suites as the project's **standing, in-tree implementation** of one typed
   information-flow (taint) invariant — the fast, shift-left enforcement layer — rather than
   engine candidates still being chosen.

Ideas that layer *above* the in-tree suites (an independent external verifier, sandboxing,
full-history authoritative scans) are real but **not principal** to this effort; they are
discussed in **Appendix A**, with only a one-line pointer in the main body. The recurring
"wouldn't a stronger type system just guarantee this?" question is talked out in **Appendix B**,
which keeps Rust as the platform.

---

## 2. Background: The Gap in AST Analysis

The original test used `syn` to parse source files as text ASTs and scanned `TypePath` nodes
for the literal identifiers `UnverifiedSignatureEnvelope`, `CleanAuthenticated`, `Externalized`.
This detects every place the code explicitly wrote one of those names.

It does **not** detect:

- A type alias `type Foo = UnverifiedSignatureEnvelope<Bar>` used as a function parameter or
  return type — `Foo` is a bare identifier in the AST with no angle brackets; the visitor never fires.
- A newtype wrapper `struct Gate(UnverifiedSignatureEnvelope<Bar>)` whose field type is a wrapper.
- Any indirection where a name other than the canonical identifiers carries wrapper semantics.

The aliases deleted in May 2026 (`UnprocessedChrononRecord`, etc.) were completely invisible to
the AST scanner. The scanner only began enforcing them after the aliases were removed and the
generic form was written literally. Any future alias reintroduction would silently bypass
enforcement again. Closing that gap is the reason the type-resolved TinmanSuite exists.

---

## 3. Conceptual Foundations: Typed Information-Flow, Two Ways

### 3.0 The invariant as taint

Stated abstractly, the rule both suites enforce is a classic **information-flow / taint** rule:

- **Sources** — points where the protected, *untrusted* type is constructed (raw data off the
  wire or disk becomes `DontUse<R>`).
- **Sinks** — points where the *trusted* boundary type appears (`CleanAuthenticated<R>`,
  consumed by in-process logic that assumes due diligence is done).
- **Barriers** — an **allow-listed set of gate functions** that are the *only* sanctioned way to
  transform a source into a sink. In our terms: only the inbound gate may turn
  `DontUse<R>` into `CleanAuthenticated<R>`.
- **Flagged** — any source→sink path that reaches a sink without passing through a barrier, or
  any appearance of the protected type outside the regions where it is permitted.

This is exactly the shape of a custom interprocedural taint query: *"only an allow-listed set of
functions may transform `A<R>` into `B<R>`."* A full dataflow engine would prove this over every
path. Our two suites are two complementary **partial** implementations of the same rule — one
cheap and syntactic, one type-accurate — and the rest of this section is about that trade.

**Terminology bridge.** The companion spec `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md` names the
trust-states conceptually as `DontUse<T>` → `CleanAuthenticated<T>` → `Externalized<T>`. In
the *implementation*, the concrete source type the scanner matches is
**`UnverifiedSignatureEnvelope<T>`** (with the alias `DontUse<T>`) — that is the realized form of
"unprocessed." Throughout this document the taint discussion uses the conceptual `DontUse<R>`;
the catalogues in §6–§7 name the literal strings the code actually matches.

### 3.1 Syntactic / AST analysis — the Strawman model

Pipeline: **source text → parse tree → pattern match.** No compiler runs; no types are resolved.
`syn::parse_file` produces an AST, a `Visit` implementation walks `TypePath` nodes, and a literal
identifier match records an occurrence (`trust_boundary_type_usage.rs`, `WrapperTypeVisitor`).
The occurrence flows into the snapshot table, where a human reads it directly.

**Strengths:**
- Rules are tiny and legible. The conceptual rule is a one-line regex — `UnverifiedSignatureEnvelope<.*`
  is instantly understandable and mirrors how a human reviewer scans a diff — and the `syn`
  `TypePath` visitor beside it is only a little more precise.
- No build. It runs on unbuildable, half-edited, or partially-broken source.
- It sees function **bodies** and **comments**, not just signatures — which is what makes the
  gate-body check (§6) possible at all.
- It mirrors human recognition: it flags what a person scanning the text would flag.

**Blind spots:** type aliases, newtypes, macro-expanded code, re-exports, and generics. Anything
that wears a different *name* than the canonical identifier is invisible (the §2 gap).

### 3.2 Compiled / type-resolved analysis — the Tinman model

Pipeline: **compile → resolved type graph → query.** The compiler (here via `rustdoc`'s JSON
output, which is derived from the same resolved `TyCtxt` the compiler uses) produces a graph in
which aliases are expanded and trait impls are enumerated. The query walks resolved function
signatures and reports positions whose *resolved* type is a wrapper, regardless of the name
written at the use site (`collect_semantic_usages`).

**Strengths:** aliases are expanded, newtypes are visible as their inner type, impls are
enumerated, and results are compiler-accurate. It closes the §2 gap.

**Costs:**
- It needs a **build** — slower, and it fails when the crate doesn't compile.
- It sees **signatures only**, not bodies. It cannot see what a function *does*, only its
  declared types — so it cannot do the gate-body check.
- **Schema-version fragility:** rustdoc JSON's `format_version` is versioned independently of
  Rust and changes under you (see §4).
- **Toolchain-bound:** rustdoc JSON output is a nightly, unstable-options feature.
- "**Right but unhelpful**" strictness: it is stricter than a human and will sometimes flag
  things that are technically wrapper-typed but not what a reviewer cared about.

**Generic-resolution caveat.** The most failure-prone part of any type-resolved layer is generic
handling — resolving `A<R>` correctly through generic parameters, associated types, and blanket
impls. Validate this on a deliberately small, toy example before trusting it at scale; do not
assume the resolver handles every generic shape until you have seen it do so.

### 3.3 Why run two (or more): redundancy as safety

The two suites have **independent failure modes**. A regression that hides from the syntactic
tripwire (because it used an alias) is exactly what the semantic backstop catches; a regression
that the semantic layer can't see (because it lives in a function *body*, not a signature) is
what the syntactic layer catches. To slip through unnoticed, a bad change must evade *both*. This
is ordinary N-version reasoning: two cheap, differently-wrong checkers beat one checker that is
wrong in a single correlated way.

The snapshot's cross-tabulation (literal vs resolved) makes this concrete: each suite's view is
laid beside the other's, so a discrepancy — literal-but-not-resolved, or resolved-but-not-literal —
is itself a signal worth reading.

(The same "two layers, different authority" instinct shows up at the *organizational* level — a
fast, author-controlled, suppressible scan plus a slower, authoritative, non-suppressible one.
That layering is a nice-to-have that sits above these suites; it is discussed in **Appendix A**,
not here.)

### 3.4 Performance & legibility character

| Tier | What it reads | Build needed? | Runtime order | Rule legibility |
|------|---------------|---------------|---------------|-----------------|
| Regex / text grep | Raw bytes | No | Milliseconds | Highest — a one-line pattern a human reads at a glance |
| Syntactic AST (`syn`, our Strawman) | Parse tree (names, structure, bodies) | No | Sub-second to seconds | High — a small visitor, still human-auditable |
| Type-resolved (rustdoc JSON / `TyCtxt`, our Tinman) | Resolved type graph (signatures) | Yes | Seconds to minutes (a compile) | Medium — correct, but interposes compiler machinery between rule and result |
| Full dataflow / taint (CodeQL, MIRAI) | Whole-program paths | Yes | Minutes | Lower — powerful, but the rule is a query in a separate language |

The anchor: syntactic rules are small and human-auditable and run anywhere; semantic and dataflow
rules buy accuracy by interposing machinery, at the cost of build time, toolchain coupling, and
rule legibility.

### 3.5 Mature alternative technologies

A neutral survey of *mature* systems in each tier, so a future maintainer can weigh options. Each
is tagged **OSS / Commercial / Open-core** and given a one-line experiential note. This is a survey,
not a recommendation; §3.6 gives the rubric and §6–§7 give the checklist any candidate must satisfy.

| Tool | Tier | License model | Buys us / costs us |
|------|------|---------------|--------------------|
| ripgrep + regex | Text | OSS | Instant, universal, zero-setup; blind to all structure. |
| `syn` *(our Strawman)* | Syntactic AST | OSS | Rust-native parse tree, sees bodies; no type resolution. |
| tree-sitter | Syntactic AST | OSS | Fast incremental multi-language AST; still syntactic only. |
| ast-grep | Syntactic AST | OSS | Ergonomic structural patterns over tree-sitter; syntactic only. |
| Semgrep | Syntactic+ | Open-core | Large rule ecosystem, easy rules; deep Rust type reasoning is limited; best rules/registry are paid. |
| Coccinelle | Syntactic (C-focused) | OSS | Powerful semantic patches — but C-centric; a contrast point, not a Rust fit. |
| Clippy | Type-resolved (lints) | OSS | First-class Rust lints on `TyCtxt`; custom project-specific lints are not its model. |
| Dylint | Type-resolved (custom lints) | OSS | Write custom `LateLintPass` lints with full `TyCtxt`; depends on **unstable** `rustc_private` APIs that move with every toolchain bump — high maintenance coupling. |
| rust-analyzer (`ra_ap_hir`) | Type-resolved | OSS | Embeddable inference engine; heavy to drive and its API is unstable. |
| rustdoc JSON *(our Tinman)* | Type-resolved (signatures) | OSS | Stable-ish resolved type graph without compiler internals; signatures only, nightly + versioned schema. |
| MIRAI | Abstract interpretation | OSS | MIR-level analysis approaching dataflow; nightly-bound, niche, heavy. |
| CodeQL | Full dataflow / taint | Commercial (free for OSS & research) | Rich `DataFlow` / `TaintTracking` for exactly this source→sink→barrier query; licensing gates private-code use; Rust frontend is **recent** (GA as of late 2025 — verify the current version before relying on it; younger than CodeQL itself). |
| Joern | Full dataflow (CPG) | Open-core | Code-property-graph queries across languages; Rust support is comparatively immature. |
| SonarQube | Mixed / platform | Open-core | Broad rule platform and reporting; custom typed-invariant rules are awkward; deeper analysis is paid. |
| Coverity | Full dataflow | Commercial | Mature interprocedural analysis; enterprise licensing and setup. |

Where a tool's **Rust frontend is younger than the tool** (CodeQL most notably, also Joern), treat
maturity claims about the tool as not automatically transferring to its Rust support.

### 3.6 Choosing the next system — decision criteria (neutral)

If the project ever weighs replacing or augmenting the suites, judge candidates against this
rubric (no winner is picked here):

- **Coverage of the invariants in §6–§7** — does it reproduce the catalogue, or only part of it?
- **CI build-cost budget** — does it fit the time the team will actually tolerate per run?
- **Rule legibility for the team** — can a reviewer read and trust the rule, or is it opaque?
- **OSS vs licensing / private-code constraints** — is the repo's openness compatible?
- **Toolchain / version coupling** — how hard does it pin you to a toolchain (e.g. Dylint's
  unstable rustc APIs, rustdoc JSON's `format_version`)?
- **Body-level vs signature-level reach** — can it see function bodies (needed for the gate-body
  triad), or only signatures?
- **Full-dataflow vs partial approximation** — does it actually prove source→sink paths, or
  approximate like our two suites do?
- **Maintainability of the rule corpus** — how much ongoing work to keep the rules alive?

### 3.7 Relationship to the independent verifier

These suites are the **in-tree, shift-left** layer. An independent external verifier — with full
history and baseline authority — is a complementary *nice-to-have* that could re-run equivalent
invariants; the in-tree suites are deliberately **not** the final authority. That layering is out
of scope here and is described in **Appendix A**.

---

## 4. Our Implementation Choice: rustdoc JSON

Given the tiers in §3, here is what we run on an ongoing basis for the type-resolved layer, and why.

### 4.1 Options considered

| Option | Description | Toolchain | Effort |
|--------|-------------|-----------|--------|
| `rustdoc --output-format json` | Subprocess; parse rustdoc's JSON, derived from the compiler's resolved type graph | **Nightly** (`-Z unstable-options`) | Low–Medium |
| `ra_ap_hir` (rust-analyzer crates) | Embed rust-analyzer's type inference engine | Stable, but unstable API | High |
| `rustc_interface` driver | Write a compiler driver; query `TyCtxt` directly | `rustc_private`, nightly only | Very high |

### 4.2 Decision

Use **rustdoc JSON** (option 1). Invoke `cargo +nightly rustdoc -Z unstable-options --output-format
json` as a subprocess from the test, read the emitted `target/doc/<crate>.json`, and parse it with
the `rustdoc-types` crate (which mirrors the exact schema rustdoc emits) to query the resolved type
graph. It operates on the same resolved type information as the compiler, so type aliases are
expanded and trait implementations are enumerated correctly — closing the §2 gap — without us
linking against compiler internals.

### 4.3 Limitation note — as of revision

- rustdoc JSON output is a **nightly**, unstable-options feature; the test invokes the nightly
  toolchain explicitly.
- rustdoc JSON's schema is versioned independently of Rust. The `format_version` field in the
  output must be checked: the test pins `rustdoc-types` to a known-good version and asserts
  `format_version` (**currently 57**), failing with a clear message on mismatch rather than
  silently producing wrong results. Update both together when bumping nightly.
- Cross-crate analysis requires invoking rustdoc separately per crate of interest.
- rustdoc JSON captures the *declared* type graph, not runtime values or bodies. It cannot observe
  dynamic dispatch or what a function does internally.

If a lower-friction or more stably-versioned mechanism becomes available (a stabilized
`rustc_interface` subset, or a `cargo` plugin API), revisit this choice against the §3.6 rubric.

---

## 5. Analysis Layers — StrawmanSuite and TinmanSuite

The two layers have names:

- **StrawmanSuite** — all **AST-based** (`syn`) checks. Syntactic, weak by construction (it reads
  source text, not resolved types): wrapper-usage scan, trust-boundary placement invariants, and
  the gate-body triad (§6). Cheap, no compiler invocation, sees function *bodies*.
- **TinmanSuite** — all **rustdoc-JSON-based** checks. Type-resolved (aliases expanded, trait impls
  enumerated): semantic wrapper-usage and semantic invariants (§7). Stronger, but only sees declared
  *signatures*, not bodies, and requires a `cargo rustdoc` invocation.

(@human — Wizard-of-Oz naming: the Strawman lacks a brain — purely syntactic; the Tinman lacks a
heart — colder/deeper type machinery. Neither alone is the full Wizard; together they cover more.)

The test file is restructured so each concern is in its own function with no cross-calling except at
the top-level snapshot test entry points.

```
┌──────────────────────────────────────────────────────────┐
│  collect_ast_usages(workspace)  [StrawmanSuite]          │
│    syn parse → visit TypePath nodes                      │
│    returns Vec<LocationEntry> + raw occurrences          │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  collect_semantic_usages(crate_json)  [TinmanSuite]      │
│    cargo +nightly rustdoc → parse JSON → walk fn items   │
│    resolve param/return types                            │
│    returns Vec<(fn, wrapper, context)>                   │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  merge_into_table(ast, semantic) → cross-tabulation      │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│  verify_ast_invariants(ast)        [StrawmanSuite rules] │
│  check_gate_bodies(source)         [StrawmanSuite rules] │
│  verify_semantic_invariants(usage) [TinmanSuite rules]   │
└──────────────────────────────────────────────────────────┘
```

No function in the AST layer calls into the semantic layer or vice versa. The test entry point
(`test_trust_boundary_snapshot`) calls both collectors and all verifiers independently, then merges
for snapshot output.

---

## 6. What the StrawmanSuite Verifies (AST / syntactic)

This section is a **prose-first catalogue and template**. Each implemented check is written as: the
English problem a human is solving, then an illustrating regex/snippet, then a one-line blind-spot
note. To add a check, copy the shape. Everything here corresponds to code in
`trust_boundary_type_usage.rs`; the matched identifiers are exact.

### 6.1 Literal wrapper-usage scan

**Problem (English).** We want to know *every place in the source where someone literally wrote one
of the trust-state wrapper types*, so the snapshot makes wrapper usage auditable at a glance.

**Illustration.** Match the canonical identifiers as written, with their generic argument:

```
{ UnverifiedSignatureEnvelope, CleanAuthenticated, Externalized } <  ...  >
```

The `syn` visitor walks `TypePath` nodes and records the wrapper name, the inner type, and the
enclosing context (`impl`/`fn`/`struct`).

**Blind spot.** Only literal names fire; an alias or newtype carrying wrapper semantics is invisible
(that is the TinmanSuite's job, §7).

### 6.2 Placement invariants (where a source type may appear)

**Problem (English).** The untrusted source type must not leak into trusted territory. Concretely:
`UnverifiedSignatureEnvelope` (and `Externalized`) should not appear inside `core-engine` except in
the gate files; and `UnverifiedSignatureEnvelope` should appear *only* in the regions that
legitimately handle un-authenticated data.

**Illustration (the three rules in `verify_ast_invariants`).**
1. `UnverifiedSignatureEnvelope` in `core-engine/src/**` outside a gate file
   (`clean_auth.rs` or `probity/report.rs`) → violation.
2. `Externalized` in `core-engine/src/**` outside a gate file → violation.
3. `UnverifiedSignatureEnvelope` (with a concrete, non-`bare`, non-`T` inner) anywhere outside the
   allowed regions — `communerd`, the gate files (`clean_auth`, `probity/report`),
   `server/handlers` (the wire→`UnverifiedSignatureEnvelope` parse boundary), or `test` files →
   violation.

These are the **placement** half of the taint rule: the protected type may exist only at the
sources and barriers, not in trusted interior code.

**Blind spot.** Purely path/name based: it trusts the file layout and the literal name. Move the
gate logic to a differently-named file, or alias the type, and these rules need updating / are evaded.

### 6.3 The gate-body triad (the *barrier* check)

**Problem (English).** A gate function is the *only* sanctioned barrier that turns an untrusted
source into a trusted sink. A gate that returns a trusted type without doing the work — never
consulting the full-signature requirement, never looking at any signature, never calling a verify
primitive — is a hole in the boundary. Each of those three references is very weak on its own, but
its **absence** is a definite bug. This check needs the function **body**, which is why it lives in
the StrawmanSuite (rustdoc JSON cannot see bodies).

**Definition of a gate function.** A `fn` whose signature has:
- at least one **parameter** typed `UnverifiedSignatureEnvelope<_>` or its alias `DontUse<_>`, **and**
- a **return type** of `CleanAuthenticated<_>` or `CleanFullyAuthenticated<_>` (directly, or wrapped
  in `Result<…>` / `Option<…>`).

(@human: the *implemented* `check_gate_bodies` detects a gate more crudely than this definition —
it triggers on a `fn` line that mentions `UnverifiedSignatureEnvelope`, without resolving the return
type. The param+return definition above is the design intent; tightening detection to also confirm a
`CleanAuthenticated`/`CleanFullyAuthenticated` return is a §6.4 backlog item.)

**Rule — the body of every gate function must reference all three:**
1. **The full-signature field** — `always_require_full_signature` (`RecordBase`, Communerdette spec
   §21.5). Absent → the gate may silently emit `CleanAuthenticated<R>` for a record that demanded full.
2. **The signatures** — the signature data: a `signatures` field/accessor (or `matrix` for
   FamilyRecord). Absent → the gate returns `Clean*` without ever looking at a signature
   (fabricating authentication from nothing).
3. **The verification library** — a call to a known verify primitive, from the fixed allowlist
   `{ verify, verify_with, tbid_verify }`. Absent → the gate reads signatures but never verifies them.

If a gate is missing **any** of the three, the lint fails, naming which.

**Illustration (pseudocode of `check_gate_bodies`).**

```
const VERIFY_PRIMITIVES = { "verify", "verify_with", "tbid_verify" };

for each gate fn body:
  has_always = body.contains("always_require_full_signature")
  has_sig    = body.contains("signatures") || body.contains("matrix")
  has_verify = VERIFY_PRIMITIVES.any(|p| body.contains(p))
  // missing any → violation naming the fn and the missing element
```

**Opt-out.** A rare, provably-exempt gate (e.g. one that delegates verification to a helper it
calls) may carry an explicit, auditable marker `// gate-strawman-exempt: <reason>`; the reason makes
the exception visible in review.

**Why strawman / what it cannot catch.** Each clause confirms a reference *exists*, not that it is
used *correctly* — a body could touch `signatures` and call `verify` on the wrong bytes. The lint
catches the empty or hollow gate, not logical misuse. Correct enforcement is additionally covered by
the negative unit tests in the implementation plan's gate phases.

### 6.4 Candidate checks to add — requirements backlog (proposed, not yet implemented)

These are *design targets for iterating the requirement set*, kept separate from §6.1–§6.3 so the
catalogue above stays an accurate description of the code. A future author promotes one of these by
implementing it and moving it up into the implemented catalogue.

- **Construction-site allow-list (true source check).** Flag any call to a source constructor
  (`DontUse::from_bytes`, `from_json_value`, `Externalized::into_inner`) outside the
  permitted boundary files — the source half of the taint rule, currently only approximated by the
  placement rules.
- **`DontUse` literal usage.** The placement scan keys on `UnverifiedSignatureEnvelope`; extend the
  visitor's identifier set to also flag the `DontUse` alias at its use sites.
- **Direct-field-access escape hatch.** Flag any code outside the gate files that reaches the inner
  value of a wrapper (e.g. a `.inner` access or a destructure), which would bypass the gate entirely.
- **`from_trusted` audit.** Surface every `CleanAuthenticated::from_trusted()` call site in the
  snapshot (a deliberately privileged constructor) so each is reviewable.
- **Tighten gate detection.** Confirm a gate's *return type* is `CleanAuthenticated<_>` /
  `CleanFullyAuthenticated<_>` (not just that a parameter is `UnverifiedSignatureEnvelope`), matching
  the §6.3 definition; today detection is parameter-line based.

### 6.5 Why strawman, overall

Every StrawmanSuite check is syntactic and therefore defeatable by renaming or aliasing — which is
the whole reason the TinmanSuite (§7) exists as the type-accurate backstop. The StrawmanSuite's job
is to be fast, legible, body-aware, and to fail loudly on the obvious mistakes a human reviewer would
also catch.

---

## 7. What the TinmanSuite Verifies (rustdoc JSON / type-resolved)

Same template as §6: problem, illustration, blind-spot. Everything corresponds to code in the
TinmanSuite section of `trust_boundary_type_usage.rs`.

### 7.1 Semantic wrapper-usage scan

**Problem (English).** Catch wrapper usage that the literal scan misses — a position whose
*resolved* type is one of the wrappers even though the source wrote an alias or a newtype. This is
the §2 gap, closed by the compiler's resolved view.

**Illustration.** Walk resolved function signatures; for each parameter and the return type, resolve
the type and ask whether it is `UnverifiedSignatureEnvelope`, `CleanAuthenticated`, or `Externalized`.

**What it adds over the Strawman.** It sees through aliases/newtypes to the real type.
**What it still cannot see.** Function *bodies* (so no gate-body triad), runtime behavior, dynamic
dispatch, and full source→sink *paths* (it reports occurrences at signatures, not proven flows).

### 7.2 Mirror placement invariants over resolved occurrences

**Problem (English).** Re-run the §6.2 placement intent, but over *resolved* occurrences, so a
wrapper that reaches trusted territory under an alias is caught where the literal rule was blind.
`verify_semantic_invariants` applies the structural rules over the semantically-detected set.

**Blind spot.** Signature-level only; a wrapper buried in a body is invisible here.

### 7.3 The exclusive rule — any resolved (non-literal) occurrence is a violation

**Problem (English).** If the type system resolves a wrapper *through* an alias or newtype anywhere,
then that alias or newtype **exists** in the codebase — and its existence is the bug. The May 2026
aliases are exactly this. So *any* `Resolved` (i.e. non-literal) wrapper occurrence, in **any** file
including tests, is a violation. Tests that write `Foo` where `Foo` aliases `UnverifiedSignatureEnvelope<Bar>`
must write the generic form directly instead.

**Illustration.** The snapshot renders such a position as `Resolved(ERROR!)` — the label is not a
state to tolerate, it is a lint failure rendered inline. A position detected both literally and via
resolution renders as `literal+Resolved(ERROR!)`.

**The only exemption.** Synthetic fixture code inside `trust_boundary_type_usage.rs` itself,
introduced specifically to prove the detection mechanism fires. Such fixtures are marked with an
`#[allow(...)]` attribute and an explaining comment, and are excluded by filtering on the source
file path.

(@human: a `Resolved` detection anywhere outside the test fixture is the signal that an alias or
indirection was reintroduced — the precise regression the TinmanSuite exists to catch.)

### 7.4 Candidate checks to add — requirements backlog (proposed, not yet implemented)

- **Full alias-chain resolution.** The current resolver is a deliberately simple string-`contains`
  heuristic over the type's rendered name (see §9.3). The design target is true recursive resolution
  through `TypeAlias` inner types and single-field newtype structs, so multi-hop indirection
  (`type A = B; struct C(A);`) resolves to the wrapper. Validate on a toy generic example first
  (§3.2 caveat).
- **Trait-impl enumeration.** Use the resolved impl set to flag wrapper types appearing as associated
  types or trait-method signatures, not just free/inherent functions.
- **Cross-crate sweep.** Invoke rustdoc per crate (`foretias-server` as well as `core-engine`) and
  merge, so a wrapper leaking across a crate boundary is covered.

---

## 8. Data Model

### 8.1 Detection mode

```rust
pub enum DetectionMode {
    /// The source code literally wrote `UnverifiedSignatureEnvelope<Foo>` (or another wrapper).
    /// Detected by: AST TypePath visitor.
    Literal,

    /// The type at this position resolves through the type system to a wrapper type,
    /// but was not written literally (e.g. written as a type alias or newtype).
    /// Detected by: rustdoc JSON type resolution.
    Resolved,
}
```

A position may carry both modes simultaneously (e.g. a transparent newtype whose field is also
written literally in the same file). Both are recorded independently; the table renders them as
`literal`, `resolved`, or `literal+resolved`.

### 8.2 Occurrence records

The AST layer records a `LocationEntry` per `(file, inner, context)` with a boolean per wrapper; the
semantic layer records `(function, wrapper, context)` tuples. (See §9 for the resolved-shape the
function spec targets.)

```rust
pub enum Position {
    Param { index: usize, name: String },
    Return,
}

pub enum WrapperKind {
    DontUse,              // realized as UnverifiedSignatureEnvelope
    CleanAuthenticated,
    Externalized,
}
```

### 8.3 Table row

Each row represents one `(file, function, position)` triple. Wrapper columns hold the detection mode
string or `-`.

```
file | function | position | UnverifiedSignatureEnvelope | CleanAuthenticated | Externalized
```

Column values: `-`, `literal`, `Resolved(ERROR!)`, `literal+Resolved(ERROR!)`.

`Resolved(ERROR!)` means the type system found a wrapper type through alias or newtype resolution at
this position. This is always a violation regardless of file type. The row exists to show where the
indirection is used; the alias or newtype definition is the root cause to fix.

---

## 9. Semantic Analysis Function Specification

The TinmanSuite functions. (The semantic *invariants* that were specified here previously now live
in §7.3; this section covers collection and resolution. See §7 for what the rules mean.)

### 9.1 `invoke_rustdoc_json(crate_path) -> Result<Crate, String>`

- Runs: `cargo +nightly rustdoc -Z unstable-options --output-format json` in the crate directory.
- Reads the output JSON from the workspace `target/doc/<crate_underscored>.json` (e.g.
  `foretias_core.json`).
- Checks the `format_version` field and returns an error if it is not the known-good version the
  parser was written against (**currently 57**; update with the nightly bump).
- Returns the parsed `rustdoc_types::Crate`.

### 9.2 `extract_fn_signatures` / signature walk

Walks the `index` map of the rustdoc JSON. For each `Function` item: records the item name, each
input as `(name, type)`, and the output type. No resolution yet.

### 9.3 `resolve_type` / `type_to_string`

Renders a rustdoc JSON `Type` to a string (`type_to_string`) and decides whether it denotes a
wrapper (`resolve_type`). (@human: the current implementation is a deliberately simple
string-`contains` heuristic over the rendered type name — it catches the wrapper appearing anywhere
in the rendered type. Full recursive alias-chain / newtype resolution is the design target and is
tracked as a §7.4 backlog item; the simpler heuristic is honest about being a first cut.)

### 9.4 `collect_semantic_usages(crate_json) -> Vec<(String, String, String)>`

For each function item in the crate index: resolve each parameter and the return type; for every
position whose resolved type is a wrapper, emit `(function, wrapper, context)`. A position written in
the canonical generic form is already covered by the AST layer's `Literal` mode; the semantic pass
contributes the `Resolved` view, and the merge step combines them in the cross-tabulation.

---

## 10. Snapshot Integration

The snapshot file carries the StrawmanSuite table, a cross-tabulation, the TinmanSuite section, and
any gate-body violations:

```
=== StrawmanSuite (AST / syn) ===

file | inner | context | UnverifiedSignatureEnvelope | CleanAuthenticated | Externalized
...rows...

=== Cross-Tabulation ===
...rows...

=== TinmanSuite ===
...rows (or "skipped: <reason>" when rustdoc/nightly is unavailable)...

=== TinmanSuite Cross-Tabulation ===
...rows...
```

`UPDATE_SNAPSHOT=1` regenerates all sections. A mismatch in any section fails the test. The
TinmanSuite section degrades gracefully to a `skipped: <reason>` line when the nightly toolchain or
rustdoc JSON is unavailable, so the StrawmanSuite still runs in minimal environments.

### 10.1 Determinism guideline

Keep the human-inspectable snapshot **stable across unrelated edits** by ordering rows on **semantic
keys** — fully-qualified symbol + module path + finding type — rather than raw `file:line`, which
churns whenever an unrelated line is added above. Pin the analysis engine version (the
`rustdoc-types` / `format_version` pin in §4/§9.1) so the *engine* is not a hidden source of
snapshot drift. The existing collectors already sort on `(file, inner, context)` / tuple order in
this spirit; preserve that property when adding rows.

---

## 11. Function Organisation in Test File

```
trust_boundary_type_usage.rs
│
├── // ── StrawmanSuite (AST / syn) ───────────────────────────
├── WrapperTypeVisitor (struct + Visit impl)
├── collect_ast_usages()
├── verify_ast_invariants()
├── check_gate_bodies()  /  check_gate_bodies_in_workspace()
├── format_ast_table()  /  build_cross_tabulation()  /  format_cross_tab()
│
├── // ── TinmanSuite (rustdoc JSON) ──────────────────────────
├── invoke_rustdoc_json()
├── type_to_string()  /  resolve_type()  /  bound_to_string()
├── collect_semantic_usages()
├── verify_semantic_invariants()
├── format_tinman_suite()
│
├── // ── Merge and snapshot ────────────────────────────────
├── merge_into_table()  /  format_merged_table()
│
├── // ── Tests ─────────────────────────────────────────────
├── test_wrapper_type_visitor_detects_wrappers
└── test_trust_boundary_snapshot   (calls both suites, merges, snapshots)
```

No function in the AST layer calls into the semantic layer or vice versa. The test entry point calls
both collectors and all verifiers independently, then merges for snapshot output.

---

## 12. Dependencies to Add (core-engine Cargo.toml, dev-dependencies)

```toml
rustdoc-types = "0.57"   # mirrors rustdoc JSON schema; pin to match the nightly toolchain
```

The `format_version` check in `invoke_rustdoc_json` must assert against the version this crate
targets (**57**). When bumping the nightly toolchain, bump `rustdoc-types` and the asserted
`format_version` together.

---

## Appendix A — Relationship to an Independent Verifier (nice-to-have, out of core scope)

The two suites in this document are the **in-tree, author-controlled, shift-left** enforcement layer:
fast, suppressible (a check can be edited or a gate exempted in the same PR), and run by whoever is
editing the code. That is deliberate — it is the layer that gives fast feedback during development.

A natural *complement*, **not required by this effort**, is a second, **authoritative** layer that is
organizationally separated from the code under test. The design space includes:

- **An ephemeral, network-denied sandbox** that runs the invariants in a clean environment, so the
  analysis cannot be influenced by developer machine state.
- **A separate repository / separate humans** responsible for the verification rules, so the people
  who can change an invariant are not the same people whose code it constrains (executor vs authority
  / role separation).
- **A baseline-signing authority** that records a signed, known-good baseline and a
  **full-history, non-suppressible scan** that an individual PR cannot quietly weaken.
- **Secret scanning** and similar repository-wide assurances bolted onto the same authoritative run.

The relationship is two-layer defense-in-depth: the in-tree suites optimize for *speed and developer
feedback*; the external verifier optimizes for *authority and non-repudiation*. They check the same
invariants from different trust positions, the same way StrawmanSuite and TinmanSuite check the same
types from different analysis positions.

This appendix is a **pointer and a sketch**, not a specification. The Verifier/Intermediary role
separation, capability split, sandbox construction, and baseline workflow belong to that system's own
design doc and are intentionally **not** specified here. Nothing in the core suites depends on it.

---

## Appendix B — "Wouldn't a stronger type system just guarantee this?"

It is worth talking out, honestly, the recurring temptation: *several of these invariants would be
expressible directly in a more powerful type system* — linear/affine types to ensure an
`DontUse<R>` is consumed exactly once by a gate; information-flow type systems (à la a security-typed
language) to make "untrusted may not reach trusted without passing a barrier" a **compile error**;
dependent types to tie a `CleanAuthenticated<R>` to evidence that verification ran. In a language
like Haskell (or Idris/Agda for the dependent end), parts of §6–§7 could in principle be discharged
by the type checker instead of by a snapshot test.

**We are keeping Rust**, deliberately:

- The entire protocol, crypto, and P2P stack is Rust. Rewriting the trust-boundary core in another
  language to gain type-level guarantees would fork the platform for a narrow benefit and a large cost.
- Rust's own type system already encodes much of the boundary: the wrappers are newtypes, the gate is
  the only sanctioned transition, and `Externalized<T>` deliberately does not implement the
  trusted-inner trait (see `GENERIC_TRUST_BOUNDARY_WRAPPERS_SPEC.md`). The *type-level* half is real
  today.
- What Rust's type system **cannot** enforce locally is exactly what the suites cover: **placement**
  ("this type may not appear in this file/region"), **gate-body shape** ("a function returning the
  trusted type must actually call verify"), and **alias-reintroduction** ("no indirection may rename
  the protected type"). These are whole-program / lint-shaped properties, not properties of a single
  type signature — so even a stronger type system would still want an out-of-band check for several
  of them, or a global effect system Rust does not have.

The honest conclusion: a stronger type system would move *some* of these checks earlier (into the
compiler) but would not eliminate the need for a project-specific, whole-program enforcement layer —
and the migration cost is not justified. The two suites are the **pragmatic** realization of the
information-flow invariant on the platform we actually ship. If that calculus ever changes (e.g. a
mature security-typed Rust dialect, or an effect system in the language), revisit this appendix
against the §3.6 rubric.

---

## §2. Active Design Requirements

> **Status:** These requirements are not yet implemented. They inform how we design the code
> analysis system (StrawmanSuite + TinmanSuite) and what checks the suites must enforce.

### 2.1 `BaseRecord` — The Trait for Important Data Structures

All important data structures in Foretias must implement `BaseRecord`. An "important" data
structure is one that is **signed**, **stored**, or **transmitted** across a trust boundary.

```rust
/// Base trait for all important data structures (signed, stored, transmitted).
pub trait BaseRecord: serde::Serialize + Send + Sync + Clone + 'static {
    /// Must this record reach `CleanFullyAuthenticated` before it may be used?
    fn always_require_full_signature(&self) -> bool { false }
}
```

### 2.2 Naming Convention: `*Record`

All important data structures must be named `*Record`:

| Type | Renamed To | Implements `BaseRecord`? |
|------|-----------|-------------------------|
| `ChrononRecord` | `ChrononRecord` (no change) | ✅ (already `RecordBase`) |
| `Foretis` | `ForetisRecord` | ❌ (needs implementation) |
| `ExternalAttestation` | `ExternalAttestationRecord` | ❌ (needs implementation) |
| `EpochSnapshot` | `EpochSnapshotRecord` | ✅ (already `RecordBase`) |
| `FamilyRecord` | `FamilyRecord` (no change) | ✅ (already `RecordBase`) |
| `ProbityReport` | `ProbityReportRecord` | ✅ (already `RecordBase`) |
| `RecordBase` | `BaseRecord` (rename) | — (this is the trait) |

**Types that do NOT rename** (not important data structures):
- `Tbid` — identifier, not a record
- `SignatureEntry` — internal, not transmitted independently
- `Calendar` — container, not a record
- Config structs (`TimeFamilyCliConfig`, `ServeConfig`, etc.) — internal, not signed/stored/transmitted

### 2.3 Construction Rules

All `BaseRecord` implementations must:

1. **Private constructor** — no `pub fn new()` or public struct literal. The only way to construct is through the builder.
2. **`#[derive(Builder)]`** — use `bon` for construction with named parameters.
3. **Fallible `build()`** — validate invariants during construction (e.g., `chronon_number > 0`).

```rust
// Correct: private constructor, builder pattern
#[derive(Debug, Clone, Serialize, Builder)]
pub struct ChrononRecord {
    // ...
}

// Wrong: public constructor
impl ChrononRecord {
    pub fn new(/* params */) -> Self { /* ... */ }  // ❌ FORBIDDEN
}
```

### 2.4 Trust Boundary Flow Rules

All `*Record` types follow these rules when crossing trust boundaries:

| Direction | Wrapper Type | When |
|-----------|-------------|------|
| Network → local | `DontUse<Record>` | Inbound data from wire |
| Communerd → internal time beings | `CleanAuthenticated<Record>` | Calendar, Chronomatter receiving data |
| Communerd → external time beings | `Externalized<Record>` | Sending data to network |
| Local production | `CleanAuthenticated<Record>` via `from_trusted()` | Chronomatter producing a new tick |
| Wire/disk persistence | `Externalized<Record>` | Calendar storing records |

**Key invariant:** A `*Record` must NEVER cross a trust boundary without a wrapper. The only
exceptions are internal method calls within the same trust boundary (e.g., Calendar calling
Chronomatter directly).

### 2.5 What the Code Analysis System Must Enforce

The StrawmanSuite and TinmanSuite must be extended to check:

1. **`BaseRecord` completeness** — every type that is signed, stored, or transmitted must
   implement `BaseRecord`. The analysis must flag types that cross trust boundaries but don't
   implement the trait.

2. **Naming convention** — all `BaseRecord` implementations must be named `*Record`. The
   analysis must flag types that implement `BaseRecord` but don't follow the naming convention.

3. **Private constructors** — all `BaseRecord` implementations must have private constructors.
   The analysis must flag public `fn new()` or public struct literals on `BaseRecord` types.

4. **Trust boundary flow** — `*Record` types must only cross trust boundaries through the
   correct wrapper types. The analysis must flag:
   - `*Record` appearing at a network boundary without `DontUse<>` wrapper
   - `*Record` passed from Communerd to internal time beings without `CleanAuthenticated<>`
   - `*Record` transmitted externally without `Externalized<>`

5. **Builder pattern** — all `BaseRecord` implementations must use `#[derive(Builder)]`. The
   analysis must flag `BaseRecord` types that don't have a builder.

### 2.6 Design Considerations

During implementation of these requirements, we will consider:

- **Tool choices:** `bon` for builders, `syn` for AST analysis, rustdoc JSON for type-resolved
  analysis, `cargo-geiger` for unsafe audit
- **Functional/stricter languages:** In a language like Haskell or OCaml, the trust boundary
  enforcement could be expressed as a type-level guarantee (phantom types, GADTs, or type
  families). Rust's type system is strong enough for `CleanAuthenticated<T>` (private constructors)
  but requires out-of-band analysis (the suites) for placement rules. The suites are the
  pragmatic realization of what a stronger type system would guarantee at compile time.
- **Incremental adoption:** The naming convention and `BaseRecord` implementation can be done
  incrementally. The analysis checks can be added one at a time. Each check is independent.

### 2.7 Implementation TODO

- [ ] Rename `RecordBase` → `BaseRecord` in `clean_auth.rs`
- [ ] Rename `Foretis` → `ForetisRecord` in `tick.rs` and all call sites
- [ ] Rename `ExternalAttestation` → `ExternalAttestationRecord` in `external_attestation.rs` and all call sites
- [ ] Rename `EpochSnapshot` → `EpochSnapshotRecord` in `epoch/snapshot.rs` and all call sites
- [ ] Rename `ProbityReport` → `ProbityReportRecord` in `probity/report.rs` and all call sites
- [ ] Implement `BaseRecord` for `ForetisRecord` and `ExternalAttestationRecord`
- [ ] Add `#[derive(Builder)]` to all renamed types
- [ ] Make constructors private on all `BaseRecord` types
- [ ] Document `BaseRecord` in `AGENTS.md` under "Important Data Structures"
- [ ] Extend StrawmanSuite to check `BaseRecord` completeness and naming
- [ ] Extend TinmanSuite to check trust boundary flow rules
- [ ] Add analysis check: `*Record` at network boundary must have `DontUse<>` wrapper
- [ ] Add analysis check: `*Record` from Communerd to internal must have `CleanAuthenticated<>`
- [ ] Add analysis check: `*Record` transmitted externally must have `Externalized<>`
- [ ] Add analysis check: `BaseRecord` types must have private constructors
- [ ] Add analysis check: `BaseRecord` types must have `#[derive(Builder)]`
