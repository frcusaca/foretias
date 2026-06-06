//! Foretias Node — Server and CLI binary.
#![cfg_attr(debug_assertions, allow(rustdoc::all))]

pub mod calendar;
pub mod calendar_store;
pub mod communerd;
pub mod metrics;
pub mod probity;
pub mod replication_logger;
pub mod server;

pub use foretias_client::{
    noise_json_rpc, ClientLevel, Foretias, ForetiasError, ForetiasStatus, PtPError,
};
