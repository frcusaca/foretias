//! External attestation types for cross-node auto attestation.

use bon::Builder;
use serde::{Deserialize, Serialize};

use super::clean_auth::BaseRecord;
use super::encoding::FTByteVector;
use super::tick::ChrononRecord;
use crate::error::NodeError;

/// An external attestation from another Time Family.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ExternalAttestationRecord {
    /// Hex-encoded TBID of the attesting peer.
    pub(crate) attester_tbid: String,
    /// B's stamp of A's tick record (signature-free payload).
    pub(crate) foretis: super::tick::ForetisRecord,
    /// v2: Signature covering postcard-encoded ForetisRecord payload (base64-encoded in JSON).
    #[serde(default)]
    #[builder(default)]
    pub(crate) signature: FTByteVector,
    /// v2: Signature algorithm identifier (e.g. "Ed25519").
    #[serde(default = "default_sig_algorithm")]
    #[builder(default = "Ed25519".to_string())]
    pub(crate) signature_algorithm: String,
    /// B's tick at attestation time (for offline re-verify).
    pub(crate) attester_tick_record: ChrononRecord,
    /// Wall-clock receive time in nanoseconds.
    pub(crate) received_at_ns: u64,
}

impl ExternalAttestationRecord {
    /// Hex-encoded TBID of the attesting peer.
    pub fn attester_tbid(&self) -> &str {
        &self.attester_tbid
    }
    /// B's stamp of A's tick record (signature-free payload).
    pub fn foretis(&self) -> &super::tick::ForetisRecord {
        &self.foretis
    }
    /// Signature covering postcard-encoded ForetisRecord payload.
    pub fn signature(&self) -> &FTByteVector {
        &self.signature
    }
    /// Signature algorithm identifier (e.g. "Ed25519").
    pub fn signature_algorithm(&self) -> &str {
        &self.signature_algorithm
    }
    /// B's tick at attestation time (for offline re-verify).
    pub fn attester_tick_record(&self) -> &ChrononRecord {
        &self.attester_tick_record
    }
    /// Wall-clock receive time in nanoseconds.
    pub fn received_at_ns(&self) -> &u64 {
        &self.received_at_ns
    }
}

/// Fallible builder for ExternalAttestationRecord — validates invariants before construction.
impl<S: external_attestation_record_builder::IsComplete> ExternalAttestationRecordBuilder<S> {
    /// Build a validated ExternalAttestationRecord.
    ///
    /// # Errors
    /// Returns `NodeError::InvalidInput` if the foretis chronon_number is 0.
    pub fn build(self) -> Result<ExternalAttestationRecord, NodeError> {
        let record = self.build_internal();
        if record.foretis.chronon_number == 0 {
            return Err(NodeError::InvalidInput(
                "foretis.chronon_number must be > 0".into(),
            ));
        }
        Ok(record)
    }
}

impl BaseRecord for ExternalAttestationRecord {
    fn always_require_full_signature(&self) -> bool {
        false
    }
}

fn default_sig_algorithm() -> String {
    "Ed25519".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foretias::tick::ChrononRecord;
    use crate::foretias::types::Tbid;

    fn make_dummy_tick() -> ChrononRecord {
        ChrononRecord {
            chronon_number: 42,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        }
    }

    #[test]
    fn external_attestation_serde_roundtrip() {
        let foretis = crate::foretias::tick::ForetisRecord {
            chronon_number: 42,
            content_hash: [1u8; 32].into(),
            tbid: crate::foretias::types::Tbid::from_raw([3u8; 96]),
            echo: "test".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+123ns".to_string(),
        };

        let att = ExternalAttestationRecord {
            attester_tbid: "ab".to_string(),
            foretis,
            signature: FTByteVector::from(vec![2u8; 64]),
            signature_algorithm: "Ed25519".to_string(),
            attester_tick_record: make_dummy_tick(),
            received_at_ns: 1_000_000,
        };

        let json = serde_json::to_string(&att).unwrap();
        let _parsed: ExternalAttestationRecord = serde_json::from_str(&json).unwrap();
    }
}
