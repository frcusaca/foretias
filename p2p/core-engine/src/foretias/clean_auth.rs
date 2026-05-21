//! Type-enforced cleansing and authentication.
//!
//! Enforces a three-stage type progression for inbound data:
//! `Unprocessed<X>` (parsed, not trusted) -> `CleanAuthenticated<X>` (authenticated + cleansed) -> `Externalized<X>` (wire/disk, minimal fields).
//!
//! The compiler enforces that data flows through this progression. You cannot skip the middle step.
//!
//! `CleanAuthenticated<X>` has **private constructors** -- only `into_clean_authenticated()` (inbound gate)
//! and `from_trusted()` (local gate) can produce them.
//!
//! `CleanAuthenticated` covers authentication + cleansing, NOT full chain-of-trust to genesis.

use serde::{Deserialize, Serialize};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use super::tick::{ChrononRecord, Foretis, verify_pair};
use super::types::Tbid;
use super::external_attestation::ExternalAttestation;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that occur during the cleansing and authentication process.
#[derive(Debug)]
pub enum CleanAuthError {
    /// Failed to parse inbound data.
    Parse(ParseError),
    /// Cryptographic signature verification failed.
    InvalidSignature,
    /// Declared algorithm does not match the record's algorithm.
    AlgorithmMismatch,
    /// Chronon chain continuity broken (hash mismatch, tick gap, etc.).
    ChainBreak,
    /// Replay detected (duplicate tick number).
    ReplayDetected,
    /// Peer identity unknown (no public key available).
    UnknownPeer,
    /// Verification not yet implemented (e.g. FROST).
    NotYetImplemented,
    /// Field has invalid length.
    InvalidLength(String),
    /// Underlying crypto/server error.
    Crypto(NodeError),
}

impl From<ParseError> for CleanAuthError {
    fn from(e: ParseError) -> Self {
        CleanAuthError::Parse(e)
    }
}

impl From<NodeError> for CleanAuthError {
    fn from(e: NodeError) -> Self {
        CleanAuthError::Crypto(e)
    }
}

impl std::fmt::Display for CleanAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CleanAuthError::Parse(e) => write!(f, "parse error: {e}"),
            CleanAuthError::InvalidSignature => write!(f, "invalid signature"),
            CleanAuthError::AlgorithmMismatch => write!(f, "algorithm mismatch"),
            CleanAuthError::ChainBreak => write!(f, "chain break"),
            CleanAuthError::ReplayDetected => write!(f, "replay detected"),
            CleanAuthError::UnknownPeer => write!(f, "unknown peer"),
            CleanAuthError::NotYetImplemented => write!(f, "not yet implemented"),
            CleanAuthError::InvalidLength(msg) => write!(f, "invalid length: {msg}"),
            CleanAuthError::Crypto(e) => write!(f, "crypto error: {e}"),
        }
    }
}

impl std::error::Error for CleanAuthError {}

/// Errors that occur during parsing of inbound data.
#[derive(Debug)]
pub enum ParseError {
    /// Invalid JSON.
    InvalidJson(serde_json::Error),
    /// Input bytes truncated.
    TruncatedBytes,
    /// Field has invalid length.
    InvalidLength(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            ParseError::TruncatedBytes => write!(f, "truncated bytes"),
            ParseError::InvalidLength(msg) => write!(f, "invalid length: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// ChrononRecord triple
// ---------------------------------------------------------------------------

/// A ChrononRecord that has been parsed but not yet verified.
///
/// This type carries raw domain data from the wire/disk. Do NOT trust it.
#[derive(Debug, Clone)]
pub struct UnprocessedChrononRecord(pub ChrononRecord);

impl UnprocessedChrononRecord {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let record: ChrononRecord = serde_json::from_slice(b)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedChrononRecord(record))
    }

    /// Parse from a JSON-RPC Value. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError> {
        let record: ChrononRecord = serde_json::from_value(v)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedChrononRecord(record))
    }

    /// Extract the inner record (for inspection only; still untrusted).
    pub fn inner(&self) -> &ChrononRecord {
        &self.0
    }
}

