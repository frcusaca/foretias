//! Comprehensive encoding tests for FTByteVector and FTByteArray base64 serialization.
//!
//! Covers all domain types that carry byte data through JSON serialization,
//! verifying that base64 strings (not integer arrays) are always produced,
//! and that roundtrip serialization preserves all data.

use crate::collision::heartbeat::Heartbeat;
use crate::crypto_server::SealedBlob;
use crate::epoch::frost_bridge::FrostMsg;
use crate::epoch::snapshot::EpochSnapshot;
use crate::foretias::calendar::Calendar;
use crate::foretias::encoding::{from_json, to_json, to_json_pretty, FTByteArray, FTByteVector};
use crate::foretias::external_attestation::ExternalAttestationRecord;
use crate::foretias::tick::{ChrononRecord, ForetisRecord};
use crate::foretias::types::Tbid;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

// ─── Helper Functions ───────────────────────────────────────────────

fn make_test_tbid() -> Tbid {
    let mut bytes = [0u8; 96];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = i as u8;
    }
    Tbid::from_raw(bytes)
}

fn make_test_foretis() -> ForetisRecord {
    let mut ch = [0u8; 32];
    for (i, c) in ch.iter_mut().enumerate() {
        *c = (i * 3 + 1) as u8;
    }
    ForetisRecord::new(
        42,
        FTByteArray::from(ch),
        make_test_tbid(),
        "test-echo".to_string(),
        "test-tbn".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("valid foretis")
}

fn make_test_tick_record(tick: u64) -> ChrononRecord {
    let mut nonce = [0u8; 16];
    for (i, n) in nonce.iter_mut().enumerate() {
        *n = (tick.wrapping_mul(7).wrapping_add(i as u64)) as u8;
    }
    ChrononRecord::new(
        tick,
        FTByteVector::from(vec![(tick % 256) as u8; 32]),
        "Ed25519".to_string(),
        FTByteVector::from(vec![0xFF; 64]),
        FTByteVector::new(),
        FTByteArray::from(nonce),
        0,
    )
    .expect("valid tick record")
}

fn make_test_calendar(num_ticks: u64) -> Calendar {
    let mut cal = Calendar::new(make_test_tbid(), "test-cal");
    for i in 1..=num_ticks {
        cal.append(make_test_tick_record(i)).expect("append tick");
    }
    cal
}

// Recursively check that no JSON array contains only numbers (integer arrays)
fn assert_json_has_no_int_arrays(value: &serde_json::Value) {
    match value {
        serde_json::Value::Array(arr) => {
            // Empty arrays (e.g. external_attestations: []) are fine — only flag
            // non-empty arrays where every element is a number (byte-array-as-ints).
            if !arr.is_empty() {
                let all_numbers = arr
                    .iter()
                    .all(|v| matches!(v, serde_json::Value::Number(_)));
                assert!(!all_numbers, "Found integer array in JSON: {:?}", arr);
            }
            for v in arr {
                assert_json_has_no_int_arrays(v);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                assert_json_has_no_int_arrays(v);
            }
        }
        _ => {}
    }
}

fn assert_field_is_base64_string(json_str: &str, field: &str) {
    let val: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("valid JSON for field {}: {}", field, e));
    let field_val = val
        .get(field)
        .unwrap_or_else(|| panic!("field {} exists", field));
    assert!(
        field_val.is_string(),
        "field {} is a string, not {:?}",
        field,
        field_val
    );
    let s = field_val.as_str().expect("field is string");
    let decoded = URL_SAFE_NO_PAD.decode(s);
    assert!(decoded.is_ok(), "field {} decodes as valid base64", field);
}

// ─── Category 1: Wrapper Type Tests (13 tests) ──────────────────────

#[test]
fn base64vec_empty_roundtrip() {
    let v = FTByteVector::new();
    let json = to_json(&v).unwrap();
    assert_eq!(json, "\"\"");
    let decoded: FTByteVector = from_json(&json).unwrap();
    assert!(decoded.is_empty());
    assert_eq!(decoded.len(), 0);
}

