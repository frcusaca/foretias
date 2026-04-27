//! Fortias domain types (TickRecord, Fortis, Calendar, TimeFamily).

pub mod tick;
pub mod calendar;

pub use tick::{TickRecord, Fortis, CalendarLookup, stamp, verify, auto_attestation_blob, verify_pair};
pub use calendar::Calendar;
