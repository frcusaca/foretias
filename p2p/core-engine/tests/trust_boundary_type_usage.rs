//! Static analysis: detect trust boundary wrapper type usage across workspace.
//!
//! Uses syn + Visit trait to traverse the AST and find all occurrences of
//! CleanAuthenticated&lt;T&gt;, UnverifiedSignatureEnvelope&lt;T&gt;, Externalized&lt;T&gt;.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use syn::visit::Visit;

// TinmanSuite: rustdoc JSON type-resolved checks
use rustdoc_types::Crate;

#[derive(Debug, Clone)]
struct LocationEntry {
    file: String,
    inner: String,
    context: String,
    is_unprocessed: bool,
    is_clean_authenticated: bool,
    is_externalized: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RawOccurrence {
    file: String,
    wrapper: String, // "CleanAuthenticated", "UnverifiedSignatureEnvelope", "Externalized"
    inner: String,
    context: String,
}

struct WrapperTypeVisitor {
    locations: HashMap<(String, String, String), LocationEntry>,
    raw_occurrences: Vec<RawOccurrence>,
    current_file: String,
    context_stack: Vec<String>,
}

impl WrapperTypeVisitor {
    fn new(file: &str) -> Self {
        Self {
            locations: HashMap::new(),
            raw_occurrences: Vec::new(),
            current_file: file.to_string(),
            context_stack: Vec::new(),
        }
    }

    fn current_context(&self) -> String {
        if self.context_stack.is_empty() {
            "other".to_string()
        } else {
            self.context_stack.join("::")
        }
    }

    fn record_wrapper(&mut self, wrapper: &str, inner: &str, context: &str) {
        let key = (
            self.current_file.clone(),
            inner.to_string(),
            context.to_string(),
        );
        let entry = self.locations.entry(key).or_insert_with(|| LocationEntry {
            file: self.current_file.clone(),
            inner: inner.to_string(),
            context: context.to_string(),
            is_unprocessed: false,
            is_clean_authenticated: false,
            is_externalized: false,
        });
        match wrapper {
            "UnverifiedSignatureEnvelope" => entry.is_unprocessed = true,
            "CleanAuthenticated" => entry.is_clean_authenticated = true,
            "Externalized" => entry.is_externalized = true,
            _ => {}
        }
        self.raw_occurrences.push(RawOccurrence {
            file: self.current_file.clone(),
            wrapper: wrapper.to_string(),
            inner: inner.to_string(),
            context: context.to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for WrapperTypeVisitor {
    fn visit_type_path(&mut self, node: &syn::TypePath) {
        // Only handle simple paths (no qualified self)
        if node.qself.is_none() && node.path.leading_colon.is_none() {
            if let Some(first_seg) = node.path.segments.first() {
                let name = first_seg.ident.to_string();
                if matches!(
                    name.as_str(),
                    "CleanAuthenticated" | "UnverifiedSignatureEnvelope" | "Externalized"
                ) {
                    // In syn, CleanAuthenticated<ChrononRecord> is ONE segment with
                    // PathArguments::AngleBracketed — NOT two segments.
                    if let syn::PathArguments::AngleBracketed(args) = &first_seg.arguments {
                        for arg in &args.args {
                            if let syn::GenericArgument::Type(t) = arg {
                                let inner_name = match t {
                                    syn::Type::Path(tp) => {
                                        if let Some(seg) = tp.path.segments.first() {
                                            seg.ident.to_string()
                                        } else {
                                            "unknown".to_string()
                                        }
                                    }
                                    syn::Type::Array(_) => "array".to_string(),
                                    syn::Type::Tuple(_) => "tuple".to_string(),
                                    syn::Type::Reference(_) => "ref".to_string(),
                                    _ => "other".to_string(),
                                };
                                self.record_wrapper(&name, &inner_name, &self.current_context());
                            }
                        }
                    } else {
                        // Bare usage without generic argument
                        self.record_wrapper(&name, "bare", &self.current_context());
                    }
                }
            }
        }
        syn::visit::visit_type_path(self, node);
    }

    fn visit_item_struct(&mut self, node: &syn::ItemStruct) {
        self.context_stack.push(format!("struct::{}", node.ident));
        syn::visit::visit_item_struct(self, node);
        self.context_stack.pop();
    }

    fn visit_item_impl(&mut self, node: &syn::ItemImpl) {
        let self_ty = match &*node.self_ty {
            syn::Type::Path(tp) => {
                if let Some(seg) = tp.path.segments.first() {
                    seg.ident.to_string()
                } else {
                    "unknown".to_string()
                }
            }
            _ => "unknown".to_string(),
        };
        self.context_stack.push(format!("impl::{}", self_ty));
        syn::visit::visit_item_impl(self, node);
        self.context_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &syn::ImplItemFn) {
        self.context_stack.push(format!("fn::{}", node.sig.ident));
        syn::visit::visit_impl_item_fn(self, node);
        self.context_stack.pop();
    }

    fn visit_item_fn(&mut self, node: &syn::ItemFn) {
        self.context_stack.push(format!("fn::{}", node.sig.ident));
        syn::visit::visit_item_fn(self, node);
        self.context_stack.pop();
    }
}

fn collect_ast_usages(workspace_root: &Path) -> (Vec<LocationEntry>, Vec<RawOccurrence>) {
    let mut all_locations = HashMap::new();
    let mut all_raw = Vec::new();
    let src_dirs = vec![
        "core-engine/src".to_string(),
        "foretias-client/src".to_string(),
        "foretias-server/src".to_string(),
    ];

    for dir in &src_dirs {
        let full_dir = workspace_root.join(dir);
        let mut stack = vec![full_dir];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let Ok(src) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let rel = path.strip_prefix(workspace_root).unwrap_or(&path);
                    let file_str = rel.display().to_string();
                    let Ok(ast) = syn::parse_file(&src) else {
                        continue;
                    };
                    let mut visitor = WrapperTypeVisitor::new(&file_str);
                    visitor.visit_file(&ast);
                    all_locations.extend(visitor.locations);
                    all_raw.extend(visitor.raw_occurrences);
                }
            }
        }
    }
    let mut result: Vec<LocationEntry> = all_locations.into_values().collect();
    result.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.inner.cmp(&b.inner))
            .then(a.context.cmp(&b.context))
    });
    (result, all_raw)
}

