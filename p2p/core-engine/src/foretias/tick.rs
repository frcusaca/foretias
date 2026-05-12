//! Foretias domain types: TickRecord, Foretis, and stamp/verify operations.

use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::crypto_server::CryptoServer;
use crate::core::rng::random_bytes;
use crate::error::NodeError;
use super::encoding::{FTByteVector, FTByteArray};
use super::types::Tbid;

/// A single entry in the Calendar, linking consecutive ticks via Foretis attestations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickRecord {
    /// The monotonically increasing tick index.
    pub tick_number: u64,
    /// The public key active at this tick.
    pub public_key: FTByteVector,
    /// Plain-text algorithm identifier for this tick's key.
    #[serde(default = "default_sig_algorithm")]
    pub signature_algorithm: String,
    /// Serialized Foretis attesting forward to the next tick.
    pub forward_foretis: FTByteVector,
    /// Serialized Foretis attesting backward to the previous tick.
    pub backward_foretis: FTByteVector,
    /// Cryptographic nonce (16 bytes) used in the auto-attestation blob for this tick pair.
    /// This prevents replay attacks by ensuring each blob is unique even if the tick data repeats.
    pub aa_nonce: FTByteArray<16>,
    /// Number of user-initiated stamps during this tick (excluding auto-attestation itself,
    /// but including mutual attestations). Persisted for blob reconstruction during verify_pair.
    #[serde(default)]
    pub stamps_per_tick: u64,
    /// External attestations from other Time Families.
    #[serde(default)]
    pub external_attestations: Vec<super::external_attestation::ExternalAttestation>,
    /// Dual-key (Ed25519 + SLH-DSA-SHA2-256f) signature over the genesis blob.
    /// Present only on tick 1 when the node supports TBID V1 dual-key identity.
    #[serde(default)]
    pub genesis_signature: FTByteVector,
    /// TBID protocol version: 0 = legacy (16-byte UUID), 1 = dual-key (96-byte).
    #[serde(default = "default_tb_version")]
    pub tb_version: u32,
}

impl TickRecord {
    /// Create a validated TickRecord.
    ///
    /// # Errors
    /// Returns `NodeError::InvalidInput` if `tick_number` is 0 or `public_key` is empty.
    pub fn new(
        tick_number: u64,
        public_key: FTByteVector,
        signature_algorithm: String,
        forward_foretis: FTByteVector,
        backward_foretis: FTByteVector,
        aa_nonce: FTByteArray<16>,
        stamps_per_tick: u64,
    ) -> Result<Self, NodeError> {
        if tick_number == 0 {
            return Err(NodeError::InvalidInput("tick_number must be > 0".into()));
        }
        if public_key.is_empty() {
            return Err(NodeError::InvalidInput("public_key must not be empty".into()));
        }
        Ok(Self {
            tick_number,
            public_key,
            signature_algorithm,
            forward_foretis,
            backward_foretis,
            aa_nonce,
            stamps_per_tick,
            external_attestations: Vec::new(),
            genesis_signature: FTByteVector::new(),
            tb_version: 0,
        })
    }
}

/// A cryptographically signed attestation of content at a specific tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foretis {
    /// The tick number at which this attestation was created.
    pub tick_number: u64,
    /// SHA-256 hash of the attested content.
    pub content_hash: FTByteArray<32>,
    /// Ed25519 signature over the content and tick metadata.
    pub signature: FTByteVector,
    /// Plain-text algorithm identifier (e.g. "SPHINCS+-SHA2-128s-simple").
    pub signature_algorithm: String,
    /// TimeBeing identifier of the signing node.
    pub tbid: Tbid,
    /// Echo string identifying the tick (e.g. `"tick-42"`).
    pub echo: String,
    /// TimeBeing name (human-readable identifier).
    pub tbn: String,
    /// Server wall-clock time at stamping, in `"UE+<nanoseconds>ns"` format.
    pub time_being_reference_time: String,
}

