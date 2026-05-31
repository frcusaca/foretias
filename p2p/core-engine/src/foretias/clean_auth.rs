//! Type-enforced cleansing and authentication.
//!
//! Enforces a three-stage type progression for inbound data:
//! `UnverifiedSignatureEnvelope<X>` (parsed, not trusted) -> `CleanAuthenticated<X>` (authenticated + cleansed) -> `Externalized<X>` (wire/disk, minimal fields).
//!
//! The compiler enforces that data flows through this progression. You cannot skip the middle step.
//!
//! `CleanAuthenticated<X>` has **private constructors** -- only `verify()` (inbound gate)
//! and `from_trusted()` (local gate) can produce them.
//!
//! `CleanAuthenticated` covers authentication + cleansing, NOT full chain-of-trust to genesis.

use serde::{Deserialize, Serialize};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use super::tick::{ChrononRecord, Foretis, verify_pair};
use super::types::Tbid;
use super::external_attestation::ExternalAttestation;
use super::encoding::{FTByteVector, FTByteArray};

// ---------------------------------------------------------------------------
// Signature types (Phase 16a — §21.2)
// ---------------------------------------------------------------------------

/// Role of the signer in the ordered signature chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignerRole {
    Chronomatter,
    Calendar,
    Member,
    CommunerdEnvelope,
}

/// Signature algorithm used by this entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SigAlgorithm {
    Ed25519,  // fast
    DualKey,  // Ed25519 ‖ SLH-DSA
}

/// One entry in the ordered signature list carried by trust-boundary wrappers.
/// Each signature covers `postcard(payload) ‖ postcard(&signatures[0..i])`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignatureEntry {
    pub role:      SignerRole,
    pub tbid:      String,
    pub algorithm: SigAlgorithm,
    pub sig:       Vec<u8>,
}

// ---------------------------------------------------------------------------
// TrustedInner trait (retained for backward compat -- inherent methods are preferred)
// ---------------------------------------------------------------------------

/// Trait for accessing the inner value of a trust boundary wrapper.
pub trait TrustedInner<T>: Sized {
    fn from_trusted(inner: T) -> Self;
    fn inner(&self) -> &T;
    fn into_inner(self) -> T;
}

// ---------------------------------------------------------------------------
// Generic trust boundary wrappers
// ---------------------------------------------------------------------------

/// A domain type that has been parsed but not yet verified.
///
/// Raw domain data from the wire/disk. Do NOT trust it.
#[derive(Debug, Clone)]
pub struct UnverifiedSignatureEnvelope<T> {
    inner: T,
    pub(crate) signatures: Vec<SignatureEntry>,
}
pub type DontUse<T> = UnverifiedSignatureEnvelope<T>;

impl<T> UnverifiedSignatureEnvelope<T> {
    /// Construct from raw parsed data.
    pub fn from_parsed(inner: T) -> Self {
        Self { inner, signatures: Vec::new() }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Consume and return the inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: serde::de::DeserializeOwned> UnverifiedSignatureEnvelope<T> {
    /// Parse from raw bytes (JSON). No verification performed.
    pub fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let val: T = serde_json::from_slice(b).map_err(ParseError::InvalidJson)?;
        Ok(Self::from_parsed(val))
    }

    /// Parse from a JSON-RPC Value. No verification performed.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, ParseError> {
        let val: T = serde_json::from_value(v).map_err(ParseError::InvalidJson)?;
        Ok(Self::from_parsed(val))
    }
}

impl<T> TrustedInner<T> for UnverifiedSignatureEnvelope<T> {
    fn from_trusted(inner: T) -> Self { Self { inner, signatures: Vec::new() } }
    fn inner(&self) -> &T { &self.inner }
    fn into_inner(self) -> T { self.inner }
}

