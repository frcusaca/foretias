//! Error types for the Fortias P2P node.

use thiserror::Error;

/// Top-level errors produced by the P2P node.
#[derive(Debug, Error)]
pub enum NodeError {
    /// A cryptographic operation failed; wraps a [`CryptoError`].
    #[error("crypto operation failed: {0}")]
    Crypto(#[from] CryptoError),
    /// A requested resource (tick, record, etc.) was not found.
    #[error("not found: {0}")]
    NotFound(&'static str),
    /// Data did not match the expected format.
    #[error("bad format: {0}")]
    BadFormat(String),
    /// The requested feature is not yet implemented.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// An I/O operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// An unexpected internal error with a descriptive message.
    #[error("internal error: {0}")]
    Internal(String),
    /// The server is in dormant (verify-only) mode and cannot perform the requested operation.
    #[error("dormant: {0}")]
    Dormant(String),
    /// Chain integrity check failed at a specific tick number.
    #[error("integrity check failed at tick {0}")]
    IntegrityFailure(u64),
    /// Transport connection failed.
    #[error("transport connect: {0}")]
    TransportConnect(String),
    /// Transport request timed out.
    #[error("transport timeout")]
    TransportTimeout,
    /// Transport response decode failed.
    #[error("transport decode: {0}")]
    TransportDecode(String),
    /// External attestation verification failed.
    #[error("attestation verification failed: {0}")]
    AttestationVerificationFailed(String),
    /// The outbound message queue is full; the request cannot be enqueued.
    #[error("queue full")]
    QueueFull,
    /// No more evictable entries available in the LRU cache.
    #[error("out of space: no unused entries to evict")]
    OutOfSpace,
    /// A message or report is stale or future-dated.
    #[error("stale: {0}")]
    Stale(String),
    /// The requested feature or curve is not supported.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

/// Errors originating from the cryptographic backend.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// A signature verification failed.
    #[error("bad signature")]
    BadSignature,
    /// The provided key is invalid or malformed.
    #[error("bad key")]
    BadKey,
    /// Input data to a crypto operation was invalid.
    #[error("bad input: {0}")]
    BadInput(&'static str),
    /// The requested crypto operation is not supported by this backend.
    #[error("operation not supported: {0}")]
    Unsupported(&'static str),
    /// An internal error from the underlying crypto library, identified by error code.
    #[error("internal crypto error: code {0}")]
    Internal(i32),
}

/// Convert C11 ForetiasResult code to CryptoError.
pub fn c_result_to_error(code: i32) -> Result<(), CryptoError> {
    match code {
        0 => Ok(()),
        -1 => Err(CryptoError::BadSignature),
        -3 => Err(CryptoError::BadKey),
        -6 => Err(CryptoError::BadInput("invalid input")),
        -8 => Err(CryptoError::Unsupported("not implemented")),
        other => Err(CryptoError::Internal(other)),
    }
}
