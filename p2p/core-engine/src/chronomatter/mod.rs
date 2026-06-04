//! Chronomatter — autonomous ticking, stamping, and verification.
//!
//! Chronomatter (*Chronos fidelis authenticus*) is a time being with its own TBID.
//! It owns autonomous tick timer, per-tick keypair generation, stamping, and verification.
//! Calendar receives tick notifications via `TickObserver` callback.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::clock::Clock;
use crate::core::identity::PrivKeyHandle;
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use crate::foretias::callbacks::{MutualAttestObserver, TickObserver};
use crate::foretias::tick::CalendarLookup;
use crate::foretias::tick::StampedForetis;
use crate::foretias::types::{Tbid, TbidSecret, TickNumber};
use crate::foretias::{
    auto_attestation_blob_with_count, auto_attestation_blob_with_genesis, ChrononRecord, Foretis,
};

struct TickKeyPair {
    pub_key: [u8; 32],
    priv_key: PrivKeyHandle,
}

pub struct Chronomatter {
    tbid: Tbid,
    /// Dual-key TBID secret — used for genesis tick signing and Ed25519 tick key derivation.
    tbid_secret: Option<TbidSecret>,
    tbn: String,
    current_tick: AtomicU64,
    keypairs: RwLock<Vec<TickKeyPair>>,
    crypto: Arc<dyn CryptoServer>,
    clock: Arc<dyn Clock>,
    tick_observer: Arc<dyn TickObserver>,
    mutual_attest_observer: Option<Arc<dyn MutualAttestObserver>>,
    chronon_ns: u64,
    daemon_handle: Mutex<Option<JoinHandle<()>>>,
    is_dormant: AtomicBool,
    /// Per-tick stamp counter — counts user-initiated stamps since last tick advance.
    /// Does not include auto-attestation itself, but may include mutual attestations.
    chronon_stamp_count: AtomicU64,
}

impl Chronomatter {
    pub fn new(
        chronon_ns: u64,
        tick_observer: Arc<dyn TickObserver>,
        crypto: Arc<dyn CryptoServer>,
    ) -> Result<Self, NodeError> {
        let (tbid, tbid_secret) = TbidSecret::generate().map_err(NodeError::Crypto)?;
        let tbn = format!("tf-{}", &tbid.to_hex()[..16]);

        Ok(Self {
            tbid,
            tbid_secret: Some(tbid_secret),
            tbn,
            current_tick: AtomicU64::new(0),
            keypairs: RwLock::new(Vec::new()),
            crypto,
            clock: Arc::new(crate::clock::SystemClock),
            tick_observer,
            mutual_attest_observer: None,
            chronon_ns,
            daemon_handle: Mutex::new(None),
            is_dormant: AtomicBool::new(false),
            chronon_stamp_count: AtomicU64::new(0),
        })
    }

    /// Create from a persisted calendar (dormant, verify-only mode).
    ///
    /// The calendar is loaded for verification, but this Chronomatter
    /// gets its own unique TBID — a dormant time being never reuses
    /// the original TBID from the loaded calendar.
    pub fn from_calendar(
        path: &str,
        crypto: Arc<dyn CryptoServer>,
        tick_observer: Arc<dyn TickObserver>,
    ) -> Result<Self, NodeError> {
        use crate::foretias::Calendar;
        let calendar_data = Calendar::load(path)?;
        let latest_tick = CalendarLookup::latest(&calendar_data).unwrap_or(0);

        let (tbid, _tbid_secret) = TbidSecret::generate().map_err(NodeError::Crypto)?;
        let tbn = format!("tf-{}", &tbid.to_hex()[..16]);

        Ok(Self {
            tbid,
            tbid_secret: None,
            tbn,
            current_tick: AtomicU64::new(latest_tick),
            keypairs: RwLock::new(Vec::new()),
            crypto,
            clock: Arc::new(crate::clock::SystemClock),
            tick_observer,
            mutual_attest_observer: None,
            chronon_ns: 0,
            daemon_handle: Mutex::new(None),
            is_dormant: AtomicBool::new(true),
            chronon_stamp_count: AtomicU64::new(0),
        })
    }

    pub fn is_dormant(&self) -> bool {
        self.is_dormant.load(SeqCst)
    }

    pub fn set_dormant(&self, dormant: bool) {
        self.is_dormant.store(dormant, SeqCst);
    }

    pub fn crypto_server(&self) -> Arc<dyn CryptoServer> {
        Arc::clone(&self.crypto)
    }