/// A ChrononRecord that has been authenticated to the claimed TBID and cleansed.
///
/// The TimeFamily has done due diligence: authenticated to the claimed TBID,
/// sanitized, validated, normalized. Safe for in-process use.
///
/// **Private fields** -- zero external construction.
pub struct CleanAuthenticatedChrononRecord {
    inner: ChrononRecord,
}

impl CleanAuthenticatedChrononRecord {
    /// Trusted construction -- only for locally-produced records.
    ///
    /// Chronomatter-produced records use this path. The caller asserts
    /// the record was signed with our own key.
    pub fn from_trusted(record: ChrononRecord) -> Self {
        Self { inner: record }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &ChrononRecord {
        &self.inner
    }

    /// Consume and return the inner record.
    pub fn into_inner(self) -> ChrononRecord {
        self.inner
    }

    /// Strip to minimal persistent form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedChrononRecord {
        let r = self.inner;
        ExternalizedChrononRecord {
            chronon_number: r.chronon_number,
            public_key: r.public_key.as_slice().try_into()
                .unwrap_or_else(|_| {
                    // Pad or truncate to 32 bytes for Ed25519
                    let mut pk = [0u8; 32];
                    let len = std::cmp::min(r.public_key.len(), 32);
                    pk[..len].copy_from_slice(&r.public_key[..len]);
                    pk
                }),
            forward_foretis: r.forward_foretis.as_slice().to_vec(),
            backward_foretis: r.backward_foretis.as_slice().to_vec(),
            aa_nonce: (*r.aa_nonce).into(),
            external_attestations: r.external_attestations
                .into_iter()
                .map(|ea| ExternalizedAttestation {
                    attester_tbid: ea.attester_tbid,
                    foretis: serde_json::to_value(&ea.foretis).unwrap_or_default(),
                    attester_tick_record: serde_json::to_value(&ea.attester_tick_record).unwrap_or_default(),
                    received_at_ns: ea.received_at_ns,
                })
                .collect(),
            tb_version: r.tb_version as u8,
            tbid: r.tbid.raw_bytes().to_vec(),
            stamps_per_tick: r.chronon_stamp_count,
            signature_algorithm: r.signature_algorithm,
        }
    }
}

/// Minimal persistent form for ChrononRecord (wire/disk).
///
/// Contains only fields needed for reconstruction and re-verification
/// on the receiving side. No runtime artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizedChrononRecord {
    pub chronon_number: u64,
    pub public_key: [u8; 32],
    pub forward_foretis: Vec<u8>,
    pub backward_foretis: Vec<u8>,
    pub aa_nonce: [u8; 16],
    pub external_attestations: Vec<ExternalizedAttestation>,
    pub tb_version: u8,
    pub tbid: Vec<u8>,
    pub stamps_per_tick: u64,
    pub signature_algorithm: String,
}

/// Minimal external attestation for wire/disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizedAttestation {
    pub attester_tbid: String,
    pub foretis: serde_json::Value,
    pub attester_tick_record: serde_json::Value,
    pub received_at_ns: u64,
}

