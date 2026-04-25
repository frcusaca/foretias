//! JSON-RPC 2.0 protocol types for TimeFamilyServer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version, should be `"2.0"`.
    pub jsonrpc: String,
    /// The method name to invoke.
    pub method: String,
    /// Method parameters as a JSON value.
    #[serde(default)]
    pub params: Value,
    /// Request identifier echoed back in the response.
    #[serde(default)]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version, always `"2.0"`.
    pub jsonrpc: String,
    /// Result value, present on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error object, present when the request failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    /// Echoes the request identifier.
    #[serde(default)]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code per JSON-RPC 2.0 specification.
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
}

/// JSON-RPC 2.0 error codes.
pub const PARSE_ERROR: i32 = -32700;
/// The JSON sent is not a valid Request object.
pub const INVALID_REQUEST: i32 = -32600;
/// The method does not exist or is not available.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// Invalid method parameter(s).
pub const INVALID_PARAMS: i32 = -32602;
/// Internal JSON-RPC error.
pub const INTERNAL_ERROR: i32 = -32603;

impl JsonRpcResponse {
    /// Constructs a successful response with the given result value.
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Constructs an error response with the given code and message.
    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}