impl Foretis {
    /// Create a validated Foretis.
    ///
    /// # Errors
    /// Returns `NodeError::InvalidInput` if `tick_number` is 0 or `signature` is empty.
    pub fn new(
        tick_number: u64,
        content_hash: FTByteArray<32>,
        signature: FTByteVector,
        signature_algorithm: String,
        tbid: Tbid,
        echo: String,
        tbn: String,
        time_being_reference_time: String,
    ) -> Result<Self, NodeError> {
        if tick_number == 0 {
            return Err(NodeError::InvalidInput("tick_number must be > 0".into()));
        }
        if signature.is_empty() {
            return Err(NodeError::InvalidInput("signature must not be empty".into()));
        }
        Ok(Self {
            tick_number,
            content_hash,
            signature,
            signature_algorithm,
            tbid,
            echo,
            tbn,
            time_being_reference_time,
        })
    }
}

/// Trait for looking up TickRecords from a calendar or calendar-like store.
///
/// Implemented by the Calendar component. Chronomatter uses this for verification
/// without owning calendar data.
pub trait CalendarLookup: Send + Sync {
    /// Retrieves up to `count` tick records starting from `tick_number`.
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError>;
    /// Returns the tick number of the most recent record, if any.
    fn latest(&self) -> Option<u64>;
    /// Returns the TBID of this calendar's owner.
    fn tbid(&self) -> Tbid;
    /// Returns the TimeBeing name.
    fn tbn(&self) -> &str;
}

