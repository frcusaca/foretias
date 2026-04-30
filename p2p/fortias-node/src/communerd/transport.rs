//! PeerTransport trait — abstract P2P communication.
//!
//! Implementations handle the actual transport (JSON-RPC over TCP, libp2p, etc.).

use async_trait::async_trait;
use fortias_core::fortias::TickRecord;

/// Peer address for P2P communication.
#[derive(Debug, Clone)]
pub struct PeerAddr {
    /// JSON-RPC endpoint (host:port).
    pub json_rpc: String,
    /// Optional libp2p PeerId for transport-layer identity.
    pub peer_id: Option<libp2p::PeerId>,
}

impl std::fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.json_rpc)
    }
}

/// Transport-level error.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connect failed: {0}")]
    Connect(String),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i32, message: String },
    #[error("timeout")]
    Timeout,
    #[error("decode error: {0}")]
    Decode(String),
}

/// PeerTransport — abstract P2P communication.
#[async_trait]
pub trait PeerTransport: Send + Sync {
    async fn stamp(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError>;

    async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<TickRecord>, TransportError>;

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;
}
