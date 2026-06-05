use serde_json::Value;

use super::jsonrpc::{self, JsonRpcResponse};
use super::TimeFamilyServer;
use crate::metrics::MetricField;
use foretias_core::error::NodeError;
use foretias_core::foretias::clean_auth::{CleanAuthenticated, UnverifiedSignatureEnvelope};
use foretias_core::foretias::tick::{CalendarLookup, ChrononRecord, Foretis};

const MAX_CONTENT_BYTES: usize = 1_073_741_824;
const MAX_CALENDAR_SLICE_COUNT: usize = 10_000;

fn resp_success(server: &TimeFamilyServer, id: Option<Value>, result: Value) -> JsonRpcResponse {
    if server.is_dormant() {
        jsonrpc::JsonRpcResponse::dormant_success(id, result)
    } else {
        jsonrpc::JsonRpcResponse::success(id, result)
    }
}

fn resp_error(
    server: &TimeFamilyServer,
    id: Option<Value>,
    code: i32,
    message: String,
) -> JsonRpcResponse {
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
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'content' (hex string)".into(),
            )
        }
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("invalid hex: {}", e),
            )
        }
    };

    if content.len() > MAX_CONTENT_BYTES {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!(
                "content exceeds maximum size of {} bytes",
                MAX_CONTENT_BYTES
            ),
        );
    }

    let echo = params
        .get("echo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let cm = server.chronomatter();
    if server.is_dormant() {
        return resp_error(server, id, jsonrpc::DORMANT_ERROR, "node is dormant".into());
    }
    match cm.stamp(content, echo) {
        Ok(stamped) => {
            server.metrics().inc(MetricField::StampsTotal);
            if let Err(e) = server.save() {
                tracing::warn!("failed to persist calendar after stamp: {}", e);
            }
            resp_success(
                server,
                id,
                serde_json::json!({
                    "foretis": stamped.foretis,
                    "signature": hex::encode(&stamped.signature_bytes),
                    "signature_algorithm": stamped.signature_algorithm,
                }),
            )
        }
        Err(e) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            format!("stamp failed: {}", e),
        ),
    }
}

pub fn handle_route_stamp(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let target_tbid = match params.get("target_tbid").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'target_tbid' (hex string)".into(),
            )
        }
    };

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'content' (hex string)".into(),
            )
        }
    };

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("invalid hex: {}", e),
            )
        }
    };

    if content.len() > MAX_CONTENT_BYTES {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!(
                "content exceeds maximum size of {} bytes",
                MAX_CONTENT_BYTES
            ),
        );
    }

    let echo = params
        .get("echo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let my_tbid_hex = server.get_tbid().to_hex();
    if target_tbid == my_tbid_hex {
        return handle_stamp(server, params);
    }

    let Some(cm) = server.communerd() else {
        return resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "p2p not enabled".into(),
        );
    };

    match tokio::runtime::Handle::current()
        .block_on(async { cm.route_stamp(&target_tbid, &content_hex, &echo).await })
    {
        Ok(ca_foretis) => {
            server.metrics().inc(MetricField::StampsTotal);
            if let Err(e) = server.save() {
                tracing::warn!("failed to persist calendar after routed stamp: {}", e);
            }
            // Extract signature metadata before consuming the wrapper, then return the
            // same envelope shape as handle_stamp for response-shape consistency.
            let sig_hex = ca_foretis
                .signature_bytes()
                .map(hex::encode)
                .unwrap_or_default();
            let sig_alg = ca_foretis
                .signature_algorithm()
                .unwrap_or("Ed25519")
                .to_string();
            resp_success(
                server,
                id,
                serde_json::json!({
                    "foretis": ca_foretis.into_inner(),
                    "signature": sig_hex,
                    "signature_algorithm": sig_alg,
                }),
            )
        }
        Err(e) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            format!("route stamp failed: {}", e),
        ),
    }
}

pub fn handle_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let content_hex = match params.get("content").and_then(|v| v.as_str()) {
        Some(h) => h.to_string(),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing 'content'".into(),
            )
        }
    };

    let foretis_value = match params.get("foretis") {
        Some(v) => v.clone(),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'foretis'".into(),
            )
        }
    };

    let signature_hex = params
        .get("signature")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let signature_algorithm = params
        .get("signature_algorithm")
        .and_then(|v| v.as_str())
        .unwrap_or("Ed25519")
        .to_string();
    let signature = hex::decode(&signature_hex).unwrap_or_default();

    let content = match hex::decode(&content_hex) {
        Ok(b) => b,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("invalid hex: {}", e),
            )
        }
    };

    if content.len() > MAX_CONTENT_BYTES {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!(
                "content exceeds maximum size of {} bytes",
                MAX_CONTENT_BYTES
            ),
        );
    }

    let foretis: Foretis = match serde_json::from_value(foretis_value.clone()) {
        Ok(f) => f,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("failed to parse foretis: {}", e),
            )
        }
    };

    let cross_node = params
        .get("cross_node")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Try local calendar first
    let crypto = server.chronomatter().crypto_server();
    let calendar = server.calendar().inner();
    let cal_read = calendar.read();
    let local_valid = if let Ok(recs) = cal_read.get(foretis.chronon_number, 1) {
        !recs.is_empty()
            && foretias_core::foretias::tick::verify(
                crypto.as_ref(),
                &foretis,
                &signature,
                &signature_algorithm,
                &content,
                &*cal_read,
            )
            .unwrap_or(false)
    } else {
        false
    };
    drop(cal_read);

    if local_valid {
        return resp_success(
            server,
            id,
            serde_json::json!({"valid": true, "method": "local"}),
        );
    }

    // Local calendar miss — if cross_node is enabled, try DHT lookup
    if !cross_node {
        return resp_success(
            server,
            id,
            serde_json::json!({"valid": false, "method": "local", "note": "foretis.tbid not found in local calendar"}),
        );
    }

    let foretis_tbid_hex = foretis.tbid.to_hex();
    if foretis.tbid == server.get_tbid() {
        return resp_success(
            server,
            id,
            serde_json::json!({"valid": false, "method": "local", "note": "own TBID but calendar miss"}),
        );
    }

    // Cross-node verification: construct envelope and use existing path
    let unproc_foretis =
        match UnverifiedSignatureEnvelope::<Foretis>::from_json_value(foretis_value) {
            Ok(f) => f,
            Err(e) => {
                return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    format!("failed to parse foretis: {}", e),
                )
            }
        };

    match tokio::runtime::Handle::current().block_on(async {
        cross_node_verify(server, &unproc_foretis, &content, &foretis_tbid_hex).await
    }) {
        Ok(valid) => resp_success(
            server,
            id,
            serde_json::json!({"valid": valid, "method": "cross_node"}),
        ),
        Err(e) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            format!("cross-node verify failed: {}", e),
        ),
    }
}

