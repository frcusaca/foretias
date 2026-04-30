//! JSON-RPC over TCP transport implementation.
//!
//! Uses the same newline-delimited framing as the TimeFamilyServer.

use async_trait::async_trait;
use fortias_core::fortias::TickRecord;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
        let request_str = format!("{}\n", serde_json::to_string(&request)
            .map_err(|e| TransportError::Decode(e.to_string()))?);

        let timeout = Duration::from_secs(self.timeout_secs);
        let result = tokio::time::timeout(timeout, async {
            let stream = tokio::net::TcpStream::connect(&peer.json_rpc)
                .await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let (reader, mut writer) = stream.into_split();
            writer.write_all(request_str.as_bytes()).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;
            writer.flush().await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let mut reader = BufReader::new(reader);
            let mut response_line = String::new();
            reader.read_line(&mut response_line).await
                .map_err(|e| TransportError::Connect(e.to_string()))?;

            let response: serde_json::Value =
                serde_json::from_str(&response_line)
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