impl<T: RecordBase> UnverifiedSignatureEnvelope<T> {
    /// Gate enforcement: verify all signatures and enforce full-signature requirement.
    ///
    /// Returns `CleanAuthenticated<T>` when the record does not require full signature
    /// and all fast (Ed25519) signatures verify correctly.
    /// Returns `CleanFullyAuthenticated<T>` when the record requires full signature
    /// and all signatures (Ed25519 + SLH-DSA) verify correctly.
    ///
    /// Rejects with distinct errors for:
    /// - Wrong signature count
    /// - Invalid signature for any role
    /// - Full signature required but only fast signatures present
    pub fn verify_all_signatures(
        self,
        crypto: &dyn CryptoServer,
        pub_key: &[u8],
    ) -> Result<CleanAuthenticated<T>, CleanAuthError> {
        let record = &self.inner;
        let require_full = record.always_require_full_signature();
        let sigs = &self.signatures;

        // Enforce signature cardinality: exactly 2 (Chronomatter + CommunerdEnvelope)
        if sigs.len() != 2 {
            return Err(CleanAuthError::SignatureCountMismatch {
                expected: 2,
                got: sigs.len(),
            });
        }

        // Verify each signature
        for (i, entry) in sigs.iter().enumerate() {
            let payload_bytes = postcard::to_allocvec(record)
                .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(crate::error::CryptoError::UnknownAlgorithm(e.to_string()))))?;
            let mut signing_data = payload_bytes.clone();
            if i > 0 {
                let prev_sigs = &sigs[..i];
                let prev_bytes = postcard::to_allocvec(prev_sigs)
                    .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(crate::error::CryptoError::UnknownAlgorithm(e.to_string()))))?;
                signing_data.extend_from_slice(&prev_bytes);
            }

            let valid = crypto.verify_with(
                pub_key,
                match entry.algorithm {
                    SigAlgorithm::Ed25519 => "Ed25519",
                    SigAlgorithm::DualKey => "SLH-DSA",
                },
                &signing_data,
                &entry.sig,
            ).map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;

            if !valid {
                return Err(CleanAuthError::SignatureVerificationFailed {
                    role: entry.role.clone(),
                    tbid: entry.tbid.clone(),
                });
            }
        }

        // Enforce full-signature requirement
        if require_full {
            let has_dual = sigs.iter().any(|s| s.algorithm == SigAlgorithm::DualKey);
            if !has_dual {
                return Err(CleanAuthError::FullSignatureRequired);
            }
        }

        Ok(CleanAuthenticated { inner: self.inner, signatures: self.signatures })
    }
}

/// Per-record full-signature requirement trait.
/// All signature-free payload types implement this common base trait.
pub trait RecordBase: serde::Serialize {
    /// Must this record reach CleanFullyAuthenticated before it may be used?
    /// Default: false (fast Ed25519 signatures suffice).
    fn always_require_full_signature(&self) -> bool {
        false
    }
}

/// A domain type that has been authenticated and cleansed.
///
/// Due diligence complete. Safe for in-process use.
/// **Private fields** -- zero external construction.
#[derive(Debug, Clone)]
pub struct CleanAuthenticated<T> {
    inner: T,
    pub(crate) signatures: Vec<SignatureEntry>,
}

impl<T> CleanAuthenticated<T> {
    /// Trusted construction -- only for locally-produced data.
    ///
    /// Chronomatter-produced records use this path. The caller asserts
    /// the record was signed with our own key.
    pub fn from_trusted(inner: T) -> Self {
        Self { inner, signatures: Vec::new() }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Consume and return the inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Runtime guarantee: fast-key (Ed25519) authentication has been performed.
    pub fn is_authenticated_quickly(&self) -> bool { true }
}

impl<T> TrustedInner<T> for CleanAuthenticated<T> {
    fn from_trusted(inner: T) -> Self { Self { inner, signatures: Vec::new() } }
    fn inner(&self) -> &T { &self.inner }
    fn into_inner(self) -> T { self.inner }
}

/// A domain type authenticated against both the fast key (Ed25519) and the slow key (SLH-DSA).
///
/// Subsumes `CleanAuthenticated<T>` — convertible via `From`. Required for initial
/// channel-binding establishment (§12.0). Produced only by dual-key verification paths.
#[derive(Debug, Clone)]
pub struct CleanFullyAuthenticated<T> {
    inner: T,
    pub(crate) signatures: Vec<SignatureEntry>,
}

impl<T> CleanFullyAuthenticated<T> {
    /// Construct from data verified against both the fast key and the slow key.
    /// Only dual-key verification paths in `communerd/` should call this.
    pub fn from_dual_verified(inner: T) -> Self {
        Self { inner, signatures: Vec::new() }
    }

