pub mod encrypted_jsonl;
pub mod lru;

use std::sync::Arc;

use foretias_core::core::merkle::{merkle_leaf, merkle_range_proof, merkle_verify_range_proof};
use foretias_core::core::{ForetiasHash32, ForetiasMerkleRangeProof};
use foretias_core::foretias::clean_auth::CleanAuthenticated;
use foretias_core::foretias::ChrononRecord;

use encrypted_jsonl::EncryptedJsonlCalendarStore;

// ---------------------------------------------------------------------------
// Storage proof types
// ---------------------------------------------------------------------------

/// Request for a storage proof covering a chronon range.
#[derive(Debug, Clone)]
pub struct StorageProofRequest {
    /// TBID of the calendar being proved.
    pub tbid: String,
    /// Inclusive start of the chronon range.
    pub chronon_start: u64,
    /// Inclusive end of the chronon range.
    pub chronon_end: u64,
}

/// Merkle range proof for a single calendar block.
#[derive(Debug, Clone)]
pub struct BlockProof {
    /// Identifier of the calendar block.
    pub block_id: u64,
    /// Merkle root stored in the block header.
    pub merkle_root: [u8; 32],
    /// Leaf hashes included in the range proof (ticks within the requested range).
    pub leaves: [[u8; 32]; 64],
    /// Sibling hashes from the Merkle range proof.
    pub siblings: [[u8; 32]; 32],
    /// Number of valid entries in `leaves`.
    pub leaf_count: usize,
    /// Number of valid entries in `siblings`.
    pub sibling_count: usize,
    /// Total number of leaves in the tree this proof was generated from.
    pub n: usize,
}

/// Response containing per-block Merkle proofs for a chronon range.
#[derive(Debug, Clone)]
pub struct StorageProofResponse {
    /// One proof per block that covers any part of the requested range.
    pub blocks: Vec<BlockProof>,
    /// Fraction of the requested chronon range covered by stored ticks.
    pub coverage_ratio: f64,
}

/// Result of verifying a storage proof.
#[derive(Debug, Clone)]
pub struct StorageProofResult {
    /// `true` if all block proofs verified successfully.
    pub verified: bool,
    /// Fraction of the requested chronon range covered by stored ticks.
    pub coverage_ratio: f64,
}

// ---------------------------------------------------------------------------
// Chronon lookup types
// ---------------------------------------------------------------------------

/// Coverage information for a chronon range query.
#[derive(Debug, Clone)]
pub struct CoverageInfo {
    /// Number of chronons in the requested range.
    pub requested: u64,
    /// Number of chronons actually returned.
    pub returned: u64,
}

/// Result of a chronon chain lookup over a range.
#[derive(Debug)]
pub enum ChrononChainResult {
    /// All requested chronons were found.
    Complete {
        records: Vec<CleanAuthenticated<ChrononRecord>>,
        coverage: CoverageInfo,
    },
    /// Some chronons found, some missing.
    Partial {
        records: Vec<CleanAuthenticated<ChrononRecord>>,
        coverage: CoverageInfo,
        gaps: Vec<std::ops::Range<u64>>,
    },
    /// No chronons found in the requested range.
    None { coverage: CoverageInfo },
}

// ---------------------------------------------------------------------------
// CalendarStore — thin wrapper exposing prove_storage and chronon lookups
// ---------------------------------------------------------------------------

/// Calendar store providing storage proof generation.
pub struct CalendarStore {
    inner: Arc<EncryptedJsonlCalendarStore>,
}

impl CalendarStore {
    /// Create a new `CalendarStore` wrapping an existing encrypted store.
    pub fn new(inner: Arc<EncryptedJsonlCalendarStore>) -> Self {
        Self { inner }
    }

