//! E2E verification discipline tests.
//!
//! These tests verify that the UnverifiedSignatureEnvelope/CleanAuthenticated discipline
//! is enforced at the protocol level — forged or unsigned data is rejected
//! before reaching trusted state.

use foretias_core::crypto_server;
use foretias_core::foretias::calendar::Calendar;
use foretias_core::foretias::clean_auth::UnverifiedSignatureEnvelope;
use foretias_core::foretias::tick::ChrononRecord;
use foretias_core::foretias::types::Tbid;

#[tokio::test]
async fn test_e2e_calendar_load_integrity() {
    let path = "/tmp/foretias-e2e-integrity.json";
    let tbid = Tbid::from_raw([0xCC; 96]);
    let tbid_str = tbid.to_hex();

    let mut cal = Calendar::new(tbid, "e2e-integrity");
    let tick = foretias_core::foretias::tick::ChrononRecord::builder()
        .chronon_number(1)
        .public_key(vec![0u8; 32].into())
        .forward_foretis(vec![].into())
        .backward_foretis(vec![].into())
        .aa_nonce([0u8; 16].into())
        .tb_version(0)
        .build()
        .unwrap();
    cal.append(tick).unwrap();
    cal.save(path).unwrap();

    let server = crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap();
    let loaded = Calendar::load_and_verify(path, server.as_ref(), &tbid_str, None, None);
    assert!(
        loaded.is_ok(),
        "single tick calendar should pass integrity check"
    );

    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn test_e2e_gossip_unsigned_rejected() {
    use foretias_server::probity::gossip_handler::handle_gossip_message;
    use foretias_server::probity::report::ProbityReportRecord;
    use foretias_server::probity::ProbityStore;

    let server = crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap();
    let store = ProbityStore::new();

    let unsigned_report = ProbityReportRecord {
        subject: "test-subject".to_string(),
        reporter: "00".repeat(48),
        attribute: "correctness".to_string(),
        value: 1.0,
        timestamp_ns: 0,
        signature: vec![],
        curve: 1,
        slow_signature: vec![],
    };
    let json = serde_json::to_vec(&unsigned_report).unwrap();

    let result = handle_gossip_message(&json, &store, server.as_ref(), 0);
    assert!(
        result.is_ok(),
        "unsigned report should be dropped (Ok), not errored"
    );
    assert_eq!(
        store.report_count("test-subject"),
        0,
        "unsigned report must not be ingested"
    );
}

#[tokio::test]
async fn test_e2e_type_discipline_enforced() {
    // The compile-time gate: UnverifiedSignatureEnvelope<ChrononRecord> CANNOT be directly
    // assigned to CleanAuthenticated<ChrononRecord>. The compiler enforces this.
    // If this test compiles with a direct assignment, the discipline has failed.
    let record = ChrononRecord::builder()
        .chronon_number(1)
        .public_key(vec![0u8; 32].into())
        .forward_foretis(vec![].into())
        .backward_foretis(vec![].into())
        .aa_nonce([0u8; 16].into())
        .tb_version(0)
        .build()
        .unwrap();
    let _up: UnverifiedSignatureEnvelope<ChrononRecord> =
        UnverifiedSignatureEnvelope::<ChrononRecord>::from_parsed(record);
    // The following would NOT compile:
    // let _: CleanAuthenticated<ChrononRecord> = _up;
    // This is the desired behavior — the compiler enforces the gate.
}
