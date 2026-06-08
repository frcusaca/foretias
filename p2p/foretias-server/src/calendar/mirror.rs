//! Mirror calendar storage for replicated TBIDs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use foretias_core::error::NodeError;
use foretias_core::foretias::tick::ChrononRecord;
use parking_lot::RwLock;

pub struct MirrorStore {
    mirrors: Arc<RwLock<HashMap<String, Vec<ChrononRecord>>>>,
    base_dir: PathBuf,
    max_mirrored_tbids: usize,
}

impl MirrorStore {
    pub fn new(base_dir: impl Into<PathBuf>, max_mirrored_tbids: usize) -> Self {
        Self {
            mirrors: Arc::new(RwLock::new(HashMap::new())),
            base_dir: base_dir.into(),
            max_mirrored_tbids,
        }
    }

    pub fn can_accept_mirror(&self, tbid_hex: &str) -> bool {
        let mirrors = self.mirrors.read();
        if mirrors.contains_key(tbid_hex) {
            return false;
        }
        mirrors.len() < self.max_mirrored_tbids
    }

    pub fn mirrored_tbids(&self) -> Vec<String> {
        self.mirrors.read().keys().cloned().collect()
    }

    pub fn mirror_tick_count(&self, tbid_hex: &str) -> u64 {
        self.mirrors
            .read()
            .get(tbid_hex)
            .map(|v| v.len() as u64)
            .unwrap_or(0)
    }

    pub fn insert_mirrored(&self, tbid_hex: &str, record: ChrononRecord) -> Result<(), NodeError> {
        let mut mirrors = self.mirrors.write();
        let entry = mirrors.entry(tbid_hex.to_string()).or_default();
        if entry
            .iter()
            .any(|r| r.chronon_number() == record.chronon_number())
        {
            return Ok(());
        }
        entry.push(record);
        entry.sort_by_key(|r| *r.chronon_number());
        Ok(())
    }

    pub fn get_mirrored(
        &self,
        tbid_hex: &str,
        chronon_number: u64,
        count: usize,
    ) -> Result<Vec<ChrononRecord>, NodeError> {
        let mirrors = self.mirrors.read();
        let records = mirrors
            .get(tbid_hex)
            .ok_or(NodeError::NotFound("mirrored calendar"))?;
        Ok(records
            .iter()
            .skip_while(|r| *r.chronon_number() < chronon_number)
            .take(count)
            .cloned()
            .collect())
    }

    pub fn latest_record(&self, tbid_hex: &str) -> Option<ChrononRecord> {
        self.mirrors
            .read()
            .get(tbid_hex)
            .and_then(|v| v.last().cloned())
    }

    pub fn mirror_info(&self, tbid_hex: &str) -> Option<(u64, u64, String)> {
        let mirrors = self.mirrors.read();
        let records = mirrors.get(tbid_hex)?;
        if records.is_empty() {
            return None;
        }
        let tick_count = records.len() as u64;
        let latest_tick = *records.last()?.chronon_number();
        let hash_sanity = compute_hash_sanity(records);
        Some((tick_count, latest_tick, hash_sanity))
    }
}

impl Clone for MirrorStore {
    fn clone(&self) -> Self {
        Self {
            mirrors: Arc::clone(&self.mirrors),
            base_dir: self.base_dir.clone(),
            max_mirrored_tbids: self.max_mirrored_tbids,
        }
    }
}

