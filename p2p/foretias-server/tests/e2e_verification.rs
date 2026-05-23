//! E2E verification discipline tests.
//!
//! These tests verify that the Unprocessed/CleanAuthenticated discipline
//! is enforced at the protocol level — forged or unsigned data is rejected
//! before reaching trusted state.

use foretias_core::foretias::calendar::Calendar;
use foretias_core::foretias::clean_auth::{Unprocessed, UnprocessedChrononRecord};
use foretias_core::foretias::types::Tbid;
use foretias_core::crypto_server;

#[tokio::test]
async fn test_e2e_calendar_load_integrity() {
    let path = "/tmp/foretias-e2e-integrity.json";
    let tbid = Tbid::from_raw([0xCC; 96]);
    let tbid_str = tbid.to_hex();

    let mut cal = Calendar::new(tbid, "e2e-integrity");
    let tick = foretias_core::foretias::tick::ChrononRecord {
        chronon_number: 1,
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
    cal.append(tick).unwrap();
    cal.save(path).unwrap();

    let server = crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap();
    let loaded = Calendar::load_and_verify(path, server.as_ref(), &tbid_str, None, None);
    assert!(loaded.is_ok(), "single tick calendar should pass integrity check");

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn test_e2e_gossip_unsigned_rejected() {
    use foretias_server::probity::gossip_handler::handle_gossip_message;
    use foretias_server::probity::report::ProbityReport;
    use foretias_server::probity::ProbityStore;

    let server = crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap();
    let store = ProbityStore::new();

    let unsigned_report = ProbityReport {
        subject: "test-subject".to_string(),
        reporter: "00".repeat(48),
        attribute: "correctness".to_string(),
        value: 1.0,
        timestamp_ns: 0,
        signature: vec![],
        curve: 1,
    };
    let json = serde_json::to_vec(&unsigned_report).unwrap();

    let result = handle_gossip_message(&json, &store, server.as_ref(), 0);
    assert!(result.is_err(), "unsigned report should be rejected");
}

#[tokio::test]
async fn test_e2e_type_discipline_enforced() {
    use foretias_core::foretias::clean_auth::UnprocessedChrononRecord;

    // The compile-time gate: UnprocessedChrononRecord CANNOT be directly
    // assigned to CleanAuthenticatedChrononRecord. The compiler enforces this.
    // If this test compiles with a direct assignment, the discipline has failed.
    let record = foretias_core::foretias::tick::ChrononRecord {
        chronon_number: 1,
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
    let _up: UnprocessedChrononRecord = Unprocessed::<foretias_core::foretias::tick::ChrononRecord>::from_parsed(record);
    // The following would NOT compile:
    // let _: CleanAuthenticatedChrononRecord = _up;
    // This is the desired behavior — the compiler enforces the gate.
}
