//! Eigenvalue decomposition wrapper for semantic matrices.

use crate::matrix::error::{Result, SemanticMatrixError};
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};

/// Eigenvalue decomposition result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EigenDecomposition {
    /// Eigenvectors (d×k matrix where d is vocabulary size, k is number of eigenvectors)
    pub eigenvectors: DMatrix<f64>,

    /// Eigenvalues (k-vector, sorted descending)
    pub eigenvalues: DVector<f64>,

    /// Coherence score (spectral gap: (λ₁ - λ₂)/λ₁)
    pub coherence_score: f64,
}

impl EigenDecomposition {
    /// Creates a new eigenvalue decomposition.
    ///
    /// # Arguments
    ///
    /// * `eigenvectors` - Matrix of eigenvectors (d×k)
    /// * `eigenvalues` - Vector of eigenvalues (k-vector, must be sorted descending)
    ///
    /// # Returns
    ///
    /// Returns an error if eigenvalues are not sorted descending.
    pub fn new(eigenvectors: DMatrix<f64>, eigenvalues: DVector<f64>) -> Result<Self> {
        // Verify eigenvalues are sorted descending
        for window in eigenvalues.as_slice().windows(2) {
            if window[0] < window[1] {
                return Err(SemanticMatrixError::InvariantViolation(
                    "Eigenvalues must be sorted descending".to_string(),
                ));
            }
        }

        // Compute spectral gap
        let coherence_score = if eigenvalues.len() >= 2 {
            let lambda1 = eigenvalues[0];
            let lambda2 = eigenvalues[1];
            if lambda1 > 0.0 {
                (lambda1 - lambda2) / lambda1
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(Self {
            eigenvectors,
            eigenvalues,
            coherence_score,
        })
    }

    /// Returns the number of eigenvectors.
    pub fn num_components(&self) -> usize {
        self.eigenvalues.len()
    }

    /// Serialize to compact binary format with optional compression
    ///
    /// Uses bincode for efficient serialization of dense matrix data.
    /// For large eigenvector matrices, this can be ~50% smaller than JSON.
    ///
    /// # Returns
    ///
    /// Serialized bytes ready for storage or transmission
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| {
            SemanticMatrixError::SerializationError(format!(
                "Failed to serialize EigenDecomposition: {}",
                e
            ))
        })
    }

    /// Deserialize from compact binary format
    ///
    /// # Arguments
    ///
    /// * `bytes` - Serialized eigendecomposition data
    ///
    /// # Returns
    ///
    /// Reconstructed EigenDecomposition
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes).map_err(|e| {
            SemanticMatrixError::SerializationError(format!(
                "Failed to deserialize EigenDecomposition: {}",
                e
            ))
        })
    }

    /// Serialize to compressed binary format
    ///
    /// Uses zstd compression on top of bincode serialization.
    /// Achieves 40-60% compression ratio on typical eigenvector data.
    ///
    /// # Arguments
    ///
    /// * `compression_level` - Zstd compression level (1-19)
    ///   - 1: Fast, ~30% compression
    ///   - 6: Balanced (default), ~40% compression
    ///   - 19: Maximum, ~60% compression
    ///
    /// # Returns
    ///
    /// Compressed serialized bytes
    pub fn to_compressed_bytes(&self, compression_level: i32) -> Result<Vec<u8>> {
        let serialized = self.to_bytes()?;
        zstd::encode_all(&serialized[..], compression_level).map_err(|e| {
            SemanticMatrixError::SerializationError(format!(
                "Failed to compress EigenDecomposition: {}",
                e
            ))
        })
    }

    /// Deserialize from compressed binary format
    ///
    /// # Arguments
    ///
    /// * `compressed_bytes` - Zstd-compressed serialized data
    ///
    /// # Returns
    ///
    /// Reconstructed EigenDecomposition
    pub fn from_compressed_bytes(compressed_bytes: &[u8]) -> Result<Self> {
        let decompressed = zstd::decode_all(compressed_bytes).map_err(|e| {
            SemanticMatrixError::SerializationError(format!(
                "Failed to decompress EigenDecomposition: {}",
                e
            ))
        })?;
        Self::from_bytes(&decompressed)
    }

    /// Estimate memory size in bytes
    ///
    /// Useful for cache sizing and memory budget tracking.
    ///
    /// # Returns
    ///
    /// Approximate size in bytes of this eigendecomposition in memory
    pub fn estimated_size_bytes(&self) -> usize {
        // Eigenvectors: rows × cols × sizeof(f64)
        let eigenvectors_size =
            self.eigenvectors.nrows() * self.eigenvectors.ncols() * std::mem::size_of::<f64>();
        // Eigenvalues: len × sizeof(f64)
        let eigenvalues_size = self.eigenvalues.len() * std::mem::size_of::<f64>();
        // Coherence score
        let coherence_size = std::mem::size_of::<f64>();

        eigenvectors_size + eigenvalues_size + coherence_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eigen_decomposition_sorted() {
        let eigenvectors = DMatrix::identity(3, 3);
        let eigenvalues = DVector::from_vec(vec![3.0, 2.0, 1.0]);
        let eigen = EigenDecomposition::new(eigenvectors, eigenvalues).unwrap();
        assert_eq!(eigen.num_components(), 3);
        assert!(eigen.coherence_score > 0.0);
    }

    #[test]
    fn test_eigen_decomposition_unsorted_fails() {
        let eigenvectors = DMatrix::identity(3, 3);
        let eigenvalues = DVector::from_vec(vec![1.0, 3.0, 2.0]); // Not sorted
        let result = EigenDecomposition::new(eigenvectors, eigenvalues);
        assert!(result.is_err());
    }
}
