# CODE_QUALITY_TOOLING_SPEC.md

**Spec: Code Quality Tooling — Clippy, cargo-geiger, and Formatting Standards**
**Status: PROPOSED**
**Date: 2026-05-31**

---

## Problem Statement

The Foretias codebase is well-engineered with strong security foundations, but lacks standardized code quality tooling configuration. Specifically:

1. **No `clippy.toml`** — Clippy (Rust's linting tool) runs with default settings, missing project-specific tuning opportunities
2. **No `rustfmt.toml`** — Code formatting is not standardized across contributors
3. **No `.editorconfig`** — Editor-specific settings may introduce inconsistent whitespace/line endings
4. **No `cargo-geiger` integration** — Unsafe code dependency tracking is not automated

While the codebase has excellent test coverage (482+ tests) and strong unsafe code controls (`#![deny(unsafe_op_in_unsafe_fn)]`), these tooling gaps create friction for contributors and miss opportunities for automated quality enforcement.

---

## Tool Explanations

### Clippy (Generic)

(@human Clippy is Rust's official linting tool, included with the Rust toolchain. It analyzes Rust code for common mistakes, non-idiomatic patterns, and potential improvements. Think of it as a "super compiler warning" system that catches things like:

- Unnecessary allocations or copies
- Incorrect use of `unwrap()` or `expect()` in production code
- Non-idiomatic Rust patterns (e.g., using `if let` instead of `match` when appropriate)
- Performance pitfalls (e.g., building a `Vec` just to check if it's empty)
- Correctness issues (e.g., comparing `&String` to `&str` incorrectly)

Clippy has ~700 lints organized into categories: `clippy::all` (default), `clippy::pedantic` (stricter), `clippy::nursery` (experimental).)

### Clippy (Foretias-Specific)

(@human For Foretias specifically, Clippy helps enforce the project's security and code quality standards:

- **`clippy::unwrap_used` / `clippy::expect_used`**: Foretias AGENTS.md explicitly forbids `unwrap()`/`expect()` in production code. Clippy can catch these automatically instead of relying on code review.

- **`clippy::unsafe_derive_deserialize`**: Flags `#[derive(Deserialize)]` on types containing `unsafe` — relevant for FFI wrappers.

- **`clippy::missing_safety_doc`**: Ensures all `unsafe fn` have safety documentation (already enforced by `#![deny(unsafe_op_in_unsafe_fn)]` but Clippy catches more edge cases).

- **`clippy::module_name_repetitions`**: Flags types like `noise::NoiseSession` — helps keep module paths clean.

- **`clippy::must_use_candidate`**: Flags functions that return `Result` but aren't marked `#[must_use]` — important for crypto operations where ignoring errors is dangerous.

- **`clippy::missing_errors_doc`**: Ensures public fallible functions document error conditions.)

### cargo-geiger (Generic)

(@human cargo-geiger is a tool that analyzes Rust crate dependencies to determine how much `unsafe` code they contain. It produces a report showing:

- **Safe**: Code that doesn't use `unsafe` at all
- **Unsafe (used)**: `unsafe` code that is actually compiled/linked
- **Unsafe (unused)**: `unsafe` code behind feature flags not enabled

The tool helps developers understand their project's "unsafe surface area" — how much untrusted low-level code they're pulling in. This is critical for security-sensitive projects where a vulnerable dependency could compromise the entire system.)

### cargo-geiger (Foretias-Specific)

(@human For Foretias, cargo-geiger serves a specific security audit purpose:

- **Dependency Unsafe Audit**: Foretias already has significant unsafe code in `core-engine` (C11 FFI wrappers). cargo-geiger shows whether dependencies like `libp2p`, `tokio`, or `serde` introduce *additional* unsafe code that isn't audited.

- **Supply Chain Risk**: If a dependency suddenly increases its unsafe code in a new version, cargo-geiger flags it. This prevents silent increases in attack surface.

- **Compliance Evidence**: For security audits, cargo-geiger reports demonstrate awareness of unsafe code provenance. Useful for any future security review or formal verification efforts.

- **Migration Tracking**: As Foretias moves toward post-quantum crypto (liboqs integration), cargo-geiger tracks whether the PQC dependencies increase unsafe surface.

Note: cargo-geiger is read-only and does not modify code — it only generates reports. Integration is manual (run on-demand or in CI for reporting).)

---

## Goal

After this spec is implemented:

- **`clippy.toml` exists** with project-specific lint thresholds
- **`rustfmt.toml` exists** with standardized formatting rules
- **`.editorconfig` exists** for cross-editor consistency
- **Clippy warnings are zero** for the default lint set
- **Cargo-geiger report** is generated and documented for baseline
- **CI integration** (if applicable) enforces clippy on new code

---

## Scope

### In Scope
| Component | Files | Action |
|-----------|-------|--------|
| Workspace root | `p2p/clippy.toml` | Create with project-specific lints |
| Workspace root | `p2p/rustfmt.toml` | Create with formatting rules |
| Project root | `.editorconfig` | Create for editor consistency |
| CI (optional) | `.github/workflows/` or similar | Add clippy check step |

### Out of Scope
- Changing existing code to fix clippy warnings (separate cleanup task)
- Automated cargo-geiger in CI (manual audit only for now)
- C11 core library (C code, not Rust)
- Python/Java bindings (backburnered)

---

## Configuration Specifications

### clippy.toml

```toml
# Foretias Clippy Configuration
# See: https://doc.rust-lang.org/clippy/configuration.html

# Warn on functions that could be `const` but aren't
warn-on-unnecessary-operation = true

# Warn on unused crate dependencies (helps keep Cargo.toml clean)
unused-crate-dependencies-deny = true

# Avoid breaking changes in public APIs
avoid-breaking-exported-api = true

# Raise threshold for large functions (default 100, keep at 100 for readability)
too-many-lines-threshold = 100

# Warn on too many arguments (default 7, keep at 7)
too-many-arguments-threshold = 7
```

### rustfmt.toml

```toml
# Foretias Rust Formatting Configuration
# See: https://rust-lang.github.io/rustfmt/

max_width = 100
tab_spaces = 4
edition = "2021"
use_field_init_shorthand = true
use_try_shorthand = true
```

### .editorconfig

```ini
# Foretias Editor Configuration
# https://editorconfig.org

root = true

[*]
indent_style = space
indent_size = 4
end_of_line = lf
charset = utf-8
trim_trailing_whitespace = true
insert_final_newline = true

[*.md]
trim_trailing_whitespace = false

[Makefile]
indent_style = tab
```

---

## Success Criteria

1. All three config files exist in the repository
2. `cargo clippy --workspace --all-targets` produces zero warnings
3. `cargo fmt --check` passes (no formatting changes needed)
4. A baseline cargo-geiger report is generated and saved to `docs/security/cargo-geiger-baseline.md`
5. All existing tests continue to pass (no behavioral changes)

---

## Risks

| Risk | Mitigation |
|------|------------|
| Clippy warnings may require code changes | Phase 2 (fixes) is separate from this spec |
| Strict clippy lints may slow CI | Run clippy only on changed files in CI |
| cargo-geiger may flag unexpected dependencies | Review findings, document exceptions |

---

## References

- [Clippy Configuration](https://doc.rust-lang.org/clippy/configuration.html)
- [rustfmt Configuration](https://rust-lang.github.io/rustfmt/)
- [cargo-geiger](https://github.com/nickel-org/cargo-geiger)
- [EditorConfig](https://editorconfig.org)