    /// Generate a Merkle storage proof for the requested chronon range.
    ///
    /// Returns `None` if no blocks cover any part of the requested range.
    pub fn prove_storage(&self, req: &StorageProofRequest) -> Option<StorageProofResponse> {
        if req.chronon_start > req.chronon_end {
            return None;
        }

        let blocks = match self.inner.read_all() {
            Ok(b) => b,
            Err(_) => return None,
        };

        let requested_count = req.chronon_end - req.chronon_start + 1;
        let mut block_proofs = Vec::new();
        let mut covered_ticks: u64 = 0;

        for block in &blocks {
            let has_overlap = block.ticks.iter().any(|t| {
                let cn = *t.inner().chronon_number();
                cn >= req.chronon_start && cn <= req.chronon_end
            });
            if !has_overlap {
                continue;
            }

            let leaves: Vec<ForetiasHash32> = block
                .ticks
                .iter()
                .map(|t| {
                    let canonical = serde_json::to_vec(t.inner()).unwrap_or_default();
                    merkle_leaf(&canonical).unwrap_or(ForetiasHash32 { bytes: [0u8; 32] })
                })
                .collect();

            if leaves.is_empty() {
                block_proofs.push(BlockProof {
                    block_id: block.block_id,
                    merkle_root: block.merkle_root,
                    leaves: [[0u8; 32]; 64],
                    siblings: [[0u8; 32]; 32],
                    leaf_count: 0,
                    sibling_count: 0,
                    n: 0,
                });
                continue;
            }

            let block_start = find_tick_index(&block.ticks, req.chronon_start);
            let block_end = find_tick_index_after(&block.ticks, req.chronon_end);

            if block_start >= block_end || block_end > leaves.len() {
                continue;
            }

            let proof = match merkle_range_proof(&leaves, block_start, block_end) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let mut proof_leaves = [[0u8; 32]; 64];
            let mut proof_siblings = [[0u8; 32]; 32];

            for (i, leaf) in proof_leaves
                .iter_mut()
                .enumerate()
                .take(proof.leaf_count as usize)
            {
                *leaf = proof.leaves[i].bytes;
            }
            for (i, sibling) in proof_siblings
                .iter_mut()
                .enumerate()
                .take(proof.sibling_count as usize)
            {
                *sibling = proof.siblings[i].bytes;
            }

            covered_ticks += (block_end - block_start) as u64;

            block_proofs.push(BlockProof {
                block_id: block.block_id,
                merkle_root: block.merkle_root,
                leaves: proof_leaves,
                siblings: proof_siblings,
                leaf_count: proof.leaf_count as usize,
                sibling_count: proof.sibling_count as usize,
                n: proof.n as usize,
            });
        }

        if block_proofs.is_empty() {
            return None;
        }

        Some(StorageProofResponse {
            blocks: block_proofs,
            coverage_ratio: covered_ticks as f64 / requested_count as f64,
        })
    }

    /// Look up a single chronon by number across all stored blocks.
    ///
    /// Returns `None` if the chronon is not found or the store cannot be read.
    pub fn get_chronon(
        &self,
        _tbid: &str,
        chronon_number: u64,
        include_attestations: bool,
    ) -> Option<CleanAuthenticated<ChrononRecord>> {
        let blocks = self.inner.read_all().ok()?;
        for block in &blocks {
            for tick in &block.ticks {
                if *tick.inner().chronon_number() == chronon_number {
                    let mut record = tick.inner().clone();
                    if !include_attestations {
                        record.external_attestations_mut().clear();
                    }
                    return Some(CleanAuthenticated::from_trusted(record));
                }
            }
        }
        None
    }

    /// Look up a range of chronons by number across all stored blocks.
    ///
    /// Returns a `ChrononChainResult` indicating whether the range was
    /// fully covered, partially covered, or empty.
    pub fn get_chronon_chain(
        &self,
        _tbid: &str,
        chronon_start: u64,
        chronon_end: u64,
        include_attestations: bool,
    ) -> ChrononChainResult {
        if chronon_start > chronon_end {
            return ChrononChainResult::None {
                coverage: CoverageInfo {
                    requested: 0,
                    returned: 0,
                },
            };
        }

        let requested = chronon_end - chronon_start + 1;

        let blocks = match self.inner.read_all() {
            Ok(b) => b,
            Err(_) => {
                return ChrononChainResult::None {
                    coverage: CoverageInfo {
                        requested,
                        returned: 0,
                    },
                }
            }
        };

        let mut found: Vec<(u64, CleanAuthenticated<ChrononRecord>)> = Vec::new();
        for block in &blocks {
            for tick in &block.ticks {
                let cn = *tick.inner().chronon_number();
                if cn >= chronon_start && cn <= chronon_end {
                    let mut record = tick.inner().clone();
                    if !include_attestations {
                        record.external_attestations_mut().clear();
                    }
                    found.push((cn, CleanAuthenticated::from_trusted(record)));
                }
            }
        }

        found.sort_by_key(|(cn, _)| *cn);
        found.dedup_by_key(|(cn, _)| *cn);

        let returned = found.len() as u64;
        let records: Vec<CleanAuthenticated<ChrononRecord>> =
            found.into_iter().map(|(_, r)| r).collect();
        let coverage = CoverageInfo {
            requested,
            returned,
        };

        if returned == 0 {
            return ChrononChainResult::None { coverage };
        }

        if returned == requested {
            return ChrononChainResult::Complete { records, coverage };
        }

        let gaps = find_gaps(&records, chronon_start, chronon_end);
        ChrononChainResult::Partial {
            records,
            coverage,
            gaps,
        }
    }
}

