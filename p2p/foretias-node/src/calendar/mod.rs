//! Calendar component — owns calendar data, receives tick notifications.

use std::sync::Arc;

use foretias_core::foretias::callbacks::TickObserver;
use foretias_core::foretias::tick::CalendarLookup;
use foretias_core::foretias::{Calendar as CoreCalendar, TickRecord};
use foretias_core::error::NodeError;
use parking_lot::RwLock;

pub struct Calendar {
    inner: Arc<RwLock<CoreCalendar>>,
    tbn: String,
}

impl Calendar {
    pub fn new(tbid: [u8; 16], tbn: &str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CoreCalendar::new(tbid, tbn))),
            tbn: tbn.to_string(),
        }
    }

    pub fn from_persisted(path: &str) -> Result<Self, NodeError> {
        let cal = CoreCalendar::load(path)?;
        let tbn = cal.tbn.clone();
        Ok(Self {
            inner: Arc::new(RwLock::new(cal)),
            tbn,
        })
    }

    pub fn inner(&self) -> Arc<RwLock<CoreCalendar>> {
        self.inner.clone()
    }

    pub fn save(&self, path: &str) -> Result<(), NodeError> {
        self.inner.read().save(path)
    }
}

impl TickObserver for Calendar {
    fn on_tick_advance(&self, _tick_number: u64, _public_key: &[u8; 32], tick_record: &TickRecord) {
        let mut cal = self.inner.write();
        if let Err(e) = cal.append(tick_record.clone()) {
            tracing::error!("Calendar::on_tick_advance failed: {}", e);
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

    fn tbid(&self) -> [u8; 16] {
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
            public_key: vec![0u8; 32],
            forward_foretis: vec![],
            backward_foretis: vec![],
            aa_nonce: [0u8; 16],
            stamps_per_tick: 0,
            external_attestations: Vec::new(),
        }
    }

    #[test]
    fn tick_observer_appends_record() {
        let cal = Calendar::new([0x01; 16], "observer-test");
        let tick = make_tick(1);

        cal.on_tick_advance(1, &[0u8; 32], &tick);

        assert_eq!(cal.latest(), Some(1));
        let records = cal.get(1, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tick_number, 1);
    }

    #[test]
    fn tick_observer_rejects_duplicate() {
        let cal = Calendar::new([0x02; 16], "dup-test");
        let tick = make_tick(5);

        cal.on_tick_advance(5, &[0u8; 32], &tick);
        cal.on_tick_advance(5, &[0u8; 32], &tick);

        assert_eq!(cal.latest(), Some(5));
        let records = cal.get(5, 10).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn calendar_lookup_get_returns_ticks() {
        let cal = Calendar::new([0x03; 16], "lookup-test");
        cal.on_tick_advance(1, &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(2, &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(3, &[0u8; 32], &make_tick(3));

        let records = cal.get(2, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].tick_number, 2);
        assert_eq!(records[1].tick_number, 3);
    }

    #[test]
    fn calendar_lookup_get_respects_count() {
        let cal = Calendar::new([0x04; 16], "count-test");
        cal.on_tick_advance(1, &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(2, &[0u8; 32], &make_tick(2));
        cal.on_tick_advance(3, &[0u8; 32], &make_tick(3));

        let records = cal.get(1, 2).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn calendar_lookup_latest_on_empty() {
        let cal = Calendar::new([0x05; 16], "empty-test");
        assert_eq!(cal.latest(), None);
    }

    #[test]
    fn calendar_lookup_tbid() {
        let tbid = [0xAB; 16];
        let cal = Calendar::new(tbid, "tbid-test");
        assert_eq!(cal.tbid(), tbid);
    }

    #[test]
    fn calendar_lookup_tbn() {
        let cal = Calendar::new([0x06; 16], "my-name");
        assert_eq!(cal.tbn(), "my-name");
    }

    #[test]
    fn calendar_save_and_load() {
        let cal = Calendar::new([0x07; 16], "persist-test");
        cal.on_tick_advance(1, &[0u8; 32], &make_tick(1));
        cal.on_tick_advance(2, &[0u8; 32], &make_tick(2));

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
        let cal = Calendar::new([0x08; 16], "arc-test");
        let inner = cal.inner();
        assert_eq!(inner.read().ticks.len(), 0);
    }
}