#[test]
fn base64vec_single_byte_roundtrip() {
    let v = FTByteVector::from(vec![0xAB]);
    let json = to_json(&v).unwrap();
    let decoded: FTByteVector = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), &[0xAB]);
    assert_eq!(decoded.len(), 1);
}

#[test]
fn base64vec_all_zeroes_32() {
    let v = FTByteVector::from(vec![0u8; 32]);
    let json = to_json(&v).unwrap();
    let decoded: FTByteVector = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), &[0u8; 32]);
}

#[test]
fn base64vec_all_0xff_64() {
    let v = FTByteVector::from(vec![0xFFu8; 64]);
    let json = to_json(&v).unwrap();
    let decoded: FTByteVector = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), &[0xFFu8; 64]);
}

#[test]
fn base64vec_slh_dsa_sig_size() {
    // SLH-DSA-SHA2-256f signature is 49856 bytes
    let v = FTByteVector::from(vec![0x42u8; 49_856]);
    let json = to_json(&v).unwrap();
    let decoded: FTByteVector = from_json(&json).unwrap();
    assert_eq!(decoded.len(), 49_856);
    assert!(decoded.as_slice().iter().all(|&b| b == 0x42));
}

#[test]
fn base64vec_output_is_string_not_array() {
    let v = FTByteVector::from(vec![1, 2, 3]);
    let json = to_json(&v).unwrap();
    // Must be a JSON string, not [1, 2, 3]
    assert!(
        json.starts_with('"'),
        "FTByteVector JSON must be a string, got: {}",
        json
    );
    assert!(
        !json.contains(','),
        "FTByteVector JSON must not contain commas: {}",
        json
    );
}

#[test]
fn base64array16_roundtrip() {
    let arr: FTByteArray<16> = FTByteArray::new([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ]);
    let json = to_json(&arr).unwrap();
    let decoded: FTByteArray<16> = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), arr.as_slice());
}

#[test]
fn base64array32_roundtrip() {
    let arr: FTByteArray<32> = FTByteArray::new([0xABu8; 32]);
    let json = to_json(&arr).unwrap();
    let decoded: FTByteArray<32> = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), &[0xABu8; 32]);
}

#[test]
fn base64array96_roundtrip() {
    let arr: FTByteArray<96> = FTByteArray::new([0xCDu8; 96]);
    let json = to_json(&arr).unwrap();
    let decoded: FTByteArray<96> = from_json(&json).unwrap();
    assert_eq!(decoded.as_slice(), &[0xCDu8; 96]);
}

#[test]
fn base64array_wrong_length_rejected() {
    // Encode 16 bytes, try to decode into FTByteArray<32> — must error
    let arr: FTByteArray<16> = FTByteArray::new([0x01u8; 16]);
    let json = to_json(&arr).unwrap();
    let result: Result<FTByteArray<32>, _> = from_json(&json);
    assert!(result.is_err(), "wrong-length array must be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("32"),
        "error should mention expected length: {}",
        err_msg
    );
}

#[test]
fn base64vec_deref_read() {
    let v = FTByteVector::from(vec![10, 20, 30]);
    // Deref to Vec<u8> — read operations
    assert_eq!(v.len(), 3);
    assert_eq!(v[0], 10);
    assert_eq!(v[1], 20);
    assert_eq!(v[2], 30);
    assert_eq!(v.first(), Some(&10));
    assert_eq!(v.last(), Some(&30));
}

#[test]
fn base64vec_deref_mut_write() {
    let mut v = FTByteVector::from(vec![1, 2, 3]);
    // DerefMut to Vec<u8> — write operations
    v.push(4);
    assert_eq!(v.len(), 4);
    v[0] = 99;
    assert_eq!(v[0], 99);
    v.extend_from_slice(&[5, 6]);
    assert_eq!(v.as_slice(), &[99, 2, 3, 4, 5, 6]);
}