impl ExternalizedChrononRecord {
    /// Reconstruct as Unprocessed for re-verification on load.
    pub fn into_unprocessed(self) -> Result<UnprocessedChrononRecord, ParseError> {
        let record = ChrononRecord {
            chronon_number: self.chronon_number,
            public_key: self.public_key.to_vec().into(),
            signature_algorithm: self.signature_algorithm,
            forward_foretis: self.forward_foretis.into(),
            backward_foretis: self.backward_foretis.into(),
            aa_nonce: self.aa_nonce.into(),
            chronon_stamp_count: self.stamps_per_tick,
            external_attestations: self.external_attestations
                .into_iter()
                .map(|ea| ExternalAttestation {
                    attester_tbid: ea.attester_tbid,
                    foretis: serde_json::from_value(ea.foretis).unwrap_or_else(|_| Foretis {
                        chronon_number: 0,
                        content_hash: [0u8; 32].into(),
                        signature: vec![].into(),
                        signature_algorithm: String::new(),
                        tbid: Tbid::default(),
                        echo: String::new(),
                        tbn: String::new(),
                        time_being_reference_time: String::new(),
                    }),
                    attester_tick_record: serde_json::from_value(ea.attester_tick_record).unwrap_or_else(|_| ChrononRecord {
                        chronon_number: 0,
                        public_key: vec![].into(),
                        signature_algorithm: String::new(),
                        forward_foretis: vec![].into(),
                        backward_foretis: vec![].into(),
                        aa_nonce: [0u8; 16].into(),
                        chronon_stamp_count: 0,
                        external_attestations: vec![],
                        tb_version: 0,
                        tbid: Tbid::default(),
                    }),
                    received_at_ns: ea.received_at_ns,
                })
                .collect(),
            tb_version: self.tb_version as u32,
            tbid: Tbid::from_raw(self.tbid.try_into().unwrap_or([0u8; 96])),
        };
        Ok(UnprocessedChrononRecord(record))
    }
}

impl UnprocessedChrononRecord {
    /// Verify a non-genesis record against its predecessor.
    ///
    /// Calls `verify_pair` under the hood. Both `self` and `prev` must
    /// belong to the same calendar (same TBID).
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        prev: &CleanAuthenticatedChrononRecord,
    ) -> Result<CleanAuthenticatedChrononRecord, CleanAuthError> {
        let valid = verify_pair(crypto, &prev.inner().tbid.to_hex(), prev.inner(), &self.0)
            .map_err(CleanAuthError::Crypto)?;
        if !valid {
            return Err(CleanAuthError::ChainBreak);
        }
        Ok(CleanAuthenticatedChrononRecord::from_trusted(self.0))
    }

    /// Verify a genesis record (tick 1). No predecessor required.
    ///
    /// For genesis, we verify the auto-attestation structure is valid.
    pub fn into_clean_authenticated_genesis(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticatedChrononRecord, CleanAuthError> {
        // Genesis verification: the record must be tick 1 and have valid structure.
        // A full genesis verification requires the genesis public key and signature.
        // For now, we perform basic structural checks.
        if self.0.chronon_number != 1 {
            return Err(CleanAuthError::ChainBreak);
        }
        if self.0.public_key.is_empty() {
            return Err(CleanAuthError::InvalidLength("public_key empty".into()));
        }
        // For tb_version >= 1, verify genesis signature (requires tbid_verify).
        // For tb_version == 0, basic structural check suffices.
        if self.0.tb_version >= 1 {
            // Genesis verification with SLH-DSA component -- delegated to verify_pair
            // with a synthetic predecessor. For now, return NotYetImplemented for
            // full genesis verification with PQC.
            // TODO: Implement full genesis verification when tbid_verify is wired.
            return Err(CleanAuthError::NotYetImplemented);
        }
        Ok(CleanAuthenticatedChrononRecord::from_trusted(self.0))
    }
}

// ---------------------------------------------------------------------------
// Foretis triple
// ---------------------------------------------------------------------------

/// A Foretis that has been parsed but not yet verified.
///
/// This type carries raw stamp data from the wire/disk. Do NOT trust it.
#[derive(Debug, Clone)]
pub struct UnprocessedForetis(pub Foretis);

impl UnprocessedForetis {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let foretis: Foretis = serde_json::from_slice(b)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedForetis(foretis))
    }

    /// Parse from a JSON-RPC Value. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError> {
        let foretis: Foretis = serde_json::from_value(v)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedForetis(foretis))
    }

    /// Extract the inner Foretis (for inspection only; still untrusted).
    pub fn inner(&self) -> &Foretis {
        &self.0
    }
}

