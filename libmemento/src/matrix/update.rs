//! Incremental eigenvalue updates using fROIPCA (Fast Rank-One Incremental PCA).

use crate::matrix::error::Result;
use nalgebra::{DMatrix, DVector};

/// SIMD-optimized sparse dot product.
///
/// Computes dot product between a dense vector and a sparse vector
/// represented as (index, value) pairs. Compiler auto-vectorizes
/// this implementation with -C target-cpu=native.
///
/// # Arguments
///
/// * `dense_vec` - Dense vector slice
/// * `sparse_vec` - Sparse vector as (index, value) pairs
///
/// # Returns
///
/// Dot product: Σᵢ dense[sparse[i].idx] * sparse[i].val
#[inline(always)]
fn sparse_dot_product(dense_vec: &[f64], sparse_vec: &[(usize, f64)]) -> f64 {
    sparse_vec
        .iter()
        .map(|&(idx, val)| {
            // Bounds check eliminated by optimizer when idx guaranteed valid
            unsafe { *dense_vec.get_unchecked(idx) * val }
        })
        .sum()
}

/// Incremental eigenvalue decomposition with fROIPCA algorithm.
#[derive(Debug, Clone)]
pub struct IncrementalEigen {
    /// Current eigenvectors (d×k matrix)
    eigenvectors: DMatrix<f64>,

    /// Current eigenvalues (k-vector)
    eigenvalues: DVector<f64>,

    /// Number of updates processed
    n_updates: usize,

    /// Accumulated drift measure
    drift_accumulator: f64,
}

impl IncrementalEigen {
    /// Creates a new incremental eigenvalue decomposition.
    ///
    /// # Arguments
    ///
    /// * `eigenvectors` - Initial eigenvector matrix (d×k)
    /// * `eigenvalues` - Initial eigenvalue vector (k-vector)
    pub fn new(eigenvectors: DMatrix<f64>, eigenvalues: DVector<f64>) -> Self {
        Self {
            eigenvectors,
            eigenvalues,
            n_updates: 0,
            drift_accumulator: 0.0,
        }
    }

    /// Updates the eigendecomposition with a new sparse vector.
    ///
    /// Uses the fROIPCA algorithm:
    /// - λᵢ(t+1) = λᵢ(t) + η(vᵢᵀx)²
    /// - vᵢ(t+1) = vᵢ(t) + η(vᵢᵀx)(x - λᵢ(t)vᵢ(t))
    ///
    /// # Arguments
    ///
    /// * `x_sparse` - Sparse vector as (index, value) pairs
    ///
    /// # Returns
    ///
    /// Returns an error if the update fails.
    pub fn update(&mut self, x_sparse: &[(usize, f64)]) -> Result<()> {
        self.n_updates += 1;
        let eta = 1.0 / (self.n_updates as f64);

        let vocabulary_size = self.eigenvectors.nrows();

        // Optimization: Use sparse dot product directly instead of densifying x
        // This is much faster when x_sparse has few non-zeros (typical case)
        if x_sparse.len() < vocabulary_size / 10 {
            // Sparse path: compute dot products directly from sparse representation
            for i in 0..self.eigenvalues.len() {
                // First pass: compute dot product and cache eigenvector values
                let vi_data: Vec<f64> = (0..vocabulary_size)
                    .map(|j| self.eigenvectors[(j, i)])
                    .collect();

                let vi_dot_x = sparse_dot_product(&vi_data, x_sparse);

                // Eigenvalue update: λᵢ += η(vᵢᵀx)²
                self.eigenvalues[i] += eta * vi_dot_x * vi_dot_x;

                // Eigenvector update: vᵢ += η(vᵢᵀx)(x - λᵢvᵢ)
                let lambda_i_old = self.eigenvalues[i] - eta * vi_dot_x * vi_dot_x;
                let eta_vi_dot_x = eta * vi_dot_x;

                // Second pass: update eigenvector elements
                let mut drift_sq = 0.0;
                for (j, &vi_j) in vi_data.iter().enumerate() {
                    let x_j = x_sparse
                        .iter()
                        .find(|&&(idx, _)| idx == j)
                        .map(|&(_, val)| val)
                        .unwrap_or(0.0);

                    let update = eta_vi_dot_x * (x_j - lambda_i_old * vi_j);
                    self.eigenvectors[(j, i)] += update;
                    drift_sq += update * update;
                }

                self.drift_accumulator += drift_sq.sqrt();
            }
        } else {
            // Dense path: for very dense sparse vectors, fallback to original algorithm
            let mut x_dense = DVector::zeros(vocabulary_size);
            for &(idx, val) in x_sparse {
                if idx < vocabulary_size {
                    x_dense[idx] = val;
                }
            }

            for i in 0..self.eigenvalues.len() {
                let vi = self.eigenvectors.column(i);
                let vi_dot_x = vi.dot(&x_dense);

                self.eigenvalues[i] += eta * vi_dot_x * vi_dot_x;

                let lambda_i_old = self.eigenvalues[i] - eta * vi_dot_x * vi_dot_x;
                let update_term = eta * vi_dot_x * (&x_dense - lambda_i_old * vi);

                for j in 0..vocabulary_size {
                    self.eigenvectors[(j, i)] += update_term[j];
                }

                self.drift_accumulator += update_term.norm();
            }
        }

        Ok(())
    }

