use std::sync::Arc;

use crate::collision::heartbeat::Heartbeat;
use crate::collision::liege::{HelpReason, LiegeChannel};

pub async fn handle_confirmed_collision(
    set_dormant: impl Fn(bool) + Send + 'static,
    liege: Arc<dyn LiegeChannel>,
    swarm_cmd_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    liege_wait_secs: u64,
    foreign_heartbeat: Heartbeat,
) {
    tracing::warn!("collision confirmed — entering dormancy");

    set_dormant(true);

    let _ = liege.send_help(HelpReason::IdentityCollision {
        foreign_heartbeat: foreign_heartbeat.clone(),
    });

    tokio::time::sleep(std::time::Duration::from_secs(liege_wait_secs)).await;

    if let Some(tx) = swarm_cmd_tx {
        let _ = tx.send("EnterDormancy".to_string());
    }

    tracing::warn!("dormancy complete: P2P suspended; local verify still active");
}
