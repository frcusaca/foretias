//! Network events from the libp2p swarm.

use super::super::capabilities::PeerCapability;
use libp2p::{identify, kad, Multiaddr, PeerId};

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    Connected {
        peer_id: PeerId,
    },
    Disconnected {
        peer_id: PeerId,
    },
    Identified {
        peer_id: PeerId,
        info: Box<identify::Info>,
    },
    PingSuccess {
        peer_id: PeerId,
        rtt: std::time::Duration,
    },
    DhtPeerDiscovered {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    DhtBootstrapComplete,
    GossipMessage {
        data: Vec<u8>,
        source: PeerId,
    },
    HeartbeatMessage {
        data: Vec<u8>,
        source: PeerId,
    },
    /// Swarm has successfully bound to a listen address
    ListenReady {
        multiaddr: Multiaddr,
    },
    /// DHT record retrieval completed
    RecordRetrieved {
        key: kad::RecordKey,
        records: Vec<kad::Record>,
    },
    /// DHT record put completed successfully
    RecordPutOk {
        key: kad::RecordKey,
    },
    /// DHT record put failed
    RecordPutError {
        key: kad::RecordKey,
        error: String,
    },
    /// Peer discovered via DHT provider query for a specific capability
    DhtPeerDiscoveredByCapability {
        capability: PeerCapability,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },
    /// Peer registration record retrieved from DHT
    PeerRegistrationRetrieved {
        record: crate::communerd::PeerRegistrationRecord,
    },
}
