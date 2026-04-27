//! Fortias domain types: TickRecord, Fortis, and stamp/verify operations.

use serde::{Deserialize, Serialize};

use crate::crypto_server::CryptoServer;
use crate::core::bindings::{FortiasPubKey32, FortiasSig64};
use crate::error::NodeError;

/// A single entry in the Calendar, linking consecutive ticks via Fortis attestations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickRecord {
    /// The monotonically increasing tick index.
    pub tick_number: u64,
    /// The public key active at this tick.
    pub public_key: Vec<u8>,
    /// Serialized Fortis attesting forward to the next tick.
    pub forward_fortis: Vec<u8>,
    /// Serialized Fortis attesting backward to the previous tick.
    pub backward_fortis: Vec<u8>,
}

/// A cryptographically signed attestation of content at a specific tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fortis {
    /// The tick number at which this attestation was created.
    pub tick_number: u64,
    /// SHA-256 hash of the attested content.
    pub content_hash: [u8; 32],
    /// Ed25519 signature over the content and tick metadata.
    pub signature: Vec<u8>,
    /// TimeBeing identifier of the signing node.
    pub tbid: [u8; 16],
    /// Echo string identifying the tick (e.g. `"tick-42"`).
    pub echo: String,
    /// TimeBeing name (human-readable identifier).
    pub tbn: String,
    /// Server wall-clock time at stamping, in `"UE+<nanoseconds>ns"` format.
    pub time_being_reference_time: String,
}

/// Trait for looking up TickRecords from a calendar or calendar-like store.
pub trait CalendarLookup: Send + Sync {
    /// Retrieves up to `count` tick records starting from `tick_number`.
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError>;
    /// Returns the tick number of the most recent record, if any.
    fn latest(&self) -> Option<u64>;
}

/// Stamp content under the current tick's key.
pub fn stamp(
    server: &dyn CryptoServer,
    tbid: &[u8; 16],
    tick_number: u64,
    content: &[u8],
    echo: &str,
    tbn: &str,
) -> Result<Fortis, NodeError> {
    let mut sig_input = Vec::with_capacity(16 + 8 + content.len());
    sig_input.extend_from_slice(tbid);
    sig_input.extend_from_slice(&tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let signature = server.sign(&sig_input)?;
    let content_hash = server.sha256(content)?;

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| NodeError::Internal(format!("SystemTime before UNIX_EPOCH: {}", e)))?
        .as_nanos() as u64;
    let time_being_reference_time = format!("UE+{}ns", now_ns);

    Ok(Fortis {
        tick_number,
        content_hash: content_hash.bytes,
        signature: signature.bytes.to_vec(),
        tbid: *tbid,
        echo: echo.to_string(),
        tbn: tbn.to_string(),
        time_being_reference_time,
    })
}

