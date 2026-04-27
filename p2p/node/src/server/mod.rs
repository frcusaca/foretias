use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::crypto_server::{self, CryptoServer};
use crate::core::bindings::FortiasPrivKey32;
use crate::error::NodeError;
use crate::fortias::{auto_attestation_blob, Calendar, TickRecord};
use crate::fortias::tick::CalendarLookup;

use self::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

/// A keypair for a single tick, with the private key zeroized on drop.
#[derive(Debug)]
pub struct TickKeyPair {
    pub pub_key: [u8; 32],
    pub priv_key: Zeroizing<[u8; 32]>,
}

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
    /// Per-tick keypairs for auto-attestation.
    keypairs: parking_lot::RwLock<Vec<TickKeyPair>>,
    /// Optional path for persisting calendar to disk.
    persist_path: Option<std::path::PathBuf>,
    /// Whether this server is in dormant (verify-only) mode.
    is_dormant: bool,
    /// Handle for the background daemon task that ticks the calendar.
    daemon_handle: parking_lot::Mutex<Option<JoinHandle<()>>>,
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
            keypairs: parking_lot::RwLock::new(Vec::new()),
            persist_path: None,
            is_dormant: false,
            daemon_handle: parking_lot::Mutex::new(None),
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

    /// Generate a new Ed25519 keypair and store it, returning its tick index.
    pub fn generate_and_store_keypair(&self) -> Result<usize, NodeError> {
        let (pub_key, priv_key) = crate::core::identity::generate_ed25519_keypair()
            .map_err(|e| NodeError::Crypto(e))?;
        let kp = TickKeyPair {
            pub_key: pub_key.bytes,
            priv_key: Zeroizing::new(priv_key.bytes),
        };
        let mut keypairs = self.keypairs.write();
        let idx = keypairs.len();
        keypairs.push(kp);
        Ok(idx)
    }

    /// Returns the public key for the keypair at the given index.
    pub fn keypair_pub(&self, idx: usize) -> Option<[u8; 32]> {
        self.keypairs.read().get(idx).map(|kp| kp.pub_key)
    }

    /// Sign a message with the keypair at the given index.
    pub fn sign_with_keypair(&self, idx: usize, msg: &[u8]) -> Result<Vec<u8>, NodeError> {
        let keypairs = self.keypairs.read();
        let kp = keypairs.get(idx).ok_or_else(|| {
            NodeError::Internal(format!("keypair index {} out of range", idx))
        })?;
        let sig = crate::core::signing::ed25519_sign(
            &FortiasPrivKey32 { bytes: *kp.priv_key },
            msg,
        )?;
        Ok(sig.bytes.to_vec())
    }

    /// Creates a new server, optionally loading calendar from disk.
    pub fn new_with_persist(
        listen_addr: &str,
        chronon_ns: u64,
        persist_path: Option<std::path::PathBuf>,
    ) -> Result<Self, NodeError> {
        let crypto = crypto_server::new_software(
            crypto_server::FortiasCurve::Ed25519,
        )?;

        let uuid = uuid::Uuid::new_v4();
        let tbid = *uuid.as_bytes();
        let tbn = format!("tf-{}", hex::encode(&tbid[..8]));

        let calendar = if let Some(ref p) = persist_path {
            let json_path = p.join(format!("{}.json", hex::encode(tbid)));
            if json_path.exists() {
                Calendar::load(json_path.to_str().unwrap())?
            } else {
                Calendar::new(tbid, &tbn)
            }
        } else {
            Calendar::new(tbid, &tbn)
        };

        Ok(Self {
            tbid,
            tbn,
            server: crypto,
            calendar: parking_lot::RwLock::new(calendar),
            current_tick: parking_lot::Mutex::new(0),
            chronon_ns,
            listen_addr: listen_addr.to_string(),
            keypairs: parking_lot::RwLock::new(Vec::new()),
            persist_path,
            is_dormant: false,
            daemon_handle: parking_lot::Mutex::new(None),
        })
    }

    /// Creates a dormant (verify-only) server from a persisted calendar file.
    ///
    /// In dormant mode the server has no private keys and cannot stamp;
    /// it can only verify existing attestations and serve calendar slices.
    pub fn from_calendar(
        path: &str,
        listen_addr: &str,
        server: Box<dyn CryptoServer>,
    ) -> Result<Self, NodeError> {
        let calendar = Calendar::load(path)?;
        let latest_tick = calendar.latest().unwrap_or(0);

        Ok(Self {
            tbid: calendar.tbid,
            tbn: calendar.tbn.clone(),
            server,
            calendar: parking_lot::RwLock::new(calendar),
            current_tick: parking_lot::Mutex::new(latest_tick),
            chronon_ns: 0,
            listen_addr: listen_addr.to_string(),
            keypairs: parking_lot::RwLock::new(Vec::new()),
            persist_path: None,
            is_dormant: true,
            daemon_handle: parking_lot::Mutex::new(None),
        })
    }

    /// Returns true if this server is in dormant (verify-only) mode.
    pub fn is_dormant(&self) -> bool {
        self.is_dormant
    }

    /// Persists the calendar to disk if a persist path was configured.
    pub fn save(&self) -> Result<(), NodeError> {
        if let Some(ref p) = self.persist_path {
            let json_path = p.join(format!("{}.json", hex::encode(self.tbid)));
            std::fs::create_dir_all(p)?;
            self.calendar.read().save(json_path.to_str().unwrap())
        } else {
            Ok(())
        }
    }

    // ── Daemon ticking ──────────────────────────────────────────────────────

    /// Creates a single tick record and appends it to the calendar.
    ///
    /// This is the core tick-creation logic shared by both `do_stamp` (in handlers)
    /// and the background daemon. The caller provides the tick number, keypair index,
    /// and optional content (for stamp) or `None` (for daemon ticks).
    fn create_tick(
        &self,
        tick: u64,
        kp_idx: usize,
        _content: Option<Vec<u8>>,
    ) -> Result<TickRecord, NodeError> {
        let tbid = self.tbid;
        let tbid_str = hex::encode(tbid);
        let new_pub = self.keypair_pub(kp_idx).unwrap();

        let (forward_fortis, backward_fortis) = if self.calendar.read().ticks.is_empty() {
            let ma_blob = auto_attestation_blob(&tbid_str, tick, &new_pub, tick, &new_pub);
            let sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (sig.clone(), sig)
        } else {
            let cal = self.calendar.read();
            let latest_rec = cal.ticks.last().unwrap();
            let prev_tick = latest_rec.tick_number;
            let prev_kp_idx = (prev_tick - 1) as usize;
            let prev_pub = self.keypair_pub(prev_kp_idx).unwrap();

            let ma_blob = auto_attestation_blob(&tbid_str, prev_tick, &prev_pub, tick, &new_pub);

            let forward_sig = self.sign_with_keypair(prev_kp_idx, &ma_blob)?;
            let backward_sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (forward_sig, backward_sig)
        };

        let record = TickRecord {
            tick_number: tick,
            public_key: new_pub.to_vec(),
            forward_fortis,
            backward_fortis,
        };

        self.calendar.write().append(record.clone())?;
        Ok(record)
    }

    /// Creates a daemon tick (no content) and appends it to the calendar.
    pub fn daemon_tick(&self) -> Result<(), NodeError> {
        let tick = {
            let mut counter = self.current_tick.lock();
            *counter += 1;
            *counter
        };

        let kp_idx = self.generate_and_store_keypair()?;
        let _record = self.create_tick(tick, kp_idx, None)?;

        if let Err(e) = self.save() {
            tracing::warn!("failed to persist calendar after daemon tick: {}", e);
        }

        Ok(())
    }

    /// Starts the daemon. Must be called on an Arc<Self>.
    pub fn start_daemon_arc(self: &Arc<Self>) {
        if self.chronon_ns == 0 {
            tracing::warn!("chronon_ns is 0, daemon will not start");
            return;
        }

        let chronon_ms = self.chronon_ns / 1_000_000;
        let server = Arc::clone(self);

        let handle = tokio::spawn(async move {
            let interval = tokio::time::Duration::from_millis(
                if chronon_ms > 0 { chronon_ms } else { 1 },
            );
            let mut interval = tokio::time::interval(interval);

            loop {
                interval.tick().await;
                tracing::debug!("daemon tick");

                if let Err(e) = server.daemon_tick() {
                    tracing::error!("daemon tick failed: {}", e);
                }
            }
        });

        let mut daemon = self.daemon_handle.lock();
        *daemon = Some(handle);
        tracing::info!("daemon started with chronon interval: {} ms", chronon_ms);
    }

    /// Stops the background daemon task.
    pub fn stop_daemon_arc(&self) {
        let mut daemon = self.daemon_handle.lock();
        if let Some(handle) = daemon.take() {
            handle.abort();
            tracing::info!("daemon stopped");
        }
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
        "integrity_check" => Ok(handlers::handle_integrity_check(server, request.params)),
        _ => Ok(jsonrpc::JsonRpcResponse::error(
            request.id.clone(),
            jsonrpc::METHOD_NOT_FOUND,
            format!("method '{}' not found", request.method),
        )),
    }
}
