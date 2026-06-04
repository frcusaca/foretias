//! libp2p transport identity module.
//!
//! Generates a libp2p Ed25519 keypair **independent** of the Foretias TBID.
//! This keypair is used for:
//! - Transport-level Noise XX handshake
//! - DHT routing (v0.4)
//! - Peer identification
//! - GossipSub message signing (v0.6)
//!
//! The TBID (application identity) is separate and managed by the C11 enclave.

pub mod behaviour;
pub mod events;
pub mod gossip;
pub mod rpc_protocol;
pub mod swarm;
pub mod tbid_handshake;

use libp2p::identity::{Keypair, PublicKey};

/// Generate a libp2p keypair independently of Foretias crypto.
///
/// This keypair is for transport identity (PeerId) only.
/// It has NO relationship to the Foretias TBID.
pub fn generate_transport_keypair() -> Keypair {
    Keypair::generate_ed25519()
}

/// Derive a PeerId from a public key.
pub fn public_key_to_peer_id(public_key: &PublicKey) -> libp2p::PeerId {
    public_key.to_peer_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libp2p_keypair_generates_valid_peer_id() {
        let keypair = generate_transport_keypair();
        let peer_id = public_key_to_peer_id(&keypair.public());
        assert!(!peer_id.to_string().is_empty());
    }

    #[test]
    fn two_keypairs_have_different_peer_ids() {
        let kp1 = generate_transport_keypair();
        let kp2 = generate_transport_keypair();
        let id1 = public_key_to_peer_id(&kp1.public());
        let id2 = public_key_to_peer_id(&kp2.public());
        assert_ne!(id1, id2);
    }

    #[test]
    fn same_keypair_produces_same_peer_id() {
        let kp = generate_transport_keypair();
        let id1 = public_key_to_peer_id(&kp.public());
        let id2 = public_key_to_peer_id(&kp.public());
        assert_eq!(id1, id2);
    }
}
