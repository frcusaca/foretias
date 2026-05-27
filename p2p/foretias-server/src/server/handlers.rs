use serde_json::Value;

use foretias_core::error::NodeError;
use foretias_core::foretias::clean_auth::{
    CleanAuthenticatedChrononRecord,
    UnprocessedChrononRecord, UnprocessedForetis,
};
use foretias_core::foretias::tick::CalendarLookup;
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

pub fn handle_route_stamp(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let target_tbid = match params.get("target_tbid").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing or invalid 'target_tbid' (hex string)".into()),
    };

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

    let my_tbid_hex = server.get_tbid().to_hex();
    if target_tbid == my_tbid_hex {
        return handle_stamp(server, params);
    }

    let Some(cm) = server.communerd() else {
        return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            "p2p not enabled".into());
    };

    match tokio::runtime::Handle::current().block_on(async {
        cm.route_stamp(&target_tbid, &content_hex, &echo).await
    }) {
        Ok(foretis) => {
            server.metrics().inc(MetricField::StampsTotal);
            if let Err(e) = server.save() {
                tracing::warn!("failed to persist calendar after routed stamp: {}", e);
            }
            resp_success(server, id, serde_json::to_value(&foretis).unwrap_or(Value::Null))
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("route stamp failed: {}", e)),
    }
}

pub fn handle_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'content'".into()),
    };

    let unproc_foretis = match params.get("foretis") {
        Some(v) => UnprocessedForetis::from_json_value(v.clone()),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing or invalid 'foretis'".into()),
    };
    let unproc_foretis = match unproc_foretis {
        Ok(f) => f,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("failed to parse foretis: {}", e)),
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

    let cross_node = params.get("cross_node")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let foretis_ref = unproc_foretis.inner();

    // Try local calendar first
    let crypto = server.chronomatter().crypto_server();
    let calendar = server.calendar().inner();
    let cal_read = calendar.read();
    let local_records = cal_read.get(foretis_ref.chronon_number, 1);
    let local_valid = match local_records {
        Ok(recs) if !recs.is_empty() => {
            let calendar_record = CleanAuthenticatedChrononRecord::from_trusted(recs[0].clone());
            unproc_foretis
                .clone()
                .into_clean_authenticated(crypto.as_ref(), &content, &calendar_record)
                .is_ok()
        }
        _ => false,
    };
    drop(cal_read);

    if local_valid {
        return resp_success(server, id, serde_json::json!({"valid": true, "method": "local"}));
    }

    // Local calendar miss — if cross_node is enabled, try DHT lookup
    if !cross_node {
        return resp_success(server, id, serde_json::json!({"valid": false, "method": "local", "note": "foretis.tbid not found in local calendar"}));
    }

    let foretis_tbid_hex = foretis_ref.tbid.to_hex();
    if foretis_ref.tbid == server.get_tbid() {
        return resp_success(server, id, serde_json::json!({"valid": false, "method": "local", "note": "own TBID but calendar miss"}));
    }

    // Cross-node verification: lookup TBID owner via DHT, fetch calendar slice, retry
    match tokio::runtime::Handle::current().block_on(async {
        cross_node_verify(server, &unproc_foretis, &content, &foretis_tbid_hex).await
    }) {
        Ok(valid) => resp_success(server, id, serde_json::json!({"valid": valid, "method": "cross_node"})),
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("cross-node verify failed: {}", e)),
    }
}