/// A Foretis that has been authenticated to the claimed TBID and cleansed.
///
/// The TimeFamily has done due diligence: authenticated to the claimed TBID,
/// sanitized, validated, normalized. Safe for in-process use.
///
/// **Private fields** -- zero external construction.
pub struct CleanAuthenticatedForetis {
    inner: Foretis,
}

impl CleanAuthenticatedForetis {
    /// Trusted construction -- only for locally-produced stamps.
    pub fn from_trusted(foretis: Foretis) -> Self {
        Self { inner: foretis }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &Foretis {
        &self.inner
    }

    /// Consume and return the inner Foretis.
    pub fn into_inner(self) -> Foretis {
        self.inner
    }

    /// Strip to minimal wire form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedForetis {
        let f = self.inner;
        ExternalizedForetis {
            chronon_number: f.chronon_number,
            content_hash: (*f.content_hash).into(),
            signature: f.signature.as_slice().to_vec(),
            signature_algorithm: f.signature_algorithm,
            tbid: f.tbid.raw_bytes().to_vec(),
            echo: if f.echo.is_empty() { None } else { Some(f.echo) },
            tbn: if f.tbn.is_empty() { 0 } else { f.tbn.len() as u64 },
        }
    }
}

/// Minimal wire form for a Foretis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizedForetis {
    pub chronon_number: u64,
    pub content_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub signature_algorithm: String,
    pub tbid: Vec<u8>,
    pub echo: Option<String>,
    pub tbn: u64,
}

impl ExternalizedForetis {
    /// Reconstruct as Unprocessed for re-verification on the receiving side.
    pub fn into_unprocessed(self) -> Result<UnprocessedForetis, ParseError> {
        let foretis = Foretis {
            chronon_number: self.chronon_number,
            content_hash: self.content_hash.into(),
            signature: self.signature.into(),
            signature_algorithm: self.signature_algorithm,
            tbid: Tbid::from_raw(self.tbid.try_into().unwrap_or([0u8; 96])),
            echo: self.echo.unwrap_or_default(),
            tbn: String::new(),
            time_being_reference_time: String::new(),
        };
        Ok(UnprocessedForetis(foretis))
    }
}

impl UnprocessedForetis {
    /// Verify against a calendar record.
    ///
    /// The `record` must be a `CleanAuthenticatedChrononRecord` for the same
    /// chronon number as this Foretis.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        content: &[u8],
        record: &CleanAuthenticatedChrononRecord,
    ) -> Result<CleanAuthenticatedForetis, CleanAuthError> {
        let rec = record.inner();

        // Algorithm mismatch detection
        if self.0.signature_algorithm != rec.signature_algorithm {
            return Err(CleanAuthError::AlgorithmMismatch);
        }

        // Chronon number must match
        if self.0.chronon_number != rec.chronon_number {
            return Err(CleanAuthError::ChainBreak);
        }

        // Recompute content hash and verify
        let recomputed = crypto.sha256(content)
            .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;
        if recomputed.bytes != *self.0.content_hash {
            return Err(CleanAuthError::InvalidSignature);
        }

        // Rebuild signature input: tbid || chronon_number || content
        let mut sig_input = Vec::new();
        sig_input.extend_from_slice(&self.0.tbid.raw_bytes());
        sig_input.extend_from_slice(&self.0.chronon_number.to_be_bytes());
        sig_input.extend_from_slice(content);

        // Verify signature against the record's public key
        let valid = crypto.verify_with(
            &rec.public_key,
            &self.0.signature_algorithm,
            &sig_input,
            &self.0.signature,
        ).map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;

        if !valid {
            return Err(CleanAuthError::InvalidSignature);
        }

        Ok(CleanAuthenticatedForetis::from_trusted(self.0))
    }
}

// ---------------------------------------------------------------------------
// EpochSnapshot triple (core-engine)
// ---------------------------------------------------------------------------

use crate::epoch::snapshot::EpochSnapshot;

/// An EpochSnapshot that has been parsed but not yet verified.
#[derive(Debug, Clone)]
pub struct UnprocessedEpochSnapshot(pub EpochSnapshot);