#[test]
fn base64array_deref_read() {
    let arr: FTByteArray<8> = FTByteArray::new([1, 2, 3, 4, 5, 6, 7, 8]);
    // Deref to [u8; 8] — read operations
    assert_eq!(arr[0], 1);
    assert_eq!(arr[7], 8);
    assert_eq!(arr.len(), 8);
    // Slice operations via deref
    assert_eq!(&arr[..4], &[1, 2, 3, 4]);
}

// ─── Category 2: Domain Type Serialization Tests ────────────────────

// ForetisRecord
#[test]
fn foretis_serializes_to_base64_strings() {
    let f = make_test_foretis();
    let json = to_json(&f).unwrap();
    assert_field_is_base64_string(&json, "content_hash");
    // tbid inner is also base64
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let tbid_inner = val.get("tbid").unwrap().get("inner").unwrap();
    assert!(tbid_inner.is_string(), "tbid.inner must be base64 string");
}

#[test]
fn foretis_roundtrip() {
    let f = make_test_foretis();
    let json = to_json(&f).unwrap();
    let decoded: ForetisRecord = from_json(&json).unwrap();
    assert_eq!(decoded.chronon_number, f.chronon_number);
    assert_eq!(decoded.content_hash, f.content_hash);
    assert_eq!(decoded.tbid, f.tbid);
    assert_eq!(decoded.echo, f.echo);
    assert_eq!(decoded.tbn, f.tbn);
    assert_eq!(
        decoded.time_being_reference_time,
        f.time_being_reference_time
    );
}

// ChrononRecord
#[test]
fn tick_record_serializes_to_base64_strings() {
    let tr = make_test_tick_record(5);
    let json = to_json(&tr).unwrap();
    assert_field_is_base64_string(&json, "public_key");
    assert_field_is_base64_string(&json, "forward_foretis");
    assert_field_is_base64_string(&json, "backward_foretis");
    assert_field_is_base64_string(&json, "aa_nonce");
}

#[test]
fn tick_record_roundtrip() {
    let tr = make_test_tick_record(7);
    let json = to_json(&tr).unwrap();
    let decoded: ChrononRecord = from_json(&json).unwrap();
    assert_eq!(decoded.chronon_number, tr.chronon_number);
    assert_eq!(decoded.public_key, tr.public_key);
    assert_eq!(decoded.signature_algorithm, tr.signature_algorithm);
    assert_eq!(decoded.forward_foretis, tr.forward_foretis);
    assert_eq!(decoded.backward_foretis, tr.backward_foretis);
    assert_eq!(decoded.aa_nonce, tr.aa_nonce);
    assert_eq!(decoded.chronon_stamp_count, tr.chronon_stamp_count);
}

// Tbid
#[test]
fn tbid_serializes_to_single_base64_string() {
    let tbid = make_test_tbid();
    let json = to_json(&tbid).unwrap();
    // Tbid serializes as {"inner": "<base64>"}
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let inner = val.get("inner").unwrap();
    assert!(inner.is_string(), "tbid.inner must be a base64 string");
    let decoded = URL_SAFE_NO_PAD.decode(inner.as_str().unwrap()).unwrap();
    assert_eq!(decoded.len(), 96, "tbid.inner must decode to 96 bytes");
}

#[test]
fn tbid_roundtrip() {
    let tbid = make_test_tbid();
    let json = to_json(&tbid).unwrap();
    let decoded: Tbid = from_json(&json).unwrap();
    assert_eq!(decoded, tbid);
    assert_eq!(decoded.raw_bytes(), tbid.raw_bytes());
}

