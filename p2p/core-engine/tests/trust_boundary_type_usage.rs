//! Static analysis: detect trust boundary wrapper type usage across workspace.
//!
//! Uses syn + Visit trait to traverse the AST and find all occurrences of
//! CleanAuthenticated&lt;T&gt;, Unprocessed&lt;T&gt;, Externalized&lt;T&gt;.

use std::collections::HashMap;
use std::path::Path;

use syn::visit::Visit;

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
    wrapper: String, // "CleanAuthenticated", "Unprocessed", "Externalized"
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
            "Unprocessed" => entry.is_unprocessed = true,
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
                    "CleanAuthenticated" | "Unprocessed" | "Externalized"
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

fn collect_all_type_usages(workspace_root: &Path) -> (Vec<LocationEntry>, Vec<RawOccurrence>) {
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
                } else if path.extension().map_or(false, |e| e == "rs") {
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
    result.sort_by(|a, b| a.file.cmp(&b.file).then(a.inner.cmp(&b.inner)).then(a.context.cmp(&b.context)));
    (result, all_raw)
}

#[test]
fn test_wrapper_type_visitor_detects_wrappers() {
    // CARGO_MANIFEST_DIR = p2p/core-engine, parent = p2p (workspace root)
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let (locations, raw) = collect_all_type_usages(workspace);

    let wrappers: HashMap<String, usize> =
        raw
            .iter()
            .fold(HashMap::new(), |mut acc, u| {
                *acc.entry(u.wrapper.clone()).or_insert(0) += 1;
                acc
            });

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
        wrappers.contains_key("Unprocessed"),
        "Unprocessed not detected"
    );
    assert!(
        wrappers.contains_key("Externalized"),
        "Externalized not detected"
    );

    println!("\n=== Detailed Usage ===");
    for loc in &locations {
        let wrappers = vec![
            (loc.is_unprocessed, "Unprocessed"),
            (loc.is_clean_authenticated, "CleanAuthenticated"),
            (loc.is_externalized, "Externalized"),
        ];
        let active: Vec<_> = wrappers.into_iter().filter(|(b, _)| *b).map(|(_, n)| n).collect();
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
fn verify_trust_boundary_invariants(locations: &[LocationEntry]) -> Vec<String> {
    let mut violations = Vec::new();

    // Rule 1: core-engine should NOT contain Unprocessed usage EXCEPT in gate files
    // Gate files: clean_auth.rs (primary gate) or probity/report.rs (domain-specific gate)
    for loc in locations {
        if loc.file.starts_with("core-engine/src") && loc.is_unprocessed {
            let is_gate_file = loc.file.ends_with("clean_auth.rs")
                || loc.file.ends_with("probity/report.rs");
            if !is_gate_file {
                violations.push(format!(
                    "VIOLATION: Unprocessed found in core-engine outside a gate file: {} [{}] {}",
                    loc.file, "Unprocessed", loc.inner
                ));
            }
        }
    }

    // Rule 2: core-engine should NOT contain Externalized usage EXCEPT in gate files
    for loc in locations {
        if loc.file.starts_with("core-engine/src") && loc.is_externalized {
            let is_gate_file = loc.file.ends_with("clean_auth.rs")
                || loc.file.ends_with("probity/report.rs");
            if !is_gate_file {
                violations.push(format!(
                    "VIOLATION: Externalized found in core-engine outside a gate file: {} [{}] {}",
                    loc.file, "Externalized", loc.inner
                ));
            }
        }
    }

    // Rule 3: Unprocessed should ONLY appear in communerd, gate files, or test files
    for loc in locations {
        if loc.is_unprocessed && loc.inner != "bare" && loc.inner != "T" {
            let allowed = loc.file.contains("communerd")
                || loc.file.contains("clean_auth")
                || loc.file.contains("probity/report")
                || loc.file.contains("test");
            if !allowed {
                violations.push(format!(
                    "VIOLATION: Unprocessed outside allowed locations: {} [{}] {}",
                    loc.file, "Unprocessed", loc.inner
                ));
            }
        }
    }

    violations
}

fn format_usage_table(locations: &[LocationEntry]) -> String {
    let mut table = String::new();
    table.push_str("file | inner | context | Unprocessed | CleanAuthenticated | Externalized\n");
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

#[test]
fn test_trust_boundary_snapshot() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let (locations, raw) = collect_all_type_usages(workspace);
    let cross_tab = build_cross_tabulation(&raw);
    let violations = verify_trust_boundary_invariants(&locations);

    let usage_table = format_usage_table(&locations);
    let cross_tab_str = format_cross_tab(&cross_tab);

    let mut snapshot = String::new();
    snapshot.push_str("=== Trust Boundary Usage Snapshot ===\n");
    snapshot.push('\n');
    snapshot.push_str(&usage_table);
    snapshot.push_str("\n\n=== Cross-Tabulation ===\n");
    snapshot.push('\n');
    snapshot.push_str(&cross_tab_str);

    let snapshot_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots");
    std::fs::create_dir_all(&snapshot_dir).ok();
    let snapshot_path = snapshot_dir.join("trust_boundary_usage_snapshot.txt");

    let update_snapshot = std::env::var("UPDATE_SNAPSHOT").map(|v| v == "1").unwrap_or(false);

    if snapshot_path.exists() {
        let existing = std::fs::read_to_string(&snapshot_path)
            .expect("failed to read snapshot");
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
