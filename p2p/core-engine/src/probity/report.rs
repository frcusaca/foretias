//! ProbityReport — signed reputation/timing observations gossiped across the network.
//!
//! Three-stage type progression:
//! `UnverifiedSignatureEnvelope<ProbityReport>` (parsed, not trusted)
//! → `CleanAuthenticated<ProbityReport>` (authenticated + cleansed)
//! → `ExternalizedProbityReport` (wire/disk, minimal fields).

use serde::{Deserialize, Serialize};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use crate::foretias::clean_auth::{CleanAuthenticated, CleanAuthError, UnverifiedSignatureEnvelope};

// ---------------------------------------------------------------------------
// ProbityReport domain type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbityReport {
    /// PeerId (hex) of the peer being reported on.
    pub subject:      String,
    /// PeerId (hex) of the reporting peer.
    pub reporter:     String,
    /// Opaque attribute name, e.g. "correctness", "liveness", "response_time".
    pub attribute:    String,
    /// Signed magnitude. Convention: positive = good, negative = bad.
    pub value:        f32,
    /// Observation time, nanoseconds since UNIX epoch.
    pub timestamp_ns: u64,
    /// Ed25519 or P-256 signature over canonical() bytes.
    pub signature:    Vec<u8>,
    /// 1 = Ed25519, 2 = P-256.
    pub curve:        u8,
}

impl ProbityReport {
    pub fn subject(&self) -> &str { &self.subject }
    pub fn reporter(&self) -> &str { &self.reporter }
    pub fn attribute(&self) -> &str { &self.attribute }
    pub fn value(&self) -> &f32 { &self.value }
    pub fn timestamp_ns(&self) -> &u64 { &self.timestamp_ns }
    pub fn signature(&self) -> &Vec<u8> { &self.signature }
    pub fn curve(&self) -> &u8 { &self.curve }

    /// Canonical byte representation for signing — postcard encoding,
    /// signature field excluded. Any change to this function is a wire-breaking change.
    pub fn canonical(&self) -> Vec<u8> {
        // Serialize a signature-free copy to exclude `signature` from canonical bytes.
        // postcard produces deterministic, no_std-compatible bytes.
        let mut no_sig = self.clone();
        no_sig.signature = Vec::new();
        postcard::to_allocvec(&no_sig).expect("postcard serialize ProbityReport")
    }
}

// ---------------------------------------------------------------------------
// Helper: extract Ed25519 public key from TBID hex
// ---------------------------------------------------------------------------

/// Extract the Ed25519 public key (first 32 bytes) from a TBID hex string.
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
// UnverifiedSignatureEnvelope<ProbityReport> — field accessors + verify
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<ProbityReport> {
    pub fn subject(&self) -> &str { &self.inner().subject }
    pub fn reporter(&self) -> &str { &self.inner().reporter }
    pub fn attribute(&self) -> &str { &self.inner().attribute }
    pub fn value(&self) -> &f32 { &self.inner().value }
    pub fn timestamp_ns(&self) -> &u64 { &self.inner().timestamp_ns }
    pub fn signature(&self) -> &Vec<u8> { &self.inner().signature }
    pub fn curve(&self) -> &u8 { &self.inner().curve }

    /// Inbound gate: verify Ed25519 signature against the reporter's public key.
    ///
    /// Extracts the public key from the reporter TBID hex via `pub_key_from_tbid_hex`,
    /// then verifies the signature over canonical() bytes.
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticated<ProbityReport>, CleanAuthError> {
        let report = self.inner();

        if report.curve != 1 {
            return Err(CleanAuthError::InvalidSignature);
        }
        if report.signature.len() < 64 {
            return Err(CleanAuthError::InvalidLength(format!(
                "probity report signature too short: {} bytes (expected 64)",
                report.signature.len()
            )));
        }

        let public_key = pub_key_from_tbid_hex(&report.reporter)?;
        let canonical = report.canonical();
        let valid = crypto.verify_with(
            &public_key,
            "Ed25519",
            &canonical,
            &report.signature,
        ).map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;

        if !valid {
            return Err(CleanAuthError::InvalidSignature);
        }

        Ok(CleanAuthenticated::from_trusted(self.into_inner()))
    }

    /// Ergonomic alias for `verify(crypto)`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticated<ProbityReport>, CleanAuthError> {
        self.verify(crypto)
    }
}

// ---------------------------------------------------------------------------
// CleanAuthenticated<ProbityReport> — field accessors + externalize
// ---------------------------------------------------------------------------

impl CleanAuthenticated<ProbityReport> {
    pub fn subject(&self) -> &str { &self.inner().subject }
    pub fn reporter(&self) -> &str { &self.inner().reporter }
    pub fn attribute(&self) -> &str { &self.inner().attribute }
    pub fn value(&self) -> &f32 { &self.inner().value }
    pub fn timestamp_ns(&self) -> &u64 { &self.inner().timestamp_ns }
    pub fn signature(&self) -> &Vec<u8> { &self.inner().signature }
    pub fn curve(&self) -> &u8 { &self.inner().curve }

