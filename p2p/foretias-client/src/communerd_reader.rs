//! CommunerdReader — outbound-only PtP communication tier.
//!
//! Capabilities:
//! - Outbound PtP connections via Noise_XX
//! - No listening port, no incoming connections

use std::time::Duration;

use crate::noise_ptp::{noise_json_rpc, PtPError};

/// Outbound-only PtP communicator.
///
/// Can initiate connections to known peers but never accepts incoming requests.
pub struct CommunerdReader {
    peers: Vec<String>,
    timeout: Duration,
}

impl CommunerdReader {
    /// Create a new outbound-only reader with known peer addresses.
    pub fn new(peers: Vec<String>, timeout_secs: u64) -> Self {
        Self {
            peers,
            timeout: Duration::from_secs(timeout_secs.max(1)),
        }
    }

    /// Stamp content on a remote peer.
    pub async fn stamp(
        &self,
        peer_idx: usize,
        content: &[u8],
        echo: &str,
    ) -> Result<serde_json::Value, PtPError> {
        let peer = self
            .peers
            .get(peer_idx)
            .ok_or_else(|| PtPError::Connect("peer index out of range".into()))?;
        let params = serde_json::json!({
            "content": hex::encode(content),
            "echo": echo,
        });
        noise_json_rpc(peer, "stamp", params, self.timeout).await
    }

    /// Verify content on a remote peer.
    pub async fn verify(
        &self,
        peer_idx: usize,
        content: &[u8],
        foretis: &serde_json::Value,
    ) -> Result<serde_json::Value, PtPError> {
        let peer = self
            .peers
            .get(peer_idx)
            .ok_or_else(|| PtPError::Connect("peer index out of range".into()))?;
        let params = serde_json::json!({
            "content": hex::encode(content),
            "foretis": foretis,
        });
        noise_json_rpc(peer, "verify", params, self.timeout).await
    }

    /// Fetch calendar slice from a remote peer.
    pub async fn calendar_slice(
        &self,
        peer_idx: usize,
        start: u64,
        count: u64,
    ) -> Result<serde_json::Value, PtPError> {
        let peer = self
            .peers
            .get(peer_idx)
            .ok_or_else(|| PtPError::Connect("peer index out of range".into()))?;
        let params = serde_json::json!({
            "cal_chronon_start": start,
            "count": count,
        });
        noise_json_rpc(peer, "get_calendar_slice", params, self.timeout).await
    }

    /// Get the list of configured peers.
    pub fn peers(&self) -> &[String] {
        &self.peers
    }

    /// Get the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_creation() {
        let reader = CommunerdReader::new(vec!["127.0.0.1:4001".into()], 30);
        assert_eq!(reader.peers().len(), 1);
        assert_eq!(reader.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn reader_minimum_timeout() {
        let reader = CommunerdReader::new(vec![], 0);
        assert_eq!(reader.timeout(), Duration::from_secs(1));
    }
}
