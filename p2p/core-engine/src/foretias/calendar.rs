//! In-memory Calendar with append-only tick records.

use super::tick::{verify_pair, CalendarLookup, ChrononRecord};
use super::types::Tbid;
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

/// An append-only chronological record of ChrononRecords belonging to a TimeFamily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// TimeBeing identifier of this calendar's owner.
    pub(crate) tbid: Tbid,
    /// TimeBeing name (human-readable identifier).
    pub(crate) tbn: String,
    /// Stamp TimeBeing identifier used for attestation.
    pub(crate) stamp_tbid: Tbid,
    /// Ordered list of tick records.
    pub(crate) ticks: Vec<ChrononRecord>,
}

impl Calendar {
    /// Creates a new empty calendar with the given TimeBeing ID and name.
    pub fn new(tbid: Tbid, tbn: &str) -> Self {
        Self {
            tbid,
            tbn: tbn.to_string(),
            stamp_tbid: tbid,
            ticks: Vec::new(),
        }
    }

    /// Returns a slice of all tick records.
    pub fn ticks(&self) -> &[ChrononRecord] {
        &self.ticks
    }

    /// Returns the tick record with the given chronon number, if present.
    ///
    /// Ticks are maintained in ascending order by `append()`, so binary search
    /// provides O(log n) lookup instead of O(n) linear scan.
    pub fn tick_at(&self, n: u64) -> Option<&ChrononRecord> {
        self.ticks
            .binary_search_by(|t| t.chronon_number.cmp(&n))
            .ok()
            .map(|idx| &self.ticks[idx])
    }

    /// Returns the most recent tick record, or `None` if the calendar is empty.
    pub fn latest_tick(&self) -> Option<&ChrononRecord> {
        self.ticks.last()
    }

    /// Returns the number of tick records in the calendar.
    pub fn tick_count(&self) -> usize {
        self.ticks.len()
    }

    /// Returns a reference to the TimeBeing identifier.
    pub fn tbid(&self) -> &Tbid {
        &self.tbid
    }

    /// Returns the TimeBeing name (human-readable identifier).
    pub fn tbn(&self) -> &str {
        &self.tbn
    }

    /// Returns a reference to the stamp TimeBeing identifier.
    pub fn stamp_tbid(&self) -> &Tbid {
        &self.stamp_tbid
    }

    /// Sets the TimeBeing identifier. Used by Chronomatter after key generation.
    pub fn set_tbid(&mut self, tbid: Tbid) {
        debug_assert!(
            self.tbid == Tbid::default(),
            "set_tbid called on already-initialized calendar"
        );
        self.tbid = tbid;
    }

    /// Sets the TimeBeing name. Used by Chronomatter after key generation.
    pub fn set_tbn(&mut self, tbn: &str) {
        debug_assert!(
            self.tbn.is_empty(),
            "set_tbn called on already-initialized calendar"
        );
        self.tbn = tbn.to_string();
    }

    /// Appends a tick record; returns an error if the tick number is not strictly greater than the last.
    #[must_use = "appending a tick may fail if chronon number is not strictly greater"]
    pub fn append(&mut self, record: ChrononRecord) -> Result<(), NodeError> {
        if let Some(last) = self.ticks.last() {
            if record.chronon_number <= last.chronon_number {
                return Err(NodeError::Internal(format!(
                    "tick number {} is not strictly greater than last tick {}",
                    record.chronon_number, last.chronon_number
                )));
            }
        }
        self.ticks.push(record);
        Ok(())
    }

