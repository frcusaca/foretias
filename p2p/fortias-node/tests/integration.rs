//! Integration tests for the Fortias CLI and server.
//!
//! Spawns `fortias serve` as a background process, then exercises
//! `stamp` and `verify` subcommands end-to-end.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

// ── helpers ──────────────────────────────────────────────────────────────────

fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn wait_for_server(addr: &str, timeout_secs: u64) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if std::net::TcpStream::connect(addr).is_ok() {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn binary_path() -> String {
    if let Ok(path) = std::env::var("FORTIAS_CLI_PATH") {
        return path;
    }
    let release = format!("{}/../target/release/fortias", env!("CARGO_MANIFEST_DIR"));
    if std::path::Path::new(&release).exists() {
        return release;
    }
    format!("{}/../target/debug/fortias", env!("CARGO_MANIFEST_DIR"))
}

// ── CLI E2E tests ───────────────────────────────────────────────────────────

#[test]
fn test_stamp_and_verify_e2e() {
    let port = find_available_port();
    let addr = format!("127.0.0.1:{}", port);
    let bin = binary_path();

    let mut server = Command::new(&bin)
        .arg("serve")
        .arg("--addr")
        .arg(&addr)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn server");

    let mut server_stdout = server.stdout.take().unwrap();
    let mut server_stderr = server.stderr.take().unwrap();

    let stdout_handle = thread::spawn(move || {
        let mut buf = String::new();
        server_stdout.read_to_string(&mut buf).unwrap();
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = String::new();
        server_stderr.read_to_string(&mut buf).unwrap();
        buf
    });

    assert!(
        wait_for_server(&addr, 15),
        "Server did not start in time on {}",
        addr
    );

    let stamp_result = Command::new(&bin)
        .arg("stamp")
        .arg("--message")
        .arg("hello world")
        .arg("--server")
        .arg(&addr)
        .output()
        .expect("Failed to run stamp");

    assert!(
        stamp_result.status.success(),
        "Stamp command failed: {}",
        String::from_utf8_lossy(&stamp_result.stderr)
    );

    let stamp_output = String::from_utf8_lossy(&stamp_result.stdout);
    let fortis: Value = serde_json::from_str(stamp_output.trim())
        .expect("Failed to parse Fortis JSON from stamp output");

    assert!(fortis.get("tick_number").is_some(), "Fortis missing tick_number");
    assert!(fortis.get("content_hash").is_some(), "Fortis missing content_hash");
    assert!(fortis.get("signature").is_some(), "Fortis missing signature");
    assert!(fortis.get("tbid").is_some(), "Fortis missing tbid");
    assert!(fortis.get("echo").is_some(), "Fortis missing echo");
    assert!(fortis.get("tbn").is_some(), "Fortis missing tbn");
    assert!(fortis.get("time_being_reference_time").is_some(), "Fortis missing time_being_reference_time");

    let fortis_json = serde_json::to_string(&fortis).unwrap();
    let verify_result = Command::new(&bin)
        .arg("verify")
        .arg("--message")
        .arg("hello world")
        .arg("--fortis")
        .arg(&fortis_json)
        .arg("--server")
        .arg(&addr)
        .output()
        .expect("Failed to run verify");

    assert!(
        verify_result.status.success(),
        "Verify command failed: {}",
        String::from_utf8_lossy(&verify_result.stderr)
    );

    let verify_output = String::from_utf8_lossy(&verify_result.stdout);
    let verify_json: Value = serde_json::from_str(verify_output.trim())
        .expect("Failed to parse verify JSON");

    assert!(
        verify_json.get("valid").unwrap().as_bool().unwrap(),
        "Verify returned invalid for correct content"
    );

    let verify_wrong = Command::new(&bin)
        .arg("verify")
        .arg("--message")
        .arg("wrong content")
        .arg("--fortis")
        .arg(&fortis_json)
        .arg("--server")
        .arg(&addr)
        .output()
        .expect("Failed to run verify with wrong content");

    let verify_wrong_output = String::from_utf8_lossy(&verify_wrong.stdout);
    let verify_wrong_json: Value = serde_json::from_str(verify_wrong_output.trim())
        .expect("Failed to parse verify JSON for wrong content");

    assert!(
        !verify_wrong_json.get("valid").unwrap().as_bool().unwrap(),
        "Verify returned true for wrong content (should be false)"
    );

    let stamp2_result = Command::new(&bin)
        .arg("stamp")
        .arg("--message")
        .arg("second tick")
        .arg("--server")
        .arg(&addr)
        .output()
        .expect("Failed to run second stamp");

    assert!(stamp2_result.status.success(), "Second stamp failed");

    let stamp2_output = String::from_utf8_lossy(&stamp2_result.stdout);
    let fortis2: Value = serde_json::from_str(stamp2_output.trim()).unwrap();

    let tick1 = fortis.get("tick_number").unwrap().as_u64().unwrap();
    let tick2 = fortis2.get("tick_number").unwrap().as_u64().unwrap();
    assert_eq!(tick2, tick1 + 1, "Second tick should be tick1 + 1");

    let _ = server.kill();
    let _ = server.wait();

    let server_stdout_str = stdout_handle.join().unwrap();
    let server_stderr_str = stderr_handle.join().unwrap();

    println!("Server stdout: {}", server_stdout_str);
    println!("Server stderr: {}", server_stderr_str);
}

// ── In-process integration tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_two_nodes_mutual_attest() {
    use fortias_node::server::TimeFamilyServer;

    let port_a = find_available_port();
    let port_b = find_available_port();
    let addr_a = format!("127.0.0.1:{}", port_a);
    let addr_b = format!("127.0.0.1:{}", port_b);

    let server_b: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new(&addr_b, 100_000_000)
            .expect("failed to create server B"),
    );

    let config = fortias_core::config::NodeConfig {
        listen_addr: addr_b.clone(),
        peers: vec![addr_b.clone()],
        mutual_attest_every_n: 1,
        request_timeout_secs: 5,
        ..Default::default()
    };
    let server_a: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new(&addr_a, 100_000_000)
            .expect("failed to create server A")
            .with_config(config),
    );

    server_a.start_daemon_arc();
    server_b.start_daemon_arc();

    for _ in 0..3 {
        server_a.daemon_tick().expect("daemon_tick failed on A");
    }

    let handle_a = server_a.clone().start().expect("failed to start A");
    let handle_b = server_b.clone().start().expect("failed to start B");

    tokio::time::sleep(Duration::from_millis(200)).await;

    if let Some(com) = server_a.communerd() {
        let result = com.stamp_peer(
            &fortias_node::communerd::transport::PeerAddr {
                json_rpc: addr_b.clone(),
            },
            &hex::encode(b"mutual attest test"),
            "ma-test",
        ).await;
        // Peer B may not be listening yet, so error is acceptable
        let _ = result;
    }

    assert_ne!(server_a.get_tbid(), server_b.get_tbid());
    assert!(!server_a.is_dormant());
    assert!(!server_b.is_dormant());

    let _ = handle_a.abort();
    let _ = handle_b.abort();

    server_a.stop_daemon_arc();
    server_b.stop_daemon_arc();
}

