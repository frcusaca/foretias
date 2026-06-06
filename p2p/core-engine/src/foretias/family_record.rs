//! FamilyRecord — k×k cross-signing matrix for TimeFamily membership.
//!
//! A FamilyRecord binds a set of TBIDs together with a k×k signature matrix
//! where `matrix[i][j]` is member i's dual-key signature over member j's TBID.
//! This record requires full (CleanFullyAuthenticated) gate enforcement.

use crate::crypto_server::VerifyOps;
use crate::error::CryptoError;
use crate::foretias::clean_auth::RecordBase;
use crate::foretias::types::Tbid;
use serde::Serialize;

/// Maximum family members (DoS guard).
pub const MAX_FAMILY_MEMBERS: usize = 64;

/// Errors that can occur when constructing a FamilyRecord.
#[derive(Debug, Clone, PartialEq)]
pub enum FamilyError {
    EmptyMembers,
    DuplicateMembers,
    BadMatrixDims,
    OversizedFamily,
}

impl std::fmt::Display for FamilyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FamilyError::EmptyMembers => write!(f, "empty members"),
            FamilyError::DuplicateMembers => write!(f, "duplicate members"),
            FamilyError::BadMatrixDims => write!(f, "bad matrix dimensions"),
            FamilyError::OversizedFamily => write!(f, "family exceeds MAX_FAMILY_MEMBERS"),
        }
    }
}

impl std::error::Error for FamilyError {}

/// Signature-free payload representing a TimeFamily membership record.
/// Carries a k×k cross-signing matrix where matrix[i][j] is the dual-key
/// (Ed25519 ‖ SLH-DSA) signature of member i over member j's TBID.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct FamilyRecord {
    pub members: Vec<String>,
    pub matrix: Vec<Vec<Vec<u8>>>,
}

impl FamilyRecord {
    /// Construct a FamilyRecord, enforcing all invariants.
    pub fn try_new(members: Vec<String>, matrix: Vec<Vec<Vec<u8>>>) -> Result<Self, FamilyError> {
        if members.is_empty() {
            return Err(FamilyError::EmptyMembers);
        }
        if members.len() > MAX_FAMILY_MEMBERS {
            return Err(FamilyError::OversizedFamily);
        }
        let k = members.len();
        // Check duplicates
        let mut seen = Vec::new();
        seen.extend_from_slice(&members);
        seen.sort();
        for i in 0..seen.len() - 1 {
            if seen[i] == seen[i + 1] {
                return Err(FamilyError::DuplicateMembers);
            }
        }
        // Check matrix dimensions: must be k×k
        if matrix.len() != k {
            return Err(FamilyError::BadMatrixDims);
        }
        for row in &matrix {
            if row.len() != k {
                return Err(FamilyError::BadMatrixDims);
            }
        }
        Ok(Self { members, matrix })
    }

    /// Returns the family size k.
    pub fn k(&self) -> usize {
        self.members.len()
    }

    /// Verify the k×k cross-signing matrix.
    ///
    /// For each cell `matrix[i][j]`, verify the Ed25519 portion (first 64 bytes)
    /// is member i's Ed25519 signature over member j's TBID raw bytes.
    /// Returns `Ok(())` if all k² signatures verify, `Err` on first failure.
    pub fn verify_matrix<C: VerifyOps + ?Sized>(&self, crypto: &C) -> Result<(), CryptoError> {
        let k = self.k();
        let tbids: Vec<Tbid> = self
            .members
            .iter()
            .map(|m| Tbid::from_hex(m))
            .collect::<Result<Vec<_>, CryptoError>>()?;

        for i in 0..k {
            for j in 0..k {
                let sig = &self.matrix[i][j];
                if sig.len() < 64 {
                    return Err(CryptoError::BadSignature);
                }
                let sig_arr: [u8; 64] = sig[..64]
                    .try_into()
                    .map_err(|_| CryptoError::BadSignature)?;
                let valid = crypto.verify_ed25519(
                    &crate::core::bindings::ForetiasPubKey32 {
                        bytes: tbids[i].ed25519_public_key(),
                    },
                    &tbids[j].raw_bytes(),
                    &crate::core::bindings::ForetiasSig64 { bytes: sig_arr },
                )?;
                if !valid {
                    return Err(CryptoError::BadSignature);
                }
            }
        }
        Ok(())
    }
}

