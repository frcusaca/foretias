//! Safe wrappers for Merkle tree and range proof operations.

use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};

/// Compute a Merkle leaf hash: SHA-256(0x00 || data).
pub fn merkle_leaf(data: &[u8]) -> Result<ForetiasHash32, CryptoError> {
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut out = unsafe { std::mem::zeroed() };
    // SAFETY: data.as_ptr() is valid for data.len() bytes; out is valid ForetiasHash32.
    let rc = unsafe { foretias_merkle_leaf(data.as_ptr(), data.len(), &mut out) };
    c_result_to_error(rc)?;
    Ok(out)
}

/// Verify a single-leaf Merkle inclusion proof.
///
/// Returns `Ok(true)` if the proof is valid, `Ok(false)` if the proof is invalid.
pub fn merkle_verify(
    root: &ForetiasHash32,
    leaf: &ForetiasHash32,
    proof: &ForetiasMerkleProof,
) -> Result<bool, CryptoError> {
    // SAFETY: root, leaf, proof are valid references to #[repr(C)] structs.
    let rc = unsafe { foretias_merkle_verify(root, leaf, proof) };
    if rc == ForetiasResult_FORETIAS_OK {
        Ok(true)
    } else if rc == ForetiasResult_FORETIAS_ERR_BAD_PROOF {
        Ok(false)
    } else {
        c_result_to_error(rc).map(|_| false)
    }
}

/// Compute the Merkle root from an array of leaf hashes.
///
/// `leaves` must be non-empty (returns `CryptoError::ProofRangeEmpty` if empty).
/// The C backend pads to the next power of two; max 128 padded leaves.
pub fn merkle_root_from_leaves(leaves: &[ForetiasHash32]) -> Result<ForetiasHash32, CryptoError> {
    if leaves.is_empty() {
        return Err(CryptoError::ProofRangeEmpty);
    }
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut root_out = unsafe { std::mem::zeroed() };
    // SAFETY: leaves.as_ptr() is valid for leaves.len() * sizeof(ForetiasHash32) bytes.
    let rc =
        unsafe { foretias_merkle_root_from_leaves(leaves.as_ptr(), leaves.len(), &mut root_out) };
    c_result_to_error(rc)?;
    Ok(root_out)
}

/// Generate a range proof for leaves[start..end].
///
/// `leaves` must be non-empty. `start` must be less than `end`, and `end` must
/// not exceed the number of leaves. The range size (`end - start`) must not
/// exceed `FORETIAS_MERKLE_MAX_RANGE_PROOF` (64).
pub fn merkle_range_proof(
    leaves: &[ForetiasHash32],
    start: usize,
    end: usize,
) -> Result<ForetiasMerkleRangeProof, CryptoError> {
    if leaves.is_empty() {
        return Err(CryptoError::ProofRangeEmpty);
    }
    if start >= end {
        return Err(CryptoError::ProofRangeEmpty);
    }
    if end > leaves.len() {
        return Err(CryptoError::ProofRangeExceeds);
    }
    if end - start > FORETIAS_MERKLE_MAX_RANGE_PROOF as usize {
        return Err(CryptoError::ProofRangeExceeds);
    }
    // SAFETY: zeroing known-good #[repr(C)] struct from bindings.
    let mut proof_out = unsafe { std::mem::zeroed() };
    // SAFETY: leaves.as_ptr() is valid for leaves.len() * sizeof(ForetiasHash32) bytes.
    let rc = unsafe {
        foretias_merkle_range_proof(leaves.as_ptr(), leaves.len(), start, end, &mut proof_out)
    };
    c_result_to_error(rc)?;
    Ok(proof_out)
}

