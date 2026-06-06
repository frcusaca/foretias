//! Collision detection module — heartbeat, detector, liege, escalation.

pub mod detector;
pub mod escalation;
pub mod heartbeat;
pub mod liege;

pub use detector::{CollisionDetector, CollisionEvent};
pub use escalation::handle_confirmed_collision;
pub use heartbeat::Heartbeat;
pub use liege::{HelpReason, LiegeChannel, StubLiegeChannel};