/// Stamp content under the current tick's key.
pub fn stamp(
    server: &dyn CryptoServer,
    clock: &dyn Clock,
    tbid: &Tbid,
    tick_number: u64,
    content: &[u8],
    echo: &str,
    tbn: &str,
) -> Result<Foretis, NodeError> {
    let raw_tbid = tbid.raw_bytes();
    let mut sig_input = Vec::with_capacity(96 + 8 + content.len());
    sig_input.extend_from_slice(&raw_tbid);
    sig_input.extend_from_slice(&tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let signature = server.sign(&sig_input)?;
    let sig_alg = crate::foretias::types::SignatureAlgorithm::Ed25519.to_id_string().to_string();
    let content_hash = server.sha256(content)?;

    let now_ns = clock.now_ns()
        .map_err(|e| NodeError::Internal(format!("clock error: {e}")))?;
    let time_being_reference_time = format!("UE+{}ns", now_ns);

    Ok(Foretis {
        tick_number,
        content_hash: content_hash.bytes.into(),
        signature: FTByteVector::from(signature.bytes.to_vec()),
        signature_algorithm: sig_alg,
        tbid: *tbid,
        echo: echo.to_string(),
        tbn: tbn.to_string(),
        time_being_reference_time,
    })
}

/// Verify a Foretis against content and calendar.
pub fn verify(
    server: &dyn CryptoServer,
    foretis: &Foretis,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    let recomputed = server.sha256(content)?;
    if recomputed.bytes != *foretis.content_hash {
        return Ok(false);
    }

    let records = calendar.get(foretis.tick_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick"))?;

    // On tick 1, verify the genesis signature first
    if foretis.tick_number == 1 && !rec.genesis_signature.is_empty() {
        let genesis_valid = verify_genesis_signature(&foretis.tbid, rec)?;
        if !genesis_valid {
            return Ok(false);
        }
    }

    // Use the algorithm declared in the Foretis itself for verification
    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&foretis.tbid.raw_bytes());
    sig_input.extend_from_slice(&foretis.tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    Ok(server.verify_with(
        &rec.public_key,
        &foretis.signature_algorithm,
        &sig_input,
        &foretis.signature,
    )?)
}

/// Build auto-attestation blob: tbid || A.tick || A.pk || B.tick || B.pk || stamps_per_tick || nonce
///
/// Returns the signed blob and the 16-byte nonce for storage in TickRecord.
/// The nonce ensures each blob is unique, preventing replay attacks.
/// `stamps_per_tick` counts user-initiated stamps during tick B (excluding auto-attestation itself,
/// but including mutual attestations). This is knowable only to the Chronomatter that produced the tick.
pub fn auto_attestation_blob_with_count(
    tbid: &str,
    a_tick: u64,
    a_pk: &[u8; 32],
    b_tick: u64,
    b_pk: &[u8; 32],
    stamps_per_tick: u64,
) -> Result<(Vec<u8>, [u8; 16]), NodeError> {
    let mut nonce = [0u8; 16];
    random_bytes(&mut nonce)?;
    let mut blob = Vec::with_capacity(tbid.len() + 8 + 32 + 8 + 32 + 8 + 16);
    blob.extend_from_slice(tbid.as_bytes());
    blob.extend_from_slice(&a_tick.to_be_bytes());
    blob.extend_from_slice(a_pk);
    blob.extend_from_slice(&b_tick.to_be_bytes());
    blob.extend_from_slice(b_pk);
    blob.extend_from_slice(&stamps_per_tick.to_be_bytes());
    blob.extend_from_slice(&nonce);
    Ok((blob, nonce))
}

/// Build auto-attestation blob: tbid || A.tick || A.pk || B.tick || B.pk || nonce
///
/// Returns the signed blob and the 16-byte nonce for storage in TickRecord.
/// The nonce ensures each blob is unique, preventing replay attacks.
pub fn auto_attestation_blob(
    tbid: &str,
    a_tick: u64,
    a_pk: &[u8; 32],
    b_tick: u64,
    b_pk: &[u8; 32],
) -> Result<(Vec<u8>, [u8; 16]), NodeError> {
    auto_attestation_blob_with_count(tbid, a_tick, a_pk, b_tick, b_pk, 0)
}

/// Verify the auto-attestation between two consecutive tick records.
///
/// Rebuilds the blob from the two ticks using the nonce stored in `curr.aa_nonce`,
/// then verifies that both signatures cover the same blob.
pub fn verify_pair(
    crypto: &dyn CryptoServer,
    tbid_str: &str,
    prev: &TickRecord,
    curr: &TickRecord,
) -> Result<bool, NodeError> {
    let nonce = curr.aa_nonce;
    let stamps = curr.stamps_per_tick;
    let mut attest_blob = Vec::with_capacity(tbid_str.len() + 8 + prev.public_key.len() + 8 + curr.public_key.len() + 8 + 16);
    attest_blob.extend_from_slice(tbid_str.as_bytes());
    attest_blob.extend_from_slice(&prev.tick_number.to_be_bytes());
    attest_blob.extend_from_slice(&prev.public_key);
    attest_blob.extend_from_slice(&curr.tick_number.to_be_bytes());
    attest_blob.extend_from_slice(&curr.public_key);
    // Backward compat: pre-v0.9 TickRecords have stamps_per_tick=0 (serde default)
    attest_blob.extend_from_slice(&stamps.to_be_bytes());
    attest_blob.extend_from_slice(&nonce[..]);

    let forward_valid = crypto.verify_with(
        &prev.public_key,
        &curr.signature_algorithm,
        &attest_blob,
        &curr.forward_foretis,
    )?;

    let backward_valid = crypto.verify_with(
        &curr.public_key,
        &curr.signature_algorithm,
        &attest_blob,
        &curr.backward_foretis,
    )?;

    Ok(forward_valid && backward_valid)
}

/// Verify the genesis signature on tick 1.
///
/// Rebuilds the genesis blob from the tick record (tbid_raw || tick_number || public_key),
/// then verifies both Ed25519 and SLH-DSA signatures against the TBID public key.
///
/// Returns `Ok(true)` if the genesis signature is valid.
/// Returns `Ok(false)` if the signature is empty (legacy tick) or invalid.
/// Returns `Err` only on internal/crypto errors.
pub fn verify_genesis_signature(
    tbid: &Tbid,
    record: &TickRecord,
) -> Result<bool, NodeError> {
    if record.tick_number != 1 {
        return Ok(false);
    }
    if record.genesis_signature.is_empty() {
        return Ok(false);
    }
    if record.tb_version != 1 {
        return Ok(false);
    }

    let mut genesis_blob = Vec::with_capacity(96 + 8 + record.public_key.len());
    genesis_blob.extend_from_slice(&tbid.raw_bytes());
    genesis_blob.extend_from_slice(&record.tick_number.to_be_bytes());
    genesis_blob.extend_from_slice(&record.public_key);

    let pub_bytes = crate::foretias::types::SignatureBytes::from(tbid.raw_bytes());
    let sig_valid = crate::crypto_server::signing_tbid::tbid_verify(
        &pub_bytes,
        &genesis_blob,
        &record.genesis_signature,
    )
    .map_err(|e| NodeError::Crypto(e))?;

    Ok(sig_valid)
}

fn default_sig_algorithm() -> String {
    "Ed25519".to_string()
}

fn default_tb_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use crate::clock::SystemClock;
    use crate::foretias::calendar::Calendar;

    fn make_server() -> Box<dyn CryptoServer> {
        crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519)
            .expect("failed to create software crypto server")
    }

    fn make_cal(server: &dyn CryptoServer) -> Calendar {
        let tbid = Tbid::from_raw([0xAA; 96]);
        let mut cal = Calendar::new(tbid, "test-cal");
        let tick_number = 1;
        let content = b"init";
        let foretis = stamp(server, &SystemClock, &tbid, tick_number, content, "init", "test-cal")
            .expect("stamp init tick");
        let public_key = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes.to_vec(),
        };
        cal.append(TickRecord {
            tick_number,
            public_key: public_key.into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: serde_json::to_vec(&foretis).unwrap().into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        }).unwrap();
        cal
    }

    #[test]
    fn stamp_creates_valid_foretis() {
        let server = make_server();
        let tbid = Tbid::from_raw([1u8; 96]);
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 42, b"hello", "echo-42", "tbn")
            .expect("stamp should succeed");
        assert_eq!(foretis.tick_number, 42);
        assert_eq!(foretis.tbid, tbid);
        assert_eq!(foretis.echo, "echo-42");
        assert_eq!(foretis.tbn, "tbn");
        assert!(!foretis.signature.is_empty());
        assert!(!foretis.time_being_reference_time.is_empty());
    }

    #[test]
    fn stamp_different_content_different_hash() {
        let server = make_server();
        let tbid = Tbid::from_raw([2u8; 96]);
        let f1 = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"aaa", "e", "t").unwrap();
        let f2 = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"bbb", "e", "t").unwrap();
        assert_ne!(f1.content_hash, f2.content_hash);
    }

    #[test]
    fn stamp_empty_content_produces_valid_stamp() {
        let server = make_server();
        let tbid = Tbid::from_raw([3u8; 96]);
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"", "empty", "t").unwrap();
        assert_eq!(foretis.tick_number, 1);
        assert!(!foretis.signature.is_empty());
    }

    #[test]
    fn verify_succeeds_with_correct_content() {
        let server = make_server();
        let cal = make_cal(server.as_ref());
        let tbid = Tbid::from_raw([0xAA; 96]);
        let content = b"init";
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, content, "init", "test-cal")
            .expect("stamp");
        let valid = verify(server.as_ref(), &foretis, content, &cal)
            .expect("verify should not error");
        assert!(valid);
    }

    #[test]
    fn verify_fails_with_wrong_content() {
        let server = make_server();
        let cal = make_cal(server.as_ref());
        let tbid = Tbid::from_raw([0xAA; 96]);
        let content = b"init";
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, content, "init", "test-cal")
            .expect("stamp");
        let valid = verify(server.as_ref(), &foretis, b"wrong", &cal)
            .expect("verify should not error");
        assert!(!valid);
    }

    #[test]
    fn verify_fails_with_wrong_public_key() {
        let server = make_server();
        let tbid = Tbid::from_raw([0xBB; 96]);
        let mut cal = Calendar::new(tbid, "bad-cal");
        cal.append(TickRecord {
            tick_number: 1,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        }).unwrap();
        let content = b"test";
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, content, "e", "bad-cal")
            .expect("stamp");
        let result = verify(server.as_ref(), &foretis, content, &cal);
        if let Ok(valid) = result {
            assert!(!valid);
        }
    }

    #[test]
    fn verify_pair_valid_returns_true() {
        let server = make_server();
        let tbid = Tbid::from_raw([0xCC; 96]);
        let tbid_str = tbid.to_hex();
        let pub_key = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes,
            crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes[..32].try_into().unwrap(),
        };

        let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, 1, &pub_key, 2, &pub_key, 0).unwrap();
        let sig = server.sign(&attest_blob).unwrap();
        let sig_bytes = sig.bytes.to_vec();

        let sig_alg = "Ed25519".to_string();

        let prev = TickRecord {
            tick_number: 1,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        };
        let curr = TickRecord {
            tick_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: FTByteVector::from(sig_bytes.clone()),
            backward_foretis: FTByteVector::from(sig_bytes),
            aa_nonce: FTByteArray::from(nonce),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        };

        let valid = verify_pair(server.as_ref(), &tbid_str, &prev, &curr).unwrap();
        assert!(valid);
    }

    #[test]
    fn verify_pair_tampered_returns_false() {
        let server = make_server();
        let tbid = Tbid::from_raw([0xDD; 96]);
        let tbid_str = tbid.to_hex();
        let pub_key = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes,
            crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes[..32].try_into().unwrap(),
        };

        let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, 1, &pub_key, 2, &pub_key, 0).unwrap();
        let sig = server.sign(&attest_blob).unwrap();
        let mut sig_bytes = sig.bytes.to_vec();
        let sig_alg = "Ed25519".to_string();

        let prev = TickRecord {
            tick_number: 1,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        };
        let curr = TickRecord {
            tick_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: FTByteVector::from(sig_bytes.clone()),
            backward_foretis: FTByteVector::from(sig_bytes.clone()),
            aa_nonce: FTByteArray::from(nonce),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        };

        assert!(verify_pair(server.as_ref(), &tbid_str, &prev, &curr).unwrap());

        sig_bytes[0] ^= 0xFF;
        let curr_tampered = TickRecord {
            tick_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg,
            forward_foretis: FTByteVector::from(sig_bytes),
            backward_foretis: vec![0u8; 64].into(),
            aa_nonce: FTByteArray::from(nonce),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
        };

