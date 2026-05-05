//! Intra-family callback traits.
//!
//! Chronomatter, Calendar, and Communerd communicate via direct method calls
//! and short-lived callbacks. No channels, no message passing, no serialization.

use crate::error::NodeError;
use crate::foretias::tick::{Foretis, TickRecord};
use crate::foretias::types::{Message, TickNumber};

pub trait TickObserver: Send + Sync {
    fn on_tick_advance(&self, tick_number: TickNumber, public_key: &[u8], tick_record: &TickRecord);
}

pub trait Attester: Send + Sync {
    fn stamp(&self, content: Message, echo: String) -> Result<Foretis, NodeError>;
    fn verify(&self, foretis: &Foretis, content: &Message) -> Result<bool, NodeError>;
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
    fn send_to_peer(&self, addr: &PeerAddr, method: &str, params: serde_json::Value)
        -> Result<serde_json::Value, TransportError>;
    /// Query community state.
    fn query_community(&self, query: CommunityQuery)
        -> Result<CommunityResponse, TransportError>;
}

/// Observer for auto-attestation lifecycle events.
///
/// Called by Chronomatter before/after each auto-attestation attempt
/// so metrics can be recorded.
pub trait AutoAttestObserver: Send + Sync {
    fn on_auto_attest_sent(&self);
    fn on_auto_attest_ok(&self);
    fn on_auto_attest_failed(&self);
}
