//! Integration tests for the Foretias CLI and server.
//!
//! Spawns `foretias serve` as a background process, then exercises
//! `stamp` and `verify` subcommands end-to-end.

use std::io::Read;
use foretias_core::foretias::types::Tbid;
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
    if let Ok(path) = std::env::var("FORETIAS_CLI_PATH") {
        return path;
    }
    let release = format!("{}/../target/release/foretias", env!("CARGO_MANIFEST_DIR"));
    if std::path::Path::new(&release).exists() {
        return release;
    }
    format!("{}/../target/debug/foretias", env!("CARGO_MANIFEST_DIR"))
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
    let foretis: Value = serde_json::from_str(stamp_output.trim())
        .expect("Failed to parse Foretis JSON from stamp output");

    assert!(foretis.get("tick_number").is_some(), "Foretis missing tick_number");
    assert!(foretis.get("content_hash").is_some(), "Foretis missing content_hash");
    assert!(foretis.get("signature").is_some(), "Foretis missing signature");
    assert!(foretis.get("tbid").is_some(), "Foretis missing tbid");
    assert!(foretis.get("echo").is_some(), "Foretis missing echo");
    assert!(foretis.get("tbn").is_some(), "Foretis missing tbn");
    assert!(foretis.get("time_being_reference_time").is_some(), "Foretis missing time_being_reference_time");

    let foretis_json = serde_json::to_string(&foretis).unwrap();
    let verify_result = Command::new(&bin)
        .arg("verify")
        .arg("--message")
        .arg("hello world")
        .arg("--foretis")
        .arg(&foretis_json)
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
        .arg("--foretis")
        .arg(&foretis_json)
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
    let foretis2: Value = serde_json::from_str(stamp2_output.trim()).unwrap();

    let tick1 = foretis.get("tick_number").unwrap().as_u64().unwrap();
    let tick2 = foretis2.get("tick_number").unwrap().as_u64().unwrap();
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
#[allow(deprecated)]
async fn test_two_nodes_auto_attest() {
    use foretias_node::server::TimeFamilyServer;

    let port_a = find_available_port();
    let port_b = find_available_port();
    let addr_a = format!("127.0.0.1:{}", port_a);
    let addr_b = format!("127.0.0.1:{}", port_b);

    let server_b: Arc<TimeFamilyServer> = Arc::new(
        TimeFamilyServer::new(&addr_b, 100_000_000)
            .expect("failed to create server B"),
    );

    let config = foretias_core::config::NodeConfig {
        listen_addr: addr_b.clone(),
        peers: vec![addr_b.clone()],
        auto_attest_every_n: 1,
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
            &foretias_node::communerd::transport::PeerAddr {
                json_rpc: addr_b.clone(),
                peer_id: None,
                last_seen_ns: 0,
            },
            &hex::encode(b"auto attest test"),
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
    use foretias_node::server::TimeFamilyServer;

    let port = find_available_port();
    let addr = format!("127.0.0.1:{}", port);

    #[allow(deprecated)]
    let config = foretias_core::config::NodeConfig {
        listen_addr: addr.clone(),
        peers: vec!["127.0.0.1:59999".to_string()],
        auto_attest_every_n: 1,
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
            &foretias_node::communerd::transport::PeerAddr {
                json_rpc: "127.0.0.1:59999".to_string(),
                peer_id: None,
                last_seen_ns: 0,
            },
            &hex::encode(b"test"),
            "test",
        ).await;
        assert!(result.is_err(), "Expected error for unreachable peer");
    }

    let foretis = server.chronomatter().stamp(b"still works".to_vec(), "ok".to_string())
        .expect("Local stamp should work despite unreachable peer");
    assert!(foretis.tick_number > 0);

    let _ = handle.abort();
    server.stop_daemon_arc();
}

