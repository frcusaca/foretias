//! Collision detection module — heartbeat, detector, liege, escalation.

pub mod heartbeat;
pub mod detector;
pub mod liege;
pub mod escalation;

pub use heartbeat::Heartbeat;
pub use detector::{CollisionDetector, CollisionEvent};
pub use liege::{LiegeChannel, StubLiegeChannel, HelpReason};
pub use escalation::handle_confirmed_collision;
