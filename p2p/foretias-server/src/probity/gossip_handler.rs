//! GossipSub message handler — deserialize, validate, and ingest probity reports.
//!
//! FB/GNF reports require full-signature gate (CleanFullyAuthenticated).
//! Other reports use fast-signature path (CleanAuthenticated).

use foretias_core::error::NodeError;
use foretias_core::crypto_server::CryptoServer;
use foretias_core::foretias::clean_auth::{CleanAuthError, UnverifiedSignatureEnvelope};

use super::report::ProbityReport;
use super::store::ProbityStore;

/// Trait for resolving a reporter's Ed25519 public key from their TBID hex.
pub trait ReporterKeyResolver {
    fn resolve_public_key(&self, reporter_tbid_hex: &str) -> Result<Vec<u8>, NodeError>;
}

/// Default implementation: extracts the first 32 bytes from the TBID hex.
/// For Ed25519, the first 64 hex chars (32 bytes) of the TBID are the public key.
pub struct DefaultReporterKeyResolver;

impl ReporterKeyResolver for DefaultReporterKeyResolver {
    fn resolve_public_key(&self, reporter_tbid_hex: &str) -> Result<Vec<u8>, NodeError> {
        let tbid_bytes = hex::decode(reporter_tbid_hex)
            .map_err(|e| NodeError::BadFormat(format!("invalid reporter TBID hex: {}", e)))?;
        if tbid_bytes.len() < 32 {
            return Err(NodeError::BadFormat(format!(
                "reporter TBID too short for Ed25519: {} bytes (need 32)",
                tbid_bytes.len()
            )));
        }
        Ok(tbid_bytes[..32].to_vec())
    }
}

pub fn handle_gossip_message(
    data:         &[u8],
    store:        &ProbityStore,
    crypto:       &dyn CryptoServer,
    now_ns:       u64,
) -> Result<(), NodeError> {
    // Parse into unverified envelope
    let envelope = UnverifiedSignatureEnvelope::<ProbityReport>::from_bytes(data)
        .map_err(|e| {
            tracing::trace!("gossip parse failed: {e}");
            NodeError::BadFormat(format!("ProbityReport deserialization: {e}"))
        })?;

    let report = envelope.inner();

    // 1. Reject future-dated reports (> 5 min clock skew tolerance)
    if report.timestamp_ns > now_ns + 5 * 60 * 1_000_000_000 {
        return Err(NodeError::Stale("future-dated probity report".to_string()));
    }

    // 2. Reject reports from peers with score < -50 (too dishonourable to report)
    if store.score(&report.reporter) < -50.0 {
        return Ok(());
    }

    // Clone attribute/reporter before consuming envelope
    let attribute = report.attribute.clone();
    let reporter = report.reporter.clone();

    // 3. Route through appropriate verification gate
    let authenticated = if attribute == "fb" || attribute == "gnf" {
        // FB/GNF: full-signature gate → CleanFullyAuthenticated
        match envelope.verify_full(crypto) {
            Ok(cfa) => cfa.into(),
            Err(CleanAuthError::FullSignatureRequired) => {
                tracing::trace!(
                    attribute = %attribute,
                    reporter = %reporter,
                    "FB/GNF report dropped: full signature required"
                );
                return Ok(());
            }
            Err(e) => {
                tracing::trace!(
                    attribute = %attribute,
                    error = %e,
                    "FB/GNF report dropped: verification failed"
                );
                return Ok(());
            }
        }
    } else {
        // Other reports: fast-signature gate → CleanAuthenticated
        match envelope.verify(crypto) {
            Ok(ca) => ca,
            Err(e) => {
                tracing::trace!(
                    attribute = %attribute,
                    error = %e,
                    "probity report dropped: verification failed"
                );
                return Ok(());
            }
        }
    };

    // 4. Ingest
    store.ingest(authenticated.into_inner())?;
    Ok(())
}

