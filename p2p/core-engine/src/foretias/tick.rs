//! Foretias domain types: ChrononRecord, Foretis, and stamp/verify operations.

use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::crypto_server::CryptoServer;
use crate::core::rng::random_bytes;
use crate::error::NodeError;
use super::encoding::{FTByteVector, FTByteArray};
use super::types::Tbid;

/// A single entry in the Calendar, linking consecutive chronons via Foretis attestations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChrononRecord {
    /// The monotonically increasing chronon index.
    #[serde(rename = "tick_number")]
    pub chronon_number: u64,
    /// The public key active at this chronon.
    pub public_key: FTByteVector,
    /// Plain-text algorithm identifier for this chronon's key.
    #[serde(default = "default_sig_algorithm")]
    pub signature_algorithm: String,
    /// Serialized Foretis attesting forward to the next chronon.
    pub forward_foretis: FTByteVector,
    /// Serialized Foretis attesting backward to the previous chronon.
    pub backward_foretis: FTByteVector,
    /// Cryptographic nonce (16 bytes) used in the auto-attestation blob for this chronon pair.
    /// This prevents replay attacks by ensuring each blob is unique even if the chronon data repeats.
    pub aa_nonce: FTByteArray<16>,
    /// Number of user-initiated stamps during this chronon (excluding auto-attestation itself,
    /// but including mutual attestations). Persisted for blob reconstruction during verify_pair.
    pub chronon_stamp_count: u64,
    /// External attestations from other Time Families.
    #[serde(default)]
    pub external_attestations: Vec<super::external_attestation::ExternalAttestation>,

    /// TBID protocol version: 0 = legacy (16-byte UUID), 1 = dual-key (96-byte).
    #[serde(default = "default_tb_version")]
    pub tb_version: u32,
    /// Time Being ID — identifies which calendar this record belongs to.
    #[serde(default)]
    pub tbid: Tbid,
}

impl ChrononRecord {
    /// Create a validated ChrononRecord.
    ///
    /// # Errors
    /// Returns `NodeError::InvalidInput` if `chronon_number` is 0 or `public_key` is empty.
    pub fn new(
        chronon_number: u64,
        public_key: FTByteVector,
        signature_algorithm: String,
        forward_foretis: FTByteVector,
        backward_foretis: FTByteVector,
        aa_nonce: FTByteArray<16>,
        chronon_stamp_count: u64,
    ) -> Result<Self, NodeError> {
        if chronon_number == 0 {
            return Err(NodeError::InvalidInput("chronon_number must be > 0".into()));
        }
        if public_key.is_empty() {
            return Err(NodeError::InvalidInput("public_key must not be empty".into()));
        }
        Ok(Self {
            chronon_number,
            public_key,
            signature_algorithm,
            forward_foretis,
            backward_foretis,
            aa_nonce,
            chronon_stamp_count,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        })
    }

    /// Returns true if this record is the genesis chronon (tick 1).
    pub fn is_genesis(&self) -> bool {
        self.chronon_number == 1
    }

    pub fn chronon_number(&self) -> &u64 { &self.chronon_number }
    pub fn public_key(&self) -> &FTByteVector { &self.public_key }
    pub fn signature_algorithm(&self) -> &str { &self.signature_algorithm }
    pub fn forward_foretis(&self) -> &FTByteVector { &self.forward_foretis }
    pub fn backward_foretis(&self) -> &FTByteVector { &self.backward_foretis }
    pub fn aa_nonce(&self) -> &FTByteArray<16> { &self.aa_nonce }
    pub fn chronon_stamp_count(&self) -> &u64 { &self.chronon_stamp_count }
    pub fn external_attestations(&self) -> &Vec<super::external_attestation::ExternalAttestation> { &self.external_attestations }
    pub fn tb_version(&self) -> &u32 { &self.tb_version }
    pub fn tbid(&self) -> &Tbid { &self.tbid }
}

