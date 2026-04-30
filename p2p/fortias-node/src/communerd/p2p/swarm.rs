//! libp2p swarm construction and event loop.

use super::behaviour::{FortiasBehaviour, FortiasBehaviourEvent};
use super::events::NetworkEvent;
use fortias_core::error::NodeError;
use futures::StreamExt;
use libp2p::{
    identify, kad,
    swarm::{derive_prelude::ListenerId, SwarmEvent},
    tcp, noise, yamux,
    SwarmBuilder, PeerId,
};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct SwarmHandle {
    pub local_peer_id: PeerId,
    pub events: mpsc::UnboundedReceiver<NetworkEvent>,
    pub cmd_tx: mpsc::UnboundedSender<SwarmCommand>,
    pub task: tokio::task::JoinHandle<()>,
}

pub enum SwarmCommand {
    Bootstrap,
    Provide { key: kad::RecordKey },
    GetProviders { key: kad::RecordKey },
    Dial { addr: libp2p::Multiaddr },
    EnterDormancy,
}

/// Build and spawn a libp2p swarm.
pub async fn build_and_spawn_swarm(
    listen: libp2p::Multiaddr,
    dials: Vec<libp2p::Multiaddr>,
    namespace: &str,
    json_rpc_addr: Option<&str>,
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
        .with_behaviour(|key| FortiasBehaviour::new(key, namespace, json_rpc_addr))
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

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(swarm_loop(swarm, events_tx, cmd_rx));

    Ok(SwarmHandle {
        local_peer_id,
        events: events_rx,
        cmd_tx,
        task,
    })
}

async fn swarm_loop(
    mut swarm: libp2p::Swarm<FortiasBehaviour>,
    tx: mpsc::UnboundedSender<NetworkEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<SwarmCommand>,
) {
    let mut listener_ids: HashSet<ListenerId> = HashSet::new();

    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(SwarmCommand::Bootstrap) => {
                        let _ = swarm.behaviour_mut().kad.bootstrap();
                    }
                    Some(SwarmCommand::Provide { key }) => {
                        let _ = swarm.behaviour_mut().kad.start_providing(key);
                    }
                    Some(SwarmCommand::GetProviders { key }) => {
                        let _ = swarm.behaviour_mut().kad.get_providers(key);
                    }
                    Some(SwarmCommand::Dial { addr }) => {
                        let _ = swarm.dial(addr);
                    }
                    Some(SwarmCommand::EnterDormancy) => {
                        let peers: Vec<PeerId> = swarm.connected_peers().cloned().collect();
                        for peer in peers {
                            let _ = swarm.disconnect_peer_id(peer);
                        }
                        for lid in listener_ids.drain() {
                            let _ = swarm.remove_listener(lid);
                        }
                    }
                    None => break,
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { listener_id, .. } => {
                        listener_ids.insert(listener_id);
                    }
                    SwarmEvent::ListenerClosed { listener_id, .. }
                    | SwarmEvent::ListenerError { listener_id, .. } => {
                        listener_ids.remove(&listener_id);
                    }
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
                    SwarmEvent::Behaviour(FortiasBehaviourEvent::Kad(event)) => {
                        match event {
                            kad::Event::OutboundQueryProgressed { result, .. } => {
                                match result {
                                    kad::QueryResult::Bootstrap(Ok(_)) => {
                                        let _ = tx.send(NetworkEvent::DhtBootstrapComplete);
                                        tracing::info!("DHT bootstrap complete");
                                    }
                                    kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })) => {
                                        for peer_id in providers {
                                            let addresses = vec![];
                                            let _ = tx.send(NetworkEvent::DhtPeerDiscovered {
                                                peer_id,
                                                addresses,
                                            });
                                            tracing::info!(peer = %peer_id, "DHT: discovered peer via providers");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn swarm_listen_only() {
        let listen: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        let mut handle = build_and_spawn_swarm(listen, vec![], "mainnet", None).await.unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle.events.recv()).await;
        assert!(!handle.local_peer_id.to_string().is_empty());
        handle.task.abort();
    }
}
