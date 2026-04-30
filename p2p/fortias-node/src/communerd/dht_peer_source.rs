//! Peer source abstraction for DHT-discovered peers.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use libp2p::PeerId;
use parking_lot::RwLock;

use super::transport::PeerAddr;

/// Abstract source of peers for auto-attestation scheduling.
#[async_trait]
pub trait PeerSource: Send + Sync {
    /// Return current list of known peers.
    async fn list(&self) -> Vec<PeerAddr>;
}

/// Peer source backed by Kademlia DHT discoveries.
pub struct DhtPeerSource {
    peers: Arc<RwLock<HashMap<PeerId, PeerAddr>>>,
}

impl Default for DhtPeerSource {
    fn default() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl DhtPeerSource {
    /// Insert or update a discovered peer.
    pub fn upsert(&self, peer_id: PeerId, addr: PeerAddr) {
        self.peers.write().insert(peer_id, addr);
    }

    /// Remove a peer.
    pub fn remove(&self, peer_id: &PeerId) {
        self.peers.write().remove(peer_id);
    }

    /// Update the json_rpc address for a known peer (from an Identified event).
    pub fn update_json_rpc(&self, peer_id: &PeerId, json_rpc: String) {
        let mut guard = self.peers.write();
        if let Some(entry) = guard.get_mut(peer_id) {
            entry.json_rpc = json_rpc;
        }
    }
}

#[async_trait]
impl PeerSource for DhtPeerSource {
    async fn list(&self) -> Vec<PeerAddr> {
        self.peers.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dht_peer_source_upsert_remove() {
        let src = DhtPeerSource::default();
        let pid = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        assert!(src.list().await.is_empty());

        src.upsert(
            pid,
            PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: Some(pid) },
        );

        let peers = src.list().await;
        assert_eq!(peers.len(), 1);

        src.remove(&pid);
        assert!(src.list().await.is_empty());
    }
}
