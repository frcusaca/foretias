//! Type-enforced cleansing and authentication for ProbityReport.
//!
//! Three-stage type progression:
//! `UnprocessedProbityReport` (parsed, not trusted)
//! → `CleanAuthenticatedProbityReport` (authenticated + cleansed)
//! → `ExternalizedProbityReport` (wire/disk, minimal fields).

use serde::{Deserialize, Serialize};
use foretias_core::crypto_server::CryptoServer;
pub use foretias_core::foretias::clean_auth::{CleanAuthError, ParseError};
pub use foretias_core::foretias::clean_auth::{CleanAuthenticated, Unprocessed, TrustedInner};
use delegate::delegate;

use super::report::ProbityReport;

// ---------------------------------------------------------------------------
// Helper: extract Ed25519 public key from TBID hex
// ---------------------------------------------------------------------------

/// Extract the Ed25519 public key (first 32 bytes) from a TBID hex string.
///
/// For Ed25519 (curve=1), the first 64 hex characters (32 bytes) of the TBID
/// encode the Ed25519 public key.
pub fn pub_key_from_tbid_hex(pub_key_hex: &str) -> Result<Vec<u8>, CleanAuthError> {
    let tbid_bytes = hex::decode(pub_key_hex)
        .map_err(|e| CleanAuthError::InvalidLength(format!("invalid TBID hex: {e}")))?;
    if tbid_bytes.len() < 32 {
        return Err(CleanAuthError::InvalidLength(format!(
            "TBID too short for Ed25519: {} bytes (need 32)",
            tbid_bytes.len()
        )));
    }
    Ok(tbid_bytes[..32].to_vec())
}

// ---------------------------------------------------------------------------
// UnprocessedProbityReport — generic wrapper
// ---------------------------------------------------------------------------

/// A ProbityReport that has been parsed but not yet verified.
///
/// This type carries raw report data from the wire. Do NOT trust it.
#[derive(Debug, Clone)]
pub struct UnprocessedProbityReport(pub Unprocessed<ProbityReport>);

impl UnprocessedProbityReport {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let report: ProbityReport = serde_json::from_slice(b)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedProbityReport(Unprocessed::from_parsed(report)))
    }

    /// Parse from a JSON Value. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError> {
        let report: ProbityReport = serde_json::from_value(v)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedProbityReport(Unprocessed::from_parsed(report)))
    }

    /// Extract the inner report (for inspection only; still untrusted).
    pub fn inner(&self) -> &ProbityReport {
        TrustedInner::<ProbityReport>::inner(&self.0)
    }

    delegate! {
        to TrustedInner::<ProbityReport>::inner(&self.0) {
            pub fn subject(&self) -> &str;
            pub fn reporter(&self) -> &str;
            pub fn attribute(&self) -> &str;
            pub fn value(&self) -> &f32;
            pub fn timestamp_ns(&self) -> &u64;
            pub fn signature(&self) -> &Vec<u8>;
            pub fn curve(&self) -> &u8;
        }
    }
}

// ---------------------------------------------------------------------------
// CleanAuthenticatedProbityReport — generic wrapper
// ---------------------------------------------------------------------------

/// A ProbityReport that has been authenticated to the claimed reporter TBID.
///
/// The reporter's Ed25519 signature over canonical() bytes has been verified
/// against the public key extracted from the reporter's TBID hex.
///
/// **Private fields** -- zero external construction.
pub struct CleanAuthenticatedProbityReport(CleanAuthenticated<ProbityReport>);

impl CleanAuthenticatedProbityReport {
    /// Trusted construction -- only for locally-produced reports.
    pub fn from_trusted(report: ProbityReport) -> Self {
        Self(CleanAuthenticated::from_trusted(report))
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &ProbityReport {
        TrustedInner::<ProbityReport>::inner(&self.0)
    }

    /// Consume and return the inner report.
    pub fn into_inner(self) -> ProbityReport {
        TrustedInner::<ProbityReport>::into_inner(self.0)
    }

    delegate! {
        to TrustedInner::<ProbityReport>::inner(&self.0) {
            pub fn subject(&self) -> &str;
            pub fn reporter(&self) -> &str;
            pub fn attribute(&self) -> &str;
            pub fn value(&self) -> &f32;
            pub fn timestamp_ns(&self) -> &u64;
            pub fn signature(&self) -> &Vec<u8>;
            pub fn curve(&self) -> &u8;
        }
    }

    /// Strip to minimal wire form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedProbityReport {
        let r = TrustedInner::<ProbityReport>::into_inner(self.0);
        ExternalizedProbityReport {
            subject: r.subject,
            reporter: r.reporter,
            attribute: r.attribute,
            value: r.value,
            timestamp_ns: r.timestamp_ns,
            signature: r.signature,
            curve: r.curve,
        }
    }
}

