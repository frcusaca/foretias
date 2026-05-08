//! Peer capabilities for multi-purpose DHT peer discovery.
//!
//! Each capability corresponds to a distinct DHT provider key. Nodes advertise
//! one or more capabilities during DHT self-registration. Discovery queries
//! target a specific capability to find peers willing to perform that function.

use libp2p::kad;
use serde::{Deserialize, Serialize};

/// Capabilities a node can advertise in the DHT for peer discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerCapability {
    /// Willing to cross-attest calendar ticks with other nodes.
    AttestWilling,
    /// Willing to mirror calendar ticks (one-way or mutual replication).
    MirrorWilling,
    /// Offering independent timestamp verification as a service.
    VerifierWilling,
}

impl PeerCapability {
    /// Return the DHT provider key suffix for this capability.
    pub fn provider_key_suffix(&self) -> &'static str {
        match self {
            Self::AttestWilling => "/foretias/attest-willing/v1",
            Self::MirrorWilling => "/foretias/mirror-willing/v1",
            Self::VerifierWilling => "/foretias/verifier-willing/v1",
        }
    }

    /// Return the full DHT provider key for a namespace + capability.
    pub fn provider_key(&self, namespace: &str) -> kad::RecordKey {
        kad::RecordKey::new(&format!("{namespace}{}", self.provider_key_suffix()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_provider_key_deterministic() {
        let key1 = PeerCapability::MirrorWilling.provider_key("mainnet");
        let key2 = PeerCapability::MirrorWilling.provider_key("mainnet");
        assert_eq!(key1.to_vec(), key2.to_vec());
    }

    #[test]
    fn capability_provider_key_distinct() {
        let attest = PeerCapability::AttestWilling.provider_key("mainnet");
        let mirror = PeerCapability::MirrorWilling.provider_key("mainnet");
        let verify = PeerCapability::VerifierWilling.provider_key("mainnet");

        assert_ne!(attest.to_vec(), mirror.to_vec());
        assert_ne!(attest.to_vec(), verify.to_vec());
        assert_ne!(mirror.to_vec(), verify.to_vec());
    }

    #[test]
    fn capability_provider_key_contains_namespace() {
        let key = PeerCapability::MirrorWilling.provider_key("testnet");
        let key_str = String::from_utf8(key.to_vec()).unwrap();
        assert!(key_str.starts_with("testnet"));
        assert!(key_str.contains("/foretias/mirror-willing/v1"));
    }

    #[test]
    fn capability_serialize_roundtrip() {
        let caps = vec![
            PeerCapability::AttestWilling,
            PeerCapability::MirrorWilling,
            PeerCapability::VerifierWilling,
        ];
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: Vec<PeerCapability> = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, decoded);
    }
}
