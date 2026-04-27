use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};

use crate::crypto_server::{self, CryptoServer};
use crate::error::NodeError;
use crate::fortias::Calendar;

use self::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

/// JSON-RPC 2.0 protocol types and error codes.
pub mod jsonrpc;
/// Request handlers for stamp, verify, and calendar queries.
pub mod handlers;

/// JSON-RPC 2.0 TCP server that handles stamp/verify/calendar requests for a TimeFamily.
pub struct TimeFamilyServer {
    /// TimeBeing identifier of this server's node.
    tbid: [u8; 16],
    /// TimeBeing name (human-readable identifier).
    tbn: String,
    /// The cryptographic backend used for signing and verification.
    server: Box<dyn CryptoServer>,
    /// The append-only calendar of tick records.
    calendar: parking_lot::RwLock<Calendar>,
    /// Monotonically increasing tick counter.
    current_tick: parking_lot::Mutex<u64>,
    /// Chronon interval in nanoseconds, defining the tick period.
    chronon_ns: u64,
    /// Network address this server listens on.
    listen_addr: String,
}

impl TimeFamilyServer {
    /// Creates a new server with a fresh software crypto backend and empty calendar.
    pub fn new(listen_addr: &str, chronon_ns: u64) -> Result<Self, NodeError> {
        let crypto = crypto_server::new_software(
            crypto_server::FortiasCurve::Ed25519,
        )?;

        let uuid = uuid::Uuid::new_v4();
        let tbid = *uuid.as_bytes();

        let tbn = format!("tf-{}", hex::encode(&tbid[..8]));
        let calendar = Calendar::new(tbid, &tbn);

        Ok(Self {
            tbid,
            tbn,
            server: crypto,
            calendar: parking_lot::RwLock::new(calendar),
            current_tick: parking_lot::Mutex::new(0),
            chronon_ns,
            listen_addr: listen_addr.to_string(),
        })
    }

    /// Returns the TimeBeing identifier of this server.
    pub fn get_tbid(&self) -> [u8; 16] {
        self.tbid
    }

    /// Returns the TimeBeing name of this server.
    pub fn get_tbn(&self) -> &str {
        &self.tbn
    }

    /// Starts the TCP listener and spawns an async task to accept connections.
    pub fn start(self: Arc<Self>) -> Result<tokio::task::JoinHandle<()>, NodeError> {
        let addr = self.listen_addr.clone();

        Ok(tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("failed to bind to {}: {}", addr, e);
                    return;
                }
            };
            tracing::info!("TimeFamilyServer listening on {}", addr);
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        let server = Arc::clone(&self);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(server, stream).await {
                                tracing::error!("connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("accept error: {}", e);
                    }
                }
            }
        }))
    }
}

/// Maximum size of a single JSON-RPC request line (4 KB).
const MAX_REQUEST_LINE_BYTES: usize = 4096;
/// Timeout for reading each request line (30 seconds).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

async fn handle_connection(
    server: Arc<TimeFamilyServer>,
    stream: tokio::net::TcpStream,
) -> Result<(), NodeError> {
    let (reader, writer) = stream.into_split();
    let reader = BufReader::new(reader);
    let mut lines = reader.lines();
    let mut writer = writer;

    loop {
        let line = match tokio::time::timeout(REQUEST_TIMEOUT, lines.next_line()).await {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                tracing::warn!("read error: {}", e);
                return Err(NodeError::from(e));
            }
            Err(_) => {
                tracing::warn!("request timeout");
                break;
            }
        };

        let line = match line {
            Some(l) => l,
            None => break,
        };

        if line.len() > MAX_REQUEST_LINE_BYTES {
            tracing::warn!("request line exceeds {} bytes, dropping connection", MAX_REQUEST_LINE_BYTES);
            break;
        }

        let response = process_request(&server, &line)?;
        let response_line = serde_json::to_string(&response)?;

        writer.write_all(response_line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

fn process_request(server: &TimeFamilyServer, line: &str) -> Result<JsonRpcResponse, NodeError> {
    let request: JsonRpcRequest = serde_json::from_str(line)
        .map_err(|e| NodeError::Internal(format!("JSON parse: {}", e)))?;

    if request.jsonrpc != "2.0" {
        return Ok(jsonrpc::JsonRpcResponse::error(
            request.id.clone(),
            jsonrpc::INVALID_REQUEST,
            "invalid jsonrpc version",
        ));
    }

    match request.method.as_str() {
        "stamp" => Ok(handlers::handle_stamp(server, request.params)),
        "verify" => Ok(handlers::handle_verify(server, request.params)),
        "get_calendar_slice" => Ok(handlers::handle_get_calendar_slice(server, request.params)),
        _ => Ok(jsonrpc::JsonRpcResponse::error(
            request.id.clone(),
            jsonrpc::METHOD_NOT_FOUND,
            format!("method '{}' not found", request.method),
        )),
    }
}
