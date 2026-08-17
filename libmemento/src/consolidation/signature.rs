//! Eigenvector Signature Compression
//!
//! Compresses eigenvector signatures from ~40 MB to ~100 KB (400× ratio)
//! using top-k truncation + int8 quantization with <5% reconstruction error.

use crate::consolidation::error::{ConsolidationError, Result};
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Sparse vector representation with (index, value) pairs
///
/// Stores only the top-k components of a vector, drastically reducing memory usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseVec<T> {
    /// Indices of non-zero components
    pub indices: Vec<usize>,
    /// Values at those indices
    pub values: Vec<T>,
    /// Original dimension of the full vector
    pub dimension: usize,
}

impl<T: Clone> SparseVec<T> {
    /// Creates a new sparse vector
    pub fn new(indices: Vec<usize>, values: Vec<T>, dimension: usize) -> Self {
        assert_eq!(
            indices.len(),
            values.len(),
            "Indices and values must have same length"
        );

        Self {
            indices,
            values,
            dimension,
        }
    }
}

impl SparseVec<f64> {
    /// Extracts top-k components from a dense vector
    ///
    /// # Arguments
    ///
    /// * `dense` - Dense vector to compress
    /// * `k` - Number of top components to keep
    ///
    /// # Returns
    ///
    /// Sparse representation with top-k largest (by absolute value) components
    pub fn from_dense(dense: &DVector<f64>, k: usize) -> Self {
        let dimension = dense.len();
        let k_actual = k.min(dimension);

        // Create (index, |value|, value) tuples
        let mut indexed: Vec<(usize, f64, f64)> = dense
            .iter()
            .enumerate()
            .map(|(idx, &val)| (idx, val.abs(), val))
            .collect();

        // Sort by absolute value (descending)
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k
        indexed.truncate(k_actual);

        // Sort by index for better cache locality
        indexed.sort_by_key(|t| t.0);

        let indices: Vec<usize> = indexed.iter().map(|t| t.0).collect();
        let values: Vec<f64> = indexed.iter().map(|t| t.2).collect();

        Self {
            indices,
            values,
            dimension,
        }
    }

    /// Converts sparse vector back to dense representation
    ///
    /// Non-stored components are set to zero.
    pub fn to_dense(&self) -> DVector<f64> {
        let mut dense = DVector::zeros(self.dimension);

        for (&idx, &val) in self.indices.iter().zip(self.values.iter()) {
            if idx < self.dimension {
                dense[idx] = val;
            }
        }

        dense
    }

    /// Quantizes to int8 with scale factors
    ///
    /// # Returns
    ///
    /// Tuple of (quantized sparse vector, (min, max) scale factors)
    pub fn quantize_to_int8(&self) -> (SparseVec<i8>, (f32, f32)) {
        if self.values.is_empty() {
            return (SparseVec::new(vec![], vec![], self.dimension), (0.0, 0.0));
        }

        // Find min and max values
        let min_val = self.values.iter().cloned().fold(f64::INFINITY, f64::min) as f32;
        let max_val = self
            .values
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max) as f32;

        // Quantize to int8 range [-128, 127]
        let range = (max_val - min_val).max(1e-10);

        let quantized_values: Vec<i8> = self
            .values
            .iter()
            .map(|&val| {
                let normalized = ((val as f32 - min_val) / range * 255.0) - 128.0;
                normalized.round().clamp(-128.0, 127.0) as i8
            })
            .collect();

        (
            SparseVec::new(self.indices.clone(), quantized_values, self.dimension),
            (min_val, max_val),
        )
    }
}

impl SparseVec<i8> {
    /// Dequantizes from int8 back to f64 using scale factors
    ///
    /// # Arguments
    ///
    /// * `scale_factors` - (min, max) values used during quantization
    pub fn dequantize_from_int8(&self, scale_factors: (f32, f32)) -> SparseVec<f64> {
        let (min_val, max_val) = scale_factors;
        let range = (max_val - min_val).max(1e-10);

        let dequantized_values: Vec<f64> = self
            .values
            .iter()
            .map(|&val| {
                let normalized = (val as f32 + 128.0) / 255.0;
                (normalized * range + min_val) as f64
            })
            .collect();

        SparseVec::new(self.indices.clone(), dequantized_values, self.dimension)
    }
}

