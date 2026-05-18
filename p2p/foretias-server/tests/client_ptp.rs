//! Integration tests for ThinClient Level 2 (PtP Networked).
//!
//! Spawns a TimeFamilyServer in the background, then exercises
//! ThinClient::connect() stamp/verify/calendar_slice over Noise_XX TCP.

use std::sync::Arc;
use std::time::Duration;

use foretias_client::{Foretias, ForetiasError};
use foretias_server::server::TimeFamilyServer;

fn find_available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_for_server(addr: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("Server did not start in time on {addr}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn spawn_server() -> (Arc<TimeFamilyServer>, String) {
    let port = find_available_port();
    let addr = format!("127.0.0.1:{port}");
    let server = Arc::new(
        TimeFamilyServer::new(&addr, 1_000_000_000u64).expect("create server"),
    );
    let _handle = Arc::clone(&server).start().expect("start server");
    wait_for_server(&addr).await;
    (server, addr)
}

#[tokio::test]
async fn ptp_stamp_and_verify() {
    let (_server, addr) = spawn_server().await;

    let client = Foretias::connect_one(
        "ptp-client".into(),
        addr.clone(),
        None,
    )
    .expect("connect client");

    let foretis = client
        .stamp(b"hello world", "ptp-test".into())
        .await
        .expect("stamp succeeds");

    assert_eq!(foretis.echo, "ptp-test");
    assert!(!foretis.content_hash.is_empty());

    let valid = client
        .verify(b"hello world", &foretis)
        .await
        .expect("verify succeeds");
    assert!(valid, "stamp should verify as valid");
}

#[tokio::test]
async fn ptp_stamp_verify_roundtrip() {
    let (_server, addr) = spawn_server().await;

    let client = Foretias::connect_one(
        "roundtrip-client".into(),
        addr.clone(),
        None,
    )
    .expect("connect client");

    let content = b"roundtrip content data";
    let foretis = client
        .stamp(content, "roundtrip".into())
        .await
        .expect("stamp succeeds");

    let valid = client
        .verify(content, &foretis)
        .await
        .expect("verify succeeds");
    assert!(valid, "roundtrip should verify");

    let invalid = client
        .verify(b"tampered content", &foretis)
        .await
        .expect("verify tampered succeeds");
    assert!(!invalid, "tampered content should not verify");
}

#[tokio::test]
async fn ptp_calendar_slice() {
    let (_server, addr) = spawn_server().await;

    let client = Foretias::connect_one(
        "calendar-client".into(),
        addr.clone(),
        None,
    )
    .expect("connect client");

    client
        .stamp(b"first", "cal1".into())
        .await
        .expect("stamp 1");
    client
        .stamp(b"second", "cal2".into())
        .await
        .expect("stamp 2");

    let records = client
        .calendar_slice(0, 10)
        .await
        .expect("calendar slice succeeds");
    assert!(!records.is_empty(), "calendar should have records after stamps");
}

#[tokio::test]
async fn ptp_unreachable_server_returns_error() {
    let addr = "127.0.0.1:59999";

    let client = Foretias::connect_one(
        "unreachable-client".into(),
        addr.into(),
        None,
    )
    .expect("connect client");

    let result = client
        .stamp(b"hello", "noop".into())
        .await;

    assert!(
        matches!(result, Err(ForetiasError::Network(_))),
        "unreachable server should return Network error, got: {result:?}"
    );
}
