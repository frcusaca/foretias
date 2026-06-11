//! TBID proof handshake protocol.
//!
//! After the Noise handshake establishes encryption, each side proves its
//! Foretias TBID by signing a challenge with the C11 enclave's `sign()` method.

use foretias_core::clock::{Clock, SystemClock};
use foretias_core::crypto_server::CryptoServer;
use foretias_core::error::NodeError;
use foretias_core::foretias::types::{PublicKeyBytes, Tbid};
use libp2p::PeerId;
use rand::Rng;
use std::sync::Arc;

/// Request sent to prove identity after connection establishment.
#[derive(Debug, Clone)]
pub struct TbidProofRequest {
    /// Fresh 32-byte nonce to prevent replay attacks.
    pub nonce: [u8; 32],
}

/// Response containing the TBID proof.
#[derive(Debug, Clone)]
pub struct TbidProofResponse {
    /// The 16-byte TBID being proven.
    pub tbid: Tbid,
    /// The Ed25519 public key (32 bytes) used for verification.
    pub public_key: PublicKeyBytes,
    /// The signed payload: tbid + peer_id + nonce + timestamp.
    pub signed_payload: Vec<u8>,
    /// Ed25519 signature (64 bytes) over the signed payload.
    pub signature: [u8; 64],
}

/// Result of TBID proof verification.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TbidProofResult {
    Success { tbid: Tbid, peer_id: PeerId },
    Failed { reason: String },
}

pub struct TbidHandshake {
    crypto: Arc<dyn CryptoServer>,
    local_tbid: Tbid,
    clock: Arc<dyn Clock>,
}

impl TbidHandshake {
    pub fn new(crypto: Arc<dyn CryptoServer>, local_tbid: Tbid) -> Self {
        Self {
            crypto,
            local_tbid,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(
        crypto: Arc<dyn CryptoServer>,
        local_tbid: Tbid,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            crypto,
            local_tbid,
            clock,
        }
    }

    pub fn generate_request(&self) -> TbidProofRequest {
        let mut nonce = [0u8; 32];
        rand::thread_rng().fill(&mut nonce);
        TbidProofRequest { nonce }
    }

    pub async fn create_proof(
        &self,
        request: &TbidProofRequest,
        peer_id: PeerId,
    ) -> Result<TbidProofResponse, NodeError> {
        let tbid = self.local_tbid;
        let public_key: Vec<u8> = match self.crypto.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => return Err(NodeError::Internal("non-Ed25519 public key".into())),
        };

        let timestamp = self.clock.now_ns().unwrap_or(0) / 1_000_000_000;

        let mut signed_payload = Vec::with_capacity(96 + 64 + 32 + 8);
        signed_payload.extend_from_slice(&tbid.raw_bytes());
        signed_payload.extend_from_slice(&peer_id.to_bytes());
        signed_payload.extend_from_slice(&request.nonce);
        signed_payload.extend_from_slice(&timestamp.to_be_bytes());

        let sig = self
            .crypto
            .sign(&signed_payload)
            .map_err(|e| NodeError::Internal(e.to_string()))?;
        let signature: [u8; 64] = sig.bytes;

        Ok(TbidProofResponse {
            tbid,
            public_key: PublicKeyBytes::from(public_key),
            signed_payload,
            signature,
        })
    }