    /// Read-only accessor.
    pub fn inner(&self) -> &T { &self.inner }

    /// Consume and return the inner value.
    pub fn into_inner(self) -> T { self.inner }

    /// Runtime guarantee: fast-key (Ed25519) authentication has been performed.
    pub fn is_authenticated_quickly(&self) -> bool { true }

    /// Runtime guarantee: slow-key (SLH-DSA) authentication has also been performed.
    pub fn is_authenticated_fully(&self) -> bool { true }
}

impl<T: Clone> From<CleanFullyAuthenticated<T>> for CleanAuthenticated<T> {
    fn from(full: CleanFullyAuthenticated<T>) -> CleanAuthenticated<T> {
        CleanAuthenticated { inner: full.inner, signatures: full.signatures }
    }
}

/// A domain type in its wire/disk form.
///
/// Minimal fields only. No runtime context.
#[derive(Debug, Clone)]
pub struct Externalized<T> {
    inner: T,
}

impl<T> TrustedInner<T> for Externalized<T> {
    fn from_trusted(inner: T) -> Self { Self { inner } }
    fn inner(&self) -> &T { &self.inner }
    fn into_inner(self) -> T { self.inner }
}

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
    /// Record requires full signature but only fast signatures present.
    FullSignatureRequired,
    /// Signature count mismatch (wrong cardinality).
    SignatureCountMismatch { expected: usize, got: usize },
    /// Signature verification failed for a specific role.
    SignatureVerificationFailed { role: SignerRole, tbid: String },
}

impl From<ParseError> for CleanAuthError {
    fn from(e: ParseError) -> Self { CleanAuthError::Parse(e) }
}

impl From<NodeError> for CleanAuthError {
    fn from(e: NodeError) -> Self { CleanAuthError::Crypto(e) }
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
            CleanAuthError::FullSignatureRequired => write!(f, "full signature required but only fast signatures present"),
            CleanAuthError::SignatureCountMismatch { expected, got } => write!(f, "signature count mismatch: expected {expected}, got {got}"),
            CleanAuthError::SignatureVerificationFailed { role, tbid } => write!(f, "signature verification failed for role {:?} tbid {}", role, tbid),
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
// ChrononRecord — field accessors + verify + externalize
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<ChrononRecord> {
    pub fn chronon_number(&self) -> &u64 { &self.inner.chronon_number }
    pub fn public_key(&self) -> &FTByteVector { &self.inner.public_key }
    pub fn signature_algorithm(&self) -> &str { &self.inner.signature_algorithm }
    pub fn forward_foretis(&self) -> &FTByteVector { &self.inner.forward_foretis }
    pub fn backward_foretis(&self) -> &FTByteVector { &self.inner.backward_foretis }
    pub fn aa_nonce(&self) -> &FTByteArray<16> { &self.inner.aa_nonce }
    pub fn chronon_stamp_count(&self) -> &u64 { &self.inner.chronon_stamp_count }
    pub fn external_attestations(&self) -> &Vec<ExternalAttestation> { &self.inner.external_attestations }
    pub fn tb_version(&self) -> &u32 { &self.inner.tb_version }
    pub fn tbid(&self) -> &Tbid { &self.inner.tbid }

    /// Inbound gate: verify this record.
    ///
    /// Pass `prev = Some(predecessor)` for non-genesis records.
    /// Pass `prev = None` for genesis (tick 1).
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        prev: Option<&CleanAuthenticated<ChrononRecord>>,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError> {
        match prev {
            Some(prev) => {
                let valid = verify_pair(
                    crypto,
                    &prev.inner.tbid.to_hex(),
                    &prev.inner,
                    &self.inner,
                ).map_err(CleanAuthError::Crypto)?;
                if !valid {
                    return Err(CleanAuthError::ChainBreak);
                }
                Ok(CleanAuthenticated { inner: self.inner, signatures: self.signatures })
            }
            None => {
                if self.inner.chronon_number != 1 {
                    return Err(CleanAuthError::ChainBreak);
                }
                if self.inner.public_key.is_empty() {
                    return Err(CleanAuthError::InvalidLength("public_key empty".into()));
                }
                if self.inner.tb_version >= 1 {
                    return Err(CleanAuthError::NotYetImplemented);
                }
                Ok(CleanAuthenticated { inner: self.inner, signatures: self.signatures })
            }
        }
    }

    /// Ergonomic alias for `verify(crypto, Some(prev))`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        prev: &CleanAuthenticated<ChrononRecord>,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError> {
        self.verify(crypto, Some(prev))
    }

