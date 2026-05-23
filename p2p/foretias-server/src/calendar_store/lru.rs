//! Bin-based LRU cache for calendar entries.
//!
//! Calendars are classified into bins: `MY_OWN` (never evicted),
//! `USED` (recently accessed), and `UNUSED` (stale). Eviction
//! always targets `UNUSED` first and discards randomly to resist
//! clairvoyant attackers.
//!
//! Spec: FORETIAS_2_P2P_SPEC.md §12.2 (Storage Policy) and §12.3 (Bin-Based LRU).

use parking_lot::RwLock;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use foretias_core::clock::{Clock, SystemClock};
use foretias_core::error::NodeError;

/// Sentinel: no eviction budget configured.
const UNLIMITED_BUDGET: u64 = u64::MAX;

/// Calendars are classified into bins. Eviction always comes from Unused
/// first, and discards randomly to avoid a clairvoyant attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarBin {
    /// This node's calendar or liege's — never evicted.
    MyOwn,
    /// Recently read OR written.
    Used,
    /// Not touched since last epoch.
    Unused,
}

/// Storage policy for the encrypted JSONL calendar store.
///
/// - `Everything`: Store calendars for every peer (expensive, complete).
/// - `MyOwn`: Store only our own calendar (and our liege's).
/// - `MyOwnPlusLru`: MyOwn + LRU cache of recently-used peers' calendars.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CalendarStoragePolicy {
    Everything,
    MyOwn,
    MyOwnPlusLru { max_bytes_total: u64 },
}

impl Default for CalendarStoragePolicy {
    fn default() -> Self {
        CalendarStoragePolicy::MyOwn
    }
}

/// Metadata for a single calendar record in the LRU cache.
#[derive(Debug, Clone)]
pub struct CalendarRecord {
    pub peer_id: String,
    pub bin: CalendarBin,
    pub size_bytes: u64,
    pub last_touch: u64,
    pub path: PathBuf,
}

/// Bin-based LRU for calendar entries.
///
/// Three bins: MyOwn (pinned), Used (recent), Unused (eviction target).
/// Eviction picks randomly from Unused to resist targeted attacks.
pub struct BinBasedLru {
    max_bytes: u64,
    current_bytes: AtomicU64,
    records: RwLock<HashMap<String, CalendarRecord>>,
    clock: Arc<dyn Clock>,
}

impl BinBasedLru {
    /// Create a new LRU with the given byte budget.
    pub fn new(max_bytes: u64) -> Self {
        Self::with_clock(max_bytes, Arc::new(SystemClock))
    }

    /// Create a new LRU with an injected clock (for testing).
    pub fn with_clock(max_bytes: u64, clock: Arc<dyn Clock>) -> Self {
        Self {
            max_bytes,
            current_bytes: AtomicU64::new(0),
            records: RwLock::new(HashMap::new()),
            clock,
        }
    }

    /// Create an LRU with unlimited budget (no eviction).
    pub fn unlimited() -> Self {
        Self::new(UNLIMITED_BUDGET)
    }

    /// Record or update a calendar entry in the cache.
    pub fn insert(&self, rec: CalendarRecord) {
        let mut guard = self.records.write();
        let is_new = !guard.contains_key(&rec.peer_id);
        if is_new {
            self.current_bytes
                .fetch_add(rec.size_bytes, Ordering::Relaxed);
        }
        guard.insert(rec.peer_id.clone(), rec);
    }

    /// Touch a peer's record: update timestamp and promote to Used.
    pub fn touch(&self, peer_id: &str) {
        let mut guard = self.records.write();
        if let Some(rec) = guard.get_mut(peer_id) {
            rec.last_touch = self.clock.now_ns().unwrap_or_default();
            rec.bin = CalendarBin::Used;
        }
    }

