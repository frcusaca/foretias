//! Probity module — reputation tracking via signed gossip.

pub mod aggregator;
pub mod clean_auth;
pub mod gossip_handler;
pub mod report;
pub mod store;

pub use aggregator::{aggregate, u_shape_weight, UShapeConfig};
pub use clean_auth::{pub_key_from_tbid_hex, CleanAuthError};
pub use gossip_handler::{handle_gossip_message, DefaultReporterKeyResolver, ReporterKeyResolver};
pub use report::ProbityReport;
pub use store::ProbityStore;

use crate::calendar_store::StorageProofResult;

// ---------------------------------------------------------------------------
// Storage proof attribute names
// ---------------------------------------------------------------------------

/// All block proofs verified successfully.
pub const STORAGE_VERIFIED: &str = "storage_verified";

/// Partial coverage — some ticks missing but some proofs valid.
pub const STORAGE_PARTIAL: &str = "storage_partial";

/// Complete failure — no coverage or all proofs invalid.
pub const STORAGE_FAILED: &str = "storage_failed";

// ---------------------------------------------------------------------------
// Storage proof → ProbityReport conversion
// ---------------------------------------------------------------------------

/// Convert a [`StorageProofResult`] into a single [`ProbityReport`].
///
/// Attribute selection and value:
/// - `verified == true` → `STORAGE_VERIFIED`, value `+1.0`
/// - `verified == false && coverage_ratio == 0.0` → `STORAGE_FAILED`, value `-1.0`
/// - `verified == false && 0.0 < coverage_ratio < 1.0` → `STORAGE_PARTIAL`, value `-0.5 * (1.0 - coverage_ratio)`
///
/// The returned report has `signature` and `slow_signature` empty — the caller
/// is responsible for signing before gossip.
pub fn storage_proof_to_probity(
    result: &StorageProofResult,
    subject: &str,
    reporter: &str,
    timestamp_ns: u64,
) -> ProbityReport {
    let (attribute, value) = if result.verified {
        (STORAGE_VERIFIED, 1.0f32)
    } else if result.coverage_ratio <= 0.0 {
        (STORAGE_FAILED, -1.0f32)
    } else {
        let penalty = -0.5 * (1.0 - result.coverage_ratio as f32);
        (STORAGE_PARTIAL, penalty)
    };

    ProbityReport {
        subject: subject.to_string(),
        reporter: reporter.to_string(),
        attribute: attribute.to_string(),
        value,
        timestamp_ns,
        signature: vec![],
        curve: 1,
        slow_signature: vec![],
    }
}

#[cfg(test)]
mod storage_proof_tests {
    use super::*;

    #[test]
    fn verified_result_maps_to_storage_verified() {
        let result = StorageProofResult {
            verified: true,
            coverage_ratio: 1.0,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 1_000_000);
        assert_eq!(report.attribute, STORAGE_VERIFIED);
        assert_eq!(report.value, 1.0);
        assert_eq!(report.subject, "peer-A");
        assert_eq!(report.reporter, "peer-B");
        assert_eq!(report.timestamp_ns, 1_000_000);
        assert!(report.signature.is_empty());
        assert_eq!(report.curve, 1);
    }

    #[test]
    fn failed_result_maps_to_storage_failed() {
        let result = StorageProofResult {
            verified: false,
            coverage_ratio: 0.0,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 2_000_000);
        assert_eq!(report.attribute, STORAGE_FAILED);
        assert_eq!(report.value, -1.0);
    }

    #[test]
    fn partial_result_maps_to_storage_partial() {
        let result = StorageProofResult {
            verified: false,
            coverage_ratio: 0.6,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 3_000_000);
        assert_eq!(report.attribute, STORAGE_PARTIAL);
        // -0.5 * (1.0 - 0.6) = -0.2
        assert!((report.value - (-0.2)).abs() < 1e-6);
    }

    #[test]
    fn partial_near_full_coverage() {
        let result = StorageProofResult {
            verified: false,
            coverage_ratio: 0.99,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 4_000_000);
        assert_eq!(report.attribute, STORAGE_PARTIAL);
        // -0.5 * (1.0 - 0.99) = -0.005
        assert!((report.value - (-0.005)).abs() < 1e-6);
    }

    #[test]
    fn partial_near_zero_coverage() {
        // coverage_ratio is > 0 but tiny — still STORAGE_PARTIAL, not FAILED
        let result = StorageProofResult {
            verified: false,
            coverage_ratio: 0.01,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 5_000_000);
        assert_eq!(report.attribute, STORAGE_PARTIAL);
        // -0.5 * (1.0 - 0.01) = -0.495
        assert!((report.value - (-0.495)).abs() < 1e-6);
    }

    #[test]
    fn verified_with_partial_coverage_still_verified() {
        // Edge case: verified=true but coverage < 1.0 (merkle proofs valid for what exists)
        let result = StorageProofResult {
            verified: true,
            coverage_ratio: 0.8,
        };
        let report = storage_proof_to_probity(&result, "peer-A", "peer-B", 6_000_000);
        assert_eq!(report.attribute, STORAGE_VERIFIED);
        assert_eq!(report.value, 1.0);
    }
}
