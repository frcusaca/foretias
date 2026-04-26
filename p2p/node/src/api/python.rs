//! PyO3 Python bindings for Fortias v0.1.
//!
//! Exposes: CryptoServer, Fortis, TickRecord, Calendar, TimeFamily.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(missing_docs)]

use pyo3::prelude::*;
use serde::Serialize;

use crate::crypto_server::{self, CryptoServer, FortiasCurve, PublicKeyBytes};
use crate::fortias::{self, calendar::Calendar as CalendarInner, tick::TickRecord as TickRecordInner};
use crate::fortias::tick::Fortis as FortisInner;
use crate::core::bindings::{FortiasPubKey32, FortiasSig64};

/// Python-facing Fortis stamp.
#[pyclass]
#[derive(Clone, Serialize)]
pub struct PyFortis {
    #[pyo3(get)]
    pub tick_number: u64,
    #[pyo3(get)]
    pub content_hash: Vec<u8>,
    #[pyo3(get)]
    pub signature: Vec<u8>,
    #[pyo3(get)]
    pub tbid: Vec<u8>,
    #[pyo3(get)]
    pub echo: String,
    #[pyo3(get)]
    pub tbn: String,
    #[pyo3(get)]
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
}

impl From<&TickRecordInner> for PyTickRecord {
    fn from(t: &TickRecordInner) -> Self {
        Self {
            tick_number: t.tick_number,
            public_key: t.public_key.clone(),
            forward_fortis: t.forward_fortis.clone(),
            backward_fortis: t.backward_fortis.clone(),
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
        let pub_key_bytes: [u8; 32] = pub_key.try_into().unwrap();
        let pub_key = FortiasPubKey32 { bytes: pub_key_bytes };
        let sig_bytes: [u8; 64] = sig.try_into().unwrap();
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
    fn stamp(&self, content: &[u8]) -> PyResult<PyFortis> {
        let mut tick = self.current_tick.lock();
        *tick += 1;
        let tick_number = *tick;

        let fortis = fortias::tick::stamp(
            self.server.as_ref(),
            &self.tbid,
            tick_number,
            content,
            "",
            &self.tbn,
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let pub_key_bytes = pubkey_to_vec(self.server.public_key());
        let record = TickRecordInner {
            tick_number,
            public_key: pub_key_bytes,
            forward_fortis: serde_json::to_string(&fortis).unwrap_or_default().into_bytes(),
            backward_fortis: Vec::new(),
        };

        let mut cal = self.calendar.write();
        cal.append(record);

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

/// Fortias P2P Python module.
#[pymodule]
fn fortias_p2p(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCryptoServer>()?;
    m.add_class::<PyFortis>()?;
    m.add_class::<PyTickRecord>()?;
    m.add_class::<PyCalendar>()?;
    m.add_class::<PyTimeFamily>()?;
    Ok(())
}
