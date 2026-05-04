use serde_json::Value;

use foretias_core::error::NodeError;
use foretias_core::epoch::EpochSnapshot;
use foretias_core::foretias::tick::CalendarLookup;
use foretias_core::foretias::Foretis;
use super::jsonrpc::{self, JsonRpcResponse};
use crate::metrics::MetricField;
use super::TimeFamilyServer;

const MAX_CONTENT_BYTES: usize = 1_073_741_824;
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

    let echo = params.get("echo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let cm = server.chronomatter();
    if server.is_dormant() {
        return resp_error(server, id, jsonrpc::DORMANT_ERROR,
            "node is dormant".into());
    }
    match cm.stamp(content, echo) {
        Ok(foretis) => {
            server.metrics().inc(MetricField::StampsTotal);
            if let Err(e) = server.save() {
                tracing::warn!("failed to persist calendar after stamp: {}", e);
            }
            resp_success(server, id, serde_json::to_value(&foretis).unwrap_or(Value::Null))
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("stamp failed: {}", e)),
    }
}

pub fn handle_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'content'".into()),
    };

    let foretis: Foretis = match params.get("foretis").and_then(|v| serde_json::from_value(v.clone()).ok()) {
        Some(f) => f,
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing or invalid 'foretis'".into()),
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

    let cm = server.chronomatter();
    let valid = match cm.verify(&foretis, &content, &*server.calendar().inner().read()) {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("verify failed: {}", e)),
    };

    resp_success(server, id, serde_json::json!({"valid": valid}))
}

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

    let calendar = server.calendar().inner();
    let cal = calendar.read();
    let records = cal.get(cal_tick_start, count)
        .map_err(|e| NodeError::Internal(format!("calendar lookup failed: {}", e)));

    match records {
        Ok(recs) => resp_success(server, id, serde_json::to_value(&recs).unwrap_or(Value::Null)),
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR, format!("{}", e)),
    }
}

pub fn handle_integrity_check(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let start = params.get("start").and_then(|v| v.as_u64());
    let end = params.get("end").and_then(|v| v.as_u64());

    let cm = server.chronomatter();
    match cm.integrity_check(&*server.calendar().inner().read(), start, end) {
        Ok(results) => {
            let all_valid = results.iter().all(|&v| v);
            resp_success(server, id, serde_json::json!({
                "all_valid": all_valid,
                "pair_results": results,
                "pairs_checked": results.len(),
            }))
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("integrity check failed: {}", e)),
    }
}

pub fn handle_collision_status(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let metrics = server.metrics();
    resp_success(server, id, serde_json::json!({
        "dormant": server.is_dormant(),
        "heartbeats_sent": metrics.heartbeats_sent.load(std::sync::atomic::Ordering::Relaxed),
        "heartbeats_received": metrics.heartbeats_received.load(std::sync::atomic::Ordering::Relaxed),
        "collisions_detected": metrics.collisions_detected.load(std::sync::atomic::Ordering::Relaxed),
        "dormant_transitions": metrics.dormant_transitions.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

pub fn handle_get_peer_score(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let peer_id = match params.get("peer_id").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'peer_id'".into()),
    };
    if let Some(com) = server.communerd() {
        let (score, count) = com.get_peer_score(&peer_id);
        resp_success(server, id, serde_json::json!({
            "peer_id": peer_id,
            "score": score,
            "report_count": count,
        }))
    } else {
        resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            "p2p not enabled".into())
    }
}

pub fn handle_get_latest_epoch(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    // Stub: return empty epoch data since we don't persist snapshots yet
    // In the real implementation, this reads from TimeFamilyServer.latest_epoch_snapshot
    resp_success(server, id, serde_json::json!({
        "epoch_number": 0u64,
        "epoch_start_ns": 0u64,
        "epoch_end_ns": 0u64,
        "peer_scores": serde_json::Value::Array(vec![]),
        "committee": serde_json::Value::Array(vec![]),
        "threshold": 0u32,
        "frost_signature": String::new(),
        "committee_pubkey": String::new(),
    }))
}

pub fn handle_verify_epoch_snapshot(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    // Stub: always valid (no FROST verification yet)
    if let Some(snapshot_val) = params.get("snapshot") {
        if let Ok(snapshot) = serde_json::from_value::<EpochSnapshot>(snapshot_val.clone()) {
            resp_success(server, id, serde_json::json!({
                "valid": true,
                "epoch_number": snapshot.epoch_number,
                "committee_size": snapshot.committee.len(),
            }))
        } else {
            resp_success(server, id, serde_json::json!({
                "valid": false,
                "error": "invalid snapshot JSON",
            }))
        }
    } else {
        resp_success(server, id, serde_json::json!({
            "valid": false,
            "error": "missing snapshot parameter",
        }))
    }
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
    fn handle_stamp_valid_content_returns_foretis() {
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
    fn handle_verify_missing_foretis_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"content": hex::encode(b"test")});
        let resp = handle_verify(&server, params);
        assert!(resp.error.is_some());
    }

    #[test]
    fn handle_verify_valid_foretis_returns_valid_true() {
        let server = make_server();
        let stamp_params = serde_json::json!({
            "content": hex::encode(b"verify-me"),
            "echo": "verify-test"
        });
        let stamp_resp = handle_stamp(&server, stamp_params);
        let foretis_json = stamp_resp.result.unwrap();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"verify-me"),
            "foretis": foretis_json,
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
        let foretis_json = stamp_resp.result.unwrap();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"tampered"),
            "foretis": foretis_json,
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

    #[test]
    fn handle_collision_status_returns_metrics() {
        let server = make_server();
        let params = serde_json::json!({});
        let resp = handle_collision_status(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["dormant"], false);
        assert_eq!(result["heartbeats_sent"], 0);
        assert_eq!(result["collisions_detected"], 0);
    }
}