// ---------------------------------------------------------------------------
// verify_storage_proof (standalone function)
// ---------------------------------------------------------------------------

/// Verify a storage proof against the provided request.
///
/// If `known_roots` is non-empty, each block's `merkle_root` is checked
/// against the set; blocks whose root is not trusted are treated as
/// unverified.
pub fn verify_storage_proof(
    req: &StorageProofRequest,
    resp: &StorageProofResponse,
    known_roots: &[[u8; 32]],
) -> StorageProofResult {
    let requested_count = if req.chronon_start > req.chronon_end {
        0
    } else {
        req.chronon_end - req.chronon_start + 1
    };
    if requested_count == 0 {
        return StorageProofResult {
            verified: false,
            coverage_ratio: 0.0,
        };
    }

    let mut all_verified = true;
    let mut covered_ticks: u64 = 0;

    for bp in &resp.blocks {
        if bp.n == 0 || bp.leaf_count == 0 {
            continue;
        }

        let reconstructed = reconstruct_range_proof(bp);

        let root = ForetiasHash32 {
            bytes: bp.merkle_root,
        };

        let valid =
            merkle_verify_range_proof(&root, &reconstructed, 0, bp.leaf_count).unwrap_or(false);

        if !valid {
            all_verified = false;
            continue;
        }

        if !known_roots.is_empty() && !known_roots.contains(&bp.merkle_root) {
            all_verified = false;
            continue;
        }

        covered_ticks += bp.leaf_count as u64;
    }

    StorageProofResult {
        verified: all_verified,
        coverage_ratio: covered_ticks as f64 / requested_count as f64,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Reconstruct a `ForetiasMerkleRangeProof` from a `BlockProof`.
fn reconstruct_range_proof(bp: &BlockProof) -> ForetiasMerkleRangeProof {
    let mut proof = ForetiasMerkleRangeProof {
        leaves: [ForetiasHash32 { bytes: [0u8; 32] }; 64],
        siblings: [ForetiasHash32 { bytes: [0u8; 32] }; 32],
        leaf_count: bp.leaf_count as i32,
        sibling_count: bp.sibling_count as i32,
        n: bp.n as i32,
    };
    for i in 0..bp.leaf_count.min(64) {
        proof.leaves[i] = ForetiasHash32 {
            bytes: bp.leaves[i],
        };
    }
    for i in 0..bp.sibling_count.min(32) {
        proof.siblings[i] = ForetiasHash32 {
            bytes: bp.siblings[i],
        };
    }
    proof
}

/// Find the index of the first tick whose chronon_number >= target.
fn find_tick_index(
    ticks: &[foretias_core::foretias::clean_auth::Externalized<
        foretias_core::foretias::ChrononRecord,
    >],
    target: u64,
) -> usize {
    ticks
        .iter()
        .position(|t| *t.inner().chronon_number() >= target)
        .unwrap_or(ticks.len())
}

/// Find the index just past the last tick whose chronon_number <= target.
fn find_tick_index_after(
    ticks: &[foretias_core::foretias::clean_auth::Externalized<
        foretias_core::foretias::ChrononRecord,
    >],
    target: u64,
) -> usize {
    ticks
        .iter()
        .rposition(|t| *t.inner().chronon_number() <= target)
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn find_gaps(
    records: &[CleanAuthenticated<ChrononRecord>],
    start: u64,
    end: u64,
) -> Vec<std::ops::Range<u64>> {
    let mut gaps = Vec::new();
    let mut expected = start;

    for record in records {
        let cn = *record.inner().chronon_number();
        if cn > expected {
            gaps.push(expected..cn);
        }
        expected = cn + 1;
    }

    if expected <= end {
        gaps.push(expected..end + 1);
    }

    gaps
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server;
    use foretias_core::foretias::clean_auth::CleanAuthenticated;
    use foretias_core::foretias::{ChrononRecord};

    fn make_server() -> Arc<dyn foretias_core::crypto_server::CryptoServer> {
        let server: Box<dyn foretias_core::crypto_server::CryptoServer> =
            crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519)
                .expect("failed to create software crypto server");
        Arc::from(server)
    }

    fn make_tick(
        chronon_number: u64,
    ) -> foretias_core::foretias::clean_auth::Externalized<ChrononRecord> {
        let record = ChrononRecord::builder()
            .chronon_number(chronon_number)
            .public_key(vec![0u8; 32].into())
            .forward_foretis(vec![].into())
            .backward_foretis(vec![].into())
            .aa_nonce([0u8; 16].into())
            .tb_version(0)
            .build_unchecked();
        CleanAuthenticated::<ChrononRecord>::from_trusted(record).externalize()
    }

    fn make_store(dir: &std::path::Path, n_blocks: u64, ticks_per_block: u64) -> CalendarStore {
        let path = dir.join("calendar.jsonl");
        let server = make_server();
        let store = EncryptedJsonlCalendarStore::new(path, server);
        for block_idx in 0..n_blocks {
            let ticks: Vec<_> = (0..ticks_per_block)
                .map(|i| make_tick(block_idx * ticks_per_block + i))
                .collect();
            store.append_block(ticks).unwrap();
        }
        CalendarStore::new(Arc::new(store))
    }

    #[test]
    fn prove_storage_single_block() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-single-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 5);

        let resp = store
            .prove_storage(&StorageProofRequest {
                tbid: "test".into(),
                chronon_start: 0,
                chronon_end: 4,
            })
            .expect("should return proof");

        assert_eq!(resp.blocks.len(), 1);
        assert_eq!(resp.blocks[0].block_id, 0);
        assert!((resp.coverage_ratio - 1.0).abs() < f64::EPSILON);
        assert_eq!(resp.blocks[0].leaf_count, 5);
        assert!(resp.blocks[0].sibling_count > 0);
        assert_eq!(resp.blocks[0].n, 5);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn prove_storage_multi_block() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-multi-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 3, 4);

        let resp = store
            .prove_storage(&StorageProofRequest {
                tbid: "test".into(),
                chronon_start: 0,
                chronon_end: 11,
            })
            .expect("should return proof");

        assert_eq!(resp.blocks.len(), 3);
        assert!((resp.coverage_ratio - 1.0).abs() < f64::EPSILON);

        for bp in &resp.blocks {
            assert_eq!(bp.leaf_count, 4);
            assert_eq!(bp.n, 4);
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn prove_storage_missing_block_returns_none() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-miss-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 2, 3);

        let result = store.prove_storage(&StorageProofRequest {
            tbid: "test".into(),
            chronon_start: 100,
            chronon_end: 200,
        });

        assert!(result.is_none());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn prove_storage_partial_coverage() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-part-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // 1 block of 5 ticks, chronon numbers 0..=4.
        let store = make_store(&tmp, 1, 5);

        // Request a wider range: 0..=9, only 0..=4 exist.
        let resp = store
            .prove_storage(&StorageProofRequest {
                tbid: "test".into(),
                chronon_start: 0,
                chronon_end: 9,
            })
            .expect("should return proof");

        assert_eq!(resp.blocks.len(), 1);
        assert!((resp.coverage_ratio - 0.5).abs() < f64::EPSILON);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn verify_storage_proof_single_block() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-v1-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 5);

        let req = StorageProofRequest {
            tbid: "test".into(),
            chronon_start: 0,
            chronon_end: 4,
        };
        let resp = store.prove_storage(&req).unwrap();
        let result = verify_storage_proof(&req, &resp, &[]);

        assert!(result.verified);
        assert!((result.coverage_ratio - 1.0).abs() < f64::EPSILON);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn verify_storage_proof_with_known_roots() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-vkr-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 3);

        let req = StorageProofRequest {
            tbid: "test".into(),
            chronon_start: 0,
            chronon_end: 2,
        };
        let resp = store.prove_storage(&req).unwrap();
        let merkle_root = resp.blocks[0].merkle_root;

        // With correct root in known_roots → verified.
        let result = verify_storage_proof(&req, &resp, &[merkle_root]);
        assert!(result.verified);

        // With wrong root → not verified.
        let wrong_root = [0xFFu8; 32];
        let result = verify_storage_proof(&req, &resp, &[wrong_root]);
        assert!(!result.verified);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn verify_storage_proof_partial_range() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-vpr-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // 1 block of 5 ticks (chronon 0..=4). Request 0..=9.
        let store = make_store(&tmp, 1, 5);

        let req = StorageProofRequest {
            tbid: "test".into(),
            chronon_start: 0,
            chronon_end: 9,
        };
        let resp = store.prove_storage(&req).unwrap();
        let result = verify_storage_proof(&req, &resp, &[]);

        assert!(result.verified);
        assert!((result.coverage_ratio - 0.5).abs() < f64::EPSILON);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn prove_storage_invalid_range_returns_none() {
        let tmp = std::env::temp_dir().join(format!("foretias-sp-inv-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 5);

        let result = store.prove_storage(&StorageProofRequest {
            tbid: "test".into(),
            chronon_start: 10,
            chronon_end: 5,
        });

        assert!(result.is_none());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_found() {
        let tmp = std::env::temp_dir().join(format!("foretias-gc-found-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // 2 blocks of 3 ticks each: chronon 0..=2 and 3..=5.
        let store = make_store(&tmp, 2, 3);

        let result = store.get_chronon("test", 4, true);
        assert!(result.is_some());
        assert_eq!(*result.unwrap().inner().chronon_number(), 4);

        let result = store.get_chronon("test", 0, true);
        assert!(result.is_some());
        assert_eq!(*result.unwrap().inner().chronon_number(), 0);

        let result = store.get_chronon("test", 5, true);
        assert!(result.is_some());
        assert_eq!(*result.unwrap().inner().chronon_number(), 5);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_not_found() {
        let tmp = std::env::temp_dir().join(format!("foretias-gc-miss-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 2, 3);

        assert!(store.get_chronon("test", 100, true).is_none());
        assert!(store.get_chronon("test", 6, true).is_none());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_strip_attestations() {
        let tmp = std::env::temp_dir().join(format!("foretias-gc-strip-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 3);

        let with_att = store.get_chronon("test", 0, true).unwrap();
        assert!(with_att.inner().external_attestations().is_empty());

        let without_att = store.get_chronon("test", 0, false).unwrap();
        assert!(without_att.inner().external_attestations().is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_chain_complete() {
        let tmp = std::env::temp_dir().join(format!("foretias-gcc-comp-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // 2 blocks of 5 ticks each: chronon 0..=4 and 5..=9.
        let store = make_store(&tmp, 2, 5);

        match store.get_chronon_chain("test", 2, 7, true) {
            ChrononChainResult::Complete { records, coverage } => {
                assert_eq!(records.len(), 6);
                assert_eq!(coverage.requested, 6);
                assert_eq!(coverage.returned, 6);
                for (i, r) in records.iter().enumerate() {
                    assert_eq!(*r.inner().chronon_number(), 2 + i as u64);
                }
            }
            other => panic!("expected Complete, got {:?}", other),
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_chain_partial() {
        let tmp = std::env::temp_dir().join(format!("foretias-gcc-part-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // 1 block of 3 ticks: chronon 0..=2. Request 0..=5.
        let store = make_store(&tmp, 1, 3);

        match store.get_chronon_chain("test", 0, 5, true) {
            ChrononChainResult::Partial {
                records,
                coverage,
                gaps,
            } => {
                assert_eq!(records.len(), 3);
                assert_eq!(coverage.requested, 6);
                assert_eq!(coverage.returned, 3);
                assert_eq!(gaps.len(), 1);
                assert_eq!(gaps[0], 3..6);
            }
            other => panic!("expected Partial, got {:?}", other),
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_chain_none() {
        let tmp = std::env::temp_dir().join(format!("foretias-gcc-none-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 3);

        match store.get_chronon_chain("test", 100, 200, true) {
            ChrononChainResult::None { coverage } => {
                assert_eq!(coverage.requested, 101);
                assert_eq!(coverage.returned, 0);
            }
            other => panic!("expected None, got {:?}", other),
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn get_chronon_chain_invalid_range() {
        let tmp = std::env::temp_dir().join(format!("foretias-gcc-inv-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let store = make_store(&tmp, 1, 3);

        match store.get_chronon_chain("test", 10, 5, true) {
            ChrononChainResult::None { coverage } => {
                assert_eq!(coverage.requested, 0);
                assert_eq!(coverage.returned, 0);
            }
            other => panic!("expected None, got {:?}", other),
        }

        std::fs::remove_dir_all(&tmp).ok();
    }
}
