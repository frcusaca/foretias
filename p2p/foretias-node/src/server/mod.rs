use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::post;
use axum::Json;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use foretias_core::chronomatter::Chronomatter;
use foretias_core::config::{NodeConfig, TimeFamilyConfig};
use foretias_core::core::identity::{generate_ed25519_keypair, derive_ed25519_peer_id};
use foretias_core::foretias::callbacks::{TickObserver, AutoAttestObserver};
use foretias_core::foretias::{TickRecord, types::TickNumber};
use foretias_core::error::NodeError;
use foretias_core::noise;

use super::calendar::{Calendar, MirrorStore};
use super::communerd::Communerd;
use super::metrics::{NodeMetrics, MetricField};
use self::jsonrpc::JsonRpcResponse;

struct NoOpObserver;
impl TickObserver for NoOpObserver {
    fn on_tick_advance(&self, _tick_number: TickNumber, _public_key: &[u8], _tick_record: &TickRecord) {}
}

pub mod jsonrpc;
pub mod handlers;

pub struct TimeFamilyServer {
    chronomatter: Arc<Chronomatter>,
    calendar: Arc<Calendar>,
    mirror_store: MirrorStore,
    communerd: Option<Arc<Communerd>>,
    listen_addr: String,
    persist_path: Option<std::path::PathBuf>,
    metrics: Arc<NodeMetrics>,
    noise_static_priv: [u8; 32],
    noise_static_pub: [u8; 32],
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
        let (pub_key, priv_key) = generate_ed25519_keypair()?;
        Ok(Self {
            chronomatter: Arc::new(cm),
            calendar,
            mirror_store: MirrorStore::new("/tmp/foretias-mirrors", 64),
            communerd,
            listen_addr: listen_addr.to_string(),
            persist_path,
            metrics,
            noise_static_priv: priv_key.bytes,
            noise_static_pub: pub_key.bytes,
        })
    }

    pub fn from_calendar(
        path: &str,
        listen_addr: &str,
    ) -> Result<Self, NodeError> {
        use std::sync::Arc as StdArc;
        use foretias_core::crypto_server;
        let crypto = StdArc::from(crypto_server::new_software(
            crypto_server::ForetiasCurve::Ed25519,
        )?);
        let mut cm = Chronomatter::from_calendar(path, crypto, Arc::new(NoOpObserver))?;
        let metrics = Arc::new(NodeMetrics::new());
        cm.set_auto_attest_observer(Arc::clone(&metrics) as Arc<dyn AutoAttestObserver>);
        let calendar = Arc::new(Calendar::from_persisted(path)?);
        let (pub_key, priv_key) = generate_ed25519_keypair()?;
        Ok(Self {
            chronomatter: Arc::new(cm),
            calendar,
            mirror_store: MirrorStore::new("/tmp/foretias-mirrors", 64),
            communerd: None,
            listen_addr: listen_addr.to_string(),
            persist_path: None,
            metrics,
            noise_static_priv: priv_key.bytes,
            noise_static_pub: pub_key.bytes,
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

    pub fn mirror_store(&self) -> &MirrorStore {
        &self.mirror_store
    }

    pub fn communerd(&self) -> Option<&Arc<Communerd>> {
        self.communerd.as_ref()
    }

    pub fn metrics(&self) -> &Arc<NodeMetrics> {
        &self.metrics
    }

    pub fn noise_static_pub(&self) -> [u8; 32] {
        self.noise_static_pub
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
            self.calendar.save(json_path.to_str().ok_or(NodeError::Internal("persist path contains invalid UTF-8".into()))?)?;
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

async fn handle_connection(
    server: Arc<TimeFamilyServer>,
    stream: tokio::net::TcpStream,
) -> Result<(), NodeError> {
    let static_priv = server.noise_static_priv;

    let (mut session, stream) = match noise::noise_handshake(stream, &static_priv, None, false).await {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!(component = "server", tbid = %hex::encode(server.get_tbid()), "noise handshake failed: {}", e);
            return Err(NodeError::Internal("noise handshake failed".into()));
        }
    };
    let peer_addr = stream.peer_addr().ok().map(|a| a.to_string());
    tracing::info!(component = "server", tbid = %hex::encode(server.get_tbid()), peer = %peer_addr.as_deref().unwrap_or("unknown"), "noise handshake: success");

    let (reader, writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut writer = writer;

    loop {
        let ciphertext = match read_length_prefixed(&mut reader).await {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("read error: {}", e);
                return Err(NodeError::from(e));
            }
        };

        let plaintext = match session.recv(&ciphertext) {
            Ok(pt) => pt,
            Err(e) => {
                tracing::warn!("noise decrypt failed: {}", e);
                break;
            }
        };

        let line = match String::from_utf8(plaintext) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("invalid UTF-8 in decrypted message: {}", e);
                break;
            }
        };

        if line.len() > MAX_REQUEST_LINE_BYTES {
            tracing::warn!("request line exceeds {} bytes, dropping connection", MAX_REQUEST_LINE_BYTES);
            break;
        }

        let response = match process_request(&server, &line) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("request processing error: {}", e);
                jsonrpc::JsonRpcResponse::error(
                    None,
                    jsonrpc::INTERNAL_ERROR,
                    e.to_string(),
                )
            }
        };

        let response_line = serde_json::to_string(&response)?;
        let ciphertext = match session.send(response_line.as_bytes()) {
            Ok(ct) => ct,
            Err(e) => {
                tracing::warn!("noise encrypt failed: {}", e);
                break;
            }
        };

        write_length_prefixed(&mut writer, &ciphertext).await?;
    }

    Ok(())
}

