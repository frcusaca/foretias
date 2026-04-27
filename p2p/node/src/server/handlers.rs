use serde_json::Value;

use crate::error::NodeError;
use crate::fortias::tick::CalendarLookup;
use crate::fortias::{Fortis, TickRecord};
use super::jsonrpc::{self, JsonRpcResponse};
use super::TimeFamilyServer;

/// Maximum content size for stamp/verify payloads (1 GB).
const MAX_CONTENT_BYTES: usize = 1_073_741_824;
/// Maximum calendar slice count per request.
const MAX_CALENDAR_SLICE_COUNT: usize = 10_000;

/// Handles a `stamp` JSON-RPC request: creates a Fortis attestation for the given content.
pub fn handle_stamp(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            "missing or invalid 'content' (hex string)",
        ),
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            format!("invalid hex: {}", e),
        ),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            format!("content exceeds maximum size of {} bytes", MAX_CONTENT_BYTES),
        );
    }

    let echo = params.get("echo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match do_stamp(server, content, echo) {
        Ok(fortis) => jsonrpc::JsonRpcResponse::success(
            params.get("id").cloned(),
            serde_json::to_value(&fortis).unwrap_or(Value::Null),
        ),
        Err(e) => jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INTERNAL_ERROR,
            format!("stamp failed: {}", e),
        ),
    }
}

fn do_stamp(server: &TimeFamilyServer, content: Vec<u8>, echo: String) -> Result<Fortis, NodeError> {
    let tick = {
        let mut counter = server.current_tick.lock();
        *counter += 1;
        *counter
    };

    let tbid = server.tbid;
    let tbn = server.tbn.clone();

    let fortis = crate::fortias::tick::stamp(
        server.server.as_ref(),
        &tbid,
        tick,
        &content,
        &echo,
        &tbn,
    )?;

    let public_key = match server.server.public_key() {
        crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
        crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes.to_vec(),
    };

    let backward_fortis = {
        let cal = server.calendar.read();
        cal.ticks.last().map(|r| r.forward_fortis.clone()).unwrap_or_default()
    };

    let forward_fortis = serde_json::to_vec(&fortis)?;

    let record = TickRecord {
        tick_number: tick,
        public_key,
        forward_fortis,
        backward_fortis,
    };

    server.calendar.write().append(record)?;
    Ok(fortis)
}

/// Handles a `verify` JSON-RPC request: verifies a Fortis attestation against content and calendar.
pub fn handle_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            "missing 'content'",
        ),
    };

    let fortis: Fortis = match params.get("fortis").and_then(|v| serde_json::from_value(v.clone()).ok()) {
        Some(f) => f,
        None => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            "missing or invalid 'fortis'",
        ),
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            format!("invalid hex: {}", e),
        ),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            format!("content exceeds maximum size of {} bytes", MAX_CONTENT_BYTES),
        );
    }

    let valid = match crate::fortias::tick::verify(
        server.server.as_ref(),
        &fortis,
        &content,
        &*server.calendar.read(),
    ) {
        Ok(v) => v,
        Err(e) => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INTERNAL_ERROR,
            format!("verify failed: {}", e),
        ),
    };

    jsonrpc::JsonRpcResponse::success(
        params.get("id").cloned(),
        serde_json::json!({"valid": valid}),
    )
}

/// Handles a `get_calendar_slice` JSON-RPC request: returns tick records from a given starting tick.
pub fn handle_get_calendar_slice(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let cal_tick_start = params.get("cal_tick_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    if count > MAX_CALENDAR_SLICE_COUNT {
        return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INVALID_PARAMS,
            format!("count exceeds maximum of {}", MAX_CALENDAR_SLICE_COUNT),
        );
    }

    let cal = server.calendar.read();
    let records = match cal.get(cal_tick_start, count) {
        Ok(recs) => recs,
        Err(e) => return jsonrpc::JsonRpcResponse::error(
            params.get("id").cloned(),
            jsonrpc::INTERNAL_ERROR,
            format!("calendar lookup failed: {}", e),
        ),
    };

    jsonrpc::JsonRpcResponse::success(
        params.get("id").cloned(),
        serde_json::to_value(&records).unwrap_or(Value::Null),
    )
}
