//! Foretias Client — library for stamping and verification without a server daemon.
//!
//! Supports three capability levels:
//! - Level 1 (Standalone): In-memory Chronomatter, no network
//! - Level 2 (PtP Networked): Outbound PtP connections via C11 Noise_XX
//! - Level 3 (P2P Full): P2P mesh participation (Phase 10)

pub mod calendar;
pub mod communerd_reader;
pub mod config;
pub mod foretias;
pub mod noise_ptp;

pub use communerd_reader::CommunerdReader;
pub use config::{ForetiasConfig, P2pConfig, PtpConfig, StandaloneConfig};
pub use foretias::{
    ClientLevel, Foretias, ForetiasError, ForetiasInner, ForetiasStatus, P2pJoinConfig,
    VerificationReport,
};
pub use noise_ptp::{noise_json_rpc, noise_json_rpc_typed, PtPError};
