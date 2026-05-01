use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::post;
use axum::Json;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};

use fortias_core::chronomatter::Chronomatter;
use fortias_core::config::NodeConfig;
use fortias_core::fortias::callbacks::{TickObserver, AutoAttestObserver};
use fortias_core::fortias::TickRecord;
use fortias_core::error::NodeError;

use super::calendar::Calendar;
use super::communerd::Communerd;
use super::metrics::{NodeMetrics, MetricField};
use self::jsonrpc::JsonRpcResponse;

struct NoOpObserver;
impl TickObserver for NoOpObserver {
    fn on_tick_advance(&self, _tick_number: u64, _public_key: &[u8; 32], _tick_record: &TickRecord) {}
}

pub mod jsonrpc;
pub mod handlers;

pub struct TimeFamilyServer {
    chronomatter: Arc<Chronomatter>,
    calendar: Arc<Calendar>,
    communerd: Option<Arc<Communerd>>,
    listen_addr: String,
    persist_path: Option<std::path::PathBuf>,
    metrics: Arc<NodeMetrics>,
}

impl TimeFamilyServer {
    pub fn new(listen_addr: &str, chronon_ns: u64) -> Result<Self, NodeError> {
        Self::new_with_config(listen_addr, chronon_ns, None, None)
    }

    pub fn new_with_persist(
        listen_addr: &str,
        chronon_ns: u64,
        persist_path: Option<std::path::PathBuf>,
    ) -> Result<Self, NodeError> {
        Self::new_with_config(listen_addr, chronon_ns, persist_path, None)
    }

    fn new_with_config(
        listen_addr: &str,
        chronon_ns: u64,
        persist_path: Option<std::path::PathBuf>,
        config: Option<NodeConfig>,
    ) -> Result<Self, NodeError> {
        let metrics = Arc::new(NodeMetrics::new());
        let calendar = Arc::new(Calendar::new([0u8; 16], "init"));
        let mut cm = Chronomatter::new(chronon_ns, Arc::clone(&calendar) as Arc<dyn TickObserver>)?;
        cm.set_auto_attest_observer(Arc::clone(&metrics) as Arc<dyn AutoAttestObserver>);
        let (tbid, tbn) = (cm.get_tbid(), cm.get_tbn().to_string());
        let binding = calendar.inner();
        let mut cal_inner = binding.write();
        cal_inner.tbid = tbid;
        cal_inner.tbn = tbn.clone();
        let communerd = config.map(|c| {
            Arc::new(Communerd::new(c))
        });
        Ok(Self {
            chronomatter: Arc::new(cm),
            calendar,
            communerd,
            listen_addr: listen_addr.to_string(),
            persist_path,
            metrics,
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
        let mut cm = Chronomatter::from_calendar(path, crypto, Arc::new(NoOpObserver))?;
        let metrics = Arc::new(NodeMetrics::new());
        cm.set_auto_attest_observer(Arc::clone(&metrics) as Arc<dyn AutoAttestObserver>);
        let calendar = Arc::new(Calendar::from_persisted(path)?);
        Ok(Self {
            chronomatter: Arc::new(cm),
            calendar,
            communerd: None,
            listen_addr: listen_addr.to_string(),
            persist_path: None,
            metrics,
        })
    }

    pub fn with_config(mut self, config: NodeConfig) -> Self {
        self.communerd = Some(Arc::new(Communerd::new(config)));
        self
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

    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub fn communerd(&self) -> Option<&Arc<Communerd>> {
        self.communerd.as_ref()
    }

    pub fn metrics(&self) -> &Arc<NodeMetrics> {
        &self.metrics
    }

    pub fn current_tick(&self) -> u64 {
        self.chronomatter.current_tick()
    }

    pub fn integrity_check(&self, start: Option<u64>, end: Option<u64>) -> Result<Vec<bool>, NodeError> {
        self.chronomatter.integrity_check(&*self.calendar, start, end)
    }

    pub fn save(&self) -> Result<(), NodeError> {
        if let Some(ref p) = self.persist_path {
            let json_path = p.join(format!("{}.json", hex::encode(self.get_tbid())));
            std::fs::create_dir_all(p)?;
            self.calendar.save(json_path.to_str().unwrap())?;
            self.metrics.inc(MetricField::CalendarFlushCount);
        }
        Ok(())
    }

    pub fn daemon_tick(&self) -> Result<(), NodeError> {
        self.chronomatter.daemon_tick()
    }

    pub fn start_daemon_arc(self: &Arc<Self>) {
        self.chronomatter.start_daemon();
        if let Some(ref communerd) = self.communerd {
            communerd.start_liveness_pings();
        }
    }

    pub fn stop_daemon_arc(&self) {
        self.chronomatter.stop_daemon();
    }

    pub fn start(self: Arc<Self>) -> Result<tokio::task::JoinHandle<()>, NodeError> {
        Self::start_tcp(self)
    }

    pub fn start_tcp(self: Arc<Self>) -> Result<tokio::task::JoinHandle<()>, NodeError> {
        let addr = self.listen_addr.clone();

        Ok(tokio::spawn(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("failed to bind to {}: {}", addr, e);
                    return;
                }
            };
            tracing::info!("TimeFamilyServer TCP listening on {}", addr);
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

    pub fn start_http(self: Arc<Self>, http_addr: &str) -> Result<tokio::task::JoinHandle<()>, NodeError> {
        let http_addr = http_addr.to_string();
        let app = axum::Router::new()
            .route("/jsonrpc", post(jsonrpc_handler))
            .with_state(Arc::clone(&self));

        Ok(tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&http_addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("failed to bind HTTP to {}: {}", http_addr, e);
                    return;
                }
            };
            tracing::info!("TimeFamilyServer HTTP listening on {}", http_addr);
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("HTTP server error: {}", e);
            }
        }))
    }
}