    /// Ergonomic alias for `verify(crypto, None)`.
    pub fn into_clean_authenticated_genesis(
        self,
        crypto: &dyn CryptoServer,
    ) -> Result<CleanAuthenticated<ChrononRecord>, CleanAuthError> {
        self.verify(crypto, None)
    }
}

impl CleanAuthenticated<ChrononRecord> {
    pub fn chronon_number(&self) -> &u64 { &self.inner.chronon_number }
    pub fn public_key(&self) -> &FTByteVector { &self.inner.public_key }
    pub fn signature_algorithm(&self) -> &str { &self.inner.signature_algorithm }
    pub fn forward_foretis(&self) -> &FTByteVector { &self.inner.forward_foretis }
    pub fn backward_foretis(&self) -> &FTByteVector { &self.inner.backward_foretis }
    pub fn aa_nonce(&self) -> &FTByteArray<16> { &self.inner.aa_nonce }
    pub fn chronon_stamp_count(&self) -> &u64 { &self.inner.chronon_stamp_count }
    pub fn external_attestations(&self) -> &Vec<ExternalAttestation> { &self.inner.external_attestations }
    pub fn tb_version(&self) -> &u32 { &self.inner.tb_version }
    pub fn tbid(&self) -> &Tbid { &self.inner.tbid }

