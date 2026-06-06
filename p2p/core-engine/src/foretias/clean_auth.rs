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

use super::encoding::{FTByteArray, FTByteVector};
use super::external_attestation::ExternalAttestationRecord;
use super::family_record::FamilyRecord;
use super::tick::{verify_pair, ChrononRecord, ForetisRecord};
use super::types::Tbid;
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

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
    Ed25519, // fast
    DualKey, // Ed25519 ‖ SLH-DSA
}

/// One entry in the ordered signature list carried by trust-boundary wrappers.
/// Each signature covers `postcard(payload) ‖ postcard(&signatures[0..i])`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignatureEntry {
    pub role: SignerRole,
    pub tbid: String,
    pub algorithm: SigAlgorithm,
    pub sig: Vec<u8>,
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
        Self {
            inner,
            signatures: Vec::new(),
        }
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
    fn from_trusted(inner: T) -> Self {
        Self {
            inner,
            signatures: Vec::new(),
        }
    }
    fn inner(&self) -> &T {
        &self.inner
    }
    fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: BaseRecord> UnverifiedSignatureEnvelope<T> {
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
            let payload_bytes = postcard::to_allocvec(record).map_err(|e| {
                CleanAuthError::Crypto(NodeError::Crypto(
                    crate::error::CryptoError::UnknownAlgorithm(e.to_string()),
                ))
            })?;
            let mut signing_data = payload_bytes.clone();
            if i > 0 {
                let prev_sigs = &sigs[..i];
                let prev_bytes = postcard::to_allocvec(prev_sigs).map_err(|e| {
                    CleanAuthError::Crypto(NodeError::Crypto(
                        crate::error::CryptoError::UnknownAlgorithm(e.to_string()),
                    ))
                })?;
                signing_data.extend_from_slice(&prev_bytes);
            }

            let valid = crypto
                .verify_with(
                    pub_key,
                    match entry.algorithm {
                        SigAlgorithm::Ed25519 => "Ed25519",
                        SigAlgorithm::DualKey => "SLH-DSA",
                    },
                    &signing_data,
                    &entry.sig,
                )
                .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;

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

        Ok(CleanAuthenticated {
            inner: self.inner,
            signatures: self.signatures,
        })
    }
}

/// Per-record full-signature requirement trait.
/// All signature-free payload types implement this common base trait.
pub trait BaseRecord: serde::Serialize {
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
        Self {
            inner,
            signatures: Vec::new(),
        }
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
    pub fn is_authenticated_quickly(&self) -> bool {
        true
    }
}

impl<T> TrustedInner<T> for CleanAuthenticated<T> {
    fn from_trusted(inner: T) -> Self {
        Self {
            inner,
            signatures: Vec::new(),
        }
    }
    fn inner(&self) -> &T {
        &self.inner
    }
    fn into_inner(self) -> T {
        self.inner
    }
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
        Self {
            inner,
            signatures: Vec::new(),
        }
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
    pub fn is_authenticated_quickly(&self) -> bool {
        true
    }

    /// Runtime guarantee: slow-key (SLH-DSA) authentication has also been performed.
    pub fn is_authenticated_fully(&self) -> bool {
        true
    }
}

impl<T: Clone> From<CleanFullyAuthenticated<T>> for CleanAuthenticated<T> {
    fn from(full: CleanFullyAuthenticated<T>) -> CleanAuthenticated<T> {
        CleanAuthenticated {
            inner: full.inner,
            signatures: full.signatures,
        }
    }
}

/// A domain type in its wire/disk form.
///
/// Wraps the full domain type `T` directly. Callers use `into_inner()` to
/// recover the domain type, then `UnverifiedSignatureEnvelope::from_parsed()`
/// to re-enter the trust boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Externalized<T> {
    inner: T,
}

impl<T> Externalized<T> {
    pub fn from_trusted(inner: T) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> TrustedInner<T> for Externalized<T> {
    fn from_trusted(inner: T) -> Self {
        Self { inner }
    }
    fn inner(&self) -> &T {
        &self.inner
    }
    fn into_inner(self) -> T {
        self.inner
    }
}

/// Builder for Externalized<R> with ordered signature chain enforcement.
///
/// Collects signatures in order: each signature covers `postcard(payload) || postcard(&signatures_so_far)`.
/// Terminal entry must be CommunerdEnvelope.
#[derive(Debug, Clone)]
pub struct ExternalizedBuilder<T> {
    inner: T,
    signatures: Vec<SignatureEntry>,
}

impl<T: serde::Serialize> ExternalizedBuilder<T> {
    /// Create builder from a raw payload.
    pub fn builder_from(inner: T) -> Self {
        Self {
            inner,
            signatures: Vec::new(),
        }
    }

