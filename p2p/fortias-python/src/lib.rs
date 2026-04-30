//! PyO3 Python bindings for Fortias v0.1.
//!
//! Exposes: CryptoServer, Fortis, TickRecord, Calendar, TimeFamily.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyType;
use serde::{Serialize, Deserialize};

use fortias_core::crypto_server::{self, CryptoServer, FortiasCurve, PublicKeyBytes};
use fortias_core::fortias::{self, calendar::Calendar as CalendarInner, tick::TickRecord as TickRecordInner};
use fortias_core::fortias::tick::{Fortis as FortisInner, CalendarLookup};
use fortias_core::core::bindings::{FortiasPubKey32, FortiasSig64};

/// Python-facing Fortis stamp.
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
pub struct PyFortis {
    #[pyo3(get, set)]
    pub tick_number: u64,
    #[pyo3(get, set)]
    pub content_hash: Vec<u8>,
    #[pyo3(get, set)]
    pub signature: Vec<u8>,
    #[pyo3(get, set)]
    pub tbid: Vec<u8>,
    #[pyo3(get, set)]
    pub echo: String,
    #[pyo3(get, set)]
    pub tbn: String,
    #[pyo3(get, set)]
    pub time_being_reference_time: String,
}

impl From<&FortisInner> for PyFortis {
    fn from(f: &FortisInner) -> Self {
        Self {
            tick_number: f.tick_number,
            content_hash: f.content_hash.to_vec(),
            signature: f.signature.clone(),
            tbid: f.tbid.to_vec(),
            echo: f.echo.clone(),
            tbn: f.tbn.clone(),
            time_being_reference_time: f.time_being_reference_time.clone(),
        }
    }
}

#[pymethods]
impl PyFortis {
    /// Create an empty PyFortis (for constructing from JSON in Python).
    #[new]
    fn new() -> Self {
        Self {
            tick_number: 0,
            content_hash: Vec::new(),
            signature: Vec::new(),
            tbid: Vec::new(),
            echo: String::new(),
            tbn: String::new(),
            time_being_reference_time: String::new(),
        }
    }

