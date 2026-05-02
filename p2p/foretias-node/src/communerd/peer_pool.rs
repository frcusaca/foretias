//! PeerPool — tracks known peers and liveness.
//!
//! Maintains the peer list, performs periodic health checks, and provides
//! the ability to add/remove peers dynamically.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::warn;

use super::transport::{PeerAddr, PeerTransport};

/// PeerPool manages the set of known peers and their liveness status.
pub struct PeerPool {
    peers: Arc<RwLock<Vec<PeerAddr>>>,
    transport: Arc<dyn PeerTransport>,
    ping_interval_secs: u64,
}

impl Clone for PeerPool {
    fn clone(&self) -> Self {
        Self {
            peers: Arc::clone(&self.peers),
            transport: Arc::clone(&self.transport),
            ping_interval_secs: self.ping_interval_secs,
        }
    }
}

impl PeerPool {
    pub fn new(
        peers: Vec<PeerAddr>,
        transport: Arc<dyn PeerTransport>,
        ping_interval_secs: u64,
    ) -> Self {
        Self {
            peers: Arc::new(RwLock::new(peers)),
            transport,
            ping_interval_secs,
        }
    }

    pub async fn add_peer(&self, addr: PeerAddr) {
        let mut peers = self.peers.write().await;
        if !peers.iter().any(|p| p.json_rpc == addr.json_rpc) {
            peers.push(addr);
        }
    }

    pub async fn remove_peer(&self, addr: &PeerAddr) {
        let mut peers = self.peers.write().await;
        peers.retain(|p| p.json_rpc != addr.json_rpc);
    }

    pub async fn get_peers(&self) -> Vec<PeerAddr> {
        self.peers.read().await.clone()
    }

    /// Start background liveness ping loop.
    pub async fn start_liveness_pings(self) {
        if self.peers.read().await.is_empty() {
            warn!("no peers configured, liveness pings disabled");
            return;
        }

        let mut interval = tokio::time::interval(Duration::from_secs(
            self.ping_interval_secs.max(1),
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let peers = self.get_peers().await;
            for peer in &peers {
                if let Err(e) = self.transport.ping(peer).await {
                    warn!("liveness ping failed for {}: {}", peer, e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct DummyTransport;

    #[async_trait]
    impl PeerTransport for DummyTransport {
        async fn stamp(
            &self,
            _peer: &PeerAddr,
            _content_hex: &str,
            _echo: &str,
        ) -> Result<serde_json::Value, crate::communerd::transport::TransportError> {
            Ok(serde_json::json!({}))
        }

        async fn get_calendar_slice(
            &self,
            _peer: &PeerAddr,
            _tick_start: u64,
            _count: u64,
        ) -> Result<Vec<foretias_core::foretias::TickRecord>, crate::communerd::transport::TransportError> {
            Ok(Vec::new())
        }

        async fn ping(&self, _peer: &PeerAddr) -> Result<(), crate::communerd::transport::TransportError> {
            Ok(())
        }
    }

    #[test]
    fn peer_pool_add_and_get() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = PeerPool::new(
                Vec::new(),
                Arc::new(DummyTransport),
                30,
            );
            pool.add_peer(PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: None, last_seen_ns: 0 }).await;
            let peers = pool.get_peers().await;
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0].json_rpc, "127.0.0.1:4001");
        });
    }

    #[test]
    fn peer_pool_remove() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = PeerPool::new(
                vec![PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: None, last_seen_ns: 0 }],
                Arc::new(DummyTransport),
                30,
            );
            pool.remove_peer(&PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: None, last_seen_ns: 0 }).await;
            let peers = pool.get_peers().await;
            assert!(peers.is_empty());
        });
    }

    #[test]
    fn peer_pool_add_duplicate_noop() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = PeerPool::new(
                vec![PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: None, last_seen_ns: 0 }],
                Arc::new(DummyTransport),
                30,
            );
            pool.add_peer(PeerAddr { json_rpc: "127.0.0.1:4001".into(), peer_id: None, last_seen_ns: 0 }).await;
            assert_eq!(pool.get_peers().await.len(), 1);
        });
    }
}