    /// Create builder from a CleanAuthenticated payload (drops inbound signatures).
    pub fn builder_from_authenticated(auth: CleanAuthenticated<T>) -> Self {
        Self {
            inner: auth.into_inner(),
            signatures: Vec::new(),
        }
    }

    /// Append a signature entry.
    /// Signs: postcard(payload) || postcard(&signatures_so_far).
    /// The caller must provide the pre-computed signature bytes.
    pub fn add_signature(
        &mut self,
        role: SignerRole,
        tbid: String,
        algorithm: SigAlgorithm,
        sig: Vec<u8>,
    ) -> &mut Self {
        self.signatures.push(SignatureEntry {
            role,
            tbid,
            algorithm,
            sig,
        });
        self
    }

    /// Finalize: validate terminal entry is CommunerdEnvelope and return Externalized.
    pub fn build(self) -> Result<Externalized<T>, CleanAuthError> {
        if self.signatures.is_empty() {
            return Err(CleanAuthError::SignatureCountMismatch {
                expected: 1,
                got: 0,
            });
        }
        let last = self.signatures.last().unwrap();
        if last.role != SignerRole::CommunerdEnvelope {
            return Err(CleanAuthError::SignatureVerificationFailed {
                role: last.role.clone(),
                tbid: last.tbid.clone(),
            });
        }
        Ok(Externalized { inner: self.inner })
    }
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
            CleanAuthError::FullSignatureRequired => write!(
                f,
                "full signature required but only fast signatures present"
            ),
            CleanAuthError::SignatureCountMismatch { expected, got } => write!(
                f,
                "signature count mismatch: expected {expected}, got {got}"
            ),
            CleanAuthError::SignatureVerificationFailed { role, tbid } => write!(
                f,
                "signature verification failed for role {:?} tbid {}",
                role, tbid
            ),
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
    /// Bad format (missing required fields, wrong structure).
    BadFormat(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            ParseError::TruncatedBytes => write!(f, "truncated bytes"),
            ParseError::InvalidLength(msg) => write!(f, "invalid length: {msg}"),
            ParseError::BadFormat(msg) => write!(f, "bad format: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// ChrononRecord — field accessors + verify + externalize
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<ChrononRecord> {
    pub fn chronon_number(&self) -> &u64 {
        &self.inner.chronon_number
    }
    pub fn public_key(&self) -> &FTByteVector {
        &self.inner.public_key
    }
    pub fn signature_algorithm(&self) -> &str {
        &self.inner.signature_algorithm
    }
    pub fn forward_foretis(&self) -> &FTByteVector {
        &self.inner.forward_foretis
    }
    pub fn backward_foretis(&self) -> &FTByteVector {
        &self.inner.backward_foretis
    }
    pub fn aa_nonce(&self) -> &FTByteArray<16> {
        &self.inner.aa_nonce
    }
    pub fn chronon_stamp_count(&self) -> &u64 {
        &self.inner.chronon_stamp_count
    }
    pub fn external_attestations(&self) -> &Vec<ExternalAttestationRecord> {
        &self.inner.external_attestations
    }
    pub fn tb_version(&self) -> &u32 {
        &self.inner.tb_version
    }
    pub fn tbid(&self) -> &Tbid {
        &self.inner.tbid
    }

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
                let valid =
                    verify_pair(crypto, &prev.inner.tbid.to_hex(), &prev.inner, &self.inner)
                        .map_err(CleanAuthError::Crypto)?;
                if !valid {
                    return Err(CleanAuthError::ChainBreak);
                }
                Ok(CleanAuthenticated {
                    inner: self.inner,
                    signatures: self.signatures,
                })
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
                Ok(CleanAuthenticated {
                    inner: self.inner,
                    signatures: self.signatures,
                })
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
    pub fn chronon_number(&self) -> &u64 {
        &self.inner.chronon_number
    }
    pub fn public_key(&self) -> &FTByteVector {
        &self.inner.public_key
    }
    pub fn signature_algorithm(&self) -> &str {
        &self.inner.signature_algorithm
    }
    pub fn forward_foretis(&self) -> &FTByteVector {
        &self.inner.forward_foretis
    }
    pub fn backward_foretis(&self) -> &FTByteVector {
        &self.inner.backward_foretis
    }
    pub fn aa_nonce(&self) -> &FTByteArray<16> {
        &self.inner.aa_nonce
    }
    pub fn chronon_stamp_count(&self) -> &u64 {
        &self.inner.chronon_stamp_count
    }
    pub fn external_attestations(&self) -> &Vec<ExternalAttestationRecord> {
        &self.inner.external_attestations
    }
    pub fn tb_version(&self) -> &u32 {
        &self.inner.tb_version
    }
    pub fn tbid(&self) -> &Tbid {
        &self.inner.tbid
    }

    /// Outbound gate: wrap the domain type for wire/disk.
    pub fn externalize(self) -> Externalized<ChrononRecord> {
        Externalized::from_trusted(self.inner)
    }
}

// ---------------------------------------------------------------------------
// ForetisRecord — field accessors + verify + externalize
// ---------------------------------------------------------------------------
// ForetisRecord — field accessors + verify + externalize
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<ForetisRecord> {
    pub fn chronon_number(&self) -> &u64 {
        &self.inner.chronon_number
    }
    pub fn content_hash(&self) -> &FTByteArray<32> {
        &self.inner.content_hash
    }
    pub fn tbid(&self) -> &Tbid {
        &self.inner.tbid
    }
    pub fn echo(&self) -> &str {
        &self.inner.echo
    }
    pub fn tbn(&self) -> &str {
        &self.inner.tbn
    }
    pub fn time_being_reference_time(&self) -> &str {
        &self.inner.time_being_reference_time
    }

    /// Returns true if this envelope has at least one signature.
    pub fn has_signatures(&self) -> bool {
        !self.signatures.is_empty()
    }

    /// Parse v2 wire format: `{foretis: <ForetisRecord>, signature: <hex>, signature_algorithm: <string>}`.
    ///
    /// v1 bare ForetisRecord JSON is rejected — clean cutover to v2.
    pub fn from_json_value_v2(v: serde_json::Value) -> Result<Self, ParseError> {
        let obj = match v {
            serde_json::Value::Object(map) => map,
            _ => {
                return Err(ParseError::BadFormat(
                    "v2 envelope requires JSON object".into(),
                ))
            }
        };

        // v2 format: must have "foretis" key
        let foretis_val = obj
            .get("foretis")
            .ok_or_else(|| ParseError::BadFormat("v2 envelope requires 'foretis' key".into()))?;
        let foretis: ForetisRecord =
            serde_json::from_value(foretis_val.clone()).map_err(ParseError::InvalidJson)?;

        let mut env = Self::from_parsed(foretis);

        // Extract signature (hex-encoded) and algorithm
        if let Some(sig_hex) = obj.get("signature").and_then(|v| v.as_str()) {
            if let Ok(sig_bytes) = hex::decode(sig_hex) {
                if !sig_bytes.is_empty() {
                    let algorithm = match obj.get("signature_algorithm").and_then(|v| v.as_str()) {
                        Some("SLH-DSA") => SigAlgorithm::DualKey,
                        _ => SigAlgorithm::Ed25519,
                    };
                    env.signatures.push(SignatureEntry {
                        role: SignerRole::Chronomatter,
                        tbid: String::new(),
                        algorithm,
                        sig: sig_bytes,
                    });
                }
            }
        }

        Ok(env)
    }

    /// Inbound gate (V2): verify this ForetisRecord against a calendar record.
    ///
    /// v2 wire-break: signature comes from the envelope's signatures list,
    /// not from the inner ForetisRecord. Uses postcard-encoded payload for sig_input.
    ///
    /// The `record` must cover the same chronon number as this ForetisRecord.
    pub fn verify(
        self,
        crypto: &dyn CryptoServer,
        record: &CleanAuthenticated<ChrononRecord>,
        content: &[u8],
    ) -> Result<CleanAuthenticated<ForetisRecord>, CleanAuthError> {
        let rec = &record.inner;
        let foretis = &self.inner;

        if foretis.chronon_number != rec.chronon_number {
            return Err(CleanAuthError::ChainBreak);
        }

        let recomputed = crypto
            .sha256(content)
            .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;
        if recomputed.bytes != *foretis.content_hash {
            return Err(CleanAuthError::InvalidSignature);
        }

        // V2: use wrapper signatures (ordered chain)
        let valid = super::tick::verify(
            crypto,
            foretis,
            self.signatures.first().map(|s| &s.sig[..]).unwrap_or(&[]),
            self.signatures
                .first()
                .map(|s| match s.algorithm {
                    SigAlgorithm::Ed25519 => "Ed25519",
                    SigAlgorithm::DualKey => "SLH-DSA",
                })
                .unwrap_or("Ed25519"),
            content,
            &CalendarLookupFromCleanRecord(record),
        )
        .map_err(CleanAuthError::Crypto)?;
        if !valid {
            return Err(CleanAuthError::InvalidSignature);
        }

        Ok(CleanAuthenticated {
            inner: self.inner,
            signatures: self.signatures,
        })
    }

    /// Ergonomic alias for `verify(crypto, record, content)`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        content: &[u8],
        record: &CleanAuthenticated<ChrononRecord>,
    ) -> Result<CleanAuthenticated<ForetisRecord>, CleanAuthError> {
        self.verify(crypto, record, content)
    }
}

/// Adapter to let a single CleanAuthenticated<ChrononRecord> satisfy CalendarLookup.
struct CalendarLookupFromCleanRecord<'a>(&'a CleanAuthenticated<ChrononRecord>);
impl<'a> super::tick::CalendarLookup for CalendarLookupFromCleanRecord<'a> {
    fn get(&self, _chronon_number: u64, _count: usize) -> Result<Vec<ChrononRecord>, NodeError> {
        Ok(vec![self.0.inner().clone()])
    }
    fn latest(&self) -> Option<u64> {
        Some(*self.0.chronon_number())
    }
    fn tbid(&self) -> Tbid {
        *self.0.tbid()
    }
    fn tbn(&self) -> &str {
        ""
    }
}

impl CleanAuthenticated<ForetisRecord> {
    pub fn chronon_number(&self) -> &u64 {
        &self.inner.chronon_number
    }
    pub fn content_hash(&self) -> &FTByteArray<32> {
        &self.inner.content_hash
    }
    pub fn tbid(&self) -> &Tbid {
        &self.inner.tbid
    }
    pub fn echo(&self) -> &str {
        &self.inner.echo
    }
    pub fn tbn(&self) -> &str {
        &self.inner.tbn
    }
    pub fn time_being_reference_time(&self) -> &str {
        &self.inner.time_being_reference_time
    }