// Calendar
#[test]
fn calendar_serializes_ticks_as_base64() {
    let cal = make_test_calendar(3);
    let json = to_json(&cal).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ticks = val.get("ticks").unwrap().as_array().unwrap();
    assert_eq!(ticks.len(), 3);
    for tick in ticks {
        assert_field_is_base64_string(&serde_json::to_string(tick).unwrap(), "public_key");
        assert_field_is_base64_string(&serde_json::to_string(tick).unwrap(), "aa_nonce");
    }
}

#[test]
fn calendar_roundtrip() {
    let cal = make_test_calendar(5);
    let json = to_json(&cal).unwrap();
    let decoded: Calendar = from_json(&json).unwrap();
    assert_eq!(decoded.tbid, cal.tbid);
    assert_eq!(decoded.tbn, cal.tbn);
    assert_eq!(decoded.ticks.len(), cal.ticks.len());
    for (a, b) in cal.ticks.iter().zip(decoded.ticks.iter()) {
        assert_eq!(a.chronon_number, b.chronon_number);
        assert_eq!(a.public_key, b.public_key);
        assert_eq!(a.aa_nonce, b.aa_nonce);
    }
}

// Heartbeat
#[test]
fn heartbeat_serializes_to_base64_strings() {
    let hb = Heartbeat {
        peer_id: "test-peer".to_string(),
        timestamp_ns: 1234567890,
        nonce: FTByteArray::from([0xAB; 16]),
        curve: 1,
        signature: FTByteVector::from(vec![0xFF; 64]),
    };
    let json = to_json(&hb).unwrap();
    assert_field_is_base64_string(&json, "nonce");
    assert_field_is_base64_string(&json, "signature");
}

#[test]
fn heartbeat_roundtrip() {
    let hb = Heartbeat {
        peer_id: "peer-abc".to_string(),
        timestamp_ns: 999,
        nonce: FTByteArray::from([0x42; 16]),
        curve: 2,
        signature: FTByteVector::from(vec![0x11; 64]),
    };
    let json = to_json(&hb).unwrap();
    let decoded: Heartbeat = from_json(&json).unwrap();
    assert_eq!(decoded.peer_id, hb.peer_id);
    assert_eq!(decoded.timestamp_ns, hb.timestamp_ns);
    assert_eq!(decoded.nonce, hb.nonce);
    assert_eq!(decoded.curve, hb.curve);
    assert_eq!(decoded.signature, hb.signature);
}

// EpochSnapshot
#[test]
fn epoch_snapshot_roundtrip() {
    let snap = EpochSnapshot {
        epoch_number: 42,
        epoch_start_ns: 1000,
        epoch_end_ns: 2000,
        peer_scores: vec![],
        committee: vec!["C1".into(), "C2".into()],
        threshold: 2,
        frost_signature: FTByteVector::from(vec![0xAA; 64]),
        committee_pubkey: FTByteVector::from(vec![0xBB; 32]),
    };
    let json = to_json(&snap).unwrap();
    let decoded: EpochSnapshot = from_json(&json).unwrap();
    assert_eq!(decoded.epoch_number, snap.epoch_number);
    assert_eq!(decoded.frost_signature, snap.frost_signature);
    assert_eq!(decoded.committee_pubkey, snap.committee_pubkey);
    assert_eq!(decoded.threshold, snap.threshold);
}

// FrostMsg
#[test]
fn frost_msg_roundtrip() {
    // Commitment variant
    let msg = FrostMsg::Commitment {
        from: "peer-1".into(),
        commitment: FTByteVector::from(vec![0x01; 32]),
    };
    let json = to_json(&msg).unwrap();
    let decoded: FrostMsg = from_json(&json).unwrap();
    match decoded {
        FrostMsg::Commitment { from, commitment } => {
            assert_eq!(from, "peer-1");
            assert_eq!(commitment.len(), 32);
        }
        _ => panic!("expected Commitment variant"),
    }

    // Share variant
    let msg = FrostMsg::Share {
        from: "peer-2".into(),
        share: FTByteVector::from(vec![0x02; 64]),
    };
    let json = to_json(&msg).unwrap();
    let decoded: FrostMsg = from_json(&json).unwrap();
    match decoded {
        FrostMsg::Share { from, share } => {
            assert_eq!(from, "peer-2");
            assert_eq!(share.len(), 64);
        }
        _ => panic!("expected Share variant"),
    }
}

