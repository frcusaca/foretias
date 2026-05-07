//! PyO3 Python bindings for Foretias v0.1.
//!
//! Exposes: CryptoServer, Foretis, TickRecord, Calendar, TimeFamily.

#![cfg_attr(debug_assertions, allow(rustdoc::all))]
#![allow(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyType;
use serde::{Serialize, Deserialize};

use foretias_core::crypto_server::{self, CryptoServer, ForetiasCurve, PublicKeyBytes, SealedBlob as SealedBlobInner};
use foretias_core::foretias::{self, calendar::Calendar as CalendarInner, tick::TickRecord as TickRecordInner, external_attestation::ExternalAttestation as ExternalAttestationInner};
use foretias_core::foretias::tick::{Foretis as ForetisInner, CalendarLookup};
use foretias_core::core::bindings::{ForetiasPubKey32, ForetiasSig64};
use foretias_core::epoch::snapshot::{PeerScore as PeerScoreInner, EpochSnapshot as EpochSnapshotInner};
use foretias_core::config::{NodeConfig as NodeConfigInner, CollisionConfig as CollisionConfigInner};
use foretias_core::collision::heartbeat::Heartbeat as HeartbeatInner;
use foretias_node::probity::report::ProbityReport as ProbityReportInner;
use foretias_node::server::jsonrpc::JsonRpcError as JsonRpcErrorInner;
use foretias_node::calendar_store::encrypted_jsonl::CalendarBlock as CalendarBlockInner;

/// Python-facing Foretis stamp.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyForetis {
    #[pyo3(get, set)]
    pub tick_number: u64,
    #[pyo3(get, set)]
    pub content_hash: Vec<u8>,
    #[pyo3(get, set)]
    pub signature: Vec<u8>,
    #[pyo3(get, set)]
    pub signature_algorithm: String,
    #[pyo3(get, set)]
    pub tbid: Vec<u8>,
    #[pyo3(get, set)]
    pub echo: String,
    #[pyo3(get, set)]
    pub tbn: String,
    #[pyo3(get, set)]
    pub time_being_reference_time: String,
}

impl From<&ForetisInner> for PyForetis {
    fn from(f: &ForetisInner) -> Self {
        Self {
            tick_number: f.tick_number,
            content_hash: f.content_hash.to_vec(),
            signature: f.signature.clone(),
            signature_algorithm: f.signature_algorithm.clone(),
            tbid: f.tbid.to_vec(),
            echo: f.echo.clone(),
            tbn: f.tbn.clone(),
            time_being_reference_time: f.time_being_reference_time.clone(),
        }
    }
}

#[pymethods]
impl PyForetis {
    /// Create an empty PyForetis (for constructing from JSON in Python).
    #[new]
    fn new() -> Self {
        Self {
            tick_number: 0,
            content_hash: Vec::new(),
            signature: Vec::new(),
            signature_algorithm: String::new(),
            tbid: Vec::new(),
            echo: String::new(),
            tbn: String::new(),
            time_being_reference_time: String::new(),
        }
    }

    /// Construct a PyForetis from a JSON string.
    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "Foretis(tick={}, tbn={}, echo={})",
            self.tick_number, self.tbn, self.echo
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing ExternalAttestation.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyExternalAttestation {
    #[pyo3(get)]
    pub attester_tbid: String,
    #[pyo3(get)]
    pub foretis: PyForetis,
    #[pyo3(get)]
    pub attester_tick_record: PyTickRecord,
    #[pyo3(get)]
    pub received_at_ns: u64,
}

impl From<&ExternalAttestationInner> for PyExternalAttestation {
    fn from(att: &ExternalAttestationInner) -> Self {
        Self {
            attester_tbid: att.attester_tbid.clone(),
            foretis: PyForetis::from(&att.foretis),
            attester_tick_record: PyTickRecord::from(&att.attester_tick_record),
            received_at_ns: att.received_at_ns,
        }
    }
}

/// Python-facing TickRecord.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyTickRecord {
    #[pyo3(get)]
    pub tick_number: u64,
    #[pyo3(get)]
    pub public_key: Vec<u8>,
    #[pyo3(get)]
    pub forward_foretis: Vec<u8>,
    #[pyo3(get)]
    pub backward_foretis: Vec<u8>,
    #[pyo3(get)]
    pub aa_nonce: Vec<u8>,
    #[pyo3(get)]
    pub external_attestations: Vec<PyExternalAttestation>,
}