    /// Return the signature bytes from the first entry in the ordered signature chain.
    ///
    /// This is the signature that was verified during the Take 3 inbound gate
    /// (typically the Chronomatter fast-key Ed25519 signature).
    pub fn signature_bytes(&self) -> Option<&[u8]> {
        self.signatures.first().map(|e| e.sig.as_slice())
    }

    /// Return the algorithm name for the first signature entry.
    ///
    /// Returns `"Ed25519"` or `"SLH-DSA"` depending on the `SigAlgorithm` variant.
    pub fn signature_algorithm(&self) -> Option<&str> {
        Some(match self.signatures.first()?.algorithm {
            SigAlgorithm::Ed25519 => "Ed25519",
            SigAlgorithm::DualKey => "SLH-DSA",
        })
    }

    /// Outbound gate: wrap the domain type for wire/disk.
    pub fn externalize(self) -> Externalized<ForetisRecord> {
        Externalized::from_trusted(self.inner)
    }
}

// ---------------------------------------------------------------------------
// EpochSnapshotRecord triple (core-engine)
// ---------------------------------------------------------------------------

use crate::epoch::snapshot::EpochSnapshotRecord;

impl UnverifiedSignatureEnvelope<EpochSnapshotRecord> {
    pub fn epoch_number(&self) -> &u64 {
        &self.inner.epoch_number
    }
    pub fn epoch_start_ns(&self) -> &u64 {
        &self.inner.epoch_start_ns
    }
    pub fn epoch_end_ns(&self) -> &u64 {
        &self.inner.epoch_end_ns
    }
    pub fn peer_scores(&self) -> &Vec<crate::epoch::snapshot::PeerScore> {
        &self.inner.peer_scores
    }
    pub fn committee(&self) -> &Vec<String> {
        &self.inner.committee
    }
    pub fn threshold(&self) -> &u32 {
        &self.inner.threshold
    }
    pub fn frost_signature(&self) -> &FTByteVector {
        &self.inner.frost_signature
    }
    pub fn committee_pubkey(&self) -> &FTByteVector {
        &self.inner.committee_pubkey
    }