async fn cross_node_verify(
    server: &TimeFamilyServer,
    unproc_foretis: &UnprocessedForetis,
    content: &[u8],
    foretis_tbid_hex: &str,
) -> Result<bool, NodeError> {
    let foretis_ref = unproc_foretis.inner();
    let Some(com) = server.communerd() else {
        return Err(NodeError::Internal("P2P not enabled".into()));
    };
    let ns = com.namespace();

    let owner = com.lookup_tbid(foretis_tbid_hex, &ns).await
        .ok_or_else(|| NodeError::Internal(format!("TBID {} not found in DHT", foretis_tbid_hex)))?;

    let owner_peer = crate::communerd::transport::PeerAddr {
        json_rpc: owner.json_rpc.clone(),
        peer_id: owner.peer_id.parse().ok(),
        last_seen_ns: 0,
    };
    let records = com.get_calendar_slice(&owner_peer, foretis_ref.chronon_number, 1).await
        .map_err(|e| NodeError::Internal(format!("calendar fetch failed: {}", e)))?;

    let rec = records.first().ok_or_else(|| NodeError::Internal(format!("tick {} not found on owner", foretis_ref.chronon_number)))?;

    if rec.chronon_number != foretis_ref.chronon_number {
        return Err(NodeError::AlgorithmMismatch(
            format!("tick chronon_number {} doesn't match Foretis {}",
                rec.chronon_number, foretis_ref.chronon_number)
        ));
    }
    if rec.signature_algorithm != foretis_ref.signature_algorithm {
        return Err(NodeError::AlgorithmMismatch(
            format!("tick uses '{}' but Foretis claims '{}'",
                rec.signature_algorithm, foretis_ref.signature_algorithm)
        ));
    }

    let calendar_record = CleanAuthenticatedChrononRecord::from_trusted(rec.clone());

    let crypto = server.chronomatter().crypto_server();
    let valid = unproc_foretis
        .clone()
        .into_clean_authenticated(crypto.as_ref(), content, &calendar_record)
        .map_err(|e| NodeError::Internal(e.to_string()))
        .map(|_| true)?;

    Ok(valid)
}

pub fn handle_get_calendar_slice(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    // Accept both new name and legacy name for wire-format compat
    let cal_chronon_start = params.get("cal_chronon_start")
        .or_else(|| params.get("cal_tick_start"))
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
    let records = cal.get(cal_chronon_start, count)
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

pub fn handle_ping(_server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    jsonrpc::JsonRpcResponse::success(id, serde_json::json!({"pong": true}))
}

pub fn handle_status(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let peer_count = server.communerd()
        .map(|c| {
            tokio::runtime::Handle::current()
                .block_on(async { c.get_peers().await.len() })
        })
        .unwrap_or(0);
    resp_success(server, id, serde_json::json!({
        "tbid": server.get_tbid().to_hex(),
        "tbn": server.get_tbn(),
        "tick_count": server.current_tick(),
        "peer_count": peer_count,
        "dormant": server.is_dormant(),
    }))
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
    // FROST epoch data not yet implemented — return an error so callers handle the unimplemented state
    resp_error(server, id, jsonrpc::INTERNAL_ERROR, "FROST epoch data not yet implemented".into())
}

pub fn handle_verify_epoch_snapshot(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    // FROST epoch verification not yet implemented — always return invalid so callers handle the unimplemented state
    resp_success(server, id, serde_json::json!({
        "valid": false,
        "reason": "FROST epoch verification not yet implemented",
    }))
}

pub fn handle_mirror_request(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'tbid'".into()),
    };

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&tbid) {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "mirror_reject".into());
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let tick_count = cal_read.ticks.len() as u64;
    let latest_tick = cal_read.latest().unwrap_or(0);
    let hash_sanity = crate::calendar::compute_hash_sanity(&cal_read.ticks);
    drop(cal_read);

    resp_success(server, id, serde_json::json!({
        "status": "accept",
        "tick_count": tick_count,
        "latest_tick": latest_tick,
        "hash_sanity": hash_sanity,
    }))
}

