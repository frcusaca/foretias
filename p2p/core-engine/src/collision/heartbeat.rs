//! Signed heartbeat messages for identity collision detection.

use serde::{Deserialize, Serialize};

use crate::foretias::encoding::{FTByteArray, FTByteVector};

/// A signed heartbeat broadcast by each node on a regular interval.
/// Carries a random nonce so replays are detectable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    pub(crate) peer_id: String,
    pub(crate) timestamp_ns: u64,
    pub(crate) nonce: FTByteArray<16>,
    pub(crate) curve: u8,
    pub(crate) signature: FTByteVector,
}

impl Heartbeat {
    pub fn new(
        peer_id: String,
        timestamp_ns: u64,
        nonce: FTByteArray<16>,
        curve: u8,
        signature: FTByteVector,
    ) -> Self {
        Self {
            peer_id,
            timestamp_ns,
            nonce,
            curve,
            signature,
        }
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }
    pub fn timestamp_ns(&self) -> &u64 {
        &self.timestamp_ns
    }
    pub fn nonce(&self) -> &FTByteArray<16> {
        &self.nonce
    }
    pub fn curve(&self) -> &u8 {
        &self.curve
    }
    pub fn signature(&self) -> &FTByteVector {
        &self.signature
    }
    pub fn signature_mut(&mut self) -> &mut FTByteVector {
        &mut self.signature
    }

    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Length-prefixed peer_id to prevent canonicalization collision attacks
        let pid = self.peer_id.as_bytes();
        buf.extend_from_slice(&(pid.len() as u16).to_le_bytes());
        buf.extend_from_slice(pid);
        buf.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        buf.extend_from_slice(&self.nonce[..]);
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
            nonce: [0xAB; 16].into(),
            curve: 1,
            signature: vec![0xFF; 64].into(),
        };
        let canon = hb.canonical();
        // len2+peer_id + timestamp(8) + nonce(16) + curve(1)
        assert_eq!(canon.len(), 2 + "test-peer".len() + 8 + 16 + 1);
        assert_eq!(&canon[0..2], &("test-peer".len() as u16).to_le_bytes());
        assert_eq!(&canon[2..11], b"test-peer");
        assert_eq!(&canon[11..19], &1234567890u64.to_le_bytes());
        assert_eq!(&canon[19..35], &[0xAB; 16]);
        assert_eq!(canon[35], 1);
    }

    #[test]
    fn heartbeat_serialization_roundtrip() {
        let hb = Heartbeat {
            peer_id: "peer-abc".to_string(),
            timestamp_ns: 999,
            nonce: [0x42; 16].into(),
            curve: 2,
            signature: vec![0x11; 64].into(),
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
