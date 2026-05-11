//! Snapshot adoption handler — verifies and adopts epoch snapshots (stubbed for v0.8).

use crate::epoch::scheduler::EpochScheduler;
use crate::epoch::snapshot::EpochSnapshot;
use crate::error::NodeError;

/// Handle an incoming epoch snapshot from the network.
///
/// Validates that the snapshot is not stale and deserializes it correctly.
/// FROST signature verification is stubbed (deferred to v0.9).
pub fn handle_epoch_snapshot(
    data: &[u8],
    scheduler: &EpochScheduler,
) -> Result<EpochSnapshot, NodeError> {
    let snapshot: EpochSnapshot = serde_json::from_slice(data)
        .map_err(|_| NodeError::BadFormat("EpochSnapshot deserialization".to_string()))?;

    let current = scheduler.current_epoch_number();
    if snapshot.epoch_number != current {
        return Err(NodeError::Stale(format!(
            "epoch snapshot {} not for current epoch {}",
            snapshot.epoch_number, current
        )));
    }

    // Stub verification: FROST signature must be non-empty (64 bytes for Ed25519)
    // TODO v0.9: verify frost_signature against committee_pubkey using FROST-Ed25519
    if snapshot.frost_signature.is_empty() {
        return Err(NodeError::BadFormat("epoch snapshot has empty FROST signature".to_string()));
    }
    if snapshot.frost_signature.len() != 64 {
        return Err(NodeError::BadFormat(format!(
            "epoch snapshot FROST signature wrong size: {} bytes (expected 64)",
            snapshot.frost_signature.len()
        )));
    }

    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scheduler() -> EpochScheduler {
        EpochScheduler::new(1_000_000)
    }

    fn make_snapshot_json(epoch: u64) -> String {
        serde_json::to_string(&EpochSnapshot {
            epoch_number: epoch,
            epoch_start_ns: 1_000,
            epoch_end_ns: 2_000,
            peer_scores: vec![],
            committee: vec!["peer-1".into()],
            threshold: 2,
            frost_signature: vec![0xAB; 64].into(),
            committee_pubkey: vec![0xCD; 32].into(),
        }).unwrap()
    }

    #[test]
    fn handle_epoch_snapshot_rejects_bad_json() {
        let scheduler = make_scheduler();
        let result = handle_epoch_snapshot(b"not json", &scheduler);
        assert!(result.is_err());
    }

    #[test]
    fn handle_epoch_snapshot_rejects_stale() {
        let scheduler = make_scheduler();
        scheduler.next_epoch_number();
        scheduler.next_epoch_number();
        let data = make_snapshot_json(0);
        let result = handle_epoch_snapshot(data.as_bytes(), &scheduler);
        assert!(result.is_err());
    }

    #[test]
    fn handle_epoch_snapshot_accepts_current() {
        let scheduler = make_scheduler();
        let current = scheduler.current_epoch_number();
        let data = make_snapshot_json(current);
        let result = handle_epoch_snapshot(data.as_bytes(), &scheduler);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().epoch_number, current);
    }
}