/// A cryptographically signed attestation of content at a specific chronon.
///
/// V2: signatures are carried by the trust-boundary wrapper (UnverifiedSignatureEnvelope),
/// not by the payload struct. Signing bytes are `postcard::to_allocvec(&foretis_payload)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foretis {
    /// The chronon number at which this attestation was created.
    pub chronon_number: u64,
    /// SHA-256 hash of the attested content.
    pub content_hash: FTByteArray<32>,
    /// TimeBeing identifier of the signing node.
    pub tbid: Tbid,
    /// Echo string identifying the chronon (e.g. `"chronon-42"`).
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
    /// Returns `NodeError::InvalidInput` if `chronon_number` is 0.
    pub fn new(
        chronon_number: u64,
        content_hash: FTByteArray<32>,
        tbid: Tbid,
        echo: String,
        tbn: String,
        time_being_reference_time: String,
    ) -> Result<Self, NodeError> {
        if chronon_number == 0 {
            return Err(NodeError::InvalidInput("chronon_number must be > 0".into()));
        }
        Ok(Self {
            chronon_number,
            content_hash,
            tbid,
            echo,
            tbn,
            time_being_reference_time,
        })
    }

    pub fn chronon_number(&self) -> &u64 { &self.chronon_number }
    pub fn content_hash(&self) -> &FTByteArray<32> { &self.content_hash }
    pub fn tbid(&self) -> &Tbid { &self.tbid }
    pub fn echo(&self) -> &str { &self.echo }
    pub fn tbn(&self) -> &str { &self.tbn }
    pub fn time_being_reference_time(&self) -> &str { &self.time_being_reference_time }

    /// Returns postcard-encoded canonical bytes for signing (v2 wire format).
    pub fn sig_input_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("postcard serialize Foretis")
    }
}

/// Trait for looking up ChrononRecords from a calendar or calendar-like store.
///
/// Implemented by the Calendar component. Chronomatter uses this for verification
/// without owning calendar data.
pub trait CalendarLookup: Send + Sync {
    /// Retrieves up to `count` chronon records starting from `chronon_number`.
    fn get(&self, chronon_number: u64, count: usize) -> Result<Vec<ChrononRecord>, NodeError>;
    /// Returns the chronon number of the most recent record, if any.
    fn latest(&self) -> Option<u64>;
    /// Returns the TBID of this calendar's owner.
    fn tbid(&self) -> Tbid;
    /// Returns the TimeBeing name.
    fn tbn(&self) -> &str;
}

/// V2 StampedForetis: payload + signature carried separately by the trust-boundary wrapper.
///
/// The `Foretis` payload is signature-free. The `signature_bytes` and `signature_algorithm`
/// are stored in the `UnverifiedSignatureEnvelope` / `CleanAuthenticated` wrappers.
pub struct StampedForetis {
    pub foretis:    Foretis,
    pub signature_bytes: Vec<u8>,
    pub signature_algorithm: String,
}

/// Stamp content under the current tick's key.
///
/// V2: signing bytes are `postcard(&foretis_payload)`.
/// Returns the signature-free payload plus signature bytes for the wrapper.
pub fn stamp(
    server: &dyn CryptoServer,
    clock: &dyn Clock,
    tbid: &Tbid,
    chronon_number: u64,
    content: &[u8],
    echo: &str,
    tbn: &str,
) -> Result<StampedForetis, NodeError> {
    let content_hash = server.sha256(content)?;

    let now_ns = clock.now_ns()
        .map_err(|e| NodeError::Internal(format!("clock error: {e}")))?;
    let time_being_reference_time = format!("UE+{}ns", now_ns);

    let foretis = Foretis {
        chronon_number,
        content_hash: content_hash.bytes.into(),
        tbid: *tbid,
        echo: echo.to_string(),
        tbn: tbn.to_string(),
        time_being_reference_time,
    };

    // V2: sign postcard canonical bytes of the payload
    let sig_input = postcard::to_allocvec(&foretis)
        .map_err(|e| NodeError::Internal(format!("postcard serialize error: {e}")))?;

    let signature = server.sign(&sig_input)?;
    let sig_alg = server.signature_algorithm().to_id_string().to_string();

    Ok(StampedForetis {
        foretis,
        signature_bytes: signature.bytes.to_vec(),
        signature_algorithm: sig_alg,
    })
}