impl From<&TickRecordInner> for PyTickRecord {
    fn from(t: &TickRecordInner) -> Self {
        Self {
            tick_number: t.tick_number,
            public_key: t.public_key.clone(),
            forward_foretis: t.forward_foretis.clone(),
            backward_foretis: t.backward_foretis.clone(),
            aa_nonce: t.aa_nonce.to_vec(),
            external_attestations: t.external_attestations.iter().map(PyExternalAttestation::from).collect(),
        }
    }
}

#[pymethods]
impl PyTickRecord {
    fn __repr__(&self) -> String {
        format!("TickRecord(tick={})", self.tick_number)
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing Calendar.
#[pyclass]
#[derive(Clone, Serialize)]
pub struct PyCalendar {
    #[pyo3(get)]
    pub tbid: Vec<u8>,
    #[pyo3(get)]
    pub tbn: String,
    #[pyo3(get)]
    pub ticks: Vec<PyTickRecord>,
}

impl From<&CalendarInner> for PyCalendar {
    fn from(c: &CalendarInner) -> Self {
        Self {
            tbid: c.tbid.to_vec(),
            tbn: c.tbn.clone(),
            ticks: c.ticks.iter().map(PyTickRecord::from).collect(),
        }
    }
}

#[pymethods]
impl PyCalendar {
    fn __repr__(&self) -> String {
        format!(
            "Calendar(tbn={}, ticks={})",
            self.tbn,
            self.ticks.len()
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    fn latest_tick(&self) -> Option<u64> {
        self.ticks.last().map(|t| t.tick_number)
    }

    fn tick_count(&self) -> usize {
        self.ticks.len()
    }
}

fn pubkey_to_vec(pk: PublicKeyBytes) -> Vec<u8> {
    match pk {
        PublicKeyBytes::Ed25519(k) => k.bytes.to_vec(),
        PublicKeyBytes::P256Compressed(k) => k.bytes.to_vec(),
    }
}

/// Python-facing CryptoServer wrapper.
#[pyclass]
pub struct PyCryptoServer {
    inner: Box<dyn CryptoServer>,
}

#[pymethods]
impl PyCryptoServer {
    /// Create a new CryptoServer with the given curve.
    #[new]
    #[pyo3(signature = (curve = "ed25519"))]
    fn new(curve: &str) -> PyResult<Self> {
        let curve_enum = match curve {
            "ed25519" => ForetiasCurve::Ed25519,
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("unsupported curve: {}", curve),
            )),
        };
        let inner = crypto_server::new_software(curve_enum)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Sign a message and return the signature bytes.
    fn sign(&self, msg: &[u8]) -> PyResult<Vec<u8>> {
        let sig = self.inner.sign(msg)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(sig.bytes.to_vec())
    }

    /// Verify an ed25519 signature.
    fn verify(&self, pub_key: &[u8], msg: &[u8], sig: &[u8]) -> PyResult<bool> {
        if pub_key.len() != 32 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("pub_key must be 32 bytes"));
        }
        if sig.len() != 64 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("sig must be 64 bytes"));
        }
        let pub_key_bytes: [u8; 32] = pub_key.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("pub_key must be exactly 32 bytes"))?;
        let pub_key = ForetiasPubKey32 { bytes: pub_key_bytes };
        let sig_bytes: [u8; 64] = sig.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("sig must be exactly 64 bytes"))?;
        let sig = ForetiasSig64 { bytes: sig_bytes };
        self.inner.verify_ed25519(&pub_key, msg, &sig)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Return the public key bytes.
    fn public_key(&self) -> PyResult<Vec<u8>> {
        Ok(pubkey_to_vec(self.inner.public_key()))
    }

    /// Return the peer ID bytes.
    fn peer_id(&self) -> PyResult<Vec<u8>> {
        Ok(self.inner.peer_id().bytes.to_vec())
    }

    /// SHA-256 hash of the given data.
    fn sha256(&self, data: &[u8]) -> PyResult<Vec<u8>> {
        let hash = self.inner.sha256(data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(hash.bytes.to_vec())
    }

    /// Blake3 hash of the given data.
    fn blake3(&self, data: &[u8]) -> PyResult<Vec<u8>> {
        let hash = self.inner.blake3(data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(hash.bytes.to_vec())
    }

    fn __repr__(&self) -> String {
        let pk = hex::encode(pubkey_to_vec(self.inner.public_key()));
        format!("CryptoServer(peer_id={})", &pk[..16])
    }
}

/// Python-facing TimeFamily orchestrator.
///
/// Manages identity, calendar, and tick numbering.
/// Provides stamp(), verify(), get_calendar() and related methods.
#[pyclass]
pub struct PyTimeFamily {
    server: Box<dyn CryptoServer>,
    calendar: parking_lot::RwLock<CalendarInner>,
    current_tick: parking_lot::Mutex<u64>,
    tbid: [u8; 16],
    tbn: String,
}

#[pymethods]
impl PyTimeFamily {
    /// Create a new TimeFamily with a fresh identity.
    #[new]
    #[pyo3(signature = (tbn = None))]
    fn new(tbn: Option<String>) -> PyResult<Self> {
        let server = crypto_server::new_software(ForetiasCurve::Ed25519)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let mut tbid = [0u8; 16];
        server.random_bytes(&mut tbid)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let tbn = tbn.unwrap_or_else(|| format!("tf-{}", hex::encode(&tbid[..8])));

        let calendar = CalendarInner::new(tbid, &tbn);

        Ok(Self {
            server,
            calendar: parking_lot::RwLock::new(calendar),
            current_tick: parking_lot::Mutex::new(0),
            tbid,
            tbn,
        })
    }

    /// Stamp content, producing a Foretis and appending to calendar.
    #[pyo3(signature = (content, echo = ""))]
    fn stamp(&self, content: &[u8], echo: &str) -> PyResult<PyForetis> {
        let mut tick = self.current_tick.lock();
        *tick += 1;
        let tick_number = *tick;

        let clock = foretias_core::clock::SystemClock;
        let foretis = foretias::tick::stamp(
            self.server.as_ref(),
            &clock,
            &self.tbid,
            tick_number,
            content,
            echo,
            &self.tbn,
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        // Note: foretias::tick::stamp uses server.sign() which is always Ed25519,
        // but labels it as server.signature_algorithm() (SPHINCS+). We correct this
        // so that verify() uses the matching algorithm path.
        let sig_alg = "Ed25519".to_string();
        let mut corrected_foretis = foretis.clone();
        corrected_foretis.signature_algorithm = sig_alg.clone();

        let pub_key_bytes = pubkey_to_vec(self.server.public_key());
        let record = TickRecordInner {
            tick_number,
            public_key: pub_key_bytes,
            signature_algorithm: sig_alg,
            forward_foretis: vec![],
            backward_foretis: vec![],
            aa_nonce: [0u8; 16],
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
        };

        let mut cal = self.calendar.write();
        cal.append(record)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(PyForetis::from(&corrected_foretis))
    }

    /// Verify content against a Foretis stamp.
    fn verify(&self, content: &[u8], foretis: &PyForetis) -> PyResult<bool> {
        let content_hash: [u8; 32] = foretis.content_hash[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("content_hash must be 32 bytes"))?;
        let tbid: [u8; 16] = foretis.tbid[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("tbid must be 16 bytes"))?;

        let inner_foretis = ForetisInner {
            tick_number: foretis.tick_number,
            content_hash,
            signature: foretis.signature.clone(),
            signature_algorithm: foretis.signature_algorithm.clone(),
            tbid,
            echo: foretis.echo.clone(),
            tbn: foretis.tbn.clone(),
            time_being_reference_time: foretis.time_being_reference_time.clone(),
        };

        let cal = self.calendar.read();
        foretias::tick::verify(self.server.as_ref(), &inner_foretis, content, &*cal)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Return the current calendar as a PyCalendar.
    fn get_calendar(&self) -> PyResult<PyCalendar> {
        let cal = self.calendar.read();
        Ok(PyCalendar::from(&*cal))
    }

    /// Return the tbid as a hex string.
    fn get_tbid(&self) -> PyResult<String> {
        Ok(hex::encode(self.tbid))
    }

    /// Return the tbn (time branch name).
    fn get_tbn(&self) -> PyResult<String> {
        Ok(self.tbn.clone())
    }

    /// Return the latest tick number, or None if no ticks yet.
    fn get_latest_tick(&self) -> PyResult<Option<u64>> {
        let cal = self.calendar.read();
        Ok(cal.ticks.last().map(|t| t.tick_number))
    }

    /// Return the current tick counter.
    fn get_current_tick(&self) -> PyResult<u64> {
        let tick = self.current_tick.lock();
        Ok(*tick)
    }

    /// Return the public key as hex.
    fn get_public_key(&self) -> PyResult<String> {
        Ok(hex::encode(pubkey_to_vec(self.server.public_key())))
    }

    /// Return the peer ID as hex.
    fn get_peer_id(&self) -> PyResult<String> {
        Ok(hex::encode(self.server.peer_id().bytes))
    }

    fn __repr__(&self) -> String {
        let tick = self.current_tick.lock();
        format!(
            "TimeFamily(tbn={}, tbid={}, ticks={})",
            self.tbn,
            &hex::encode(self.tbid)[..8],
            *tick
        )
    }
}

/// Python-facing TimeFamilyServer — wraps the full Rust server with Phase 4 features.
///
/// Provides: stamp(), verify(), integrity_check(), get_calendar(), get_calendar_slice(),
/// save(), is_dormant(), daemon_tick(), and related properties.
/// Supports persistence via persist_path and dormant (verify-only) mode.
#[pyclass]
pub struct PyTimeFamilyServer {
    server: Arc<foretias_node::server::TimeFamilyServer>,
}

#[pymethods]
impl PyTimeFamilyServer {
    /// Create a new TimeFamilyServer with a fresh identity.
    ///
    /// Args:
    ///     listen_addr: TCP listen address (default "127.0.0.1:4001").
    ///     chronon_ns: Chronon interval in nanoseconds (default 60s = 60_000_000_000ns).
    ///     persist_path: Optional directory path for calendar persistence.
    #[new]
    #[pyo3(signature = (listen_addr = "127.0.0.1:4001", chronon_ns = 60_000_000_000, persist_path = None))]
    fn new(listen_addr: &str, chronon_ns: u64, persist_path: Option<String>) -> PyResult<Self> {
        let persist = persist_path.map(PathBuf::from);
        let server = Arc::new(
            foretias_node::server::TimeFamilyServer::new_with_persist(
                listen_addr,
                chronon_ns,
                persist,
            ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?,
        );
        Ok(Self { server })
    }

    /// Create a dormant (verify-only) server from a persisted calendar JSON file.
    ///
    /// Args:
    ///     calendar_path: Path to the calendar JSON file.
    ///     listen_addr: TCP listen address (default "127.0.0.1:4001").
    #[classmethod]
    #[pyo3(signature = (calendar_path, listen_addr = "127.0.0.1:4001"))]
    fn from_calendar(_cls: &Bound<'_, PyType>, calendar_path: String, listen_addr: &str) -> PyResult<Self> {
        let server = Arc::new(
            foretias_node::server::TimeFamilyServer::from_calendar(
                &calendar_path,
                listen_addr,
            ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?,
        );
        Ok(Self { server })
    }

    /// Stamp content, producing a Foretis attestation.
    ///
    /// Args:
    ///     content: Raw bytes to attesting.
    ///     echo: Optional echo string.
    ///
    /// Returns:
    ///     PyForetis attestation record.
    ///
    /// Raises RuntimeError if the server is in dormant mode.
    #[pyo3(signature = (content, echo = ""))]
    fn stamp(&self, content: &[u8], echo: &str) -> PyResult<PyForetis> {
        let foretis = self.server.chronomatter().stamp(content.to_vec(), echo.to_string())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(PyForetis::from(&foretis))
    }

    /// Verify a Foretis attestation against content.
    ///
    /// Args:
    ///     content: Raw bytes to verify.
    ///     foretis: The Foretis attestation to verify.
    ///
    /// Returns:
    ///     True if the attestation is valid.
    fn verify(&self, content: &[u8], foretis: &PyForetis) -> PyResult<bool> {
        let content_hash: [u8; 32] = foretis.content_hash[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("content_hash must be 32 bytes"))?;
        let tbid: [u8; 16] = foretis.tbid[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("tbid must be 16 bytes"))?;

        let inner_foretis = ForetisInner {
            tick_number: foretis.tick_number,
            content_hash,
            signature: foretis.signature.clone(),
            signature_algorithm: foretis.signature_algorithm.clone(),
            tbid,
            echo: foretis.echo.clone(),
            tbn: foretis.tbn.clone(),
            time_being_reference_time: foretis.time_being_reference_time.clone(),
        };

        let result = self.server.chronomatter().verify(
            &inner_foretis,
            &content.to_vec(),
            &*self.server.calendar().inner().read(),
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(result)
    }

    /// Verify a Foretis attestation from a JSON string.
    ///
    /// Args:
    ///     content: Raw bytes to verify.
    ///     foretis_json: JSON string of the Foretis attestation.
    ///
    /// Returns:
    ///     True if the attestation is valid.
    fn verify_json(&self, content: &[u8], foretis_json: &str) -> PyResult<bool> {
        let foretis: ForetisInner = serde_json::from_str(foretis_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let result = self.server.chronomatter().verify(
            &foretis,
            &content.to_vec(),
            &*self.server.calendar().inner().read(),
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(result)
    }

    /// Check chain integrity over a tick range.
    ///
    /// Returns JSON string with keys: all_valid, pair_results, pairs_checked.
    #[pyo3(signature = (start = None, end = None))]
    fn integrity_check(&self, start: Option<u64>, end: Option<u64>) -> PyResult<String> {
        let pair_results = self.server.integrity_check(start, end)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let all_valid = pair_results.iter().all(|&v| v);
        let pairs_checked = pair_results.len();
        let result = serde_json::json!({
            "all_valid": all_valid,
            "pair_results": pair_results,
            "pairs_checked": pairs_checked,
        });
        Ok(serde_json::to_string(&result)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?)
    }

    /// Return the full calendar as a PyCalendar.
    fn get_calendar(&self) -> PyResult<PyCalendar> {
        let binding = self.server.calendar().inner();
        let cal = binding.read();
        Ok(PyCalendar::from(&*cal))
    }

    /// Return a slice of calendar tick records.
    ///
    /// Args:
    ///     cal_tick_start: Starting tick number (default 0).
    ///     count: Number of records to return (default 10).
    ///
    /// Returns:
    ///     List of PyTickRecord objects.
    #[pyo3(signature = (cal_tick_start = 0, count = 10))]
    fn get_calendar_slice(&self, cal_tick_start: u64, count: usize) -> PyResult<Vec<PyTickRecord>> {
        let binding = self.server.calendar().inner();
        let cal = binding.read();
        let records = cal.get(cal_tick_start, count)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(records.iter().map(PyTickRecord::from).collect())
    }

    /// Persist the calendar to disk if a persist_path was configured.
    fn save(&self) -> PyResult<()> {
        self.server.save()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Create a daemon tick (auto-attestation tick with no content).
    ///
    /// Not available in dormant mode.
    fn daemon_tick(&self) -> PyResult<()> {
        if self.server.is_dormant() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "daemon_tick not available in dormant mode".to_string(),
            ));
        }
        self.server.daemon_tick()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    /// Return True if this server is in dormant (verify-only) mode.
    fn is_dormant(&self) -> bool {
        self.server.is_dormant()
    }

    /// Return the TimeBeing identifier as a hex string.
    fn get_tbid(&self) -> String {
        hex::encode(self.server.get_tbid())
    }

    /// Return the TimeBeing name.
    fn get_tbn(&self) -> String {
        self.server.get_tbn().to_string()
    }

    /// Return the chronon interval in nanoseconds.
    fn get_chronon(&self) -> u64 {
        self.server.chronomatter().chronon_ns()
    }

    /// Return the latest tick number, or None if no ticks yet.
    fn get_latest_tick(&self) -> Option<u64> {
        let binding = self.server.calendar().inner();
        let cal = binding.read();
        cal.ticks.last().map(|t| t.tick_number)
    }

    /// Return the current tick counter.
    fn get_current_tick(&self) -> u64 {
        self.server.current_tick()
    }

    /// Return the public key (main crypto server) as hex.
    fn get_public_key(&self) -> PyResult<String> {
        let chrono = self.server.chronomatter();
        match chrono.latest_public_key() {
            Some(pk) => Ok(hex::encode(pk)),
            None => Ok(String::new()),
        }
    }

    fn __repr__(&self) -> String {
        let tick = self.server.current_tick();
        let dormant = if self.server.is_dormant() { " (dormant)" } else { "" };
        format!(
            "TimeFamilyServer(tbn={}, tbid={}, ticks={}{})",
            self.server.get_tbn(),
            &hex::encode(self.server.get_tbid())[..8],
            tick,
            dormant
        )
    }
}

/// Python-facing PeerScore.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyPeerScore {
    #[pyo3(get)]
    pub peer_id: String,
    #[pyo3(get)]
    pub score: f32,
}

impl From<&PeerScoreInner> for PyPeerScore {
    fn from(p: &PeerScoreInner) -> Self {
        Self { peer_id: p.peer_id.clone(), score: p.score }
    }
}

#[pymethods]
impl PyPeerScore {
    #[new]
    fn new() -> Self {
        Self { peer_id: String::new(), score: 0.0 }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("PeerScore(peer_id={}, score={})", self.peer_id, self.score)
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing EpochSnapshot.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyEpochSnapshot {
    #[pyo3(get)]
    pub epoch_number: u64,
    #[pyo3(get)]
    pub epoch_start_ns: u64,
    #[pyo3(get)]
    pub epoch_end_ns: u64,
    #[pyo3(get)]
    pub peer_scores: Vec<PyPeerScore>,
    #[pyo3(get)]
    pub committee: Vec<String>,
    #[pyo3(get)]
    pub threshold: u32,
    #[pyo3(get)]
    pub frost_signature: Vec<u8>,
    #[pyo3(get)]
    pub committee_pubkey: Vec<u8>,
}

impl From<&EpochSnapshotInner> for PyEpochSnapshot {
    fn from(e: &EpochSnapshotInner) -> Self {
        Self {
            epoch_number: e.epoch_number,
            epoch_start_ns: e.epoch_start_ns,
            epoch_end_ns: e.epoch_end_ns,
            peer_scores: e.peer_scores.iter().map(PyPeerScore::from).collect(),
            committee: e.committee.clone(),
            threshold: e.threshold,
            frost_signature: e.frost_signature.clone(),
            committee_pubkey: e.committee_pubkey.clone(),
        }
    }
}

#[pymethods]
impl PyEpochSnapshot {
    #[new]
    fn new() -> Self {
        Self {
            epoch_number: 0,
            epoch_start_ns: 0,
            epoch_end_ns: 0,
            peer_scores: Vec::new(),
            committee: Vec::new(),
            threshold: 0,
            frost_signature: Vec::new(),
            committee_pubkey: Vec::new(),
        }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "EpochSnapshot(epoch={}, peers={}, committee={})",
            self.epoch_number, self.peer_scores.len(), self.committee.len()
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing SealedBlob.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PySealedBlob {
    #[pyo3(get)]
    pub nonce: Vec<u8>,
    #[pyo3(get)]
    pub ciphertext: Vec<u8>,
}

impl From<&SealedBlobInner> for PySealedBlob {
    fn from(s: &SealedBlobInner) -> Self {
        Self { nonce: s.nonce.clone(), ciphertext: s.ciphertext.clone() }
    }
}

#[pymethods]
impl PySealedBlob {
    #[new]
    fn new() -> Self {
        Self { nonce: Vec::new(), ciphertext: Vec::new() }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "SealedBlob(nonce_len={}, ciphertext_len={})",
            self.nonce.len(), self.ciphertext.len()
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing CollisionConfig.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyCollisionConfig {
    #[pyo3(get)]
    pub heartbeat_interval_secs: u64,
    #[pyo3(get)]
    pub nonce_window: usize,
    #[pyo3(get)]
    pub liege_wait_secs: u64,
}

impl From<&CollisionConfigInner> for PyCollisionConfig {
    fn from(c: &CollisionConfigInner) -> Self {
        Self {
            heartbeat_interval_secs: c.heartbeat_interval_secs,
            nonce_window: c.nonce_window,
            liege_wait_secs: c.liege_wait_secs,
        }
    }
}

#[pymethods]
impl PyCollisionConfig {
    #[new]
    fn new() -> Self {
        Self { heartbeat_interval_secs: 30, nonce_window: 10, liege_wait_secs: 30 }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "CollisionConfig(hb_interval={}, nonce_window={}, liege_wait={})",
            self.heartbeat_interval_secs, self.nonce_window, self.liege_wait_secs
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing NodeConfig.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyNodeConfig {
    #[pyo3(get)]
    pub listen_addr: String,
    #[pyo3(get)]
    pub version: String,
    #[pyo3(get)]
    pub calendar_path: String,
    #[pyo3(get)]
    pub chronon_ns: u64,
    #[pyo3(get)]
    pub serialized: bool,
    #[pyo3(get)]
    pub peers: Vec<String>,
    #[pyo3(get)]
    pub auto_attest_every_n: u64,
    #[pyo3(get)]
    pub request_timeout_secs: u64,
    #[pyo3(get)]
    pub p2p_listen: Option<String>,
    #[pyo3(get)]
    pub p2p_dial: Vec<String>,
    #[pyo3(get)]
    pub dht_namespace: String,
    #[pyo3(get)]
    pub dht_bootstrap: Vec<String>,
    #[pyo3(get)]
    pub collision: PyCollisionConfig,
}

impl From<&NodeConfigInner> for PyNodeConfig {
    fn from(c: &NodeConfigInner) -> Self {
        Self {
            listen_addr: c.listen_addr.clone(),
            version: c.version.clone(),
            calendar_path: c.calendar_path.to_string_lossy().to_string(),
            chronon_ns: c.chronon_ns,
            serialized: c.serialized,
            peers: c.peers.clone(),
            auto_attest_every_n: c.auto_attest_every_n,
            request_timeout_secs: c.request_timeout_secs,
            p2p_listen: c.p2p_listen.clone(),
            p2p_dial: c.p2p_dial.clone(),
            dht_namespace: c.dht_namespace.clone(),
            dht_bootstrap: c.dht_bootstrap.clone(),
            collision: PyCollisionConfig::from(&c.collision),
        }
    }
}

#[pymethods]
impl PyNodeConfig {
    #[new]
    fn new() -> Self {
        Self {
            listen_addr: "127.0.0.1:4001".to_string(),
            version: String::new(),
            calendar_path: ".foretias/calendars".to_string(),
            chronon_ns: 60_000_000_000,
            serialized: false,
            peers: Vec::new(),
            auto_attest_every_n: 1,
            request_timeout_secs: 5,
            p2p_listen: None,
            p2p_dial: Vec::new(),
            dht_namespace: "mainnet".to_string(),
            dht_bootstrap: Vec::new(),
            collision: PyCollisionConfig::new(),
        }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "NodeConfig(listen={}, dht_ns={}, peers={})",
            self.listen_addr, self.dht_namespace, self.peers.len()
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing Heartbeat.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyHeartbeat {
    #[pyo3(get)]
    pub peer_id: String,
    #[pyo3(get)]
    pub timestamp_ns: u64,
    #[pyo3(get)]
    pub nonce: Vec<u8>,
    #[pyo3(get)]
    pub curve: u8,
    #[pyo3(get)]
    pub signature: Vec<u8>,
}

impl From<&HeartbeatInner> for PyHeartbeat {
    fn from(h: &HeartbeatInner) -> Self {
        Self {
            peer_id: h.peer_id.clone(),
            timestamp_ns: h.timestamp_ns,
            nonce: h.nonce.to_vec(),
            curve: h.curve,
            signature: h.signature.clone(),
        }
    }
}

#[pymethods]
impl PyHeartbeat {
    #[new]
    fn new() -> Self {
        Self {
            peer_id: String::new(),
            timestamp_ns: 0,
            nonce: vec![0u8; 16],
            curve: 0,
            signature: Vec::new(),
        }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "Heartbeat(peer_id={}, ts_ns={}, curve={})",
            self.peer_id, self.timestamp_ns, self.curve
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing ProbityReport.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyProbityReport {
    #[pyo3(get)]
    pub subject: String,
    #[pyo3(get)]
    pub reporter: String,
    #[pyo3(get)]
    pub attribute: String,
    #[pyo3(get)]
    pub value: f32,
    #[pyo3(get)]
    pub timestamp_ns: u64,
    #[pyo3(get)]
    pub signature: Vec<u8>,
    #[pyo3(get)]
    pub curve: u8,
}

impl From<&ProbityReportInner> for PyProbityReport {
    fn from(p: &ProbityReportInner) -> Self {
        Self {
            subject: p.subject.clone(),
            reporter: p.reporter.clone(),
            attribute: p.attribute.clone(),
            value: p.value,
            timestamp_ns: p.timestamp_ns,
            signature: p.signature.clone(),
            curve: p.curve,
        }
    }
}

#[pymethods]
impl PyProbityReport {
    #[new]
    fn new() -> Self {
        Self {
            subject: String::new(),
            reporter: String::new(),
            attribute: String::new(),
            value: 0.0,
            timestamp_ns: 0,
            signature: Vec::new(),
            curve: 0,
        }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "ProbityReport(subject={}, reporter={}, attr={}, value={})",
            self.subject, self.reporter, self.attribute, self.value
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing JsonRpcError.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyJsonRpcError {
    #[pyo3(get)]
    pub code: i32,
    #[pyo3(get)]
    pub message: String,
}

impl From<&JsonRpcErrorInner> for PyJsonRpcError {
    fn from(e: &JsonRpcErrorInner) -> Self {
        Self { code: e.code, message: e.message.clone() }
    }
}

#[pymethods]
impl PyJsonRpcError {
    #[new]
    fn new() -> Self {
        Self { code: 0, message: String::new() }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("JsonRpcError(code={}, message={})", self.code, self.message)
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing CalendarBlock.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyCalendarBlock {
    #[pyo3(get)]
    pub block_id: u64,
    #[pyo3(get)]
    pub written_at_ns: u64,
    #[pyo3(get)]
    pub ticks: Vec<PyTickRecord>,
}

impl From<&CalendarBlockInner> for PyCalendarBlock {
    fn from(b: &CalendarBlockInner) -> Self {
        Self {
            block_id: b.block_id,
            written_at_ns: b.written_at_ns,
            ticks: b.ticks.iter().map(PyTickRecord::from).collect(),
        }
    }
}

#[pymethods]
impl PyCalendarBlock {
    #[new]
    fn new() -> Self {
        Self { block_id: 0, written_at_ns: 0, ticks: Vec::new() }
    }

    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "CalendarBlock(block_id={}, ticks={})",
            self.block_id, self.ticks.len()
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Foretias P2P Python module.
#[pymodule]
fn foretias_p2p(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCryptoServer>()?;
    m.add_class::<PyForetis>()?;
    m.add_class::<PyExternalAttestation>()?;
    m.add_class::<PyTickRecord>()?;
    m.add_class::<PyCalendar>()?;
    m.add_class::<PyTimeFamily>()?;
    m.add_class::<PyTimeFamilyServer>()?;
    m.add_class::<PyPeerScore>()?;
    m.add_class::<PyEpochSnapshot>()?;
    m.add_class::<PySealedBlob>()?;
    m.add_class::<PyCollisionConfig>()?;
    m.add_class::<PyNodeConfig>()?;
    m.add_class::<PyHeartbeat>()?;
    m.add_class::<PyProbityReport>()?;
    m.add_class::<PyJsonRpcError>()?;
    m.add_class::<PyCalendarBlock>()?;
    Ok(())
}
