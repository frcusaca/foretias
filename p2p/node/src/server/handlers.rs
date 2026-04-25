use serde_json::Value;

use crate::error::NodeError;
use crate::fortias::tick::CalendarLookup;
use crate::fortias::{Fortis, TickRecord};
use super::jsonrpc::{self, JsonRpcResponse};
use super::TimeFamilyServer;

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

    match do_stamp(server, content) {
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

fn do_stamp(server: &TimeFamilyServer, content: Vec<u8>) -> Result<Fortis, NodeError> {
    let tick = {
        let mut counter = server.current_tick.lock();
        *counter += 1;
        *counter
    };

    let tbid = server.tbid;
    let tbn = server.tbn.clone();
    let echo = format!("tick-{}", tick);

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

    server.calendar.write().append(record);
    Ok(fortis)
}

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

pub fn handle_get_calendar_slice(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let cal_tick_start = params.get("cal_tick_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    let cal = server.calendar.read();
    let records = cal.get(cal_tick_start, count)
        .unwrap_or_default();

    jsonrpc::JsonRpcResponse::success(
        params.get("id").cloned(),
        serde_json::to_value(&records).unwrap_or(Value::Null),
    )
}
