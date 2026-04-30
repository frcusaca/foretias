use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};

use fortias_core::chronomatter::Chronomatter;
use fortias_core::fortias::callbacks::TickObserver;
use fortias_core::fortias::types::{PublicKey, TickNumber};
use fortias_core::error::NodeError;

use self::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

/// No-op observer for the server layer — Calendar integration happens in Phase 4.
struct NoOpObserver;
impl TickObserver for NoOpObserver {
    fn on_tick_advance(&self, _tick_number: TickNumber, _public_key: &PublicKey) {}
}

pub mod jsonrpc;
pub mod handlers;

pub struct TimeFamilyServer {
    chronomatter: Arc<Chronomatter>,
    listen_addr: String,
    persist_path: Option<std::path::PathBuf>,
}

impl TimeFamilyServer {
    pub fn new(listen_addr: &str, chronon_ns: u64) -> Result<Self, NodeError> {
        let observer = Arc::new(NoOpObserver);
        let chronomatter = Arc::new(Chronomatter::new(chronon_ns, observer)?);
        Ok(Self {
            chronomatter,
            listen_addr: listen_addr.to_string(),
            persist_path: None,
        })
    }

    pub fn new_with_persist(
        listen_addr: &str,
        chronon_ns: u64,
        persist_path: Option<std::path::PathBuf>,
    ) -> Result<Self, NodeError> {
        let observer = Arc::new(NoOpObserver);
        let chronomatter = Arc::new(Chronomatter::new(chronon_ns, observer)?);
        Ok(Self {
            chronomatter,
            listen_addr: listen_addr.to_string(),
            persist_path,
        })
    }

    pub fn from_calendar(
        path: &str,
        listen_addr: &str,
    ) -> Result<Self, NodeError> {
        use std::sync::Arc as StdArc;
        use fortias_core::crypto_server;
        let crypto = StdArc::from(crypto_server::new_software(
            crypto_server::FortiasCurve::Ed25519,
        )?);
        let observer = Arc::new(NoOpObserver);
        let chronomatter = Arc::new(Chronomatter::from_calendar(path, crypto, observer)?);
        Ok(Self {
            chronomatter,
            listen_addr: listen_addr.to_string(),
            persist_path: None,
        })
    }

    pub fn get_tbid(&self) -> [u8; 16] {
        self.chronomatter.get_tbid()
    }

    pub fn get_tbn(&self) -> &str {
        self.chronomatter.get_tbn()
    }

    pub fn is_dormant(&self) -> bool {
        self.chronomatter.is_dormant()
    }

    pub fn chronomatter(&self) -> &Arc<Chronomatter> {
        &self.chronomatter
    }

    pub fn current_tick(&self) -> u64 {
        self.chronomatter.current_tick()
    }

    pub fn calendar_read(&self) -> parking_lot::RwLockReadGuard<'_, fortias_core::fortias::Calendar> {
        self.chronomatter.calendar().read()
    }

    pub fn integrity_check(&self, start: Option<u64>, end: Option<u64>) -> Result<Vec<bool>, NodeError> {
        self.chronomatter.integrity_check(start, end)
    }

    pub fn save(&self) -> Result<(), NodeError> {
        if let Some(ref p) = self.persist_path {
            let json_path = p.join(format!("{}.json", hex::encode(self.get_tbid())));
            std::fs::create_dir_all(p)?;
            self.chronomatter.calendar().read().save(json_path.to_str().unwrap())
        } else {
            Ok(())
        }
    }

    pub fn daemon_tick(&self) -> Result<(), NodeError> {
        self.chronomatter.daemon_tick()
    }

    pub fn start_daemon_arc(self: &Arc<Self>) {
        self.chronomatter.start_daemon();
    }

    pub fn stop_daemon_arc(&self) {
        self.chronomatter.stop_daemon();
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

const MAX_REQUEST_LINE_BYTES: usize = 4096;
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
        "integrity_check" => Ok(handlers::handle_integrity_check(server, request.params)),
        _ => Ok(jsonrpc::JsonRpcResponse::error(
            request.id.clone(),
            jsonrpc::METHOD_NOT_FOUND,
            format!("method '{}' not found", request.method),
        )),
    }
}