#[test]
fn test_wrapper_type_visitor_detects_wrappers() {
    // CARGO_MANIFEST_DIR = p2p/core-engine, parent = p2p (workspace root)
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let (locations, raw) = collect_ast_usages(workspace);

    let mut wrappers: HashMap<String, usize> = HashMap::new();
    for u in &raw {
        *wrappers.entry(u.wrapper.clone()).or_insert(0) += 1;
    }

    println!("\n=== Trust Boundary Wrapper Usage Summary ===");
    for (wrapper, count) in &wrappers {
        println!("  {}: {} occurrences", wrapper, count);
    }

    // Verify all three wrapper types are detected
    assert!(
        wrappers.contains_key("CleanAuthenticated"),
        "CleanAuthenticated not detected"
    );
    assert!(
        wrappers.contains_key("UnverifiedSignatureEnvelope"),
        "UnverifiedSignatureEnvelope not detected"
    );
    assert!(
        wrappers.contains_key("Externalized"),
        "Externalized not detected"
    );

    println!("\n=== Detailed Usage ===");
    for loc in &locations {
        let wrappers = vec![
            (loc.is_unprocessed, "UnverifiedSignatureEnvelope"),
            (loc.is_clean_authenticated, "CleanAuthenticated"),
            (loc.is_externalized, "Externalized"),
        ];
        let active: Vec<_> = wrappers
            .into_iter()
            .filter(|(b, _)| *b)
            .map(|(_, n)| n)
            .collect();
        println!(
            "  {} [{}] {} (context: {})",
            loc.file,
            active.join(", "),
            loc.inner,
            loc.context
        );
    }
}

fn build_cross_tabulation(raw: &[RawOccurrence]) -> Vec<(String, String, String, usize)> {
    let mut map: HashMap<(String, String, String), usize> = HashMap::new();
    for u in raw {
        let crate_name = u.file.split('/').next().unwrap_or("unknown").to_string();
        let key = (crate_name, u.wrapper.clone(), u.inner.clone());
        *map.entry(key).or_insert(0) += 1;
    }
    let mut result: Vec<_> = map.into_iter().map(|(k, v)| (k.0, k.1, k.2, v)).collect();
    result.sort();
    result
}