    pub fn get_tbid(&self) -> Tbid {
        self.tbid
    }

    pub fn get_tbn(&self) -> &str {
        &self.tbn
    }

    pub fn current_tick(&self) -> u64 {
        self.current_tick.load(SeqCst)
    }

    /// Return the chronon interval in nanoseconds.
    pub fn chronon_ns(&self) -> u64 {
        self.chronon_ns
    }

    /// Sign a message with this TBID's secret key (Ed25519 + SLH-DSA combined).
    ///
    /// Returns Ed25519_SIG(64) ‖ SLH-DSA_SIG(49856) = 49920 bytes.
    /// Used by channel-binding and authenticated-ping handlers to prove TBID ownership.
    /// // SIGN(local-tbid, fast-key+slow-key)
    pub fn sign_tbid_message(
        &self,
        msg: &[u8],
    ) -> Result<crate::foretias::types::SignatureBytes, crate::error::NodeError> {
        self.tbid_secret
            .as_ref()
            .ok_or(crate::error::NodeError::Crypto(
                crate::error::CryptoError::BadInput("no TBID signing key"),
            ))?
            .sign(msg)
            .map_err(crate::error::NodeError::Crypto)
    }

    /// Return the public key of the most recently generated tick keypair, if any.
    pub fn latest_public_key(&self) -> Option<[u8; 32]> {
        self.keypairs.read().last().map(|kp| kp.pub_key)
    }

    /// Set the mutual-attestation observer (for metrics).
    pub fn set_mutual_attest_observer(&mut self, observer: Arc<dyn MutualAttestObserver>) {
        self.mutual_attest_observer = Some(observer);
    }

    fn generate_and_store_keypair(&self) -> Result<(), NodeError> {
        let handle = PrivKeyHandle::generate().map_err(NodeError::Crypto)?;
        let pub_key = handle.public_key().map_err(NodeError::Crypto)?;
        let kp = TickKeyPair {
            pub_key,
            priv_key: handle,
        };
        let mut keypairs = self.keypairs.write();
        keypairs.push(kp);
        // REQ-Z4.3: retain only current + previous; evict older.
        while keypairs.len() > 2 {
            let _ = keypairs.remove(0);
        }
        Ok(())
    }

    fn sign_with_keypair(&self, relative_idx: usize, msg: &[u8]) -> Result<Vec<u8>, NodeError> {
        let keypairs = self.keypairs.read();
        let len = keypairs.len();
        let idx = len.checked_sub(relative_idx + 1).ok_or_else(|| {
            NodeError::Internal(format!(
                "keypair relative index {} out of range (len={})",
                relative_idx, len
            ))
        })?;
        let kp = keypairs
            .get(idx)
            .ok_or_else(|| NodeError::Internal(format!("keypair index {} out of range", idx)))?;
        let sig = kp.priv_key.sign(msg).map_err(NodeError::Crypto)?;
        Ok(sig.bytes.to_vec())
    }

    fn keypair_pub(&self, relative_idx: usize) -> Option<[u8; 32]> {
        let keypairs = self.keypairs.read();
        let len = keypairs.len();
        let idx = len.checked_sub(relative_idx + 1)?;
        keypairs.get(idx).map(|kp| kp.pub_key)
    }

    fn build_auto_attestation(
        &self,
        tick: u64,
        new_pub: [u8; 32],
    ) -> Result<(Vec<u8>, Vec<u8>, [u8; 16], u64), NodeError> {
        let tbid_str = self.tbid.to_hex();
        let stamps = self.chronon_stamp_count.swap(0, SeqCst);

        if tick == 1 {
            let (attest_blob, nonce) = auto_attestation_blob_with_count(
                &tbid_str, tick, &new_pub, tick, &new_pub, stamps,
            )?;
            let sig = self.sign_with_keypair(0, &attest_blob)?;
            Ok((sig.clone(), sig, nonce, stamps))
        } else {
            let prev_pub = self
                .keypair_pub(1)
                .ok_or_else(|| NodeError::Internal("missing previous keypair".into()))?;

            let (attest_blob, nonce) = auto_attestation_blob_with_count(
                &tbid_str,
                tick - 1,
                &prev_pub,
                tick,
                &new_pub,
                stamps,
            )?;
            let forward_sig = self.sign_with_keypair(1, &attest_blob)?;
            let backward_sig = self.sign_with_keypair(0, &attest_blob)?;
            Ok((forward_sig, backward_sig, nonce, stamps))
        }
    }