async fn cross_node_verify(
    server: &TimeFamilyServer,
    unproc_foretis: &UnverifiedSignatureEnvelope<Foretis>,
    content: &[u8],
    foretis_tbid_hex: &str,
) -> Result<bool, NodeError> {
    let foretis_ref = unproc_foretis.inner();
    let Some(com) = server.communerd() else {
        return Err(NodeError::Internal("P2P not enabled".into()));
    };

    // Phase 8.1: use CommunerdetteLine::get_tick — goes through the full Take 3
    // inbound gate (gate_chronon_records) and returns CleanAuthenticated<ChrononRecord>.
    // No manual DHT lookup, no PeerAddr construction, no from_trusted bypass.
    let tbid = foretias_core::foretias::types::Tbid::from_hex(foretis_tbid_hex)
        .map_err(|e| NodeError::Internal(format!("bad TBID hex: {e}")))?;
    let tick: foretias_core::foretias::clean_auth::CleanAuthenticated<
        foretias_core::foretias::tick::ChrononRecord,
    > = com
        .line_for_tbid(tbid)
        .get_tick(foretis_ref.chronon_number)
        .await
        .map_err(|e| NodeError::Internal(format!("get_tick failed: {e:?}")))?;

    let crypto = server.chronomatter().crypto_server();
    let valid = unproc_foretis
        .clone()
        .into_clean_authenticated(crypto.as_ref(), content, &tick)
        .map_err(|e| NodeError::Internal(e.to_string()))
        .map(|_| true)?;

    Ok(valid)
}

pub fn handle_get_calendar_slice(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    // Accept both new name and legacy name for wire-format compat
    let cal_chronon_start = params
        .get("cal_chronon_start")
        .or_else(|| params.get("cal_tick_start"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if count > MAX_CALENDAR_SLICE_COUNT {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!("count exceeds maximum of {}", MAX_CALENDAR_SLICE_COUNT),
        );
    }

    let calendar = server.calendar().inner();
    let cal = calendar.read();
    let records = cal
        .get(cal_chronon_start, count)
        .map_err(|e| NodeError::Internal(format!("calendar lookup failed: {}", e)));

    match records {
        Ok(recs) => resp_success(
            server,
            id,
            serde_json::to_value(&recs).unwrap_or(Value::Null),
        ),
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
            resp_success(
                server,
                id,
                serde_json::json!({
                    "all_valid": all_valid,
                    "pair_results": results,
                    "pairs_checked": results.len(),
                }),
            )
        }
        Err(e) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            format!("integrity check failed: {}", e),
        ),
    }
}

pub fn handle_ping(_server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    jsonrpc::JsonRpcResponse::success(id, serde_json::json!({"pong": true}))
}

/// Handle `channel_bind_challenge` — Phase 12.0 channel-binding establishment.
///
/// The remote Communerdette sends a nonce + channel_id + requester_tbid.
/// We sign (nonce ‖ channel_id ‖ responder_tbid_hex) with both the fast key
/// (Ed25519) and the slow key (SLH-DSA) and return the dual-signed response.
/// // SIGN(local-tbid, fast-key+slow-key)
pub fn handle_channel_bind_challenge(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let nonce_hex = match params.get("nonce").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return jsonrpc::JsonRpcResponse::error(id, -32602, "missing nonce".to_string()),
    };
    let channel_id = match params.get("channel_id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return jsonrpc::JsonRpcResponse::error(id, -32602, "missing channel_id".to_string())
        }
    };

    let responder_tbid_hex = server.get_tbid().to_hex();

    // Build the canonical message: nonce_bytes ‖ channel_id_bytes ‖ responder_tbid_hex_bytes
    let nonce_bytes = match hex::decode(&nonce_hex) {
        Ok(b) => b,
        Err(_) => return jsonrpc::JsonRpcResponse::error(id, -32602, "nonce not hex".to_string()),
    };
    let mut msg = Vec::new();
    msg.extend_from_slice(&nonce_bytes);
    msg.extend_from_slice(channel_id.as_bytes());
    msg.extend_from_slice(responder_tbid_hex.as_bytes());

    // Dual-sign with the TBID secret key (Ed25519 || SLH-DSA, 49920 bytes total)
    let combined_sig = match server.sign_tbid_message(&msg) {
        Ok(s) => s,
        Err(e) => {
            return jsonrpc::JsonRpcResponse::error(id, -32000, format!("signing failed: {e}"))
        }
    };
    let sig_bytes = combined_sig.as_bytes();
    if sig_bytes.len() < 64 {
        return jsonrpc::JsonRpcResponse::error(id, -32000, "signature too short".to_string());
    }
    // Split: first 64 bytes = Ed25519 fast sig; remaining = SLH-DSA slow sig
    let fast_sig = hex::encode(&sig_bytes[..64]);
    let slow_sig = hex::encode(&sig_bytes[64..]);

    jsonrpc::JsonRpcResponse::success(
        id,
        serde_json::json!({
            "responder_tbid": responder_tbid_hex,
            "nonce_echo": nonce_hex,
            "channel_id": channel_id,
            "fast_sig": fast_sig,
            "slow_sig": slow_sig,
        }),
    )
}

