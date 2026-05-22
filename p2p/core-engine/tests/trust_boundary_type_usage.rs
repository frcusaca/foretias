//! Static analysis: detect trust boundary wrapper type usage across workspace.
//!
//! Uses syn + Visit trait to traverse the AST and find all occurrences of
//! CleanAuthenticated&lt;T&gt;, Unprocessed&lt;T&gt;, Externalized&lt;T&gt;.

use std::collections::HashMap;
use std::path::Path;

use syn::visit::Visit;

#[derive(Debug, Clone)]
struct TypeUsage {
    file: String,
    wrapper: String, // "CleanAuthenticated", "Unprocessed", "Externalized"
    inner: String,   // e.g. "ChrononRecord", "Foretis", "EpochSnapshot", "ProbityReport"
    context: String, // brief context: "struct::X", "impl::X::fn::Y", "fn::Z"
}

struct WrapperTypeVisitor {
    usages: Vec<TypeUsage>,
    current_file: String,
    context_stack: Vec<String>,
}

impl WrapperTypeVisitor {
    fn new(file: &str) -> Self {
        Self {
            usages: Vec::new(),
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
                                self.usages.push(TypeUsage {
                                    file: self.current_file.clone(),
                                    wrapper: name.clone(),
                                    inner: inner_name,
                                    context: self.current_context(),
                                });
                            }
                        }
                    } else {
                        // Bare usage without generic argument
                        self.usages.push(TypeUsage {
                            file: self.current_file.clone(),
                            wrapper: name,
                            inner: "bare".to_string(),
                            context: self.current_context(),
                        });
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

/// Collect all wrapper type usages across the workspace source directories.
fn collect_all_type_usages(workspace_root: &Path) -> Vec<TypeUsage> {
    let mut all_usages = Vec::new();
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
                    all_usages.extend(visitor.usages);
                }
            }
        }
    }
    all_usages
}

#[test]
fn test_wrapper_type_visitor_detects_wrappers() {
    // CARGO_MANIFEST_DIR = p2p/core-engine, parent = p2p (workspace root)
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();

    let usages = collect_all_type_usages(workspace);

    let wrappers: HashMap<String, usize> =
        usages
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
    for u in &usages {
        println!(
            "  {} [{}] {} (context: {})",
            u.file, u.wrapper, u.inner, u.context
        );
    }
}

/// Build cross-tabulation: (crate, wrapper, inner) -> count
fn build_cross_tabulation(usages: &[TypeUsage]) -> Vec<(String, String, String, usize)> {
    let mut map: HashMap<(String, String, String), usize> = HashMap::new();
    for u in usages {
        let crate_name = u.file.split('/').next().unwrap_or("unknown").to_string();
        let key = (crate_name, u.wrapper.clone(), u.inner.clone());
        *map.entry(key).or_insert(0) += 1;
    }
    let mut result: Vec<_> = map.into_iter().map(|(k, v)| (k.0, k.1, k.2, v)).collect();
    result.sort();
    result
}

/// Verify trust boundary invariants from AGENTS.md type-enforced trust boundaries.
fn verify_trust_boundary_invariants(usages: &[TypeUsage]) -> Vec<String> {
    let mut violations = Vec::new();

    // Rule 1: core-engine should NOT contain Unprocessed usage EXCEPT in clean_auth.rs
    for u in usages {
        if u.file.starts_with("core-engine/src") && u.wrapper == "Unprocessed" {
            if !u.file.ends_with("clean_auth.rs") {
                violations.push(format!(
                    "VIOLATION: Unprocessed found in core-engine outside clean_auth.rs: {} [{}] {}",
                    u.file, u.wrapper, u.inner
                ));
            }
        }
    }

    // Rule 2: core-engine should NOT contain Externalized usage EXCEPT in clean_auth.rs
    for u in usages {
        if u.file.starts_with("core-engine/src") && u.wrapper == "Externalized" {
            if !u.file.ends_with("clean_auth.rs") {
                violations.push(format!(
                    "VIOLATION: Externalized found in core-engine outside clean_auth.rs: {} [{}] {}",
                    u.file, u.wrapper, u.inner
                ));
            }
        }
    }

    // Rule 3: Unprocessed should ONLY appear in communerd, clean_auth, or test files
    for u in usages {
        if u.wrapper == "Unprocessed" && u.inner != "bare" && u.inner != "T" {
            let allowed = u.file.contains("communerd")
                || u.file.contains("clean_auth")
                || u.file.contains("test");
            if !allowed {
                violations.push(format!(
                    "VIOLATION: Unprocessed outside allowed locations: {} [{}] {}",
                    u.file, u.wrapper, u.inner
                ));
            }
        }
    }

    violations
}

/// Build a formatted table string from usages.
fn format_usage_table(usages: &[TypeUsage]) -> String {
    let mut table = String::new();
    table.push_str("file | crate | wrapper | inner | context\n");
    table.push_str(&"=".repeat(120));
    table.push('\n');
    for u in usages {
        let crate_name = u.file.split('/').next().unwrap_or("?");
        use std::fmt::Write;
        writeln!(
            table,
            "{} | {} | {} | {} | {}",
            u.file, crate_name, u.wrapper, u.inner, u.context
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

    let usages = collect_all_type_usages(workspace);
    let cross_tab = build_cross_tabulation(&usages);
    let violations = verify_trust_boundary_invariants(&usages);

    let usage_table = format_usage_table(&usages);
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