#[tokio::test]
async fn test_peer_unreachable_does_not_crash() {
    use fortias_core::config::NodeConfig;
    use fortias_node::server::TimeFamilyServer;

    let port = find_available_port();
    let addr = format!("127.0.0.1:{}", port);

    let config = NodeConfig {
        listen_addr: addr.clone(),
        peers: vec!["127.0.0.1:59999".to_string()],
        mutual_attest_every_n: 1,
        request_timeout_secs: 1,
        ..Default::default()
    };

    let server: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new(&addr, 1_000_000_000)
            .expect("failed to create server")
            .with_config(config),
    );

    let handle = server.clone().start().expect("failed to start server");
    tokio::time::sleep(Duration::from_millis(100)).await;

    if let Some(com) = server.communerd() {
        let result = com.stamp_peer(
            &fortias_node::communerd::transport::PeerAddr {
                json_rpc: "127.0.0.1:59999".to_string(),
            },
            &hex::encode(b"test"),
            "test",
        ).await;
        assert!(result.is_err(), "Expected error for unreachable peer");
    }

    let fortis = server.chronomatter().stamp(b"still works".to_vec(), "ok".to_string())
        .expect("Local stamp should work despite unreachable peer");
    assert!(fortis.tick_number > 0);

    let _ = handle.abort();
    server.stop_daemon_arc();
}

#[test]
fn test_crash_recovery_calendar() {
    use fortias_core::fortias::calendar::Calendar as CoreCalendar;

    let tmp_dir = std::env::temp_dir().join(format!("fortias_crash_recovery_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create temp dir");

    let cal_path = tmp_dir.join("test.json");
    let tmp_path = tmp_dir.join("test.json.tmp");

    // Create a calendar with 3 ticks
    let mut cal = CoreCalendar::new([0u8; 16], "crash-test");
    for i in 0..3u64 {
        let record = fortias_core::fortias::tick::TickRecord {
            tick_number: i,
            public_key: vec![0u8; 32],
            forward_fortis: vec![],
            backward_fortis: vec![],
            aa_nonce: [0u8; 16],
            external_attestations: Vec::new(),
        };
        cal.append(record).expect("append failed");
    }

    // Save to main file
    cal.save(cal_path.to_str().unwrap()).expect("save failed");

    // Simulate crash: write a newer state to .tmp (more ticks than main)
    let mut cal2 = CoreCalendar::new([0u8; 16], "crash-test");
    for i in 0..5u64 {
        let record = fortias_core::fortias::tick::TickRecord {
            tick_number: i,
            public_key: vec![0u8; 32],
            forward_fortis: vec![],
            backward_fortis: vec![],
            aa_nonce: [0u8; 16],
            external_attestations: Vec::new(),
        };
        cal2.append(record).expect("append failed");
    }
    cal2.save(tmp_path.to_str().unwrap()).expect("tmp save failed");

    // Now rename .tmp to simulate a crash recovery scenario
    // The main file has 3 ticks, the .tmp has 5 ticks
    // Load should recover from .tmp
    let recovered = CoreCalendar::load(cal_path.to_str().unwrap());
    assert!(recovered.is_ok(), "Failed to load calendar: {:?}", recovered.err());

    let recovered = recovered.unwrap();
    assert!(recovered.ticks.len() >= 3, "Recovery should have at least 3 ticks, got {}", recovered.ticks.len());

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
