//! ProbityReport — signed reputation/timing observations gossiped across the network.

use serde::{Deserialize, Serialize};

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
    /// Canonical byte representation for signing — fixed field order,
    /// no signature field. Any change to this function is a wire-breaking change.
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.subject.as_bytes());   buf.push(0);
        buf.extend_from_slice(self.reporter.as_bytes());  buf.push(0);
        buf.extend_from_slice(self.attribute.as_bytes()); buf.push(0);
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.push(self.curve);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report() -> ProbityReport {
        ProbityReport {
            subject: "peer-abc".to_string(),
            reporter: "peer-123".to_string(),
            attribute: "correctness".to_string(),
            value: -10.0,
            timestamp_ns: 1_000_000_000_000,
            signature: vec![0xAB; 64],
            curve: 1,
        }
    }

    #[test]
    fn report_serde_roundtrip() {
        let report = make_report();
        let json = serde_json::to_string(&report).unwrap();
        let parsed: ProbityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, parsed);
    }

    #[test]
    fn canonical_excludes_signature() {
        let r1 = make_report();
        let mut r2 = make_report();
        let canon1 = r1.canonical();
        r2.signature = vec![0xFF; 64];
        let canon2 = r2.canonical();
        assert_eq!(canon1, canon2, "canonical must not include signature");
    }

    #[test]
    fn canonical_includes_all_other_fields() {
        let r = make_report();
        let canon = r.canonical();
        // subject\0reporter\0attribute\0value(4)timestamp(8)curve(1)
        let expected_len = r.subject.len() + 1 + r.reporter.len() + 1 + r.attribute.len() + 1 + 4 + 8 + 1;
        assert_eq!(canon.len(), expected_len);
    }
}