async fn jsonrpc_handler(
    State(server): State<Arc<TimeFamilyServer>>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let resp = process_request_from_value(&server, req)
        .unwrap_or_else(|e| jsonrpc::JsonRpcResponse::error(
            None,
            jsonrpc::INTERNAL_ERROR,
            e.to_string(),
        ));
    Json(serde_json::to_value(resp).unwrap_or(serde_json::Value::Null))
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

fn process_request_from_value(server: &TimeFamilyServer, req: serde_json::Value) -> Result<JsonRpcResponse, NodeError> {
    let jsonrpc = req.get("jsonrpc")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NodeError::Internal("missing jsonrpc field".into()))?;
    if jsonrpc != "2.0" {
        return Ok(jsonrpc::JsonRpcResponse::error(
            req.get("id").cloned(),
            jsonrpc::INVALID_REQUEST,
            "invalid jsonrpc version",
        ));
    }

    let id = req.get("id").clone();
    let method = req.get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| NodeError::Internal("missing method field".into()))?;
    let params = req.get("params").cloned().unwrap_or(serde_json::Value::Null);

    match method {
        "stamp" => Ok(handlers::handle_stamp(server, params)),
        "verify" => Ok(handlers::handle_verify(server, params)),
        "get_calendar_slice" => Ok(handlers::handle_get_calendar_slice(server, params)),
        "integrity_check" => Ok(handlers::handle_integrity_check(server, params)),
        "get_peer_score" => Ok(handlers::handle_get_peer_score(server, params)),
        "collision_status" => Ok(handlers::handle_collision_status(server, params)),
        "get_latest_epoch" => Ok(handlers::handle_get_latest_epoch(server, params)),
        "verify_epoch_snapshot" => Ok(handlers::handle_verify_epoch_snapshot(server, params)),
        _ => Ok(jsonrpc::JsonRpcResponse::error(
            id.cloned(),
            jsonrpc::METHOD_NOT_FOUND,
            format!("method '{}' not found", method),
        )),
    }
}

fn process_request(server: &TimeFamilyServer, line: &str) -> Result<JsonRpcResponse, NodeError> {
    let req: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| NodeError::Internal(format!("JSON parse: {}", e)))?;
    process_request_from_value(server, req)
}