// SealedBlob
#[test]
fn sealed_blob_roundtrip() {
    let blob = SealedBlob {
        nonce: FTByteVector::from(vec![0x00; 12]),
        ciphertext: FTByteVector::from(vec![0xFF; 256]),
    };
    let json = to_json(&blob).unwrap();
    let decoded: SealedBlob = from_json(&json).unwrap();
    assert_eq!(decoded.nonce, blob.nonce);
    assert_eq!(decoded.ciphertext, blob.ciphertext);
}

// ExternalAttestationRecord
#[test]
fn external_attestation_roundtrip() {
    let foretis = make_test_foretis();
    let tick = make_test_tick_record(42);
    let att = ExternalAttestationRecord {
        attester_tbid: "test-attester".to_string(),
        foretis,
        signature: FTByteVector::default(),
        signature_algorithm: "Ed25519".to_string(),
        attester_tick_record: tick,
        received_at_ns: 1_000_000_000,
    };
    let json = to_json(&att).unwrap();
    let decoded: ExternalAttestationRecord = from_json(&json).unwrap();
    assert_eq!(decoded.attester_tbid, att.attester_tbid);
    assert_eq!(decoded.received_at_ns, att.received_at_ns);
    assert_eq!(decoded.foretis.chronon_number, att.foretis.chronon_number);
    assert_eq!(
        decoded.attester_tick_record.chronon_number,
        att.attester_tick_record.chronon_number
    );
}

// ─── Category 3: Cross-Language Canonical Output (4 tests) ──────────

#[test]
fn to_json_and_from_json_are_inverses() {
    let f = make_test_foretis();
    let json = to_json(&f).unwrap();
    let decoded: ForetisRecord = from_json(&json).unwrap();
    assert_eq!(decoded.chronon_number, f.chronon_number);
    assert_eq!(decoded.content_hash, f.content_hash);
    assert_eq!(decoded.tbid, f.tbid);

    let tr = make_test_tick_record(10);
    let json = to_json(&tr).unwrap();
    let decoded: ChrononRecord = from_json(&json).unwrap();
    assert_eq!(decoded.chronon_number, tr.chronon_number);
    assert_eq!(decoded.public_key, tr.public_key);
    assert_eq!(decoded.aa_nonce, tr.aa_nonce);
    assert_eq!(decoded.forward_foretis, tr.forward_foretis);

    let cal = make_test_calendar(3);
    let json = to_json(&cal).unwrap();
    let decoded: Calendar = from_json(&json).unwrap();
    assert_eq!(decoded.tbid, cal.tbid);
    assert_eq!(decoded.tbn, cal.tbn);
    assert_eq!(decoded.ticks.len(), cal.ticks.len());
}

#[test]
fn canonical_output_is_deterministic() {
    // Serializing the same value twice must produce identical output
    let f = make_test_foretis();
    let json1 = to_json(&f).unwrap();
    let json2 = to_json(&f).unwrap();
    assert_eq!(json1, json2, "to_json must be deterministic");

    let cal = make_test_calendar(5);
    let json1 = to_json(&cal).unwrap();
    let json2 = to_json(&cal).unwrap();
    assert_eq!(json1, json2, "to_json must be deterministic for Calendar");
}

#[test]
fn no_integer_arrays_in_any_output() {
    // No serialized domain type should contain integer arrays
    let f = make_test_foretis();
    let json = to_json(&f).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);

    let tr = make_test_tick_record(1);
    let json = to_json(&tr).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);

    let cal = make_test_calendar(3);
    let json = to_json(&cal).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);

    let tbid = make_test_tbid();
    let json = to_json(&tbid).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);
}

