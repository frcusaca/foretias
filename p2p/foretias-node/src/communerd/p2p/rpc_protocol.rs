//! libp2p request_response protocol for Foretias JSON-RPC over multiplexed streams.
//!
//! Implements `libp2p::request_response::Protocol` so the swarm can send/receive
//! JSON-RPC 2.0 requests over existing yamux/mplex streams.

use libp2p::request_response::{Protocol, Protocols};
use std::time::Duration;

/// Foretias RPC protocol over libp2p request_response.
///
/// Protocol name: `/foretias/{namespace}/rpc/1.0.0`
/// Payload: raw JSON-RPC 2.0 bytes (serde_json handles serialization).
pub struct ForetiasRpcProtocol {
    /// Namespace derived protocol list, e.g. `/foretias/mainnet/rpc/1.0.0`
    protocols: Protocols,
    /// Request timeout (default 5s).
    timeout: Duration,
}

impl ForetiasRpcProtocol {
    pub fn new(namespace: &str) -> Self {
        let protocol_string = format!("/foretias/{}/rpc/1.0.0", namespace);
        Self {
            protocols: Protocols::new(protocol_string),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_timeout(namespace: &str, timeout: Duration) -> Self {
        let protocol_string = format!("/foretias/{}/rpc/1.0.0", namespace);
        Self {
            protocols: Protocols::new(protocol_string),
            timeout,
        }
    }
}

impl Protocol for ForetiasRpcProtocol {
    type Protocol = Self;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    fn inbound_protocol(&self) -> Self {
        Self {
            protocols: self.protocols.clone(),
            timeout: self.timeout,
        }
    }

    fn outbound_protocol(&self) -> Self {
        Self {
            protocols: self.protocols.clone(),
            timeout: self.timeout,
        }
    }

    fn inbound_timeout(&self) -> Duration {
        self.timeout
    }

    fn outbound_timeout(&self) -> Duration {
        self.timeout
    }
}

impl Default for ForetiasRpcProtocol {
    fn default() -> Self {
        Self::new("mainnet")
    }
}