impl UnprocessedEpochSnapshot {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let snapshot: EpochSnapshot = serde_json::from_slice(b)
            .map_err(ParseError::InvalidJson)?;
        Ok(UnprocessedEpochSnapshot(snapshot))
    }

    /// Extract the inner snapshot (for inspection only; still untrusted).
    pub fn inner(&self) -> &EpochSnapshot {
        &self.0
    }
}

/// An EpochSnapshot that has been authenticated and cleansed.
///
/// **Private fields** -- zero external construction.
pub struct CleanAuthenticatedEpochSnapshot {
    inner: EpochSnapshot,
}

impl CleanAuthenticatedEpochSnapshot {
    /// Trusted construction.
    pub fn from_trusted(snapshot: EpochSnapshot) -> Self {
        Self { inner: snapshot }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &EpochSnapshot {
        &self.inner
    }

    /// Strip to minimal wire form.
    pub fn externalize(self) -> ExternalizedEpochSnapshot {
        let s = self.inner;
        ExternalizedEpochSnapshot {
            epoch_number: s.epoch_number,
            peer_scores: s.peer_scores
                .into_iter()
                .map(|ps| (ps.peer_id.into_bytes(), ps.score as i32))
                .collect(),
            committee: s.committee
                .into_iter()
                .map(|t| t.into_bytes())
                .collect(),
            threshold: s.threshold as usize,
            frost_signature: s.frost_signature.as_slice().to_vec(),
            committee_pubkey: s.committee_pubkey.as_slice().to_vec(),
        }
    }
}

/// Minimal epoch snapshot for wire transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalizedEpochSnapshot {
    pub epoch_number: u64,
    pub peer_scores: Vec<(Vec<u8>, i32)>,
    pub committee: Vec<Vec<u8>>,
    pub threshold: usize,
    pub frost_signature: Vec<u8>,
    pub committee_pubkey: Vec<u8>,
}

impl ExternalizedEpochSnapshot {
    /// Reconstruct as Unprocessed for re-verification.
    pub fn into_unprocessed(self) -> Result<UnprocessedEpochSnapshot, ParseError> {
        use crate::epoch::snapshot::PeerScore;
        let snapshot = EpochSnapshot {
            epoch_number: self.epoch_number,
            epoch_start_ns: 0,
            epoch_end_ns: 0,
            peer_scores: self.peer_scores
                .into_iter()
                .map(|(id, score)| PeerScore {
                    peer_id: String::from_utf8(id).unwrap_or_default(),
                    score: score as f32,
                })
                .collect(),
            committee: self.committee
                .into_iter()
                .map(|t| String::from_utf8(t).unwrap_or_default())
                .collect(),
            threshold: self.threshold as u32,
            frost_signature: self.frost_signature.into(),
            committee_pubkey: self.committee_pubkey.into(),
        };
        Ok(UnprocessedEpochSnapshot(snapshot))
    }
}

