//! libp2p direct transport — sends JSON-RPC over existing multiplexed libp2p streams.
//!
//! Uses `libp2p::request_response` to send/receive JSON-RPC 2.0 messages over
//! yamux streams instead of opening fresh TCP+Noise_XX connections.

use async_trait::async_trait;
use foretias_core::foretias::ChrononRecord;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::p2p::swarm::SwarmCommand;
use super::transport::{PeerAddr, PeerTransport, TransportError};

/// RPC transport that sends JSON-RPC over libp2p request_response streams.
pub struct Libp2pTransport {
    cmd_tx: Arc<OnceLock<tokio::sync::mpsc::UnboundedSender<SwarmCommand>>>,
    timeout_secs: u64,
}

impl Libp2pTransport {
    pub fn new(timeout_secs: u64) -> Self {
        tracing::warn!(
            component = "communerd",
            "libp2p direct transport active — uses libp2p-noise encryption (NOT C11 Noise_XX). C11 deviation acknowledged."
        );
        Self {
            cmd_tx: Arc::new(OnceLock::new()),
            timeout_secs: timeout_secs.max(1),
        }
    }

    pub fn set_cmd_tx(&self, cmd_tx: tokio::sync::mpsc::UnboundedSender<SwarmCommand>) -> bool {
        self.cmd_tx.set(cmd_tx).is_ok()
    }

    async fn rpc_call(
        &self,
        peer: &PeerAddr,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError> {
        let Some(cmd_tx) = self.cmd_tx.get() else {
            return Err(TransportError::Connect("libp2p swarm not active".into()));
        };

        let peer_id = match peer.peer_id {
            Some(id) => id,
            None => {
                return Err(TransportError::Unsupported("peer has no libp2p PeerId".into()));
            }
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let (tx, rx) = tokio::sync::oneshot::channel();
        if cmd_tx.send(SwarmCommand::RequestResponse {
            peer_id,
            request,
            reply: tx,
        }).is_err() {
            return Err(TransportError::Connect("swarm channel closed".into()));
        }

        match tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            rx
        ).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(TransportError::Connect("oneshot dropped".into())),
            Err(_) => Err(TransportError::Timeout),
        }
    }
}

#[async_trait]
impl PeerTransport for Libp2pTransport {
    async fn stamp(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        let params = serde_json::json!({
            "content": content_hex,
            "echo": echo,
        });
        self.rpc_call(peer, "stamp", params).await
    }

    async fn route_stamp(
        &self,
        peer: &PeerAddr,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        let params = serde_json::json!({
            "target_tbid": target_tbid,
            "content": content_hex,
            "echo": echo,
        });
        self.rpc_call(peer, "route_stamp", params).await
    }

    async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        let params = serde_json::json!({
            "cal_chronon_start": tick_start,
            "count": count,
        });
        let result = self.rpc_call(peer, "get_calendar_slice", params).await?;
        let records: Vec<ChrononRecord> =
            serde_json::from_value(result).map_err(|e| TransportError::Decode(e.to_string()))?;
        Ok(records)
    }

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError> {
        let _ = self.rpc_call(peer, "ping", serde_json::json!({})).await?;
        Ok(())
    }
}
