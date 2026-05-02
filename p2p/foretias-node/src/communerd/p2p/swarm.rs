//! libp2p swarm construction and event loop.

use super::behaviour::{FortiasBehaviour, FortiasBehaviourEvent};
use super::events::NetworkEvent;
use super::gossip::{probity_topic, heartbeat_topic};
use crate::probity::ProbityReport;
use foretias_core::collision::Heartbeat;
use foretias_core::error::NodeError;
use futures::StreamExt;
use libp2p::{
    identify, kad, gossipsub,
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
    PublishProbity { report: ProbityReport, namespace: String },
    PublishHeartbeat { heartbeat: Heartbeat, namespace: String },
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

    // Subscribe to probity topic
    let topic = probity_topic(namespace);
    swarm.behaviour_mut().gossip.subscribe(&topic)
        .map_err(|e| NodeError::Internal(format!("gossip subscribe: {e}")))?;

    // Subscribe to heartbeat topic for collision detection
    let hbt = heartbeat_topic(namespace);
    swarm.behaviour_mut().gossip.subscribe(&hbt)
        .map_err(|e| NodeError::Internal(format!("gossip subscribe heartbeat: {e}")))?;

    for ma in dials {
        let _ = swarm.dial(ma);
    }

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(swarm_loop(swarm, events_tx, cmd_rx, namespace.to_string()));

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
    _namespace: String,
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
                    Some(SwarmCommand::PublishProbity { report, namespace }) => {
                        let bytes = match serde_json::to_vec(&report) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!("failed to serialize ProbityReport: {e}");
                                continue;
                            }
                        };
                        let topic = probity_topic(&namespace);
                        if let Err(e) = swarm.behaviour_mut().gossip.publish(topic, bytes) {
                            tracing::warn!("failed to publish probity report: {e}");
                        }
                    }
                    Some(SwarmCommand::PublishHeartbeat { heartbeat, namespace }) => {
                        let bytes = match serde_json::to_vec(&heartbeat) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!("failed to serialize Heartbeat: {e}");
                                continue;
                            }
                        };
                        let topic = heartbeat_topic(&namespace);
                        if let Err(e) = swarm.behaviour_mut().gossip.publish(topic, bytes) {
                            tracing::warn!("failed to publish heartbeat: {e}");
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
                    SwarmEvent::Behaviour(FortiasBehaviourEvent::Gossip(event)) => {
                        match event {
                            gossipsub::Event::Message { propagation_source, message, .. } => {
                                tracing::debug!(
                                    peer = %propagation_source,
                                    "received gossipsub message"
                                );
                                let topic_str = message.topic.to_string();
                                if topic_str.starts_with("/fortias/") && topic_str.ends_with("/heartbeat/v1") {
                                    let _ = tx.send(NetworkEvent::HeartbeatMessage {
                                        data: message.data.clone(),
                                        source: propagation_source,
                                    });
                                } else {
                                    let _ = tx.send(NetworkEvent::GossipMessage {
                                        data: message.data.clone(),
                                        source: propagation_source,
                                    });
                                }
                            }
                            gossipsub::Event::Subscribed { peer_id, topic } => {
                                tracing::info!(peer = %peer_id, ?topic, "peer subscribed to gossip topic");
                            }
                            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                                tracing::info!(peer = %peer_id, ?topic, "peer unsubscribed from gossip topic");
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