/// SHA-256 of concatenated tick numbers (big-endian u64 each).
pub fn compute_hash_sanity(records: &[ChrononRecord]) -> String {
    let mut data = Vec::with_capacity(records.len() * 8);
    for record in records {
        data.extend_from_slice(&record.chronon_number().to_be_bytes());
    }
    let hash = match foretias_core::core::hashing::sha256(&data) {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    hex::encode(hash.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick(chronon_number: u64) -> ChrononRecord {
        ChrononRecord::builder()
            .chronon_number(chronon_number)
            .public_key(vec![0u8; 32].into())
            .forward_foretis(vec![].into())
            .backward_foretis(vec![].into())
            .aa_nonce([0u8; 16].into())
            .tb_version(0)
            .build()
            .expect("make_tick")
    }

    #[test]
    fn can_accept_mirror_within_capacity() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        assert!(store.can_accept_mirror("abc123"));
    }

    #[test]
    fn can_accept_mirror_rejects_already_mirrored() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("abc123", make_tick(1)).unwrap();
        assert!(!store.can_accept_mirror("abc123"));
    }

    #[test]
    fn can_accept_mirror_rejects_at_capacity() {
        let store = MirrorStore::new("/tmp/mirror-test", 2);
        store.insert_mirrored("aaa", make_tick(1)).unwrap();
        store.insert_mirrored("bbb", make_tick(1)).unwrap();
        assert!(!store.can_accept_mirror("ccc"));
    }

    #[test]
    fn insert_and_get_mirrored() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("tbid1", make_tick(1)).unwrap();
        store.insert_mirrored("tbid1", make_tick(2)).unwrap();
        store.insert_mirrored("tbid1", make_tick(3)).unwrap();

        let records = store.get_mirrored("tbid1", 2, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(*records[0].chronon_number(), 2);
        assert_eq!(*records[1].chronon_number(), 3);
    }

    #[test]
    fn insert_mirrored_avoids_duplicates() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("tbid1", make_tick(5)).unwrap();
        store.insert_mirrored("tbid1", make_tick(5)).unwrap();

        assert_eq!(store.mirror_tick_count("tbid1"), 1);
    }

    #[test]
    fn insert_mirrored_sorts_by_chronon_number() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("tbid1", make_tick(3)).unwrap();
        store.insert_mirrored("tbid1", make_tick(1)).unwrap();
        store.insert_mirrored("tbid1", make_tick(2)).unwrap();

        let records = store.get_mirrored("tbid1", 0, 10).unwrap();
        assert_eq!(*records[0].chronon_number(), 1);
        assert_eq!(*records[1].chronon_number(), 2);
        assert_eq!(*records[2].chronon_number(), 3);
    }

    #[test]
    fn mirror_info_returns_correct_data() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("tbid1", make_tick(10)).unwrap();
        store.insert_mirrored("tbid1", make_tick(20)).unwrap();

        let (count, latest, hash) = store.mirror_info("tbid1").unwrap();
        assert_eq!(count, 2);
        assert_eq!(latest, 20);
        assert!(!hash.is_empty());
    }

    #[test]
    fn mirror_info_returns_none_for_empty() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        assert!(store.mirror_info("nonexistent").is_none());
    }

    #[test]
    fn mirror_info_returns_none_for_empty_tbid() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("tbid1", make_tick(1)).unwrap();
        assert!(store.mirror_info("other").is_none());
    }

    #[test]
    fn mirrored_tbids_returns_all_keys() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        store.insert_mirrored("aaa", make_tick(1)).unwrap();
        store.insert_mirrored("bbb", make_tick(1)).unwrap();

        let mut keys = store.mirrored_tbids();
        keys.sort();
        assert_eq!(keys, vec!["aaa".to_string(), "bbb".to_string()]);
    }

    #[test]
    fn get_mirrored_returns_error_for_missing_tbid() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        let result = store.get_mirrored("nonexistent", 0, 10);
        assert!(result.is_err());
    }

    #[test]
    fn compute_hash_sanity_is_deterministic() {
        let records = vec![make_tick(1), make_tick(2), make_tick(3)];
        let h1 = compute_hash_sanity(&records);
        let h2 = compute_hash_sanity(&records);
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn compute_hash_sanity_empty_records() {
        let records: Vec<ChrononRecord> = vec![];
        let hash = compute_hash_sanity(&records);
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn compute_hash_sanity_changes_with_different_ticks() {
        let r1 = vec![make_tick(1), make_tick(2)];
        let r2 = vec![make_tick(1), make_tick(3)];
        assert_ne!(compute_hash_sanity(&r1), compute_hash_sanity(&r2));
    }

    #[test]
    fn clone_mirror_store_shares_state() {
        let store = MirrorStore::new("/tmp/mirror-test", 64);
        let store2 = store.clone();
        store.insert_mirrored("shared", make_tick(1)).unwrap();
        assert_eq!(store2.mirror_tick_count("shared"), 1);
    }

    #[test]
    fn base_dir_is_preserved() {
        let store = MirrorStore::new("/custom/path", 64);
        assert_eq!(store.base_dir, std::path::Path::new("/custom/path"));
    }
}