pub fn handle_mirror_accept(_server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = params.get("tbid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tick_count = params.get("tick_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let latest_tick = params.get("latest_tick").and_then(|v| v.as_u64()).unwrap_or(0);
    let hash_sanity = params.get("hash_sanity").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mirror_store = _server.mirror_store();
    mirror_store.mirrored_tbids();

    resp_success(_server, id, serde_json::json!({
        "status": "accepted",
        "tbid": tbid,
        "tick_count": tick_count,
        "latest_tick": latest_tick,
        "hash_sanity": hash_sanity,
    }))
}

pub fn handle_ship_batch(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tick_start = params.get("tick_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    if count > MAX_CALENDAR_SLICE_COUNT {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("count exceeds maximum of {}", MAX_CALENDAR_SLICE_COUNT));
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let records = cal_read.get(tick_start, count)
        .map_err(|e| NodeError::Internal(format!("calendar lookup failed: {}", e)));
    drop(cal_read);

    match records {
        Ok(recs) => {
            let batch_hash = crate::calendar::compute_hash_sanity(&recs);
            resp_success(server, id, serde_json::json!({
                "records": recs,
                "batch_hash": batch_hash,
                "count": recs.len(),
            }))
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR, format!("{}", e)),
    }
}

pub fn handle_ship_ack(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params.get("tbid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let empty_vec: Vec<Value> = vec![];
    let raw_records = params.get("records").and_then(|v| v.as_array()).unwrap_or(&empty_vec);
    let unprocessed: Result<Vec<UnprocessedChrononRecord>, _> = raw_records
        .iter()
        .map(|v| UnprocessedChrononRecord::from_json_value(v.clone()))
        .collect();
    let unprocessed = match unprocessed {
        Ok(r) => r,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("failed to parse records: {}", e)),
    };

    if unprocessed.is_empty() {
        return resp_success(server, id, serde_json::json!({
            "status": "acked",
            "tbid": tbid,
            "tick_count": server.mirror_store().mirror_tick_count(&tbid),
        }));
    }

    let crypto = server.chronomatter().crypto_server();

    let mut verified: Vec<CleanAuthenticatedChrononRecord> = Vec::new();
    for (i, unproc) in unprocessed.into_iter().enumerate() {
        let clean = if i == 0 {
            if unproc.inner().chronon_number == 1 {
                unproc.into_clean_authenticated_genesis(crypto.as_ref())
            } else {
                return resp_error(server, id, jsonrpc::INVALID_PARAMS,
                    "batch first record is not genesis (chronon_number != 1)".into());
            }
        } else {
            let prev = verified.last().unwrap();
            unproc.into_clean_authenticated(crypto.as_ref(), prev)
        };
        match clean {
            Ok(v) => verified.push(v),
            Err(e) => {
                return resp_error(server, id, jsonrpc::INVALID_PARAMS,
                    format!("chain verification failed at record {}: {}", i, e));
            }
        }
    }

    let mirror_store = server.mirror_store();
    for record in verified {
        if let Err(e) = mirror_store.insert_mirrored(&tbid, record.into_inner()) {
            tracing::warn!("mirror insert failed for {}: {}", tbid, e);
        }
    }

    let tick_count = mirror_store.mirror_tick_count(&tbid);

    resp_success(server, id, serde_json::json!({
        "status": "acked",
        "tbid": tbid,
        "tick_count": tick_count,
    }))
}

pub fn handle_stream_tick(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params.get("tbid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let chronon_number = params.get("chronon_number").and_then(|v| v.as_u64()).unwrap_or(0);
    let unproc = match params.get("record") {
        Some(v) => UnprocessedChrononRecord::from_json_value(v.clone()),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "missing 'record'".into()),
    };
    let unproc = match unproc {
        Ok(r) => r,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("failed to parse record: {}", e)),
    };

    let crypto = server.chronomatter().crypto_server();
    let mirror_store = server.mirror_store();

    let latest = mirror_store.latest_record(&tbid);
    let verified = match latest {
        Some(trusted_rec) => {
            let prev = CleanAuthenticatedChrononRecord::from_trusted(trusted_rec);
            unproc.into_clean_authenticated(crypto.as_ref(), &prev)
        }
        None => {
            if unproc.inner().chronon_number == 1 {
                unproc.into_clean_authenticated_genesis(crypto.as_ref())
            } else {
                return resp_error(server, id, jsonrpc::INVALID_PARAMS,
                    "non-genesis record without predecessor".into());
            }
        }
    };
    let verified = match verified {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            format!("verification failed: {}", e)),
    };

    if let Err(e) = mirror_store.insert_mirrored(&tbid, verified.into_inner()) {
        return resp_error(server, id, jsonrpc::INTERNAL_ERROR,
            format!("mirror insert failed: {}", e));
    }

    let tick_count = mirror_store.mirror_tick_count(&tbid);

    resp_success(server, id, serde_json::json!({
        "status": "acked",
        "tbid": tbid,
        "chronon_number": chronon_number,
        "tick_count": tick_count,
    }))
}

