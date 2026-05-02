//! GossipSub message handler — deserialize, validate, and ingest probity reports.

use foretias_core::error::NodeError;
use foretias_core::crypto_server::CryptoServer;

use super::report::ProbityReport;
use super::store::ProbityStore;

pub fn handle_gossip_message(
    data:         &[u8],
    store:        &ProbityStore,
    _crypto:      &dyn CryptoServer,
    now_ns:       u64,
) -> Result<(), NodeError> {
    let report: ProbityReport = serde_json::from_slice(data)
        .map_err(|_| NodeError::BadFormat("ProbityReport deserialization".to_string()))?;

    // 1. Reject future-dated reports (> 5 min clock skew tolerance)
    if report.timestamp_ns > now_ns + 5 * 60 * 1_000_000_000 {
        return Err(NodeError::Stale("future-dated probity report".to_string()));
    }

    // 2. Reject reports from peers with score < -50 (too dishonourable to report)
    if store.score(&report.reporter) < -50.0 {
        return Ok(());
    }

    // 3. Verify reporter signature over canonical bytes
    verify_report_signature(&report, _crypto)?;

    // 4. Ingest
    store.ingest(report)?;
    Ok(())
}

fn verify_report_signature(report: &ProbityReport, _crypto: &dyn CryptoServer)
    -> Result<(), NodeError>
{
    if report.curve != 1 {
        return Err(NodeError::Unsupported("only Ed25519 probity signatures in v0.6"));
    }
    // Stub verification: signature must be non-empty (64 bytes for Ed25519)
    // TODO v0.9: resolve reporter public key and verify Ed25519 signature
    if report.signature.len() < 64 {
        return Err(NodeError::BadFormat(format!(
            "probity report signature too short: {} bytes (expected 64)",
            report.signature.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server;
    use foretias_core::crypto_server::ForetiasCurve;

    fn make_crypto() -> Box<dyn CryptoServer> {
        crypto_server::new_software(ForetiasCurve::Ed25519).unwrap()
    }

    fn make_report(subject: &str, reporter: &str, ts: u64) -> ProbityReport {
        ProbityReport {
            subject: subject.to_string(),
            reporter: reporter.to_string(),
            attribute: "correctness".to_string(),
            value: -10.0,
            timestamp_ns: ts,
            signature: vec![0xAB; 64],
            curve: 1,
        }
    }

    #[test]
    fn gossip_handler_rejects_future_dated() {
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;
        let report = make_report("A", "B", now + 10 * 60 * 1_000_000_000);
        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);
        assert!(result.is_err());
    }

    #[test]
    fn gossip_handler_rejects_dishonourable_reporter() {
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;

        // First make reporter "B" have a score < -50.
        // Reporters need positive scores themselves for their credibility to carry weight
        // in pass 2, so we need reporters that receive good reports.
        // Q vouches for each R → R has positive pass-1 score → positive pass-2 credibility
        for i in 0..10 {
            store.ingest(ProbityReport {
                subject: format!("R{}", i),
                reporter: "Q".to_string(),
                attribute: "correctness".to_string(),
                value: 80.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
            }).unwrap();
        }
        // Each R reports badly on B
        for i in 0..10 {
            store.ingest(ProbityReport {
                subject: "B".to_string(),
                reporter: format!("R{}", i),
                attribute: "correctness".to_string(),
                value: -10.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
            }).unwrap();
        }
        store.recompute_all(now);
        assert!(store.score("B") < -40.0, "B score: {}", store.score("B"));

        // Now B tries to report on someone
        let report = make_report("A", "B", now - 1_000_000);
        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);
        assert!(result.is_ok());
        assert_eq!(store.report_count("A"), 0, "report should not be ingested");
    }

    #[test]
    fn gossip_handler_accepts_valid_report() {
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;
        let report = make_report("A", "B", now - 1_000_000);
        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);
        assert!(result.is_ok());
        assert_eq!(store.report_count("A"), 1);
    }

    #[test]
    fn gossip_handler_rejects_invalid_json() {
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let result = handle_gossip_message(b"not json", &store, crypto.as_ref(), 1_000_000);
        assert!(result.is_err());
    }
}
