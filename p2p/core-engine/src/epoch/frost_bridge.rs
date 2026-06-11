//! FROST bridge — coordinates FROST signing rounds (stubbed for v0.8).

use serde::{Deserialize, Serialize};

use crate::epoch::snapshot::{EpochSnapshotRecord, PeerScore};
use crate::error::NodeError;
use crate::foretias::encoding::FTByteVector;

/// Messages exchanged during a FROST signing round.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrostMsg {
    /// First round: a peer's commitment to its nonces.
    Commitment {
        from: String,
        commitment: FTByteVector,
    },
    /// Second round: a peer's signed share of the message.
    Share { from: String, share: FTByteVector },
}

/// Run a FROST signing round for the given epoch.
///
/// For v0.8 this returns a stub snapshot with dummy signature data.
/// The actual FROST protocol will be wired in once the C11 backend is ready.
pub async fn run_frost_round(
    epoch_num: u64,
    start_ns: u64,
    end_ns: u64,
    committee: &[String],
    threshold_k: u32,
) -> Result<EpochSnapshotRecord, NodeError> {
    // Stub: return a dummy snapshot with placeholder signature.
    Ok(EpochSnapshotRecord {
        epoch_number: epoch_num,
        epoch_start_ns: start_ns,
        epoch_end_ns: end_ns,
        peer_scores: vec![],
        committee: committee.to_vec(),
        threshold: threshold_k,
        frost_signature: vec![0x00; 64].into(),
        committee_pubkey: vec![0x00; 32].into(),
    })
}

/// Stub FROST round — produces a fake snapshot for testing with peer scores.
/// Real FROST implementation deferred to v0.9+ enclave work.
pub async fn run_frost_round_stub(
    epoch_num: u64,
    start_ns: u64,
    end_ns: u64,
    peer_scores: Vec<PeerScore>,
    committee: Vec<String>,
    threshold: u32,
) -> Result<EpochSnapshotRecord, NodeError> {
    let mut scores = peer_scores;
    scores.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));

    // Stub: produce a dummy FROST signature (64 bytes of 0xAB)
    // Real implementation uses frost_ed25519.c via CryptoServer trait
    let frost_signature: FTByteVector = vec![0xAB; 64].into();
    let committee_pubkey: FTByteVector = vec![0xCD; 32].into();

    Ok(EpochSnapshotRecord {
        epoch_number: epoch_num,
        epoch_start_ns: start_ns,
        epoch_end_ns: end_ns,
        peer_scores: scores,
        committee,
        threshold,
        frost_signature,
        committee_pubkey,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_commitment_msg() -> FrostMsg {
        FrostMsg::Commitment {
            from: "peer-1".into(),
            commitment: vec![0x01; 32].into(),
        }
    }

    fn make_share_msg() -> FrostMsg {
        FrostMsg::Share {
            from: "peer-1".into(),
            share: vec![0x02; 64].into(),
        }
    }

    #[test]
    fn frost_msg_serialization() {
        let commitment = make_commitment_msg();
        let json = serde_json::to_string(&commitment).unwrap();
        let parsed: FrostMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            FrostMsg::Commitment {
                from,
                commitment: c,
            } => {
                assert_eq!(from, "peer-1");
                assert_eq!(c.len(), 32);
            }
            _ => panic!("expected Commitment variant"),
        }

        let share = make_share_msg();
        let json = serde_json::to_string(&share).unwrap();
        let parsed: FrostMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            FrostMsg::Share { from, share: s } => {
                assert_eq!(from, "peer-1");
                assert_eq!(s.len(), 64);
            }
            _ => panic!("expected Share variant"),
        }
    }

    #[tokio::test]
    async fn frost_round_stub_produces_valid_snapshot() {
        let scores = vec![
            PeerScore::new("peer-b".into(), 90.0),
            PeerScore::new("peer-a".into(), 80.0),
        ];
        let snap = run_frost_round_stub(
            5,
            1_000,
            2_000,
            scores,
            vec!["peer-a".into(), "peer-b".into()],
            2,
        )
        .await
        .unwrap();

        assert_eq!(snap.epoch_number, 5);
        assert_eq!(snap.epoch_start_ns, 1_000);
        assert_eq!(snap.epoch_end_ns, 2_000);
        assert_eq!(snap.threshold, 2);
        assert_eq!(snap.committee.len(), 2);
        assert_eq!(snap.frost_signature.len(), 64);
        assert_eq!(snap.committee_pubkey.len(), 32);
        assert_eq!(snap.peer_scores.len(), 2);
    }

    #[tokio::test]
    async fn frost_round_stub_peer_scores_sorted() {
        let scores = vec![
            PeerScore::new("zzz".into(), 10.0),
            PeerScore::new("aaa".into(), 99.0),
            PeerScore::new("mmm".into(), 50.0),
        ];
        let snap = run_frost_round_stub(
            1,
            0,
            1_000,
            scores,
            vec!["aaa".into(), "mmm".into(), "zzz".into()],
            2,
        )
        .await
        .unwrap();

        let ids: Vec<&str> = snap
            .peer_scores
            .iter()
            .map(|s| s.peer_id.as_str())
            .collect();
        assert_eq!(ids, vec!["aaa", "mmm", "zzz"]);
    }
}
