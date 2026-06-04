//! ProbityStore — stores peer probity scores for epoch consensus.

use parking_lot::RwLock;
use std::collections::HashMap;

/// Stores peer probity scores and provides them for committee selection.
pub struct ProbityStore {
    scores: RwLock<HashMap<String, f32>>,
}

impl Default for ProbityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbityStore {
    pub fn new() -> Self {
        Self {
            scores: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_score(&self, peer_id: String, score: f32) {
        self.scores.write().insert(peer_id, score);
    }

    pub fn score(&self, peer_id: &str) -> f32 {
        self.scores.read().get(peer_id).copied().unwrap_or(0.0)
    }

    /// Return all (peer_id, score) pairs.
    pub fn all_scores(&self) -> Vec<(String, f32)> {
        self.scores
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// Snapshot of current scores for epoch consensus.
    pub fn snapshot_scores(&self) -> Vec<(String, f32)> {
        self.all_scores()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probity_store_all_scores() {
        let store = ProbityStore::new();
        store.set_score("A".into(), 10.0);
        store.set_score("B".into(), 20.0);
        let mut scores = store.all_scores();
        scores.sort_by_key(|(k, _)| k.clone());
        assert_eq!(scores, vec![("A".into(), 10.0), ("B".into(), 20.0)]);
    }

    #[test]
    fn probity_store_snapshot_scores() {
        let store = ProbityStore::new();
        store.set_score("X".into(), 5.0);
        let snapshot = store.snapshot_scores();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].0, "X");
        assert_eq!(snapshot[0].1, 5.0);
    }
}