/// Handle `authenticated_ping` — Phase 12.3 L2 liveness.
///
/// Remote Communerdette sends a challenge hex. We sign (challenge ‖ responder_tbid_hex)
/// with the local TBID's Ed25519 fast key (first 64 bytes of sign_tbid_message output).
/// // SIGN(local-tbid, fast-key)
pub fn handle_authenticated_ping(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let challenge_hex = match params.get("challenge").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return jsonrpc::JsonRpcResponse::error(id, -32602, "missing challenge".to_string())
        }
    };
    let challenge = match hex::decode(&challenge_hex) {
        Ok(b) => b,
        Err(_) => {
            return jsonrpc::JsonRpcResponse::error(id, -32602, "challenge not hex".to_string())
        }
    };
    let responder_tbid_hex = server.get_tbid().to_hex();
    let mut msg = Vec::new();
    msg.extend_from_slice(&challenge);
    msg.extend_from_slice(responder_tbid_hex.as_bytes());

    // Sign with both keys; extract Ed25519 (fast-key) portion (first 64 bytes)
    let combined_sig = match server.sign_tbid_message(&msg) {
        Ok(s) => s,
        Err(e) => {
            return jsonrpc::JsonRpcResponse::error(id, -32000, format!("signing failed: {e}"))
        }
    };
    let sig_bytes = combined_sig.as_bytes();
    if sig_bytes.len() < 64 {
        return jsonrpc::JsonRpcResponse::error(id, -32000, "signature too short".to_string());
    }
    let fast_sig_hex = hex::encode(&sig_bytes[..64]);

    jsonrpc::JsonRpcResponse::success(
        id,
        serde_json::json!({
            "responder_tbid": responder_tbid_hex,
            "challenge_echo": challenge_hex,
            "signature": fast_sig_hex,
            "signature_algorithm": "Ed25519",
        }),
    )
}

pub fn handle_status(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let peer_count = server
        .communerd()
        .map(|c| tokio::runtime::Handle::current().block_on(async { c.get_peers().await.len() }))
        .unwrap_or(0);
    resp_success(
        server,
        id,
        serde_json::json!({
            "tbid": server.get_tbid().to_hex(),
            "tbn": server.get_tbn(),
            "tick_count": server.current_tick(),
            "peer_count": peer_count,
            "dormant": server.is_dormant(),
        }),
    )
}

pub fn handle_collision_status(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let metrics = server.metrics();
    resp_success(
        server,
        id,
        serde_json::json!({
            "dormant": server.is_dormant(),
            "heartbeats_sent": metrics.heartbeats_sent.load(std::sync::atomic::Ordering::Relaxed),
            "heartbeats_received": metrics.heartbeats_received.load(std::sync::atomic::Ordering::Relaxed),
            "collisions_detected": metrics.collisions_detected.load(std::sync::atomic::Ordering::Relaxed),
            "dormant_transitions": metrics.dormant_transitions.load(std::sync::atomic::Ordering::Relaxed),
        }),
    )
}

pub fn handle_get_peer_score(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let peer_id = match params.get("peer_id").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing 'peer_id'".into(),
            )
        }
    };
    if let Some(com) = server.communerd() {
        let (score, count) = com.get_peer_score(&peer_id);
        resp_success(
            server,
            id,
            serde_json::json!({
                "peer_id": peer_id,
                "score": score,
                "report_count": count,
            }),
        )
    } else {
        resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "p2p not enabled".into(),
        )
    }
}

pub fn handle_get_latest_epoch(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    // FROST epoch data not yet implemented — return an error so callers handle the unimplemented state
    resp_error(
        server,
        id,
        jsonrpc::INTERNAL_ERROR,
        "FROST epoch data not yet implemented".into(),
    )
}

pub fn handle_verify_epoch_snapshot(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    // FROST epoch verification not yet implemented — always return invalid so callers handle the unimplemented state
    resp_success(
        server,
        id,
        serde_json::json!({
            "valid": false,
            "reason": "FROST epoch verification not yet implemented",
        }),
    )
}

pub fn handle_mirror_request(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return resp_error(server, id, jsonrpc::INVALID_PARAMS, "missing 'tbid'".into()),
    };

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&tbid) {
        return resp_error(server, id, jsonrpc::INVALID_PARAMS, "mirror_reject".into());
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let tick_count = cal_read.ticks.len() as u64;
    let latest_tick = cal_read.latest().unwrap_or(0);
    let hash_sanity = crate::calendar::compute_hash_sanity(&cal_read.ticks);
    drop(cal_read);

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "accept",
            "tick_count": tick_count,
            "latest_tick": latest_tick,
            "hash_sanity": hash_sanity,
        }),
    )
}

pub fn handle_mirror_accept(_server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tick_count = params
        .get("tick_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let latest_tick = params
        .get("latest_tick")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let hash_sanity = params
        .get("hash_sanity")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mirror_store = _server.mirror_store();
    mirror_store.mirrored_tbids();

    resp_success(
        _server,
        id,
        serde_json::json!({
            "status": "accepted",
            "tbid": tbid,
            "tick_count": tick_count,
            "latest_tick": latest_tick,
            "hash_sanity": hash_sanity,
        }),
    )
}

pub fn handle_ship_batch(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tick_start = params
        .get("tick_start")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if count > MAX_CALENDAR_SLICE_COUNT {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            format!("count exceeds maximum of {}", MAX_CALENDAR_SLICE_COUNT),
        );
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let records = cal_read
        .get(tick_start, count)
        .map_err(|e| NodeError::Internal(format!("calendar lookup failed: {}", e)));
    drop(cal_read);

    match records {
        Ok(recs) => {
            let batch_hash = crate::calendar::compute_hash_sanity(&recs);
            resp_success(
                server,
                id,
                serde_json::json!({
                    "records": recs,
                    "batch_hash": batch_hash,
                    "count": recs.len(),
                }),
            )
        }
        Err(e) => resp_error(server, id, jsonrpc::INTERNAL_ERROR, format!("{}", e)),
    }
}

pub fn handle_ship_ack(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let empty_vec: Vec<Value> = vec![];
    let raw_records = params
        .get("records")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_vec);
    let unprocessed: Result<Vec<UnverifiedSignatureEnvelope<ChrononRecord>>, _> = raw_records
        .iter()
        .map(|v| UnverifiedSignatureEnvelope::<ChrononRecord>::from_json_value(v.clone()))
        .collect();
    let unprocessed = match unprocessed {
        Ok(r) => r,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("failed to parse records: {}", e),
            )
        }
    };

    if unprocessed.is_empty() {
        return resp_success(
            server,
            id,
            serde_json::json!({
                "status": "acked",
                "tbid": tbid,
                "tick_count": server.mirror_store().mirror_tick_count(&tbid),
            }),
        );
    }

    let crypto = server.chronomatter().crypto_server();

    let mut verified: Vec<CleanAuthenticated<ChrononRecord>> = Vec::new();
    for (i, unproc) in unprocessed.into_iter().enumerate() {
        let clean = if i == 0 {
            if unproc.inner().chronon_number == 1 {
                unproc.into_clean_authenticated_genesis(crypto.as_ref())
            } else {
                return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "batch first record is not genesis (chronon_number != 1)".into(),
                );
            }
        } else {
            let prev = match verified.last() {
                Some(p) => p,
                None => return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "chain verification: missing previous record (internal invariant violation)"
                        .into(),
                ),
            };
            unproc.into_clean_authenticated(crypto.as_ref(), prev)
        };
        match clean {
            Ok(v) => verified.push(v),
            Err(e) => {
                return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    format!("chain verification failed at record {}: {}", i, e),
                );
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

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "acked",
            "tbid": tbid,
            "tick_count": tick_count,
        }),
    )
}

