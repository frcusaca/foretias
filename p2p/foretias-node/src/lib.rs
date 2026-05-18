//! Foretias Node — Server and CLI binary.
#![cfg_attr(debug_assertions, allow(rustdoc::all))]

pub mod server;
pub mod communerd;
pub mod calendar;
pub mod metrics;
pub mod calendar_store;
pub mod probity;
pub mod replication_logger;
pub mod client;
