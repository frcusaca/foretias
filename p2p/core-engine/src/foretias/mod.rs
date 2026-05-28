//! Foretias domain types (ChrononRecord, Foretis, Calendar, TimeFamily).

pub mod types;
pub mod tick;
pub mod calendar;
pub mod external_attestation;
pub mod callbacks;
pub mod encoding;
pub mod clean_auth;
pub use clean_auth::{CleanAuthenticated, CleanFullyAuthenticated, Externalized, Unprocessed};

pub use types::{Tbid, TbidSecret, PublicKeyBytes, SignatureBytes, AlgorithmId, Digest, Message, AaNonce, TickNumber, SignatureAlgorithm, KemAlgorithm};
pub use tick::{ChrononRecord, Foretis, CalendarLookup, stamp, verify, auto_attestation_blob, auto_attestation_blob_with_count, auto_attestation_blob_with_genesis, verify_pair};
pub use calendar::Calendar;
pub use external_attestation::ExternalAttestation;
pub use callbacks::{TickObserver, Attester, PeerMessenger, PeerAddr, CommunityQuery, CommunityResponse, TransportError, MutualAttestObserver};

#[cfg(test)]
mod encoding_tests;