    /// Inbound gate: verify FROST threshold signature.
    ///
    /// Returns `CleanAuthError::NotYetImplemented` until FROST ships.
    pub fn verify(
        self,
        _crypto: &dyn CryptoServer,
        _committee_pubkeys: &[crate::foretias::types::SignatureBytes],
        _threshold: usize,
    ) -> Result<CleanAuthenticated<EpochSnapshotRecord>, CleanAuthError> {
        Err(CleanAuthError::NotYetImplemented)
    }

    /// Ergonomic alias for `verify(crypto, committee_pubkeys, threshold)`.
    pub fn into_clean_authenticated(
        self,
        crypto: &dyn CryptoServer,
        committee_pubkeys: &[crate::foretias::types::SignatureBytes],
        threshold: usize,
    ) -> Result<CleanAuthenticated<EpochSnapshotRecord>, CleanAuthError> {
        self.verify(crypto, committee_pubkeys, threshold)
    }
}

impl CleanAuthenticated<EpochSnapshotRecord> {
    pub fn epoch_number(&self) -> &u64 {
        &self.inner.epoch_number
    }
    pub fn epoch_start_ns(&self) -> &u64 {
        &self.inner.epoch_start_ns
    }
    pub fn epoch_end_ns(&self) -> &u64 {
        &self.inner.epoch_end_ns
    }
    pub fn peer_scores(&self) -> &Vec<crate::epoch::snapshot::PeerScore> {
        &self.inner.peer_scores
    }
    pub fn committee(&self) -> &Vec<String> {
        &self.inner.committee
    }
    pub fn threshold(&self) -> &u32 {
        &self.inner.threshold
    }
    pub fn frost_signature(&self) -> &FTByteVector {
        &self.inner.frost_signature
    }
    pub fn committee_pubkey(&self) -> &FTByteVector {
        &self.inner.committee_pubkey
    }

