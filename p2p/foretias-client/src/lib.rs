//! Foretias Client — library for stamping and verification without a server daemon.
//!
//! Supports three capability levels:
//! - Level 1 (Standalone): In-memory Chronomatter, no network
//! - Level 2 (PtP Networked): Outbound PtP connections via C11 Noise_XX
//! - Level 3 (P2P Full): P2P mesh participation (Phase 10)

pub mod calendar;
pub mod noise_ptp;
pub mod foretias;
pub mod config;
pub mod communerd_reader;

pub use noise_ptp::{noise_json_rpc, noise_json_rpc_typed, PtPError};
pub use foretias::{Foretias, ForetiasError, ForetiasStatus, ForetiasInner, ClientLevel, P2pJoinConfig, VerificationReport};
pub use config::{StandaloneConfig, PtpConfig, P2pConfig, ForetiasConfig};
pub use communerd_reader::CommunerdReader;
