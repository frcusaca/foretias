//! Foretias Core — Rust library wrapping C11 verified core.
#![cfg_attr(debug_assertions, allow(rustdoc::all))]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod chronomatter;
pub mod clock;
pub mod collision;
pub mod config;
pub mod core;
pub mod crypto_server;
pub mod epoch;
pub mod error;
pub mod foretias;
#[cfg(test)]
mod integration_tests;
pub mod noise;
pub mod probity;
pub mod snapshot_signature;
pub mod snapshot_suite;
