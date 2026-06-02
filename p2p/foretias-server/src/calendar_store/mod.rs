pub mod encrypted_jsonl;
pub mod lru;

use std::sync::Arc;

use foretias_core::core::merkle::{merkle_leaf, merkle_range_proof, merkle_verify_range_proof};
use foretias_core::core::{ForetiasHash32, ForetiasMerkleRangeProof};

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
// CalendarStore — thin wrapper exposing prove_storage
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
                let cn = t.inner().chronon_number;
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

            for i in 0..(proof.leaf_count as usize) {
                proof_leaves[i] = proof.leaves[i].bytes;
            }
            for i in 0..(proof.sibling_count as usize) {
                proof_siblings[i] = proof.siblings[i].bytes;
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

        if !known_roots.is_empty() && !known_roots.iter().any(|r| *r == bp.merkle_root) {
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
        .position(|t| t.inner().chronon_number >= target)
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
        .rposition(|t| t.inner().chronon_number <= target)
        .map(|i| i + 1)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use foretias_core::crypto_server;
    use foretias_core::foretias::clean_auth::CleanAuthenticated;
    use foretias_core::foretias::{ChrononRecord, Tbid};

    fn make_server() -> Arc<dyn foretias_core::crypto_server::CryptoServer> {
        let server: Box<dyn foretias_core::crypto_server::CryptoServer> =
            crypto_server::new_software(crypto_server::ForetiasCurve::Ed25519)
                .expect("failed to create software crypto server");
        Arc::from(server)
    }

    fn make_tick(
        chronon_number: u64,
    ) -> foretias_core::foretias::clean_auth::Externalized<ChrononRecord> {
        let record = ChrononRecord {
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
        };
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
}
