//! Safe Rust wrappers over C11 FFI.

pub mod bindings;

// Re-export from generated bindings
pub use bindings::*;