    /// Verifies chain integrity by checking cryptographic attestations between consecutive ticks.
    ///
    /// Returns a `Vec<bool>` where each element corresponds to the integrity of one pair
    /// `(tick[i], tick[i+1])`. An empty vec means 0 or 1 tick in range (nothing to verify).
    ///
    /// If `start` is `None`, verification begins from the first tick.
    /// If `end` is `None`, verification proceeds to the last tick.
    pub fn integrity_check(
        &self,
        crypto: &dyn CryptoServer,
        tbid_str: &str,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Vec<bool>, NodeError> {
        let start_tick = start.unwrap_or(0);
        let end_tick = end.unwrap_or(u64::MAX);

        let ticks: Vec<&ChrononRecord> = self
            .ticks
            .iter()
            .filter(|t| t.chronon_number >= start_tick && t.chronon_number <= end_tick)
            .collect();

        if ticks.len() < 2 {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(ticks.len() - 1);
        for i in 0..ticks.len() - 1 {
            let valid = verify_pair(crypto, tbid_str, ticks[i], ticks[i + 1])?;
            results.push(valid);
        }
        Ok(results)
    }

    /// Persists the calendar to a JSON file at the given path using atomic write.
    ///
    /// Writes to a `.tmp` file first, then atomically renames it to the target path.
    /// On POSIX systems, `rename` is atomic — if a crash occurs mid-write, the
    /// original file is unaffected. On load, any leftover `.tmp` is recovered.
    #[must_use = "calendar save may fail and the error must be handled"]
    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let data = serde_json::to_string_pretty(self)?;
        let tmp_path = format!("{path}.tmp");
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&tmp_path, &data)?;
        // POSIX-atomic rename (overwrites target on success)
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Loads a calendar from a JSON file at the given path with crash recovery.
    ///
    /// Recovery logic:
    /// 1. If a `.tmp` file exists alongside the target, try to parse it.
    ///    If valid and has >= ticks as the main file, use the `.tmp` (it's a
    ///    more recent atomic-write that completed its write but not its rename).
    /// 2. If `.tmp` is corrupt, delete it and load the main file.
    /// 3. If only the main file exists, load it normally.
    #[must_use = "calendar load may fail and the error must be handled"]
    pub fn load(path: &str) -> Result<Self, NodeError> {
        let tmp_path = format!("{path}.tmp");

        // Try loading the main file
        let main_cal = std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str::<Calendar>(&data).ok());

        // Load .tmp if it exists; on deserialization failure remove it and warn.
        let tmp_cal = if std::path::Path::new(&tmp_path).exists() {
            match std::fs::read_to_string(&tmp_path)
                .ok()
                .and_then(|data| serde_json::from_str::<Calendar>(&data).ok())
            {
                Some(cal) => Some(cal),
                None => {
                    tracing::warn!(path = %tmp_path, "removing corrupt .tmp during crash-recovery scan");
                    let _ = std::fs::remove_file(&tmp_path);
                    None
                }
            }
        } else {
            None
        };

        match (main_cal, tmp_cal) {
            (Some(cal), None) => Ok(cal),
            (Some(main), Some(tmp)) => {
                // .tmp has more ticks → it's a newer write that didn't rename yet
                if tmp.ticks.len() >= main.ticks.len() {
                    let _ = std::fs::remove_file(&tmp_path);
                    Ok(tmp)
                } else {
                    // .tmp is stale — discard it
                    let _ = std::fs::remove_file(&tmp_path);
                    Ok(main)
                }
            }
            (None, Some(tmp)) => {
                // Main file missing but .tmp exists — recover from .tmp
                let _ = std::fs::remove_file(&tmp_path);
                Ok(tmp)
            }
            (None, None) => Err(NodeError::Internal(format!(
                "calendar file not found: {}",
                path
            ))),
        }
    }

