//! Foretias Core — Rust library wrapping C11 verified core.
#![cfg_attr(debug_assertions, allow(rustdoc::all))]

#![allow(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod config;
pub mod core;
pub mod crypto_server;
pub mod foretias;
pub mod chronomatter;
pub mod collision;
pub mod epoch;
pub mod probity;
pub mod noise;
#[cfg(test)]
mod integration_tests;