pub fn handle_stream_tick(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let chronon_number = params
        .get("chronon_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let unproc = match params.get("record") {
        Some(v) => UnverifiedSignatureEnvelope::<ChrononRecord>::from_json_value(v.clone()),
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing 'record'".into(),
            )
        }
    };
    let unproc = match unproc {
        Ok(r) => r,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("failed to parse record: {}", e),
            )
        }
    };

    let crypto = server.chronomatter().crypto_server();
    let mirror_store = server.mirror_store();

    let latest = mirror_store.latest_record(&tbid);
    let verified = match latest {
        Some(trusted_rec) => {
            let prev = CleanAuthenticated::<ChrononRecord>::from_trusted(trusted_rec);
            unproc.into_clean_authenticated(crypto.as_ref(), &prev)
        }
        None => {
            if unproc.inner().chronon_number == 1 {
                unproc.into_clean_authenticated_genesis(crypto.as_ref())
            } else {
                return resp_error(
                    server,
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "non-genesis record without predecessor".into(),
                );
            }
        }
    };
    let verified = match verified {
        Ok(v) => v,
        Err(e) => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                format!("verification failed: {}", e),
            )
        }
    };

    if let Err(e) = mirror_store.insert_mirrored(&tbid, verified.into_inner()) {
        return resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            format!("mirror insert failed: {}", e),
        );
    }

    let tick_count = mirror_store.mirror_tick_count(&tbid);

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "acked",
            "tbid": tbid,
            "chronon_number": chronon_number,
            "tick_count": tick_count,
        }),
    )
}

pub fn handle_stream_ack(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();
    let chronon_number = params
        .get("chronon_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "acked",
            "chronon_number": chronon_number,
        }),
    )
}

pub fn handle_mirror_mutual(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let peer_addr = params
        .get("peer_addr")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let my_tbid = params
        .get("my_tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let peer_tbid = params
        .get("peer_tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&peer_tbid) {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            "cannot accept mirror: limit exceeded".into(),
        );
    }

    let cal = server.calendar().inner();
    let cal_read = cal.read();
    let my_tick_count = cal_read.ticks.len() as u64;
    let my_latest_tick = cal_read.latest().unwrap_or(0);
    let my_hash_sanity = crate::calendar::compute_hash_sanity(&cal_read.ticks);
    drop(cal_read);

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "ready",
            "my_addr": &server.listen_addr,
            "my_tbid": my_tbid,
            "peer_tbid": peer_tbid,
            "my_tick_count": my_tick_count,
            "my_latest_tick": my_latest_tick,
            "my_hash_sanity": my_hash_sanity,
            "peer_addr": peer_addr,
        }),
    )
}

pub fn handle_mirror_reconcile(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = params
        .get("tbid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let peer_tick_count = params
        .get("peer_tick_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _peer_latest_tick = params
        .get("peer_latest_tick")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let peer_hash_sanity = params
        .get("peer_hash_sanity")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

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
        resp_success(
            server,
            id,
            serde_json::json!({
                "status": "accept",
                "tbid": tbid,
                "current_tick_count": mirror_store.mirror_tick_count(&tbid),
            }),
        )
    } else {
        resp_success(
            server,
            id,
            serde_json::json!({
                "status": "reject",
                "tbid": tbid,
                "reason": "mirror capacity exhausted",
            }),
        )
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
            format!("chronon_end ({chronon_end}) < chronon_start ({chronon_start})"),
        );
    }

    let mirror_store = server.mirror_store();
    if !mirror_store.can_accept_mirror(&tbid) {
        return resp_success(
            server,
            id,
            serde_json::json!({
                "status": "reject",
                "tbid": tbid,
                "reason": "mirror capacity exhausted",
            }),
        );
    }

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "accept",
            "tbid": tbid,
            "chronon_start": chronon_start,
            "chronon_end": chronon_end,
            "current_tick_count": mirror_store.mirror_tick_count(&tbid),
        }),
    )
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
    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "noted",
            "tbid": tbid,
            "echoed_status": status,
        }),
    )
}

/// `history_dump_chunk` — source delivering one chunk of N
/// Externalized<ChrononRecord>. Each record is parsed as
/// `UnverifiedSignatureEnvelopeChrononRecord` and chain-verified against the previous
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

    let unprocessed: Result<Vec<UnverifiedSignatureEnvelope<ChrononRecord>>, _> = raw_records
        .iter()
        .map(|v| UnverifiedSignatureEnvelope::<ChrononRecord>::from_json_value(v.clone()))
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

    let mut verified: Vec<CleanAuthenticated<ChrononRecord>> = Vec::new();
    for (i, unproc) in unprocessed.into_iter().enumerate() {
        // First record in chunk: chain against the latest stored record if any,
        // else treat as genesis (chronon_number must be 1).
        let clean = if let Some(prev_record) = verified.last() {
            unproc.into_clean_authenticated(crypto.as_ref(), prev_record)
        } else if let Some(prev_trusted) = mirror_store.latest_record(&tbid) {
            let prev = CleanAuthenticated::<ChrononRecord>::from_trusted(prev_trusted);
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

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "ok",
            "tbid": tbid,
            "accepted_count": accepted,
            "tick_count": mirror_store.mirror_tick_count(&tbid),
        }),
    )
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

    resp_success(
        server,
        id,
        serde_json::json!({
            "status": "complete",
            "tbid": tbid,
            "tick_count": tick_count,
            "claimed_total": claimed_total,
        }),
    )
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
    resp_success(
        server,
        id,
        serde_json::json!({
            "alive": true,
            "tbid": tbid,
            "tick_count": tick_count,
            "latest_tick": latest,
        }),
    )
}