/// Verify a range proof against a Merkle root.
///
/// Returns `Ok(true)` if the proof is valid, `Ok(false)` if invalid.
/// The `start` and `end` must match the values used when generating the proof.
pub fn merkle_verify_range_proof(
    root: &ForetiasHash32,
    proof: &ForetiasMerkleRangeProof,
    start: usize,
    end: usize,
) -> Result<bool, CryptoError> {
    let mut result_out: ForetiasResult = 0;
    // SAFETY: root, proof are valid references; result_out is a valid i32 pointer.
    let rc =
        unsafe { foretias_merkle_verify_range_proof(root, proof, start, end, &mut result_out) };
    c_result_to_error(rc)?;
    if result_out == ForetiasResult_FORETIAS_OK {
        Ok(true)
    } else if result_out == ForetiasResult_FORETIAS_ERR_BAD_PROOF
        || result_out == ForetiasResult_FORETIAS_ERR_PROOF_RANGE_EMPTY
        || result_out == ForetiasResult_FORETIAS_ERR_PROOF_RANGE_EXCEEDS
    {
        Ok(false)
    } else {
        c_result_to_error(result_out).map(|_| false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_leaf_deterministic() {
        let data = b"hello";
        let h1 = merkle_leaf(data).unwrap();
        let h2 = merkle_leaf(data).unwrap();
        assert_eq!(h1.bytes, h2.bytes);
    }

    #[test]
    fn merkle_leaf_different_inputs_differ() {
        let h1 = merkle_leaf(b"a").unwrap();
        let h2 = merkle_leaf(b"b").unwrap();
        assert_ne!(h1.bytes, h2.bytes);
    }

    #[test]
    fn merkle_root_from_single_leaf() {
        let leaf = merkle_leaf(b"only").unwrap();
        let root = merkle_root_from_leaves(&[leaf]).unwrap();
        // For a single leaf, the root should equal the leaf itself
        // (after padding to power-of-2, the tree has one leaf duplicated,
        // then hashed — but for n=1 the padded tree is 1 node).
        // Actually, next_pow2(1) = 1, so root = leaf.
        assert_eq!(root.bytes, leaf.bytes);
    }

    #[test]
    fn merkle_root_from_two_leaves() {
        let l0 = merkle_leaf(b"leaf0").unwrap();
        let l1 = merkle_leaf(b"leaf1").unwrap();
        let root = merkle_root_from_leaves(&[l0, l1]).unwrap();
        assert_ne!(root.bytes, l0.bytes);
        assert_ne!(root.bytes, l1.bytes);
        // Root should be deterministic
        let root2 = merkle_root_from_leaves(&[l0, l1]).unwrap();
        assert_eq!(root.bytes, root2.bytes);
    }

    #[test]
    fn merkle_root_empty_leaves_returns_error() {
        let result = merkle_root_from_leaves(&[]);
        assert!(matches!(result, Err(CryptoError::ProofRangeEmpty)));
    }

    #[test]
    fn merkle_range_proof_and_verify_roundtrip() {
        let leaves: Vec<ForetiasHash32> = (0..8u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let root = merkle_root_from_leaves(&leaves).unwrap();

        // Prove leaves[2..5]
        let proof = merkle_range_proof(&leaves, 2, 5).unwrap();
        assert_eq!(proof.leaf_count, 3);
        assert_eq!(proof.n, 8);
        assert!(proof.sibling_count > 0);

        let valid = merkle_verify_range_proof(&root, &proof, 2, 5).unwrap();
        assert!(valid, "range proof should verify against correct root");
    }

    #[test]
    fn merkle_range_proof_verify_fails_with_wrong_root() {
        let leaves: Vec<ForetiasHash32> = (0..4u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let _root = merkle_root_from_leaves(&leaves).unwrap();

        let different_leaves: Vec<ForetiasHash32> =
            (0..4u8).map(|i| merkle_leaf(&[i + 100]).unwrap()).collect();
        let wrong_root = merkle_root_from_leaves(&different_leaves).unwrap();

        let proof = merkle_range_proof(&leaves, 0, 2).unwrap();
        let valid = merkle_verify_range_proof(&wrong_root, &proof, 0, 2).unwrap();
        assert!(!valid, "range proof should fail against wrong root");
    }

    #[test]
    fn merkle_range_proof_full_range() {
        let leaves: Vec<ForetiasHash32> = (0..4u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let root = merkle_root_from_leaves(&leaves).unwrap();

        let proof = merkle_range_proof(&leaves, 0, 4).unwrap();
        assert_eq!(proof.leaf_count, 4);
        let valid = merkle_verify_range_proof(&root, &proof, 0, 4).unwrap();
        assert!(valid);
    }

    #[test]
    fn merkle_range_proof_empty_leaves_error() {
        let result = merkle_range_proof(&[], 0, 1);
        assert!(matches!(result, Err(CryptoError::ProofRangeEmpty)));
    }

    #[test]
    fn merkle_range_proof_start_geq_end_error() {
        let leaves: Vec<ForetiasHash32> = (0..4u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let result = merkle_range_proof(&leaves, 3, 3);
        assert!(matches!(result, Err(CryptoError::ProofRangeEmpty)));
    }

    #[test]
    fn merkle_range_proof_end_exceeds_leaves_error() {
        let leaves: Vec<ForetiasHash32> = (0..4u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let result = merkle_range_proof(&leaves, 0, 5);
        assert!(matches!(result, Err(CryptoError::ProofRangeExceeds)));
    }

    #[test]
    fn merkle_single_leaf_range_proof() {
        let leaves: Vec<ForetiasHash32> = (0..4u8).map(|i| merkle_leaf(&[i]).unwrap()).collect();
        let root = merkle_root_from_leaves(&leaves).unwrap();

        let proof = merkle_range_proof(&leaves, 1, 2).unwrap();
        assert_eq!(proof.leaf_count, 1);
        let valid = merkle_verify_range_proof(&root, &proof, 1, 2).unwrap();
        assert!(valid);
    }
}