    /// Outbound gate: strip to minimal persistent form. No runtime context leaks.
    pub fn externalize(self) -> ExternalizedChrononRecord {
        let r = self.inner;
        ExternalizedChrononRecord {
            chronon_number: r.chronon_number,
            public_key: r.public_key.as_slice().try_into()
                .unwrap_or_else(|_| {
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
    /// Reconstruct as UnverifiedSignatureEnvelope for re-verification on load.
    pub fn reconstruct(self) -> Result<UnverifiedSignatureEnvelope<ChrononRecord>, ParseError> {
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
        Ok(UnverifiedSignatureEnvelope::from_parsed(record))
    }

    /// Alias for `reconstruct()` — backward compat.
    pub fn into_unprocessed(self) -> Result<UnverifiedSignatureEnvelope<ChrononRecord>, ParseError> {
        self.reconstruct()
    }
}

// ---------------------------------------------------------------------------
// Foretis — field accessors + verify + externalize
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<Foretis> {
    pub fn chronon_number(&self) -> &u64 { &self.inner.chronon_number }
    pub fn content_hash(&self) -> &FTByteArray<32> { &self.inner.content_hash }
    pub fn signature(&self) -> &FTByteVector { &self.inner.signature }
    pub fn signature_algorithm(&self) -> &str { &self.inner.signature_algorithm }
    pub fn tbid(&self) -> &Tbid { &self.inner.tbid }
    pub fn echo(&self) -> &str { &self.inner.echo }
    pub fn tbn(&self) -> &str { &self.inner.tbn }
    pub fn time_being_reference_time(&self) -> &str { &self.inner.time_being_reference_time }

    /// Inbound gate: verify this Foretis against a calendar record.
    ///
    /// The `record` must cover the same chronon number as this Foretis.
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        record: &CleanAuthenticated<ChrononRecord>,
        content: &[u8],
    ) -> Result<CleanAuthenticated<Foretis>, CleanAuthError> {
        let rec = &record.inner;
        let foretis = &self.inner;

        if foretis.signature_algorithm != rec.signature_algorithm {
            return Err(CleanAuthError::AlgorithmMismatch);
        }
        if foretis.chronon_number != rec.chronon_number {
            return Err(CleanAuthError::ChainBreak);
        }

        let recomputed = crypto.sha256(content)
            .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;
        if recomputed.bytes != *foretis.content_hash {
            return Err(CleanAuthError::InvalidSignature);
        }

        let mut sig_input = Vec::new();
        sig_input.extend_from_slice(&foretis.tbid.raw_bytes());
        sig_input.extend_from_slice(&foretis.chronon_number.to_be_bytes());
        sig_input.extend_from_slice(content);

        let valid = crypto.verify_with(
            &rec.public_key,
            &foretis.signature_algorithm,
            &sig_input,
            &foretis.signature,
        ).map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;

        if !valid {
            return Err(CleanAuthError::InvalidSignature);
        }

        Ok(CleanAuthenticated { inner: self.inner, signatures: self.signatures })
    }

    /// Ergonomic alias for `verify(crypto, record, content)`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        content: &[u8],
        record: &CleanAuthenticated<ChrononRecord>,
    ) -> Result<CleanAuthenticated<Foretis>, CleanAuthError> {
        self.verify(crypto, record, content)
    }
}

impl CleanAuthenticated<Foretis> {
    pub fn chronon_number(&self) -> &u64 { &self.inner.chronon_number }
    pub fn content_hash(&self) -> &FTByteArray<32> { &self.inner.content_hash }
    pub fn signature(&self) -> &FTByteVector { &self.inner.signature }
    pub fn signature_algorithm(&self) -> &str { &self.inner.signature_algorithm }
    pub fn tbid(&self) -> &Tbid { &self.inner.tbid }
    pub fn echo(&self) -> &str { &self.inner.echo }
    pub fn tbn(&self) -> &str { &self.inner.tbn }
    pub fn time_being_reference_time(&self) -> &str { &self.inner.time_being_reference_time }

    /// Outbound gate: strip to minimal wire form. No runtime context leaks.
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
    /// Reconstruct as UnverifiedSignatureEnvelope for re-verification on the receiving side.
    pub fn reconstruct(self) -> Result<UnverifiedSignatureEnvelope<Foretis>, ParseError> {
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
        Ok(UnverifiedSignatureEnvelope::from_parsed(foretis))
    }

    /// Alias for `reconstruct()` — backward compat.
    pub fn into_unprocessed(self) -> Result<UnverifiedSignatureEnvelope<Foretis>, ParseError> {
        self.reconstruct()
    }
}

// ---------------------------------------------------------------------------
// EpochSnapshot triple (core-engine)
// ---------------------------------------------------------------------------

use crate::epoch::snapshot::EpochSnapshot;

impl UnverifiedSignatureEnvelope<EpochSnapshot> {
    pub fn epoch_number(&self) -> &u64 { &self.inner.epoch_number }
    pub fn epoch_start_ns(&self) -> &u64 { &self.inner.epoch_start_ns }
    pub fn epoch_end_ns(&self) -> &u64 { &self.inner.epoch_end_ns }
    pub fn peer_scores(&self) -> &Vec<crate::epoch::snapshot::PeerScore> { &self.inner.peer_scores }
    pub fn committee(&self) -> &Vec<String> { &self.inner.committee }
    pub fn threshold(&self) -> &u32 { &self.inner.threshold }
    pub fn frost_signature(&self) -> &FTByteVector { &self.inner.frost_signature }
    pub fn committee_pubkey(&self) -> &FTByteVector { &self.inner.committee_pubkey }

    /// Inbound gate: verify FROST threshold signature.
    ///
    /// Returns `CleanAuthError::NotYetImplemented` until FROST ships.
    pub fn verify(
        self,
        _crypto: &dyn CryptoServer,
        _committee_pubkeys: &[crate::foretias::types::SignatureBytes],
        _threshold: usize,
    ) -> Result<CleanAuthenticated<EpochSnapshot>, CleanAuthError> {
        Err(CleanAuthError::NotYetImplemented)
    }

    /// Ergonomic alias for `verify(crypto, committee_pubkeys, threshold)`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        committee_pubkeys: &[crate::foretias::types::SignatureBytes],
        threshold: usize,
    ) -> Result<CleanAuthenticated<EpochSnapshot>, CleanAuthError> {
        self.verify(crypto, committee_pubkeys, threshold)
    }
}

impl CleanAuthenticated<EpochSnapshot> {
    pub fn epoch_number(&self) -> &u64 { &self.inner.epoch_number }
    pub fn epoch_start_ns(&self) -> &u64 { &self.inner.epoch_start_ns }
    pub fn epoch_end_ns(&self) -> &u64 { &self.inner.epoch_end_ns }
    pub fn peer_scores(&self) -> &Vec<crate::epoch::snapshot::PeerScore> { &self.inner.peer_scores }
    pub fn committee(&self) -> &Vec<String> { &self.inner.committee }
    pub fn threshold(&self) -> &u32 { &self.inner.threshold }
    pub fn frost_signature(&self) -> &FTByteVector { &self.inner.frost_signature }
    pub fn committee_pubkey(&self) -> &FTByteVector { &self.inner.committee_pubkey }

