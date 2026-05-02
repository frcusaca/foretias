//! JSON-RPC over TCP transport with Noise_XX encryption.

use async_trait::async_trait;
use foretias_core::core::identity::generate_ed25519_keypair;
use foretias_core::foretias::TickRecord;
use foretias_core::noise;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

use super::transport::{PeerAddr, PeerTransport, TransportError};

/// JSON-RPC transport over TCP.
pub struct JsonRpcTransport {
    /// Request timeout in seconds.
    timeout_secs: u64,
}

impl JsonRpcTransport {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

#[async_trait]
impl PeerTransport for JsonRpcTransport {
    async fn stamp(
        &self,
        peer: &PeerAddr,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        let params = serde_json::json!({
            "content": content_hex,
            "echo": echo,
        });
        let result = self.json_rpc_call(peer, "stamp", params).await?;
        Ok(result)
    }

    async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<TickRecord>, TransportError> {
        let params = serde_json::json!({
            "cal_tick_start": tick_start,
            "count": count,
        });
        let result = self.json_rpc_call(peer, "get_calendar_slice", params).await?;
        let records: Vec<TickRecord> =
            serde_json::from_value(result).map_err(|e| TransportError::Decode(e.to_string()))?;
        Ok(records)
    }

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError> {
        let _ = self.json_rpc_call(peer, "ping", serde_json::json!({})).await?;
        Ok(())
    }
}

impl JsonRpcTransport {
    async fn json_rpc_call(
        &self,
        peer: &PeerAddr,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, TransportError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let request_bytes = serde_json::to_string(&request)
            .map_err(|e| TransportError::Decode(e.to_string()))?
            .into_bytes();

        let timeout = Duration::from_secs(self.timeout_secs);
        let result = tokio::time::timeout(timeout, async {
            let stream = tokio::net::TcpStream::connect(&peer.json_rpc)
                .await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let (_pub_key, priv_key) = generate_ed25519_keypair()
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            let (mut session, stream) = noise::noise_handshake(stream, &priv_key.bytes, None, true).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let (mut reader, mut writer) = stream.into_split();
            let mut reader = tokio::io::BufReader::new(reader);

            let ct = session.send(&request_bytes)
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            let ct_len = (ct.len() as u32).to_le_bytes();
            writer.write_all(&ct_len).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            writer.write_all(&ct).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            writer.flush().await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let mut len_buf = [0u8; 4];
            tokio::io::AsyncReadExt::read_exact(&mut reader, &mut len_buf).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            let resp_len = u32::from_le_bytes(len_buf) as usize;
            let mut resp_buf = vec![0u8; resp_len];
            tokio::io::AsyncReadExt::read_exact(&mut reader, &mut resp_buf).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let plaintext = session.recv(&resp_buf)
                .map_err(|e| TransportError::Decode(e.to_string()))?;
            let response: serde_json::Value =
                serde_json::from_slice(&plaintext)
                    .map_err(|e| TransportError::Decode(e.to_string()))?;

            if let Some(err) = response.get("error") {
                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
                let message = err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                return Err(TransportError::Rpc { code, message });
            }

            response.get("result")
                .cloned()
                .ok_or_else(|| TransportError::Decode("missing result".to_string()))
        })
        .await;

        match result {
            Ok(inner_result) => inner_result,
            Err(_) => Err(TransportError::Timeout),
        }
    }
}