    /// Outbound gate: strip to minimal wire form.
    pub fn externalize(self) -> ExternalizedProbityReport {
        let r = self.into_inner();
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
    /// Reconstruct as UnverifiedSignatureEnvelope for re-verification on load.
    pub fn reconstruct(self) -> Result<UnverifiedSignatureEnvelope<ProbityReport>, crate::foretias::clean_auth::ParseError> {
        let report = ProbityReport {
            subject: self.subject,
            reporter: self.reporter,
            attribute: self.attribute,
            value: self.value,
            timestamp_ns: self.timestamp_ns,
            signature: self.signature,
            curve: self.curve,
        };
        Ok(UnverifiedSignatureEnvelope::from_parsed(report))
    }

    /// Alias for `reconstruct()` — backward compat.
    pub fn into_unprocessed(self) -> Result<UnverifiedSignatureEnvelope<ProbityReport>, crate::foretias::clean_auth::ParseError> {
        self.reconstruct()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use crate::crypto_server::ForetiasCurve;

    fn make_crypto() -> Box<dyn CryptoServer> {
        crypto_server::new_software(ForetiasCurve::Ed25519).unwrap()
    }

    fn make_signed_report(crypto: &dyn CryptoServer) -> ProbityReport {
        let pub_key = match crypto.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519"),
        };
        let mut tbid = pub_key.clone();
        tbid.resize(48, 0);
        let reporter_hex = hex::encode(&tbid);

        let mut report = ProbityReport {
            subject: "peer-A".to_string(),
            reporter: reporter_hex,
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
    fn report_serde_roundtrip() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let json = serde_json::to_string(&report).unwrap();
        let parsed: ProbityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, parsed);
    }

    #[test]
    fn canonical_excludes_signature() {
        let crypto = make_crypto();
        let mut r1 = make_signed_report(crypto.as_ref());
        let mut r2 = r1.clone();
        r1.signature = vec![];
        r2.signature = vec![0xFF; 64];
        assert_eq!(r1.canonical(), r2.canonical(), "canonical must not include signature");
    }

    #[test]
    fn canonical_includes_all_other_fields() {
        let crypto = make_crypto();
        let r = make_signed_report(crypto.as_ref());
        let canon = r.canonical();
        // Verify postcard roundtrip: canonical bytes must deserialize back to a signature-free report
        let decoded: ProbityReport = postcard::from_bytes(&canon).expect("postcard deserialize");
        assert_eq!(decoded.subject, r.subject);
        assert_eq!(decoded.reporter, r.reporter);
        assert_eq!(decoded.attribute, r.attribute);
        assert_eq!(decoded.value, r.value);
        assert_eq!(decoded.timestamp_ns, r.timestamp_ns);
        assert_eq!(decoded.curve, r.curve);
        assert!(decoded.signature.is_empty(), "canonical must exclude signature");
        // Verify changing any field changes the canonical bytes
        let mut r2 = r.clone();
        r2.value = -1.0;
        assert_ne!(canon, r2.canonical(), "canonical must reflect value changes");
    }

    #[test]
    fn test_unprocessed_from_bytes() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let json = serde_json::to_vec(&report).unwrap();
        let up = UnverifiedSignatureEnvelope::<ProbityReport>::from_bytes(&json).unwrap();
        assert_eq!(up.inner().subject, "peer-A");
    }

    #[test]
    fn test_unprocessed_from_invalid_json() {
        let result = UnverifiedSignatureEnvelope::<ProbityReport>::from_bytes(b"not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_clean_authenticated_from_trusted() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let ca = CleanAuthenticated::<ProbityReport>::from_trusted(report.clone());
        assert_eq!(ca.inner().subject, report.subject);
        assert_eq!(ca.into_inner().subject, report.subject);
    }

    #[test]
    fn test_verify_valid_signature() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let up = UnverifiedSignatureEnvelope::from_parsed(report);
        let ca = up.into_clean_authenticated(crypto.as_ref()).unwrap();
        assert_eq!(ca.inner().subject, "peer-A");
    }

    #[test]
    fn test_verify_invalid_signature() {
        let crypto = make_crypto();
        let mut report = make_signed_report(crypto.as_ref());
        report.signature = vec![0xFF; 64];
        let up = UnverifiedSignatureEnvelope::from_parsed(report);
        let result = up.into_clean_authenticated(crypto.as_ref());
        assert!(result.is_err());
        assert!(matches!(result, Err(CleanAuthError::InvalidSignature)));
    }

    #[test]
    fn test_verify_short_signature() {
        let crypto = make_crypto();
        let mut report = make_signed_report(crypto.as_ref());
        report.signature = vec![0xFF; 10];
        let up = UnverifiedSignatureEnvelope::from_parsed(report);
        let result = up.into_clean_authenticated(crypto.as_ref());
        assert!(result.is_err());
        assert!(matches!(result, Err(CleanAuthError::InvalidLength(_))));
    }

    #[test]
    fn test_externalize_roundtrip() {
        let crypto = make_crypto();
        let report = make_signed_report(crypto.as_ref());
        let ca = CleanAuthenticated::<ProbityReport>::from_trusted(report.clone());
        let ext = ca.externalize();
        assert_eq!(ext.subject, report.subject);
        assert_eq!(ext.value, report.value);

        let up = ext.into_unprocessed().unwrap();
        assert_eq!(up.inner().subject, report.subject);
    }

    #[test]
    fn test_pub_key_from_tbid_hex() {
        let hex_str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string() + "000011112222";
        let pk = pub_key_from_tbid_hex(&hex_str).unwrap();
        assert_eq!(pk.len(), 32);
    }

    #[test]
    fn test_pub_key_from_tbid_hex_too_short() {
        let result = pub_key_from_tbid_hex("0123");
        assert!(result.is_err());
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
        let ca = CleanAuthenticated::<ProbityReport>::from_trusted(report);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(v["subject"], "peer-A");
        assert_eq!(v["reporter"], "peer-123");
        assert_eq!(v["attribute"], "correctness");
        assert_eq!(v["timestamp_ns"].as_u64().unwrap(), 1_000_000_000_000);
        assert_eq!(v["signature"].as_array().unwrap().len(), 64);
        assert_eq!(v["curve"], 1);
    }
}