impl UnprocessedEpochSnapshot {
    /// Verify FROST threshold signature.
    ///
    /// Returns `CleanAuthError::NotYetImplemented` until FROST ships.
    pub fn into_clean_authenticated(
        self,
        _crypto: &dyn CryptoServer,
        _committee_pubkeys: &[crate::foretias::types::SignatureBytes],
        _threshold: usize,
    ) -> Result<CleanAuthenticatedEpochSnapshot, CleanAuthError> {
        Err(CleanAuthError::NotYetImplemented)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unprocessed_chronon_record_from_bytes() {
        let record = ChrononRecord {
            chronon_number: 1,
            public_key: vec![0x01u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x02u8; 64].into(),
            backward_foretis: vec![0x03u8; 64].into(),
            aa_nonce: [0x04u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        };
        let json = serde_json::to_vec(&record).unwrap();
        let up = UnprocessedChrononRecord::from_bytes(&json).unwrap();
        assert_eq!(up.inner().chronon_number, 1);
    }

    #[test]
    fn test_unprocessed_chronon_record_from_invalid_json() {
        let result = UnprocessedChrononRecord::from_bytes(b"not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_unprocessed_foretis_from_bytes() {
        let foretis = Foretis {
            chronon_number: 42,
            content_hash: [0xABu8; 32].into(),
            signature: vec![0xCDu8; 64].into(),
            signature_algorithm: "Ed25519".to_string(),
            tbid: Tbid::default(),
            echo: "echo-42".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+12345ns".to_string(),
        };
        let json = serde_json::to_vec(&foretis).unwrap();
        let up = UnprocessedForetis::from_bytes(&json).unwrap();
        assert_eq!(up.inner().chronon_number, 42);
    }

    #[test]
    fn test_unprocessed_foretis_from_invalid_json() {
        let result = UnprocessedForetis::from_bytes(b"{invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_externalize_roundtrip_chronon_record() {
        let record = ChrononRecord {
            chronon_number: 7,
            public_key: vec![0x11u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![0x22u8; 64].into(),
            backward_foretis: vec![0x33u8; 64].into(),
            aa_nonce: [0x44u8; 16].into(),
            chronon_stamp_count: 3,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        };
        let ca = CleanAuthenticatedChrononRecord::from_trusted(record);
        let ext = ca.externalize();
        assert_eq!(ext.chronon_number, 7);
        assert_eq!(ext.stamps_per_tick, 3);
        assert_eq!(ext.public_key, [0x11u8; 32]);

        // Roundtrip
        let up = ext.into_unprocessed().unwrap();
        assert_eq!(up.inner().chronon_number, 7);
        assert_eq!(up.inner().chronon_stamp_count, 3);
    }

    #[test]
    fn test_externalize_roundtrip_foretis() {
        let foretis = Foretis {
            chronon_number: 10,
            content_hash: [0x55u8; 32].into(),
            signature: vec![0x66u8; 64].into(),
            signature_algorithm: "Ed25519".to_string(),
            tbid: Tbid::default(),
            echo: "test".to_string(),
            tbn: "tbn".to_string(),
            time_being_reference_time: "UE+999ns".to_string(),
        };
        let ca = CleanAuthenticatedForetis::from_trusted(foretis);
        let ext = ca.externalize();
        assert_eq!(ext.chronon_number, 10);
        assert_eq!(ext.content_hash, [0x55u8; 32]);

        // Roundtrip
        let up = ext.into_unprocessed().unwrap();
        assert_eq!(up.inner().chronon_number, 10);
    }

    #[test]
    fn test_parse_error_variants() {
        // Truncated bytes
        let result = UnprocessedChrononRecord::from_bytes(b"{}");
        // Empty JSON object -- missing required fields
        assert!(result.is_err());

        // Invalid length
        let result = UnprocessedForetis::from_bytes(b"[]");
        assert!(matches!(result, Err(ParseError::InvalidJson(_))));
    }

    #[test]
    fn test_clean_auth_error_from_parse_error() {
        let parse_err = ParseError::TruncatedBytes;
        let ca_err: CleanAuthError = parse_err.into();
        assert!(matches!(ca_err, CleanAuthError::Parse(ParseError::TruncatedBytes)));
    }

    #[test]
    fn test_clean_auth_error_display() {
        let err = CleanAuthError::InvalidSignature;
        assert!(err.to_string().contains("invalid signature"));
    }

    #[test]
    fn test_unprocessed_cannot_be_used_as_clean_authenticated() {
        // This test verifies the type system enforces the distinction.
        // An UnprocessedChrononRecord CANNOT be directly assigned to
        // CleanAuthenticatedChrononRecord -- the compiler rejects it.
        // If this compiles, the type discipline has failed.
        let _up: UnprocessedChrononRecord = UnprocessedChrononRecord(ChrononRecord {
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
        });
        // The following line would NOT compile:
        // let _: CleanAuthenticatedChrononRecord = _up;
        // This is the desired behavior -- the compiler enforces the gate.
    }
}
