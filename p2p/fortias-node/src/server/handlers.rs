use serde_json::Value;

use fortias_core::error::NodeError;
use fortias_core::fortias::tick::CalendarLookup;
use fortias_core::fortias::{auto_attestation_blob, Fortis, TickRecord};
use super::jsonrpc::{self, JsonRpcResponse};
use super::TimeFamilyServer;

/// Maximum content size for stamp/verify payloads (1 GB).
const MAX_CONTENT_BYTES: usize = 1_073_741_824;
/// Maximum calendar slice count per request.
const MAX_CALENDAR_SLICE_COUNT: usize = 10_000;

fn resp_success(server: &TimeFamilyServer, id: Option<Value>, result: Value) -> JsonRpcResponse {
    if server.is_dormant() {
        jsonrpc::JsonRpcResponse::dormant_success(id, result)
    } else {
        jsonrpc::JsonRpcResponse::success(id, result)
    }
}

fn resp_error(server: &TimeFamilyServer, id: Option<Value>, code: i32, message: String) -> JsonRpcResponse {
    if server.is_dormant() {
        jsonrpc::JsonRpcResponse::dormant_error(id, code, message)
    } else {
        jsonrpc::JsonRpcResponse::error(id, code, message)
    }
}

/// Handles a `stamp` JSON-RPC request: creates a Fortis attestation for the given content.
pub fn handle_stamp(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing or invalid 'content' (hex string)".into()),
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("invalid hex: {}", e)),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("content exceeds maximum size of {} bytes", MAX_CONTENT_BYTES));
    }

    if server.is_dormant() {
        return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            "Cannot stamp: server is in verify-only mode".into());
    }

    let echo = params.get("echo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match do_stamp(server, content, echo) {
        Ok(fortis) => {
            if let Err(e) = server.save() {
                tracing::warn!("failed to persist calendar after stamp: {}", e);
            }
            resp_success(server, id, serde_json::to_value(&fortis).unwrap_or(Value::Null))
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("stamp failed: {}", e)),
    }
}

pub fn do_stamp(server: &TimeFamilyServer, content: Vec<u8>, echo: String) -> Result<Fortis, NodeError> {
    let tick = {
        let mut counter = server.current_tick.lock();
        *counter += 1;
        *counter
    };

    let tbid = server.tbid;
    let tbn = server.tbn.clone();
    let tbid_str = hex::encode(tbid);

    // Generate new keypair for this tick
    let kp_idx = server.generate_and_store_keypair()?;
    let new_pub = server.keypair_pub(kp_idx).unwrap();

    // Build auto-attestation (forward_fortis / backward_fortis)
    let (forward_fortis, backward_fortis, aa_nonce) = if server.calendar.read().ticks.is_empty() {
        // Genesis: self-signed — both sides use the same keypair
        let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, tick, &new_pub, tick, &new_pub)?;
        let sig = server.sign_with_keypair(kp_idx, &ma_blob)?;
        (sig.clone(), sig, nonce)
    } else {
        // Non-genesis: auto-attested between previous tick and current tick
        let cal = server.calendar.read();
        let latest_rec = cal.ticks.last().unwrap();
        let prev_tick = latest_rec.tick_number;
        let prev_kp_idx = (prev_tick - 1) as usize;
        let prev_pub = server.keypair_pub(prev_kp_idx).unwrap();

        let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, prev_tick, &prev_pub, tick, &new_pub)?;

        let forward_sig = server.sign_with_keypair(prev_kp_idx, &ma_blob)?;
        let backward_sig = server.sign_with_keypair(kp_idx, &ma_blob)?;
        (forward_sig, backward_sig, nonce)
    };

    // Sign the content with this tick's keypair
    let mut sig_input = Vec::with_capacity(16 + 8 + content.len());
    sig_input.extend_from_slice(&tbid);
    sig_input.extend_from_slice(&tick.to_be_bytes());
    sig_input.extend_from_slice(&content);

    let sig = server.keypairs.read().get(kp_idx).unwrap().priv_key
        .sign(&sig_input)
        .map_err(|e| NodeError::Crypto(e))?;

    let content_hash = server.server.sha256(&content)?;

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| NodeError::Internal(format!("SystemTime before UNIX_EPOCH: {}", e)))?
        .as_nanos() as u64;
    let time_being_reference_time = format!("UE+{}ns", now_ns);

    let fortis = Fortis {
        tick_number: tick,
        content_hash: content_hash.bytes,
        signature: sig.bytes.to_vec(),
        tbid,
        echo,
        tbn,
        time_being_reference_time,
    };

    let record = TickRecord {
        tick_number: tick,
        public_key: new_pub.to_vec(),
        forward_fortis,
        backward_fortis,
        aa_nonce,
        external_attestations: Vec::new(),
    };

    server.calendar.write().append(record)?;
    Ok(fortis)
}

