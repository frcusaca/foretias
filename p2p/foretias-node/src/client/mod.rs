//! Client module — Noise PtP transport and ThinClient API.

pub mod noise_ptp;

pub use noise_ptp::{noise_json_rpc, noise_json_rpc_typed, PtPError};
