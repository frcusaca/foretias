//! Libp2pTransport unit tests — COMBINED_GROUP4_SPEC.md §2 (Stream 4a).
//!
//! Verifies JSON-RPC serialization and closed-channel error handling
//! without requiring a live libp2p swarm.

use foretias_server::communerd::libp2p_transport::Libp2pTransport;
use foretias_server::communerd::p2p::swarm::SwarmCommand;
use foretias_server::communerd::transport::{PeerAddr, PeerTransport, TransportError};

/// Generate a random PeerId for test use.
fn test_peer_id() -> libp2p::PeerId {
    libp2p::PeerId::random()
}

fn make_peer() -> PeerAddr {
    PeerAddr {
        json_rpc: "127.0.0.1:4002".into(),
        peer_id: Some(test_peer_id()),
        last_seen_ns: 0,
    }
}

/// Test 2a.1: Request serialization writes a valid JSON-RPC 2.0 envelope.
///
/// Construct a Libp2pTransport, inject a fake cmd_tx channel, invoke
/// `stamp()`, and assert the `SwarmCommand::RequestResponse` request
/// field parses as a JSON-RPC 2.0 envelope with the expected method,
/// params, and id.
#[tokio::test]
async fn request_serialization_writes_jsonrpc_envelope() {
    let transport = Libp2pTransport::new(5);
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SwarmCommand>();
    assert!(transport.set_cmd_tx(cmd_tx));

    let peer = make_peer();
    let content_hex = "abcd";
    let echo = "echo";

    // Spawn a task that will receive the command on the cmd channel
    let cmd_handle = tokio::spawn(async move {
        cmd_rx.recv().await
    });

    // Call stamp — this sends on cmd_tx but will timeout waiting for
    // the oneshot reply (no one is driving the swarm). We ignore the
    // result here; we only care about what was sent on cmd_tx.
    let _result = transport.stamp(&peer, content_hex, echo).await;

    // Retrieve the command that was sent
    let Some(SwarmCommand::RequestResponse { request, .. }) = (match
        tokio::time::timeout(std::time::Duration::from_secs(2), cmd_handle).await
    {
        Ok(Ok(Some(cmd))) => Some(cmd),
        _ => None,
    }) else {
        panic!("expected SwarmCommand::RequestResponse on cmd channel");
    };

    // Assert JSON-RPC 2.0 envelope structure
    assert_eq!(request["jsonrpc"], "2.0", "must be JSON-RPC 2.0");
    assert_eq!(request["method"], "stamp", "method must be 'stamp'");
    assert_eq!(request["params"]["content"], content_hex);
    assert_eq!(request["params"]["echo"], echo);
    assert!(request["id"].is_number(), "id must be a JSON number");
}

/// Test 2a.2: Closed cmd channel returns a transport error.
///
/// Construct Libp2pTransport, inject cmd_tx, then immediately drop
/// the receiver side. Assert that calling `stamp()` returns
/// `TransportError::Connect` (the error mapped from a closed channel).
#[tokio::test]
async fn closed_cmd_channel_returns_transport_error() {
    let transport = Libp2pTransport::new(5);
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SwarmCommand>();
    assert!(transport.set_cmd_tx(cmd_tx));

    // Drop the receiver side BEFORE calling stamp — this causes
    // `cmd_tx.send(...)` to fail inside `rpc_call()`, returning
    // `TransportError::Connect("swarm channel closed")`.
    drop(cmd_rx);

    let peer = make_peer();
    let result = transport.stamp(&peer, "abcd", "echo").await;

    match result {
        Err(TransportError::Connect(msg)) => {
            assert!(
                msg.contains("swarm channel closed"),
                "expected 'swarm channel closed' in error message, got: {msg}"
            );
        }
        other => panic!(
            "expected TransportError::Connect, got: {:?}",
            other
        ),
    }
}