#[test]
fn encoding_to_json_equals_direct_serde() {
    // to_json() must produce the same output as serde_json::to_string()
    let f = make_test_foretis();
    let our_json = to_json(&f).unwrap();
    let serde_json_str = serde_json::to_string(&f).unwrap();
    assert_eq!(our_json, serde_json_str);

    let tr = make_test_tick_record(3);
    let our_json = to_json(&tr).unwrap();
    let serde_json_str = serde_json::to_string(&tr).unwrap();
    assert_eq!(our_json, serde_json_str);

    let tbid = make_test_tbid();
    let our_json = to_json(&tbid).unwrap();
    let serde_json_str = serde_json::to_string(&tbid).unwrap();
    assert_eq!(our_json, serde_json_str);
}

// ─── Category 4: Pathological Cases (6 tests) ───────────────────────

#[test]
fn calendar_100_ticks_serialization() {
    let cal = make_test_calendar(100);
    let json = to_json(&cal).unwrap();
    let decoded: Calendar = from_json(&json).unwrap();
    assert_eq!(decoded.ticks.len(), 100);
    for i in 0..100 {
        assert_eq!(decoded.ticks[i].chronon_number, cal.ticks[i].chronon_number);
    }
    // Verify no integer arrays leaked
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);
}

#[test]
fn calendar_0_ticks_serialization() {
    let cal = Calendar::new(make_test_tbid(), "empty-cal");
    let json = to_json(&cal).unwrap();
    let decoded: Calendar = from_json(&json).unwrap();
    assert!(decoded.ticks.is_empty());
    assert_eq!(decoded.tbid, cal.tbid);
    assert_eq!(decoded.tbn, cal.tbn);
}

#[test]
fn deep_nesting_external_attestation() {
    // ExternalAttestationRecord contains ForetisRecord + ChrononRecord — deep nesting
    let foretis = make_test_foretis();
    let tick = make_test_tick_record(100);
    let att = ExternalAttestationRecord {
        attester_tbid: "deep-attester".to_string(),
        foretis,
        signature: FTByteVector::default(),
        signature_algorithm: "Ed25519".to_string(),
        attester_tick_record: tick,
        received_at_ns: 9_999_999_999,
    };
    let json = to_json(&att).unwrap();
    let decoded: ExternalAttestationRecord = from_json(&json).unwrap();
    assert_eq!(decoded.attester_tbid, att.attester_tbid);
    assert_eq!(decoded.received_at_ns, att.received_at_ns);
    assert_eq!(decoded.foretis.chronon_number, att.foretis.chronon_number);
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_json_has_no_int_arrays(&val);
}

#[test]
fn all_zero_calendars() {
    // Calendar with all-zero byte fields
    let tbid = Tbid::from_raw([0u8; 96]);
    let mut cal = Calendar::new(tbid, "zero-cal");
    let zero_nonce: [u8; 16] = [0u8; 16];
    let tr = ChrononRecord::new(
        1,
        FTByteVector::from(vec![0u8; 32]),
        "Ed25519".to_string(),
        FTByteVector::from(vec![0u8; 64]),
        FTByteVector::new(),
        FTByteArray::from(zero_nonce),
        0,
    )
    .expect("zero tick");
    cal.append(tr).expect("append zero tick");

    let json = to_json(&cal).unwrap();
    let decoded: Calendar = from_json(&json).unwrap();
    assert_eq!(decoded.tbid, cal.tbid);
    assert_eq!(decoded.tbn, cal.tbn);
    assert_eq!(decoded.ticks.len(), cal.ticks.len());
}

