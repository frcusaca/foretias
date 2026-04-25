use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};

use crate::crypto_server::{self, CryptoServer};
use crate::error::NodeError;
use crate::fortias::Calendar;

use self::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

pub mod jsonrpc;
pub mod handlers;

pub struct TimeFamilyServer {
    tbid: [u8; 16],
    tbn: String,
    server: Box<dyn CryptoServer>,
    calendar: parking_lot::RwLock<Calendar>,
    current_tick: parking_lot::Mutex<u64>,
    chronon_ns: u64,
    listen_addr: String,
}

impl TimeFamilyServer {
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

async fn handle_connection(
    server: Arc<TimeFamilyServer>,
    stream: tokio::net::TcpStream,
) -> Result<(), NodeError> {
    let (reader, writer) = stream.into_split();
    let reader = BufReader::new(reader);
    let mut lines = reader.lines();
    let mut writer = writer;

    loop {
        let line = match lines.next_line().await? {
            Some(l) => l,
            None => break,
        };

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
