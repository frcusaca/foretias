//! Fortias P2P — Rust node layer wrapping C11 verified core.

#![warn(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod config;
pub mod core;
pub mod crypto_server;
pub mod fortias;
/// TimeFamilyServer — JSON-RPC 2.0 over TCP.
pub mod server;

/// PyO3 Python bindings (requires `python` feature).
#[cfg(feature = "python")]
pub mod api;