// ── Chronon attestation handlers ───────────────────────────────────────────

/// `stamp_my_chronon` — enqueue a chronon-level mutual attestation with a
/// specific TBID. The requester provides its TBID hex and the chronon number.
/// The handler enqueues `DoChrononAttestation` to Calendar's task queue and
/// returns immediately with `{ "status": "queued" }`.
pub fn handle_stamp_my_chronon(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let requester_tbid = match params.get("requester_tbid").and_then(|v| v.as_str()) {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'requester_tbid' (hex string)".into(),
            )
        }
    };

    if hex::decode(&requester_tbid).is_err() {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            "invalid hex in 'requester_tbid'".into(),
        );
    }

    let _chronon_number = match params.get("chronon_number").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_number' (u64)".into(),
            )
        }
    };

    let task = crate::calendar::CalendarTask::DoChrononAttestation {
        target_tbid: requester_tbid,
    };

    match server.calendar().enqueue_task(task) {
        Ok(()) => resp_success(server, id, serde_json::json!({"status": "queued"})),
        Err(_) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "task queue not started or closed".into(),
        ),
    }
}

/// `stamp_my_chronon_block` — enqueue an epoch-level mutual attestation with a
/// specific TBID. The requester provides its TBID hex and the epoch number.
/// The handler enqueues `DoEpochAttestation` to Calendar's task queue and
/// returns immediately with `{ "status": "queued" }`.
pub fn handle_stamp_my_chronon_block(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let requester_tbid = match params.get("requester_tbid").and_then(|v| v.as_str()) {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'requester_tbid' (hex string)".into(),
            )
        }
    };

    if hex::decode(&requester_tbid).is_err() {
        return resp_error(
            server,
            id,
            jsonrpc::INVALID_PARAMS,
            "invalid hex in 'requester_tbid'".into(),
        );
    }

    let _epoch_number = match params.get("epoch_number").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'epoch_number' (u64)".into(),
            )
        }
    };

    let task = crate::calendar::CalendarTask::DoEpochAttestation {
        target_tbid: requester_tbid,
    };

    match server.calendar().enqueue_task(task) {
        Ok(()) => resp_success(server, id, serde_json::json!({"status": "queued"})),
        Err(_) => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "task queue not started or closed".into(),
        ),
    }
}

// ── Storage proof handlers ─────────────────────────────────────────────────

/// `storage_proof_request` — generate a Merkle storage proof covering a
/// chronon range. Returns per-block proofs with Merkle range evidence, or
/// an error if no CalendarStore is configured or no data covers the range.
pub fn handle_storage_proof_request(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    use crate::calendar_store::StorageProofRequest as Req;

    let id = params.get("id").cloned();

    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            )
        }
    };

    let chronon_start = match params.get("chronon_start").and_then(|v| v.as_u64()) {
        Some(v) => v,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_start' (u64)".into(),
            )
        }
    };

    let chronon_end = match params.get("chronon_end").and_then(|v| v.as_u64()) {
        Some(v) => v,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_end' (u64)".into(),
            )
        }
    };

    let Some(store) = server.calendar_store() else {
        return resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "no calendar store configured (requires --persist-path)".into(),
        );
    };

    let req = Req {
        tbid,
        chronon_start,
        chronon_end,
    };

    match store.prove_storage(&req) {
        Some(resp) => {
            let blocks_json: Vec<Value> = resp
                .blocks
                .iter()
                .map(|bp| {
                    serde_json::json!({
                        "block_id": bp.block_id,
                        "merkle_root": hex::encode(bp.merkle_root),
                        "leaves": bp.leaves.iter().take(bp.leaf_count)
                            .map(hex::encode).collect::<Vec<_>>(),
                        "siblings": bp.siblings.iter().take(bp.sibling_count)
                            .map(hex::encode).collect::<Vec<_>>(),
                        "leaf_count": bp.leaf_count,
                        "sibling_count": bp.sibling_count,
                        "n": bp.n,
                    })
                })
                .collect();

            resp_success(
                server,
                id,
                serde_json::json!({
                    "blocks": blocks_json,
                    "coverage_ratio": resp.coverage_ratio,
                }),
            )
        }
        None => resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "no storage proof available for the requested range".into(),
        ),
    }
}

/// `storage_proof_verify` — verify a storage proof against the original
/// request and optional known Merkle roots. Returns whether all block
/// proofs verified and the coverage ratio.
pub fn handle_storage_proof_verify(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    use crate::calendar_store::{verify_storage_proof, StorageProofResponse as Resp};

    let id = params.get("id").cloned();

    let req = match parse_storage_proof_request(&params) {
        Ok(r) => r,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS, e),
    };

    let (coverage_ratio, blocks_val) = match parse_storage_proof_response(&params) {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS, e),
    };

    let blocks = match parse_storage_proof_blocks(blocks_val) {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS, e),
    };

    let resp = Resp {
        blocks,
        coverage_ratio,
    };

    let known_roots = match parse_known_roots(&params) {
        Ok(v) => v,
        Err(e) => return resp_error(server, id, jsonrpc::INVALID_PARAMS, e),
    };

    let result = verify_storage_proof(&req, &resp, &known_roots);

    resp_success(
        server,
        id,
        serde_json::json!({
            "verified": result.verified,
            "coverage_ratio": result.coverage_ratio,
        }),
    )
}