pub fn handle_stream_ack(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let chronon_number = params.get("chronon_number").and_then(|v| v.as_u64()).unwrap_or(0);

    resp_success(server, id, serde_json::json!({
        "status": "acked",
        "chronon_number": chronon_number,
    }))
}

pub fn handle_mirror_mutual(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let peer_addr = params.get("peer_addr")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let my_tbid = params.get("my_tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let peer_tbid = params.get("peer_tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&peer_tbid) {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS,
            "cannot accept mirror: limit exceeded".into());
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let my_tick_count = cal_read.ticks.len() as u64;
    let my_latest_tick = cal_read.latest().unwrap_or(0);
    let my_hash_sanity = crate::calendar::compute_hash_sanity(&cal_read.ticks);
    drop(cal_read);

    resp_success(server, id, serde_json::json!({
        "status": "ready",
        "my_addr": &server.listen_addr,
        "my_tbid": my_tbid,
        "peer_tbid": peer_tbid,
        "my_tick_count": my_tick_count,
        "my_latest_tick": my_latest_tick,
        "my_hash_sanity": my_hash_sanity,
        "peer_addr": peer_addr,
    }))
}

pub fn handle_mirror_reconcile(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params.get("tbid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let peer_tick_count = params.get("peer_tick_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let _peer_latest_tick = params.get("peer_latest_tick").and_then(|v| v.as_u64()).unwrap_or(0);
    let peer_hash_sanity = params.get("peer_hash_sanity").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mirror_store = server.mirror_store();
    let my_info = mirror_store.mirror_info(&tbid);

    let result = match my_info {
        Some((my_tick_count, my_latest_tick, my_hash)) => {
            let status = if my_tick_count == peer_tick_count && my_hash == peer_hash_sanity {
                "in_sync"
            } else if my_tick_count < peer_tick_count {
                "behind"
            } else if my_tick_count > peer_tick_count {
                "ahead"
            } else {
                "diverged"
            };

            let divergence_tick = if status == "behind" || status == "diverged" {
                Some(my_latest_tick + 1)
            } else {
                None
            };

            serde_json::json!({
                "status": status,
                "my_tick_count": my_tick_count,
                "my_latest_tick": my_latest_tick,
                "my_hash_sanity": my_hash,
                "divergence_tick": divergence_tick,
            })
        }
        None => {
            serde_json::json!({
                "status": "not_mirrored",
                "my_tick_count": 0u64,
                "my_latest_tick": 0u64,
                "my_hash_sanity": String::new(),
                "divergence_tick": None::<u64>,
            })
        }
    };

    resp_success(server, id, result)
}

// ── Group 4b: Active Mirroring wire handlers ───────────────────────────────
//
// Six JSON-RPC methods that drive the active-mirror dance defined in
// COMBINED_GROUP4_SPEC.md §3.4. These run on the *receiving* side of each
// arrow in that table; sender-side flow is driven by Calendar's task queue
// (Phase 4b.4) which constructs requests and processes responses.

/// `mirror_announce` — a source node asking this node "I have TBID X, will
/// you mirror?". Response status is "accept" if MirrorStore has capacity for
/// the requested TBID; "reject" otherwise.
pub fn handle_mirror_announce(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            );
        }
    };

    let mirror_store = server.mirror_store();
    if mirror_store.can_accept_mirror(&tbid) {
        resp_success(server, id, serde_json::json!({
            "status": "accept",
            "tbid": tbid,
            "current_tick_count": mirror_store.mirror_tick_count(&tbid),
        }))
    } else {
        resp_success(server, id, serde_json::json!({
            "status": "reject",
            "tbid": tbid,
            "reason": "mirror capacity exhausted",
        }))
    }
}