    /// Returns the current eigenvectors.
    pub fn eigenvectors(&self) -> &DMatrix<f64> {
        &self.eigenvectors
    }

    /// Returns the current eigenvalues.
    pub fn eigenvalues(&self) -> &DVector<f64> {
        &self.eigenvalues
    }

    /// Returns the accumulated drift measure.
    pub fn drift(&self) -> f64 {
        self.drift_accumulator / (self.n_updates as f64).max(1.0)
    }

    /// Returns the number of updates processed.
    pub fn num_updates(&self) -> usize {
        self.n_updates
    }

    /// Checks if recomputation is needed based on drift threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Maximum allowed drift (default: 0.1)
    ///
    /// # Returns
    ///
    /// True if full eigendecomposition should be recomputed.
    pub fn should_recompute(&self, threshold: f64) -> bool {
        self.drift() > threshold
    }

    /// Renormalizes eigenvectors to maintain orthonormality.
    ///
    /// Applies Gram-Schmidt orthonormalization to correct drift accumulation.
    pub fn renormalize(&mut self) {
        let k = self.eigenvalues.len();

        for i in 0..k {
            // Orthogonalize against previous vectors
            for j in 0..i {
                let proj = self
                    .eigenvectors
                    .column(i)
                    .dot(&self.eigenvectors.column(j));
                for row in 0..self.eigenvectors.nrows() {
                    self.eigenvectors[(row, i)] -= proj * self.eigenvectors[(row, j)];
                }
            }

            // Normalize
            let norm = self.eigenvectors.column(i).norm();
            if norm > 1e-10 {
                for row in 0..self.eigenvectors.nrows() {
                    self.eigenvectors[(row, i)] /= norm;
                }
            }
        }

        // Reset drift accumulator after renormalization
        self.drift_accumulator = 0.0;
    }

    /// Converts to EigenDecomposition for compatibility.
    pub fn to_eigen_decomposition(&self) -> crate::matrix::eigen::EigenDecomposition {
        crate::matrix::eigen::EigenDecomposition::new(
            self.eigenvectors.clone(),
            self.eigenvalues.clone(),
        )
        .expect("Incremental eigen should always be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_incremental_update_basic() {
        let eigenvectors = DMatrix::identity(5, 2);
        let eigenvalues = DVector::from_vec(vec![2.0, 1.0]);
        let mut inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues);

        // Update with a sparse vector
        let x_sparse = vec![(0, 1.0), (1, 0.5)];
        inc_eigen.update(&x_sparse).unwrap();

        assert_eq!(inc_eigen.num_updates(), 1);
        assert!(inc_eigen.drift() >= 0.0);
    }

