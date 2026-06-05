//! Experiment: bon builders + serde(try_from) for permissive parsing → strict validation.
//!
//! Tests three ideas:
//! 1. Shadow type pattern: all wire fields optional (permissive), validate on conversion
//! 2. bon builders for local construction (replaces multi-param constructors)
//! 3. Recursive nesting: ExternalAttestation contains ChrononRecord + Foretis

use bon::Builder;
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize};

// ===========================================================================
// DOMAIN TYPES — strict, validated, what the rest of the codebase uses
// ===========================================================================

/// Strict ChrononRecord — non-Option fields are required by default in bon.
#[derive(Debug, Clone, Serialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ChrononRecord {
    #[serde(rename = "tick_number")]
    pub chronon_number: u64,
    pub public_key: Vec<u8>,
    #[builder(default = "Ed25519".to_string())]
    pub signature_algorithm: String,
    pub forward_foretis: Vec<u8>,
    pub backward_foretis: Vec<u8>,
    pub aa_nonce: [u8; 16],
    #[builder(default)]
    pub chronon_stamp_count: u64,
    #[builder(default)]
    pub external_attestations: Vec<ExternalAttestation>,
    #[builder(default = 1)]
    pub tb_version: u32,
    #[builder(default)]
    pub tbid: Vec<u8>,
}

impl<S: chronon_record_builder::IsComplete> ChrononRecordBuilder<S> {
    /// Fallible build — validates invariants.
    pub fn build(self) -> Result<ChrononRecord, ValidationError> {
        let record = self.build_internal();
        if record.chronon_number == 0 {
            return Err(ValidationError("chronon_number must be > 0".into()));
        }
        if record.public_key.is_empty() {
            return Err(ValidationError("public_key must not be empty".into()));
        }
        Ok(record)
    }
}

/// Strict Foretis — non-Option fields are required by default in bon.
#[derive(Debug, Clone, Serialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct Foretis {
    pub chronon_number: u64,
    pub content_hash: [u8; 32],
    pub tbid: Vec<u8>,
    pub echo: String,
    pub tbn: String,
    pub time_being_reference_time: String,
}

impl<S: foretis_builder::IsComplete> ForetisBuilder<S> {
    pub fn build(self) -> Result<Foretis, ValidationError> {
        let foretis = self.build_internal();
        if foretis.chronon_number == 0 {
            return Err(ValidationError("chronon_number must be > 0".into()));
        }
        Ok(foretis)
    }
}

/// Strict ExternalAttestation — non-Option fields required, Option fields optional in bon.
#[derive(Debug, Clone, Serialize, Builder)]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ExternalAttestation {
    pub attester_tbid: String,
    pub foretis: Foretis,
    #[builder(default)]
    pub signature: Vec<u8>,
    #[builder(default = "Ed25519".to_string())]
    pub signature_algorithm: String,
    pub attester_tick_record: ChrononRecord,
    pub received_at_ns: u64,
}

impl<S: external_attestation_builder::IsComplete> ExternalAttestationBuilder<S> {
    pub fn build(self) -> Result<ExternalAttestation, ValidationError> {
        let att = self.build_internal();
        if att.attester_tbid.is_empty() {
            return Err(ValidationError("attester_tbid must not be empty".into()));
        }
        Ok(att)
    }
}

// ===========================================================================
// SHADOW TYPES — all fields optional, permissive wire parsing
// ===========================================================================

/// Wire format for ChrononRecord — all fields optional for permissive parsing.
#[derive(Debug, Clone, Deserialize)]
struct ChrononRecordUnchecked {
    #[serde(rename = "tick_number")]
    chronon_number: Option<u64>,
    public_key: Option<Vec<u8>>,
    #[serde(default)]
    signature_algorithm: Option<String>,
    forward_foretis: Option<Vec<u8>>,
    backward_foretis: Option<Vec<u8>>,
    aa_nonce: Option<[u8; 16]>,
    #[serde(default)]
    chronon_stamp_count: Option<u64>,
    #[serde(default)]
    external_attestations: Option<Vec<ExternalAttestationUnchecked>>,
    #[serde(default)]
    tb_version: Option<u32>,
    #[serde(default)]
    tbid: Option<Vec<u8>>,
}

