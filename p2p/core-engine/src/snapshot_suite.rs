//! SnapshotSuite — approval-based snapshot testing with cryptographic provenance.
//!
//! Unlike foolish-rust's file-driven approach, foretias uses code-driven tests:
//! test names come from the implementation, not file paths.

use std::fs;
use std::path::PathBuf;

use crate::snapshot_signature::{sign_snapshot, verify_snapshot, parse_snapshot_footer, SnapshotVerification};

/// Trait for test scenarios that produce a signed snapshot output.
///
/// A foretias evaluator executes a stamp/verify scenario against a server
/// and returns the formatted output.
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

/// Error type for SnapshotSuite initialization failures.
#[derive(Debug)]
pub enum SnapshotSuiteError {
    /// Approved output files exist without corresponding test registrations.
    ExtraneousOutputs { files: Vec<String> },
}

impl std::fmt::Display for SnapshotSuiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotSuiteError::ExtraneousOutputs { files } => {
                writeln!(f, "Extraneous approved output files (no matching test):")?;
                for file in files {
                    writeln!(f, "  - {}", file)?;
                }
                write!(f, "Remove these files or register matching tests.")
            }
        }
    }
}

impl std::error::Error for SnapshotSuiteError {}

/// Failure type for individual snapshot tests.
#[derive(Debug)]
pub enum TestFailure {
    /// No approved snapshot exists yet.
    Pending { name: String, actual: String },
    /// Output differs from approved snapshot.
    Mismatch {
        name: String,
        expected: String,
        actual: String,
    },
    /// Evaluation or I/O error.
    Error { name: String, message: String },
}

impl std::fmt::Display for TestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestFailure::Pending { name, actual } => {
                write!(f, "PENDING: {} \u{2014} no approved snapshot", name)?;
                if !actual.is_empty() {
                    write!(f, " (actual output: {})", actual)?;
                }
                Ok(())
            }
            TestFailure::Mismatch { name, expected, actual } => {
                write!(
                    f,
                    "MISMATCH: {} \u{2014} output differs from approved snapshot\n  expected: {}\n  actual:   {}",
                    name, expected, actual
                )
            }
            TestFailure::Error { name, message } => {
                write!(f, "ERROR: {} \u{2014} {}", name, message)
            }
        }
    }
}

/// Orchestrator for snapshot-based approval testing.
///
/// Compares evaluation output against approved snapshots in `approved_dir`.
/// Supports cryptographic verification of the SIGNATURES footer.
pub struct SnapshotSuite {
    approved_dir: PathBuf,
    passphrase:   String,
}

impl SnapshotSuite {
    /// Create a new suite for an approved snapshots directory.
    pub fn new(approved_dir: impl Into<PathBuf>, passphrase: &str) -> Self {
        Self {
            approved_dir: approved_dir.into(),
            passphrase:   passphrase.to_string(),
        }
    }

    /// Return the path to the approved snapshot for a given test name.
    fn snapshot_path(&self, test_name: &str) -> PathBuf {
        self.approved_dir.join(format!("{}.snap", test_name))
    }

    /// Compare actual output against an approved snapshot.
    ///
    /// Returns `Ok(())` if the snapshot matches.
    /// Returns `TestFailure::Pending` if no approved snapshot exists.
    /// Returns `TestFailure::Mismatch` if the output differs.
    pub fn compare(&self, test_name: &str, actual: &str) -> Result<(), TestFailure> {
        let path = self.snapshot_path(test_name);
        if !path.exists() {
            return Err(TestFailure::Pending {
                name: test_name.to_string(),
                actual: actual.to_string(),
            });
        }

        let expected = fs::read_to_string(&path)
            .map_err(|e| TestFailure::Error {
                name: test_name.to_string(),
                message: format!("Failed to read {}: {}", path.display(), e),
            })?;

        if expected.trim_end() != actual.trim_end() {
            return Err(TestFailure::Mismatch {
                name: test_name.to_string(),
                expected,
                actual: actual.to_string(),
            });
        }

        Ok(())
    }

    /// Verify the SIGNATURES footer in a snapshot text.
    ///
    /// Extracts the footer and verifies all three progressive signatures.
    pub fn verify_signatures(&self, snapshot_text: &str) -> Option<SnapshotVerification> {
        let sig = parse_snapshot_footer(snapshot_text)?;
        // Parse the snapshot to extract input, result, and comments blocks
        let (input, results, comments) = Self::parse_blocks(snapshot_text)?;
        let results_vec: Vec<String> = results;
        Some(verify_snapshot(
            &self.passphrase,
            &input,
            &results_vec,
            &comments,
            &sig,
        ))
    }