/// Handles a `verify` JSON-RPC request: verifies a Fortis attestation against content and calendar.
pub fn handle_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'content'".into()),
    };

    let fortis: Fortis = match params.get("fortis").and_then(|v| serde_json::from_value(v.clone()).ok()) {
        Some(f) => f,
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing or invalid 'fortis'".into()),
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("invalid hex: {}", e)),
    };

    if content.len() > MAX_CONTENT_BYTES {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("content exceeds maximum size of {} bytes", MAX_CONTENT_BYTES));
    }

    let valid = match fortias_core::fortias::tick::verify(
        server.server.as_ref(),
        &fortis,
        &content,
        &*server.calendar.read(),
    ) {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("verify failed: {}", e)),
    };

    resp_success(server, id, serde_json::json!({"valid": valid}))
}

/// Handles a `get_calendar_slice` JSON-RPC request: returns tick records from a given starting tick.
pub fn handle_get_calendar_slice(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let cal_tick_start = params.get("cal_tick_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    if count > MAX_CALENDAR_SLICE_COUNT {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("count exceeds maximum of {}", MAX_CALENDAR_SLICE_COUNT));
    }

    let cal = server.calendar.read();
    let records = match cal.get(cal_tick_start, count) {
        Ok(recs) => recs,
        Err(e) => return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("calendar lookup failed: {}", e)),
    };

    resp_success(server, id, serde_json::to_value(&records).unwrap_or(Value::Null))
}

/// Handles an `integrity_check` JSON-RPC request: verifies chain integrity over a tick range.
pub fn handle_integrity_check(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let start = params.get("start").and_then(|v| v.as_u64());
    let end = params.get("end").and_then(|v| v.as_u64());

    match do_integrity_check(server, start, end) {
        Ok(result) => resp_success(server, id, result),
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("integrity check failed: {}", e)),
    }
}

