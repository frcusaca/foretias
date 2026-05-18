//! C11 Noise_XX PtP client — JSON-RPC over encrypted TCP.
//!
//! This module provides a library-scoped Noise_XX client that can be used
//! by the CLI (main.rs) and the ThinClient (Level 2 — PtP Networked).
//!
//! Protocol: TCP → Noise_XX handshake → JSON-RPC 2.0 over encrypted channel.
//! Frame format: 4-byte LE length prefix + ciphertext.

use std::time::Duration;
use tokio::io::{AsyncWriteExt, AsyncReadExt};

use foretias_core::core::identity::generate_ed25519_keypair;
use foretias_core::noise;

/// Errors specific to PtP Noise transport operations.
#[derive(Debug)]
pub enum PtPError {
    Connect(String),
    Timeout,
    Noise(String),
    Decode(String),
    Rpc { code: i32, message: String },
}

impl std::fmt::Display for PtPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtPError::Connect(msg) => write!(f, "PtP connect error: {}", msg),
            PtPError::Timeout => write!(f, "PtP request timeout"),
            PtPError::Noise(msg) => write!(f, "PtP noise error: {}", msg),
            PtPError::Decode(msg) => write!(f, "PtP decode error: {}", msg),
            PtPError::Rpc { code, message } => write!(f, "PtP RPC error {}: {}", code, message),
        }
    }
}

impl std::error::Error for PtPError {}

impl From<foretias_core::error::CryptoError> for PtPError {
    fn from(e: foretias_core::error::CryptoError) -> Self {
        PtPError::Noise(e.to_string())
    }
}

/// Perform a single JSON-RPC 2.0 request over C11 Noise_XX encrypted TCP.
///
/// This is the core PtP client function used by both the CLI and ThinClient.
/// Each call creates a fresh TCP connection, performs a Noise_XX handshake,
/// sends the request, and reads the response.
pub async fn noise_json_rpc(
    server: &str,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, PtPError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });
    let request_bytes = serde_json::to_string(&request)
        .map_err(|e| PtPError::Decode(e.to_string()))?
        .into_bytes();

    let result = tokio::time::timeout(timeout, async {
        let stream = tokio::net::TcpStream::connect(server)
            .await
            .map_err(|e| PtPError::Connect(e.to_string()))?;

        let (_pub_key, priv_key) = generate_ed25519_keypair()
            .map_err(|e| PtPError::Connect(e.to_string()))?;

        let (mut session, stream) = noise::noise_handshake(stream, &priv_key.bytes, None, true)
            .await
            .map_err(|e| PtPError::Noise(e.to_string()))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(reader);

        // Encrypt and send request with length-prefix framing
        let ct = session.send(&request_bytes)
            .map_err(|e| PtPError::Noise(e.to_string()))?;
        let ct_len = (ct.len() as u32).to_le_bytes();
        writer.write_all(&ct_len).await
            .map_err(|e| PtPError::Connect(e.to_string()))?;
        writer.write_all(&ct).await
            .map_err(|e| PtPError::Connect(e.to_string()))?;
        writer.flush().await
            .map_err(|e| PtPError::Connect(e.to_string()))?;

        // Read response with length-prefix framing and decrypt
        let mut len_buf = [0u8; 4];
        AsyncReadExt::read_exact(&mut reader, &mut len_buf).await
            .map_err(|e| PtPError::Connect(e.to_string()))?;
        let resp_len = u32::from_le_bytes(len_buf) as usize;
        let mut resp_buf = vec![0u8; resp_len];
        AsyncReadExt::read_exact(&mut reader, &mut resp_buf).await
            .map_err(|e| PtPError::Connect(e.to_string()))?;

        let plaintext = session.recv(&resp_buf)
            .map_err(|e| PtPError::Decode(e.to_string()))?;
        let response: serde_json::Value = serde_json::from_slice(&plaintext)
            .map_err(|e| PtPError::Decode(e.to_string()))?;

        // Validate JSON-RPC 2.0 response structure
        validate_jsonrpc_response(&response)
            .map_err(PtPError::Decode)?;

        if let Some(err) = response.get("error") {
            let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
            let message = err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string();
            return Err(PtPError::Rpc { code, message });
        }

        response.get("result")
            .cloned()
            .ok_or_else(|| PtPError::Decode("missing result in response".to_string()))
    })
    .await;

    match result {
        Ok(inner_result) => inner_result,
        Err(_) => Err(PtPError::Timeout),
    }
}

/// Typed variant — deserializes the JSON-RPC result into a Rust type.
pub async fn noise_json_rpc_typed<T: serde::de::DeserializeOwned>(
    server: &str,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<T, PtPError> {
    let result = noise_json_rpc(server, method, params, timeout).await?;
    serde_json::from_value(result)
        .map_err(|e| PtPError::Decode(e.to_string()))
}

/// Validate JSON-RPC 2.0 response schema.
fn validate_jsonrpc_response(response: &serde_json::Value) -> Result<(), String> {
    if !response.is_object() {
        return Err("invalid JSON-RPC response: not an object".into());
    }
    match response.get("jsonrpc").and_then(|v| v.as_str()) {
        Some("2.0") => (),
        Some(other) => return Err(format!("invalid JSON-RPC response: unexpected version {other:?}")),
        None => return Err("invalid JSON-RPC response: missing jsonrpc field".into()),
    }
    if response.get("result").is_none() && response.get("error").is_none() {
        return Err("invalid JSON-RPC response: missing both result and error".into());
    }
    Ok(())
}
