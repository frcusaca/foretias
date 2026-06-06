//! Integration tests for RPC codec frame-size bounds.
//!
//! Verifies that `ForetiasRpcCodec` rejects oversized length-prefixed
//! frames (e.g. a 4-GiB claim encoded as `0xFFFFFFFF`) without
//! allocating a correspondingly huge buffer, and that normal frames
//! are still accepted.

use foretias_server::communerd::p2p::rpc_protocol::ForetiasRpcCodec;
use futures::io::Cursor;
use libp2p::request_response::Codec;
use libp2p::StreamProtocol;

#[tokio::test]
async fn rpc_codec_rejects_malicious_frame_length() {
    let mut codec = ForetiasRpcCodec;

    // 4-byte length prefix = 0xFFFFFFFF (≈ 4 GiB) — no actual payload follows.
    // The codec must reject this before attempting to allocate or read.
    let mut buf = Vec::new();
    buf.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes());
    let mut cursor = Cursor::new(buf);

    let result = codec
        .read_request(&StreamProtocol::new("/foretias/rpc"), &mut cursor)
        .await;

    assert!(result.is_err(), "Expected error for oversized frame");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("too large"),
        "Error message should mention size: {}",
        err
    );
}

#[tokio::test]
async fn rpc_codec_accepts_normal_frame() {
    let mut codec = ForetiasRpcCodec;

    let payload = b"{}";
    let mut buf = Vec::new();
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(payload);
    let mut cursor = Cursor::new(buf);

    let result = codec
        .read_request(&StreamProtocol::new("/foretias/rpc"), &mut cursor)
        .await;

    assert!(result.is_ok(), "Expected OK for valid frame: {:?}", result);
    assert_eq!(result.unwrap(), payload);
}