    pub async fn verify_proof(
        &self,
        response: &TbidProofResponse,
        expected_nonce: &[u8; 32],
        peer_id: PeerId,
    ) -> TbidProofResult {
        let public_key: [u8; 32] = match response.public_key.as_slice().try_into() {
            Ok(pk) => pk,
            Err(_) => {
                return TbidProofResult::Failed {
                    reason: "public key is not 32 bytes".into(),
                }
            }
        };
        let valid = self
            .crypto
            .verify_ed25519(
                &foretias_core::core::bindings::ForetiasPubKey32 { bytes: public_key },
                &response.signed_payload,
                &foretias_core::core::bindings::ForetiasSig64 {
                    bytes: response.signature,
                },
            )
            .unwrap_or(false);

        if !valid {
            return TbidProofResult::Failed {
                reason: "signature verification failed".into(),
            };
        }

        let nonce_offset = 96 + peer_id.to_bytes().len();
        // Bounds-check before slicing — a malicious peer can return a too-short
        // signed_payload that was signed legitimately. Without this check, the
        // index into signed_payload[nonce_offset..nonce_offset + 32] panics.
        if response.signed_payload.len() < nonce_offset + 32 {
            return TbidProofResult::Failed {
                reason: "malformed signed_payload (too short for nonce)".into(),
            };
        }
        let payload_nonce: [u8; 32] =
            match response.signed_payload[nonce_offset..nonce_offset + 32].try_into() {
                Ok(n) => n,
                Err(_) => {
                    return TbidProofResult::Failed {
                        reason: "malformed signed_payload nonce slice".into(),
                    }
                }
            };
        if payload_nonce != *expected_nonce {
            return TbidProofResult::Failed {
                reason: "nonce mismatch".into(),
            };
        }

        TbidProofResult::Success {
            tbid: response.tbid,
            peer_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server::{new_software, ForetiasCurve};

    fn test_crypto() -> Arc<dyn CryptoServer> {
        Arc::from(new_software(ForetiasCurve::Ed25519).expect("create software crypto"))
    }

    fn test_handshake() -> (TbidHandshake, Tbid) {
        let crypto = test_crypto();
        let tbid = Tbid::from_raw([0xAB; 96]);
        (TbidHandshake::new(crypto, tbid), tbid)
    }

    #[tokio::test]
    async fn tbid_proof_create_and_verify() {
        let (h1, _tbid) = test_handshake();
        let crypto2 = test_crypto();
        let h2 = TbidHandshake::new(crypto2, Tbid::from_raw([0xCD; 96]));

        let request = h1.generate_request();
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();

        let response = h2.create_proof(&request, peer_id).await.unwrap();
        let result = h1.verify_proof(&response, &request.nonce, peer_id).await;

        assert!(matches!(result, TbidProofResult::Success { .. }));
    }

    #[tokio::test]
    async fn tbid_proof_rejects_wrong_nonce() {
        let (h1, _tbid) = test_handshake();
        let crypto2 = test_crypto();
        let h2 = TbidHandshake::new(crypto2, Tbid::from_raw([0xCD; 96]));

        let request = h1.generate_request();
        let peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();

        let response = h2.create_proof(&request, peer_id).await.unwrap();
        let wrong_nonce = [0xFF; 32];
        let result = h1.verify_proof(&response, &wrong_nonce, peer_id).await;

        assert!(matches!(result, TbidProofResult::Failed { .. }));
    }

    #[tokio::test]
    async fn tbid_proof_rejects_tampered_payload() {
        let (h1, _tbid) = test_handshake();
        let crypto2 = test_crypto();
        let h2 = TbidHandshake::new(crypto2, Tbid::from_raw([0xCD; 96]));

        let request = h1.generate_request();
        let peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();

        let mut response = h2.create_proof(&request, peer_id).await.unwrap();
        response.signed_payload[0] ^= 0xFF;

        let result = h1.verify_proof(&response, &request.nonce, peer_id).await;
        assert!(matches!(result, TbidProofResult::Failed { .. }));
    }

    /// g3-e regression: a malicious peer can supply a too-short signed_payload
    /// that is legitimately signed. Before the bounds-check fix, indexing into
    /// `signed_payload[nonce_offset..nonce_offset + 32]` panicked. After the fix,
    /// verify_proof returns a `Failed` result with a malformed-payload reason.
    #[tokio::test]
    async fn tbid_proof_rejects_short_signed_payload_without_panic() {
        let (h1, _tbid) = test_handshake();
        let crypto2 = test_crypto();
        let h2 = TbidHandshake::new(Arc::clone(&crypto2), Tbid::from_raw([0xCD; 96]));

        let request = h1.generate_request();
        let peer_id = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();

        // Build a syntactically signed but semantically too-short payload.
        // signed_payload is 50 bytes; nonce_offset alone (96 + peer_id len) exceeds it.
        let short_payload = vec![0u8; 50];
        let sig = crypto2.sign(&short_payload).expect("sign short payload");

        let pk_bytes = match crypto2.public_key() {
            foretias_core::crypto_server::PublicKeyBytes::Ed25519(pk) => pk.bytes.to_vec(),
            _ => panic!("expected Ed25519 public key from software crypto"),
        };

        let response = TbidProofResponse {
            tbid: Tbid::from_raw([0xCD; 96]),
            public_key: PublicKeyBytes::from(pk_bytes),
            signed_payload: short_payload,
            signature: sig.bytes,
        };

        // The signature verifies (it's a legitimate signature over 50 bytes),
        // but the subsequent nonce-offset index must NOT panic.
        let result = h2.verify_proof(&response, &request.nonce, peer_id).await;
        match result {
            TbidProofResult::Failed { reason } => {
                assert!(
                    reason.contains("malformed"),
                    "expected malformed-payload reason, got: {reason}"
                );
            }
            TbidProofResult::Success { .. } => panic!("short payload must not succeed"),
        }
    }
}
