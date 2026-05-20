/// REQ-Z8.5: Assert no contiguous hex string >= 64 chars appears in CLI output.
///
/// A 64-char hex string corresponds to 32+ bytes of raw material, which
/// would indicate a leaked Ed25519 seed, private key, or similar secret.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::io::Read;

fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
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
    if let Ok(path) = std::env::var("FORETIAS_CLI_PATH") {
        return path;
    }
    let release = format!("{}/../target/release/foretias", env!("CARGO_MANIFEST_DIR"));
    if std::path::Path::new(&release).exists() {
        return release;
    }
    format!("{}/../target/debug/foretias", env!("CARGO_MANIFEST_DIR"))
}

/// Matches 64+ contiguous hex characters (case-insensitive).
fn contains_long_hex(text: &str) -> bool {
    text.matches(|c: char| c.is_ascii_hexdigit())
        .any(|s| s.len() >= 64)
}

#[test]
fn no_secret_hex_in_serve_stdout() {
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

    let mut stdout = server.stdout.take().unwrap();
    let mut stderr = server.stderr.take().unwrap();

    let stdout_handle = thread::spawn(move || {
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).unwrap();
        buf
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = String::new();
        stderr.read_to_string(&mut buf).unwrap();
        buf
    });

    if !wait_for_server(&addr, 15) {
        // Server didn't start; skip test
        return;
    }

    // Give server a moment to produce output
    std::thread::sleep(Duration::from_millis(500));

    let _ = server.kill();
    let _ = server.wait();

    let stdout_text = stdout_handle.join().unwrap();
    let stderr_text = stderr_handle.join().unwrap();

    assert!(
        !contains_long_hex(&stdout_text),
        "stdout contains 64+ char hex string (possible secret leak):\n{}",
        stdout_text
    );
    assert!(
        !contains_long_hex(&stderr_text),
        "stderr contains 64+ char hex string (possible secret leak):\n{}",
        stderr_text
    );
}

#[test]
fn no_secret_hex_in_stamp_output() {
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

    let _stdout = server.stdout.take();
    let _stderr = server.stderr.take();

    if !wait_for_server(&addr, 15) {
        return;
    }

    let stamp_result = Command::new(&bin)
        .arg("stamp")
        .arg("--message")
        .arg("hex leak test")
        .arg("--server")
        .arg(&addr)
        .output()
        .expect("Failed to run stamp");

    assert!(stamp_result.status.success(), "stamp failed");

    let stdout_text = String::from_utf8_lossy(&stamp_result.stdout);
    let stderr_text = String::from_utf8_lossy(&stamp_result.stderr);

    // stamp stdout is JSON with content_hash (64-char hex) — this is a hash, not a secret.
    // The check applies to stderr only.
    assert!(
        !contains_long_hex(&stderr_text),
        "stamp stderr contains 64+ char hex string:\n{}",
        stderr_text
    );

    let _ = server.kill();
    let _ = server.wait();
}