    fn build_tick_record(&self, tick: u64, new_pub: [u8; 32]) -> Result<ChrononRecord, NodeError> {
        let alg = self.crypto.signature_algorithm().to_id_string().to_string();
        if tick == 1 {
            if let Some(ref obs) = self.mutual_attest_observer {
                obs.on_mutual_attest_sent();
            }

            let tb_version;
            let (forward_foretis, backward_foretis, aa_nonce, stamps);

            if let Some(secret) = &self.tbid_secret {
                let mut genesis_blob = Vec::with_capacity(96 + 8 + new_pub.len());
                genesis_blob.extend_from_slice(&self.tbid.raw_bytes());
                genesis_blob.extend_from_slice(&tick.to_be_bytes());
                genesis_blob.extend_from_slice(&new_pub);
                let genesis_sig = secret.sign(&genesis_blob).map_err(NodeError::Crypto)?;
                info!(
                    genesis_blob_len = genesis_blob.len(),
                    sig_len = genesis_sig.len(),
                    "signed genesis tick"
                );
                tb_version = 1u32;

                let stamps_count = self.chronon_stamp_count.swap(0, SeqCst);
                let tbid_str = self.tbid.to_hex();
                let (attest_blob, nonce) = auto_attestation_blob_with_genesis(
                    &tbid_str,
                    tick,
                    &new_pub,
                    &genesis_sig,
                    stamps_count,
                )?;

                let kp_idx = 0usize;
                let ed_sig = self.sign_with_keypair(kp_idx, &attest_blob)?;

                let mut forward = Vec::with_capacity(ed_sig.len() + genesis_sig.len());
                forward.extend_from_slice(&ed_sig);
                forward.extend_from_slice(&genesis_sig);
                let mut backward = Vec::with_capacity(ed_sig.len() + genesis_sig.len());
                backward.extend_from_slice(&ed_sig);
                backward.extend_from_slice(&genesis_sig);

                forward_foretis = forward;
                backward_foretis = backward;
                aa_nonce = nonce;
                stamps = stamps_count;
            } else {
                warn!("no TBID secret available for genesis tick signing");
                let result = self.build_auto_attestation(tick, new_pub);
                if let Some(ref obs) = self.mutual_attest_observer {
                    match &result {
                        Ok(_) => obs.on_mutual_attest_ok(),
                        Err(_) => obs.on_mutual_attest_failed(),
                    }
                }
                let (fwd, bwd, nonce, stamps_count) = result?;
                tb_version = 0u32;
                forward_foretis = fwd;
                backward_foretis = bwd;
                aa_nonce = nonce;
                stamps = stamps_count;
            }

            if let Some(ref obs) = self.mutual_attest_observer {
                obs.on_mutual_attest_ok();
            }

            Ok(ChrononRecord {
                chronon_number: tick,
                public_key: new_pub.to_vec().into(),
                signature_algorithm: alg.clone(),
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: aa_nonce.into(),
                chronon_stamp_count: stamps,
                external_attestations: Vec::new(),
                tb_version,
                tbid: self.tbid,
            })
        } else {
            if let Some(ref obs) = self.mutual_attest_observer {
                obs.on_mutual_attest_sent();
            }
            let result = self.build_auto_attestation(tick, new_pub);
            if let Some(ref obs) = self.mutual_attest_observer {
                match &result {
                    Ok(_) => obs.on_mutual_attest_ok(),
                    Err(_) => obs.on_mutual_attest_failed(),
                }
            }
            let (forward_foretis, backward_foretis, aa_nonce, stamps) = result?;

            Ok(ChrononRecord {
                chronon_number: tick,
                public_key: new_pub.to_vec().into(),
                signature_algorithm: alg,
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: aa_nonce.into(),
                chronon_stamp_count: stamps,
                external_attestations: Vec::new(),
                tb_version: 0u32,
                tbid: self.tbid,
            })
        }
    }

    fn notify_observer(&self, tick: u64, pk: &[u8; 32], record: &ChrononRecord) {
        self.tick_observer
            .on_tick_advance(TickNumber(tick), pk, record);
    }

    // ── Stamp ────────────────────────────────────────────────────────────────

