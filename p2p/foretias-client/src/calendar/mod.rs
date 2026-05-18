//! Calendar component — owns calendar data, receives tick notifications.

use std::sync::Arc;

pub mod mirror;

use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::tick::CalendarLookup;
use foretias_core::foretias::{Calendar as CoreCalendar, TickRecord, types::{TickNumber, Tbid}};
use foretias_core::error::NodeError;
use parking_lot::RwLock;
use tracing::{debug, info};

pub use mirror::{MirrorStore, compute_hash_sanity};

pub struct Calendar {
    inner: Arc<RwLock<CoreCalendar>>,
    tbn: String,
}

impl Calendar {
    pub fn new(tbid: Tbid, tbn: &str) -> Self {
        info!(component = "calendar", tbid = %tbid.to_hex(), tbn = %tbn, "calendar initialized");
        Self {
            inner: Arc::new(RwLock::new(CoreCalendar::new(tbid, tbn))),
            tbn: tbn.to_string(),
        }
    }

    pub fn from_persisted(path: &str) -> Result<Self, NodeError> {
        let cal = CoreCalendar::load(path)?;
        let tbid = cal.tbid();
        let tbn = cal.tbn.clone();
        info!(component = "calendar", tbid = %tbid.to_hex(), tbn = %tbn, "calendar loaded from persisted: {}", path);
        Ok(Self {
            inner: Arc::new(RwLock::new(cal)),
            tbn,
        })
    }

    pub fn inner(&self) -> Arc<RwLock<CoreCalendar>> {
        self.inner.clone()
    }

    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        let cal = self.inner.read();
        let tick_count = cal.ticks.len();
        cal.save(path)?;
        info!(component = "calendar", tbid = %cal.tbid().to_hex(), tick_count, "calendar saved to: {}", path);
        Ok(())
    }

    /// Save the calendar wrapped in `PersistedCalendar` format (with metadata header).
    pub fn save_as_persisted(&self, path: &str) -> Result<(), NodeError> {
        use foretias_core::config::{PersistedCalendar, CalendarMetadata, CalendarConfig};

        let cal = self.inner.read();
        let persisted = PersistedCalendar {
            config: CalendarMetadata {
                tbid: cal.tbid().to_hex(),
                tbn: cal.tbn().to_string(),
                stamp_tbid: cal.tbid().to_hex(),
                persisted_by: env!("CARGO_PKG_VERSION").to_string(),
                calendar_config: CalendarConfig::default(),
            },
            ticks: cal.ticks.iter()
                .map(|t| serde_json::to_value(t).unwrap_or_default())
                .collect(),
        };
        let json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| NodeError::Internal(format!("failed to serialize persisted calendar: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| NodeError::Internal(format!("failed to write persisted calendar: {}", e)))?;
        Ok(())
    }
}

impl TickObserver for Calendar {
    fn on_tick_advance(&self, tick_number: TickNumber, _public_key: &[u8], tick_record: &TickRecord) {
        let mut cal = self.inner.write();
        if let Err(e) = cal.append(tick_record.clone()) {
            tracing::error!(component = "calendar", tbid = %cal.tbid().to_hex(), tick = tick_number.0, "calendar: on_tick_advance failed: {}", e);
        } else {
            debug!(component = "calendar", tbid = %cal.tbid().to_hex(), tick = tick_number.0, tick_count = cal.ticks.len(), "calendar: heartbeat");
        }
    }
}

impl CalendarLookup for Calendar {
    fn get(&self, tick_number: u64, count: usize) -> Result<Vec<TickRecord>, NodeError> {
        self.inner.read().get(tick_number, count)
    }

    fn latest(&self) -> Option<u64> {
        self.inner.read().latest()
    }

    fn tbid(&self) -> Tbid {
        self.inner.read().tbid()
    }

    fn tbn(&self) -> &str {
        &self.tbn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            genesis_signature: Vec::new().into(),
            tb_version: 0,
        }
    }

    #[test]
    fn tick_observer_appends_record() {
        let cal = Calendar::new(Tbid::from_raw([0x01; 96]), "observer-test");
        let tick = make_tick(1);
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &tick);
        assert_eq!(cal.latest(), Some(1));
        let records = cal.get(1, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tick_number, 1);
    }

    #[test]
    fn tick_observer_rejects_duplicate() {
        let cal = Calendar::new(Tbid::from_raw([0x02; 96]), "dup-test");
        let tick = make_tick(5);
        cal.on_tick_advance(TickNumber(5), &[0u8; 32], &tick);
        cal.on_tick_advance(TickNumber(5), &[0u8; 32], &tick);
        assert_eq!(cal.latest(), Some(5));
        let records = cal.get(5, 10).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn calendar_lookup_get_returns_ticks() {
        let cal = Calendar::new(Tbid::from_raw([0x03; 96]), "lookup-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(TickNumber(3), &[0u8; 32], &make_tick(3));
        let records = cal.get(2, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].tick_number, 2);
    }

    #[test]
    fn calendar_lookup_get_respects_count() {
        let cal = Calendar::new(Tbid::from_raw([0x04; 96]), "count-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(TickNumber(3), &[0u8; 32], &make_tick(3));
        let records = cal.get(1, 2).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn calendar_lookup_latest_on_empty() {
        let cal = Calendar::new(Tbid::from_raw([0x05; 96]), "empty-test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn calendar_lookup_tbid() {
        let tbid = Tbid::from_raw([0xAB; 96]);
        let cal = Calendar::new(tbid, "tbid-test");
        assert_eq!(cal.tbid(), tbid);
    }

    #[test]
    fn calendar_lookup_tbn() {
        let cal = Calendar::new(Tbid::from_raw([0x06; 96]), "my-name");
        assert_eq!(cal.tbn(), "my-name");
    }

    #[test]
    fn calendar_save_and_load() {
        let cal = Calendar::new(Tbid::from_raw([0x07; 96]), "persist-test");
        cal.on_tick_advance(TickNumber(1), &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(TickNumber(2), &[0u8; 32], &make_tick(2));
        let path = "/tmp/foretias-test-calendar.json";
        cal.save(path).unwrap();
        let loaded = Calendar::from_persisted(path).unwrap();
        assert_eq!(loaded.latest(), Some(2));
        assert_eq!(loaded.tbn(), "persist-test");
        let records = loaded.get(1, 10).unwrap();
        assert_eq!(records.len(), 2);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn inner_returns_arc() {
        let cal = Calendar::new(Tbid::from_raw([0x08; 96]), "arc-test");
        let inner = cal.inner();
        assert_eq!(inner.read().ticks.len(), 0);
    }
}