async fn read_length_prefixed(
    reader: &mut tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Vec<u8>, NodeError> {
    let mut len_buf = [0u8; 4];
    tokio::io::AsyncReadExt::read_exact(reader, &mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_REQUEST_LINE_BYTES + 65535 {
        return Err(NodeError::Internal("oversized message".into()));
    }
    let mut buf = vec![0u8; len];
    tokio::io::AsyncReadExt::read_exact(reader, &mut buf).await?;
    Ok(buf)
}

async fn write_length_prefixed(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    data: &[u8],
) -> Result<(), NodeError> {
    let len = (data.len() as u32).to_le_bytes();
    tokio::io::AsyncWriteExt::write_all(writer, &len).await?;
    tokio::io::AsyncWriteExt::write_all(writer, data).await?;
    tokio::io::AsyncWriteExt::flush(writer).await?;
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
        "route_stamp" => Ok(handlers::handle_route_stamp(server, params)),
        "verify" => Ok(handlers::handle_verify(server, params)),
        "get_calendar_slice" => Ok(handlers::handle_get_calendar_slice(server, params)),
        "integrity_check" => Ok(handlers::handle_integrity_check(server, params)),
        "get_peer_score" => Ok(handlers::handle_get_peer_score(server, params)),
        "collision_status" => Ok(handlers::handle_collision_status(server, params)),
        "get_latest_epoch" => Ok(handlers::handle_get_latest_epoch(server, params)),
        "verify_epoch_snapshot" => Ok(handlers::handle_verify_epoch_snapshot(server, params)),
        "mirror_request" => Ok(handlers::handle_mirror_request(server, params)),
        "mirror_accept" => Ok(handlers::handle_mirror_accept(server, params)),
        "ship_batch" => Ok(handlers::handle_ship_batch(server, params)),
        "ship_ack" => Ok(handlers::handle_ship_ack(server, params)),
        "stream_tick" => Ok(handlers::handle_stream_tick(server, params)),
        "stream_ack" => Ok(handlers::handle_stream_ack(server, params)),
        "mirror_mutual" => Ok(handlers::handle_mirror_mutual(server, params)),
        "mirror_reconcile" => Ok(handlers::handle_mirror_reconcile(server, params)),
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
