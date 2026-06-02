//! Integration tests for the proof-of-storage system.
//!
//! Exercises [`CalendarStore::prove_storage`] and [`verify_storage_proof`]
//! with full coverage, partial coverage, and backward-compatible
//! (pre-v0.7) block formats.

use std::sync::Arc;

use foretias_core::crypto_server;
use foretias_core::foretias::clean_auth::{CleanAuthenticated, Externalized};
use foretias_core::foretias::{ChrononRecord, Tbid};

use foretias_server::calendar_store::encrypted_jsonl::{
    CalendarBlock, EncryptedJsonlCalendarStore,
};
use foretias_server::calendar_store::{verify_storage_proof, CalendarStore, StorageProofRequest};

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_server() -> Arc<dyn crypto_server::CryptoServer> {
    let server: Box<dyn crypto_server::CryptoServer> =
        crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519)
            .expect("failed to create software crypto server");
    Arc::from(server)
}

fn make_tick(chronon_number: u64) -> Externalized<ChrononRecord> {
    let record = ChrononRecord {
        chronon_number,
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
    CleanAuthenticated::<ChrononRecord>::from_trusted(record).externalize()
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Server holds chronons 1–100 across three blocks, challenge range 30–70.
/// Every requested tick is present → coverage_ratio == 1.0, verified == true.
///
/// The block containing the challenge range (30–70) is stored as a single
/// block whose first tick equals the challenge start, so the Merkle range
/// proof starts at index 0 within the block.  This is required because
/// `verify_storage_proof` currently passes `start=0` to the underlying
/// Merkle verifier.
#[test]
fn full_coverage_scenario() {
    let tmp = std::env::temp_dir().join(format!("foretias-pos-full-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let path = tmp.join("calendar.jsonl");
    let server = make_server();
    let store = EncryptedJsonlCalendarStore::new(path, server);

    // Block 0: ticks 1–29 (before the challenge window)
    let ticks_a: Vec<Externalized<ChrononRecord>> = (1..=29u64).map(make_tick).collect();
    store.append_block(ticks_a).unwrap();

    // Block 1: ticks 30–70 (exactly the challenge window)
    let ticks_b: Vec<Externalized<ChrononRecord>> = (30..=70u64).map(make_tick).collect();
    store.append_block(ticks_b).unwrap();

    // Block 2: ticks 71–100 (after the challenge window)
    let ticks_c: Vec<Externalized<ChrononRecord>> = (71..=100u64).map(make_tick).collect();
    store.append_block(ticks_c).unwrap();

    let cal_store = CalendarStore::new(Arc::new(store));

    let req = StorageProofRequest {
        tbid: "test-full".into(),
        chronon_start: 30,
        chronon_end: 70,
    };
    let resp = cal_store
        .prove_storage(&req)
        .expect("prove_storage should return Some for full coverage");

    // Only block 1 overlaps the challenge range.
    assert_eq!(resp.blocks.len(), 1);
    assert!(
        (resp.coverage_ratio - 1.0).abs() < f64::EPSILON,
        "expected coverage_ratio == 1.0, got {}",
        resp.coverage_ratio
    );

    let result = verify_storage_proof(&req, &resp, &[]);
    assert!(
        result.verified,
        "proof should verify when all requested ticks are present"
    );
    assert!(
        (result.coverage_ratio - 1.0).abs() < f64::EPSILON,
        "verified coverage should be 1.0, got {}",
        result.coverage_ratio
    );

    std::fs::remove_dir_all(&tmp).ok();
}

/// Server holds chronons 1–50, challenge range 1–100.
/// Only half the requested ticks exist → coverage_ratio == 0.5.
#[test]
fn partial_coverage_scenario() {
    let tmp = std::env::temp_dir().join(format!("foretias-pos-partial-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let path = tmp.join("calendar.jsonl");
    let server = make_server();
    let store = EncryptedJsonlCalendarStore::new(path, server);

    // Single block with ticks 1–50 (challenge start == block start).
    let ticks: Vec<Externalized<ChrononRecord>> = (1..=50u64).map(make_tick).collect();
    store.append_block(ticks).unwrap();

    let cal_store = CalendarStore::new(Arc::new(store));

    let req = StorageProofRequest {
        tbid: "test-partial".into(),
        chronon_start: 1,
        chronon_end: 100,
    };
    let resp = cal_store
        .prove_storage(&req)
        .expect("prove_storage should return Some for partial coverage");

    assert_eq!(resp.blocks.len(), 1);
    assert!(
        (resp.coverage_ratio - 0.5).abs() < f64::EPSILON,
        "expected coverage_ratio == 0.5, got {}",
        resp.coverage_ratio
    );

    let result = verify_storage_proof(&req, &resp, &[]);
    assert!(
        result.verified,
        "proof for held ticks should verify even with partial coverage"
    );
    assert!(
        (result.coverage_ratio - 0.5).abs() < f64::EPSILON,
        "verified coverage should be 0.5, got {}",
        result.coverage_ratio
    );

    std::fs::remove_dir_all(&tmp).ok();
}

/// Load a block without a computed merkle_root (pre-v0.7 format, all-zeros).
///
/// `prove_storage` still returns a response (the block has ticks that overlap
/// the requested range), but `verify_storage_proof` correctly rejects it
/// because the zero merkle_root does not match the actual Merkle tree over
/// the tick data.
#[test]
fn backward_compat_test() {
    use base64::Engine;

    let tmp = std::env::temp_dir().join(format!("foretias-pos-bcompat-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let path = tmp.join("calendar.jsonl");
    let server = make_server();

    // Manually construct a CalendarBlock with merkle_root = [0; 32]
    // to simulate a pre-v0.7 block that lacks a computed merkle root.
    let ticks: Vec<Externalized<ChrononRecord>> = (1..=10u64).map(make_tick).collect();
    let block = CalendarBlock {
        block_id: 0,
        written_at_ns: 1_700_000_000_000_000_000,
        ticks,
        merkle_root: [0u8; 32],
    };

    // Seal and write to the JSONL file (mimicking the encrypted format).
    let json_bytes = serde_json::to_vec(&block).expect("serialize block");
    let sealed = server.seal_for_self(&json_bytes).expect("seal block");
    let cbor_bytes = serde_cbor::to_vec(&sealed).expect("cbor encode");
    let line = base64::engine::general_purpose::STANDARD.encode(&cbor_bytes);
    std::fs::write(&path, format!("{}\n", line)).expect("write jsonl");

    // Re-open the store and read the block back.
    let store_enc = EncryptedJsonlCalendarStore::new(path.clone(), server);
    let read_blocks = store_enc.read_all().expect("read_all should succeed");
    assert_eq!(read_blocks.len(), 1);
    assert_eq!(
        read_blocks[0].merkle_root, [0u8; 32],
        "pre-v0.7 block should have zero merkle_root"
    );

    let store = CalendarStore::new(Arc::new(store_enc));

    let req = StorageProofRequest {
        tbid: "test-bcompat".into(),
        chronon_start: 1,
        chronon_end: 10,
    };

    // prove_storage should NOT return None — the block has ticks that overlap
    // the requested range, so a proof is generated.
    let resp = store
        .prove_storage(&req)
        .expect("prove_storage should return Some for pre-v0.7 block");
    assert_eq!(resp.blocks.len(), 1);
    assert_eq!(
        resp.blocks[0].merkle_root, [0u8; 32],
        "proof should carry the zero merkle_root from the pre-v0.7 block"
    );

    // Verification must fail: the stored merkle_root is all-zeros and does
    // not match the actual Merkle tree computed from the tick data.
    let result = verify_storage_proof(&req, &resp, &[]);
    assert!(
        !result.verified,
        "pre-v0.7 block with zero merkle_root should fail verification"
    );

    std::fs::remove_dir_all(&tmp).ok();
}
