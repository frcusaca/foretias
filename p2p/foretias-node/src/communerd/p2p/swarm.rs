//! libp2p swarm construction and event loop.

use super::behaviour::{ForetiasBehaviour, ForetiasBehaviourEvent, RpcProtocolFactory};
use super::events::NetworkEvent;
use super::super::capabilities::PeerCapability;
use super::super::transport::TransportError;
use super::gossip::{probity_topic, heartbeat_topic};
use crate::probity::ProbityReport;
use foretias_core::collision::Heartbeat;
use foretias_core::error::NodeError;
use futures::StreamExt;
use libp2p::{
    identify, kad, gossipsub, request_response,
    swarm::{derive_prelude::ListenerId, SwarmEvent},
    tcp, noise, yamux,
    SwarmBuilder, PeerId,
};
use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use rand::seq::SliceRandom;
use tokio::sync::mpsc;
use async_trait::async_trait;

#[async_trait]
pub trait CommunerdRpcHandler: Send + Sync {
    async fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError>;
}

/// Find a free port within the given range by shuffling and testing each port.
pub fn find_free_port(range: std::ops::Range<u16>) -> Result<u16, NodeError> {
    let mut ports: Vec<u16> = range.collect();
    ports.shuffle(&mut rand::thread_rng());
    for port in ports {
        if let Ok(_listener) = TcpListener::bind(format!("0.0.0.0:{}", port)) {
            return Ok(port);
        }
    }
    Err(NodeError::Internal("no free ports in range".into()))
}

pub struct SwarmHandle {
    pub local_peer_id: PeerId,
    pub local_multiaddr: Arc<Mutex<Option<libp2p::Multiaddr>>>,
    pub events: mpsc::UnboundedReceiver<NetworkEvent>,
    pub cmd_tx: mpsc::UnboundedSender<SwarmCommand>,
    pub task: tokio::task::JoinHandle<()>,
}

pub enum SwarmCommand {
    Bootstrap,
    Provide { key: kad::RecordKey },
    GetProviders { key: kad::RecordKey },
    PutRecord { key: kad::RecordKey, record: kad::Record },
    PutRecordTo { key: kad::RecordKey, record: kad::Record, peers: Vec<PeerId> },
    /// Store a record directly in the local kad store (bypasses network entirely).
    StoreRecordLocal { record: kad::Record },
    GetRecord { key: kad::RecordKey },
    Dial { addr: libp2p::Multiaddr },
    AddAddress { peer_id: PeerId, addr: libp2p::Multiaddr },
    EnterDormancy,
    PublishProbity { report: ProbityReport, namespace: String },
    PublishHeartbeat { heartbeat: Heartbeat, namespace: String },
    ProvideForCapability { capability: PeerCapability, namespace: String },
    GetProvidersForCapability { capability: PeerCapability, namespace: String },
    RequestResponse {
        peer_id: PeerId,
        request: serde_json::Value,
        reply: tokio::sync::oneshot::Sender<Result<serde_json::Value, crate::communerd::transport::TransportError>>,
    },
}

/// Build and spawn a libp2p swarm.
pub async fn build_and_spawn_swarm(
    listen: Option<libp2p::Multiaddr>,
    dials: Vec<libp2p::Multiaddr>,
    namespace: &str,
    json_rpc_addr: Option<&str>,
    rpc_handler: Option<Arc<dyn CommunerdRpcHandler>>,
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
        .with_behaviour(|key| ForetiasBehaviour::new(key, namespace, json_rpc_addr))
        .map_err(|e| NodeError::Internal(format!("{e}")))?
        .with_swarm_config(|c: libp2p::swarm::Config| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
        })
        .build();

    if let Some(listen_addr) = listen {
        swarm.listen_on(listen_addr)
            .map_err(|e| NodeError::Internal(format!("{e}")))?;
    }

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
    let local_multiaddr = Arc::new(Mutex::new(None));
    let rpc_factory = RpcProtocolFactory::new(namespace);
    let task = tokio::spawn(swarm_loop(swarm, events_tx, cmd_rx, namespace.to_string(), local_multiaddr.clone(), rpc_handler, rpc_factory));

    Ok(SwarmHandle {
        local_peer_id,
        local_multiaddr,
        events: events_rx,
        cmd_tx,
        task,
    })
}

