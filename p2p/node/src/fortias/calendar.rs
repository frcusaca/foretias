//! In-memory Calendar with append-only tick records.

use super::tick::{TickRecord, CalendarLookup, verify_pair};
use crate::crypto_server::CryptoServer;
use crate::error::NodeError;
use serde::{Deserialize, Serialize};

/// An append-only chronological record of TickRecords belonging to a TimeFamily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// TimeBeing identifier of this calendar's owner.
    pub tbid: [u8; 16],
    /// TimeBeing name (human-readable identifier).
    pub tbn: String,
    /// Stamp TimeBeing identifier used for attestation.
    pub stamp_tbid: [u8; 16],
    /// Ordered list of tick records.
    pub ticks: Vec<TickRecord>,
}

impl Calendar {
    /// Creates a new empty calendar with the given TimeBeing ID and name.
    pub fn new(tbid: [u8; 16], tbn: &str) -> Self {
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

    /// Persists the calendar to a JSON file at the given path.
    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap_or_else(|| std::path::Path::new(".")))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Loads a calendar from a JSON file at the given path.
    pub fn load(path: &str) -> Result<Self, NodeError> {
        let data = std::fs::read_to_string(path)?;
        let cal: Calendar = serde_json::from_str(&data)?;
        Ok(cal)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto_server;

    fn make_tick(tick_number: u64) -> TickRecord {
        TickRecord {
            tick_number,
            public_key: vec![0u8; 32],
            forward_fortis: vec![],
            backward_fortis: vec![],
        }
    }

    #[test]
    fn append_valid_tick_succeeds() {
        let mut cal = Calendar::new([0u8; 16], "test");
        assert!(cal.append(make_tick(1)).is_ok());
        assert_eq!(cal.ticks.len(), 1);
    }

    #[test]
    fn append_duplicate_tick_returns_error() {
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(5)).unwrap();
        let result = cal.append(make_tick(5));
        assert!(result.is_err());
    }

    #[test]
    fn append_out_of_order_tick_returns_error() {
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(10)).unwrap();
        let result = cal.append(make_tick(3));
        assert!(result.is_err());
    }

    #[test]
    fn get_returns_correct_ticks() {
        let mut cal = Calendar::new([0u8; 16], "test");
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
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(1)).unwrap();
        let results = cal.get(100, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn get_respects_count_limit() {
        let mut cal = Calendar::new([0u8; 16], "test");
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
        let cal = Calendar::new([0u8; 16], "test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn latest_returns_correct_tick_number() {
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(7)).unwrap();
        cal.append(make_tick(14)).unwrap();
        assert_eq!(cal.latest(), Some(14));
    }

    #[test]
    fn integrity_check_returns_empty_on_empty_calendar() {
        let cal = Calendar::new([0u8; 16], "test");
        let server = crypto_server::new_software(crate::crypto_server::FortiasCurve::Ed25519).unwrap();
        let results = cal.integrity_check(server.as_ref(), "test", None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_returns_empty_on_single_tick() {
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(1)).unwrap();
        let server = crypto_server::new_software(crate::crypto_server::FortiasCurve::Ed25519).unwrap();
        let results = cal.integrity_check(server.as_ref(), "test", None, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn integrity_check_full_chain() {
        use crate::core::identity::generate_ed25519_keypair;
        use crate::core::signing::ed25519_sign;
        use crate::core::bindings::FortiasPrivKey32;
        use crate::fortias::tick::auto_attestation_blob;
        use zeroize::Zeroizing;

        let server = crypto_server::new_software(crate::crypto_server::FortiasCurve::Ed25519).unwrap();
        let tbid: [u8; 16] = [0xEE; 16];
        let tbid_str = hex::encode(tbid);
        let mut cal = Calendar::new(tbid, "full-chain");

        let mut keypairs: Vec<([u8; 32], Zeroizing<[u8; 32]>)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            keypairs.push((pub_key.bytes, Zeroizing::new(priv_key.bytes)));
        }

        for i in 0..5u64 {
            let (forward_fortis, backward_fortis) = if i == 0 {
                let ma_blob = auto_attestation_blob(&tbid_str, i, &keypairs[0].0, i, &keypairs[0].0);
                let sig = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[0].1 }, &ma_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec())
            } else {
                let ma_blob = auto_attestation_blob(&tbid_str, i - 1, &keypairs[(i-1) as usize].0, i, &keypairs[i as usize].0);
                let fwd = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[(i-1) as usize].1 }, &ma_blob).unwrap();
                let bwd = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[i as usize].1 }, &ma_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec())
            };

            cal.append(TickRecord {
                tick_number: i,
                public_key: keypairs[i as usize].0.to_vec(),
                forward_fortis,
                backward_fortis,
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
        use crate::core::bindings::FortiasPrivKey32;
        use crate::fortias::tick::auto_attestation_blob;
        use zeroize::Zeroizing;

        let server = crypto_server::new_software(crate::crypto_server::FortiasCurve::Ed25519).unwrap();
        let tbid: [u8; 16] = [0xFF; 16];
        let tbid_str = hex::encode(tbid);
        let mut cal = Calendar::new(tbid, "partial-range");

        let mut keypairs: Vec<([u8; 32], Zeroizing<[u8; 32]>)> = Vec::new();
        for _ in 0..5 {
            let (pub_key, priv_key) = generate_ed25519_keypair().unwrap();
            keypairs.push((pub_key.bytes, Zeroizing::new(priv_key.bytes)));
        }

        for i in 0..5u64 {
            let (forward_fortis, backward_fortis) = if i == 0 {
                let ma_blob = auto_attestation_blob(&tbid_str, i, &keypairs[0].0, i, &keypairs[0].0);
                let sig = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[0].1 }, &ma_blob).unwrap();
                (sig.bytes.to_vec(), sig.bytes.to_vec())
            } else {
                let ma_blob = auto_attestation_blob(&tbid_str, i - 1, &keypairs[(i-1) as usize].0, i, &keypairs[i as usize].0);
                let fwd = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[(i-1) as usize].1 }, &ma_blob).unwrap();
                let bwd = ed25519_sign(&FortiasPrivKey32 { bytes: *keypairs[i as usize].1 }, &ma_blob).unwrap();
                (fwd.bytes.to_vec(), bwd.bytes.to_vec())
            };

            cal.append(TickRecord {
                tick_number: i,
                public_key: keypairs[i as usize].0.to_vec(),
                forward_fortis,
                backward_fortis,
            }).unwrap();
        }

        let results = cal.integrity_check(server.as_ref(), &tbid_str, Some(1), Some(3)).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|&v| v));
    }

    #[test]
    fn latest_tick_number_via_calendar_lookup() {
        let mut cal = Calendar::new([0u8; 16], "test");
        cal.append(make_tick(42)).unwrap();
        assert_eq!(cal.latest(), Some(42));
    }
}