/// Wire format for Foretis — all fields optional.
#[derive(Debug, Clone, Deserialize)]
struct ForetisUnchecked {
    chronon_number: Option<u64>,
    content_hash: Option<[u8; 32]>,
    tbid: Option<Vec<u8>>,
    echo: Option<String>,
    tbn: Option<String>,
    time_being_reference_time: Option<String>,
}

/// Wire format for ExternalAttestation — all fields optional.
#[derive(Debug, Clone, Deserialize)]
struct ExternalAttestationUnchecked {
    attester_tbid: Option<String>,
    foretis: Option<ForetisUnchecked>,
    #[serde(default)]
    signature: Option<Vec<u8>>,
    #[serde(default)]
    signature_algorithm: Option<String>,
    attester_tick_record: Option<ChrononRecordUnchecked>,
    received_at_ns: Option<u64>,
}

// ===========================================================================
// VALIDATION ERRORS
// ===========================================================================

#[derive(Debug, Clone)]
pub struct ValidationError(pub String);

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Validation error: {}", self.0)
    }
}

impl std::error::Error for ValidationError {}

impl serde::de::Error for ValidationError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        ValidationError(msg.to_string())
    }
}

// ===========================================================================
// TRYFROM IMPLS — the validation gates
// ===========================================================================

fn missing(field: &str) -> ValidationError {
    ValidationError(format!("missing required field: {field}"))
}

impl TryFrom<ChrononRecordUnchecked> for ChrononRecord {
    type Error = ValidationError;

    fn try_from(raw: ChrononRecordUnchecked) -> Result<Self, Self::Error> {
        let chronon_number = raw.chronon_number.ok_or(missing("chronon_number"))?;
        if chronon_number == 0 {
            return Err(ValidationError("chronon_number must be > 0".into()));
        }
        let public_key = raw.public_key.ok_or(missing("public_key"))?;
        if public_key.is_empty() {
            return Err(ValidationError("public_key must not be empty".into()));
        }
        Ok(Self {
            chronon_number,
            public_key,
            signature_algorithm: raw
                .signature_algorithm
                .unwrap_or_else(|| "Ed25519".to_string()),
            forward_foretis: raw.forward_foretis.ok_or(missing("forward_foretis"))?,
            backward_foretis: raw.backward_foretis.ok_or(missing("backward_foretis"))?,
            aa_nonce: raw.aa_nonce.ok_or(missing("aa_nonce"))?,
            chronon_stamp_count: raw.chronon_stamp_count.unwrap_or(0),
            external_attestations: raw
                .external_attestations
                .map(|v| v.into_iter().map(ExternalAttestation::try_from).collect())
                .transpose()?
                .unwrap_or_default(),
            tb_version: raw.tb_version.unwrap_or(0),
            tbid: raw.tbid.unwrap_or_default(),
        })
    }
}

impl TryFrom<ForetisUnchecked> for Foretis {
    type Error = ValidationError;

    fn try_from(raw: ForetisUnchecked) -> Result<Self, Self::Error> {
        let chronon_number = raw.chronon_number.ok_or(missing("chronon_number"))?;
        if chronon_number == 0 {
            return Err(ValidationError("chronon_number must be > 0".into()));
        }
        Ok(Self {
            chronon_number,
            content_hash: raw.content_hash.ok_or(missing("content_hash"))?,
            tbid: raw.tbid.ok_or(missing("tbid"))?,
            echo: raw.echo.ok_or(missing("echo"))?,
            tbn: raw.tbn.ok_or(missing("tbn"))?,
            time_being_reference_time: raw
                .time_being_reference_time
                .ok_or(missing("time_being_reference_time"))?,
        })
    }
}

impl TryFrom<ExternalAttestationUnchecked> for ExternalAttestation {
    type Error = ValidationError;

