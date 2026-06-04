//! Config module — hierarchical configuration for Foretias nodes.

pub mod p2p;
pub mod chronomatter;
pub mod calendar;
pub mod time_family;
pub mod calendar_persist;
pub mod node;

pub use p2p::{DHTConfig, CollisionConfig, CommunerdConfig, MutualAttestConfig};
pub use chronomatter::{ChronomatterConfig, KeyRotationConfig};
pub use calendar::{CalendarConfig, EncryptionConfig};
pub use time_family::{TimeFamilyCliConfig, TimeFamilyConfig};
pub use calendar_persist::{PersistedCalendar, CalendarMetadata};
pub use node::{NodeConfig, FORETIAS_MUTUAL_ATTESTATION_MINIMUM, compute_attestation_interval};
