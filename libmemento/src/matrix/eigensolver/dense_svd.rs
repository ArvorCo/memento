//! Dense SVD eigensolver (fallback for dense matrices).
//!
//! This is the current implementation, refactored as an EigenSolver.
//! Used as fallback for dense matrices, small matrices, or debugging.
//!
//! # Performance
//!
//! For 10K vocabulary, dense matrix:
//! - Memory: O(n²) ≈ 800MB
//! - Latency: O(n²k) ≈ 2000ms
//!
//! # When to Use
//!
//! - Dense matrices (density > 5%)
//! - Small matrices (n < 1000)
//! - Debugging/validation (compare against Lanczos)

use super::EigenSolver;
use crate::matrix::eigen::EigenDecomposition;
use crate::matrix::error::{Result, SemanticMatrixError};
use nalgebra::{DMatrix, DVector};
use sprs::CsMat;

/// Dense SVD-based eigensolver.
pub struct DenseSvdEigenSolver {
    /// Maximum iterations for SVD
    max_iterations: usize,

    /// Convergence tolerance
    tolerance: f64,
}

impl DenseSvdEigenSolver {
    /// Create a new Dense SVD eigensolver with default parameters.
    pub fn new() -> Self {
        Self {
            max_iterations: 1000,
            tolerance: 1e-6,
        }
    }
}

impl EigenSolver for DenseSvdEigenSolver {
    fn decompose(&self, matrix: &CsMat<f64>, k: usize) -> Result<EigenDecomposition> {
        // Convert sparse to dense (EXISTING CODE from sparse_matrix.rs)
        let sprs_dense = matrix.to_dense();
        let shape = sprs_dense.shape();
        let nrows = shape[0];
        let ncols = shape[1];

        let mut dense_data = Vec::with_capacity(nrows * ncols);
        for j in 0..ncols {
            for i in 0..nrows {
                dense_data.push(sprs_dense[[i, j]]);
            }
        }
        let dense_matrix = DMatrix::from_vec(nrows, ncols, dense_data);

        // Compute SVD
        let max_niter = self.max_iterations.min(10 * nrows.min(ncols));
        let svd =
            nalgebra::linalg::SVD::try_new(dense_matrix, true, true, self.tolerance, max_niter)
                .ok_or_else(|| {
                    SemanticMatrixError::EigenDecompositionFailed(format!(
                        "SVD computation failed after {} iterations",
                        max_niter
                    ))
                })?;

        let u = svd.u.ok_or_else(|| {
            SemanticMatrixError::EigenDecompositionFailed(
                "SVD did not return left singular vectors".to_string(),
            )
        })?;
        let singular_values = &svd.singular_values;

        // Take only top-k components
        let k_actual = k.min(singular_values.len());
        let eigenvectors = u.columns(0, k_actual).clone_owned();

        // For symmetric matrices, eigenvalues = singular values
        // (not squared, because M is already PSD and we're finding eigenvalues of M)
        let eigenvalues =
            DVector::from_iterator(k_actual, (0..k_actual).map(|i| singular_values[i]));

        EigenDecomposition::new(eigenvectors, eigenvalues)
    }

    fn name(&self) -> &str {
        "DenseSVD"
    }

    fn is_suitable(&self, nnz: usize, vocab_size: usize) -> bool {
        let density = (nnz as f64) / ((vocab_size * vocab_size) as f64);
        density >= 0.05 || vocab_size < 1000
    }
}

impl Default for DenseSvdEigenSolver {
    fn default() -> Self {
        Self::new()
    }
}