    fn try_from(raw: ExternalAttestationUnchecked) -> Result<Self, Self::Error> {
        let attester_tbid = raw.attester_tbid.ok_or(missing("attester_tbid"))?;
        if attester_tbid.is_empty() {
            return Err(ValidationError("attester_tbid must not be empty".into()));
        }
        Ok(Self {
            attester_tbid,
            foretis: Foretis::try_from(raw.foretis.ok_or(missing("foretis"))?)?,
            signature: raw.signature.unwrap_or_default(),
            signature_algorithm: raw
                .signature_algorithm
                .unwrap_or_else(|| "Ed25519".to_string()),
            attester_tick_record: ChrononRecord::try_from(
                raw.attester_tick_record
                    .ok_or(missing("attester_tick_record"))?,
            )?,
            received_at_ns: raw.received_at_ns.ok_or(missing("received_at_ns"))?,
        })
    }
}

// ===========================================================================
// SERDE INTEGRATION — serde(try_from) on domain types
// ===========================================================================

// ChrononRecord uses serde(try_from) so JSON deserialization goes through validation.
// The Serialize derive stays on the domain type for outbound wire format.
impl<'de> Deserialize<'de> for ChrononRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let unchecked = ChrononRecordUnchecked::deserialize(deserializer)?;
        ChrononRecord::try_from(unchecked).map_err(D::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Foretis {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let unchecked = ForetisUnchecked::deserialize(deserializer)?;
        Foretis::try_from(unchecked).map_err(D::Error::custom)
    }
}

impl<'de> Deserialize<'de> for ExternalAttestation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let unchecked = ExternalAttestationUnchecked::deserialize(deserializer)?;
        ExternalAttestation::try_from(unchecked).map_err(D::Error::custom)
    }
}

// ===========================================================================
// WRAPPER TYPE — simulates Externalized<T>
// ===========================================================================

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

