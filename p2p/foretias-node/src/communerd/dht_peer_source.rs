//! Peer source abstraction for DHT-discovered peers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use libp2p::PeerId;
use parking_lot::RwLock;

use super::capabilities::PeerCapability;
use super::transport::PeerAddr;

/// Maximum number of peers allowed in the DHT peer source.
const MAX_PEERS: usize = 256;

/// Abstract source of peers for auto-attestation scheduling.
#[async_trait]
pub trait PeerSource: Send + Sync {
    /// Return current list of known peers.
    async fn list(&self) -> Vec<PeerAddr>;
}

/// Peer source backed by Kademlia DHT discoveries.
pub struct DhtPeerSource {
    peers: Arc<RwLock<HashMap<PeerId, PeerAddr>>>,
    capability_index: Arc<RwLock<HashMap<PeerCapability, HashSet<PeerId>>>>,
}

impl Default for DhtPeerSource {
    fn default() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            capability_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl DhtPeerSource {
    /// Insert or update a discovered peer.
    /// If the peer table is full and the peer is not already known, the insertion is rejected.
    pub fn upsert(&self, peer_id: PeerId, addr: PeerAddr) {
        self.upsert_with_capabilities(peer_id, addr, vec![PeerCapability::AttestWilling]);
    }

    /// Insert or update a peer, indexing it under the given capabilities.
    /// If the peer table is full and the peer is not already known, the insertion is rejected.
    pub fn upsert_with_capabilities(
        &self,
        peer_id: PeerId,
        addr: PeerAddr,
        caps: Vec<PeerCapability>,
    ) {
        let mut peers = self.peers.write();
        if peers.len() >= MAX_PEERS && !peers.contains_key(&peer_id) {
            tracing::warn!(peer = %peer_id, max_peers = MAX_PEERS, "DHT peer table full, rejecting new peer");
            return;
        }
        peers.insert(peer_id, addr);

        let mut idx = self.capability_index.write();
        for cap in caps {
            idx.entry(cap).or_default().insert(peer_id);
        }
    }

    /// Return the addresses of all peers that advertise the given capability.
    pub async fn list_by_capability(&self, cap: PeerCapability) -> Vec<PeerAddr> {
        let peer_ids = {
            let idx = self.capability_index.read();
            idx.get(&cap).cloned().unwrap_or_default()
        };
        let peers = self.peers.read();
        peer_ids
            .into_iter()
            .filter_map(|id| peers.get(&id).cloned())
            .collect()
    }

    /// Remove a peer from both the peers map and all capability indices.
    pub fn remove(&self, peer_id: &PeerId) {
        self.peers.write().remove(peer_id);
        let mut idx = self.capability_index.write();
        for set in idx.values_mut() {
            set.remove(peer_id);
        }
        idx.retain(|_, set| !set.is_empty());
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

    fn make_addr(peer_id: PeerId) -> PeerAddr {
        PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: Some(peer_id), last_seen_ns: 0 }
    }

    #[tokio::test]
    async fn dht_peer_source_upsert_remove() {
        let src = DhtPeerSource::default();
        let pid = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        assert!(src.list().await.is_empty());

        src.upsert(pid, make_addr(pid));

        let peers = src.list().await;
        assert_eq!(peers.len(), 1);

        src.remove(&pid);
        assert!(src.list().await.is_empty());
    }

    #[tokio::test]
    async fn upsert_with_capabilities_indexes_peers() {
        let src = DhtPeerSource::default();
        let p1 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();
        let p2 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        src.upsert_with_capabilities(
            p1,
            PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: Some(p1), last_seen_ns: 0 },
            vec![PeerCapability::AttestWilling, PeerCapability::MirrorWilling],
        );
        src.upsert_with_capabilities(
            p2,
            PeerAddr { json_rpc: "127.0.0.1:4002".into(), peer_id: Some(p2), last_seen_ns: 0 },
            vec![PeerCapability::MirrorWilling],
        );

        let attest = src.list_by_capability(PeerCapability::AttestWilling).await;
        assert_eq!(attest.len(), 1);
        assert_eq!(attest[0].json_rpc, "127.0.0.1:4001");

        let mirror = src.list_by_capability(PeerCapability::MirrorWilling).await;
        assert_eq!(mirror.len(), 2);

        let verify = src.list_by_capability(PeerCapability::VerifierWilling).await;
        assert!(verify.is_empty());
    }

    #[tokio::test]
    async fn list_by_capability_filters_correctly() {
        let src = DhtPeerSource::default();
        let p1 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();
        let p2 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();
        let p3 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        src.upsert_with_capabilities(
            p1,
            PeerAddr { json_rpc: "127.0.0.1:1001".into(), peer_id: Some(p1), last_seen_ns: 0 },
            vec![PeerCapability::AttestWilling],
        );
        src.upsert_with_capabilities(
            p2,
            PeerAddr { json_rpc: "127.0.0.1:1002".into(), peer_id: Some(p2), last_seen_ns: 0 },
            vec![PeerCapability::VerifierWilling],
        );
        src.upsert_with_capabilities(
            p3,
            PeerAddr { json_rpc: "127.0.0.1:1003".into(), peer_id: Some(p3), last_seen_ns: 0 },
            vec![PeerCapability::AttestWilling, PeerCapability::VerifierWilling],
        );

        let attest = src.list_by_capability(PeerCapability::AttestWilling).await;
        assert_eq!(attest.len(), 2);

        let verify = src.list_by_capability(PeerCapability::VerifierWilling).await;
        assert_eq!(verify.len(), 2);

        let mirror = src.list_by_capability(PeerCapability::MirrorWilling).await;
        assert!(mirror.is_empty());
    }

    #[tokio::test]
    async fn remove_clears_capability_index() {
        let src = DhtPeerSource::default();
        let p1 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();
        let p2 = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        src.upsert_with_capabilities(
            p1,
            PeerAddr { json_rpc: "127.0.0.1:5001".into(), peer_id: Some(p1), last_seen_ns: 0 },
            vec![PeerCapability::AttestWilling, PeerCapability::MirrorWilling],
        );
        src.upsert_with_capabilities(
            p2,
            PeerAddr { json_rpc: "127.0.0.1:5002".into(), peer_id: Some(p2), last_seen_ns: 0 },
            vec![PeerCapability::AttestWilling],
        );

        src.remove(&p1);

        let attest = src.list_by_capability(PeerCapability::AttestWilling).await;
        assert_eq!(attest.len(), 1);
        assert_eq!(attest[0].json_rpc, "127.0.0.1:5002");

        let mirror = src.list_by_capability(PeerCapability::MirrorWilling).await;
        assert!(mirror.is_empty());

        let all = src.list().await;
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn upsert_backward_compat_defaults_to_attest_willing() {
        let src = DhtPeerSource::default();
        let pid = libp2p::identity::Keypair::generate_ed25519().public().to_peer_id();

        src.upsert(pid, make_addr(pid));

        let attest = src.list_by_capability(PeerCapability::AttestWilling).await;
        assert_eq!(attest.len(), 1);

        let mirror = src.list_by_capability(PeerCapability::MirrorWilling).await;
        assert!(mirror.is_empty());

        let verify = src.list_by_capability(PeerCapability::VerifierWilling).await;
        assert!(verify.is_empty());
    }
}