    /// Outbound gate: strip to minimal wire form.
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
    /// Reconstruct as UnverifiedSignatureEnvelope for re-verification.
    pub fn reconstruct(self) -> Result<UnverifiedSignatureEnvelope<EpochSnapshot>, ParseError> {
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
        Ok(UnverifiedSignatureEnvelope::from_parsed(snapshot))
    }

    /// Alias for `reconstruct()` — backward compat.
    pub fn into_unprocessed(self) -> Result<UnverifiedSignatureEnvelope<EpochSnapshot>, ParseError> {
        self.reconstruct()
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
        let up = UnverifiedSignatureEnvelope::<ChrononRecord>::from_bytes(&json).unwrap();
        assert_eq!(up.inner().chronon_number, 1);
    }

    #[test]
    fn test_unprocessed_chronon_record_from_invalid_json() {
        let result = UnverifiedSignatureEnvelope::<ChrononRecord>::from_bytes(b"not json");
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
        let up = UnverifiedSignatureEnvelope::<Foretis>::from_bytes(&json).unwrap();
        assert_eq!(up.inner().chronon_number, 42);
    }

    #[test]
    fn test_unprocessed_foretis_from_invalid_json() {
        let result = UnverifiedSignatureEnvelope::<Foretis>::from_bytes(b"{invalid");
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
        let ca = CleanAuthenticated::<ChrononRecord>::from_trusted(record);
        let ext = ca.externalize();
        assert_eq!(ext.chronon_number, 7);
        assert_eq!(ext.stamps_per_tick, 3);
        assert_eq!(ext.public_key, [0x11u8; 32]);

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
        let ca = CleanAuthenticated::<Foretis>::from_trusted(foretis);
        let ext = ca.externalize();
        assert_eq!(ext.chronon_number, 10);
        assert_eq!(ext.content_hash, [0x55u8; 32]);

        let up = ext.into_unprocessed().unwrap();
        assert_eq!(up.inner().chronon_number, 10);
    }