    /// Loads a calendar from disk and verifies chain integrity.
    ///
    /// After loading (including any `.tmp` crash recovery), runs `integrity_check`
    /// on the full calendar. If any pair fails verification, the load is rejected.
    ///
    /// This is the verification gate: data retrieved from disk is treated as
    /// `UnverifiedSignatureEnvelope` until the integrity check passes, at which point the calendar
    /// becomes `CleanAuthenticated`.
    ///
    /// If `start` is `None`, verification begins from the first tick.
    /// If `end` is `None`, verification proceeds to the last tick.
    pub fn load_and_verify(
        path: &str,
        crypto: &dyn CryptoServer,
        tbid_str: &str,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Self, NodeError> {
        let cal = Self::load(path)?;

        // Run integrity check on loaded calendar — reject if any pair fails
        let results = cal.integrity_check(crypto, tbid_str, start, end)?;
        if results.iter().any(|&v| !v) {
            return Err(NodeError::Internal(format!(
                "calendar integrity check failed for {}: {} pair(s) invalid out of {}",
                path,
                results.iter().filter(|&&v| !v).count(),
                results.len()
            )));
        }

        Ok(cal)
    }

    /// Add an external attestation to a specific tick.
    pub fn add_external_attestation(
        &mut self,
        chronon_number: u64,
        att: super::external_attestation::ExternalAttestationRecord,
    ) -> Result<(), NodeError> {
        for tick in self.ticks.iter_mut() {
            if tick.chronon_number == chronon_number {
                tick.external_attestations.push(att);
                return Ok(());
            }
        }
        Err(NodeError::Internal(format!(
            "tick number {} not found for external attestation",
            chronon_number
        )))
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, chronon_number: u64, count: usize) -> Result<Vec<ChrononRecord>, NodeError> {
        Ok(self
            .ticks
            .iter()
            .filter(|t| t.chronon_number >= chronon_number)
            .take(count)
            .cloned()
            .collect())
    }

    fn latest(&self) -> Option<u64> {
        self.ticks.last().map(|t| t.chronon_number)
    }

    fn tbid(&self) -> Tbid {
        self.tbid
    }

    fn tbn(&self) -> &str {
        &self.tbn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;
    use std::path::Path;

    fn make_tick(chronon_number: u64) -> ChrononRecord {
        ChrononRecord {
            chronon_number,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        }
    }

    #[test]
    fn append_valid_tick_succeeds() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        assert!(cal.append(make_tick(1)).is_ok());
        assert_eq!(cal.ticks.len(), 1);
    }

    #[test]
    fn append_duplicate_tick_returns_error() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(5)).unwrap();
        let result = cal.append(make_tick(5));
        assert!(result.is_err());
    }

    #[test]
    fn append_out_of_order_tick_returns_error() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(10)).unwrap();
        let result = cal.append(make_tick(3));
        assert!(result.is_err());
    }

    #[test]
    fn get_returns_correct_ticks() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(1)).unwrap();
        cal.append(make_tick(2)).unwrap();
        cal.append(make_tick(3)).unwrap();

        let results = cal.get(2, 10).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chronon_number, 2);
        assert_eq!(results[1].chronon_number, 3);
    }

    #[test]
    fn get_returns_empty_for_out_of_bounds() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(1)).unwrap();
        let results = cal.get(100, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn get_respects_count_limit() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(1)).unwrap();
        cal.append(make_tick(2)).unwrap();
        cal.append(make_tick(3)).unwrap();

        let results = cal.get(1, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chronon_number, 1);
        assert_eq!(results[1].chronon_number, 2);
    }

    #[test]
    fn latest_returns_none_on_empty() {
        let cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn latest_returns_correct_chronon_number() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(7)).unwrap();
        cal.append(make_tick(14)).unwrap();
        assert_eq!(cal.latest(), Some(14));
    }

    #[test]
    fn integrity_check_returns_empty_on_empty_calendar() {
        let cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let results = cal
            .integrity_check(server.as_ref(), "test", None, None)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_returns_empty_on_single_tick() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(1)).unwrap();
        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let results = cal
            .integrity_check(server.as_ref(), "test", None, None)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_full_chain() {
        use crate::core::identity::{generate_ed25519_keypair, PrivKeyHandle};
        use crate::core::signing::ed25519_sign_with_handle;
        use crate::foretias::tick::auto_attestation_blob_with_count;

        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let tbid = Tbid::from_raw([0xEE; 96]);
        let tbid_str = tbid.to_hex();
        let mut cal = Calendar::new(tbid, "full-chain");

        PrivKeyHandle::init();
        let mut keypairs: Vec<([u8; 32], PrivKeyHandle)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            let handle = PrivKeyHandle::from_seed(&priv_key.bytes).unwrap();
            keypairs.push((pub_key.bytes, handle));
        }

        for i in 0..5u64 {
            let (forward_foretis, backward_foretis, nonce) = if i == 0 {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(
                    &tbid_str,
                    i,
                    &keypairs[0].0,
                    i,
                    &keypairs[0].0,
                    0,
                )
                .unwrap();
                let sig = ed25519_sign_with_handle(&keypairs[0].1, &attest_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec(), nonce)
            } else {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(
                    &tbid_str,
                    i - 1,
                    &keypairs[(i - 1) as usize].0,
                    i,
                    &keypairs[i as usize].0,
                    0,
                )
                .unwrap();
                let fwd =
                    ed25519_sign_with_handle(&keypairs[(i - 1) as usize].1, &attest_blob).unwrap();
                let bwd = ed25519_sign_with_handle(&keypairs[i as usize].1, &attest_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec(), nonce)
            };

            cal.append(ChrononRecord {
                chronon_number: i,
                public_key: keypairs[i as usize].0.to_vec().into(),
                signature_algorithm: "Ed25519".to_string(),
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: nonce.into(),
                chronon_stamp_count: 0,
                external_attestations: Vec::new(),
                tb_version: 0,
                tbid: Tbid::default(),
            })
            .unwrap();
        }

        let results = cal
            .integrity_check(server.as_ref(), &tbid_str, None, None)
            .unwrap();
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn integrity_check_partial_range() {
        use crate::core::identity::{generate_ed25519_keypair, PrivKeyHandle};
        use crate::core::signing::ed25519_sign_with_handle;
        use crate::foretias::tick::auto_attestation_blob_with_count;

        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let tbid = Tbid::from_raw([0xFF; 96]);
        let tbid_str = tbid.to_hex();
        let mut cal = Calendar::new(tbid, "partial-range");

        PrivKeyHandle::init();
        let mut keypairs: Vec<([u8; 32], PrivKeyHandle)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            let handle = PrivKeyHandle::from_seed(&priv_key.bytes).unwrap();
            keypairs.push((pub_key.bytes, handle));
        }

        for i in 0..5u64 {
            let (forward_foretis, backward_foretis, nonce) = if i == 0 {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(
                    &tbid_str,
                    i,
                    &keypairs[0].0,
                    i,
                    &keypairs[0].0,
                    0,
                )
                .unwrap();
                let sig = ed25519_sign_with_handle(&keypairs[0].1, &attest_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec(), nonce)
            } else {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(
                    &tbid_str,
                    i - 1,
                    &keypairs[(i - 1) as usize].0,
                    i,
                    &keypairs[i as usize].0,
                    0,
                )
                .unwrap();
                let fwd =
                    ed25519_sign_with_handle(&keypairs[(i - 1) as usize].1, &attest_blob).unwrap();
                let bwd = ed25519_sign_with_handle(&keypairs[i as usize].1, &attest_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec(), nonce)
            };

            cal.append(ChrononRecord {
                chronon_number: i,
                public_key: keypairs[i as usize].0.to_vec().into(),
                signature_algorithm: "Ed25519".to_string(),
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: nonce.into(),
                chronon_stamp_count: 0,
                external_attestations: Vec::new(),
                tb_version: 0,
                tbid: Tbid::default(),
            })
            .unwrap();
        }

        let results = cal
            .integrity_check(server.as_ref(), &tbid_str, Some(1), Some(3))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn latest_chronon_number_via_calendar_lookup() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(42)).unwrap();
        assert_eq!(cal.latest(), Some(42));
    }

    #[test]
    fn calendar_save_is_atomic() {
        use std::path::Path;
        let path = "/tmp/foretias-test-atomic-save.json";
        let tmp_path = format!("{path}.tmp");

        let mut cal = Calendar::new(Tbid::from_raw([0xAA; 96]), "atomic-test");
        cal.append(make_tick(1)).unwrap();
        cal.save(path).unwrap();

        // .tmp file must not exist after successful save
        assert!(
            !Path::new(&tmp_path).exists(),
            ".tmp should be cleaned up after atomic save"
        );

        // Main file must exist and be valid
        let loaded = Calendar::load(path).unwrap();
        assert_eq!(loaded.latest(), Some(1));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_from_tmp() {
        let path = "/tmp/foretias-test-crash-recovery.json";
        let tmp_path = format!("{path}.tmp");

        // Simulate crash: main file has 1 tick, .tmp has 2 ticks (write completed, rename didn't)
        let mut main_cal = Calendar::new(Tbid::from_raw([0xBB; 96]), "crash-recovery");
        main_cal.append(make_tick(1)).unwrap();
        main_cal.save(path).unwrap();

        let mut new_cal = Calendar::new(Tbid::from_raw([0xBB; 96]), "crash-recovery");
        new_cal.append(make_tick(1)).unwrap();
        new_cal.append(make_tick(2)).unwrap();
        // Write .tmp directly (simulating crash mid-save — rename never happened)
        let data = serde_json::to_string_pretty(&new_cal).unwrap();
        std::fs::write(&tmp_path, &data).unwrap();

        // Load should recover from .tmp
        let recovered = Calendar::load(path).unwrap();
        assert_eq!(recovered.latest(), Some(2), "should recover newer .tmp");
        assert!(
            !Path::new(&tmp_path).exists(),
            ".tmp should be cleaned up after recovery"
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_ignores_stale_tmp() {
        let path = "/tmp/foretias-test-stale-tmp.json";
        let tmp_path = format!("{path}.tmp");

        // Main file has 3 ticks, .tmp has only 1 (stale/corrupt .tmp)
        let mut main_cal = Calendar::new(Tbid::from_raw([0xCC; 96]), "stale-tmp");
        main_cal.append(make_tick(1)).unwrap();
        main_cal.append(make_tick(2)).unwrap();
        main_cal.append(make_tick(3)).unwrap();
        main_cal.save(path).unwrap();

        let mut stale_cal = Calendar::new(Tbid::from_raw([0xCC; 96]), "stale-tmp");
        stale_cal.append(make_tick(1)).unwrap();
        let data = serde_json::to_string_pretty(&stale_cal).unwrap();
        std::fs::write(&tmp_path, &data).unwrap();

        // Load should prefer main file
        let loaded = Calendar::load(path).unwrap();
        assert_eq!(
            loaded.latest(),
            Some(3),
            "should prefer main file over stale .tmp"
        );
        assert!(
            !Path::new(&tmp_path).exists(),
            "stale .tmp should be cleaned up"
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_corrupt_tmp() {
        let path = "/tmp/foretias-test-corrupt-tmp.json";
        let tmp_path = format!("{path}.tmp");

        // Main file is valid, .tmp contains garbage
        let mut cal = Calendar::new(Tbid::from_raw([0xDD; 96]), "corrupt-tmp");
        cal.append(make_tick(1)).unwrap();
        cal.save(path).unwrap();

        std::fs::write(&tmp_path, "this is not json").unwrap();

        // Load should ignore corrupt .tmp and load main file, removing the corrupt .tmp
        let loaded = Calendar::load(path).unwrap();
        assert_eq!(loaded.latest(), Some(1));
        assert!(
            !Path::new(&tmp_path).exists(),
            "corrupt .tmp must be removed by crash-recovery scan"
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn load_and_verify_accepts_valid_calendar() {
        let path = "/tmp/foretias-test-load-verify-valid.json";
        let mut cal = Calendar::new(Tbid::from_raw([0xAA; 96]), "load-verify");
        cal.append(make_tick(1)).unwrap();
        cal.save(path).unwrap();

        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let loaded = Calendar::load_and_verify(path, server.as_ref(), "test", None, None);
        // Single tick calendar has no pairs to verify, so it passes
        assert!(loaded.is_ok());
        assert_eq!(loaded.unwrap().latest(), Some(1));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn load_and_verify_rejects_corrupted_calendar() {
        use crate::core::identity::{generate_ed25519_keypair, PrivKeyHandle};
        use crate::core::signing::ed25519_sign_with_handle;
        use crate::foretias::tick::auto_attestation_blob_with_count;

        let path = "/tmp/foretias-test-load-verify-corrupt.json";
        let tbid = Tbid::from_raw([0xBB; 96]);
        let tbid_str = tbid.to_hex();
        let mut cal = Calendar::new(tbid, "corrupt-verify");

        // Generate two keypairs
        let (pk1, sk1) = generate_ed25519_keypair().unwrap();
        let (pk2, _sk2) = generate_ed25519_keypair().unwrap();
        let sk1_handle = PrivKeyHandle::from_seed(&sk1.bytes).unwrap();

        // Tick 1 with valid auto-attestation
        let (blob1, nonce1) =
            auto_attestation_blob_with_count(&tbid_str, 1, &pk1.bytes, 1, &pk1.bytes, 0).unwrap();
        let sig1 = ed25519_sign_with_handle(&sk1_handle, &blob1).unwrap();

        cal.append(ChrononRecord {
            chronon_number: 1,
            public_key: pk1.bytes.to_vec().into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: sig1.bytes.to_vec().into(),
            backward_foretis: sig1.bytes.to_vec().into(),
            aa_nonce: nonce1.into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        })
        .unwrap();

        // Tick 2 with TAMPERED forward foretis (signed with wrong key)
        let (blob2, nonce2) =
            auto_attestation_blob_with_count(&tbid_str, 1, &pk1.bytes, 2, &pk2.bytes, 0).unwrap();
        // Sign with tick 1's key for forward, but use garbage for backward
        let fwd_sig = ed25519_sign_with_handle(&sk1_handle, &blob2).unwrap();
        let mut backward_sig = fwd_sig.bytes.to_vec();
        backward_sig[0] ^= 0xFF; // tamper

        cal.append(ChrononRecord {
            chronon_number: 2,
            public_key: pk2.bytes.to_vec().into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: fwd_sig.bytes.to_vec().into(),
            backward_foretis: backward_sig.into(),
            aa_nonce: nonce2.into(),
            chronon_stamp_count: 0,
            external_attestations: Vec::new(),
            tb_version: 0,
            tbid: Tbid::default(),
        })
        .unwrap();

        cal.save(path).unwrap();

        let server =
            crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let loaded = Calendar::load_and_verify(path, server.as_ref(), &tbid_str, None, None);
        // Tampered backward signature should cause integrity check failure
        assert!(loaded.is_err());

        std::fs::remove_file(path).ok();
    }

    #[test]
    #[should_panic(expected = "set_tbid called on already-initialized calendar")]
    fn test_set_tbid_twice_panics() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.set_tbid(Tbid::from_raw([1u8; 96]));
        cal.set_tbid(Tbid::from_raw([2u8; 96]));
    }

    #[test]
    fn test_tick_at_finds_existing() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        for n in 1..=5 {
            cal.append(make_tick(n)).unwrap();
        }
        let tick = cal.tick_at(3).unwrap();
        assert_eq!(tick.chronon_number, 3);
    }

    #[test]
    fn test_tick_at_missing_returns_none() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        for n in 1..=5 {
            cal.append(make_tick(n)).unwrap();
        }
        assert!(cal.tick_at(999).is_none());
    }

    #[test]
    fn test_tick_at_first_and_last() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        for n in 1..=5 {
            cal.append(make_tick(n)).unwrap();
        }
        assert_eq!(cal.tick_at(1).unwrap().chronon_number, 1);
        assert_eq!(cal.tick_at(5).unwrap().chronon_number, 5);
    }

    #[test]
    fn test_tick_at_empty_calendar() {
        let cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        assert!(cal.tick_at(1).is_none());
    }
}
