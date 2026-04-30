//! Fortias domain types (TickRecord, Fortis, Calendar, TimeFamily).

pub mod types;
pub mod tick;
pub mod calendar;
pub mod external_attestation;
pub mod callbacks;

pub use types::{Tbid, PublicKey, Signature, Digest, Message, AaNonce, TickNumber};
pub use tick::{TickRecord, Fortis, CalendarLookup, stamp, verify, auto_attestation_blob, verify_pair};
pub use calendar::Calendar;
pub use external_attestation::ExternalAttestation;
pub use callbacks::{TickObserver, Stamper, PeerMessenger, PeerAddr, CommunityQuery, CommunityResponse, TransportError};
