//! Committee selection — trait and default top-probity implementation.

use std::cmp::Ordering;

use crate::probity::store::ProbityStore;

/// Selects committee members and signing threshold from current probity scores.
pub trait CommitteeSelector: Send + Sync {
    /// Called at freeze time. Returns (member_peer_ids, threshold_k).
    fn select(&self, store: &ProbityStore) -> (Vec<String>, u32);
}

/// Default v0.8 selector: top-scoring peers by current probity score.
pub struct TopProbitySelector {
    pub committee_size: usize,
    pub threshold_k:    u32,
}

impl CommitteeSelector for TopProbitySelector {
    fn select(&self, store: &ProbityStore) -> (Vec<String>, u32) {
        let mut scored: Vec<(String, f32)> = store.all_scores();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let members: Vec<String> = scored.into_iter()
            .take(self.committee_size)
            .map(|(id, _)| id)
            .collect();
        (members, self.threshold_k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_probity_selector_selects_top_n() {
        let store = ProbityStore::new();
        for i in 0..10u8 {
            store.set_score(format!("peer-{}", i), (10 - i) as f32 * 10.0);
        }
        // peer-0=90, peer-1=80, ..., peer-9=0
        let selector = TopProbitySelector {
            committee_size: 3,
            threshold_k: 2,
        };
        let (members, threshold) = selector.select(&store);
        assert_eq!(threshold, 2);
        assert_eq!(members.len(), 3);
        assert_eq!(members[0], "peer-0");
        assert_eq!(members[1], "peer-1");
        assert_eq!(members[2], "peer-2");
    }

    #[test]
    fn top_probity_selector_empty_store() {
        let store = ProbityStore::new();
        let selector = TopProbitySelector {
            committee_size: 5,
            threshold_k: 3,
        };
        let (members, threshold) = selector.select(&store);
        assert!(members.is_empty());
        assert_eq!(threshold, 3);
    }
}