    /// Parse a snapshot text into its input, results, and comments blocks.
    fn parse_blocks(text: &str) -> Option<(String, Vec<String>, String)> {
        let lines: Vec<&str> = text.lines().collect();

        // Find INPUT: block
        let input_start = lines.iter().position(|l| l.trim() == "INPUT:")?;
        let input_fence_start = lines[input_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + input_start + 1;
        let input_fence_end = lines[input_fence_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + input_fence_start + 1;
        let input = lines[input_fence_start + 1..input_fence_end]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join("\n");

        // Find RESULT: block(s)
        let result_start = lines.iter()
            .skip(input_fence_end)
            .position(|l| l.trim().ends_with("RESULT:"))?
            + input_fence_end;
        let result_fence_start = lines[result_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + result_start + 1;
        let result_fence_end = lines[result_fence_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + result_fence_start + 1;
        let result = lines[result_fence_start + 1..result_fence_end]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join("\n");

        // Find COMMENTS: block
        let comments_start = lines.iter()
            .skip(result_fence_end)
            .position(|l| l.trim() == "COMMENTS:")?
            + result_fence_end;
        let comments_fence_start = lines[comments_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + comments_start + 1;
        let comments_fence_end = lines[comments_fence_start + 1..]
            .iter()
            .position(|l| l.trim().starts_with("```"))?
            + comments_fence_start + 1;
        let comments = lines[comments_fence_start + 1..comments_fence_end]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join("\n");

        Some((input, vec![result], comments))
    }

    /// Return sorted list of approved snapshot names.
    pub fn approved_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.approved_dir) {
            for entry in entries.flatten() {
                if let Some(fname) = entry.file_name().to_str() {
                    if let Some(stem) = fname.strip_suffix(".snap") {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        names.sort();
        names
    }

    /// Return sorted list of approved snapshot names that have no corresponding test.
    pub fn get_extraneous_snapshots(&self, registered_tests: &[&str]) -> Vec<String> {
        let mut extraneous = Vec::new();
        for name in self.approved_names() {
            if !registered_tests.contains(&name.as_str()) {
                extraneous.push(name);
            }
        }
        extraneous
    }
}

/// Build a formatted snapshot output from input, results, comments, and a passphrase.
///
/// Returns the complete snapshot text including the SIGNATURES footer.
pub fn build_snapshot(
    passphrase: &str,
    input: &str,
    results: &[String],
    comments: &str,
) -> String {
    let sig = sign_snapshot(passphrase, input, results, comments);

    let mut lines = Vec::new();
    lines.push("INPUT:".to_string());
    lines.push("```json".to_string());
    lines.push(input.trim_end().to_string());
    lines.push("```".to_string());

    for (i, result) in results.iter().enumerate() {
        lines.push(format!("[{}] RESULT:", i));
        lines.push("```json".to_string());
        lines.push(result.trim_end().to_string());
        lines.push("```".to_string());
    }

    lines.push("COMMENTS:".to_string());
    lines.push("```markdown".to_string());
    lines.push(comments.trim_end().to_string());
    lines.push("```".to_string());
    lines.push(sig.format_footer());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_snapshot_roundtrip() {
        let input = r#"{"test_name":"roundtrip","messages":["hello"]}"#;
        let results = vec![r#"{"chronon_number":1}"#.to_string()];
        let comments = "roundtrip";

        let snapshot = build_snapshot("", input, &results, comments);

        // Verify the SIGNATURES footer is present
        assert!(snapshot.contains("SIGNATURES:"));
        assert!(snapshot.contains("Public key:"));
        assert!(snapshot.contains("Input signature:"));
        assert!(snapshot.contains("Result signature:"));
        assert!(snapshot.contains("Comments signature:"));

        // Verify signatures
        let suite = SnapshotSuite::new("/tmp", "");
        let verification = suite.verify_signatures(&snapshot);
        assert!(verification.is_some(), "Footer must parse");
        let v = verification.unwrap();
        assert!(v.all_ok(), "Signatures must verify: {:?}", v);
    }

    #[test]
    fn compare_pending_when_no_snapshot() {
        let suite = SnapshotSuite::new("/tmp/nonexistent", "");
        let result = suite.compare("no_such_test", "some output");
        assert!(matches!(result, Err(TestFailure::Pending { .. })));
    }

    #[test]
    fn compare_mismatch_when_different() {
        let dir = std::env::temp_dir().join(format!("snapsuite_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let approved = dir.join("test_mismatch.snap");
        std::fs::write(&approved, "expected output").unwrap();

        let suite = SnapshotSuite::new(&dir, "");
        let result = suite.compare("test_mismatch", "actual output");
        assert!(matches!(result, Err(TestFailure::Mismatch { .. })));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_passes_when_identical() {
        let dir = std::env::temp_dir().join(format!("snapsuite_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let content = "identical output";
        let approved = dir.join("test_match.snap");
        std::fs::write(&approved, content).unwrap();

        let suite = SnapshotSuite::new(&dir, "");
        let result = suite.compare("test_match", content);
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