pub fn do_integrity_check(
    server: &TimeFamilyServer,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<Value, NodeError> {
    let results = server.calendar.read().integrity_check(
        server.server.as_ref(),
        &hex::encode(server.tbid),
        start,
        end,
    )?;

    let all_valid = results.iter().all(|&v| v);
    Ok(serde_json::json!({
        "all_valid": all_valid,
        "pair_results": results,
        "pairs_checked": results.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_content_bytes_is_1gb() {
        assert_eq!(MAX_CONTENT_BYTES, 1_073_741_824);
    }

    #[test]
    fn max_calendar_slice_count_is_10k() {
        assert_eq!(MAX_CALENDAR_SLICE_COUNT, 10_000);
    }

    fn make_server() -> TimeFamilyServer {
        TimeFamilyServer::new("127.0.0.1:0", 1_000_000_000)
            .expect("failed to create server")
    }

    #[test]
    fn handle_stamp_missing_content_returns_error() {
        let server = make_server();
        let params = serde_json::json!({});
        let resp = handle_stamp(&server, params);
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
    }

    #[test]
    fn handle_stamp_invalid_hex_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"content": "not-hex"});
        let resp = handle_stamp(&server, params);
        assert!(resp.error.is_some());
    }

    #[test]
    fn handle_stamp_valid_content_returns_fortis() {
        let server = make_server();
        let params = serde_json::json!({
            "content": hex::encode(b"hello"),
            "echo": "test-stamp"
        });
        let resp = handle_stamp(&server, params);
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert!(result.get("tick_number").is_some());
        assert!(result.get("content_hash").is_some());
        assert!(result.get("signature").is_some());
    }

    #[test]
    fn handle_stamp_empty_content_succeeds() {
        let server = make_server();
        let params = serde_json::json!({
            "content": hex::encode(b""),
        });
        let resp = handle_stamp(&server, params);
        assert!(resp.error.is_none());
    }

    #[test]
    fn handle_verify_missing_content_returns_error() {
        let server = make_server();
        let params = serde_json::json!({});
        let resp = handle_verify(&server, params);
        assert!(resp.error.is_some());
    }

    #[test]
    fn handle_verify_missing_fortis_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"content": hex::encode(b"test")});
        let resp = handle_verify(&server, params);
        assert!(resp.error.is_some());
    }

    #[test]
    fn handle_verify_valid_fortis_returns_valid_true() {
        let server = make_server();
        let stamp_params = serde_json::json!({
            "content": hex::encode(b"verify-me"),
            "echo": "verify-test"
        });
        let stamp_resp = handle_stamp(&server, stamp_params);
        let fortis_json = stamp_resp.result.unwrap();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"verify-me"),
            "fortis": fortis_json,
        });
        let verify_resp = handle_verify(&server, verify_params);
        assert!(verify_resp.error.is_none());
        let result = verify_resp.result.unwrap();
        assert_eq!(result.get("valid").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn handle_verify_wrong_content_returns_valid_false() {
        let server = make_server();
        let stamp_params = serde_json::json!({
            "content": hex::encode(b"original"),
            "echo": "verify-test"
        });
        let stamp_resp = handle_stamp(&server, stamp_params);
        let fortis_json = stamp_resp.result.unwrap();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"tampered"),
            "fortis": fortis_json,
        });
        let verify_resp = handle_verify(&server, verify_params);
        assert!(verify_resp.error.is_none());
        let result = verify_resp.result.unwrap();
        assert_eq!(result.get("valid").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn handle_get_calendar_slice_returns_records() {
        let server = make_server();
        for i in 0..3 {
            let params = serde_json::json!({
                "content": hex::encode(format!("item-{}", i).as_bytes()),
            });
            handle_stamp(&server, params);
        }

        let params = serde_json::json!({"cal_tick_start": 0, "count": 10});
        let resp = handle_get_calendar_slice(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let records = result.as_array().unwrap();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn handle_get_calendar_slice_exceeds_count_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"cal_tick_start": 0, "count": 10_001});
        let resp = handle_get_calendar_slice(&server, params);
        assert!(resp.error.is_some());
    }

    #[test]
    fn do_stamp_produces_valid_fortis() {
        let server = make_server();
        let fortis = do_stamp(&server, b"test".to_vec(), "echo".to_string()).unwrap();
        assert_eq!(fortis.tick_number, 1);
        assert!(!fortis.signature.is_empty());
        assert_eq!(fortis.echo, "echo");
    }

    #[test]
    fn handle_integrity_check_empty_calendar() {
        let server = make_server();
        let params = serde_json::json!({});
        let resp = handle_integrity_check(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.get("all_valid").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(result.get("pairs_checked").and_then(|v| v.as_u64()), Some(0));
    }

    #[test]
    fn handle_integrity_check_after_stamps() {
        let server = make_server();
        for i in 0..3 {
            let params = serde_json::json!({
                "content": hex::encode(format!("item-{}", i).as_bytes()),
            });
            handle_stamp(&server, params);
        }

        let params = serde_json::json!({});
        let resp = handle_integrity_check(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.get("all_valid").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(result.get("pairs_checked").and_then(|v| v.as_u64()), Some(2));
    }

    #[test]
    fn handle_integrity_check_with_range() {
        let server = make_server();
        for i in 0..5 {
            let params = serde_json::json!({
                "content": hex::encode(format!("item-{}", i).as_bytes()),
            });
            handle_stamp(&server, params);
        }

        let params = serde_json::json!({"start": 1, "end": 3});
        let resp = handle_integrity_check(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.get("all_valid").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(result.get("pairs_checked").and_then(|v| v.as_u64()), Some(2));
    }
}
