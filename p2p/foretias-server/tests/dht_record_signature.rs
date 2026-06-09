//! g3-d regression: DHT `PeerRegistrationRecord` signing and verification.
//!
//! Tests cover four cases (per COMBINED_GROUP3_SPEC.md §2.2 Tests):
//! 1. A record with no signature (legacy peer) is accepted with a warn log.
//! 2. A record with a valid signature is accepted.
//! 3. A record tampered after signing is rejected.
//! 4. A record signed by one key but claiming a different TBID is rejected.

use foretias_core::crypto_server::{self, ForetiasCurve, PublicKeyBytes};
use foretias_server::communerd::{validate_peer_registration, PeerRegistrationRecord};

/// Build a 96-byte hex TBID with `pubkey_32` as the first 32 bytes and zeros
/// for the remaining 64 (SLH-DSA portion). The signature-verification path
/// only consumes the first 32 bytes; the rest is filler for these tests.
fn tbid_from_pubkey(pubkey_32: &[u8; 32]) -> String {
    let mut raw = [0u8; 96];
    raw[..32].copy_from_slice(pubkey_32);
    hex::encode(raw)
}

/// Construct a PeerRegistrationRecord matching the given TBID with empty signature.
fn make_record(tbid_hex: String) -> PeerRegistrationRecord {
    PeerRegistrationRecord::new(
        "peer-dht-test".to_string(),
        tbid_hex,
        "/ip4/127.0.0.1/tcp/9911".to_string(),
        "127.0.0.1:4011".to_string(),
        60_000_000_000,
        1_700_000_000_000_000_000,
        Vec::new(),
    )
}

/// Extract the 32-byte Ed25519 pubkey from a CryptoServer's public_key().
fn ed25519_pub_of(crypto: &dyn foretias_core::crypto_server::CryptoServer) -> [u8; 32] {
    match crypto.public_key() {
        PublicKeyBytes::Ed25519(pk) => pk.bytes,
        _ => panic!("expected Ed25519 public key from software crypto"),
    }
}

#[test]
fn legacy_record_without_signature_accepted_with_warn() {
    // Spec: a record with empty `signature` is the pre-signing format. Until
    // the legacy compatibility window closes, validate_peer_registration must
    // accept it (and log at debug).
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));
    record.set_signature(Vec::new()); // explicit: legacy peers omit the field

    let result = validate_peer_registration(&record, &*crypto);
    match result {
        Ok(true) => {}
        other => panic!("legacy record without signature must be accepted, got {other:?}"),
    }
}

#[test]
fn valid_signature_accepted() {
    // Spec: sign canonical_payload with the TBID's Ed25519 key; verify with the
    // same pubkey extracted from the TBID hex. Must accept.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    let canonical = record.canonical_payload();
    let sig = crypto.sign(&canonical).expect("sign canonical payload");
    record.set_signature(sig.bytes.to_vec());

    let result = validate_peer_registration(&record, &*crypto);
    match result {
        Ok(true) => {}
        other => panic!("validly signed record must be accepted, got {other:?}"),
    }
}

#[test]
fn tampered_record_rejected() {
    // Spec: a record where any signed field is modified after signing must
    // produce signature verification failure (Ok(false)).
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    // Sign with the original multiaddr.
    let canonical = record.canonical_payload();
    let sig = crypto.sign(&canonical).expect("sign canonical payload");
    record.set_signature(sig.bytes.to_vec());

    // Tamper: flip the multiaddr and re-apply the old (now-stale) signature
    // to simulate a malicious actor who modifies a field but keeps the original sig.
    record.set_multiaddr("/ip4/10.0.0.66/tcp/9911".to_string());
    record.set_signature(sig.bytes.to_vec());

    let result = validate_peer_registration(&record, &*crypto);
    match result {
        Ok(false) => {}
        other => panic!("tampered record must be rejected, got {other:?}"),
    }
}

