//! Client module — Noise PtP transport and ThinClient API.

pub mod noise_ptp;
pub mod thin_client;

pub use noise_ptp::{noise_json_rpc, noise_json_rpc_typed, PtPError};
pub use thin_client::{ThinClient, ClientLevel, ThinClientError, ThinClientStatus, P2pJoinConfig, VerificationReport};