/// Verify trust boundary invariants from AGENTS.md type-enforced trust boundaries.
fn verify_ast_invariants(locations: &[LocationEntry]) -> Vec<String> {
    let mut violations = Vec::new();

    // Rule 1: core-engine should NOT contain UnverifiedSignatureEnvelope usage EXCEPT in gate files
    // Gate files: clean_auth.rs (primary gate) or probity/report.rs (domain-specific gate)
    for loc in locations {
        if loc.file.starts_with("core-engine/src") && loc.is_unprocessed {
            let is_gate_file =
                loc.file.ends_with("clean_auth.rs") || loc.file.ends_with("probity/report.rs");
            if !is_gate_file {
                violations.push(format!(
                    "VIOLATION: UnverifiedSignatureEnvelope found in core-engine outside a gate file: {} [{}] {}",
                    loc.file, "UnverifiedSignatureEnvelope", loc.inner
                ));
            }
        }
    }

    // Rule 2: core-engine should NOT contain Externalized usage EXCEPT in gate files
    for loc in locations {
        if loc.file.starts_with("core-engine/src") && loc.is_externalized {
            let is_gate_file =
                loc.file.ends_with("clean_auth.rs") || loc.file.ends_with("probity/report.rs");
            if !is_gate_file {
                violations.push(format!(
                    "VIOLATION: Externalized found in core-engine outside a gate file: {} [{}] {}",
                    loc.file, "Externalized", loc.inner
                ));
            }
        }
    }

    // Rule 3: UnverifiedSignatureEnvelope should ONLY appear in communerd, gate files, server handlers
    // (handlers.rs is a parse boundary: it creates UnverifiedSignatureEnvelope<T> from wire JSON
    // and hands it to communerd or crypto for authentication), or test files
    for loc in locations {
        if loc.is_unprocessed && loc.inner != "bare" && loc.inner != "T" {
            let allowed = loc.file.contains("communerd")
                || loc.file.contains("clean_auth")
                || loc.file.contains("probity/report")
                || loc.file.contains("server/handlers")
                || loc.file.contains("test");
            if !allowed {
                violations.push(format!(
                    "VIOLATION: UnverifiedSignatureEnvelope outside allowed locations: {} [{}] {}",
                    loc.file, "UnverifiedSignatureEnvelope", loc.inner
                ));
            }
        }
    }

    violations
}

fn format_ast_table(locations: &[LocationEntry]) -> String {
    let mut table = String::new();
    table.push_str("file | inner | context | UnverifiedSignatureEnvelope | CleanAuthenticated | Externalized\n");
    table.push_str(&"=".repeat(120));
    table.push('\n');
    for loc in locations {
        let up = if loc.is_unprocessed { "Y" } else { "-" };
        let ca = if loc.is_clean_authenticated { "Y" } else { "-" };
        let ex = if loc.is_externalized { "Y" } else { "-" };
        use std::fmt::Write;
        writeln!(
            table,
            "{} | {} | {} | {} | {} | {}",
            loc.file, loc.inner, loc.context, up, ca, ex
        )
        .unwrap();
    }
    table
}

/// Build a formatted cross-tabulation string.
fn format_cross_tab(cross_tab: &[(String, String, String, usize)]) -> String {
    let mut tab = String::new();
    tab.push_str("crate | wrapper | inner | count\n");
    tab.push_str(&"=".repeat(80));
    tab.push('\n');
    for (crate_name, wrapper, inner, count) in cross_tab {
        use std::fmt::Write;
        writeln!(tab, "{} | {} | {} | {}", crate_name, wrapper, inner, count).unwrap();
    }
    tab
}

/// Verify primitives recognized by check_gate_bodies.
const VERIFY_PRIMITIVES: &[&str] = &["verify", "verify_with", "tbid_verify"];