impl RecordBase for FamilyRecord {
    fn always_require_full_signature(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_members_rejected() {
        let result = FamilyRecord::try_new(vec![], vec![]);
        assert_eq!(result, Err(FamilyError::EmptyMembers));
    }

    #[test]
    fn duplicate_members_rejected() {
        let members = vec!["tbid1".to_string(), "tbid1".to_string()];
        let matrix = vec![vec![vec![], vec![]], vec![vec![], vec![]]];
        let result = FamilyRecord::try_new(members, matrix);
        assert_eq!(result, Err(FamilyError::DuplicateMembers));
    }

    #[test]
    fn bad_matrix_dims_rejected() {
        let members = vec!["tbid1".to_string(), "tbid2".to_string()];
        let matrix = vec![vec![vec![], vec![]]]; // only 1 row, need 2
        let result = FamilyRecord::try_new(members, matrix);
        assert_eq!(result, Err(FamilyError::BadMatrixDims));
    }

    #[test]
    fn bad_row_width_rejected() {
        let members = vec!["tbid1".to_string(), "tbid2".to_string()];
        let matrix = vec![vec![vec![], vec![]], vec![vec![]]]; // row 1 has 1 col, need 2
        let result = FamilyRecord::try_new(members, matrix);
        assert_eq!(result, Err(FamilyError::BadMatrixDims));
    }

    #[test]
    fn oversized_family_rejected() {
        let members: Vec<String> = (0..65).map(|i| format!("tbid{}", i)).collect();
        let result = FamilyRecord::try_new(members, vec![]);
        assert_eq!(result, Err(FamilyError::OversizedFamily));
    }

    #[test]
    fn valid_two_member_family() {
        let members = vec!["tbid1".to_string(), "tbid2".to_string()];
        let matrix = vec![
            vec![vec![1u8; 64], vec![2u8; 64]],
            vec![vec![3u8; 64], vec![4u8; 64]],
        ];
        let record = FamilyRecord::try_new(members, matrix).unwrap();
        assert_eq!(record.k(), 2);
        assert!(record.always_require_full_signature());
    }

    #[test]
    fn max_size_family_accepted() {
        let members: Vec<String> = (0..64).map(|i| format!("tbid{}", i)).collect();
        let matrix: Vec<Vec<Vec<u8>>> = (0..64)
            .map(|_| (0..64).map(|_| vec![0u8; 64]).collect())
            .collect();
        let result = FamilyRecord::try_new(members, matrix);
        assert!(result.is_ok());
    }

    #[test]
    fn verify_matrix_rejects_short_signature() {
        let tbid1 = Tbid::test();
        let tbid2 = Tbid::from_bytes(&[0xBB; 96]).unwrap();
        let members = vec![tbid1.to_hex(), tbid2.to_hex()];
        let matrix = vec![
            vec![vec![1u8; 64], vec![1u8; 64]],
            vec![vec![1u8; 32], vec![1u8; 64]],
        ];
        let record = FamilyRecord::try_new(members, matrix).unwrap();
        let crypto = crate::crypto_server::software::SoftwareCryptoServer::generate(
            crate::crypto_server::ForetiasCurve::Ed25519,
        )
        .unwrap();
        let result = record.verify_matrix(&crypto);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CryptoError::BadSignature));
    }

    #[test]
    fn verify_matrix_rejects_tampered_signature() {
        let tbid1 = Tbid::test();
        let tbid2 = Tbid::from_bytes(&[0xBB; 96]).unwrap();
        let members = vec![tbid1.to_hex(), tbid2.to_hex()];
        let matrix = vec![
            vec![vec![0xFF; 64], vec![0xFF; 64]],
            vec![vec![0xFF; 64], vec![0xFF; 64]],
        ];
        let record = FamilyRecord::try_new(members, matrix).unwrap();
        let crypto = crate::crypto_server::software::SoftwareCryptoServer::generate(
            crate::crypto_server::ForetiasCurve::Ed25519,
        )
        .unwrap();
        let result = record.verify_matrix(&crypto);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CryptoError::BadSignature));
    }
}
