//! Fortias domain types (TickRecord, Fortis, Calendar, TimeFamily).

pub mod types;
pub mod tick;
pub mod calendar;

pub use types::{Tbid, PublicKey, Signature, Digest, Message, AaNonce, TickNumber};
pub use tick::{TickRecord, Fortis, CalendarLookup, stamp, verify, auto_attestation_blob, verify_pair};
pub use calendar::Calendar;