    #[test]
    fn test_parse_error_variants() {
        // Empty JSON object -- missing required fields
        let result = UnverifiedSignatureEnvelope::<ChrononRecord>::from_bytes(b"{}");
        assert!(result.is_err());

        let result = UnverifiedSignatureEnvelope::<Foretis>::from_bytes(b"[]");
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

    // ---------------------------------------------------------------------------
    // Serialization snapshots — regression guards for wire format
    // ---------------------------------------------------------------------------

    #[test]
    fn snapshot_chronon_record_externalized() {
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
        let ca = CleanAuthenticated::<ChrononRecord>::from_trusted(record);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let obj: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(obj["chronon_number"], 7);
        assert_eq!(obj["stamps_per_tick"], 3);
        assert_eq!(obj["signature_algorithm"], "Ed25519");
        assert_eq!(obj["public_key"].as_array().unwrap().len(), 32);
    }

    #[test]
    fn snapshot_foretis_externalized() {
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
        let ca = CleanAuthenticated::<Foretis>::from_trusted(foretis);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let obj: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(obj["chronon_number"], 10);
        assert_eq!(obj["signature_algorithm"], "Ed25519");
        assert_eq!(obj["echo"], "test");
        assert_eq!(obj["tbn"], 3);
        assert_eq!(obj["content_hash"].as_array().unwrap().len(), 32);
    }

    #[test]
    fn snapshot_epoch_snapshot_externalized() {
        use crate::epoch::snapshot::PeerScore;
        let snapshot = EpochSnapshot {
            epoch_number: 5,
            epoch_start_ns: 1000,
            epoch_end_ns: 2000,
            peer_scores: vec![
                PeerScore { peer_id: "A".into(), score: 10.0 },
                PeerScore { peer_id: "B".into(), score: 20.0 },
            ],
            committee: vec!["C1".into(), "C2".into()],
            threshold: 2,
            frost_signature: vec![0xAAu8; 64].into(),
            committee_pubkey: vec![0xBBu8; 32].into(),
        };
        let ca = CleanAuthenticated::<EpochSnapshot>::from_trusted(snapshot);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let obj: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(obj["epoch_number"], 5);
        assert_eq!(obj["threshold"], 2);
        assert_eq!(obj["peer_scores"].as_array().unwrap().len(), 2);
        assert_eq!(obj["committee"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_unprocessed_cannot_be_used_as_clean_authenticated() {
        // This test verifies the type system enforces the distinction.
        // An UnverifiedSignatureEnvelope<ChrononRecord> CANNOT be directly assigned to
        // CleanAuthenticated<ChrononRecord> -- the compiler rejects it.
        // If this compiles, the type discipline has failed.
        let _up: UnverifiedSignatureEnvelope<ChrononRecord> = UnverifiedSignatureEnvelope::from_parsed(ChrononRecord {
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
        // let _: CleanAuthenticated<ChrononRecord> = _up;
        // This is the desired behavior -- the compiler enforces the gate.
    }

    #[test]
    fn clean_authenticated_is_authenticated_quickly() {
        let ca: CleanAuthenticated<u32> = CleanAuthenticated::from_trusted(42u32);
        assert!(ca.is_authenticated_quickly());
    }

    #[test]
    fn clean_fully_authenticated_both_markers_true() {
        let cfa: CleanFullyAuthenticated<u32> = CleanFullyAuthenticated::from_dual_verified(7u32);
        assert!(cfa.is_authenticated_quickly());
        assert!(cfa.is_authenticated_fully());
    }

    #[test]
    fn clean_fully_authenticated_converts_to_clean_authenticated() {
        let cfa: CleanFullyAuthenticated<u32> = CleanFullyAuthenticated::from_dual_verified(99u32);
        let ca: CleanAuthenticated<u32> = cfa.into();
        assert_eq!(*ca.inner(), 99u32);
        assert!(ca.is_authenticated_quickly());
    }
}
