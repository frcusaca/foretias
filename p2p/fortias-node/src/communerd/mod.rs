//! Communerd — P2P transport and peer management.

pub mod transport;
pub mod json_rpc_transport;
pub mod peer_pool;

pub use transport::{PeerTransport, PeerAddr, TransportError};
pub use json_rpc_transport::JsonRpcTransport;
pub use peer_pool::PeerPool;
