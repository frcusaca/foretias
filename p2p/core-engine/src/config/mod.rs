//! Config module — hierarchical configuration for Foretias nodes.

pub mod p2p;
pub mod chronomatter;
pub mod calendar;
pub mod time_family;
pub mod calendar_persist;
pub mod node;

pub use p2p::{P2PConfig, DHTConfig, CollisionConfig};
pub use chronomatter::{ChronomatterConfig, AutoAttestConfig, KeyRotationConfig};
pub use calendar::{CalendarConfig, EncryptionConfig};
pub use time_family::TimeFamilyConfig;
pub use calendar_persist::{PersistedCalendar, CalendarMetadata};
#[allow(deprecated)]
pub use node::{NodeConfig, FORETIAS_MUTUAL_ATTESTATION_MINIMUM, compute_attestation_interval};
