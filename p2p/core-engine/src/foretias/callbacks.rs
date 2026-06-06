//! Intra-family callback traits.
//!
//! Chronomatter, Calendar, and Communerd communicate via direct method calls
//! and short-lived callbacks. No channels, no message passing, no serialization.

use crate::error::NodeError;
use crate::foretias::tick::{ChrononRecord, ForetisRecord};
use crate::foretias::types::{Message, TickNumber};

pub trait TickObserver: Send + Sync {
    fn on_tick_advance(
        &self,
        chronon_number: TickNumber,
        public_key: &[u8],
        tick_record: &ChrononRecord,
    );
}

pub trait Attester: Send + Sync {
    fn stamp(&self, content: Message, echo: String) -> Result<ForetisRecord, NodeError>;
    fn verify(&self, foretis: &ForetisRecord, content: &Message) -> Result<bool, NodeError>;
}

/// Peer address for extra-family communication.
#[derive(Debug, Clone)]
pub struct PeerAddr {
    /// JSON-RPC endpoint address.
    pub json_rpc: String,
}

/// Query types for community state.
#[derive(Debug, Clone)]
pub enum CommunityQuery {
    /// Get known peers.
    KnownPeers,
    /// Get peer by address.
    PeerByAddr(PeerAddr),
}

/// Response types for community queries.
#[derive(Debug, Clone)]
pub enum CommunityResponse {
    /// List of known peer addresses.
    KnownPeers(Vec<PeerAddr>),
    /// Peer status.
    PeerStatus { peer: PeerAddr, alive: bool },
}

/// Transport-level error.
#[derive(Debug)]
pub enum TransportError {
    /// Connection failed.
    Connect(String),
    /// RPC error from peer.
    Rpc { code: i32, message: String },
    /// Request timed out.
    Timeout,
    /// Response decode failed.
    Decode(String),
}

/// Community query interface — Communerd implements this.
/// Calendar calls this for all extra-family communication.
pub trait PeerMessenger: Send + Sync {
    /// Send a JSON-RPC call to a peer and receive the result.
    fn send_to_peer(
        &self,
        addr: &PeerAddr,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError>;
    /// Query community state.
    fn query_community(&self, query: CommunityQuery) -> Result<CommunityResponse, TransportError>;
}

/// Observer for mutual-attestation lifecycle events.
///
/// Called by Chronomatter before/after each mutual-attestation attempt
/// so metrics can be recorded.
pub trait MutualAttestObserver: Send + Sync {
    fn on_mutual_attest_sent(&self);
    fn on_mutual_attest_ok(&self);
    fn on_mutual_attest_failed(&self);
}

/// Peer-pool change notification, fired by Communerd when its peer pool
/// changes (peer added, removed, or refreshed). Calendar implements this
/// to drive Group 4b active-mirroring decisions (when to look for a new
/// mirror, when to expire an unreachable one, etc.).
///
/// Implementations must be cheap (no blocking I/O); push work into the
/// Calendar task queue rather than performing it inline.
pub trait PeerChangeCallback: Send + Sync {
    /// Called with the full current peer-pool snapshot. The Vec is freshly
    /// constructed on each call; the callee may move or retain entries.
    fn on_peer_change(&self, peers: Vec<PeerAddr>);
}
