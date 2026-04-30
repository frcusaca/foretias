//! Network events from the libp2p swarm.

use libp2p::{identify, Multiaddr, PeerId};

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Connected { peer_id: PeerId },
    Disconnected { peer_id: PeerId },
    Identified { peer_id: PeerId, info: identify::Info },
    PingSuccess { peer_id: PeerId, rtt: std::time::Duration },
    DhtPeerDiscovered { peer_id: PeerId, addresses: Vec<Multiaddr> },
    DhtBootstrapComplete,
}