async fn swarm_loop(
    mut swarm: libp2p::Swarm<ForetiasBehaviour>,
    tx: mpsc::UnboundedSender<NetworkEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<SwarmCommand>,
    _namespace: String,
    local_multiaddr: Arc<Mutex<Option<libp2p::Multiaddr>>>,
    rpc_handler: Option<Arc<dyn CommunerdRpcHandler>>,
    rpc_factory: RpcProtocolFactory,
) {
    let mut listener_ids: HashSet<ListenerId> = HashSet::new();
    let mut pending_get_record: HashMap<libp2p::kad::QueryId, (kad::RecordKey, Vec<kad::Record>)> = HashMap::new();
    let mut pending_put_record: HashMap<libp2p::kad::QueryId, kad::RecordKey> = HashMap::new();
    let mut pending_rpc: HashMap<
        request_response::OutboundRequestId,
        tokio::sync::oneshot::Sender<Result<serde_json::Value, TransportError>>,
    > = HashMap::new();
    let mut pending_inbound: HashMap<
        request_response::InboundRequestId,
        (
            request_response::ResponseChannel<Vec<u8>>,
            libp2p::PeerId,
        ),
    > = HashMap::new();
    let (inbound_res_tx, mut inbound_res_rx) = tokio::sync::mpsc::unbounded_channel::<(
        request_response::InboundRequestId,
        Vec<u8>,
    )>();

    loop {
        tokio::select! {
            biased;
            inbound_res = inbound_res_rx.recv() => {
                if let Some((request_id, bytes)) = inbound_res {
                    if let Some((channel, peer)) = pending_inbound.remove(&request_id) {
                        let _ = swarm.behaviour_mut().request_response.send_response(channel, bytes);
                        tracing::debug!(peer = %peer, "libp2p RPC response sent");
                    }
                }
            }
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
                    Some(SwarmCommand::AddAddress { peer_id, addr }) => {
                        swarm.behaviour_mut().kad.add_address(&peer_id, addr);
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
                    Some(SwarmCommand::PutRecord { key, record }) => {
                        let key_vec = key.to_vec();
                        match swarm.behaviour_mut().kad.put_record(record, kad::Quorum::One) {
                            Ok(query_id) => {
                                pending_put_record.insert(query_id, key.clone());
                                tracing::info!(key = ?key_vec, "PutRecord initiated");
                            }
                            Err(e) => {
                                let _ = tx.send(NetworkEvent::RecordPutError {
                                    key: key.clone(),
                                    error: e.to_string(),
                                });
                                tracing::warn!(key = ?key_vec, ?e, "PutRecord failed");
                            }
                        }
                    }
                    Some(SwarmCommand::PutRecordTo { key, record, peers }) => {
                        let num = peers.len();
                        let key_vec = key.to_vec();
                        let query_id = swarm.behaviour_mut().kad.put_record_to(record, peers.into_iter(), kad::Quorum::One);
                        pending_put_record.insert(query_id, key.clone());
                        tracing::info!(key = ?key_vec, num_peers = num, "PutRecordTo initiated");
                    }
                    Some(SwarmCommand::StoreRecordLocal { record }) => {
                        use libp2p::kad::store::RecordStore;
                        let key = record.key.clone();
                        if let Err(e) = swarm.behaviour_mut().kad.store_mut().put(record) {
                            tracing::warn!(?e, key = ?key.to_vec(), "StoreRecordLocal failed");
                        } else {
                            tracing::info!(key = ?key.to_vec(), "StoreRecordLocal succeeded");
                        }
                    }
                    Some(SwarmCommand::GetRecord { key }) => {
                        let entry = (key.clone(), Vec::new());
                        let query_id = swarm.behaviour_mut().kad.get_record(key);
                        pending_get_record.insert(query_id, entry);
                    }
                    Some(SwarmCommand::ProvideForCapability { capability, namespace }) => {
                        let key = capability.provider_key(&namespace);
                        let _ = swarm.behaviour_mut().kad.start_providing(key);
                    }
                    Some(SwarmCommand::GetProvidersForCapability { capability, namespace }) => {
                        let key = capability.provider_key(&namespace);
                        let _ = swarm.behaviour_mut().kad.get_providers(key);
                    }
                    Some(SwarmCommand::RequestResponse { peer_id, request, reply }) => {
                        let bytes = match serde_json::to_vec(&request) {
                            Ok(b) => b,
                            Err(e) => {
                                let _ = reply.send(Err(TransportError::Decode(e.to_string())));
                                continue;
                            }
                        };
                        let _ = rpc_factory.create_protocol();
                        let request_id = swarm.behaviour_mut().request_response.send_request(&peer_id, bytes);
                        pending_rpc.insert(request_id, reply);
                        tracing::debug!(peer = %peer_id, "libp2p RPC request sent");
                    }
                    None => break,
                }
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, listener_id, .. } => {
                        listener_ids.insert(listener_id);
                        let mut ma = local_multiaddr.lock().unwrap();
                        if ma.is_none() {
                            *ma = Some(address.clone());
                        }
                        let _ = tx.send(NetworkEvent::ListenReady { multiaddr: address });
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
                    SwarmEvent::Behaviour(ForetiasBehaviourEvent::Identify(event)) => {
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
                    SwarmEvent::Behaviour(ForetiasBehaviourEvent::Ping(event)) => {
                        if let Ok(rtt) = event.result {
                            let _ = tx.send(NetworkEvent::PingSuccess {
                                peer_id: event.peer,
                                rtt,
                            });
                            tracing::debug!(peer = %event.peer, rtt = ?rtt, "libp2p ping");
                        }
                    }
                    SwarmEvent::Behaviour(ForetiasBehaviourEvent::Kad(event)) => {
                        match event {
                            kad::Event::OutboundQueryProgressed { id, result, .. } => {
                                match result {
                                    kad::QueryResult::Bootstrap(Ok(_)) => {
                                        let _ = tx.send(NetworkEvent::DhtBootstrapComplete);
                                        tracing::info!("DHT bootstrap complete");
                                    }
                                    kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord { record, .. }))) => {
                                        if let Some(entry) = pending_get_record.get_mut(&id) {
                                            entry.1.push(record);
                                        }
                                    }
                                    kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. })) => {
                                        if let Some((key, records)) = pending_get_record.remove(&id) {
                                            let _ = tx.send(NetworkEvent::RecordRetrieved {
                                                key,
                                                records,
                                            });
                                            tracing::info!("DHT: GetRecord query finished");
                                        }
                                    }
                                    kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { .. })) => {
                                        if let Some(k) = pending_put_record.remove(&id) {
                                            let _ = tx.send(NetworkEvent::RecordPutOk { key: k });
                                            tracing::info!("DHT: PutRecord succeeded");
                                        }
                                    }
                                    kad::QueryResult::PutRecord(Err(e)) => {
                                        if let Some(k) = pending_put_record.remove(&id) {
                                            let _ = tx.send(NetworkEvent::RecordPutError {
                                                key: k,
                                                error: e.to_string(),
                                            });
                                            tracing::warn!(?e, "DHT: PutRecord failed");
                                        }
                                    }
                                    kad::QueryResult::GetRecord(Err(e)) => {
                                        if let Some((key, _)) = pending_get_record.remove(&id) {
                                            tracing::warn!(?e, key = ?key.to_vec(), "DHT: GetRecord query failed");
                                        }
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
                      SwarmEvent::Behaviour(ForetiasBehaviourEvent::RequestResponse(event)) => {
                          match event {
                              request_response::Event::Message { peer, message, .. } => {
                                  match message {
                                      request_response::Message::Request { request_id, request, channel } => {
                                          let req_val: serde_json::Value = match serde_json::from_slice(&request) {
                                              Ok(v) => v,
                                              Err(e) => {
                                                  tracing::warn!(peer = %peer, ?e, "libp2p RPC: invalid JSON");
                                                  continue;
                                              }
                                          };
                                          let method = req_val.get("method")
                                              .and_then(|m| m.as_str())
                                              .unwrap_or("")
                                              .to_string();
                                          let params = req_val.get("params").cloned().unwrap_or(serde_json::Value::Null);
                                          match &rpc_handler {
                                              Some(handler) => {
                                                  let h = Arc::clone(handler);
                                                  let tx = inbound_res_tx.clone();
                                                  pending_inbound.insert(request_id, (channel, peer));
                                                  tokio::spawn(async move {
                                                          let bytes = match h.handle(&method, params).await {
                                                          Ok(result) => match serde_json::to_vec(&result) {
                                                              Ok(b) => b,
                                                              Err(e) => {
                                                                  tracing::warn!(peer = %peer, ?e, "libp2p RPC: serialize response failed");
                                                                  Vec::new()
                                                              }
                                                          },
                                                          Err(e) => {
                                                              tracing::warn!(peer = %peer, ?e, "libp2p RPC: handler error");
                                                              Vec::new()
                                                          }
                                                      };
                                                      let _ = tx.send((request_id, bytes));
                                                  });
                                              }
                                              None => {
                                                  tracing::warn!(peer = %peer, method, "libp2p RPC: no handler configured");
                                              }
                                          }
                                      }
                                      request_response::Message::Response { request_id, response } => {
                                          if let Some(sender) = pending_rpc.remove(&request_id) {
                                              let val: serde_json::Value = match serde_json::from_slice(&response) {
                                                  Ok(v) => v,
                                                  Err(e) => {
                                                      let _ = sender.send(Err(TransportError::Decode(e.to_string())));
                                                      continue;
                                                  }
                                              };
                                              let result: Result<serde_json::Value, TransportError> = if let Some(err) = val.get("error") {
                                                  let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
                                                  let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
                                                  Err(TransportError::Rpc { code, message })
                                              } else {
                                                  Ok(val.get("result").cloned().unwrap_or(serde_json::Value::Null))
                                              };
                                              let _ = sender.send(result);
                                          }
                                      }
                                  }
                              }
                              request_response::Event::OutboundFailure { request_id, error, peer, .. } => {
                                  if let Some(sender) = pending_rpc.remove(&request_id) {
                                      let err = match error {
                                          request_response::OutboundFailure::Timeout => TransportError::Timeout,
                                          _ => TransportError::Connect(error.to_string()),
                                      };
                                      let _ = sender.send(Err(err));
                                      tracing::warn!(peer = %peer, ?error, "libp2p RPC outbound failure");
                                  }
                              }
                              request_response::Event::InboundFailure { .. } => {}
                              request_response::Event::ResponseSent { .. } => {}
                              _ => {}
                          }
                      }
                    SwarmEvent::Behaviour(ForetiasBehaviourEvent::Gossip(event)) => {
                        match event {
                            gossipsub::Event::Message { propagation_source, message, .. } => {
                                tracing::debug!(
                                    peer = %propagation_source,
                                    "received gossipsub message"
                                );
                                let topic_str = message.topic.to_string();
                                if topic_str.starts_with("/foretias/") && topic_str.ends_with("/heartbeat/v1") {
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
        let mut handle = build_and_spawn_swarm(Some(listen), vec![], "mainnet", None, None).await.unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle.events.recv()).await;
        assert!(!handle.local_peer_id.to_string().is_empty());
        handle.task.abort();
    }
}