/// `history_dump_request` — source asking mirror to open a chunked dump
/// stream for chronons [start..=end]. Response is accept/reject.
pub fn handle_history_dump_request(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            );
        }
    };
    let chronon_start = params
        .get("chronon_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let chronon_end = params
        .get("chronon_end")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if chronon_end < chronon_start {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!(
                "chronon_end ({chronon_end}) < chronon_start ({chronon_start})"
            ),
        );
    }

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&tbid) {
        return resp_success(server, id, serde_json::json!({
            "status": "reject",
            "tbid": tbid,
            "reason": "mirror capacity exhausted",
        }));
    }

    resp_success(server, id, serde_json::json!({
        "status": "accept",
        "tbid": tbid,
        "chronon_start": chronon_start,
        "chronon_end": chronon_end,
        "current_tick_count": mirror_store.mirror_tick_count(&tbid),
    }))
}

/// `history_dump_ack` — mirror acknowledging a dump request to the source.
/// Server-side this is a passthrough echo (the source consumes the ack on
/// its side). Provided for dispatch-table symmetry.
pub fn handle_history_dump_ack(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status = params
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("accept")
        .to_string();
    resp_success(server, id, serde_json::json!({
        "status": "noted",
        "tbid": tbid,
        "echoed_status": status,
    }))
}

/// `history_dump_chunk` — source delivering one chunk of N
/// ExternalizedChrononRecords. Each record is parsed as
/// `UnprocessedChrononRecord` and chain-verified against the previous
/// authenticated record (or genesis) before insertion into the mirror
/// store. Preserves the trust boundary.
pub fn handle_history_dump_chunk(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            );
        }
    };
    let empty_vec: Vec<Value> = vec![];
    let raw_records = params
        .get("records")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_vec);

    if raw_records.len() > MAX_CALENDAR_SLICE_COUNT {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!("chunk size exceeds maximum of {MAX_CALENDAR_SLICE_COUNT}"),
        );
    }

    let unprocessed: Result<Vec<UnprocessedChrononRecord>, _> = raw_records
        .iter()
        .map(|v| UnprocessedChrononRecord::from_json_value(v.clone()))
        .collect();
    let unprocessed = match unprocessed {
        Ok(r) => r,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("failed to parse records: {e}"),
            );
        }
    };

    let crypto = server.chronomatter().crypto_server();
    let mirror_store = server.mirror_store();

    let mut verified: Vec<CleanAuthenticatedChrononRecord> = Vec::new();
    for (i, unproc) in unprocessed.into_iter().enumerate() {
        // First record in chunk: chain against the latest stored record if any,
        // else treat as genesis (chronon_number must be 1).
        let clean = if let Some(prev_record) = verified.last() {
            unproc.into_clean_authenticated(crypto.as_ref(), prev_record)
        } else if let Some(prev_trusted) = mirror_store.latest_record(&tbid) {
            let prev = CleanAuthenticatedChrononRecord::from_trusted(prev_trusted);
            unproc.into_clean_authenticated(crypto.as_ref(), &prev)
        } else if unproc.inner().chronon_number == 1 {
            unproc.into_clean_authenticated_genesis(crypto.as_ref())
        } else {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "non-genesis record without predecessor in store".into(),
            );
        };
        match clean {
            Ok(v) => verified.push(v),
            Err(e) => {
                return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    format!("chain verification failed at record {i}: {e}"),
                );
            }
        }
    }

    let accepted = verified.len();
    for record in verified {
        if let Err(e) = mirror_store.insert_mirrored(&tbid, record.into_inner()) {
            tracing::warn!(tbid = %tbid, "mirror insert failed: {e}");
        }
    }

    resp_success(server, id, serde_json::json!({
        "status": "ok",
        "tbid": tbid,
        "accepted_count": accepted,
        "tick_count": mirror_store.mirror_tick_count(&tbid),
    }))
}