// ---------------------------------------------------------------------------
// ExternalizedProbityReport — concrete wire format
// ---------------------------------------------------------------------------

/// Minimal persistent form for ProbityReport (wire/disk).
///
/// Contains only fields needed for reconstruction and re-verification.
/// No runtime artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizedProbityReport {
    pub subject:      String,
    pub reporter:     String,
    pub attribute:    String,
    pub value:        f32,
    pub timestamp_ns: u64,
    pub signature:    Vec<u8>,
    pub curve:        u8,
}

impl ExternalizedProbityReport {
    /// Reconstruct as Unprocessed for re-verification on load.
    pub fn into_unprocessed(self) -> Result<UnprocessedProbityReport, ParseError> {
        let report = ProbityReport {
            subject: self.subject,
            reporter: self.reporter,
            attribute: self.attribute,
            value: self.value,
            timestamp_ns: self.timestamp_ns,
            signature: self.signature,
            curve: self.curve,
        };
        Ok(UnprocessedProbityReport(Unprocessed::from_parsed(report)))
    }
}

// ---------------------------------------------------------------------------
// Verification gate: Unprocessed → CleanAuthenticated
// ---------------------------------------------------------------------------

impl UnprocessedProbityReport {
    /// Verify Ed25519 signature against the reporter's public key.
    ///
    /// Extracts the public key from the reporter TBID hex via `pub_key_from_tbid_hex`,
    /// then verifies the signature over canonical() bytes.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticatedProbityReport, CleanAuthError> {
        let report = self.inner();

        // Only Ed25519 is supported for probity reports
        if report.curve != 1 {
            return Err(CleanAuthError::InvalidSignature);
        }

        if report.signature.len() < 64 {
            return Err(CleanAuthError::InvalidLength(format!(
                "probity report signature too short: {} bytes (expected 64)",
                report.signature.len()
            )));
        }

        // Extract reporter's public key from TBID hex
        let public_key = pub_key_from_tbid_hex(&report.reporter)?;

        // Verify Ed25519 signature over canonical bytes
        let canonical = report.canonical();
        let valid = crypto.verify_with(
            &public_key,
            "Ed25519",
            &canonical,
            &report.signature,
        ).map_err(|e| CleanAuthError::Crypto(foretias_core::error::NodeError::Crypto(e)))?;

        if !valid {
            return Err(CleanAuthError::InvalidSignature);
        }

        Ok(CleanAuthenticatedProbityReport::from_trusted(
            TrustedInner::<ProbityReport>::into_inner(self.0)
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server;
    use foretias_core::crypto_server::ForetiasCurve;
    use foretias_core::crypto_server::SignOps;

    fn make_crypto() -> Box<dyn CryptoServer> {
        crypto_server::new_software(ForetiasCurve::Ed25519).unwrap()
    }

    fn make_signed_report(crypto: &dyn CryptoServer) -> ProbityReport {
        let pub_key = match crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid = pub_key.clone();
        tbid.resize(48, 0);
        let reporter_hex = hex::encode(&tbid);

        let mut report = ProbityReport {
            subject: "peer-A".to_string(),
            reporter: reporter_hex.clone(),
            attribute: "correctness".to_string(),
            value: 10.0,
            timestamp_ns: 1_000_000_000_000,
            signature: vec![],
            curve: 1,
        };

        let canonical = report.canonical();
        let sig = crypto.sign(&canonical).unwrap();
        report.signature = sig.bytes.to_vec();
        report
    }

    #[test]
    fn test_unprocessed_from_bytes() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let json = serde_json::to_vec(&report).unwrap();
        let up = UnprocessedProbityReport::from_bytes(&json).unwrap();
        assert_eq!(up.inner().subject, "peer-A");
    }

    #[test]
    fn test_unprocessed_from_invalid_json() {
        let result = UnprocessedProbityReport::from_bytes(b"not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_clean_authenticated_from_trusted() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let ca = CleanAuthenticatedProbityReport::from_trusted(report.clone());
        assert_eq!(ca.inner().subject, report.subject);
        assert_eq!(ca.into_inner().subject, report.subject);
    }

    #[test]
    fn test_verify_valid_signature() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let up = UnprocessedProbityReport(Unprocessed::from_parsed(report));
        let ca = up.into_clean_authenticated(crypto.as_ref()).unwrap();
        assert_eq!(ca.inner().subject, "peer-A");
    }

    #[test]
    fn test_verify_invalid_signature() {
        let crypto = make_crypto();
        let mut report = make_signed_report(crypto.as_ref());
        report.signature = vec![0xFF; 64]; // wrong signature
        let up = UnprocessedProbityReport(Unprocessed::from_parsed(report));
        let result = up.into_clean_authenticated(crypto.as_ref());
        assert!(result.is_err());
        assert!(matches!(result, Err(CleanAuthError::InvalidSignature)));
    }

    #[test]
    fn test_verify_short_signature() {
        let crypto = make_crypto();
        let mut report = make_signed_report(crypto.as_ref());
        report.signature = vec![0xFF; 10]; // too short
        let up = UnprocessedProbityReport(Unprocessed::from_parsed(report));
        let result = up.into_clean_authenticated(crypto.as_ref());
        assert!(result.is_err());
        assert!(matches!(result, Err(CleanAuthError::InvalidLength(_))));
    }

    #[test]
    fn test_externalize_roundtrip() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let ca = CleanAuthenticatedProbityReport::from_trusted(report.clone());
        let ext = ca.externalize();
        assert_eq!(ext.subject, report.subject);
        assert_eq!(ext.value, report.value);

        // Roundtrip
        let up = ext.into_unprocessed().unwrap();
        assert_eq!(up.inner().subject, report.subject);
    }

