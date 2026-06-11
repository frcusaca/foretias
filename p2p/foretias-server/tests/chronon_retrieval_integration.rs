//! Integration tests for chronon retrieval and FB verification.
//!
//! These tests start a real `TimeFamilyServer` with an HTTP JSON-RPC endpoint
//! and exercise the `get_chronon` and `get_chronon_chain` RPC methods end-to-end.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        if try_jsonrpc_call(addr, "ping", serde_json::json!({}))
            .await
            .is_ok()
        {
            return;
        }
        if tokio::time::Instant::now() > deadline {
            panic!(
                "HTTP server did not become ready on {} within {:?}",
                addr, timeout
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A valid hex-encoded TBID (192 hex chars = 96 bytes).
fn sample_tbid_hex() -> String {
    "aa".repeat(96)
}

/// Build a minimal `ChrononRecord` wrapped as `Externalized<ChrononRecord>`.
fn make_externalized_tick(
    chronon_number: u64,
) -> foretias_core::foretias::clean_auth::Externalized<foretias_core::foretias::ChrononRecord> {
    use foretias_core::foretias::clean_auth::CleanAuthenticated;
    use foretias_core::foretias::ChrononRecord;

    let record = ChrononRecord::builder()
        .chronon_number(chronon_number)
        .public_key(vec![0u8; 32].into())
        .forward_foretis(vec![].into())
        .backward_foretis(vec![].into())
        .aa_nonce([0u8; 16].into())
        .tb_version(0)
        .build()
        .unwrap();
    CleanAuthenticated::<ChrononRecord>::from_trusted(record).externalize()
}

/// Start a server with a CalendarStore backed by a temporary directory, then
/// pre-populate the encrypted JSONL store with `n_ticks` test chronon records
/// (chronon numbers 0..n_ticks). Returns (http_addr, persist_dir, handle).
async fn start_server_with_calendar_data(
    n_ticks: u64,
) -> (String, std::path::PathBuf, tokio::task::JoinHandle<()>) {
    use foretias_core::foretias::clean_auth::Externalized;
    use foretias_core::foretias::ChrononRecord;
    use foretias_server::calendar_store::encrypted_jsonl::EncryptedJsonlCalendarStore;
    use foretias_server::server::TimeFamilyServer;

    let listen_port = find_available_port();
    let listen_addr = format!("127.0.0.1:{}", listen_port);
    let http_port = find_available_port();
    let http_addr = format!("127.0.0.1:{}", http_port);

    // Create a unique temporary directory for persistence.
    let persist_dir = std::env::temp_dir().join(format!(
        "foretias-chronon-retrieval-{}-{}",
        std::process::id(),
        listen_port,
    ));
    std::fs::create_dir_all(&persist_dir).expect("failed to create persist dir");

    // Create the server with persist_path so it has a CalendarStore.
    let server: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new_with_persist(&listen_addr, 100_000_000, Some(persist_dir.clone()))
            .expect("failed to create server"),
    );

    // Get the same CryptoServer the server uses for sealing/unsealing.
    let crypto = server.chronomatter().crypto_server();

    // Create an EncryptedJsonlCalendarStore pointing to the same file the
    // server's CalendarStore reads from, using the same crypto key.
    let store_path = persist_dir.join("calendar_store.jsonl");
    let encrypted_store = EncryptedJsonlCalendarStore::new(store_path, crypto);

    // Populate with n_ticks chronon records as a single block.
    // Chronon numbers start at 1 (genesis) per tick.rs validation.
    if n_ticks > 0 {
        let ticks: Vec<Externalized<ChrononRecord>> =
            (1..=n_ticks).map(make_externalized_tick).collect();
        encrypted_store
            .append_block(ticks)
            .expect("failed to append block to calendar store");
    }

    // Start the HTTP JSON-RPC endpoint.
    let handle = server
        .clone()
        .start_http(&http_addr)
        .expect("failed to start HTTP server");

    wait_for_http(&http_addr, Duration::from_secs(10)).await;

    (http_addr, persist_dir, handle)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_chronon_found() {
    // Start a server with 5 chronon records (chronon 0..4).
    let (http_addr, persist_dir, handle) = start_server_with_calendar_data(5).await;

    // Query chronon 3 — should be found.
    let resp = jsonrpc_call(
        &http_addr,
        "get_chronon",
        serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_number": 3,
        }),
    )
    .await;

    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "get_chronon should succeed, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("found"),
        "expected status 'found', got: {}",
        result
    );

    let record = result
        .get("record")
        .expect("response missing 'record' field");
    // The ChrononRecord serializes chronon_number as "tick_number" via serde rename.
    assert_eq!(
        record.get("tick_number").and_then(|v| v.as_u64()),
        Some(3),
        "record should have tick_number 3, got: {}",
        record
    );

    // Also query chronon 1 (first/genesis) and chronon 5 (last).
    for cn in [1u64, 5] {
        let resp = jsonrpc_call(
            &http_addr,
            "get_chronon",
            serde_json::json!({
                "tbid": sample_tbid_hex(),
                "chronon_number": cn,
            }),
        )
        .await;
        let result = resp.get("result").expect("missing result");
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("found"),
            "chronon {} should be found",
            cn
        );
        let record = result.get("record").expect("missing record");
        assert_eq!(record.get("tick_number").and_then(|v| v.as_u64()), Some(cn),);
    }

    handle.abort();
    let _ = std::fs::remove_dir_all(&persist_dir);
}

