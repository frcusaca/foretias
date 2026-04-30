//! libp2p swarm construction and event loop.

use super::behaviour::{FortiasBehaviour, FortiasBehaviourEvent};
use super::events::NetworkEvent;
use fortias_core::error::NodeError;
use futures::StreamExt;
use libp2p::{
    identify,
    swarm::SwarmEvent,
    tcp, noise, yamux,
    SwarmBuilder, PeerId,
};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SwarmHandle {
    pub local_peer_id: PeerId,
    pub events: mpsc::UnboundedReceiver<NetworkEvent>,
    pub task: tokio::task::JoinHandle<()>,
}

/// Build and spawn a libp2p swarm.
pub async fn build_and_spawn_swarm(
    listen: libp2p::Multiaddr,
    dials: Vec<libp2p::Multiaddr>,
) -> Result<SwarmHandle, NodeError> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let local_peer_id = keypair.public().to_peer_id();

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|e| NodeError::Internal(format!("{e}")))?
        .with_behaviour(|key| FortiasBehaviour::new(key))
        .map_err(|e| NodeError::Internal(format!("{e}")))?
        .with_swarm_config(|c: libp2p::swarm::Config| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    swarm.listen_on(listen)
        .map_err(|e| NodeError::Internal(format!("{e}")))?;

    for ma in dials {
        let _ = swarm.dial(ma);
    }

    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(swarm_loop(swarm, tx));

    Ok(SwarmHandle {
        local_peer_id,
        events: rx,
        task,
    })
}

async fn swarm_loop(
    mut swarm: libp2p::Swarm<FortiasBehaviour>,
    tx: mpsc::UnboundedSender<NetworkEvent>,
) {
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                let _ = tx.send(NetworkEvent::Connected { peer_id });
                tracing::info!(peer = %peer_id, "libp2p connection established");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                let _ = tx.send(NetworkEvent::Disconnected { peer_id });
                tracing::info!(peer = %peer_id, "libp2p connection closed");
            }
            SwarmEvent::Behaviour(FortiasBehaviourEvent::Identify(event)) => {
                match event {
                    identify::Event::Sent { peer_id, .. } => {
                        tracing::debug!(peer = %peer_id, "libp2p identify sent");
                    }
                    identify::Event::Received { peer_id, info, .. } => {
                        let _ = tx.send(NetworkEvent::Identified {
                            peer_id,
                            info: info.clone(),
                        });
                        tracing::info!(
                            peer = %peer_id,
                            agent = %info.agent_version,
                            "libp2p identify received"
                        );
                    }
                    identify::Event::Error { peer_id, error, .. } => {
                        tracing::warn!(peer = %peer_id, ?error, "libp2p identify error");
                    }
                    _ => {}
                }
            }
            SwarmEvent::Behaviour(FortiasBehaviourEvent::Ping(event)) => {
                if let Ok(rtt) = event.result {
                    let _ = tx.send(NetworkEvent::PingSuccess {
                        peer_id: event.peer,
                        rtt,
                    });
                    tracing::debug!(peer = %event.peer, rtt = ?rtt, "libp2p ping");
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn swarm_listen_only() {
        let listen: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        let mut handle = build_and_spawn_swarm(listen, vec![]).await.unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle.events.recv()).await;
        assert!(!handle.local_peer_id.to_string().is_empty());
        handle.task.abort();
    }
}
