//! Foretias domain types (ChrononRecord, ForetisRecord, Calendar, TimeFamily).

pub mod calendar;
pub mod callbacks;
pub mod clean_auth;
pub mod encoding;
pub mod external_attestation;
pub mod family_record;
pub mod tick;
pub mod types;
pub use clean_auth::{
    CleanAuthenticated, CleanFullyAuthenticated, Externalized, UnverifiedSignatureEnvelope,
};
pub use family_record::{FamilyError, FamilyRecord, MAX_FAMILY_MEMBERS};

pub use calendar::Calendar;
pub use callbacks::{
    Attester, CommunityQuery, CommunityResponse, MutualAttestObserver, PeerAddr, PeerMessenger,
    TickObserver, TransportError,
};
pub use external_attestation::ExternalAttestationRecord;
pub use tick::{
    auto_attestation_blob, auto_attestation_blob_with_count, auto_attestation_blob_with_genesis,
    stamp, verify, verify_pair, CalendarLookup, ChrononRecord, ForetisRecord,
    SerializationAlgorithm,
};
pub use types::{
    AaNonce, AlgorithmId, Digest, KemAlgorithm, Message, PublicKeyBytes, SignatureAlgorithm,
    SignatureBytes, Tbid, TbidSecret, TickNumber,
};

#[cfg(test)]
mod encoding_tests;