fn parse_storage_proof_request(
    params: &Value,
) -> Result<crate::calendar_store::StorageProofRequest, String> {
    let req_val = params
        .get("request")
        .ok_or_else(|| "missing 'request'".to_string())?;

    let tbid = match req_val.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return Err("missing or empty 'request.tbid'".into()),
    };
    let chronon_start = req_val
        .get("chronon_start")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing or invalid 'request.chronon_start'".to_string())?;
    let chronon_end = req_val
        .get("chronon_end")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing or invalid 'request.chronon_end'".to_string())?;

    Ok(crate::calendar_store::StorageProofRequest {
        tbid,
        chronon_start,
        chronon_end,
    })
}

fn parse_storage_proof_response(params: &Value) -> Result<(f64, &Vec<Value>), String> {
    let resp_val = params
        .get("response")
        .ok_or_else(|| "missing 'response'".to_string())?;
    let coverage_ratio = resp_val
        .get("coverage_ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let blocks_val = resp_val
        .get("blocks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'response.blocks'".to_string())?;

    Ok((coverage_ratio, blocks_val))
}

fn parse_storage_proof_blocks(
    blocks_val: &[serde_json::Value],
) -> Result<Vec<crate::calendar_store::BlockProof>, String> {
    let mut blocks = Vec::new();
    for (i, bv) in blocks_val.iter().enumerate() {
        let block_id = bv.get("block_id").and_then(|v| v.as_u64()).unwrap_or(0);

        let merkle_root = match bv.get("merkle_root").and_then(|v| v.as_str()) {
            Some(h) => {
                let bytes = hex::decode(h).unwrap_or_default();
                if bytes.len() != 32 {
                    return Err(format!("blocks[{i}].merkle_root must be 64-char hex"));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                arr
            }
            None => return Err(format!("missing blocks[{i}].merkle_root")),
        };

        let leaf_count = bv.get("leaf_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let mut leaves = [[0u8; 32]; 64];
        if let Some(leaves_val) = bv.get("leaves").and_then(|v| v.as_array()) {
            for (j, lv) in leaves_val.iter().take(leaf_count.min(64)).enumerate() {
                if let Some(h) = lv.as_str() {
                    if let Ok(bytes) = hex::decode(h) {
                        if bytes.len() == 32 {
                            leaves[j].copy_from_slice(&bytes);
                        }
                    }
                }
            }
        }

        let sibling_count = bv
            .get("sibling_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let mut siblings = [[0u8; 32]; 32];
        if let Some(siblings_val) = bv.get("siblings").and_then(|v| v.as_array()) {
            for (j, sv) in siblings_val.iter().take(sibling_count.min(32)).enumerate() {
                if let Some(h) = sv.as_str() {
                    if let Ok(bytes) = hex::decode(h) {
                        if bytes.len() == 32 {
                            siblings[j].copy_from_slice(&bytes);
                        }
                    }
                }
            }
        }

        let n = bv.get("n").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        blocks.push(crate::calendar_store::BlockProof {
            block_id,
            merkle_root,
            leaves,
            siblings,
            leaf_count,
            sibling_count,
            n,
        });
    }
    Ok(blocks)
}

fn parse_known_roots(params: &Value) -> Result<Vec<[u8; 32]>, String> {
    let known_roots_val = params
        .get("known_roots")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut known_roots: Vec<[u8; 32]> = Vec::new();
    for (i, rv) in known_roots_val.iter().enumerate() {
        if let Some(h) = rv.as_str() {
            let bytes = hex::decode(h).unwrap_or_default();
            if bytes.len() != 32 {
                return Err(format!("known_roots[{i}] must be 64-char hex"));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            known_roots.push(arr);
        }
    }
    Ok(known_roots)
}

// ── Chronon query handlers ─────────────────────────────────────────────────

/// `get_chronon` — look up a single chronon record by number from the
/// persisted CalendarStore. Returns `{ "status": "found", "record": ... }`
/// if the chronon exists, or `{ "status": "not_found" }` otherwise.
pub fn handle_get_chronon(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    let id = params.get("id").cloned();

    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            )
        }
    };

    let chronon_number = match params.get("chronon_number").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_number' (u64)".into(),
            )
        }
    };

    let include_attestations = params
        .get("include_attestations")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let Some(store) = server.calendar_store() else {
        return resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "no calendar store configured (requires --persist-path)".into(),
        );
    };

    match store.get_chronon(&tbid, chronon_number, include_attestations) {
        Some(record) => {
            let record_json = serde_json::to_value(record.inner()).unwrap_or(Value::Null);
            resp_success(
                server,
                id,
                serde_json::json!({
                    "status": "found",
                    "record": record_json,
                }),
            )
        }
        None => resp_success(
            server,
            id,
            serde_json::json!({
                "status": "not_found",
            }),
        ),
    }
}