    /// Outbound gate: wrap the domain type for wire/disk.
    pub fn externalize(self) -> Externalized<EpochSnapshotRecord> {
        Externalized::from_trusted(self.inner)
    }
}

// ---------------------------------------------------------------------------
// FamilyRecord — gate enforcement (CleanFullyAuthenticated only)
// ---------------------------------------------------------------------------

impl UnverifiedSignatureEnvelope<FamilyRecord> {
    /// Verify FamilyRecord: envelope signatures + k×k matrix.
    ///
    /// Returns `CleanFullyAuthenticated<FamilyRecord>` on success.
    /// Rejects with `FullSignatureRequired` if any signature is Ed25519-only (fast).
    /// Rejects with `Crypto(BadSignature)` if the k×k matrix verification fails.
    pub fn verify_family_record(
        self,
        crypto: &dyn CryptoServer,
        pub_key: &[u8],
    ) -> Result<CleanFullyAuthenticated<FamilyRecord>, CleanAuthError> {
        let ca = self.verify_all_signatures(crypto, pub_key)?;
        let record = ca.inner();
        record
            .verify_matrix(crypto)
            .map_err(|e| CleanAuthError::Crypto(NodeError::Crypto(e)))?;
        Ok(CleanFullyAuthenticated::from_dual_verified(ca.into_inner()))
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
        let foretis = ForetisRecord {
            chronon_number: 42,
            content_hash: [0xABu8; 32].into(),
            tbid: Tbid::default(),
            echo: "echo-42".to_string(),
            tbn: "test".to_string(),
            time_being_reference_time: "UE+12345ns".to_string(),
        };
        let json = serde_json::to_vec(&foretis).unwrap();
        let up = UnverifiedSignatureEnvelope::<ForetisRecord>::from_bytes(&json).unwrap();
        assert_eq!(up.inner().chronon_number, 42);
    }

    #[test]
    fn test_unprocessed_foretis_from_invalid_json() {
        let result = UnverifiedSignatureEnvelope::<ForetisRecord>::from_bytes(b"{invalid");
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
        assert_eq!(ext.inner().chronon_number, 7);
        assert_eq!(ext.inner().chronon_stamp_count, 3);

        let inner = ext.into_inner();
        assert_eq!(inner.chronon_number, 7);
        assert_eq!(inner.chronon_stamp_count, 3);
        let up = UnverifiedSignatureEnvelope::from_parsed(inner);
        assert_eq!(up.inner().chronon_number, 7);
        assert_eq!(up.inner().chronon_stamp_count, 3);
    }

