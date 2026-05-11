//! Collision detection — watches heartbeat stream for identity collisions.

use std::collections::VecDeque;

use crate::core::bindings::ForetiasPubKey32;
use crate::crypto_server::CryptoServer;
use crate::collision::heartbeat::Heartbeat;

/// Event emitted when a collision is confirmed.
#[derive(Debug, Clone)]
pub enum CollisionEvent {
    Confirmed { foreign_heartbeat: Heartbeat },
}

/// Watches the heartbeat stream for messages that claim our own peer_id
/// with a valid signature but an unknown nonce — cryptographic proof of collision.
pub struct CollisionDetector {
    my_peer_id: String,
    my_pub_key: ForetiasPubKey32,
    my_nonces:  parking_lot::Mutex<VecDeque<[u8; 16]>>,
    nonce_window: usize,
}

impl CollisionDetector {
    pub fn new(my_peer_id: String, my_pub_key: ForetiasPubKey32, nonce_window: usize) -> Self {
        Self {
            my_peer_id,
            my_pub_key,
            my_nonces: parking_lot::Mutex::new(VecDeque::new()),
            nonce_window,
        }
    }

    pub fn register_own_nonce(&self, nonce: [u8; 16]) {
        let mut guard = self.my_nonces.lock();
        if guard.len() >= self.nonce_window {
            guard.pop_front();
        }
        guard.push_back(nonce);
    }

    pub fn on_heartbeat(
        &self,
        hb: &Heartbeat,
        crypto: &dyn CryptoServer,
    ) -> Option<CollisionEvent> {
        if hb.peer_id != self.my_peer_id {
            return None;
        }

        let is_known_nonce = self.my_nonces.lock().iter().any(|n| *n == *hb.nonce);
        if is_known_nonce {
            return None;
        }

        let sig_bytes: [u8; 64] = hb.signature.get(..64)?.try_into().ok()?;
        let sig = crate::core::bindings::ForetiasSig64 { bytes: sig_bytes };
        let valid = crypto.verify_ed25519(&self.my_pub_key, &hb.canonical(), &sig).ok()?;
        if !valid {
            return None;
        }

        Some(CollisionEvent::Confirmed { foreign_heartbeat: hb.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use crate::crypto_server::ForetiasCurve;
    use crate::core::identity::generate_ed25519_keypair;

    fn make_server() -> Box<dyn CryptoServer> {
        crypto_server::new_software(ForetiasCurve::Ed25519).unwrap()
    }

    fn build_heartbeat(peer_id: &str, nonce: [u8; 16], server: &dyn CryptoServer) -> Heartbeat {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let mut hb = Heartbeat {
            peer_id: peer_id.to_string(),
            timestamp_ns,
            nonce: nonce.into(),
            curve: 1,
            signature: vec![].into(),
        };
        let sig = server.sign(&hb.canonical()).unwrap();
        *hb.signature = sig.bytes.to_vec();
        hb
    }

    #[test]
    fn detector_ignores_different_peer() {
        let server = make_server();
        let (_, _priv_key) = generate_ed25519_keypair().unwrap();
        let pub_key = crate::core::bindings::ForetiasPubKey32 { bytes: [0xAA; 32] };
        let detector = CollisionDetector::new("my-peer".to_string(), pub_key, 10);

        let mut nonce = [0u8; 16];
        crate::core::rng::random_bytes(&mut nonce).unwrap();
        let hb = build_heartbeat("other-peer", nonce, server.as_ref());

        assert!(detector.on_heartbeat(&hb, server.as_ref()).is_none());
    }

    #[test]
    fn detector_ignores_own_echo() {
        let server = make_server();
        let (pub_key, _) = generate_ed25519_keypair().unwrap();
        let detector = CollisionDetector::new("my-peer".to_string(), pub_key, 10);

        let mut nonce = [0u8; 16];
        crate::core::rng::random_bytes(&mut nonce).unwrap();
        detector.register_own_nonce(nonce);

        let mut hb = build_heartbeat("my-peer", nonce, server.as_ref());
        hb.peer_id = "my-peer".to_string();

        assert!(detector.on_heartbeat(&hb, server.as_ref()).is_none());
    }

    #[test]
    fn detector_ignores_invalid_signature() {
        let server = make_server();
        let pub_key = crate::core::bindings::ForetiasPubKey32 { bytes: [0xBB; 32] };
        let detector = CollisionDetector::new("my-peer".to_string(), pub_key, 10);

        let mut nonce = [0u8; 16];
        crate::core::rng::random_bytes(&mut nonce).unwrap();
        let hb = build_heartbeat("my-peer", nonce, server.as_ref());

        assert!(detector.on_heartbeat(&hb, server.as_ref()).is_none());
    }

    #[test]
    fn detector_confirms_collision() {
        let server = make_server();
        let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
        let detector = CollisionDetector::new("my-peer".to_string(), pub_key, 10);

        let mut nonce = [0u8; 16];
        crate::core::rng::random_bytes(&mut nonce).unwrap();

        let mut hb = Heartbeat {
            peer_id: "my-peer".to_string(),
            timestamp_ns: 12345,
            nonce: nonce.into(),
            curve: 1,
            signature: vec![].into(),
        };
        let _sig_bytes: [u8; 32] = priv_key.bytes;
        let sig = crate::core::signing::ed25519_sign(
            &crate::core::bindings::ForetiasPrivKey32 { bytes: priv_key.bytes },
            &hb.canonical(),
        ).unwrap();
        *hb.signature = sig.bytes.to_vec();

        let result = detector.on_heartbeat(&hb, server.as_ref());
        assert!(result.is_some());
        if let Some(CollisionEvent::Confirmed { foreign_heartbeat }) = result {
            assert_eq!(foreign_heartbeat.peer_id, "my-peer");
            assert_eq!(*foreign_heartbeat.nonce, nonce);
        }
    }
}