/// `get_chronon_chain` — look up a range of chronon records by number from
/// the persisted CalendarStore. Returns `{ "status": "complete"/"partial"/"none",
/// "records": [...], "coverage": { "requested": N, "returned": M } }`.
pub fn handle_get_chronon_chain(server: &TimeFamilyServer, params: Value) -> JsonRpcResponse {
    use crate::calendar_store::ChrononChainResult;

    let id = params.get("id").cloned();

    let tbid = match params.get("tbid").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or empty 'tbid'".into(),
            )
        }
    };

    let chronon_start = match params.get("chronon_start").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_start' (u64)".into(),
            )
        }
    };

    let chronon_end = match params.get("chronon_end").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            return resp_error(
                server,
                id,
                jsonrpc::INVALID_PARAMS,
                "missing or invalid 'chronon_end' (u64)".into(),
            )
        }
    };

    let include_attestations = params
        .get("include_attestations")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let Some(store) = server.calendar_store() else {
        return resp_error(
            server,
            id,
            jsonrpc::INTERNAL_ERROR,
            "no calendar store configured (requires --persist-path)".into(),
        );
    };

    let result = store.get_chronon_chain(&tbid, chronon_start, chronon_end, include_attestations);

    match result {
        ChrononChainResult::Complete { records, coverage } => {
            let records_json: Vec<Value> = records
                .iter()
                .map(|r| serde_json::to_value(r.inner()).unwrap_or(Value::Null))
                .collect();
            resp_success(
                server,
                id,
                serde_json::json!({
                    "status": "complete",
                    "records": records_json,
                    "coverage": {
                        "requested": coverage.requested,
                        "returned": coverage.returned,
                    },
                }),
            )
        }
        ChrononChainResult::Partial {
            records,
            coverage,
            gaps,
        } => {
            let records_json: Vec<Value> = records
                .iter()
                .map(|r| serde_json::to_value(r.inner()).unwrap_or(Value::Null))
                .collect();
            let gaps_json: Vec<Value> = gaps
                .iter()
                .map(|g| serde_json::json!({"start": g.start, "end": g.end}))
                .collect();
            resp_success(
                server,
                id,
                serde_json::json!({
                    "status": "partial",
                    "records": records_json,
                    "coverage": {
                        "requested": coverage.requested,
                        "returned": coverage.returned,
                    },
                    "gaps": gaps_json,
                }),
            )
        }
        ChrononChainResult::None { coverage } => resp_success(
            server,
            id,
            serde_json::json!({
                "status": "none",
                "records": [],
                "coverage": {
                    "requested": coverage.requested,
                    "returned": coverage.returned,
                },
            }),
        ),
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
        TimeFamilyServer::new("127.0.0.1:0", 1_000_000_000).expect("failed to create server")
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
        assert!(
            result.get("foretis").is_some(),
            "v2 response must have foretis field"
        );
        assert!(
            result.get("signature").is_some(),
            "v2 response must have signature field"
        );
        assert!(
            result.get("signature_algorithm").is_some(),
            "v2 response must have signature_algorithm field"
        );
        let foretis = result.get("foretis").unwrap();
        assert!(foretis.get("chronon_number").is_some());
        assert!(foretis.get("content_hash").is_some());
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
    /// `UnverifiedSignatureEnvelope::<Foretis>::from_json_value` error surfaces correctly.
    #[test]
    fn handle_verify_malformed_foretis_returns_invalid_params() {
        let server = make_server();
        // 'foretis' is a string instead of the expected object shape.
        let params = serde_json::json!({
            "content": hex::encode(b"test"),
            "foretis": "this-is-not-a-foretis-object",
        });
        let resp = handle_verify(&server, params);
        let err = resp
            .error
            .expect("malformed foretis must produce an error response");
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
        let stamp_result = stamp_resp.result.unwrap();
        let foretis_json = stamp_result.get("foretis").unwrap().clone();
        let signature_hex = stamp_result
            .get("signature")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let sig_alg = stamp_result
            .get("signature_algorithm")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"verify-me"),
            "foretis": foretis_json,
            "signature": signature_hex,
            "signature_algorithm": sig_alg,
        });
        let verify_resp = handle_verify(&server, verify_params);
        assert!(
            verify_resp.error.is_none(),
            "verify should succeed: {:?}",
            verify_resp.error
        );
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
        let stamp_result = stamp_resp.result.unwrap();
        let foretis_json = stamp_result.get("foretis").unwrap().clone();
        let signature_hex = stamp_result
            .get("signature")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let sig_alg = stamp_result
            .get("signature_algorithm")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let verify_params = serde_json::json!({
            "content": hex::encode(b"tampered"),
            "foretis": foretis_json,
            "signature": signature_hex,
            "signature_algorithm": sig_alg,
        });
        let verify_resp = handle_verify(&server, verify_params);
        assert!(
            verify_resp.error.is_none(),
            "verify should succeed: {:?}",
            verify_resp.error
        );
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
        assert_eq!(
            result.get("all_valid").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            result.get("pairs_checked").and_then(|v| v.as_u64()),
            Some(0)
        );
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
        assert_eq!(
            result.get("all_valid").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            result.get("pairs_checked").and_then(|v| v.as_u64()),
            Some(2)
        );
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
        assert_eq!(
            result.get("all_valid").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            result.get("pairs_checked").and_then(|v| v.as_u64()),
            Some(3)
        );
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
        assert!(error
            .message
            .contains("FROST epoch data not yet implemented"));
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
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("accept")
        );
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
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("accept")
        );
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
        assert_eq!(
            result.get("accepted_count").and_then(|v| v.as_u64()),
            Some(0)
        );
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

    /// Regression: handle_route_stamp must return the same envelope shape as handle_stamp.
    ///
    /// When target_tbid matches the server's own TBID, handle_route_stamp delegates
    /// to handle_stamp, so the response must include foretis, signature, and
    /// signature_algorithm fields — not a bare Foretis object.
    #[test]
    fn handle_route_stamp_self_route_returns_envelope_shape() {
        let server = make_server();
        let my_tbid = server.get_tbid().to_hex();
        let params = serde_json::json!({
            "target_tbid": my_tbid,
            "content": hex::encode(b"route-test"),
            "echo": "route-shape-check"
        });
        let resp = handle_route_stamp(&server, params);
        assert!(
            resp.error.is_none(),
            "self-route should succeed: {:?}",
            resp.error
        );
        let result = resp.result.expect("expected result");
        assert!(result.get("foretis").is_some(), "must have foretis field");
        assert!(
            result.get("signature").is_some(),
            "must have signature field"
        );
        assert!(
            result.get("signature_algorithm").is_some(),
            "must have signature_algorithm field"
        );
        let foretis = result.get("foretis").unwrap();
        assert!(foretis.get("chronon_number").is_some());
        assert!(foretis.get("content_hash").is_some());
    }

    #[test]
    fn handle_storage_proof_request_missing_tbid_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"chronon_start": 0, "chronon_end": 10});
        let resp = handle_storage_proof_request(&server, params);
        let err = resp.error.expect("missing tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_storage_proof_request_missing_chronon_start_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"tbid": "abc", "chronon_end": 10});
        let resp = handle_storage_proof_request(&server, params);
        let err = resp.error.expect("missing chronon_start must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_storage_proof_request_no_calendar_store_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 0,
            "chronon_end": 10,
        });
        let resp = handle_storage_proof_request(&server, params);
        let err = resp.error.expect("no calendar store must error");
        assert_eq!(err.code, jsonrpc::INTERNAL_ERROR);
        assert!(err.message.contains("calendar store"));
    }

    #[test]
    fn handle_storage_proof_verify_missing_request_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"response": {}});
        let resp = handle_storage_proof_verify(&server, params);
        let err = resp.error.expect("missing request must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_storage_proof_verify_missing_response_returns_error() {
        let server = make_server();
        let params =
            serde_json::json!({"request": {"tbid": "abc", "chronon_start": 0, "chronon_end": 5}});
        let resp = handle_storage_proof_verify(&server, params);
        let err = resp.error.expect("missing response must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_storage_proof_verify_empty_proof_returns_verified_true() {
        let server = make_server();
        let params = serde_json::json!({
            "request": {
                "tbid": sample_tbid_hex(),
                "chronon_start": 0,
                "chronon_end": 5,
            },
            "response": {
                "blocks": [],
                "coverage_ratio": 0.0,
            },
            "known_roots": [],
        });
        let resp = handle_storage_proof_verify(&server, params);
        assert!(
            resp.error.is_none(),
            "empty proof should succeed: {:?}",
            resp.error
        );
        let result = resp.result.unwrap();
        assert_eq!(result.get("verified").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn handle_storage_proof_verify_invalid_known_root_hex_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "request": {
                "tbid": sample_tbid_hex(),
                "chronon_start": 0,
                "chronon_end": 5,
            },
            "response": {
                "blocks": [],
                "coverage_ratio": 0.0,
            },
            "known_roots": ["zzzz"],
        });
        let resp = handle_storage_proof_verify(&server, params);
        let err = resp.error.expect("invalid hex must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_storage_proof_verify_invalid_merkle_root_length_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "request": {
                "tbid": sample_tbid_hex(),
                "chronon_start": 0,
                "chronon_end": 5,
            },
            "response": {
                "blocks": [{
                    "block_id": 0,
                    "merkle_root": "abcd",
                    "leaves": [],
                    "siblings": [],
                    "leaf_count": 0,
                    "sibling_count": 0,
                    "n": 0,
                }],
                "coverage_ratio": 0.0,
            },
            "known_roots": [],
        });
        let resp = handle_storage_proof_verify(&server, params);
        let err = resp.error.expect("short merkle_root must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
        assert!(err.message.contains("merkle_root"));
    }

    // ── stamp_my_chronon / stamp_my_chronon_block tests ──────────────────

    #[test]
    fn handle_stamp_my_chronon_missing_requester_tbid_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"chronon_number": 1});
        let resp = handle_stamp_my_chronon(&server, params);
        let err = resp.error.expect("missing requester_tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_invalid_hex_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"requester_tbid": "zzzz", "chronon_number": 1});
        let resp = handle_stamp_my_chronon(&server, params);
        let err = resp.error.expect("invalid hex must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_missing_chronon_number_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"requester_tbid": "abcd"});
        let resp = handle_stamp_my_chronon(&server, params);
        let err = resp.error.expect("missing chronon_number must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_queue_not_started_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "requester_tbid": sample_tbid_hex(),
            "chronon_number": 42,
        });
        let resp = handle_stamp_my_chronon(&server, params);
        let err = resp.error.expect("queue not started must error");
        assert_eq!(err.code, jsonrpc::INTERNAL_ERROR);
        assert!(err.message.contains("task queue not started"));
    }

    #[test]
    fn handle_stamp_my_chronon_block_missing_requester_tbid_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"epoch_number": 3});
        let resp = handle_stamp_my_chronon_block(&server, params);
        let err = resp.error.expect("missing requester_tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_block_invalid_hex_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"requester_tbid": "zzzz", "epoch_number": 3});
        let resp = handle_stamp_my_chronon_block(&server, params);
        let err = resp.error.expect("invalid hex must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_block_missing_epoch_number_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"requester_tbid": "abcd"});
        let resp = handle_stamp_my_chronon_block(&server, params);
        let err = resp.error.expect("missing epoch_number must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_stamp_my_chronon_block_queue_not_started_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "requester_tbid": sample_tbid_hex(),
            "epoch_number": 3,
        });
        let resp = handle_stamp_my_chronon_block(&server, params);
        let err = resp.error.expect("queue not started must error");
        assert_eq!(err.code, jsonrpc::INTERNAL_ERROR);
        assert!(err.message.contains("task queue not started"));
    }

    // ── get_chronon tests ───────────────────────────────────────────────────

    #[test]
    fn handle_get_chronon_missing_tbid_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"chronon_number": 1});
        let resp = handle_get_chronon(&server, params);
        let err = resp.error.expect("missing tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_get_chronon_missing_chronon_number_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"tbid": "abc"});
        let resp = handle_get_chronon(&server, params);
        let err = resp.error.expect("missing chronon_number must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_get_chronon_no_calendar_store_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_number": 0,
        });
        let resp = handle_get_chronon(&server, params);
        let err = resp.error.expect("no calendar store must error");
        assert_eq!(err.code, jsonrpc::INTERNAL_ERROR);
        assert!(err.message.contains("calendar store"));
    }

    // ── get_chronon_chain tests ─────────────────────────────────────────────

    #[test]
    fn handle_get_chronon_chain_missing_tbid_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"chronon_start": 0, "chronon_end": 5});
        let resp = handle_get_chronon_chain(&server, params);
        let err = resp.error.expect("missing tbid must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_get_chronon_chain_missing_chronon_start_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"tbid": "abc", "chronon_end": 5});
        let resp = handle_get_chronon_chain(&server, params);
        let err = resp.error.expect("missing chronon_start must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_get_chronon_chain_missing_chronon_end_returns_error() {
        let server = make_server();
        let params = serde_json::json!({"tbid": "abc", "chronon_start": 0});
        let resp = handle_get_chronon_chain(&server, params);
        let err = resp.error.expect("missing chronon_end must error");
        assert_eq!(err.code, jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn handle_get_chronon_chain_no_calendar_store_returns_error() {
        let server = make_server();
        let params = serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 0,
            "chronon_end": 5,
        });
        let resp = handle_get_chronon_chain(&server, params);
        let err = resp.error.expect("no calendar store must error");
        assert_eq!(err.code, jsonrpc::INTERNAL_ERROR);
        assert!(err.message.contains("calendar store"));
    }
}