/// `history_dump_complete` — source marking end-of-stream for a dump.
/// Mirror finalizes and returns its current tick count for sanity check.
pub fn handle_history_dump_complete(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let claimed_total = params.get("total_records").and_then(|v| v.as_u64());

    let mirror_store = server.mirror_store();
    let tick_count = mirror_store.mirror_tick_count(&tbid);

    resp_success(server, id, serde_json::json!({
        "status": "complete",
        "tbid": tbid,
        "tick_count": tick_count,
        "claimed_total": claimed_total,
    }))
}

/// `mirror_health_check` — liveness probe from source. Mirror echoes the
/// current tick count + latest tick number for the TBID under mirror.
pub fn handle_mirror_health_check(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            );
        }
    };
    let mirror_store = server.mirror_store();
    let tick_count = mirror_store.mirror_tick_count(&tbid);
    let info = mirror_store.mirror_info(&tbid);
    let latest = info.as_ref().map(|(_, latest, _)| *latest);
    resp_success(server, id, serde_json::json!({
        "alive": true,
        "tbid": tbid,
        "tick_count": tick_count,
        "latest_tick": latest,
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
        assert!(result.get("chronon_number").is_some());
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

    /// g3-e regression: a malformed `foretis` field (e.g., an object with the
    /// wrong shape or a string instead of an object) must produce INVALID_PARAMS
    /// rather than panicking or silently returning a 500. Verifies the
    /// `UnprocessedForetis::from_json_value` error surfaces correctly.
    #[test]
    fn handle_verify_malformed_foretis_returns_invalid_params() {
        let server = make_server();
        // 'foretis' is a string instead of the expected object shape.
        let params = serde_json::json!({
            "content": hex::encode(b"test"),
            "foretis": "this-is-not-a-foretis-object",
        });
        let resp = handle_verify(&server, params);
        let err = resp.error.expect("malformed foretis must produce an error response");
        assert_eq!(
            err.code,
            jsonrpc::INVALID_PARAMS,
            "expected INVALID_PARAMS (-32602), got {} ({})",
            err.code,
            err.message
        );
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

        let params = serde_json::json!({"cal_chronon_start": 0, "count": 10});
        let resp = handle_get_calendar_slice(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let records = result.as_array().unwrap();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn handle_get_calendar_slice_exceeds_count_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"cal_chronon_start": 0, "count": 10_001});
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

        let params = serde_json::json!({"start": 1, "end": 4});
        let resp = handle_integrity_check(&server, params);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.get("all_valid").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(result.get("pairs_checked").and_then(|v| v.as_u64()), Some(3));
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

    #[test]
    fn handle_verify_epoch_snapshot_returns_valid_false() {
        let server = make_server();
        let params = serde_json::json!({"snapshot": {"epoch": 1}});
        let resp = handle_verify_epoch_snapshot(&server, params);
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
        let result = resp.result.unwrap();
        assert_eq!(result.get("valid").and_then(|v| v.as_bool()), Some(false));
        let reason = result.get("reason").and_then(|v| v.as_str()).unwrap();
        assert!(reason.contains("FROST epoch verification not yet implemented"));
    }

    #[test]
    fn handle_get_latest_epoch_returns_error() {
        let server = make_server();
        let params = serde_json::json!({});
        let resp = handle_get_latest_epoch(&server, params);
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
        let error = resp.error.unwrap();
        assert_eq!(error.code, jsonrpc::INTERNAL_ERROR);
        assert!(error.message.contains("FROST epoch data not yet implemented"));
    }

    // ── Group 4b: mirror wire handler tests ─────────────────────────────────

    fn sample_tbid_hex() -> String {
        // 96 bytes, all 0xAB → 192-char lowercase hex.
        "ab".repeat(96)
    }

    #[test]
    fn handle_mirror_announce_accepts_when_capacity_available() {
        let server = make_server();
        let params = serde_json::json!({ "tbid": sample_tbid_hex() });
        let resp = handle_mirror_announce(&server, params);
        let result = resp.result.expect("expected success on accept");
        assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("accept"));
    }

    #[test]
    fn handle_mirror_announce_rejects_missing_tbid() {
        let server = make_server();
        let resp = handle_mirror_announce(&server, serde_json::json!({}));
        let err = resp.error.expect("missing tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_history_dump_request_accepts_valid_range() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 1,
            "chronon_end": 10,
        });
        let resp = handle_history_dump_request(&server, params);
        let result = resp.result.expect("expected success");
        assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("accept"));
    }

    #[test]
    fn handle_history_dump_request_rejects_inverted_range() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 100,
            "chronon_end": 10,
        });
        let resp = handle_history_dump_request(&server, params);
        let err = resp.error.expect("inverted range must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_history_dump_ack_echoes_status() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "status": "accept",
        });
        let resp = handle_history_dump_ack(&server, params);
        let result = resp.result.expect("expected success");
        assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("noted"));
        assert_eq!(
            result.get("echoed_status").and_then(|v| v.as_str()),
            Some("accept")
        );
    }

    #[test]
    fn handle_history_dump_chunk_with_empty_records_succeeds() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "records": Vec::<Value>::new(),
        });
        let resp = handle_history_dump_chunk(&server, params);
        let result = resp.result.expect("empty chunk should succeed");
        assert_eq!(result.get("accepted_count").and_then(|v| v.as_u64()), Some(0));
    }

    #[test]
    fn handle_history_dump_chunk_rejects_non_genesis_first_record() {
        use foretias_core::foretias::tick::ChrononRecord;
        use foretias_core::foretias::types::Tbid;
        let server = make_server();
        let bad_record = ChrononRecord {
            chronon_number: 999,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        };
        let bad_json = serde_json::to_value(&bad_record).expect("serialize bad record");
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "records": [bad_json],
        });
        let resp = handle_history_dump_chunk(&server, params);
        let err = resp.error.expect("non-genesis first record must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
        assert!(
            err.message.contains("genesis") || err.message.contains("predecessor"),
            "unexpected error message: {}",
            err.message
        );
    }

    #[test]
    fn handle_history_dump_chunk_rejects_oversized() {
        let server = make_server();
        // Construct a fake records array exceeding MAX_CALENDAR_SLICE_COUNT.
        let bloat: Vec<Value> = (0..(MAX_CALENDAR_SLICE_COUNT + 1))
            .map(|_| serde_json::json!({}))
            .collect();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "records": bloat,
        });
        let resp = handle_history_dump_chunk(&server, params);
        let err = resp.error.expect("oversized chunk must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
        assert!(err.message.contains("exceeds maximum"));
    }

    #[test]
    fn handle_history_dump_complete_returns_tick_count() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "total_records": 42,
        });
        let resp = handle_history_dump_complete(&server, params);
        let result = resp.result.expect("expected success");
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("complete")
        );
        assert_eq!(
            result.get("claimed_total").and_then(|v| v.as_u64()),
            Some(42)
        );
    }

    #[test]
    fn handle_mirror_health_check_reports_alive() {
        let server = make_server();
        let params = serde_json::json!({ "tbid": sample_tbid_hex() });
        let resp = handle_mirror_health_check(&server, params);
        let result = resp.result.expect("expected success");
        assert_eq!(result.get("alive").and_then(|v| v.as_bool()), Some(true));
        assert!(result.get("tick_count").is_some());
    }

    #[test]
    fn handle_mirror_health_check_rejects_missing_tbid() {
        let server = make_server();
        let resp = handle_mirror_health_check(&server, serde_json::json!({}));
        let err = resp.error.expect("missing tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }
}
