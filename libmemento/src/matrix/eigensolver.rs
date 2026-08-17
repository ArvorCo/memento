//! Eigensolver trait and implementations for semantic matrices.
//!
//! Provides multiple eigendecomposition strategies:
//! - Lanczos: Fast sparse eigendecomposition (optimal for our use case)
//! - Dense SVD: Fallback for dense matrices or debugging
//!
//! The library automatically selects the optimal algorithm based on matrix properties.

use crate::matrix::eigen::EigenDecomposition;
use crate::matrix::error::Result;
use sprs::CsMat;

pub mod dense_svd;
pub mod lanczos;

pub use dense_svd::DenseSvdEigenSolver;
pub use lanczos::LanczosEigenSolver;

/// Eigensolver trait for computing top-k eigendecomposition.
pub trait EigenSolver {
    /// Compute top-k eigendecomposition of a sparse symmetric matrix.
    ///
    /// # Arguments
    ///
    /// * `matrix` - Sparse CSR matrix (symmetric, PSD)
    /// * `k` - Number of top eigenvectors to compute
    ///
    /// # Returns
    ///
    /// EigenDecomposition with top-k eigenvectors and eigenvalues
    fn decompose(&self, matrix: &CsMat<f64>, k: usize) -> Result<EigenDecomposition>;

    /// Name of the eigensolver algorithm.
    fn name(&self) -> &str;

    /// Check if this solver is suitable for the given matrix.
    ///
    /// # Arguments
    ///
    /// * `nnz` - Number of non-zeros
    /// * `vocab_size` - Matrix dimension
    ///
    /// # Returns
    ///
    /// True if this solver is a good fit
    fn is_suitable(&self, nnz: usize, vocab_size: usize) -> bool;
}

/// Auto-select the best eigensolver based on matrix properties.
///
/// # Selection Strategy
///
/// - Sparse (density < 5%, n ≥ 1000): **Lanczos** (optimal)
/// - Dense or small (n < 1000): **Dense SVD** (fallback)
///
/// # Performance
///
/// For 10K vocabulary, 2K non-zeros:
/// - Lanczos: ~40ms
/// - Dense SVD: ~2000ms
///
/// # Example
///
/// ```no_run
/// use libmemento::matrix::eigensolver::select_eigensolver;
///
/// let solver = select_eigensolver(2000, 10000);
/// println!("Selected: {}", solver.name());
/// ```
pub fn select_eigensolver(nnz: usize, vocab_size: usize) -> Box<dyn EigenSolver> {
    let density = (nnz as f64) / ((vocab_size * vocab_size) as f64);

    // Sparse matrix with reasonable size: Use Lanczos
    if density < 0.05 && vocab_size >= 1000 {
        Box::new(LanczosEigenSolver::new())
    } else {
        // Dense or small matrix: Use dense SVD
        Box::new(DenseSvdEigenSolver::new())
    }
}