/// Check that every gate function body references all required elements.
/// A gate function has a DontUse/UnverifiedSignatureEnvelope parameter and returns Clean*.
/// Must reference: always_require_full_signature, signature data, verify primitive.
/// Opt-out: // gate-strawman-exempt: <reason>
fn check_gate_bodies(source: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut in_gate_fn = false;
    let mut fn_name = String::new();
    let mut fn_start = 0usize;
    let mut brace_depth = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if !in_gate_fn
            && (trimmed.contains("fn ") && trimmed.contains("UnverifiedSignatureEnvelope"))
        {
            in_gate_fn = true;
            fn_name = trimmed
                .split("fn ")
                .nth(1)
                .unwrap_or("")
                .split('(')
                .next()
                .unwrap_or("")
                .to_string();
            fn_start = i;
            brace_depth = 0;
        }

        if in_gate_fn {
            brace_depth += line.chars().filter(|&c| c == '{').count();
            brace_depth -= line.chars().filter(|&c| c == '}').count();

            if trimmed.starts_with("// gate-strawman-exempt:") {
                in_gate_fn = false;
                continue;
            }

            if brace_depth == 0 && i > fn_start {
                let body = lines[fn_start..=i].join("\n");
                let has_always = body.contains("always_require_full_signature");
                let has_sig_data = body.contains("signatures") || body.contains("matrix");
                let has_verify = VERIFY_PRIMITIVES.iter().any(|p| body.contains(*p));

                if !has_always {
                    violations.push(format!(
                        "{}: missing always_require_full_signature",
                        fn_name
                    ));
                }
                if !has_sig_data {
                    violations.push(format!("{}: missing signature data reference", fn_name));
                }
                if !has_verify {
                    violations.push(format!("{}: missing verify primitive", fn_name));
                }

                in_gate_fn = false;
            }
        }
    }

    violations
}

#[test]
fn test_trust_boundary_snapshot() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let (locations, raw) = collect_ast_usages(workspace);
    let cross_tab = build_cross_tabulation(&raw);
    let violations = verify_ast_invariants(&locations);

    let usage_table = format_ast_table(&locations);
    let cross_tab_str = format_cross_tab(&cross_tab);

    let mut snapshot = String::new();
    snapshot.push_str("=== StrawmanSuite (AST / syn) ===\n");
    snapshot.push('\n');
    snapshot.push_str(&usage_table);
    snapshot.push_str("\n\n=== Cross-Tabulation ===\n");
    snapshot.push('\n');
    snapshot.push_str(&cross_tab_str);

    // TinmanSuite section
    let crate_path = workspace.join("core-engine");
    let (tinman_usages, tinman_skipped) = match invoke_rustdoc_json(&crate_path) {
        Ok(crate_json) => (collect_semantic_usages(&crate_json), None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let tinman_violations = verify_semantic_invariants(&tinman_usages);
    snapshot.push_str("\n\n");
    if let Some(e) = tinman_skipped {
        snapshot.push_str(&format!("=== TinmanSuite ===\n\n  (skipped: {})\n", e));
    } else {
        snapshot.push_str(&format_tinman_suite(&tinman_usages, &tinman_violations));
    }

    // Cross-tabulation section
    let merged = merge_into_table(&raw, &tinman_usages);
    snapshot.push_str("\n\n");
    snapshot.push_str(&format_merged_table(&merged));

    // StrawmanSuite gate body check
    let gate_violations = check_gate_bodies_in_workspace(workspace);
    if !gate_violations.is_empty() {
        snapshot.push_str("\n\n=== StrawmanSuite Gate Body Violations ===\n");
        for v in &gate_violations {
            use std::fmt::Write;
            writeln!(snapshot, "  {}", v).unwrap();
        }
    }

    let snapshot_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    std::fs::create_dir_all(&snapshot_dir).ok();
    let snapshot_path = snapshot_dir.join("trust_boundary_usage_snapshot.txt");

    let update_snapshot = std::env::var("UPDATE_SNAPSHOT")
        .map(|v| v == "1")
        .unwrap_or(false);

    if snapshot_path.exists() {
        let existing = std::fs::read_to_string(&snapshot_path).expect("failed to read snapshot");
        if existing != snapshot && !update_snapshot {
            panic!(
                "Snapshot mismatch.\n\nExpected:\n{}\n\nActual:\n{}\n\nSet UPDATE_SNAPSHOT=1 to update.",
                existing, snapshot
            );
        }
    }
    std::fs::write(&snapshot_path, &snapshot).expect("failed to write snapshot");

    println!("\n=== Cross-Tabulation ===");
    for (crate_name, wrapper, inner, count) in &cross_tab {
        println!("  {} | {} | {} | {}", crate_name, wrapper, inner, count);
    }

    println!("\n=== Invariant Violations ===");
    if violations.is_empty() {
        println!("  No violations detected");
    } else {
        for v in &violations {
            println!("  {}", v);
        }
        panic!("{} trust boundary violations detected", violations.len());
    }
}

// ---------------------------------------------------------------------------
// TinmanSuite: rustdoc JSON type-resolved checks
// ---------------------------------------------------------------------------

/// Known wrapper type names to search for in resolved types.
const WRAPPER_NAMES: &[&str] = &[
    "CleanAuthenticated",
    "UnverifiedSignatureEnvelope",
    "Externalized",
];

/// Invoke `cargo +nightly rustdoc -Z unstable-options --output-format json`
/// and return the parsed Crate.
///
/// Reads the JSON from the generated file (`target/doc/{crate_name}.json`)
/// rather than stdout, because `cargo rustdoc --output-format json` writes
/// to a file.
///
/// **Rustdoc JSON schema versioning limitation:** The `rustdoc-types` crate
/// version must match the nightly rustdoc JSON output format. If the
/// `format_version` field in the JSON doesn't match the expected version,
/// parsing will fail. Pin `rustdoc-types` in Cargo.toml and update when
/// upgrading nightly.
fn invoke_rustdoc_json(crate_path: &Path) -> Result<Crate, String> {
    // Try nightly first (needed for --output-format json / -Z unstable-options)
    let output = Command::new("cargo")
        .current_dir(crate_path)
        .args([
            "+nightly",
            "rustdoc",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
        ])
        .output()
        .map_err(|e| format!("cargo rustdoc failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo rustdoc exited with error: {}", stderr));
    }

    // Read the JSON from the generated file (not stdout)
    // Cargo uses the workspace-level target dir, not the crate's own target dir
    let json_path = crate_path
        .parent()
        .unwrap()
        .join("target/doc/foretias_core.json");
    let json_bytes = std::fs::read(&json_path).map_err(|e| {
        format!(
            "failed to read rustdoc JSON at {}: {}",
            json_path.display(),
            e
        )
    })?;

    let crate_json: Crate = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("failed to parse rustdoc JSON: {}", e))?;

    // Assert format_version to catch schema drift early
    // rustdoc-types 0.57.x uses format_version 57; update this when upgrading
    let expected_version: u32 = 57;
    if crate_json.format_version != expected_version {
        return Err(format!(
            "rustdoc JSON format_version mismatch: got {}, expected {}. \
             Update the rustdoc-types dependency to match your nightly toolchain.",
            crate_json.format_version, expected_version
        ));
    }

    Ok(crate_json)
}

