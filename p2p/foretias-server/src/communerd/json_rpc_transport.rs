//! JSON-RPC over TCP transport with Noise_XX encryption.

use async_trait::async_trait;
use foretias_core::core::identity::generate_ed25519_keypair;
use foretias_core::foretias::ChrononRecord;
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
    async fn route_stamp(
        &self,
        peer: &PeerAddr,
        target_tbid: &str,
        content_hex: &str,
        echo: &str,
    ) -> Result<serde_json::Value, TransportError> {
        let params = serde_json::json!({
            "target_tbid": target_tbid,
            "content": content_hex,
            "echo": echo,
        });
        let result = self.json_rpc_call(peer, "route_stamp", params).await?;
        Ok(result)
    }

    async fn get_calendar_slice(
        &self,
        peer: &PeerAddr,
        tick_start: u64,
        count: u64,
    ) -> Result<Vec<ChrononRecord>, TransportError> {
        let params = serde_json::json!({
            "cal_chronon_start": tick_start,
            "count": count,
        });
        let result = self.json_rpc_call(peer, "get_calendar_slice", params).await?;
        let records: Vec<ChrononRecord> =
            serde_json::from_value(result).map_err(|e| TransportError::Decode(e.to_string()))?;
        Ok(records)
    }

    async fn ping(&self, peer: &PeerAddr) -> Result<(), TransportError> {
        let _ = self.json_rpc_call(peer, "ping", serde_json::json!({})).await?;
        Ok(())
    }

    async fn channel_bind_challenge(
        &self,
        peer: &PeerAddr,
        nonce_hex: &str,
        channel_id: &str,
        requester_tbid_hex: &str,
    ) -> Result<serde_json::Value, TransportError> {
        self.json_rpc_call(peer, "channel_bind_challenge", serde_json::json!({
            "nonce": nonce_hex,
            "channel_id": channel_id,
            "requester_tbid": requester_tbid_hex,
        })).await
    }
}

impl JsonRpcTransport {
    /// Generic JSON-RPC dispatch over Noise_XX TCP. Made `pub(crate)` in
    /// Phase 4b.4 so MirrorDispatcher can use it for the new mirror RPC
    /// methods that don't fit the legacy stamp/route/slice/ping shape.
    pub(crate) async fn json_rpc_call(
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

        let peer_addr = peer.json_rpc.clone();
        let timeout_secs = self.timeout_secs;

        // INVARIANT: NoiseSession is !Send (mutable nonce counters that must not be
        // accessed from multiple threads). We run the entire Noise session lifecycle
        // on a dedicated blocking thread with its own current_thread runtime so the
        // session is never present in a Send future.
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let timeout = Duration::from_secs(timeout_secs);
            rt.block_on(async move {
                let result = tokio::time::timeout(timeout, async move {
                    let stream = tokio::net::TcpStream::connect(&peer_addr)
                        .await
                        .map_err(|e| TransportError::Connect(e.to_string()))?;

                    let (_pub_key, priv_key) = generate_ed25519_keypair()
                        .map_err(|e| TransportError::Connect(e.to_string()))?;
                    let (mut session, stream) = noise::noise_handshake(stream, &priv_key.bytes, None, true).await
                        .map_err(|e| TransportError::Connect(e.to_string()))?;

                    let (reader, mut writer) = stream.into_split();
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
            })
        })
        .await
        .map_err(|e| TransportError::Connect(format!("worker thread panic: {e}")))?
    }
}