    #[test]
    fn test_pub_key_from_tbid_hex() {
        let hex_str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string() + "000011112222"; // 48 bytes worth of hex
        let pk = pub_key_from_tbid_hex(&hex_str).unwrap();
        assert_eq!(pk.len(), 32);
    }

    #[test]
    fn test_pub_key_from_tbid_hex_too_short() {
        let result = pub_key_from_tbid_hex("0123");
        assert!(result.is_err());
    }

    #[test]
    fn test_pub_key_from_tbid_hex_invalid_hex() {
        let result = pub_key_from_tbid_hex("not-hex!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_unprocessed_cannot_be_used_as_clean_authenticated() {
        // Type system enforces the distinction.
        let _up: UnprocessedProbityReport = UnprocessedProbityReport(Unprocessed::from_parsed(ProbityReport {
            subject: "A".into(),
            reporter: "B".into(),
            attribute: "correctness".into(),
            value: 0.0,
            timestamp_ns: 0,
            signature: vec![],
            curve: 1,
        }));
        // The following would NOT compile:
        // let _: CleanAuthenticatedProbityReport = _up;
    }

    // ---------------------------------------------------------------------------
    // Serialization snapshots — regression guards for wire format
    // ---------------------------------------------------------------------------

    /// Byte-exact snapshot: compare serialized JSON bytes directly.
    /// The expected string must match the declaration-order serialization of the Externalized struct.
    fn assert_snapshot_structural(json_bytes: &[u8], expected_json: &str, name: &str) {
        let actual_str = std::str::from_utf8(json_bytes)
            .unwrap_or_else(|_| panic!("failed to decode utf8 for {name} snapshot"));
        if actual_str != expected_json {
            panic!("Snapshot mismatch for {name}: wire-format serialization changed.\nactual:   {actual_str}\nexpected: {expected_json}");
        }
    }

    #[test]
    fn snapshot_probity_report_externalized() {
        let report = ProbityReport {
            subject: "peer-A".to_string(),
            reporter: "peer-123".to_string(),
            attribute: "correctness".to_string(),
            value: -10.0,
            timestamp_ns: 1_000_000_000_000,
            signature: vec![0xABu8; 64],
            curve: 1,
        };
        let ca = CleanAuthenticatedProbityReport::from_trusted(report);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(v["subject"], "peer-A");
        assert_eq!(v["reporter"], "peer-123");
        assert_eq!(v["attribute"], "correctness");
        assert_eq!(v["timestamp_ns"].as_u64().unwrap(), 1_000_000_000_000);
        assert_eq!(v["signature"].as_array().unwrap().len(), 64);
        assert_eq!(v["curve"], 1);
        assert_snapshot_structural(&json_bytes,
            r#"{"subject":"peer-A","reporter":"peer-123","attribute":"correctness","value":-10.0,"timestamp_ns":1000000000000,"signature":[171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171,171],"curve":1}"#,
            "snapshot_probity_report_externalized");
    }
}