    pub fn stamp(&self, content: Vec<u8>, echo: String) -> Result<StampedForetis, NodeError> {
        if self.is_dormant.load(SeqCst) {
            return Err(NodeError::Internal(
                "Cannot stamp: Chronomatter is in verify-only mode".into(),
            ));
        }

        let tick = self.current_tick.fetch_add(1, SeqCst) + 1;

        // Increment stamp counter — this count is included in the auto-attestation blob
        self.chronon_stamp_count.fetch_add(1, SeqCst);

        self.create_foretis(tick, content, echo)
    }

    fn create_foretis(
        &self,
        tick: u64,
        content: Vec<u8>,
        echo: String,
    ) -> Result<StampedForetis, NodeError> {
        use crate::foretias::tick::StampedForetis;

        let tbid = self.tbid;
        let tbn = self.tbn.clone();

        self.generate_and_store_keypair()?;
        let new_pub = self
            .keypair_pub(0)
            .ok_or_else(|| NodeError::Internal("keypair not found after generation".into()))?;

        let content_hash = self.crypto.sha256(&content)?;

        let now_ns = self
            .clock
            .now_ns()
            .map_err(|e| NodeError::Internal(format!("clock error: {e}")))?;
        let time_being_reference_time = format!("UE+{}ns", now_ns);

        // V2: Foretis payload is signature-free
        let foretis = Foretis {
            chronon_number: tick,
            content_hash: content_hash.bytes.into(),
            tbid,
            echo,
            tbn,
            time_being_reference_time,
        };

        // V2: sign postcard canonical bytes with the per-tick keypair
        let sig_input = postcard::to_allocvec(&foretis)
            .map_err(|e| NodeError::Internal(format!("postcard serialize error: {e}")))?;

        let signature_bytes = self.sign_with_keypair(0, &sig_input)?;
        let sig_alg = self.crypto.signature_algorithm().to_id_string().to_string();

        let record = self.build_tick_record(tick, new_pub)?;
        self.notify_observer(tick, &new_pub, &record);

        Ok(StampedForetis {
            foretis,
            signature_bytes,
            signature_algorithm: sig_alg,
        })
    }

    // ── Verify ───────────────────────────────────────────────────────────────

    pub fn verify(
        &self,
        foretis: &Foretis,
        signature: &[u8],
        signature_algorithm: &str,
        content: &[u8],
        calendar: &dyn CalendarLookup,
    ) -> Result<bool, NodeError> {
        crate::foretias::tick::verify(
            self.crypto.as_ref(),
            foretis,
            signature,
            signature_algorithm,
            content,
            calendar,
        )
    }

    /// Verify an unprocessed Foretis against the local calendar.
    ///
    /// Accepts `UnverifiedSignatureEnvelope<Foretis>` (parsed, not trusted) and returns
    /// `CleanAuthenticated<Foretis>` (authenticated + cleansed) on success.
    /// The calendar record for the matching chronon must exist and be trusted
    /// by construction (locally produced or previously verified).
    pub fn verify_unprocessed(
        &self,
        unprocessed: crate::foretias::clean_auth::UnverifiedSignatureEnvelope<
            crate::foretias::tick::Foretis,
        >,
        content: &[u8],
        calendar: &dyn CalendarLookup,
    ) -> Result<
        crate::foretias::clean_auth::CleanAuthenticated<crate::foretias::tick::Foretis>,
        NodeError,
    > {
        // Look up the calendar record for this chronon
        let records = calendar
            .get(unprocessed.inner().chronon_number, 1)
            .map_err(|e| NodeError::Internal(format!("calendar lookup failed: {e}")))?;
        let rec = records.first().ok_or_else(|| {
            NodeError::Internal(format!(
                "chronon {} not found in calendar for verification",
                unprocessed.inner().chronon_number
            ))
        })?;

        // Wrap the calendar record as CleanAuthenticated (trusted by construction)
        let clean_rec =
            crate::foretias::clean_auth::CleanAuthenticated::<ChrononRecord>::from_trusted(
                rec.clone(),
            );

        // Verify and authenticate
        unprocessed
            .into_clean_authenticated(self.crypto.as_ref(), content, &clean_rec)
            .map_err(|e| NodeError::Internal(format!("verification failed: {e}")))
    }

    // ── Daemon Tick Loop ────────────────────────────────────────────────────