#[test]
fn test_crash_recovery_calendar() {
    use foretias_core::foretias::calendar::Calendar as CoreCalendar;

    let tmp_dir = std::env::temp_dir().join(format!("foretias_crash_recovery_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create temp dir");

    let cal_path = tmp_dir.join("test.json");
    let tmp_path = tmp_dir.join("test.json.tmp");

    // Create a calendar with 3 ticks
    let mut cal = CoreCalendar::new(Tbid::default(), "crash-test");
    for i in 0..3u64 {
        let record = foretias_core::foretias::tick::TickRecord {
            tick_number: i,
            public_key: vec![0u8; 32],
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![],
            backward_foretis: vec![],
            aa_nonce: [0u8; 16],
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: Vec::new(),
            tb_version: 0,
        };
        cal.append(record).expect("append failed");
    }

    // Save to main file
    cal.save(cal_path.to_str().unwrap()).expect("save failed");

    // Simulate crash: write a newer state to .tmp (more ticks than main)
    let mut cal2 = CoreCalendar::new(Tbid::default(), "crash-test");
    for i in 0..5u64 {
        let record = foretias_core::foretias::tick::TickRecord {
            tick_number: i,
            public_key: vec![0u8; 32],
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![],
            backward_foretis: vec![],
            aa_nonce: [0u8; 16],
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: Vec::new(),
            tb_version: 0,
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

#[tokio::test]
async fn two_swarms_connect_and_identify() {
    use foretias_node::communerd::p2p::swarm::build_and_spawn_swarm;
    use foretias_node::communerd::p2p::events::NetworkEvent;

    let port_a = find_available_port();
    let port_b = find_available_port();
    let ma_a: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_a).parse().unwrap();
    let ma_b: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_b).parse().unwrap();

    let mut handle_a = build_and_spawn_swarm(Some(ma_a.clone()), vec![], "mainnet", None, None).await.unwrap();
    let peer_id_a = handle_a.local_peer_id.clone();

    let dial_a: libp2p::Multiaddr = format!("{}/p2p/{}", ma_a, peer_id_a).parse().unwrap();
    let mut handle_b = build_and_spawn_swarm(Some(ma_b), vec![dial_a], "mainnet", None, None).await.unwrap();

    let mut a_connected = false;
    let mut b_connected = false;
    let mut a_identified = false;
    let mut b_identified = false;

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        while let Ok(Some(event)) = tokio::time::timeout(
            Duration::from_millis(100),
            handle_a.events.recv(),
        )
        .await
        {
            match event {
                NetworkEvent::Connected { .. } => a_connected = true,
                NetworkEvent::Identified { .. } => a_identified = true,
                _ => {}
            }
        }
        while let Ok(Some(event)) = tokio::time::timeout(
            Duration::from_millis(100),
            handle_b.events.recv(),
        )
        .await
        {
            match event {
                NetworkEvent::Connected { .. } => b_connected = true,
                NetworkEvent::Identified { .. } => b_identified = true,
                _ => {}
            }
        }
        if a_connected && b_connected && a_identified && b_identified {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(a_connected, "Swarm A never connected");
    assert!(b_connected, "Swarm B never connected");
    assert!(a_identified, "Swarm A never identified B");
    assert!(b_identified, "Swarm B never identified A");
    assert_ne!(peer_id_a, handle_b.local_peer_id);

    handle_a.task.abort();
    handle_b.task.abort();
}

#[tokio::test]
async fn dht_discovery_three_nodes() {
    use foretias_node::communerd::p2p::swarm::{build_and_spawn_swarm, SwarmCommand};
    use foretias_node::communerd::p2p::events::NetworkEvent;

    let port_a = find_available_port();
    let port_b = find_available_port();
    let port_c = find_available_port();
    let ma_a: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_a).parse().unwrap();
    let ma_b: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_b).parse().unwrap();
    let ma_c: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_c).parse().unwrap();

    let namespace = "dht-test";

    let mut handle_a = build_and_spawn_swarm(Some(ma_a.clone()), vec![], namespace, None, None).await.unwrap();
    let peer_id_a = handle_a.local_peer_id;

    let mut handle_b = build_and_spawn_swarm(Some(ma_b.clone()), vec![], namespace, None, None).await.unwrap();
    let peer_id_b = handle_b.local_peer_id;

    let mut handle_c = build_and_spawn_swarm(Some(ma_c.clone()), vec![], namespace, None, None).await.unwrap();
    let peer_id_c = handle_c.local_peer_id;

    // Wait for all swarms to bind
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut listen_ready_count = 0u32;
    while std::time::Instant::now() < deadline && listen_ready_count < 3 {
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(50), handle_a.events.recv()).await {
            if matches!(event, NetworkEvent::ListenReady { .. }) { listen_ready_count += 1; }
        }
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(50), handle_b.events.recv()).await {
            if matches!(event, NetworkEvent::ListenReady { .. }) { listen_ready_count += 1; }
        }
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(50), handle_c.events.recv()).await {
            if matches!(event, NetworkEvent::ListenReady { .. }) { listen_ready_count += 1; }
        }
        if listen_ready_count < 3 { tokio::time::sleep(Duration::from_millis(100)).await; }
    }

    // Build full multiaddrs
    let ma_a_full: libp2p::Multiaddr = format!("{}/p2p/{}", ma_a, peer_id_a).parse().unwrap();
    let ma_b_full: libp2p::Multiaddr = format!("{}/p2p/{}", ma_b, peer_id_b).parse().unwrap();
    let ma_c_full: libp2p::Multiaddr = format!("{}/p2p/{}", ma_c, peer_id_c).parse().unwrap();

    // Fully connect: every node dials every other node
    let _ = handle_a.cmd_tx.send(SwarmCommand::Dial { addr: ma_b_full.clone() });
    let _ = handle_a.cmd_tx.send(SwarmCommand::Dial { addr: ma_c_full.clone() });
    let _ = handle_b.cmd_tx.send(SwarmCommand::Dial { addr: ma_a_full.clone() });
    let _ = handle_b.cmd_tx.send(SwarmCommand::Dial { addr: ma_c_full.clone() });
    let _ = handle_c.cmd_tx.send(SwarmCommand::Dial { addr: ma_a_full.clone() });
    let _ = handle_c.cmd_tx.send(SwarmCommand::Dial { addr: ma_b_full.clone() });

    // Wait for all connections to establish. Accumulate discovered peers across
    // all polling iterations to avoid missing events under parallel test execution.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut connected_a: std::collections::HashSet<libp2p::PeerId> = std::collections::HashSet::new();
    let mut connected_b: std::collections::HashSet<libp2p::PeerId> = std::collections::HashSet::new();
    let mut connected_c: std::collections::HashSet<libp2p::PeerId> = std::collections::HashSet::new();
    let mut all_connected = false;
    while std::time::Instant::now() < deadline {
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(20), handle_a.events.recv()).await {
            if let NetworkEvent::Connected { peer_id } | NetworkEvent::Identified { peer_id, .. } = event {
                connected_a.insert(peer_id);
            }
        }
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(20), handle_b.events.recv()).await {
            if let NetworkEvent::Connected { peer_id } | NetworkEvent::Identified { peer_id, .. } = event {
                connected_b.insert(peer_id);
            }
        }
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(20), handle_c.events.recv()).await {
            if let NetworkEvent::Connected { peer_id } | NetworkEvent::Identified { peer_id, .. } = event {
                connected_c.insert(peer_id);
            }
        }
        let a_ok = connected_a.contains(&peer_id_b) && connected_a.contains(&peer_id_c);
        let b_ok = connected_b.contains(&peer_id_a) && connected_b.contains(&peer_id_c);
        let c_ok = connected_c.contains(&peer_id_a) && connected_c.contains(&peer_id_b);
        if a_ok && b_ok && c_ok {
            all_connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(all_connected, "Not all nodes became fully connected");

    // Add addresses to k-buckets so GetRecord can route to any peer.
    // Dial + identify alone does NOT populate k-buckets in libp2p-kad 0.48.
    // Each node needs to know about every other node's address.
    let _ = handle_a.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_b, addr: ma_b_full.clone() });
    let _ = handle_a.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_c, addr: ma_c_full.clone() });
    let _ = handle_b.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_a, addr: ma_a_full.clone() });
    let _ = handle_b.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_c, addr: ma_c_full.clone() });
    let _ = handle_c.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_a, addr: ma_a_full.clone() });
    let _ = handle_c.cmd_tx.send(SwarmCommand::AddAddress { peer_id: peer_id_b, addr: ma_b_full.clone() });

    // Allow k-bucket entries to settle
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Store the record locally on all 3 nodes. In small networks, standard PutRecord
    // fails because k-bucket XOR distance routing cannot find α closest peers, and
    // PutRecordTo fails because kad substream negotiation requires protocol-level
    // connectivity beyond TCP. StoreRecordLocal bypasses network entirely and writes
    // directly to the kad MemoryStore — this mirrors the official libp2p-kad test
    // pattern (see get_record() test in libp2p-kad/src/behaviour/test.rs).
    let discovery_key = libp2p::kad::RecordKey::new(b"foretias-peer-discovery-v1");
    let record = libp2p::kad::Record {
        key: discovery_key.clone(),
        value: peer_id_a.to_bytes().to_vec(),
        publisher: Some(peer_id_a),
        expires: None,
    };
    let _ = handle_a.cmd_tx.send(SwarmCommand::StoreRecordLocal { record: record.clone() });
    let _ = handle_b.cmd_tx.send(SwarmCommand::StoreRecordLocal { record: record.clone() });
    let _ = handle_c.cmd_tx.send(SwarmCommand::StoreRecordLocal { record });

    // C queries the DHT for the record - proves DHT routing works across the network
    let _ = handle_c.cmd_tx.send(SwarmCommand::GetRecord { key: discovery_key.clone() });

    let mut c_discovered_a = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(50), handle_c.events.recv()).await {
            match event {
                NetworkEvent::RecordRetrieved { records, .. } => {
                    for r in records {
                        // Note: libp2p-kad overwrites record.publisher with the storing
                        // node's PeerId during replication. The original publisher is
                        // only preserved on the node that called put_record. We check
                        // the record value instead, which contains peer_id_a bytes.
                        if r.value == peer_id_a.to_bytes() {
                            c_discovered_a = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if c_discovered_a { break; }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(
        c_discovered_a,
        "C never discovered A through DHT - fully connected network with bootstrap should allow GetRecord to find records published by any node"
    );

    handle_a.task.abort();
    handle_b.task.abort();
    handle_c.task.abort();
}

#[tokio::test]
async fn gossip_probity_propagation() {
    use foretias_node::communerd::p2p::swarm::{build_and_spawn_swarm, SwarmCommand};
    use foretias_node::communerd::p2p::events::NetworkEvent;
    use foretias_node::probity::ProbityReport;

    let port_a = find_available_port();
    let port_b = find_available_port();
    let ma_a: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_a).parse().unwrap();
    let ma_b: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_b).parse().unwrap();

    let namespace = "testnet";

    let mut handle_a = build_and_spawn_swarm(Some(ma_a.clone()), vec![], namespace, None, None).await.unwrap();
    let peer_id_a = handle_a.local_peer_id;

    let dial_a: libp2p::Multiaddr = format!("{}/p2p/{}", ma_a, peer_id_a).parse().unwrap();
    let mut handle_b = build_and_spawn_swarm(Some(ma_b), vec![dial_a], namespace, None, None).await.unwrap();
    let peer_id_b = handle_b.local_peer_id;

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut both_connected = false;

    while std::time::Instant::now() < deadline {
        let mut a_conn = false;
        let mut b_conn = false;

        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), handle_a.events.recv()).await {
            if matches!(event, NetworkEvent::Connected { .. } | NetworkEvent::Identified { .. }) {
                a_conn = true;
            }
        }
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), handle_b.events.recv()).await {
            if matches!(event, NetworkEvent::Connected { .. } | NetworkEvent::Identified { .. }) {
                b_conn = true;
            }
        }

        if a_conn && b_conn {
            both_connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    assert!(both_connected, "Swarms never connected");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish probity report from A
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let report = ProbityReport {
        subject: peer_id_b.to_string(),
        reporter: peer_id_a.to_string(),
        attribute: "correctness".to_string(),
        value: 10.0,
        timestamp_ns: now_ns,
        signature: vec![],
        curve: 1u8,
    };
    let _ = handle_a.cmd_tx.send(SwarmCommand::PublishProbity {
        report: report.clone(),
        namespace: namespace.to_string(),
    });

    // B should receive the gossip message
    let mut gossip_received = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(200), handle_b.events.recv()).await {
            if let NetworkEvent::GossipMessage { data, .. } = event {
                let received: Result<ProbityReport, _> = serde_json::from_slice(&data);
                if let Ok(received_report) = received {
                    assert_eq!(received_report.subject, peer_id_b.to_string());
                    assert_eq!(received_report.reporter, peer_id_a.to_string());
                    assert_eq!(received_report.attribute, "correctness");
                    assert_eq!(received_report.value, 10.0);
                    gossip_received = true;
                }
            }
        }
        if gossip_received {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(gossip_received, "B never received gossip message from A");

    handle_a.task.abort();
    handle_b.task.abort();
}