    #[test]
    fn test_incremental_preserves_eigenvalue_order() {
        let eigenvectors = DMatrix::identity(10, 3);
        let eigenvalues = DVector::from_vec(vec![10.0, 5.0, 1.0]);
        let mut inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues.clone());

        // Apply multiple updates
        for i in 0..100 {
            let x_sparse = vec![(i % 10, 0.1)];
            inc_eigen.update(&x_sparse).unwrap();
        }

        // Eigenvalues should remain in descending order (approximately)
        let eigenvals = inc_eigen.eigenvalues();
        for i in 0..(eigenvals.len() - 1) {
            assert!(
                eigenvals[i] >= eigenvals[i + 1] * 0.9, // Allow 10% tolerance
                "Eigenvalues not ordered: {}[{}] = {} < {}[{}] = {}",
                i,
                eigenvals[i],
                i + 1,
                eigenvals[i + 1],
                i,
                i + 1
            );
        }
    }

    #[test]
    fn test_renormalization_restores_orthonormality() {
        let eigenvectors = DMatrix::identity(20, 5);
        let eigenvalues = DVector::from_vec(vec![10.0, 8.0, 6.0, 4.0, 2.0]);
        let mut inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues);

        // Apply many updates to accumulate drift
        for i in 0..1000 {
            let x_sparse = vec![(i % 20, 0.01)];
            inc_eigen.update(&x_sparse).unwrap();
        }

        // Renormalize
        inc_eigen.renormalize();

        // Check orthonormality: VᵀV = I
        let eigenvecs = inc_eigen.eigenvectors();
        let vtv = eigenvecs.transpose() * eigenvecs;

        for i in 0..5 {
            for j in 0..5 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert_relative_eq!(vtv[(i, j)], expected, epsilon = 1e-3);
            }
        }
    }

    #[test]
    fn test_should_recompute_triggers_correctly() {
        let eigenvectors = DMatrix::identity(10, 2);
        let eigenvalues = DVector::from_vec(vec![5.0, 2.0]);
        let mut inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues);

        // Initially should not need recomputation
        assert!(!inc_eigen.should_recompute(0.1));

        // Apply updates until drift exceeds threshold
        for _ in 0..100 {
            let x_sparse = vec![(0, 1.0), (1, 0.5)];
            inc_eigen.update(&x_sparse).unwrap();
        }

        // May or may not trigger depending on drift accumulation
        let drift = inc_eigen.drift();
        assert_eq!(inc_eigen.should_recompute(0.1), drift > 0.1);
    }

    #[test]
    fn test_conversion_to_eigen_decomposition() {
        let eigenvectors = DMatrix::identity(5, 3);
        let eigenvalues = DVector::from_vec(vec![10.0, 5.0, 1.0]);
        let inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues.clone());

        let eigen_decomp = inc_eigen.to_eigen_decomposition();

        assert_eq!(eigen_decomp.num_components(), 3);
        assert_eq!(eigen_decomp.eigenvalues[0], 10.0);
        assert_eq!(eigen_decomp.eigenvalues[1], 5.0);
        assert_eq!(eigen_decomp.eigenvalues[2], 1.0);
    }

    #[test]
    fn test_sparse_vector_update_efficiency() {
        let eigenvectors = DMatrix::identity(1000, 10);
        let eigenvalues = DVector::from_vec((0..10).map(|i| (10 - i) as f64).collect());
        let mut inc_eigen = IncrementalEigen::new(eigenvectors, eigenvalues);

        // Sparse update with only a few non-zeros
        let x_sparse = vec![(0, 1.0), (500, 0.5), (999, 0.3)];

        let start = std::time::Instant::now();
        inc_eigen.update(&x_sparse).unwrap();
        let duration = start.elapsed();

        // Should be fast (< 500ms for debug builds)
        assert!(duration.as_millis() < 500, "Update took {:?}", duration);
    }
}