    pub fn start_daemon(self: &Arc<Self>) {
        if self.chronon_ns == 0 {
            warn!("chronon_ns is 0, daemon will not start");
            return;
        }

        let chronon_ms = self.chronon_ns / 1_000_000;
        let duration =
            tokio::time::Duration::from_millis(if chronon_ms > 0 { chronon_ms } else { 1 });
        let this = Arc::clone(self);

        let tbid = this.get_tbid();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(duration);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                let tick_before = this.current_tick.load(SeqCst);
                debug!(tbid = %tbid.to_hex(), tick = tick_before, "chronomatter: heartbeat");
                if let Err(e) = this.daemon_tick() {
                    error!(tbid = %tbid.to_hex(), "chronomatter: daemon tick failed: {}", e);
                }
            }
        });

        let mut daemon = self.daemon_handle.lock();
        *daemon = Some(handle);
        info!(tbid = %self.tbid.to_hex(), component = "chronomatter", "chronomatter daemon started with chronon interval: {} ms", chronon_ms);
    }

    pub fn stop_daemon(&self) {
        let mut daemon = self.daemon_handle.lock();
        if let Some(handle) = daemon.take() {
            handle.abort();
            info!("chronomatter daemon stopped");
        }
    }

    pub fn daemon_tick(&self) -> Result<(), NodeError> {
        if self.is_dormant.load(SeqCst) {
            return Ok(());
        }

        let tick = self.current_tick.fetch_add(1, SeqCst) + 1;

        self.generate_and_store_keypair()?;
        let new_pub = self
            .keypair_pub(0)
            .ok_or_else(|| NodeError::Internal("keypair not found after generation".into()))?;

        let record = self.build_tick_record(tick, new_pub)?;
        self.notify_observer(tick, &new_pub, &record);
        info!(tick = tick, "Tick #{}", tick);

        Ok(())
    }

    // ── Integrity Check ─────────────────────────────────────────────────────

    pub fn integrity_check(
        &self,
        calendar: &dyn CalendarLookup,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Vec<bool>, NodeError> {
        use crate::foretias::tick::verify_pair;

        let records = calendar.get(start.unwrap_or(0), 10_000)?;
        if records.is_empty() {
            return Ok(Vec::new());
        }

        let start_tick = start.unwrap_or(0);
        let end_tick = end.unwrap_or(u64::MAX);

        let filtered: Vec<&ChrononRecord> = records
            .iter()
            .filter(|t| t.chronon_number >= start_tick && t.chronon_number <= end_tick)
            .collect();

        if filtered.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(filtered.len());

        // Verify auto-attestation pairs between consecutive ticks
        if filtered.len() >= 2 {
            let tbid_str = calendar.tbid().to_hex();
            for i in 0..filtered.len() - 1 {
                let valid = verify_pair(
                    self.crypto.as_ref(),
                    &tbid_str,
                    filtered[i],
                    filtered[i + 1],
                )?;
                results.push(valid);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use crate::foretias::types::TickNumber;
    use crate::foretias::Calendar;
    use parking_lot::RwLock;
    use std::sync::atomic::AtomicU64 as AtomicU64Std;

    struct DummyObserver {
        last_tick: Arc<AtomicU64Std>,
        calendar: Arc<RwLock<Calendar>>,
    }
    impl TickObserver for DummyObserver {
        fn on_tick_advance(
            &self,
            chronon_number: TickNumber,
            _public_key: &[u8],
            tick_record: &ChrononRecord,
        ) {
            self.last_tick.store(chronon_number.0, SeqCst);
            self.calendar.write().append(tick_record.clone()).unwrap();
        }
    }

    fn make_chronomatter() -> (Arc<Chronomatter>, Arc<AtomicU64Std>, Arc<RwLock<Calendar>>) {
        let last_tick = Arc::new(AtomicU64Std::new(0));
        let calendar = Arc::new(RwLock::new(Calendar::new(
            Tbid::from_raw([0u8; 96]),
            "test",
        )));
        let observer = Arc::new(DummyObserver {
            last_tick: Arc::clone(&last_tick),
            calendar: Arc::clone(&calendar),
        });
        let crypto: Arc<dyn crate::crypto_server::CryptoServer> =
            Arc::from(crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap());
        let cm = Chronomatter::new(1_000_000_000, observer, crypto)
            .expect("failed to create Chronomatter");
        calendar.write().tbid = cm.get_tbid();
        calendar.write().tbn = cm.get_tbn().to_string();
        (Arc::new(cm), last_tick, calendar)
    }

    #[test]
    fn stamp_returns_foretis_with_correct_tick() {
        let (cm, _last, calendar) = make_chronomatter();
        let foretis = cm.stamp(b"hello".to_vec(), "echo".to_string()).unwrap();
        assert_eq!(foretis.foretis.chronon_number, 1);
        assert_eq!(foretis.foretis.echo, "echo");
        assert!(!foretis.signature_bytes.is_empty());
        // Calendar received the tick record via observer
        assert_eq!(calendar.read().ticks.len(), 1);
    }

    #[test]
    fn two_stamps_share_same_tick_if_no_daemon_advance() {
        let (cm, _last, calendar) = make_chronomatter();
        let f1 = cm.stamp(b"one".to_vec(), "e1".to_string()).unwrap();
        let f2 = cm.stamp(b"two".to_vec(), "e2".to_string()).unwrap();
        assert_eq!(f1.foretis.chronon_number, 1);
        assert_eq!(f2.foretis.chronon_number, 2);
        assert_eq!(calendar.read().ticks.len(), 2);
    }

    #[test]
    fn verify_succeeds_for_valid_stamp() {
        let (cm, _last, calendar) = make_chronomatter();
        let content = b"verify-me".to_vec();
        let foretis = cm.stamp(content.clone(), "v".to_string()).unwrap();
        let valid = cm
            .verify(
                &foretis.foretis,
                &foretis.signature_bytes,
                &foretis.signature_algorithm,
                &content,
                &*calendar.read(),
            )
            .unwrap();
        assert!(valid);
    }

    #[test]
    fn verify_fails_for_wrong_content() {
        let (cm, _last, calendar) = make_chronomatter();
        let foretis = cm.stamp(b"original".to_vec(), "v".to_string()).unwrap();
        let valid = cm
            .verify(
                &foretis.foretis,
                &foretis.signature_bytes,
                &foretis.signature_algorithm,
                b"tampered",
                &*calendar.read(),
            )
            .unwrap();
        assert!(!valid);
    }

    #[test]
    fn stamp_uses_crypto_server_algorithm() {
        let (cm, _last, _calendar) = make_chronomatter();
        let foretis = cm.stamp(b"algo-test".to_vec(), "e".to_string()).unwrap();
        let expected_alg = cm.crypto_server().signature_algorithm().to_id_string();
        assert_eq!(foretis.signature_algorithm, expected_alg.to_string());
    }

    #[test]
    fn daemon_tick_advances_and_notifies_observer() {
        let (cm, last_tick, calendar) = make_chronomatter();
        cm.daemon_tick().unwrap();
        assert_eq!(cm.current_tick(), 1);
        assert_eq!(last_tick.load(SeqCst), 1);
        assert_eq!(calendar.read().ticks.len(), 1);
    }

    #[test]
    fn keypair_changes_on_each_tick() {
        let (cm, _last, calendar) = make_chronomatter();
        cm.daemon_tick().unwrap();
        let pk1 = calendar.read().ticks[0].public_key.clone();
        cm.daemon_tick().unwrap();
        let pk2 = calendar.read().ticks[1].public_key.clone();
        assert_ne!(pk1, pk2, "keypairs should differ between ticks");
    }

    #[test]
    fn dormant_cannot_stamp() {
        let last_tick = Arc::new(AtomicU64Std::new(0));
        let calendar = Arc::new(RwLock::new(Calendar::new(
            Tbid::from_raw([0u8; 96]),
            "dormant-test",
        )));
        let observer = Arc::new(DummyObserver {
            last_tick: Arc::clone(&last_tick),
            calendar: Arc::clone(&calendar),
        });
        let crypto =
            Arc::from(crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519).unwrap());
        let tmp =
            std::env::temp_dir().join(format!("chronomatter_test_{}.json", std::process::id()));
        {
            let (cm_init, _, cal_init) = make_chronomatter();
            cm_init.stamp(b"init".to_vec(), "init".to_string()).unwrap();
            std::fs::write(&tmp, serde_json::to_string(&*cal_init.read()).unwrap()).unwrap();
        }
        let cm = Chronomatter::from_calendar(tmp.to_str().unwrap(), crypto, observer).unwrap();
        assert!(cm.is_dormant());
        let result = cm.stamp(b"should-fail".to_vec(), "e".to_string());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn integrity_check_empty_calendar() {
        let (cm, _last, calendar) = make_chronomatter();
        let results = cm.integrity_check(&*calendar.read(), None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_after_ticks() {
        let (cm, _last, calendar) = make_chronomatter();
        for _ in 0..3 {
            cm.daemon_tick().unwrap();
        }
        let results = cm.integrity_check(&*calendar.read(), None, None).unwrap();
        // 2 pair checks (1→2, 2→3) = 2
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|&v| v));
    }
}
