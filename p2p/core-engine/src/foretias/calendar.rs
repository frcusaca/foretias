//! In-memory Calendar with append-only tick records.

use super::tick::{TickRecord, CalendarLookup, verify_pair};
use super::types::Tbid;
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

/// An append-only chronological record of TickRecords belonging to a TimeFamily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// TimeBeing identifier of this calendar's owner.
    pub tbid: Tbid,
    /// TimeBeing name (human-readable identifier).
    pub tbn: String,
    /// Stamp TimeBeing identifier used for attestation.
    pub stamp_tbid: Tbid,
    /// Ordered list of tick records.
    pub ticks: Vec<TickRecord>,
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

    /// Appends a tick record; returns an error if the tick number is not strictly greater than the last.
    pub fn append(&mut self, record: TickRecord) -> Result<(), NodeError> {
        if let Some(last) = self.ticks.last() {
            if record.tick_number <= last.tick_number {
                return Err(NodeError::Internal(format!(
                    "tick number {} is not strictly greater than last tick {}",
                    record.tick_number, last.tick_number
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

        let ticks: Vec<&TickRecord> = self.ticks.iter()
            .filter(|t| t.tick_number >= start_tick && t.tick_number <= end_tick)
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
    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let data = serde_json::to_string_pretty(self)?;
        let tmp_path = format!("{}.tmp", path);
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
    pub fn load(path: &str) -> Result<Self, NodeError> {
        let tmp_path = format!("{}.tmp", path);

        // Try loading the main file
        let main_cal = std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str::<Calendar>(&data).ok());

        // Try loading the .tmp file (potential crash recovery)
        let tmp_cal = std::fs::read_to_string(&tmp_path)
            .ok()
            .and_then(|data| serde_json::from_str::<Calendar>(&data).ok());

        match (main_cal, tmp_cal) {
            (Some(cal), None) => Ok(cal),
            (Some(main), Some(tmp)) => {
                // .tmp has more ticks → it's a newer write that didn't rename yet
                if tmp.ticks.len() >= main.ticks.len() {
                    // Clean up the leftover .tmp
                    let _ = std::fs::remove_file(&tmp_path);
                    Ok(tmp)
                } else {
                    // .tmp is stale or corrupt → ignore it
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
                "calendar file not found: {}", path
            ))),
        }
    }

    /// Add an external attestation to a specific tick.
    pub fn add_external_attestation(
        &mut self,
        tick_number: u64,
        att: super::external_attestation::ExternalAttestation,
    ) -> Result<(), NodeError> {
        for tick in self.ticks.iter_mut() {
            if tick.tick_number == tick_number {
                tick.external_attestations.push(att);
                return Ok(());
            }
        }
        Err(NodeError::Internal(format!(
            "tick number {} not found for external attestation",
            tick_number
        )))
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError> {
        Ok(self.ticks.iter()
            .filter(|t| t.tick_number >= tick_number)
            .take(count)
            .cloned()
            .collect())
    }

    fn latest(&self) -> Option<u64> {
        self.ticks.last().map(|t| t.tick_number)
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

    fn make_tick(tick_number: u64) -> TickRecord {
        TickRecord {
            tick_number,
            public_key: vec![0u8; 32].into(),
            signature_algorithm: "Ed25519".to_string(),
            forward_foretis: vec![].into(),
            backward_foretis: vec![].into(),
            aa_nonce: [0u8; 16].into(),
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
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
        assert_eq!(results[0].tick_number, 2);
        assert_eq!(results[1].tick_number, 3);
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
        assert_eq!(results[0].tick_number, 1);
        assert_eq!(results[1].tick_number, 2);
    }

    #[test]
    fn latest_returns_none_on_empty() {
        let cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn latest_returns_correct_tick_number() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(7)).unwrap();
        cal.append(make_tick(14)).unwrap();
        assert_eq!(cal.latest(), Some(14));
    }

    #[test]
    fn integrity_check_returns_empty_on_empty_calendar() {
        let cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        let server = crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let results = cal.integrity_check(server.as_ref(), "test", None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_returns_empty_on_single_tick() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(1)).unwrap();
        let server = crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let results = cal.integrity_check(server.as_ref(), "test", None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_full_chain() {
        use crate::core::identity::generate_ed25519_keypair;
        use crate::core::signing::ed25519_sign;
        use crate::core::bindings::ForetiasPrivKey32;
        use crate::foretias::tick::auto_attestation_blob_with_count;
        use zeroize::Zeroizing;

        let server = crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let tbid = Tbid::from_raw([0xEE; 96]);
        let tbid_str = tbid.to_hex();
        let mut cal = Calendar::new(tbid, "full-chain");

        let mut keypairs: Vec<([u8; 32], Zeroizing<[u8; 32]>)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            keypairs.push((pub_key.bytes, Zeroizing::new(priv_key.bytes)));
        }

        for i in 0..5u64 {
            let (forward_foretis, backward_foretis, nonce) = if i == 0 {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, i, &keypairs[0].0, i, &keypairs[0].0, 0).unwrap();
                let sig = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[0].1 }, &attest_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec(), nonce)
            } else {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, i - 1, &keypairs[(i-1) as usize].0, i, &keypairs[i as usize].0, 0).unwrap();
                let fwd = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[(i-1) as usize].1 }, &attest_blob).unwrap();
                let bwd = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[i as usize].1 }, &attest_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec(), nonce)
            };

            cal.append(TickRecord {
                tick_number: i,
                public_key: keypairs[i as usize].0.to_vec().into(),
                signature_algorithm: "Ed25519".to_string(),
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: nonce.into(),
                stamps_per_tick: 0,
                external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
            }).unwrap();
        }

        let results = cal.integrity_check(server.as_ref(), &tbid_str, None, None).unwrap();
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn integrity_check_partial_range() {
        use crate::core::identity::generate_ed25519_keypair;
        use crate::core::signing::ed25519_sign;
        use crate::core::bindings::ForetiasPrivKey32;
        use crate::foretias::tick::auto_attestation_blob_with_count;
        use zeroize::Zeroizing;

        let server = crypto_server::new_software(crate::crypto_server::ForetiasCurve::Ed25519).unwrap();
        let tbid = Tbid::from_raw([0xFF; 96]);
        let tbid_str = tbid.to_hex();
        let mut cal = Calendar::new(tbid, "partial-range");

        let mut keypairs: Vec<([u8; 32], Zeroizing<[u8; 32]>)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            keypairs.push((pub_key.bytes, Zeroizing::new(priv_key.bytes)));
        }

        for i in 0..5u64 {
            let (forward_foretis, backward_foretis, nonce) = if i == 0 {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, i, &keypairs[0].0, i, &keypairs[0].0, 0).unwrap();
                let sig = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[0].1 }, &attest_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec(), nonce)
            } else {
                let (attest_blob, nonce) = auto_attestation_blob_with_count(&tbid_str, i - 1, &keypairs[(i-1) as usize].0, i, &keypairs[i as usize].0, 0).unwrap();
                let fwd = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[(i-1) as usize].1 }, &attest_blob).unwrap();
                let bwd = ed25519_sign(&ForetiasPrivKey32 { bytes: *keypairs[i as usize].1 }, &attest_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec(), nonce)
            };

            cal.append(TickRecord {
                tick_number: i,
                public_key: keypairs[i as usize].0.to_vec().into(),
                signature_algorithm: "Ed25519".to_string(),
                forward_foretis: forward_foretis.into(),
                backward_foretis: backward_foretis.into(),
                aa_nonce: nonce.into(),
                stamps_per_tick: 0,
                external_attestations: Vec::new(),
            genesis_signature: vec![].into(),
            tb_version: 0,
            }).unwrap();
        }

        let results = cal.integrity_check(server.as_ref(), &tbid_str, Some(1), Some(3)).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn latest_tick_number_via_calendar_lookup() {
        let mut cal = Calendar::new(Tbid::from_raw([0u8; 96]), "test");
        cal.append(make_tick(42)).unwrap();
        assert_eq!(cal.latest(), Some(42));
    }

    #[test]
    fn calendar_save_is_atomic() {
        use std::path::Path;
        let path = "/tmp/foretias-test-atomic-save.json";
        let tmp_path = format!("{}.tmp", path);

        let mut cal = Calendar::new(Tbid::from_raw([0xAA; 96]), "atomic-test");
        cal.append(make_tick(1)).unwrap();
        cal.save(path).unwrap();

        // .tmp file must not exist after successful save
        assert!(!Path::new(&tmp_path).exists(), ".tmp should be cleaned up after atomic save");

        // Main file must exist and be valid
        let loaded = Calendar::load(path).unwrap();
        assert_eq!(loaded.latest(), Some(1));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_from_tmp() {
        let path = "/tmp/foretias-test-crash-recovery.json";
        let tmp_path = format!("{}.tmp", path);

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
        assert!(!Path::new(&tmp_path).exists(), ".tmp should be cleaned up after recovery");

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_ignores_stale_tmp() {
        let path = "/tmp/foretias-test-stale-tmp.json";
        let tmp_path = format!("{}.tmp", path);

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
        assert_eq!(loaded.latest(), Some(3), "should prefer main file over stale .tmp");
        assert!(!Path::new(&tmp_path).exists(), "stale .tmp should be cleaned up");

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn calendar_crash_recovery_corrupt_tmp() {
        let path = "/tmp/foretias-test-corrupt-tmp.json";
        let tmp_path = format!("{}.tmp", path);

        // Main file is valid, .tmp contains garbage
        let mut cal = Calendar::new(Tbid::from_raw([0xDD; 96]), "corrupt-tmp");
        cal.append(make_tick(1)).unwrap();
        cal.save(path).unwrap();

        std::fs::write(&tmp_path, "this is not json").unwrap();

        // Load should ignore corrupt .tmp and load main file
        let loaded = Calendar::load(path).unwrap();
        assert_eq!(loaded.latest(), Some(1));
        assert!(Path::new(&tmp_path).exists(), "corrupt .tmp not removed by current implementation");

        std::fs::remove_file(path).ok();
        std::fs::remove_file(&tmp_path).ok();
    }
}