/// Convert a rustdoc Type to its string representation.
fn type_to_string(ty: &rustdoc_types::Type) -> String {
    match ty {
        rustdoc_types::Type::ResolvedPath(path) => path.path.clone(),
        rustdoc_types::Type::Generic(name) => name.clone(),
        rustdoc_types::Type::Primitive(name) => name.clone(),
        rustdoc_types::Type::Tuple(types) => format!(
            "({})",
            types
                .iter()
                .map(type_to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        rustdoc_types::Type::FunctionPointer(fn_ptr) => {
            format!(
                "fn({})",
                fn_ptr
                    .sig
                    .inputs
                    .iter()
                    .map(|(_, ty)| type_to_string(ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        rustdoc_types::Type::QualifiedPath {
            name, self_type, ..
        } => {
            format!("{}::{}", type_to_string(self_type), name)
        }
        _ => String::from("..."),
    }
}

/// Resolve a type name to determine if it's a wrapper or an alias/newtype.
fn resolve_type(type_str: &str) -> Option<String> {
    for wrapper in WRAPPER_NAMES {
        if type_str.contains(wrapper) {
            return Some(wrapper.to_string());
        }
    }
    None
}

/// Collect semantic usages of wrapper types from resolved rustdoc JSON.
fn collect_semantic_usages(crate_: &Crate) -> Vec<(String, String, String)> {
    let mut usages = Vec::new();
    for item in crate_.index.values() {
        let item_name = item.name.as_deref().unwrap_or("<unnamed>").to_string();
        match &item.inner {
            rustdoc_types::ItemEnum::Function(func) => {
                for (name, ty) in &func.sig.inputs {
                    let type_str = type_to_string(ty);
                    if let Some(wrapper) = resolve_type(&type_str) {
                        usages.push((
                            item_name.clone(),
                            wrapper.clone(),
                            format!("param {}: {}", name, type_str),
                        ));
                    }
                }
                if let Some(output_ty) = &func.sig.output {
                    let output_str = type_to_string(output_ty);
                    if let Some(wrapper) = resolve_type(&output_str) {
                        usages.push((
                            item_name.clone(),
                            wrapper.clone(),
                            format!("output: {}", output_str),
                        ));
                    }
                }
            }
            rustdoc_types::ItemEnum::AssocType {
                generics,
                type_: default,
                ..
            } => {
                if let Some(default_ty) = default {
                    let type_str = type_to_string(default_ty);
                    if let Some(wrapper) = resolve_type(&type_str) {
                        usages.push((
                            item_name.clone(),
                            wrapper.clone(),
                            format!("assoc_type default: {}", type_str),
                        ));
                    }
                }
                for param in &generics.params {
                    if let rustdoc_types::GenericParamDefKind::Type { bounds, .. } = &param.kind {
                        for bound in bounds {
                            let bound_str = bound_to_string(bound);
                            if let Some(wrapper) = resolve_type(&bound_str) {
                                usages.push((
                                    item_name.clone(),
                                    wrapper.clone(),
                                    format!("assoc_type bound: {}", bound_str),
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    usages.sort();
    usages
}

fn bound_to_string(bound: &rustdoc_types::GenericBound) -> String {
    match bound {
        rustdoc_types::GenericBound::TraitBound { trait_, .. } => trait_.path.clone(),
        rustdoc_types::GenericBound::Outlives(lifetime) => lifetime.clone(),
        _ => "...".to_string(),
    }
}

/// Verify semantic invariants: flag Resolved (alias/newtype) occurrences as violations.
fn verify_semantic_invariants(usages: &[(String, String, String)]) -> Vec<String> {
    let mut violations = Vec::new();

    for (fn_name, wrapper, context) in usages {
        // Check for alias/newtype usage (these are Type::Alias in rustdoc JSON)
        if context.contains("Alias") {
            violations.push(format!(
                "{}: {} used via alias/newtype (should be direct wrapper)",
                fn_name, wrapper
            ));
        }
    }

    violations
}

/// Format TinmanSuite findings into a snapshot section.
fn format_tinman_suite(usages: &[(String, String, String)], violations: &[String]) -> String {
    let mut section = String::from("=== TinmanSuite ===\n");
    section.push('\n');
    if usages.is_empty() {
        section.push_str("  (no wrapper usages found in rustdoc JSON)\n");
    } else {
        for (fn_name, wrapper, context) in usages {
            use std::fmt::Write;
            writeln!(section, "  {} | {} | {}", fn_name, wrapper, context).unwrap();
        }
    }
    if !violations.is_empty() {
        section.push_str("\n  Violations:\n");
        for v in violations {
            use std::fmt::Write;
            writeln!(section, "    {}", v).unwrap();
        }
    }
    section
}

/// Merge AST usages and TinmanSuite usages into a cross-tabulation table.
fn merge_into_table(
    ast_raw: &[RawOccurrence],
    tinman_usages: &[(String, String, String)],
) -> Vec<(String, String, usize)> {
    let mut map: HashMap<(String, String), usize> = HashMap::new();
    for u in ast_raw {
        let key = (u.wrapper.clone(), u.inner.clone());
        *map.entry(key).or_insert(0) += 1;
    }
    for (_, wrapper, context) in tinman_usages {
        let key = (wrapper.clone(), context.clone());
        *map.entry(key).or_insert(0) += 1;
    }
    let mut result: Vec<_> = map.into_iter().map(|(k, v)| (k.0, k.1, v)).collect();
    result.sort();
    result
}

/// Format the merged cross-tabulation table.
fn format_merged_table(merged: &[(String, String, usize)]) -> String {
    let mut tab = String::from("=== TinmanSuite Cross-Tabulation ===\n");
    tab.push('\n');
    tab.push_str("wrapper | inner/context | count\n");
    tab.push_str(&"=".repeat(80));
    tab.push('\n');
    for (wrapper, inner, count) in merged {
        use std::fmt::Write;
        writeln!(tab, "{} | {} | {}", wrapper, inner, count).unwrap();
    }
    tab
}

/// Run check_gate_bodies on all Rust source files in the workspace.
fn check_gate_bodies_in_workspace(workspace: &Path) -> Vec<String> {
    let mut all_violations = Vec::new();
    let src_dirs = vec![
        "core-engine/src".to_string(),
        "foretias-client/src".to_string(),
        "foretias-server/src".to_string(),
    ];

    for dir in &src_dirs {
        let full_dir = workspace.join(dir);
        let mut stack = vec![full_dir];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let Ok(src) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let rel = path.strip_prefix(workspace).unwrap_or(&path);
                    let file_str = rel.display().to_string();
                    let violations = check_gate_bodies(&src);
                    for v in violations {
                        all_violations.push(format!("{}: {}", file_str, v));
                    }
                }
            }
        }
    }
    all_violations
}