    /// Construct a PyFortis from a JSON string.
    #[classmethod]
    fn from_json(_cls: &Bound<'_, PyType>, json_str: &str) -> PyResult<Self> {
        serde_json::from_str(json_str)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "Fortis(tick={}, tbn={}, echo={})",
            self.tick_number, self.tbn, self.echo
        )
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Python-facing TickRecord.
#[pyclass]
#[derive(Clone, Serialize)]
pub struct PyTickRecord {
    #[pyo3(get)]
    pub tick_number: u64,
    #[pyo3(get)]
    pub public_key: Vec<u8>,
    #[pyo3(get)]
    pub forward_fortis: Vec<u8>,
    #[pyo3(get)]
    pub backward_fortis: Vec<u8>,
    #[pyo3(get)]
    pub aa_nonce: Vec<u8>,
}

impl From<&TickRecordInner> for PyTickRecord {
    fn from(t: &TickRecordInner) -> Self {
        Self {
            tick_number: t.tick_number,
            public_key: t.public_key.clone(),
            forward_fortis: t.forward_fortis.clone(),
            backward_fortis: t.backward_fortis.clone(),
            aa_nonce: t.aa_nonce.to_vec(),
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
            "ed25519" => FortiasCurve::Ed25519,
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
        let pub_key = FortiasPubKey32 { bytes: pub_key_bytes };
        let sig_bytes: [u8; 64] = sig.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("sig must be exactly 64 bytes"))?;
        let sig = FortiasSig64 { bytes: sig_bytes };
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
        let server = crypto_server::new_software(FortiasCurve::Ed25519)
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

    /// Stamp content, producing a Fortis and appending to calendar.
    #[pyo3(signature = (content, echo = ""))]
    fn stamp(&self, content: &[u8], echo: &str) -> PyResult<PyFortis> {
        let mut tick = self.current_tick.lock();
        *tick += 1;
        let tick_number = *tick;

        let fortis = fortias::tick::stamp(
            self.server.as_ref(),
            &self.tbid,
            tick_number,
            content,
            echo,
            &self.tbn,
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let pub_key_bytes = pubkey_to_vec(self.server.public_key());
        let record = TickRecordInner {
            tick_number,
            public_key: pub_key_bytes,
            forward_fortis: serde_json::to_string(&fortis).unwrap_or_default().into_bytes(),
            backward_fortis: Vec::new(),
            aa_nonce: [0u8; 16],
            external_attestations: Vec::new(),
        };

        let mut cal = self.calendar.write();
        cal.append(record)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(PyFortis::from(&fortis))
    }

    /// Verify content against a Fortis stamp.
    fn verify(&self, content: &[u8], fortis: &PyFortis) -> PyResult<bool> {
        let content_hash: [u8; 32] = fortis.content_hash[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("content_hash must be 32 bytes"))?;
        let tbid: [u8; 16] = fortis.tbid[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("tbid must be 16 bytes"))?;

        let inner_fortis = FortisInner {
            tick_number: fortis.tick_number,
            content_hash,
            signature: fortis.signature.clone(),
            tbid,
            echo: fortis.echo.clone(),
            tbn: fortis.tbn.clone(),
            time_being_reference_time: fortis.time_being_reference_time.clone(),
        };

        let cal = self.calendar.read();
        fortias::tick::verify(self.server.as_ref(), &inner_fortis, content, &*cal)
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
    server: Arc<fortias_node::server::TimeFamilyServer>,
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
            fortias_node::server::TimeFamilyServer::new_with_persist(
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
            fortias_node::server::TimeFamilyServer::from_calendar(
                &calendar_path,
                listen_addr,
            ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?,
        );
        Ok(Self { server })
    }

    /// Stamp content, producing a Fortis attestation.
    ///
    /// Args:
    ///     content: Raw bytes to attesting.
    ///     echo: Optional echo string.
    ///
    /// Returns:
    ///     PyFortis attestation record.
    ///
    /// Raises RuntimeError if the server is in dormant mode.
    #[pyo3(signature = (content, echo = ""))]
    fn stamp(&self, content: &[u8], echo: &str) -> PyResult<PyFortis> {
        let fortis = self.server.chronomatter().stamp(content.to_vec(), echo.to_string())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(PyFortis::from(&fortis))
    }

    /// Verify a Fortis attestation against content.
    ///
    /// Args:
    ///     content: Raw bytes to verify.
    ///     fortis: The Fortis attestation to verify.
    ///
    /// Returns:
    ///     True if the attestation is valid.
    fn verify(&self, content: &[u8], fortis: &PyFortis) -> PyResult<bool> {
        let content_hash: [u8; 32] = fortis.content_hash[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("content_hash must be 32 bytes"))?;
        let tbid: [u8; 16] = fortis.tbid[..].try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("tbid must be 16 bytes"))?;

        let inner_fortis = FortisInner {
            tick_number: fortis.tick_number,
            content_hash,
            signature: fortis.signature.clone(),
            tbid,
            echo: fortis.echo.clone(),
            tbn: fortis.tbn.clone(),
            time_being_reference_time: fortis.time_being_reference_time.clone(),
        };

        let result = self.server.chronomatter().verify(
            &inner_fortis,
            &content.to_vec(),
            &*self.server.calendar().inner().read(),
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(result)
    }

    /// Verify a Fortis attestation from a JSON string.
    ///
    /// Args:
    ///     content: Raw bytes to verify.
    ///     fortis_json: JSON string of the Fortis attestation.
    ///
    /// Returns:
    ///     True if the attestation is valid.
    fn verify_json(&self, content: &[u8], fortis_json: &str) -> PyResult<bool> {
        let fortis: FortisInner = serde_json::from_str(fortis_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let result = self.server.chronomatter().verify(
            &fortis,
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
        0
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
        Ok(String::new())
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

/// Fortias P2P Python module.
#[pymodule]
fn fortias_p2p(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCryptoServer>()?;
    m.add_class::<PyFortis>()?;
    m.add_class::<PyTickRecord>()?;
    m.add_class::<PyCalendar>()?;
    m.add_class::<PyTimeFamily>()?;
    m.add_class::<PyTimeFamilyServer>()?;
    Ok(())
}
