//! PeerTransport trait — abstract P2P communication.
//!
//! Implementations handle the actual transport (JSON-RPC over TCP, libp2p, etc.).

use async_trait::async_trait;
use foretias_core::foretias::ChrononRecord;

/// Peer address for P2P communication.
#[derive(Debug, Clone)]
pub struct PeerAddr {
    /// JSON-RPC endpoint (host:port).
    pub json_rpc: String,
    /// Optional libp2p PeerId for transport-layer identity.
    pub peer_id: Option<libp2p::PeerId>,
    /// Monotonic timestamp (nanoseconds) of the last time this peer was seen.
    pub last_seen_ns: u64,
}

impl std::fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.json_rpc)
    }
}

/// Transport-level error.
#[non_exhaustive]
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
    #[error("transport unsupported: {0}")]
    Unsupported(String),
}

/// Which transport to use for a peer.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// Direct TCP JSON-RPC — "close friend" low latency.
    DirectJsonRpc,
    /// libp2p streams — general purpose (v0.5+).
    Libp2p,
}

/// Select the best transport for a peer.
pub fn select_transport(peer: &PeerAddr) -> TransportKind {
    if !peer.json_rpc.is_empty() {
        TransportKind::DirectJsonRpc
    } else {
        TransportKind::Libp2p
    }
}

/// PeerTransport — abstract P2P communication.
///
/// NOTE: The legacy `stamp` method was removed in Phase 10.
/// All stamp requests now go through `route_stamp`, which is routed
/// via CommunerdetteLine for proper Take 3 inbound gate enforcement.
#[async_trait]
pub trait PeerTransport: Send + Sync {
    async fn route_stamp(
        &self,
        peer: &PeerAddr,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError>;

    async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError>;

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError>;

    /// Send a channel_bind_challenge request (Phase 12.0).
    async fn channel_bind_challenge(
        &self,
        peer: &PeerAddr,
        nonce_hex: &str,
        channel_id: &str,
        requester_tbid_hex: &str,
    ) -> Result<serde_json::Value, TransportError>;
}
