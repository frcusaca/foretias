//! Chronomatter — autonomous ticking, stamping, and verification.
//!
//! Chronomatter (*Chronos fidelis authenticus*) is a time being with its own TBID.
//! It owns autonomous tick timer, per-tick keypair generation, stamping, and verification.
//! Calendar receives tick notifications via `TickObserver` callback.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};

use parking_lot::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::crypto_server::{self, CryptoServer};
use crate::core::identity::PrivKeyHandle;
use crate::error::NodeError;
use crate::fortias::{auto_attestation_blob, Calendar, Fortis, TickRecord};
use crate::fortias::tick::CalendarLookup;
use crate::fortias::callbacks::TickObserver;
use crate::fortias::types::{Tbid, PublicKey, TickNumber};

struct TickKeyPair {
    pub_key: [u8; 32],
    priv_key: PrivKeyHandle,
}

pub struct Chronomatter {
    tbid: Tbid,
    tbn: String,
    current_tick: AtomicU64,
    keypairs: RwLock<Vec<TickKeyPair>>,
    crypto: Arc<dyn CryptoServer>,
    calendar: Arc<RwLock<Calendar>>,
    tick_observer: Arc<dyn TickObserver>,
    chronon_ns: u64,
    daemon_handle: Mutex<Option<JoinHandle<()>>>,
    is_dormant: bool,
}

impl Chronomatter {
    pub fn new(chronon_ns: u64, tick_observer: Arc<dyn TickObserver>) -> Result<Self, NodeError> {
        let crypto = Arc::from(crypto_server::new_software(
            crypto_server::FortiasCurve::Ed25519,
        )?);

        let uuid = uuid::Uuid::new_v4();
        let tbid = *uuid.as_bytes();
        let tbn = format!("tf-{}", hex::encode(&tbid[..8]));
        let calendar = Arc::new(RwLock::new(Calendar::new(tbid, &tbn)));

        Ok(Self {
            tbid,
            tbn,
            current_tick: AtomicU64::new(0),
            keypairs: RwLock::new(Vec::new()),
            crypto,
            calendar,
            tick_observer,
            chronon_ns,
            daemon_handle: Mutex::new(None),
            is_dormant: false,
        })
    }

    /// Create from a persisted calendar (dormant, verify-only mode).
    pub fn from_calendar(
        path: &str,
        crypto: Arc<dyn CryptoServer>,
        tick_observer: Arc<dyn TickObserver>,
    ) -> Result<Self, NodeError> {
        let calendar_data = Calendar::load(path)?;
        let latest_tick = CalendarLookup::latest(&calendar_data).unwrap_or(0);
        let loaded_tbid = calendar_data.tbid;
        let loaded_tbn = calendar_data.tbn.clone();
        let calendar = Arc::new(RwLock::new(calendar_data));

        Ok(Self {
            tbid: loaded_tbid,
            tbn: loaded_tbn,
            current_tick: AtomicU64::new(latest_tick),
            keypairs: RwLock::new(Vec::new()),
            crypto,
            calendar,
            tick_observer,
            chronon_ns: 0,
            daemon_handle: Mutex::new(None),
            is_dormant: true,
        })
    }

    /// Returns true if this Chronomatter is in dormant (verify-only) mode.
    pub fn is_dormant(&self) -> bool {
        self.is_dormant
    }

    /// Returns the TBID of this Chronomatter.
    pub fn get_tbid(&self) -> Tbid {
        self.tbid
    }

    /// Returns the TimeBeing name.
    pub fn get_tbn(&self) -> &str {
        &self.tbn
    }

    /// Returns the current tick number.
    pub fn current_tick(&self) -> u64 {
        self.current_tick.load(SeqCst)
    }

    /// Returns a reference to the calendar.
    pub fn calendar(&self) -> &Arc<RwLock<Calendar>> {
        &self.calendar
    }

    /// Generate a new keypair and store it, returning its index.
    fn generate_and_store_keypair(&self) -> Result<usize, NodeError> {
        let handle = PrivKeyHandle::generate()
            .map_err(|e| NodeError::Crypto(e))?;
        let pub_key = handle.public_key().map_err(|e| NodeError::Crypto(e))?;
        let kp = TickKeyPair { pub_key, priv_key: handle };
        let mut keypairs = self.keypairs.write();
        let idx = keypairs.len();
        keypairs.push(kp);
        Ok(idx)
    }

