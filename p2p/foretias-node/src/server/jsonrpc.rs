//! JSON-RPC 2.0 protocol types for TimeFamilyServer.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    #[serde(default)]
    pub id: Option<Value>,
    /// Whether this TimeFamily is currently dormant (not ticking).
    #[serde(default)]
    pub dormant: bool,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
pub const DORMANT_ERROR: i32 = -32001;

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
            dormant: false,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
            dormant: false,
        }
    }

    pub fn dormant_success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
            dormant: true,
        }
    }

    pub fn dormant_error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
            dormant: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_response_has_correct_structure() {
        let resp = JsonRpcResponse::success(
            Some(Value::Number(serde_json::Number::from(1))),
            serde_json::json!({"key": "value"}),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.id, Some(Value::Number(serde_json::Number::from(1))));
        assert!(!resp.dormant);
    }

    #[test]
    fn error_response_has_correct_structure() {
        let resp = JsonRpcResponse::error(
            Some(Value::Number(serde_json::Number::from(42))),
            INVALID_PARAMS,
            "bad request",
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.id, Some(Value::Number(serde_json::Number::from(42))));
        assert!(!resp.dormant);
    }

    #[test]
    fn error_response_contains_error_code_and_message() {
        let resp = JsonRpcResponse::error(
            None,
            INTERNAL_ERROR,
            "something went wrong",
        );
        let err = resp.error.unwrap();
        assert_eq!(err.code, INTERNAL_ERROR);
        assert_eq!(err.message, "something went wrong");
    }

    #[test]
    fn success_response_serializes_and_deserializes() {
        let resp = JsonRpcResponse::success(
            Some(Value::String("req-1".to_string())),
            serde_json::json!({"data": [1, 2, 3]}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["result"]["data"].as_array().unwrap().len(), 3);
        assert_eq!(parsed["id"], "req-1");
    }

    #[test]
    fn error_response_serializes_cleanly() {
        let resp = JsonRpcResponse::error(
            Some(Value::Number(serde_json::Number::from(7))),
            METHOD_NOT_FOUND,
            "no such method",
        );
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("result").is_none());
        assert_eq!(parsed["error"]["code"], -32601);
        assert_eq!(parsed["error"]["message"], "no such method");
    }

    #[test]
    fn dormant_flag_preserved_in_response() {
        let resp = JsonRpcResponse::success(None, serde_json::json!(null));
        assert!(!resp.dormant);

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get("dormant").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn request_deserialization_roundtrip() {
        let json = r#"{"jsonrpc":"2.0","method":"stamp","params":{"content":"abc"},"id":"test-123"}"#;
        let parsed: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.method, "stamp");
        assert_eq!(parsed.id, Some(Value::String("test-123".to_string())));
    }

    #[test]
    fn request_with_missing_fields_deserializes_with_defaults() {
        let json = r#"{"jsonrpc": "2.0", "method": "ping"}"#;
        let parsed: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.jsonrpc, "2.0");
        assert_eq!(parsed.method, "ping");
        assert!(parsed.params.is_null());
        assert!(parsed.id.is_none());
    }

    #[test]
    fn jsonrpc_error_codes_are_correct() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
        assert_eq!(DORMANT_ERROR, -32001);
    }
}
