//! Phase 11.5 — Crypto call-site snapshot audit (spec §11.6, §15 criterion 13).
//!
//! Scans the Rust source tree for every location that:
//!   1. Invokes a CryptoServer primitive (sign, verify, sha256, etc.)
//!   2. Calls TbidSecret::sign or sign_tbid_message (TBID-level signing)
//!   3. Produces or consumes CleanAuthenticated / CleanFullyAuthenticated
//!
//! Results are stored as an approved snapshot. The test fails if the current
//! scan differs from the committed snapshot, forcing a deliberate review of
//! any new or removed crypto call site.

use foretias_core::snapshot_suite::{build_snapshot, SnapshotSuite};
use std::path::{Path, PathBuf};

// ── patterns ─────────────────────────────────────────────────────────────────

/// Patterns that identify crypto call sites.
const SIGN_PATTERNS: &[&str] = &[
    "sign_with(",
    ".sign(",
    "sign_tbid_message(",
    "tbid_sign(",
    // SIGN annotation comments left by developers
    "// SIGN(",
];

const VERIFY_PATTERNS: &[&str] = &[
    "verify_with(",
    "verify_ed25519(",
    "verify_p256(",
    ".verify(",
    // VERIFY annotation comments
    "// VERIFY(",
];

const GATE_PATTERNS: &[&str] = &[
    "CleanAuthenticated::from_trusted(",
    "CleanFullyAuthenticated::from_dual_verified(",
    "UnverifiedSignatureEnvelope<",
    "gate_foretis(",
    "gate_chronon_records(",
    "from_dual_verified(",
];

// ── scanner ──────────────────────────────────────────────────────────────────

struct CallSite {
    rel_path: String,
    line: usize,
    kind: &'static str,
    snippet: String,
}

fn scan_dir(root: &Path) -> Vec<CallSite> {
    let mut hits = Vec::new();
    scan_recursive(root, root, &mut hits);
    hits.sort_by(|a, b| a.rel_path.cmp(&b.rel_path).then(a.line.cmp(&b.line)));
    hits
}

fn scan_recursive(root: &Path, dir: &Path, hits: &mut Vec<CallSite>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            scan_recursive(root, &path, hits);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            scan_file(root, &path, hits);
        }
    }
}

fn scan_file(root: &Path, path: &Path, hits: &mut Vec<CallSite>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();

    for (lineno, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Skip pure comment lines that aren't annotation comments
        if trimmed.starts_with("//")
            && !trimmed.contains("// SIGN(")
            && !trimmed.contains("// VERIFY(")
        {
            continue;
        }

        let kind = classify(line);
        if let Some(k) = kind {
            hits.push(CallSite {
                rel_path: rel.clone(),
                line: lineno + 1,
                kind: k,
                snippet: trimmed.chars().take(80).collect(),
            });
        }
    }
}

fn classify(line: &str) -> Option<&'static str> {
    for p in SIGN_PATTERNS {
        if line.contains(p) {
            return Some("sign");
        }
    }
    for p in VERIFY_PATTERNS {
        if line.contains(p) {
            return Some("verify");
        }
    }
    for p in GATE_PATTERNS {
        if line.contains(p) {
            return Some("gate");
        }
    }
    None
}

// ── snapshot ─────────────────────────────────────────────────────────────────

fn suite() -> SnapshotSuite {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    SnapshotSuite::new(base.join("snapshot_tests").join("approved"), "")
}

/// Scan the workspace source and compare against the committed snapshot.
///
/// To regenerate: set UPDATE_SNAPSHOT=1 in the environment.
#[test]
fn crypto_callsite_snapshot() {
    let ws_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("p2p dir")
        .to_path_buf();

    let hits = scan_dir(&ws_root);

    // Build formatted result
    let rows: Vec<String> = hits
        .iter()
        .map(|h| {
            format!(
                "{:60}  {:6}  {:6}  {}",
                h.rel_path, h.line, h.kind, h.snippet
            )
        })
        .collect();
    let result = rows.join("\n");

    let input = serde_json::json!({
        "test": "crypto_callsite_snapshot",
        "description": "All sign/verify/gate call sites in the Rust workspace",
        "source_root": ws_root.to_string_lossy(),
    });
    let input_str = serde_json::to_string_pretty(&input).unwrap();
    let comments = "crypto_callsite_snapshot — update with UPDATE_SNAPSHOT=1 when adding new crypto call sites";

    let actual = build_snapshot("", &input_str, &[result], comments).unwrap();

    let suite = suite();

    if std::env::var("UPDATE_SNAPSHOT").is_ok() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("snapshot_tests")
            .join("approved");
        std::fs::create_dir_all(&dir).expect("create approved dir");
        std::fs::write(dir.join("crypto_callsite_snapshot.snap"), &actual).expect("write snapshot");
        println!(
            "Snapshot updated: crypto_callsite_snapshot.snap ({} call sites)",
            hits.len()
        );
        return;
    }

    match suite.compare("crypto_callsite_snapshot", &actual) {
        Ok(()) => {}
        Err(failure) => {
            if let Some(v) = suite.verify_signatures(&actual) {
                assert!(v.all_ok(), "SIGNATURES footer must verify: {:?}", v);
            }
            panic!(
                "{}\n\nSet UPDATE_SNAPSHOT=1 to approve new call sites after review.",
                failure
            );
        }
    }
}