// ===========================================================================
// TESTS
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Test 1: Permissive parsing — missing fields get defaults ---
    #[test]
    fn permissive_parsing_missing_optional_fields() {
        let json = r#"{
            "tick_number": 42,
            "public_key": [1, 2, 3],
            "forward_foretis": [4, 5, 6],
            "backward_foretis": [7, 8, 9],
            "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
        }"#;

        let record: ChrononRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.chronon_number, 42);
        assert_eq!(record.signature_algorithm, "Ed25519");
        assert_eq!(record.chronon_stamp_count, 0);
        assert!(record.external_attestations.is_empty());
        assert_eq!(record.tb_version, 0);
        assert!(record.tbid.is_empty());
    }

    // --- Test 2: Missing required field → clear error ---
    #[test]
    fn missing_required_field_rejected() {
        let json = r#"{
            "tick_number": 42,
            "forward_foretis": [4, 5, 6],
            "backward_foretis": [7, 8, 9],
            "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
        }"#;

        let result = serde_json::from_str::<ChrononRecord>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("public_key"), "error: {err}");
    }

    // --- Test 3: chronon_number = 0 → validation error ---
    #[test]
    fn zero_chronon_number_rejected() {
        let json = r#"{
            "tick_number": 0,
            "public_key": [1, 2, 3],
            "forward_foretis": [4, 5, 6],
            "backward_foretis": [7, 8, 9],
            "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
        }"#;

        let result = serde_json::from_str::<ChrononRecord>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("chronon_number must be > 0"), "error: {err}");
    }

    // --- Test 4: bon builder for local construction ---
    #[test]
    fn bon_builder_local_construction() {
        let record = ChrononRecord::builder()
            .chronon_number(1)
            .public_key(vec![1, 2, 3])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build()
            .unwrap();

        assert_eq!(record.chronon_number, 1);
        assert_eq!(record.signature_algorithm, "Ed25519"); // default
        assert_eq!(record.chronon_stamp_count, 0); // default
    }

    // --- Test 5: bon builder with all defaults ---
    #[test]
    fn bon_builder_defaults_applied() {
        let record = ChrononRecord::builder()
            .chronon_number(10)
            .public_key(vec![1, 2, 3])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build()
            .unwrap();

        assert_eq!(record.signature_algorithm, "Ed25519");
        assert_eq!(record.chronon_stamp_count, 0);
        assert_eq!(record.tb_version, 1); // builder default
        assert!(record.external_attestations.is_empty());
    }

    // --- Test 6: bon builder validation — zero chronon_number ---
    #[test]
    fn bon_builder_validation_zero_chronon() {
        let result = ChrononRecord::builder()
            .chronon_number(0)
            .public_key(vec![1, 2, 3])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().0.contains("chronon_number must be > 0"));
    }

    // --- Test 7: bon builder validation — empty public_key ---
    #[test]
    fn bon_builder_validation_empty_pubkey() {
        let result = ChrononRecord::builder()
            .chronon_number(1)
            .public_key(vec![])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .0
            .contains("public_key must not be empty"));
    }

    // --- Test 8: Foretis serde round-trip ---
    #[test]
    fn foretis_serde_roundtrip() {
        let foretis = Foretis::builder()
            .chronon_number(42)
            .content_hash([1u8; 32])
            .tbid(vec![3u8; 96])
            .echo("chronon-42".into())
            .tbn("test-tb".into())
            .time_being_reference_time("UE+123ns".into())
            .build()
            .unwrap();

        let json = serde_json::to_string(&foretis).unwrap();
        let deserialized: Foretis = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.chronon_number, 42);
        assert_eq!(deserialized.echo, "chronon-42");
    }

    // --- Test 9: ChrononRecord serde round-trip ---
    #[test]
    fn chronon_record_serde_roundtrip() {
        let record = ChrononRecord::builder()
            .chronon_number(5)
            .public_key(vec![0xAA; 32])
            .forward_foretis(vec![0xBB; 64])
            .backward_foretis(vec![0xCC; 64])
            .aa_nonce([0xDD; 16])
            .chronon_stamp_count(3)
            .build()
            .unwrap();

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ChrononRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.chronon_number, 5);
        assert_eq!(deserialized.chronon_stamp_count, 3);
    }

    // --- Test 10: Externalized wrapper round-trip ---
    #[test]
    fn externalized_roundtrip() {
        let record = ChrononRecord::builder()
            .chronon_number(7)
            .public_key(vec![1, 2, 3])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build()
            .unwrap();

        let ext = Externalized::from_trusted(record);
        let json = serde_json::to_string(&ext).unwrap();
        let deserialized: Externalized<ChrononRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.inner().chronon_number, 7);
    }

    // --- Test 11: Nested ExternalAttestation with missing optional fields ---
    #[test]
    fn external_attestation_permissive_parsing() {
        let json = r#"{
            "attester_tbid": "aabbccdd",
            "foretis": {
                "chronon_number": 1,
                "content_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                "tbid": [1, 2, 3],
                "echo": "chronon-1",
                "tbn": "test",
                "time_being_reference_time": "UE+0ns"
            },
            "attester_tick_record": {
                "tick_number": 1,
                "public_key": [1, 2, 3],
                "forward_foretis": [4, 5, 6],
                "backward_foretis": [7, 8, 9],
                "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            },
            "received_at_ns": 12345
        }"#;

        let att: ExternalAttestation = serde_json::from_str(json).unwrap();
        assert_eq!(att.attester_tbid, "aabbccdd");
        assert_eq!(att.foretis.chronon_number, 1);
        assert_eq!(att.attester_tick_record.chronon_number, 1);
        assert_eq!(att.signature_algorithm, "Ed25519"); // default
        assert!(att.signature.is_empty()); // default
    }

    // --- Test 12: Nested ExternalAttestation missing nested required field ---
    #[test]
    fn external_attestation_missing_nested_required() {
        let json = r#"{
            "attester_tbid": "aabbccdd",
            "foretis": {
                "chronon_number": 1,
                "content_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                "tbid": [1, 2, 3],
                "echo": "chronon-1",
                "tbn": "test",
                "time_being_reference_time": "UE+0ns"
            },
            "attester_tick_record": {
                "tick_number": 1,
                "forward_foretis": [4, 5, 6],
                "backward_foretis": [7, 8, 9],
                "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
            },
            "received_at_ns": 12345
        }"#;

        let result = serde_json::from_str::<ExternalAttestation>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("public_key"), "error: {err}");
    }

    // --- Test 13: ChrononRecord with external attestations ---
    #[test]
    fn chronon_record_with_nested_external_attestations() {
        let json = r#"{
            "tick_number": 5,
            "public_key": [1, 2, 3],
            "forward_foretis": [4, 5, 6],
            "backward_foretis": [7, 8, 9],
            "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
            "chronon_stamp_count": 2,
            "external_attestations": [
                {
                    "attester_tbid": "eeff0011",
                    "foretis": {
                        "chronon_number": 4,
                        "content_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                        "tbid": [1, 2, 3],
                        "echo": "chronon-4",
                        "tbn": "remote-tb",
                        "time_being_reference_time": "UE+100ns"
                    },
                    "attester_tick_record": {
                        "tick_number": 4,
                        "public_key": [1, 2, 3, 4],
                        "forward_foretis": [4, 5, 6],
                        "backward_foretis": [7, 8, 9],
                        "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
                    },
                    "received_at_ns": 99999
                }
            ]
        }"#;

        let record: ChrononRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.chronon_number, 5);
        assert_eq!(record.chronon_stamp_count, 2);
        assert_eq!(record.external_attestations.len(), 1);
        assert_eq!(record.external_attestations[0].attester_tbid, "eeff0011");
    }

    // --- Test 14: Foretis builder validation ---
    #[test]
    fn foretis_builder_validation() {
        let result = Foretis::builder()
            .chronon_number(0)
            .content_hash([0u8; 32])
            .tbid(vec![])
            .echo("".into())
            .tbn("".into())
            .time_being_reference_time("".into())
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().0.contains("chronon_number must be > 0"));
    }

    // --- Test 15: Empty JSON → all missing fields ---
    #[test]
    fn empty_json_rejected() {
        let json = r#"{}"#;
        let result = serde_json::from_str::<ChrononRecord>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing required field"), "error: {err}");
    }

    // --- Test 16: Partial JSON — only chronon_number ---
    #[test]
    fn partial_json_rejected() {
        let json = r#"{"tick_number": 1}"#;
        let result = serde_json::from_str::<ChrononRecord>(json);
        assert!(result.is_err());
    }

    // --- Test 17: Foretis with missing fields ---
    #[test]
    fn foretis_missing_required_fields() {
        let json = r#"{"chronon_number": 1}"#;
        let result = serde_json::from_str::<Foretis>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("content_hash"), "error: {err}");
    }

    // --- Test 18: Tick number rename works ---
    #[test]
    fn tick_number_rename_works() {
        let json = r#"{
            "tick_number": 99,
            "public_key": [1, 2, 3],
            "forward_foretis": [4, 5, 6],
            "backward_foretis": [7, 8, 9],
            "aa_nonce": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
        }"#;

        let record: ChrononRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.chronon_number, 99);
    }

    // --- Test 19: Serialize uses tick_number (not chronon_number) ---
    #[test]
    fn serialize_uses_tick_number() {
        let record = ChrononRecord::builder()
            .chronon_number(7)
            .public_key(vec![1, 2, 3])
            .forward_foretis(vec![4, 5, 6])
            .backward_foretis(vec![7, 8, 9])
            .aa_nonce([0u8; 16])
            .build()
            .unwrap();

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("tick_number"), "json: {json}");
        assert!(!json.contains("chronon_number"), "json: {json}");
    }

    // --- Test 20: bon builder missing required field → compile error ---
    // Uncomment to verify: omitting public_key produces a compile error
    // "method 'build' not found ... Unset<public_key>: IsSet not satisfied"
    //
    // fn bon_builder_missing_required_fails_compile() {
    //     let _ = ChrononRecord::builder()
    //         .chronon_number(1)
    //         // .public_key(vec![1, 2, 3])  // ← missing!
    //         .forward_foretis(vec![4, 5, 6])
    //         .backward_foretis(vec![7, 8, 9])
    //         .aa_nonce([0u8; 16])
    //         .build();
    // }
}

fn main() {
    println!("bon-serde-poc: run `cargo test` to verify all 19 tests pass");
}