fn verify_report_signature(report: &ProbityReport, crypto: &dyn CryptoServer)
    -> Result<(), NodeError>
{
    if report.curve != 1 {
        return Err(NodeError::Unsupported("only Ed25519 probity signatures in v0.6"));
    }
    if report.signature.len() < 64 {
        return Err(NodeError::BadFormat(format!(
            "probity report signature too short: {} bytes (expected 64)",
            report.signature.len()
        )));
    }
    
    // Get the reporter's public key from their TBID.
    // The TBID hex encodes the identity. For Ed25519 (curve=1),
    // the first 64 hex chars (32 bytes) of the TBID are the Ed25519 public key.
    let tbid_bytes = match hex::decode(&report.reporter) {
        Ok(b) => b,
        Err(e) => return Err(NodeError::BadFormat(format!("invalid reporter TBID hex: {}", e))),
    };
    
    if tbid_bytes.len() < 32 {
        return Err(NodeError::BadFormat(format!(
            "reporter TBID too short for Ed25519: {} bytes (need 32)",
            tbid_bytes.len()
        )));
    }
    
    let public_key = tbid_bytes[..32].to_vec();
    
    // Verify Ed25519 signature over canonical bytes
    let canonical = report.canonical();
    let valid = crypto.verify_with(
        &public_key,
        "Ed25519",
        &canonical,
        &report.signature,
    ).map_err(|e| NodeError::Crypto(e))?;
    
    if !valid {
        return Err(NodeError::BadFormat(format!(
            "probity report signature verification failed for reporter {}",
            report.reporter
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
            slow_signature: vec![],
        }
    }

    fn make_signed_report(crypto: &dyn CryptoServer, subject: &str, ts: u64) -> ProbityReport {
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid = pub_key.clone();
        tbid.resize(48, 0);
        let reporter_hex = hex::encode(&tbid);

        let mut report = ProbityReport {
            subject: subject.to_string(),
            reporter: reporter_hex.clone(),
            attribute: "correctness".to_string(),
            value: -10.0,
            timestamp_ns: ts,
            signature: vec![],
            curve: 1,
            slow_signature: vec![],
        };

        let canonical = report.canonical();
        let sig = crypto.sign(&canonical).unwrap();
        report.signature = sig.bytes.to_vec();
        report
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
        // Q vouches for each R -> R has positive pass-1 score -> positive pass-2 credibility
        for i in 0..10 {
            store.ingest(ProbityReport {
                subject: format!("R{}", i),
                reporter: "Q".to_string(),
                attribute: "correctness".to_string(),
                value: 80.0,
                timestamp_ns: now - 1_000_000,
                signature: vec![],
                curve: 1,
            slow_signature: vec![],
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
            slow_signature: vec![],
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
        let report = make_signed_report(crypto.as_ref(), "A", now - 1_000_000);
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

    #[test]
    fn gossip_handler_rejects_fb_fast_only() {
        // FB report with only Ed25519 signature (no slow_signature) must be rejected
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;

        // Create a signed FB report (fast signature only)
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid = pub_key.clone();
        tbid.resize(48, 0);
        let reporter_hex = hex::encode(&tbid);

        let mut report = ProbityReport {
            subject: "A".to_string(),
            reporter: reporter_hex,
            attribute: "fb".to_string(), // FB attribute requires full signature
            value: 1.0,
            timestamp_ns: now - 1_000_000,
            signature: vec![],
            curve: 1,
            slow_signature: vec![], // Empty — should trigger FullSignatureRequired
        };

        let canonical = report.canonical();
        let sig = crypto.sign(&canonical).unwrap();
        report.signature = sig.bytes.to_vec();

        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);

        // Must be dropped (Ok) — FB requires full signature, fast-only is silently discarded
        assert!(result.is_ok(), "FB report with fast-only signature must be dropped (Ok), not errored");
        assert_eq!(store.report_count("A"), 0, "report must not be ingested");
    }

    #[test]
    fn gossip_handler_rejects_gnf_fast_only() {
        // GNF report with only Ed25519 signature must be rejected
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;

        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid = pub_key.clone();
        tbid.resize(48, 0);
        let reporter_hex = hex::encode(&tbid);

        let mut report = ProbityReport {
            subject: "A".to_string(),
            reporter: reporter_hex,
            attribute: "gnf".to_string(), // GNF attribute requires full signature
            value: 1.0,
            timestamp_ns: now - 1_000_000,
            signature: vec![],
            curve: 1,
            slow_signature: vec![], // Empty — should trigger FullSignatureRequired
        };

        let canonical = report.canonical();
        let sig = crypto.sign(&canonical).unwrap();
        report.signature = sig.bytes.to_vec();

        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);

        assert!(result.is_ok(), "GNF report with fast-only signature must be dropped (Ok), not errored");
        assert_eq!(store.report_count("A"), 0, "report must not be ingested");
    }

    #[test]
    fn gossip_handler_accepts_non_fb_fast_only() {
        // Non-FB/GNF report with only Ed25519 signature must be accepted
        let store = ProbityStore::new();
        let crypto = make_crypto();
        let now = 1_000_000_000_000;

        let report = make_signed_report(crypto.as_ref(), "A", now - 1_000_000);
        assert_ne!(report.attribute, "fb", "test helper should not produce FB");
        assert_ne!(report.attribute, "gnf", "test helper should not produce GNF");

        let data = serde_json::to_vec(&report).unwrap();
        let result = handle_gossip_message(&data, &store, crypto.as_ref(), now);

        assert!(result.is_ok(), "non-FB report with fast-only should be accepted");
        assert_eq!(store.report_count("A"), 1);
    }
}
