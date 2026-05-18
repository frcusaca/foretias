//! External attestation types for cross-node auto attestation.

use serde::{Deserialize, Serialize};

use super::tick::ChrononRecord;

/// An external attestation from another Time Family.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAttestation {
    /// Hex-encoded TBID of the attesting peer.
    pub attester_tbid: String,
    /// B's stamp of A's tick record.
    pub foretis: super::tick::Foretis,
    /// B's tick at attestation time (for offline re-verify).
    pub attester_tick_record: ChrononRecord,
    /// Wall-clock receive time in nanoseconds.
    pub received_at_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foretias::tick::ChrononRecord;

    fn make_dummy_tick() -> ChrononRecord {
        ChrononRecord {
            chronon_number: 42,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        }
    }

    #[test]
    fn external_attestation_serde_roundtrip() {
        let foretis = crate::foretias::tick::Foretis {
            chronon_number: 42,
            content_hash: [1u8; 32].into(),
            signature: vec![2u8; 64].into(),
            signature_algorithm: "Ed25519".to_string(),
            tbid: crate::foretias::types::Tbid::from_raw([3u8; 96]),
            echo: "test".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+123ns".to_string(),
        };

        let att = ExternalAttestation {
            attester_tbid: "ab".to_string(),
            foretis,
            attester_tick_record: make_dummy_tick(),
            received_at_ns: 1_000_000,
        };

        let json = serde_json::to_string(&att).unwrap();
        let _parsed: ExternalAttestation = serde_json::from_str(&json).unwrap();
    }
}