#[test]
fn wrong_pubkey_rejected() {
    // Spec: sign with key A, claim TBID derived from key B; verification
    // must extract key B's pubkey from the TBID and reject the signature
    // (which was produced by key A).
    let crypto_a = crypto_server::new_software(ForetiasCurve::Ed25519)
        .expect("libsodium must be available (crypto_a)");
    let crypto_b = crypto_server::new_software(ForetiasCurve::Ed25519)
        .expect("libsodium must be available (crypto_b)");
    let pubkey_a = ed25519_pub_of(&*crypto_a);
    let pubkey_b = ed25519_pub_of(&*crypto_b);
    assert_ne!(pubkey_a, pubkey_b, "two fresh keypairs must differ");

    // Build a record claiming TBID = key B...
    let mut record = make_record(tbid_from_pubkey(&pubkey_b));
    // ...but sign canonical_payload using key A.
    let canonical = record.canonical_payload();
    let sig = crypto_a.sign(&canonical).expect("sign with key A");
    record.set_signature(sig.bytes.to_vec());

    // Verification extracts pubkey from record.tbid (= key B), checks the
    // signature against key B, finds it doesn't validate → Ok(false).
    let result = validate_peer_registration(&record, &*crypto_a);
    match result {
        Ok(false) => {}
        other => panic!("wrong-pubkey record must be rejected, got {other:?}"),
    }
}

#[test]
fn structurally_invalid_record_rejected_even_without_signature() {
    // Bonus coverage: structural validation runs before signature checks, so
    // a record with empty peer_id must be rejected regardless of legacy compat.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));
    record.set_peer_id(String::new()); // structurally invalid

    let result = validate_peer_registration(&record, &*crypto);
    match result {
        Ok(false) => {}
        other => panic!("structurally invalid record must be rejected, got {other:?}"),
    }
}

#[test]
fn canonical_payload_excludes_signature_field() {
    // The canonical payload must not include the signature itself, otherwise
    // re-signing with a known good signature would produce a different payload.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    let before = record.canonical_payload();
    record.set_signature(vec![0xAB; 64]);
    let after = record.canonical_payload();

    assert_eq!(
        before, after,
        "canonical_payload must be identical regardless of the signature field's contents"
    );
}

#[test]
fn set_multiaddr_clears_signature() {
    // After calling set_multiaddr, the signature must be cleared because the
    // canonical payload has changed and any prior signature is no longer valid.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    // Sign the record.
    let canonical = record.canonical_payload();
    let sig = crypto.sign(&canonical).expect("sign canonical payload");
    record.set_signature(sig.bytes.to_vec());
    assert!(!record.signature().is_empty(), "signature must be set");

    // Mutate multiaddr — signature must be cleared.
    record.set_multiaddr("/ip4/10.0.0.99/tcp/8000".to_string());
    assert!(
        record.signature().is_empty(),
        "set_multiaddr must clear signature"
    );
}

#[test]
fn set_peer_id_clears_signature() {
    // After calling set_peer_id, the signature must be cleared.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    let canonical = record.canonical_payload();
    let sig = crypto.sign(&canonical).expect("sign canonical payload");
    record.set_signature(sig.bytes.to_vec());
    assert!(!record.signature().is_empty());

    record.set_peer_id("new-peer-id".to_string());
    assert!(
        record.signature().is_empty(),
        "set_peer_id must clear signature"
    );
}

#[test]
fn set_multiaddr_then_resign() {
    // Full cycle: sign → mutate → clear → re-sign → verify.
    let crypto =
        crypto_server::new_software(ForetiasCurve::Ed25519).expect("libsodium must be available");
    let pubkey = ed25519_pub_of(&*crypto);
    let mut record = make_record(tbid_from_pubkey(&pubkey));

    // Sign original.
    let canonical = record.canonical_payload();
    let sig = crypto.sign(&canonical).expect("sign canonical payload");
    record.set_signature(sig.bytes.to_vec());

    // Mutate (clears signature).
    record.set_multiaddr("/ip4/10.0.0.99/tcp/8000".to_string());
    assert!(record.signature().is_empty());

    // Re-sign with updated payload.
    let canonical2 = record.canonical_payload();
    let sig2 = crypto.sign(&canonical2).expect("re-sign after mutation");
    record.set_signature(sig2.bytes.to_vec());

    // Verify passes.
    let result = validate_peer_registration(&record, &*crypto);
    match result {
        Ok(true) => {}
        other => panic!("re-signed record must be accepted, got {other:?}"),
    }
}