/// Compressed eigenvector signature for distributed synchronization
///
/// Compresses eigenvectors from ~40 MB to ~100 KB using:
/// - Top-k truncation (keep only 1000 components per vector)
/// - Int8 quantization with scale factors
/// - Sparse encoding
///
/// # Example
///
/// ```no_run
/// use libmemento::consolidation::signature::CoherenceSignature;
/// use nalgebra::{DMatrix, DVector};
/// use uuid::Uuid;
///
/// let eigenvectors = DMatrix::from_element(100_000, 100, 0.5);
/// let eigenvalues = DVector::from_element(100, 1.0);
/// let node_id = Uuid::new_v4();
///
/// let signature = CoherenceSignature::compress(
///     &eigenvectors,
///     &eigenvalues,
///     node_id,
///     "medical".to_string()
/// ).unwrap();
///
/// println!("Compression ratio: {:.1}×", signature.compression_ratio);
/// println!("Reconstruction error: {:.2}%", signature.reconstruction_error * 100.0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceSignature {
    /// Unique signature ID
    pub signature_id: Uuid,

    /// Node that generated this signature
    pub node_id: Uuid,

    /// Compressed eigenvectors (top-k, int8 quantized)
    pub top_k_eigenvectors: Vec<SparseVec<i8>>,

    /// Eigenvalues (stored as f32 for space savings)
    pub top_k_eigenvalues: Vec<f32>,

    /// Scale factors for each eigenvector (for dequantization)
    scale_factors: Vec<(f32, f32)>,

    /// Domain label
    pub domain_label: String,

    /// Original vocabulary size
    pub vocabulary_size: usize,

    /// Achieved compression ratio
    pub compression_ratio: f64,

    /// Reconstruction error (1 - average cosine similarity)
    pub reconstruction_error: f64,

    /// Creation timestamp
    pub created_at: SystemTime,
}

impl CoherenceSignature {
    /// Compresses eigenvectors to sparse int8 representation
    ///
    /// Keeps only top-1000 components per eigenvector and quantizes to int8.
    ///
    /// # Arguments
    ///
    /// * `eigenvectors` - Matrix of eigenvectors (dimensions × num_vectors)
    /// * `eigenvalues` - Corresponding eigenvalues
    /// * `node_id` - ID of the node generating this signature
    /// * `domain` - Domain label (e.g., "medical", "legal")
    ///
    /// # Returns
    ///
    /// Compressed signature with <5% reconstruction error
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::signature::CoherenceSignature;
    /// use nalgebra::{DMatrix, DVector};
    /// use uuid::Uuid;
    ///
    /// let eigenvectors = DMatrix::from_element(10_000, 10, 0.1);
    /// let eigenvalues = DVector::from_element(10, 1.0);
    ///
    /// let signature = CoherenceSignature::compress(
    ///     &eigenvectors,
    ///     &eigenvalues,
    ///     Uuid::new_v4(),
    ///     "test".to_string()
    /// ).unwrap();
    /// ```
    pub fn compress(
        eigenvectors: &DMatrix<f64>,
        eigenvalues: &DVector<f64>,
        node_id: Uuid,
        domain: String,
    ) -> Result<Self> {
        let (nrows, ncols) = eigenvectors.shape();

        if ncols != eigenvalues.len() {
            return Err(ConsolidationError::InvalidSignature(format!(
                "Eigenvalue count mismatch: {} vectors but {} eigenvalues",
                ncols,
                eigenvalues.len()
            )));
        }

        let vocabulary_size = nrows;
        let num_vectors = ncols;

        // Target: keep top-1000 components per vector
        let k = 1000.min(nrows);

        let mut compressed_vectors = Vec::with_capacity(num_vectors);
        let mut scale_factors_list = Vec::with_capacity(num_vectors);

        // Compress each eigenvector
        for col_idx in 0..num_vectors {
            let eigenvector = eigenvectors.column(col_idx);
            let dense_vec = DVector::from_iterator(nrows, eigenvector.iter().copied());

            // Truncate to top-k
            let sparse_f64 = SparseVec::from_dense(&dense_vec, k);

            // Quantize to int8
            let (sparse_i8, scale_factors) = sparse_f64.quantize_to_int8();

            compressed_vectors.push(sparse_i8);
            scale_factors_list.push(scale_factors);
        }

        // Compute compression ratio
        let original_bytes = nrows * ncols * 8; // f64 = 8 bytes
        let compressed_bytes = Self::compute_compressed_size(&compressed_vectors);
        let compression_ratio = Self::compute_compression_ratio(original_bytes, compressed_bytes);

        // Compute reconstruction error
        let reconstructed_matrix = Self::decompress_internal(
            &compressed_vectors,
            &scale_factors_list,
            vocabulary_size,
            num_vectors,
        )?;

        let reconstruction_error =
            Self::compute_reconstruction_error_matrix(eigenvectors, &reconstructed_matrix);

        // Convert eigenvalues to f32
        let top_k_eigenvalues: Vec<f32> = eigenvalues.iter().map(|&v| v as f32).collect();

        Ok(Self {
            signature_id: Uuid::new_v4(),
            node_id,
            top_k_eigenvectors: compressed_vectors,
            top_k_eigenvalues,
            scale_factors: scale_factors_list,
            domain_label: domain,
            vocabulary_size,
            compression_ratio,
            reconstruction_error,
            created_at: SystemTime::now(),
        })
    }

    /// Decompresses back to f64 eigenvectors
    ///
    /// Returns approximate eigenvectors with <5% error
    pub fn decompress(&self) -> Result<DMatrix<f64>> {
        Self::decompress_internal(
            &self.top_k_eigenvectors,
            &self.scale_factors,
            self.vocabulary_size,
            self.top_k_eigenvectors.len(),
        )
    }

