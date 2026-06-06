//! Config module — hierarchical configuration for Foretias nodes.

pub mod calendar;
pub mod calendar_persist;
pub mod chronomatter;
pub mod node;
pub mod p2p;
pub mod time_family;

pub use calendar::{CalendarConfig, EncryptionConfig};
pub use calendar_persist::{CalendarMetadata, PersistedCalendar};
pub use chronomatter::{ChronomatterConfig, KeyRotationConfig};
pub use node::{compute_attestation_interval, NodeConfig, FORETIAS_MUTUAL_ATTESTATION_MINIMUM};
pub use p2p::{CollisionConfig, CommunerdConfig, DHTConfig, MutualAttestConfig};
pub use time_family::{TimeFamilyCliConfig, TimeFamilyConfig};