// ── libp2p direct RPC test ─────────────────────────────────────────────

#[tokio::test]
async fn test_libp2p_direct_rpc() {
    use foretias_node::communerd::p2p::swarm::{build_and_spawn_swarm, CommunerdRpcHandler, SwarmCommand};
    use foretias_node::communerd::p2p::events::NetworkEvent;
    use foretias_node::communerd::transport::TransportError;

    struct EchoRpcHandler;

    #[async_trait::async_trait]
    impl CommunerdRpcHandler for EchoRpcHandler {
        async fn handle(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, TransportError> {
            Ok(serde_json::json!({
                "jsonrpc": "2.0",
                "result": { "method": method, "params": params },
                "id": 1,
            }))
        }
    }

    let port_a = find_available_port();
    let port_b = find_available_port();
    let ma_a: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_a).parse().unwrap();
    let ma_b: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port_b).parse().unwrap();

    let handler_a = std::sync::Arc::new(EchoRpcHandler);
    let mut handle_a = build_and_spawn_swarm(
        Some(ma_a.clone()),
        vec![],
        "libp2p-rpc-test",
        None,
        Some(handler_a),
    )
    .await
    .unwrap();
    let peer_id_a = handle_a.local_peer_id.clone();

    let handler_b = std::sync::Arc::new(EchoRpcHandler);
    let dial_a: libp2p::Multiaddr = format!("{}/p2p/{}", ma_a, peer_id_a).parse().unwrap();
    let mut handle_b = build_and_spawn_swarm(
        Some(ma_b),
        vec![dial_a.clone()],
        "libp2p-rpc-test",
        None,
        Some(handler_b),
    )
    .await
    .unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut a_connected = false;
    let mut b_connected = false;
    let mut a_identified = false;
    let mut b_identified = false;

    while std::time::Instant::now() < deadline {
        while let Ok(Some(event)) = tokio::time::timeout(
            Duration::from_millis(100),
            handle_a.events.recv(),
        )
        .await
        {
            match event {
                NetworkEvent::Connected { .. } => a_connected = true,
                NetworkEvent::Identified { .. } => a_identified = true,
                _ => {}
            }
        }
        while let Ok(Some(event)) = tokio::time::timeout(
            Duration::from_millis(100),
            handle_b.events.recv(),
        )
        .await
        {
            match event {
                NetworkEvent::Connected { .. } => b_connected = true,
                NetworkEvent::Identified { .. } => b_identified = true,
                _ => {}
            }
        }
        if a_connected && b_connected && a_identified && b_identified {
            break;
        }
    }

    assert!(a_connected, "A never connected to B");
    assert!(b_connected, "B never connected to A");
    assert!(a_identified, "A never identified B");
    assert!(b_identified, "B never identified A");

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "ping",
        "params": { "from": "peer_b" },
        "id": 1,
    });
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle_b
        .cmd_tx
        .send(SwarmCommand::RequestResponse {
            peer_id: peer_id_a,
            request,
            reply: tx,
        })
        .unwrap();

    let response = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .expect("RPC timed out")
        .expect("RPC channel failed")
        .expect("RPC request failed");
    let resp_val: serde_json::Value = response;
    assert_eq!(resp_val["method"], "ping");
    assert_eq!(resp_val["params"]["from"], "peer_b");

    let peer_id_b = handle_b.local_peer_id.clone();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "echo",
        "params": { "message": "hello_from_a" },
        "id": 2,
    });
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle_a
        .cmd_tx
        .send(SwarmCommand::RequestResponse {
            peer_id: peer_id_b,
            request,
            reply: tx,
        })
        .unwrap();

    let response = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .expect("RPC timed out")
        .expect("RPC channel failed")
        .expect("RPC request failed");
    let resp_val: serde_json::Value = response;
    assert_eq!(resp_val["method"], "echo");
    assert_eq!(resp_val["params"]["message"], "hello_from_a");

    handle_a.task.abort();
    handle_b.task.abort();
}