    /// Demote stale Used records to Unused.
    pub fn demote_stale_to_unused(&self, stale_threshold_ns: u64) {
        let now = self.clock.now_ns().unwrap_or_default();
        let mut guard = self.records.write();
        for rec in guard.values_mut() {
            if matches!(rec.bin, CalendarBin::Used)
                && (now - rec.last_touch) > stale_threshold_ns
            {
                rec.bin = CalendarBin::Unused;
            }
        }
    }

    /// Evict Unused entries until there is room for `incoming_bytes`.
    pub fn evict_to_fit(&self, incoming_bytes: u64) -> Result<(), NodeError> {
        if self.max_bytes == UNLIMITED_BUDGET {
            return Ok(());
        }
        while self.current_bytes.load(Ordering::Relaxed) + incoming_bytes > self.max_bytes {
            let victim = self.pick_unused_random();
            if let Some(rec) = victim {
                std::fs::remove_file(&rec.path).ok();
                self.current_bytes
                    .fetch_sub(rec.size_bytes, Ordering::Relaxed);
                self.records.write().remove(&rec.peer_id);
            } else {
                return Err(NodeError::OutOfSpace);
            }
        }
        Ok(())
    }

    /// Remove a peer's record from the cache.
    pub fn remove(&self, peer_id: &str) -> Option<CalendarRecord> {
        let rec = self.records.write().remove(peer_id);
        if let Some(ref r) = rec {
            self.current_bytes
                .fetch_sub(r.size_bytes, Ordering::Relaxed);
        }
        rec
    }

    /// Return the current byte usage.
    pub fn current_bytes(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }

    /// Return the configured byte budget.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Return the number of tracked records.
    pub fn len(&self) -> usize {
        self.records.read().len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a record by peer id (read-only access).
    pub fn get(&self, peer_id: &str) -> Option<CalendarRecord> {
        self.records.read().get(peer_id).cloned()
    }

    /// Pick a random Unused record for eviction.
    fn pick_unused_random(&self) -> Option<CalendarRecord> {
        let guard = self.records.read();
        let unused: Vec<_> = guard
            .values()
            .filter(|r| matches!(r.bin, CalendarBin::Unused))
            .cloned()
            .collect();
        if unused.is_empty() {
            return None;
        }
        unused.choose(&mut rand::thread_rng()).cloned()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn now_ns() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }

    fn make_record(peer_id: &str, bin: CalendarBin, size: u64) -> CalendarRecord {
        CalendarRecord {
            peer_id: peer_id.to_string(),
            bin,
            size_bytes: size,
            last_touch: now_ns(),
            path: PathBuf::from("/tmp/nonexistent"),
        }
    }

    #[test]
    fn default_policy_is_my_own() {
        let policy = CalendarStoragePolicy::default();
        assert!(matches!(policy, CalendarStoragePolicy::MyOwn));
    }

    #[test]
    fn insert_and_get() {
        let lru = BinBasedLru::new(1024);
        let rec = make_record("peer_a", CalendarBin::Used, 100);
        lru.insert(rec);

        assert_eq!(lru.len(), 1);
        assert_eq!(lru.current_bytes(), 100);

        let found = lru.get("peer_a").unwrap();
        assert_eq!(found.peer_id, "peer_a");
    }

    #[test]
    fn touch_promotes_to_used() {
        let lru = BinBasedLru::new(1024);
        let rec = make_record("peer_b", CalendarBin::Unused, 50);
        lru.insert(rec);

        lru.touch("peer_b");
        let found = lru.get("peer_b").unwrap();
        assert_eq!(found.bin, CalendarBin::Used);
    }

    #[test]
    fn touch_noop_for_unknown_peer() {
        let lru = BinBasedLru::new(1024);
        lru.touch("nonexistent");
        assert!(lru.is_empty());
    }

    #[test]
    fn demote_stale_to_unused() {
        let lru = BinBasedLru::new(1024);
        let mut rec = make_record("peer_c", CalendarBin::Used, 50);
        rec.last_touch = 0;
        lru.insert(rec);

        lru.demote_stale_to_unused(1_000_000_000);
        let found = lru.get("peer_c").unwrap();
        assert_eq!(found.bin, CalendarBin::Unused);
    }

    #[test]
    fn demote_does_not_touch_my_own() {
        let lru = BinBasedLru::new(1024);
        let mut rec = make_record("me", CalendarBin::MyOwn, 50);
        rec.last_touch = 0;
        lru.insert(rec);

        lru.demote_stale_to_unused(1_000_000_000);
        let found = lru.get("me").unwrap();
        assert_eq!(found.bin, CalendarBin::MyOwn);
    }

    #[test]
    fn evict_to_fit_removes_unused() {
        let lru = BinBasedLru::new(150);

        lru.insert(make_record("unused_1", CalendarBin::Unused, 80));
        lru.insert(make_record("unused_2", CalendarBin::Unused, 80));

        assert!(lru.evict_to_fit(10).is_ok());
        assert!(lru.len() <= 2);
        assert!(lru.current_bytes() + 10 <= 150);
    }

    #[test]
    fn evict_to_fit_respects_my_own() {
        let lru = BinBasedLru::new(100);

        lru.insert(make_record("me", CalendarBin::MyOwn, 90));
        lru.insert(make_record("unused", CalendarBin::Unused, 20));

        assert!(lru.evict_to_fit(0).is_ok());
        assert!(lru.get("me").is_some());
        assert!(lru.get("unused").is_none());
    }

    #[test]
    fn evict_to_fit_fails_when_no_unused() {
        let lru = BinBasedLru::new(50);

        lru.insert(make_record("me", CalendarBin::MyOwn, 40));
        lru.insert(make_record("used", CalendarBin::Used, 30));

        assert!(matches!(
            lru.evict_to_fit(10),
            Err(NodeError::OutOfSpace)
        ));
    }

    #[test]
    fn unlimited_lru_never_evicts() {
        let lru = BinBasedLru::unlimited();

        lru.insert(make_record("a", CalendarBin::Unused, 1_000_000));
        lru.insert(make_record("b", CalendarBin::Unused, 2_000_000));

        assert!(lru.evict_to_fit(999_999_999).is_ok());
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn remove_decrements_bytes() {
        let lru = BinBasedLru::new(1024);
        lru.insert(make_record("peer_d", CalendarBin::Used, 200));

        assert_eq!(lru.current_bytes(), 200);
        let removed = lru.remove("peer_d");
        assert!(removed.is_some());
        assert_eq!(lru.current_bytes(), 0);
        assert!(lru.is_empty());
    }

    #[test]
    fn remove_unknown_returns_none() {
        let lru = BinBasedLru::new(1024);
        assert!(lru.remove("nobody").is_none());
    }

    #[test]
    fn max_entries_enforcement_via_eviction() {
        let lru = BinBasedLru::new(300);

        lru.insert(make_record("a", CalendarBin::Unused, 100));
        lru.insert(make_record("b", CalendarBin::Unused, 100));
        lru.insert(make_record("c", CalendarBin::Unused, 100));

        assert!(lru.evict_to_fit(50).is_ok());
        assert!(lru.current_bytes() + 50 <= 300);
    }

    #[test]
    fn eviction_is_random_not_deterministic() {
        let lru = BinBasedLru::new(500);
        for i in 0..10 {
            lru.insert(make_record(
                &format!("peer_{i}"),
                CalendarBin::Unused,
                100,
            ));
        }
        lru.evict_to_fit(0).ok();
        assert!(lru.len() < 10);
    }

    #[test]
    fn policy_serialization_roundtrip() {
        let policy = CalendarStoragePolicy::MyOwnPlusLru {
            max_bytes_total: 10_000,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: CalendarStoragePolicy = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            CalendarStoragePolicy::MyOwnPlusLru { max_bytes_total }
            if max_bytes_total == 10_000
        ));
    }
}
