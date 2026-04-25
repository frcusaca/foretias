//! Error types for the Fortias P2P node.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("crypto operation failed: {0}")]
    Crypto(#[from] CryptoError),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("bad format: {0}")]
    BadFormat(&'static str),
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("bad signature")]
    BadSignature,
    #[error("bad key")]
    BadKey,
    #[error("bad input: {0}")]
    BadInput(&'static str),
    #[error("operation not supported: {0}")]
    Unsupported(&'static str),
    #[error("internal crypto error: code {0}")]
    Internal(i32),
}

/// Convert C11 FortiasResult code to CryptoError.
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