#[tokio::test]
async fn get_chronon_not_found() {
    // Start a server with 3 chronon records (chronon 0..2).
    let (http_addr, persist_dir, handle) = start_server_with_calendar_data(3).await;

    // Query chronon 100 — does not exist.
    let resp = jsonrpc_call(
        &http_addr,
        "get_chronon",
        serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_number": 100,
        }),
    )
    .await;

    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "get_chronon should succeed even for missing chronon, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("not_found"),
        "expected status 'not_found', got: {}",
        result
    );

    // Also query chronon 4 — just past the end (data has 1..3).
    let resp = jsonrpc_call(
        &http_addr,
        "get_chronon",
        serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_number": 4,
        }),
    )
    .await;
    let result = resp.get("result").expect("missing result");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("not_found"),
        "chronon 4 should not be found when only 1..3 exist"
    );

    handle.abort();
    let _ = std::fs::remove_dir_all(&persist_dir);
}

#[tokio::test]
async fn get_chronon_chain_complete() {
    // Start a server with 10 chronon records (chronon 0..9).
    let (http_addr, persist_dir, handle) = start_server_with_calendar_data(10).await;

    // Query chronon 2..7 — all 6 records should be found.
    let resp = jsonrpc_call(
        &http_addr,
        "get_chronon_chain",
        serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 2,
            "chronon_end": 7,
        }),
    )
    .await;

    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "get_chronon_chain should succeed, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("complete"),
        "expected status 'complete', got: {}",
        result
    );

    let records = result
        .get("records")
        .and_then(|v| v.as_array())
        .expect("missing 'records' array");
    assert_eq!(
        records.len(),
        6,
        "expected 6 records, got {}",
        records.len()
    );

    // Verify chronon numbers are in order: 2, 3, 4, 5, 6, 7.
    for (i, rec) in records.iter().enumerate() {
        assert_eq!(
            rec.get("tick_number").and_then(|v| v.as_u64()),
            Some(2 + i as u64),
            "record {} should have tick_number {}",
            i,
            2 + i
        );
    }

    let coverage = result.get("coverage").expect("missing 'coverage'");
    assert_eq!(coverage.get("requested").and_then(|v| v.as_u64()), Some(6),);
    assert_eq!(coverage.get("returned").and_then(|v| v.as_u64()), Some(6),);

    handle.abort();
    let _ = std::fs::remove_dir_all(&persist_dir);
}

#[tokio::test]
async fn get_chronon_chain_partial() {
    // Start a server with 5 chronon records (chronon 0..4).
    let (http_addr, persist_dir, handle) = start_server_with_calendar_data(5).await;

    // Query chronon 1..10 — only 1..5 exist, so result is partial.
    let resp = jsonrpc_call(
        &http_addr,
        "get_chronon_chain",
        serde_json::json!({
            "tbid": sample_tbid_hex(),
            "chronon_start": 1,
            "chronon_end": 10,
        }),
    )
    .await;

    assert!(
        resp.get("error").is_none() || resp["error"].is_null(),
        "get_chronon_chain should succeed, got error: {:?}",
        resp.get("error")
    );
    let result = resp.get("result").expect("response missing 'result' field");
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("partial"),
        "expected status 'partial', got: {}",
        result
    );

    let records = result
        .get("records")
        .and_then(|v| v.as_array())
        .expect("missing 'records' array");
    assert_eq!(
        records.len(),
        5,
        "expected 5 records, got {}",
        records.len()
    );

    // Verify the returned records are chronon 1..5.
    for (i, rec) in records.iter().enumerate() {
        assert_eq!(
            rec.get("tick_number").and_then(|v| v.as_u64()),
            Some(1 + i as u64),
        );
    }

    let coverage = result.get("coverage").expect("missing 'coverage'");
    assert_eq!(
        coverage.get("requested").and_then(|v| v.as_u64()),
        Some(10),
        "requested should be 10 (1..10 inclusive)"
    );
    assert_eq!(
        coverage.get("returned").and_then(|v| v.as_u64()),
        Some(5),
        "returned should be 5"
    );

    // Verify gaps are reported.
    let gaps = result
        .get("gaps")
        .and_then(|v| v.as_array())
        .expect("missing 'gaps' array");
    assert_eq!(gaps.len(), 1, "expected 1 gap, got {}", gaps.len());
    assert_eq!(
        gaps[0].get("start").and_then(|v| v.as_u64()),
        Some(6),
        "gap should start at 6"
    );
    assert_eq!(
        gaps[0].get("end").and_then(|v| v.as_u64()),
        Some(11),
        "gap end is exclusive (6..11), should be 11"
    );

    handle.abort();
    let _ = std::fs::remove_dir_all(&persist_dir);
}
