//! Snapshot tests for Foretias stamp/verify operations.
//!
//! Each test produces a cryptographically signed snapshot of its output.
//! The first test (`server_stamp_verify_fixed_seed`) is intentionally failing
//! because the C11 RNG (libsodium) cannot be externally seeded.

use std::path::PathBuf;

use foretias_core::snapshot_suite::{build_snapshot, Evaluator, SnapshotSuite};

/// Evaluator that creates a TimeFamilyServer, stamps messages, and verifies them.
pub struct ServerStampEvaluator {
    chronon_ns: u64,
    messages: Vec<String>,
}

impl ServerStampEvaluator {
    pub fn new(chronon_ns: u64, messages: Vec<String>) -> Self {
        Self {
            chronon_ns,
            messages,
        }
    }
}

impl Evaluator for ServerStampEvaluator {
    fn evaluate(&self, test_name: &str) -> Result<String, String> {
        use foretias_server::server::TimeFamilyServer;
        use std::sync::Arc;

        // Find an available port
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to bind listener: {}", e))?;
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let addr = format!("127.0.0.1:{}", port);

        // Create the server
        let server = Arc::new(
            TimeFamilyServer::new(&addr, self.chronon_ns)
                .map_err(|e| format!("Failed to create server: {}", e))?,
        );

        // Stamp each message
        let mut stamps = Vec::new();
        for msg in &self.messages {
            let stamped = server
                .chronomatter()
                .stamp(msg.as_bytes().to_vec(), msg.clone())
                .map_err(|e| format!("Stamp failed for '{}': {}", msg, e))?;
            stamps.push((
                serde_json::to_value(&stamped.foretis)
                    .map_err(|e| format!("Failed to serialize ForetisRecord: {}", e))?,
                serde_json::to_value(&stamped.signature_bytes)
                    .map_err(|e| format!("Failed to serialize signature: {}", e))?,
                serde_json::to_value(&stamped.signature_algorithm)
                    .map_err(|e| format!("Failed to serialize algorithm: {}", e))?,
            ));
        }

        // Verify each stamp
        let mut verifications = Vec::new();
        for (msg, (foretis_val, sig_val, alg_val)) in self.messages.iter().zip(stamps.iter()) {
            let foretis: foretias_core::foretias::tick::ForetisRecord =
                serde_json::from_value(foretis_val.clone()).map_err(|e| {
                    format!("Failed to deserialize ForetisRecord for verify: {}", e)
                })?;
            let sig: Vec<u8> = serde_json::from_value(sig_val.clone())
                .map_err(|e| format!("Failed to deserialize signature: {}", e))?;
            let alg: String = serde_json::from_value(alg_val.clone())
                .map_err(|e| format!("Failed to deserialize algorithm: {}", e))?;
            let result = server
                .chronomatter()
                .verify(&foretis, &sig, &alg, msg.as_bytes(), server.calendar())
                .map_err(|e| format!("Verify failed for '{}': {}", msg, e))?;
            verifications.push(serde_json::json!({
                "message": msg,
                "valid": result,
            }));
        }

        // Build input block
        let input = serde_json::json!({
            "test_name": test_name,
            "messages": self.messages,
            "server_config": {
                "chronon_ns": self.chronon_ns,
                "fixed_seed": true,
            }
        });

        // Build result block
        let result = serde_json::json!({
            "stamps": stamps,
            "verifications": verifications,
        });

        let input_str = serde_json::to_string_pretty(&input)
            .map_err(|e| format!("Failed to serialize input: {}", e))?;
        let result_str = serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Failed to serialize result: {}", e))?;

        build_snapshot("", &input_str, &[result_str], test_name).map_err(|e| e.to_string())
    }
}

// ── snapshot tests ───────────────────────────────────────────────────────────

fn suite() -> SnapshotSuite {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    SnapshotSuite::new(base.join("snapshot_tests").join("approved"), "")
}

/// First snapshot test: stamp and verify with a server.
///
/// THIS TEST IS EXPECTED TO FAIL on every run because the C11 RNG (libsodium)
/// cannot be externally seeded. Each run produces different keys, signatures,
/// and timestamps — making deterministic snapshots impossible.
///
/// The approved snapshot serves as a structural reference, not a value reference.
#[test]
#[ignore = "C11 RNG is not externally controllable — output is non-deterministic"]
fn server_stamp_verify_fixed_seed() {
    let eval = ServerStampEvaluator::new(
        100_000_000, // 100ms chronon period
        vec!["hello world".to_string(), "second message".to_string()],
    );

    let test_name = "server_stamp_verify_fixed_seed";
    let actual = eval.evaluate(test_name).expect("Evaluator must succeed");

    let suite = suite();
    match suite.compare(test_name, &actual) {
        Ok(()) => {
            // Should never happen in practice due to non-deterministic C11 RNG
            println!("Snapshot matched (unexpected)");
        }
        Err(failure) => {
            // Expected failure — print the failure and the actual output
            eprintln!("{}", failure);
            // Verify the SIGNATURES footer is valid even though values differ
            if let Some(verification) = suite.verify_signatures(&actual) {
                assert!(
                    verification.all_ok(),
                    "SIGNATURES footer must verify: {:?}",
                    verification
                );
            } else {
                panic!("SIGNATURES footer must be parseable");
            }
            // Fail the test to document the non-determinism
            panic!("{}", failure);
        }
    }
}