/// Verify a Fortis against content and calendar.
pub fn verify(
    server: &dyn CryptoServer,
    fortis: &Fortis,
    content: &[u8],
    calendar: &dyn CalendarLookup,
) -> Result<bool, NodeError> {
    let recomputed = server.sha256(content)?;
    if recomputed.bytes != fortis.content_hash {
        return Ok(false);
    }

    let records = calendar.get(fortis.tick_number, 1)?;
    let rec = records.first().ok_or(NodeError::NotFound("tick"))?;

    let mut sig_input = Vec::new();
    sig_input.extend_from_slice(&fortis.tbid);
    sig_input.extend_from_slice(&fortis.tick_number.to_be_bytes());
    sig_input.extend_from_slice(content);

    let pub_key_bytes: [u8; 32] = rec.public_key[..32].try_into()
        .map_err(|_| NodeError::BadFormat("public_key"))?;
    let pub_key = FortiasPubKey32 { bytes: pub_key_bytes };

    let sig_bytes: [u8; 64] = fortis.signature[..].try_into()
        .map_err(|_| NodeError::BadFormat("signature"))?;
    let sig = FortiasSig64 { bytes: sig_bytes };

    Ok(server.verify_ed25519(&pub_key, &sig_input, &sig)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use crate::fortias::calendar::Calendar;

    fn make_server() -> Box<dyn CryptoServer> {
        crypto_server::new_software(crate::crypto_server::FortiasCurve::Ed25519)
            .expect("failed to create software crypto server")
    }

    fn make_cal(server: &dyn CryptoServer) -> Calendar {
        let tbid: [u8; 16] = [0xAA; 16];
        let mut cal = Calendar::new(tbid, "test-cal");
        let tick_number = 1;
        let content = b"init";
        let fortis = stamp(server, &tbid, tick_number, content, "init", "test-cal")
            .expect("stamp init tick");
        let public_key = match server.public_key() {
            crate::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            crate::crypto_server::PublicKeyBytes::P256Compressed(pk) => pk.bytes.to_vec(),
        };
        cal.append(TickRecord {
            tick_number,
            public_key,
            forward_fortis: serde_json::to_vec(&fortis).unwrap(),
            backward_fortis: vec![],
        }).unwrap();
        cal
    }

    #[test]
    fn stamp_creates_valid_fortis() {
        let server = make_server();
        let tbid: [u8; 16] = [1u8; 16];
        let fortis = stamp(server.as_ref(), &tbid, 42, b"hello", "echo-42", "tbn")
            .expect("stamp should succeed");
        assert_eq!(fortis.tick_number, 42);
        assert_eq!(fortis.tbid, tbid);
        assert_eq!(fortis.echo, "echo-42");
        assert_eq!(fortis.tbn, "tbn");
        assert!(!fortis.signature.is_empty());
        assert!(!fortis.time_being_reference_time.is_empty());
    }

    #[test]
    fn stamp_different_content_different_hash() {
        let server = make_server();
        let tbid: [u8; 16] = [2u8; 16];
        let f1 = stamp(server.as_ref(), &tbid, 1, b"aaa", "e", "t").unwrap();
        let f2 = stamp(server.as_ref(), &tbid, 1, b"bbb", "e", "t").unwrap();
        assert_ne!(f1.content_hash, f2.content_hash);
    }

    #[test]
    fn stamp_empty_content_produces_valid_stamp() {
        let server = make_server();
        let tbid: [u8; 16] = [3u8; 16];
        let fortis = stamp(server.as_ref(), &tbid, 1, b"", "empty", "t").unwrap();
        assert_eq!(fortis.tick_number, 1);
        assert!(!fortis.signature.is_empty());
    }

    #[test]
    fn verify_succeeds_with_correct_content() {
        let server = make_server();
        let cal = make_cal(server.as_ref());
        let tbid: [u8; 16] = [0xAA; 16];
        let content = b"init";
        let fortis = stamp(server.as_ref(), &tbid, 1, content, "init", "test-cal")
            .expect("stamp");
        let valid = verify(server.as_ref(), &fortis, content, &cal)
            .expect("verify should not error");
        assert!(valid);
    }

    #[test]
    fn verify_fails_with_wrong_content() {
        let server = make_server();
        let cal = make_cal(server.as_ref());
        let tbid: [u8; 16] = [0xAA; 16];
        let content = b"init";
        let fortis = stamp(server.as_ref(), &tbid, 1, content, "init", "test-cal")
            .expect("stamp");
        let valid = verify(server.as_ref(), &fortis, b"wrong", &cal)
            .expect("verify should not error");
        assert!(!valid);
    }

    #[test]
    fn verify_fails_with_wrong_public_key() {
        let server = make_server();
        let tbid: [u8; 16] = [0xBB; 16];
        let mut cal = Calendar::new(tbid, "bad-cal");
        cal.append(TickRecord {
            tick_number: 1,
            public_key: vec![0u8; 32],
            forward_fortis: vec![],
            backward_fortis: vec![],
        }).unwrap();
        let content = b"test";
        let fortis = stamp(server.as_ref(), &tbid, 1, content, "e", "bad-cal")
            .expect("stamp");
        let result = verify(server.as_ref(), &fortis, content, &cal);
        if let Ok(valid) = result {
            assert!(!valid);
        }
    }
}