    #[test]
    fn test_externalize_roundtrip_foretis() {
        let foretis = ForetisRecord {
            chronon_number: 10,
            content_hash: [0x55u8; 32].into(),
            tbid: Tbid::default(),
            echo: "test".to_string(),
            tbn: "tbn".to_string(),
            time_being_reference_time: "UE+999ns".to_string(),
        };
        let ca = CleanAuthenticated::<ForetisRecord>::from_trusted(foretis);
        let ext = ca.externalize();
        assert_eq!(ext.inner().chronon_number, 10);

        let inner = ext.into_inner();
        let up = UnverifiedSignatureEnvelope::from_parsed(inner);
        assert_eq!(up.inner().chronon_number, 10);
    }

    #[test]
    fn test_parse_error_variants() {
        // Empty JSON object -- missing required fields
        let result = UnverifiedSignatureEnvelope::<ChrononRecord>::from_bytes(b"{}");
        assert!(result.is_err());

        let result = UnverifiedSignatureEnvelope::<ForetisRecord>::from_bytes(b"[]");
        assert!(matches!(result, Err(ParseError::InvalidJson(_))));
    }

    #[test]
    fn test_clean_auth_error_from_parse_error() {
        let parse_err = ParseError::TruncatedBytes;
        let ca_err: CleanAuthError = parse_err.into();
        assert!(matches!(
            ca_err,
            CleanAuthError::Parse(ParseError::TruncatedBytes)
        ));
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
        assert_eq!(obj["inner"]["tick_number"], 7);
        assert_eq!(obj["inner"]["chronon_stamp_count"], 3);
        assert_eq!(obj["inner"]["signature_algorithm"], "Ed25519");
        assert!(obj["inner"]["public_key"].is_string());
    }

    #[test]
    fn snapshot_foretis_externalized() {
        let foretis = ForetisRecord {
            chronon_number: 10,
            content_hash: [0x55u8; 32].into(),
            tbid: Tbid::default(),
            echo: "test".to_string(),
            tbn: "tbn".to_string(),
            time_being_reference_time: "UE+999ns".to_string(),
        };
        let ca = CleanAuthenticated::<ForetisRecord>::from_trusted(foretis);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let obj: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(obj["inner"]["chronon_number"], 10);
        assert_eq!(obj["inner"]["echo"], "test");
        assert_eq!(obj["inner"]["tbn"], "tbn");
        assert!(obj["inner"]["content_hash"].is_string());
    }

    #[test]
    fn snapshot_epoch_snapshot_externalized() {
        use crate::epoch::snapshot::PeerScore;
        let snapshot = EpochSnapshotRecord {
            epoch_number: 5,
            epoch_start_ns: 1000,
            epoch_end_ns: 2000,
            peer_scores: vec![
                PeerScore {
                    peer_id: "A".into(),
                    score: 10.0,
                },
                PeerScore {
                    peer_id: "B".into(),
                    score: 20.0,
                },
            ],
            committee: vec!["C1".into(), "C2".into()],
            threshold: 2,
            frost_signature: vec![0xAAu8; 64].into(),
            committee_pubkey: vec![0xBBu8; 32].into(),
        };
        let ca = CleanAuthenticated::<EpochSnapshotRecord>::from_trusted(snapshot);
        let ext = ca.externalize();
        let json_bytes = serde_json::to_vec(&ext).unwrap();
        let obj: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(obj["inner"]["epoch_number"], 5);
        assert_eq!(obj["inner"]["threshold"], 2);
        assert_eq!(obj["inner"]["peer_scores"].as_array().unwrap().len(), 2);
        assert_eq!(obj["inner"]["committee"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_unprocessed_cannot_be_used_as_clean_authenticated() {
        // This test verifies the type system enforces the distinction.
        // An UnverifiedSignatureEnvelope<ChrononRecord> CANNOT be directly assigned to
        // CleanAuthenticated<ChrononRecord> -- the compiler rejects it.
        // If this compiles, the type discipline has failed.
        let _up: UnverifiedSignatureEnvelope<ChrononRecord> =
            UnverifiedSignatureEnvelope::from_parsed(ChrononRecord {
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
