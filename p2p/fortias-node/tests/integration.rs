//! Integration tests for the Fortias CLI.
//!
//! Spawns `fortias serve` as a background process, then exercises
//! `stamp` and `verify` subcommands end-to-end.

use std::io::Read;
use std::process::{Command, Stdio};
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
    // Allow overriding via env var (useful for CI)
    if let Ok(path) = std::env::var("FORTIAS_CLI_PATH") {
        return path;
    }
    let release = format!("{}/../target/release/fortias", env!("CARGO_MANIFEST_DIR"));
    if std::path::Path::new(&release).exists() {
        return release;
    }
    format!("{}/../target/debug/fortias", env!("CARGO_MANIFEST_DIR"))
}

// ── tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_stamp_and_verify_e2e() {
    let port = find_available_port();
    let addr = format!("127.0.0.1:{}", port);
    let bin = binary_path();

    // 1. Spawn server
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

    // 2. Wait for server to be ready
    assert!(
        wait_for_server(&addr, 15),
        "Server did not start in time on {}",
        addr
    );

    // 3. Stamp "hello world"
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

    // Verify the Fortis has expected fields
    assert!(fortis.get("tick_number").is_some(), "Fortis missing tick_number");
    assert!(fortis.get("content_hash").is_some(), "Fortis missing content_hash");
    assert!(fortis.get("signature").is_some(), "Fortis missing signature");
    assert!(fortis.get("tbid").is_some(), "Fortis missing tbid");
    assert!(fortis.get("echo").is_some(), "Fortis missing echo");
    assert!(fortis.get("tbn").is_some(), "Fortis missing tbn");
    assert!(fortis.get("time_being_reference_time").is_some(), "Fortis missing time_being_reference_time");

    // 4. Verify "hello world" with the Fortis
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

    // 5. Verify wrong content fails
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

    // 6. Stamp a second time to test tick increment
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

    // 7. Cleanup
    let _ = server.kill();
    let _ = server.wait();

    let server_stdout_str = stdout_handle.join().unwrap();
    let server_stderr_str = stderr_handle.join().unwrap();

    println!("Server stdout: {}", server_stdout_str);
    println!("Server stderr: {}", server_stderr_str);
}