#[test]
fn max_slh_dsa_signature_in_tick_record() {
    // ChrononRecord with a 49856-byte forward_foretis (max SLH-DSA sig)
    let mut nonce = [0u8; 16];
    for (i, n) in nonce.iter_mut().enumerate() {
        *n = i as u8;
    }
    let tr = ChrononRecord::new(
        1,
        FTByteVector::from(vec![0x01; 32]),
        "SPHINCS+-SHA2-256f-simple".to_string(),
        FTByteVector::from(vec![0x42u8; 49_856]),
        FTByteVector::from(vec![0x43u8; 49_856]),
        FTByteArray::from(nonce),
        0,
    )
    .expect("large tick");
    let json = to_json(&tr).unwrap();
    let decoded: ChrononRecord = from_json(&json).unwrap();
    assert_eq!(decoded.forward_foretis.len(), 49_856);
    assert_eq!(decoded.backward_foretis.len(), 49_856);
    assert_eq!(decoded.signature_algorithm, "SPHINCS+-SHA2-256f-simple");
}

#[test]
fn mimic_spincs_signature_size() {
    // Mimic SPHINCS+-SHA2-128s signature size (7856 bytes)
    let mut nonce = [0u8; 16];
    for (i, n) in nonce.iter_mut().enumerate() {
        *n = (i + 1) as u8;
    }
    let tr = ChrononRecord::new(
        2,
        FTByteVector::from(vec![0x02; 32]),
        "SPHINCS+-SHA2-128s-simple".to_string(),
        FTByteVector::from(vec![0x55u8; 7_856]),
        FTByteVector::new(),
        FTByteArray::from(nonce),
        0,
    )
    .expect("sphincs tick");
    let json = to_json(&tr).unwrap();
    let decoded: ChrononRecord = from_json(&json).unwrap();
    assert_eq!(decoded.forward_foretis.len(), 7_856);
    assert_eq!(decoded.signature_algorithm, "SPHINCS+-SHA2-128s-simple");
}

// ─── Category 5: Negative Tests (5 tests) ───────────────────────────

#[test]
fn deserialize_invalid_base64_fails() {
    // Invalid base64 characters should produce an error
    let result: Result<FTByteVector, _> = from_json("\"!!!invalid-base64!!!\"");
    assert!(result.is_err(), "invalid base64 must fail");
}

#[test]
fn deserialize_wrong_length_for_array32_fails() {
    // Encode 16 bytes, try to decode as FTByteArray<32>
    let arr: FTByteArray<16> = FTByteArray::new([0x01u8; 16]);
    let json = to_json(&arr).unwrap();
    let result: Result<FTByteArray<32>, _> = from_json(&json);
    assert!(
        result.is_err(),
        "wrong length must fail for FTByteArray<32>"
    );
}

#[test]
fn deserialize_integer_array_fails() {
    // FTByteVector must reject [1, 2, 3] — only strings accepted
    let result: Result<FTByteVector, _> = from_json("[1, 2, 3]");
    assert!(
        result.is_err(),
        "integer array must be rejected for FTByteVector"
    );

    // FTByteArray must also reject integer arrays
    let result: Result<FTByteArray<32>, _> = from_json("[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]");
    assert!(
        result.is_err(),
        "integer array must be rejected for FTByteArray<32>"
    );
}

#[test]
fn deserialize_empty_string_for_array32_fails() {
    // Empty base64 decodes to 0 bytes, which is wrong for FTByteArray<32>
    let result: Result<FTByteArray<32>, _> = from_json("\"\"");
    assert!(
        result.is_err(),
        "empty string must fail for FTByteArray<32>"
    );
}

#[test]
fn to_json_pretty_produces_valid_output() {
    // Verify to_json_pretty also works and produces valid JSON
    let f = make_test_foretis();
    let json = to_json_pretty(&f).unwrap();
    let decoded: ForetisRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.chronon_number, f.chronon_number);
    assert_eq!(decoded.content_hash, f.content_hash);
    assert_eq!(decoded.tbid, f.tbid);
    // Pretty output should contain newlines
    assert!(json.contains('\n'), "pretty output should contain newlines");
}