    /// Sign a message with the keypair at the given index.
    fn sign_with_keypair(&self, idx: usize, msg: &[u8]) -> Result<Vec<u8>, NodeError> {
        let keypairs = self.keypairs.read();
        let kp = keypairs.get(idx).ok_or_else(|| {
            NodeError::Internal(format!("keypair index {} out of range", idx))
        })?;
        let sig = kp.priv_key.sign(msg)
            .map_err(|e| NodeError::Crypto(e))?;
        Ok(sig.bytes.to_vec())
    }

    /// Get public key for keypair at index.
    fn keypair_pub(&self, idx: usize) -> Option<[u8; 32]> {
        self.keypairs.read().get(idx).map(|kp| kp.pub_key)
    }

    // ── Stamp ────────────────────────────────────────────────────────────────

    /// Stamp content, creating a Fortis attestation.
    ///
    /// Advances the tick, generates a new keypair, creates the TickRecord,
    /// and appends it to the calendar.
    pub fn stamp(&self, content: Vec<u8>, echo: String) -> Result<Fortis, NodeError> {
        if self.is_dormant {
            return Err(NodeError::Internal(
                "Cannot stamp: Chronomatter is in verify-only mode".into(),
            ));
        }

        let tick = {
            let old = self.current_tick.load(SeqCst);
            self.current_tick.compare_exchange(
                old, old + 1, SeqCst, SeqCst,
            ).map_err(|e| NodeError::Internal(format!("tick counter conflict: {}", e)))?;
            old + 1
        };

        self.create_fortis(tick, content, echo)
    }