let valid = verify_pair(server.as_ref(), &tbid_str, &prev, &curr_tampered).unwrap();
        assert!(!valid);
    }

    #[test]
    fn auto_attestation_blob_nonce_is_unique() {
        let tbid = Tbid::from_raw([0x12; 96]);
        let tbid_str = tbid.to_hex();
        let pk = [0xABu8; 32];

        let (blob1, nonce1) = auto_attestation_blob_with_count(&tbid_str, 1, &pk, 2, &pk, 0).unwrap();
        let (blob2, nonce2) = auto_attestation_blob_with_count(&tbid_str, 1, &pk, 2, &pk, 0).unwrap();

        assert_ne!(nonce1, nonce2, "nonces must be unique");
        assert_eq!(blob1.len(), tbid_str.len() + 8 + 32 + 8 + 32 + 8 + 16);
        assert_ne!(blob1, blob2, "blobs with different nonces must differ");
    }

    #[test]
    fn auto_attestation_blob_nonce_is_present() {
        let tbid = Tbid::from_raw([0x34; 96]);
        let tbid_str = tbid.to_hex();
        let pk = [0xCDu8; 32];

        let (blob, nonce) = auto_attestation_blob_with_count(&tbid_str, 5, &pk, 6, &pk, 0).unwrap();
        let nonce_pos = blob.len() - 16;
        assert_eq!(&blob[nonce_pos..], &nonce, "nonce must be appended to blob");
    }
}