/// Verify a Foretis against content and calendar (V2: signature comes from wrapper).
pub fn verify(
    server: &dyn CryptoServer,
    foretis: &Foretis,
    signature: &[u8],
    signature_algorithm: &str,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    let recomputed = server.sha256(content)?;
    if recomputed.bytes != *foretis.content_hash {
        return Ok(false);
    }

    let records = calendar.get(foretis.chronon_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick"))?;

    // V2: verify postcard canonical bytes of the payload
    let sig_input = postcard::to_allocvec(foretis)
        .map_err(|e| NodeError::Internal(format!("postcard serialize error: {e}")))?;

    Ok(server.verify_with(
        &rec.public_key,
        signature_algorithm,
        &sig_input,
        signature,
    )?)
}

/// Build auto-attestation blob: tbid || A.tick || A.pk || B.tick || B.pk || chronon_stamp_count || nonce
///
/// Returns the signed blob and the 16-byte nonce for storage in ChrononRecord.
/// The nonce ensures each blob is unique, preventing replay attacks.
/// `chronon_stamp_count` counts user-initiated stamps during tick B (excluding auto-attestation itself,
/// but including mutual attestations). This is knowable only to the Chronomatter that produced the tick.
pub fn auto_attestation_blob_with_count(
    tbid: &str,
    a_tick: u64,
    a_pk: &[u8; 32],
    b_tick: u64,
    b_pk: &[u8; 32],
    chronon_stamp_count: u64,
) -> Result<(Vec<u8>, [u8; 16]), NodeError> {
    let mut nonce = [0u8; 16];
    random_bytes(&mut nonce)?;
    let mut blob = Vec::with_capacity(tbid.len() + 8 + 32 + 8 + 32 + 8 + 16);
    blob.extend_from_slice(tbid.as_bytes());
    blob.extend_from_slice(&a_tick.to_be_bytes());
    blob.extend_from_slice(a_pk);
    blob.extend_from_slice(&b_tick.to_be_bytes());
    blob.extend_from_slice(b_pk);
    blob.extend_from_slice(&chronon_stamp_count.to_be_bytes());
    blob.extend_from_slice(&nonce);
    Ok((blob, nonce))
}

/// Build auto-attestation blob: tbid || A.tick || A.pk || B.tick || B.pk || nonce
///
/// Returns the signed blob and the 16-byte nonce for storage in ChrononRecord.
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

/// Build auto-attestation blob for genesis tick with embedded genesis signature:
/// tbid || A.tick || A.pk || B.tick || B.pk || genesis_sig || chronon_stamp_count || nonce
///
/// Returns the signed blob and the 16-byte nonce for storage in ChrononRecord.
/// The genesis_sig is included in the blob so both forward and backward foretis cover it.
pub fn auto_attestation_blob_with_genesis(
    tbid: &str,
    tick: u64,
    pk: &[u8; 32],
    genesis_sig: &[u8],
    chronon_stamp_count: u64,
) -> Result<(Vec<u8>, [u8; 16]), NodeError> {
    let mut nonce = [0u8; 16];
    random_bytes(&mut nonce)?;
    let mut blob = Vec::with_capacity(tbid.len() + 8 + 32 + 8 + 32 + genesis_sig.len() + 8 + 16);
    blob.extend_from_slice(tbid.as_bytes());
    blob.extend_from_slice(&tick.to_be_bytes());
    blob.extend_from_slice(pk);
    blob.extend_from_slice(&tick.to_be_bytes());
    blob.extend_from_slice(pk);
    blob.extend_from_slice(genesis_sig);
    blob.extend_from_slice(&chronon_stamp_count.to_be_bytes());
    blob.extend_from_slice(&nonce);
    Ok((blob, nonce))
}

/// Verify the auto-attestation between two consecutive tick records.
///
/// Rebuilds the blob from the two ticks using the nonce stored in `curr.aa_nonce`,
/// then verifies that both signatures cover the same blob.
/// At tick 1, extracts the genesis signature from forward/backward foretis and verifies it.
pub fn verify_pair(
    crypto: &dyn CryptoServer,
    tbid_str: &str,
    prev: &ChrononRecord,
    curr: &ChrononRecord,
) -> Result<bool, NodeError> {
    let nonce = curr.aa_nonce;
    let stamps = curr.chronon_stamp_count;

    let (forward_sig, backward_sig, attest_blob, genesis_valid) = if curr.chronon_number == 1 && curr.tb_version == 1 {
        let forward = &curr.forward_foretis;
        let backward = &curr.backward_foretis;

        // Validate minimum length for genesis foretis
        // forward_foretis and backward_foretis must contain at least an Ed25519 signature (64 bytes)
        const ED25519_SIG_LEN: usize = 64;
        if forward.len() < ED25519_SIG_LEN {
            return Err(NodeError::InvalidInput(
                "forward_foretis too short for genesis split".into()
            ));
        }
        if backward.len() < ED25519_SIG_LEN {
            return Err(NodeError::InvalidInput(
                "backward_foretis too short for genesis split".into()
            ));
        }

        let ed25519_sig_len = 64usize;

        let forward_ed_sig = if forward.len() > ed25519_sig_len {
            &forward[..ed25519_sig_len]
        } else {
            forward
        };
        let forward_genesis = if forward.len() > ed25519_sig_len {
            &forward[ed25519_sig_len..]
        } else {
            &[]
        };

        let backward_ed_sig = if backward.len() > ed25519_sig_len {
            &backward[..ed25519_sig_len]
        } else {
            backward
        };
        let backward_genesis = if backward.len() > ed25519_sig_len {
            &backward[ed25519_sig_len..]
        } else {
            &[]
        };

        let genesis_match = forward_genesis == backward_genesis;

        let mut genesis_blob = Vec::with_capacity(96 + 8 + curr.public_key.len());
        genesis_blob.extend_from_slice(&prev.tbid.raw_bytes());
        genesis_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
        genesis_blob.extend_from_slice(&curr.public_key);

        let genesis_valid = if !forward_genesis.is_empty() && genesis_match {
            let pub_bytes = crate::foretias::types::SignatureBytes::from(prev.tbid.raw_bytes().clone());
            let sig = crate::foretias::types::SignatureBytes::from(forward_genesis.to_vec());
            crate::crypto_server::signing_tbid::tbid_verify(
                &pub_bytes,
                &genesis_blob,
                &sig,
            )
            .map_err(|e| NodeError::Crypto(e))?
        } else if forward_genesis.is_empty() && curr.tb_version == 0 {
            true
        } else {
            false
        };

        let attest_blob = if !forward_genesis.is_empty() {
            let mut attest_blob = Vec::with_capacity(tbid_str.len() + 8 + 32 + 8 + 32 + forward_genesis.len() + 8 + 16);
            attest_blob.extend_from_slice(tbid_str.as_bytes());
            attest_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
            attest_blob.extend_from_slice(&curr.public_key);
            attest_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
            attest_blob.extend_from_slice(&curr.public_key);
            attest_blob.extend_from_slice(forward_genesis);
            attest_blob.extend_from_slice(&stamps.to_be_bytes());
            attest_blob.extend_from_slice(&nonce[..]);
            attest_blob
        } else {
            let mut attest_blob = Vec::with_capacity(tbid_str.len() + 8 + 32 + 8 + 32 + 8 + 16);
            attest_blob.extend_from_slice(tbid_str.as_bytes());
            attest_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
            attest_blob.extend_from_slice(&curr.public_key);
            attest_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
            attest_blob.extend_from_slice(&curr.public_key);
            attest_blob.extend_from_slice(&stamps.to_be_bytes());
            attest_blob.extend_from_slice(&nonce[..]);
            attest_blob
        };

        (forward_ed_sig.to_vec().into(), backward_ed_sig.to_vec().into(), attest_blob, genesis_valid)
    } else {
        let mut attest_blob = Vec::with_capacity(tbid_str.len() + 8 + prev.public_key.len() + 8 + curr.public_key.len() + 8 + 16);
        attest_blob.extend_from_slice(tbid_str.as_bytes());
        attest_blob.extend_from_slice(&prev.chronon_number.to_be_bytes());
        attest_blob.extend_from_slice(&prev.public_key);
        attest_blob.extend_from_slice(&curr.chronon_number.to_be_bytes());
        attest_blob.extend_from_slice(&curr.public_key);
        attest_blob.extend_from_slice(&stamps.to_be_bytes());
        attest_blob.extend_from_slice(&nonce[..]);

        (curr.forward_foretis.clone(), curr.backward_foretis.clone(), attest_blob, true)
    };

    let forward_valid = crypto.verify_with(
        &prev.public_key,
        &curr.signature_algorithm,
        &attest_blob,
        &forward_sig,
    )?;

    let backward_valid = crypto.verify_with(
        &curr.public_key,
        &curr.signature_algorithm,
        &attest_blob,
        &backward_sig,
    )?;

    Ok(forward_valid && backward_valid && genesis_valid)
}

impl super::clean_auth::RecordBase for ChrononRecord {
    fn always_require_full_signature(&self) -> bool {
        false
    }
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
    use crate::foretias::clean_auth::RecordBase;
    use postcard;

    fn make_server() -> Box<dyn CryptoServer> {
        crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519)
            .expect("failed to create software crypto server")
    }

    fn make_cal(server: &dyn CryptoServer) -> Calendar {
        let tbid = Tbid::from_raw([0xAA; 96]);
        let mut cal = Calendar::new(tbid, "test-cal");
        let chronon_number = 1;
        let content = b"init";
        let foretis = stamp(server, &SystemClock, &tbid, chronon_number, content, "init", "test-cal")
            .expect("stamp init tick");
        let public_key = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes.to_vec(),
        };
        cal.append(ChrononRecord {
            chronon_number,
            public_key: public_key.into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: serde_json::to_vec(&foretis.foretis).unwrap().into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        }).unwrap();
        cal
    }

    #[test]
    fn stamp_creates_valid_foretis() {
        let server = make_server();
        let tbid = Tbid::from_raw([1u8; 96]);
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 42, b"hello", "echo-42", "tbn")
            .expect("stamp should succeed");
        assert_eq!(foretis.foretis.chronon_number, 42);
        assert_eq!(foretis.foretis.tbid, tbid);
        assert_eq!(foretis.foretis.echo, "echo-42");
        assert_eq!(foretis.foretis.tbn, "tbn");
        assert!(!foretis.signature_bytes.is_empty());
        assert!(!foretis.foretis.time_being_reference_time.is_empty());
    }

    #[test]
    fn stamp_different_content_different_hash() {
        let server = make_server();
        let tbid = Tbid::from_raw([2u8; 96]);
        let f1 = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"aaa", "e", "t").unwrap();
        let f2 = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"bbb", "e", "t").unwrap();
        assert_ne!(f1.foretis.content_hash, f2.foretis.content_hash);
    }

    #[test]
    fn stamp_empty_content_produces_valid_stamp() {
        let server = make_server();
        let tbid = Tbid::from_raw([3u8; 96]);
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, b"", "empty", "t").unwrap();
        assert_eq!(foretis.foretis.chronon_number, 1);
        assert!(!foretis.signature_bytes.is_empty());
    }

    #[test]
    fn verify_succeeds_with_correct_content() {
        let server = make_server();
        let cal = make_cal(server.as_ref());
        let tbid = Tbid::from_raw([0xAA; 96]);
        let content = b"init";
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, content, "init", "test-cal")
            .expect("stamp");
        let valid = verify(server.as_ref(), &foretis.foretis, &foretis.signature_bytes, &foretis.signature_algorithm, content, &cal)
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
        let valid = verify(server.as_ref(), &foretis.foretis, &foretis.signature_bytes, &foretis.signature_algorithm, b"wrong", &cal)
            .expect("verify should not error");
        assert!(!valid);
    }

    #[test]
    fn verify_fails_with_wrong_public_key() {
        let server = make_server();
        let tbid = Tbid::from_raw([0xBB; 96]);
        let mut cal = Calendar::new(tbid, "bad-cal");
        cal.append(ChrononRecord {
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
        }).unwrap();
        let content = b"test";
        let foretis = stamp(server.as_ref(), &SystemClock, &tbid, 1, content, "e", "bad-cal")
            .expect("stamp");
        let result = verify(server.as_ref(), &foretis.foretis, &foretis.signature_bytes, &foretis.signature_algorithm, content, &cal);
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

        let prev = ChrononRecord {
            chronon_number: 1,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        };
        let curr = ChrononRecord {
            chronon_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: FTByteVector::from(sig_bytes.clone()),
            backward_foretis: FTByteVector::from(sig_bytes),
            aa_nonce: FTByteArray::from(nonce),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
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

        let prev = ChrononRecord {
            chronon_number: 1,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        };
        let curr = ChrononRecord {
            chronon_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg.clone(),
            forward_foretis: FTByteVector::from(sig_bytes.clone()),
            backward_foretis: FTByteVector::from(sig_bytes.clone()),
            aa_nonce: FTByteArray::from(nonce),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        };

        assert!(verify_pair(server.as_ref(), &tbid_str, &prev, &curr).unwrap());

        sig_bytes[0] ^= 0xFF;
        let curr_tampered = ChrononRecord {
            chronon_number: 2,
            public_key: pub_key.to_vec().into(),
            signature_algorithm: sig_alg,
            forward_foretis: FTByteVector::from(sig_bytes),
            backward_foretis: vec![0u8; 64].into(),
            aa_nonce: FTByteArray::from(nonce),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),

            tb_version: 0,
            tbid: Tbid::default(),
        };

let valid = verify_pair(server.as_ref(), &tbid_str, &prev, &curr_tampered).unwrap();
        assert!(!valid);
    }

    #[test]
    fn verify_pair_rejects_short_genesis_foretis() {
        let server = make_server();
        let tbid = Tbid::from_raw([0xEE; 96]);
        let tbid_str = tbid.to_hex();

        let prev = ChrononRecord {
            chronon_number: 0,
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
        let curr = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0u8; 50].into(),
            backward_foretis: vec![0u8; 70].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };

        let result = verify_pair(server.as_ref(), &tbid_str, &prev, &curr);
        assert!(
            result.is_err(),
            "verify_pair should reject forward_foretis shorter than 64 bytes at genesis"
        );
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

    #[test]
    fn chronon_record_deserialize_requires_chronon_stamp_count() {
        let record = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0x01u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x02u8; 64].into(),
            backward_foretis: vec![0x03u8; 64].into(),
            aa_nonce: [0x04u8; 16].into(),
            chronon_stamp_count: 5,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let json_missing = json.replace(
            r#""chronon_stamp_count":5"#,
            "",
        );
        let json_missing = if json_missing.contains(r#"":}"#) {
            json_missing.replace(r#"":}"#, r#"}}"#)
        } else {
            json_missing.replace(r#"": ,"#, r#"},"#)
        };

        let result = serde_json::from_str::<ChrononRecord>(&json_missing);
        assert!(
            result.is_err(),
            "deserialization must fail when chronon_stamp_count is omitted; got: {json_missing}"
        );
    }

    #[test]
    fn chronon_record_deserialize_succeeds_with_chronon_stamp_count_zero() {
        let record = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0x01u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x02u8; 64].into(),
            backward_foretis: vec![0x03u8; 64].into(),
            aa_nonce: [0x04u8; 16].into(),
            chronon_stamp_count: 7,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let json_with_zero = json.replace(
            r#""chronon_stamp_count":7"#,
            r#""chronon_stamp_count":0"#,
        );

        let result = serde_json::from_str::<ChrononRecord>(&json_with_zero);
        assert!(
            result.is_ok(),
            "deserialization must succeed when chronon_stamp_count is explicitly zero"
        );
        assert_eq!(result.unwrap().chronon_stamp_count, 0);
    }

    #[test]
    fn postcard_round_trip_chronon_record() {
        let record = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0x01u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x02u8; 64].into(),
            backward_foretis: vec![0x03u8; 64].into(),
            aa_nonce: [0x04u8; 16].into(),
            chronon_stamp_count: 7,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };
        let bytes = postcard::to_allocvec(&record).expect("postcard serialize");
        let decoded: ChrononRecord =
            postcard::from_bytes(&bytes).expect("postcard deserialize");
        assert_eq!(decoded.chronon_number, record.chronon_number);
        assert_eq!(decoded.public_key.as_slice(), record.public_key.as_slice());
        assert_eq!(decoded.signature_algorithm, record.signature_algorithm);
        assert_eq!(decoded.forward_foretis.as_slice(), record.forward_foretis.as_slice());
        assert_eq!(decoded.backward_foretis.as_slice(), record.backward_foretis.as_slice());
        assert_eq!(decoded.aa_nonce.as_slice(), record.aa_nonce.as_slice());
        assert_eq!(decoded.chronon_stamp_count, record.chronon_stamp_count);
        assert_eq!(decoded.tb_version, record.tb_version);
        assert_eq!(decoded.tbid, record.tbid);
    }

    #[test]
    fn postcard_determinism_chronon_record() {
        let record = ChrononRecord {
            chronon_number: 42,
            public_key: vec![0xABu8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0xCDu8; 64].into(),
            backward_foretis: vec![0xEFu8; 64].into(),
            aa_nonce: [0x12u8; 16].into(),
            chronon_stamp_count: 99,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };
        let bytes1 = postcard::to_allocvec(&record).expect("serialize 1");
        let bytes2 = postcard::to_allocvec(&record).expect("serialize 2");
        assert_eq!(
            bytes1, bytes2,
            "postcard must produce deterministic output for the same input"
        );
    }

    #[test]
    fn record_base_always_require_full_signature_default() {
        let record = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0x01u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x02u8; 64].into(),
            backward_foretis: vec![0x03u8; 64].into(),
            aa_nonce: [0x04u8; 16].into(),
            chronon_stamp_count: 7,
            external_attestations: Vec::new(),
            tb_version: 1,
            tbid: Tbid::default(),
        };
        assert!(
            !record.always_require_full_signature(),
            "ChrononRecord should not require full signature by default"
        );
    }
}
