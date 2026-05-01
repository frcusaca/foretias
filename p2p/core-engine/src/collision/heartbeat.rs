//! Signed heartbeat messages for identity collision detection.

use serde::{Deserialize, Serialize};

/// A signed heartbeat broadcast by each node on a regular interval.
/// Carries a random nonce so replays are detectable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub peer_id:      String,
    pub timestamp_ns: u64,
    pub nonce:        [u8; 16],
    pub curve:        u8,
    pub signature:    Vec<u8>,
}

impl Heartbeat {
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.peer_id.as_bytes());
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(&self.nonce);
        buf.push(self.curve);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_canonical_roundtrip() {
        let hb = Heartbeat {
            peer_id: "test-peer".to_string(),
            timestamp_ns: 1234567890,
            nonce: [0xAB; 16],
            curve: 1,
            signature: vec![0xFF; 64],
        };
        let canon = hb.canonical();
        assert_eq!(canon.len(), "test-peer".len() + 8 + 16 + 1);
        assert_eq!(&canon[0..9], b"test-peer");
        assert_eq!(&canon[9..17], &1234567890u64.to_le_bytes());
        assert_eq!(&canon[17..33], &[0xAB; 16]);
        assert_eq!(canon[33], 1);
    }

    #[test]
    fn heartbeat_serialization_roundtrip() {
        let hb = Heartbeat {
            peer_id: "peer-abc".to_string(),
            timestamp_ns: 999,
            nonce: [0x42; 16],
            curve: 2,
            signature: vec![0x11; 64],
        };
        let json = serde_json::to_string(&hb).unwrap();
        let parsed: Heartbeat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.peer_id, hb.peer_id);
        assert_eq!(parsed.timestamp_ns, hb.timestamp_ns);
        assert_eq!(parsed.nonce, hb.nonce);
        assert_eq!(parsed.curve, hb.curve);
        assert_eq!(parsed.signature, hb.signature);
    }
}
