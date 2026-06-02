//! Integration tests for the GNF/FB mutual attestation RPC methods.
//!
//! These tests start a real `TimeFamilyServer` with an HTTP JSON-RPC endpoint
//! and exercise the `stamp_my_chronon` and `stamp_my_chronon_block` RPC
//! methods end-to-end.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Find an available TCP port by binding to port 0 and reading the assigned port.
fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Send a JSON-RPC POST request via raw HTTP/1.1 over TCP and parse the
/// JSON body from the response. Returns `Err` on any I/O or parse failure
/// so callers can retry (e.g. while waiting for the server to start).
async fn try_jsonrpc_call(addr: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });
    let body_bytes = serde_json::to_vec(&body).map_err(|e| format!("serialize: {}", e))?;
    let content_length = body_bytes.len();

    let request = format!(
        "POST /jsonrpc HTTP/1.1\r\n\
         Host: {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        addr, content_length,
    );

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| format!("TCP connect: {}", e))?;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("write headers: {}", e))?;
    stream
        .write_all(&body_bytes)
        .await
        .map_err(|e| format!("write body: {}", e))?;
    stream.flush().await.map_err(|e| format!("flush: {}", e))?;

    // Read the full response.
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|e| format!("read response: {}", e))?;
    let response_str = String::from_utf8_lossy(&response);

    // Split headers and body. The JSON-RPC response lives after the
    // first blank line (`\r\n\r\n`).
    let body_start = response_str
        .find("\r\n\r\n")
        .ok_or_else(|| "HTTP response missing header/body separator".to_string())?
        + 4;
    let json_body = &response_str[body_start..];

    serde_json::from_str(json_body.trim())
        .map_err(|e| format!("parse JSON: {}\nraw: {}", e, json_body))
}

/// Send a JSON-RPC POST request and panic on failure. For use in test bodies
/// where the server is already known to be running.
async fn jsonrpc_call(addr: &str, method: &str, params: Value) -> Value {
    try_jsonrpc_call(addr, method, params)
        .await
        .expect("jsonrpc_call failed")
}

/// Wait until the HTTP JSON-RPC endpoint responds to a ping, up to `timeout`.
async fn wait_for_http(addr: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if try_jsonrpc_call(addr, "ping", serde_json::json!({})).await.is_ok() {
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("HTTP server did not become ready on {} within {:?}", addr, timeout);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A valid hex-encoded TBID (192 hex chars = 96 bytes).
fn sample_tbid_hex() -> String {
    "aa".repeat(96)
}

/// Start a server with its calendar task queue active, listening for HTTP
/// requests on a random port. Returns (http_addr, server_arc, join_handle).
async fn start_server_with_task_queue() -> (String, Arc<foretias_server::server::TimeFamilyServer>, tokio::task::JoinHandle<()>) {
    use foretias_server::server::TimeFamilyServer;

    let listen_port = find_available_port();
    let listen_addr = format!("127.0.0.1:{}", listen_port);
    let http_port = find_available_port();
    let http_addr = format!("127.0.0.1:{}", http_port);

    let server: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new(&listen_addr, 100_000_000)
            .expect("failed to create server"),
    );

    // Start the calendar task queue so enqueue_task succeeds.
    server.calendar().start_task_queue();

    let handle = server
        .clone()
        .start_http(&http_addr)
        .expect("failed to start HTTP server");

    wait_for_http(&http_addr, Duration::from_secs(10)).await;

    (http_addr, server, handle)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn stamp_my_chronon_queues_task() {
    let (http_addr, _server, handle) = start_server_with_task_queue().await;

    let resp = jsonrpc_call(
        &http_addr,
        "stamp_my_chronon",
        serde_json::json!({
            "requester_tbid": sample_tbid_hex(),
            "chronon_number": 42,
        }),
    )
    .await;

    // Expect success: { "result": { "status": "queued" } }
    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "stamp_my_chronon should succeed, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("queued"),
        "expected {{\"status\": \"queued\"}}, got: {}",
        result
    );

    handle.abort();
}

#[tokio::test]
async fn stamp_my_chronon_block_queues_task() {
    let (http_addr, _server, handle) = start_server_with_task_queue().await;

    let resp = jsonrpc_call(
        &http_addr,
        "stamp_my_chronon_block",
        serde_json::json!({
            "requester_tbid": sample_tbid_hex(),
            "epoch_number": 7,
        }),
    )
    .await;

    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "stamp_my_chronon_block should succeed, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("queued"),
        "expected {{\"status\": \"queued\"}}, got: {}",
        result
    );

    handle.abort();
}

#[tokio::test]
async fn stamp_my_chronon_invalid_tbid() {
    let (http_addr, _server, handle) = start_server_with_task_queue().await;

    let resp = jsonrpc_call(
        &http_addr,
        "stamp_my_chronon",
        serde_json::json!({
            "requester_tbid": "zzzz",
            "chronon_number": 1,
        }),
    )
    .await;

    // Expect error: { "error": { "code": -32602, "message": "invalid hex in 'requester_tbid'" } }
    assert!(
        resp.get("error").is_some() && !resp["error"].is_null(),
        "stamp_my_chronon with invalid TBID should return error, got: {}",
        resp
    );
    let error = resp.get("error").unwrap();
    assert_eq!(
        error.get("code").and_then(|v| v.as_i64()),
        Some(-32602),
        "expected INVALID_PARAMS error code (-32602), got: {}",
        error
    );
    let message = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        message.contains("invalid hex"),
        "error message should mention invalid hex, got: {}",
        message
    );

    handle.abort();
}