    fn create_fortis(&self, tick: u64, content: Vec<u8>, echo: String) -> Result<Fortis, NodeError> {
        let tbid = self.tbid;
        let tbn = self.tbn.clone();
        let tbid_str = hex::encode(tbid);

        let kp_idx = self.generate_and_store_keypair()?;
        let new_pub = self.keypair_pub(kp_idx).unwrap();

        // Build auto-attestation
        let (forward_fortis, backward_fortis, aa_nonce) = if self.calendar.read().ticks.is_empty() {
            let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, tick, &new_pub, tick, &new_pub)?;
            let sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (sig.clone(), sig, nonce)
        } else {
            let cal = self.calendar.read();
            let latest_rec = cal.ticks.last().unwrap();
            let prev_tick = latest_rec.tick_number;
            let prev_kp_idx = (prev_tick - 1) as usize;
            let prev_pub = self.keypair_pub(prev_kp_idx).unwrap();

            let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, prev_tick, &prev_pub, tick, &new_pub)?;
            let forward_sig = self.sign_with_keypair(prev_kp_idx, &ma_blob)?;
            let backward_sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (forward_sig, backward_sig, nonce)
        };

        // Sign the content
        let mut sig_input = Vec::with_capacity(16 + 8 + content.len());
        sig_input.extend_from_slice(&tbid);
        sig_input.extend_from_slice(&tick.to_be_bytes());
        sig_input.extend_from_slice(&content);

        let sig = self.keypairs.read().get(kp_idx).unwrap().priv_key
            .sign(&sig_input)
            .map_err(|e| NodeError::Crypto(e))?;

        let content_hash = self.crypto.sha256(&content)?;

        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| NodeError::Internal(format!("SystemTime before UNIX_EPOCH: {}", e)))?
            .as_nanos() as u64;
        let time_being_reference_time = format!("UE+{}ns", now_ns);

        let fortis = Fortis {
            tick_number: tick,
            content_hash: content_hash.bytes,
            signature: sig.bytes.to_vec(),
            tbid,
            echo,
            tbn,
            time_being_reference_time,
        };

        let record = TickRecord {
            tick_number: tick,
            public_key: new_pub.to_vec(),
            forward_fortis,
            backward_fortis,
            aa_nonce,
            external_attestations: Vec::new(),
        };

        self.calendar.write().append(record)?;

        // Notify observers
        let pk = new_pub;
        self.tick_observer.on_tick_advance(tick, &pk);

        Ok(fortis)
    }

    // ── Verify ───────────────────────────────────────────────────────────────

    /// Verify a Fortis attestation against content and calendar.
    pub fn verify(&self, fortis: &Fortis, content: &Vec<u8>) -> Result<bool, NodeError> {
        crate::fortias::tick::verify(
            self.crypto.as_ref(),
            fortis,
            content,
            &*self.calendar.read(),
        )
    }

    // ── Daemon Tick Loop ────────────────────────────────────────────────────

    /// Start the autonomous tick daemon.
    pub fn start_daemon(self: &Arc<Self>) {
        if self.chronon_ns == 0 {
            warn!("chronon_ns is 0, daemon will not start");
            return;
        }

        let chronon_ms = self.chronon_ns / 1_000_000;
        let duration = tokio::time::Duration::from_millis(
            if chronon_ms > 0 { chronon_ms } else { 1 },
        );
        let this = Arc::clone(self);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(duration);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                debug!("chronomatter daemon tick");
                if let Err(e) = this.daemon_tick() {
                    error!("daemon tick failed: {}", e);
                }
            }
        });

        let mut daemon = self.daemon_handle.lock();
        *daemon = Some(handle);
        info!("chronomatter daemon started with chronon interval: {} ms", chronon_ms);
    }

    /// Stop the daemon.
    pub fn stop_daemon(&self) {
        let mut daemon = self.daemon_handle.lock();
        if let Some(handle) = daemon.take() {
            handle.abort();
            info!("chronomatter daemon stopped");
        }
    }

    /// Create a daemon tick (no content) and append to calendar.
    pub fn daemon_tick(&self) -> Result<(), NodeError> {
        if self.is_dormant {
            return Ok(());
        }

        let tick = {
            let old = self.current_tick.load(SeqCst);
            self.current_tick.compare_exchange(
                old, old + 1, SeqCst, SeqCst,
            ).map_err(|e| NodeError::Internal(format!("tick counter conflict: {}", e)))?;
            old + 1
        };

        let tbid = self.tbid;
        let tbid_str = hex::encode(tbid);

        let kp_idx = self.generate_and_store_keypair()?;
        let new_pub = self.keypair_pub(kp_idx).unwrap();

        let (forward_fortis, backward_fortis, aa_nonce) = if self.calendar.read().ticks.is_empty() {
            let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, tick, &new_pub, tick, &new_pub)?;
            let sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (sig.clone(), sig, nonce)
        } else {
            let cal = self.calendar.read();
            let latest_rec = cal.ticks.last().unwrap();
            let prev_tick = latest_rec.tick_number;
            let prev_kp_idx = (prev_tick - 1) as usize;
            let prev_pub = self.keypair_pub(prev_kp_idx).unwrap();

            let (ma_blob, nonce) = auto_attestation_blob(&tbid_str, prev_tick, &prev_pub, tick, &new_pub)?;
            let forward_sig = self.sign_with_keypair(prev_kp_idx, &ma_blob)?;
            let backward_sig = self.sign_with_keypair(kp_idx, &ma_blob)?;
            (forward_sig, backward_sig, nonce)
        };

        let record = TickRecord {
            tick_number: tick,
            public_key: new_pub.to_vec(),
            forward_fortis,
            backward_fortis,
            aa_nonce,
            external_attestations: Vec::new(),
        };

        self.calendar.write().append(record)?;

        // Notify observers
        let pk = new_pub;
        self.tick_observer.on_tick_advance(tick, &pk);

        Ok(())
    }

    // ── Integrity Check ─────────────────────────────────────────────────────

    /// Verify chain integrity over a tick range.
    pub fn integrity_check(
        &self,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Vec<bool>, NodeError> {
        self.calendar.read().integrity_check(
            self.crypto.as_ref(),
            &hex::encode(self.tbid),
            start,
            end,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64 as AtomicU64Std;

    /// Dummy TickObserver that records tick advances.
    struct DummyObserver {
        last_tick: Arc<AtomicU64Std>,
    }
    impl TickObserver for DummyObserver {
        fn on_tick_advance(&self, tick_number: TickNumber, _public_key: &PublicKey) {
            self.last_tick.store(tick_number, SeqCst);
        }
    }

    fn make_chronomatter() -> (Arc<Chronomatter>, Arc<AtomicU64Std>) {
        let last_tick = Arc::new(AtomicU64Std::new(0));
        let observer = Arc::new(DummyObserver { last_tick: Arc::clone(&last_tick) });
        let cm = Chronomatter::new(1_000_000_000, observer)
            .expect("failed to create Chronomatter");
        (Arc::new(cm), last_tick)
    }

    #[test]
    fn stamp_returns_fortis_with_correct_tick() {
        let (cm, _last) = make_chronomatter();
        let fortis = cm.stamp(b"hello".to_vec(), "echo".to_string()).unwrap();
        assert_eq!(fortis.tick_number, 1);
        assert_eq!(fortis.echo, "echo");
        assert!(!fortis.signature.is_empty());
    }

    #[test]
    fn two_stamps_share_same_tick_if_no_daemon_advance() {
        let (cm, _last) = make_chronomatter();
        let f1 = cm.stamp(b"one".to_vec(), "e1".to_string()).unwrap();
        let f2 = cm.stamp(b"two".to_vec(), "e2".to_string()).unwrap();
        // Each stamp advances tick, so they should be different
        assert_eq!(f1.tick_number, 1);
        assert_eq!(f2.tick_number, 2);
    }

    #[test]
    fn verify_succeeds_for_valid_stamp() {
        let (cm, _last) = make_chronomatter();
        let content = b"verify-me".to_vec();
        let fortis = cm.stamp(content.clone(), "v".to_string()).unwrap();
        let valid = cm.verify(&fortis, &content).unwrap();
        assert!(valid);
    }

    #[test]
    fn verify_fails_for_wrong_content() {
        let (cm, _last) = make_chronomatter();
        let fortis = cm.stamp(b"original".to_vec(), "v".to_string()).unwrap();
        let valid = cm.verify(&fortis, &b"tampered".to_vec()).unwrap();
        assert!(!valid);
    }

    #[test]
    fn daemon_tick_advances_and_notifies_observer() {
        let (cm, last_tick) = make_chronomatter();
        cm.daemon_tick().unwrap();
        assert_eq!(cm.current_tick(), 1);
        assert_eq!(last_tick.load(SeqCst), 1);
    }

    #[test]
    fn keypair_changes_on_each_tick() {
        let (cm, _last) = make_chronomatter();
        cm.daemon_tick().unwrap();
        let pk1 = { let cal = cm.calendar.read(); cal.ticks[0].public_key.clone() };
        cm.daemon_tick().unwrap();
        let pk2 = { let cal = cm.calendar.read(); cal.ticks[1].public_key.clone() };
        assert_ne!(pk1, pk2, "keypairs should differ between ticks");
    }

    #[test]
    fn dormant_cannot_stamp() {
        let last_tick = Arc::new(AtomicU64Std::new(0));
        let observer = Arc::new(DummyObserver { last_tick: Arc::clone(&last_tick) });
        let crypto = Arc::from(crypto_server::new_software(
            crypto_server::FortiasCurve::Ed25519,
        ).unwrap());
        // Create a temp calendar file
        let tmp = std::env::temp_dir().join(format!("chronomatter_test_{}.json", std::process::id()));
        {
            let cm = Chronomatter::new(1_000_000_000, observer.clone()).unwrap();
            cm.stamp(b"init".to_vec(), "init".to_string()).unwrap();
            std::fs::write(&tmp, serde_json::to_string(&*cm.calendar.read()).unwrap()).unwrap();
        }
        let cm = Chronomatter::from_calendar(
            tmp.to_str().unwrap(),
            crypto,
            observer,
        ).unwrap();
        assert!(cm.is_dormant());
        let result = cm.stamp(b"should-fail".to_vec(), "e".to_string());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn integrity_check_empty_calendar() {
        let (cm, _last) = make_chronomatter();
        let results = cm.integrity_check(None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_after_ticks() {
        let (cm, _last) = make_chronomatter();
        for _ in 0..3 {
            cm.daemon_tick().unwrap();
        }
        let results = cm.integrity_check(None, None).unwrap();
        assert_eq!(results.len(), 2); // 3 ticks → 2 pairs
        assert!(results.iter().all(|&v| v));
    }
}
