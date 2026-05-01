//! EpochSnapshot — canonical record of peer scores for an epoch.

use serde::{Deserialize, Serialize};

/// A peer's probity score at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerScore {
    pub peer_id: String,
    pub score:   f32,
}

/// A signed snapshot of all peer scores for a given epoch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochSnapshot {
    pub epoch_number:     u64,
    pub epoch_start_ns:   u64,
    pub epoch_end_ns:     u64,
    /// All peer scores known to the committee at freeze time, sorted by peer_id.
    pub peer_scores:      Vec<PeerScore>,
    /// PeerIds (hex) of the committee members who contributed shares.
    pub committee:        Vec<String>,
    /// Signing threshold k (k-of-n).
    pub threshold:        u32,
    /// Aggregate FROST-Ed25519 signature over canonical_bytes().
    pub frost_signature:  Vec<u8>,
    /// FROST group public key for this committee.
    pub committee_pubkey: Vec<u8>,
}

impl EpochSnapshot {
    /// Canonical bytes for FROST signing — everything except frost_signature.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut val = serde_json::to_value(self).unwrap();
        val.as_object_mut().unwrap().remove("frost_signature");
        // Sort peer_scores by peer_id for determinism across nodes
        if let Some(arr) = val["peer_scores"].as_array_mut() {
            arr.sort_by(|a, b| {
                a["peer_id"].as_str().unwrap_or("")
                    .cmp(b["peer_id"].as_str().unwrap_or(""))
            });
        }
        serde_json::to_vec(&val).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(epoch: u64) -> EpochSnapshot {
        EpochSnapshot {
            epoch_number: epoch,
            epoch_start_ns: 1000,
            epoch_end_ns: 2000,
            peer_scores: vec![
                PeerScore { peer_id: "B".into(), score: 20.0 },
                PeerScore { peer_id: "A".into(), score: 10.0 },
            ],
            committee: vec!["C1".into(), "C2".into()],
            threshold: 2,
            frost_signature: vec![0xAA; 64],
            committee_pubkey: vec![0xBB; 32],
        }
    }

    #[test]
    fn epoch_snapshot_canonical_bytes_deterministic() {
        let s1 = make_snapshot(1);
        let s2 = make_snapshot(1);
        assert_eq!(s1.canonical_bytes(), s2.canonical_bytes());
    }

    #[test]
    fn epoch_snapshot_canonical_excludes_signature() {
        let mut s = make_snapshot(1);
        let canon1 = s.canonical_bytes();
        s.frost_signature = vec![0xFF; 64];
        let canon2 = s.canonical_bytes();
        assert_eq!(canon1, canon2);
    }

    #[test]
    fn epoch_snapshot_json_roundtrip() {
        let s = make_snapshot(42);
        let json = serde_json::to_string(&s).unwrap();
        let parsed: EpochSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.epoch_number, s.epoch_number);
        assert_eq!(parsed.epoch_start_ns, s.epoch_start_ns);
        assert_eq!(parsed.epoch_end_ns, s.epoch_end_ns);
        assert_eq!(parsed.peer_scores.len(), s.peer_scores.len());
        assert_eq!(parsed.committee, s.committee);
        assert_eq!(parsed.threshold, s.threshold);
        assert_eq!(parsed.frost_signature, s.frost_signature);
        assert_eq!(parsed.committee_pubkey, s.committee_pubkey);
    }
}
