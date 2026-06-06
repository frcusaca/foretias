//! GossipSub topic construction and publish helpers for probity reports and heartbeats.

use super::behaviour::ForetiasBehaviour;
use crate::probity::ProbityReportRecord;
use foretias_core::collision::Heartbeat;
use foretias_core::error::NodeError;
use libp2p::gossipsub::IdentTopic;

/// Probity gossip topic — per-namespace isolation.
pub fn probity_topic(namespace: &str) -> IdentTopic {
    IdentTopic::new(format!("/foretias/{}/probity/v1", namespace))
}

/// Heartbeat topic — per-namespace isolation for collision detection.
pub fn heartbeat_topic(namespace: &str) -> IdentTopic {
    IdentTopic::new(format!("/foretias/{}/heartbeat/v1", namespace))
}

/// Publish a probity report to the gossip network.
pub fn publish_probity_report(
    swarm: &mut libp2p::Swarm<ForetiasBehaviour>,
    report: &ProbityReportRecord,
    namespace: &str,
) -> Result<(), NodeError> {
    let bytes = serde_json::to_vec(report)
        .map_err(|e| NodeError::Internal(format!("ProbityReportRecord serialization: {e}")))?;
    swarm
        .behaviour_mut()
        .gossip
        .publish(probity_topic(namespace), bytes)
        .map_err(|e| NodeError::Internal(format!("gossip publish: {e}")))?;
    Ok(())
}

/// Publish a signed heartbeat to the gossip network.
pub fn publish_heartbeat(
    swarm: &mut libp2p::Swarm<ForetiasBehaviour>,
    hb: &Heartbeat,
    namespace: &str,
) -> Result<(), NodeError> {
    let bytes = serde_json::to_vec(hb)
        .map_err(|e| NodeError::Internal(format!("Heartbeat serialization: {e}")))?;
    swarm
        .behaviour_mut()
        .gossip
        .publish(heartbeat_topic(namespace), bytes)
        .map_err(|e| NodeError::Internal(format!("gossip publish heartbeat: {e}")))?;
    Ok(())
}