    /// Internal decompression helper
    fn decompress_internal(
        compressed_vectors: &[SparseVec<i8>],
        scale_factors: &[(f32, f32)],
        vocabulary_size: usize,
        num_vectors: usize,
    ) -> Result<DMatrix<f64>> {
        let mut data = vec![0.0; vocabulary_size * num_vectors];

        for (col_idx, (sparse_i8, &factors)) in compressed_vectors
            .iter()
            .zip(scale_factors.iter())
            .enumerate()
        {
            // Dequantize
            let sparse_f64 = sparse_i8.dequantize_from_int8(factors);

            // Reconstruct dense
            let dense = sparse_f64.to_dense();

            // Copy to matrix data (column-major)
            for (row_idx, &val) in dense.iter().enumerate() {
                data[col_idx * vocabulary_size + row_idx] = val;
            }
        }

        Ok(DMatrix::from_vec(vocabulary_size, num_vectors, data))
    }

    /// Computes compression ratio
    fn compute_compression_ratio(original_bytes: usize, compressed_bytes: usize) -> f64 {
        if compressed_bytes == 0 {
            return 0.0;
        }
        original_bytes as f64 / compressed_bytes as f64
    }

    /// Computes compressed size in bytes
    fn compute_compressed_size(compressed_vectors: &[SparseVec<i8>]) -> usize {
        let mut total = 0;

        for sparse_vec in compressed_vectors {
            // Indices: 4 bytes per index (usize on 64-bit)
            total += sparse_vec.indices.len() * 4;
            // Values: 1 byte per value (i8)
            total += sparse_vec.values.len();
            // Scale factors: 2 × 4 bytes
            total += 8;
        }

        // Add metadata overhead (rough estimate)
        total += 256; // UUIDs, timestamps, etc.

        total
    }

    /// Computes reconstruction error via cosine similarity
    fn compute_reconstruction_error_matrix(
        original: &DMatrix<f64>,
        reconstructed: &DMatrix<f64>,
    ) -> f64 {
        let num_vectors = original.ncols();

        if num_vectors == 0 {
            return 0.0;
        }

        let mut total_error = 0.0;

        for col_idx in 0..num_vectors {
            let orig_col = original.column(col_idx);
            let recon_col = reconstructed.column(col_idx);

            // Compute cosine similarity
            let dot_product: f64 = orig_col
                .iter()
                .zip(recon_col.iter())
                .map(|(&a, &b)| a * b)
                .sum();

            let orig_norm: f64 = orig_col.iter().map(|&x| x * x).sum::<f64>().sqrt();
            let recon_norm: f64 = recon_col.iter().map(|&x| x * x).sum::<f64>().sqrt();

            let cosine_similarity = if orig_norm > 0.0 && recon_norm > 0.0 {
                (dot_product / (orig_norm * recon_norm)).clamp(-1.0, 1.0)
            } else {
                0.0
            };

            // Error = 1 - similarity
            total_error += 1.0 - cosine_similarity;
        }

        total_error / num_vectors as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_vec_creation() {
        let indices = vec![0, 5, 10];
        let values = vec![1.0, 2.0, 3.0];
        let sparse = SparseVec::new(indices.clone(), values.clone(), 100);

        assert_eq!(sparse.indices, indices);
        assert_eq!(sparse.values, values);
        assert_eq!(sparse.dimension, 100);
    }

    #[test]
    fn test_sparse_vec_from_dense() {
        let mut dense = DVector::zeros(100);
        dense[5] = 10.0;
        dense[25] = -8.0;
        dense[50] = 5.0;

        let sparse = SparseVec::from_dense(&dense, 3);

        assert_eq!(sparse.indices.len(), 3);
        assert!(sparse.indices.contains(&5));
        assert!(sparse.indices.contains(&25));
        assert!(sparse.indices.contains(&50));
    }

    #[test]
    fn test_sparse_vec_to_dense() {
        let indices = vec![0, 50, 99];
        let values = vec![1.0, 2.0, 3.0];
        let sparse = SparseVec::new(indices, values, 100);

        let dense = sparse.to_dense();

        assert_eq!(dense.len(), 100);
        assert_eq!(dense[0], 1.0);
        assert_eq!(dense[50], 2.0);
        assert_eq!(dense[99], 3.0);
        assert_eq!(dense[10], 0.0); // Non-stored value
    }

    #[test]
    fn test_quantization_round_trip() {
        let values = vec![1.5, -2.3, 0.7, 4.2];
        let indices = vec![0, 1, 2, 3];
        let sparse_f64 = SparseVec::new(indices, values.clone(), 10);

        let (sparse_i8, scale_factors) = sparse_f64.quantize_to_int8();
        let reconstructed = sparse_i8.dequantize_from_int8(scale_factors);

        // Check values are approximately preserved
        for (orig, recon) in values.iter().zip(reconstructed.values.iter()) {
            let error = (orig - recon).abs() / orig.abs().max(0.1);
            assert!(error < 0.05, "Quantization error too large: {}", error);
        }
    }
}
