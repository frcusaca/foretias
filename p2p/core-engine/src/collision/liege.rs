//! Liege channel — reports collision events to application logic.

use crate::collision::heartbeat::Heartbeat;
use crate::error::NodeError;

/// Reason for requesting help from the liege (application-level peer manager).
#[derive(Debug, Clone)]
pub enum HelpReason {
    IdentityCollision { foreign_heartbeat: Heartbeat },
    SuspectedBadActor { peer_id: String },
}

pub trait LiegeChannel: Send + Sync {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError>;
}

/// Stub implementation: pushes to an internal mpsc channel.
/// The application can poll this channel to observe collision events.
pub struct StubLiegeChannel {
    tx: tokio::sync::mpsc::UnboundedSender<HelpReason>,
}

impl StubLiegeChannel {
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<HelpReason>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

impl LiegeChannel for StubLiegeChannel {
    fn send_help(&self, reason: HelpReason) -> Result<(), NodeError> {
        self.tx
            .send(reason)
            .map_err(|e| NodeError::Internal(e.to_string()))
    }
}
