//! Principal Angle Alignment for Distributed Node Consolidation (T070)
//!
//! Computes alignment scores between eigenvector subspaces using principal angles.
//! Uses SVD to compute the largest singular value of Q_local^T × Q_remote.
//!
//! # Complexity
//!
//! - Cross-product: O(k²d) where k=100, d=50,000
//! - SVD: O(k³) which is negligible for k=100
//! - Total: O(k²d) ≈ 500M ops ≈ 200ms on modern CPU
//!
//! # Alignment Score Interpretation
//!
//! - score > 0.9: Strong alignment → **Merge**
//! - 0.7 ≤ score ≤ 0.9: Moderate alignment → **Review**
//! - score < 0.7: Weak alignment → **Separate**

use crate::consolidation::error::{ConsolidationError, Result};
use crate::consolidation::node::ServerNode;
use nalgebra::{DMatrix, SVD};
use uuid::Uuid;

/// Merge decision based on alignment score
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeDecision {
    /// High alignment (>0.9): Safe to merge automatically
    Merge,
    /// Medium alignment (0.7-0.9): Requires human review
    Review,
    /// Low alignment (<0.7): Keep nodes separate
    Separate,
}

/// Coherence monitor for distributed node alignment
///
/// Manages subspace alignment computation and merge decision logic.
///
/// # Example
///
/// ```no_run
/// use libmemento::consolidation::alignment::CoherenceMonitor;
/// use libmemento::consolidation::node::ServerNode;
///
/// let monitor = CoherenceMonitor::new();
/// let mut node1 = ServerNode::new("medical".to_string(), 10000);
/// let mut node2 = ServerNode::new("physics".to_string(), 10000);
///
/// let decision = monitor.should_merge(&mut node1, &mut node2).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct CoherenceMonitor {
    /// High alignment threshold (default: 0.9)
    pub threshold_high: f64,
    /// Low alignment threshold (default: 0.7)
    pub threshold_low: f64,
}

impl CoherenceMonitor {
    /// Creates a new coherence monitor with default thresholds
    ///
    /// - High threshold: 0.9
    /// - Low threshold: 0.7
    pub fn new() -> Self {
        Self {
            threshold_high: 0.9,
            threshold_low: 0.7,
        }
    }

    /// Creates a coherence monitor with custom thresholds
    ///
    /// # Arguments
    ///
    /// * `threshold_high` - Minimum alignment score for automatic merge
    /// * `threshold_low` - Minimum alignment score for review (below this = separate)
    ///
    /// # Panics
    ///
    /// Panics if thresholds are not in valid range [0, 1] or if low > high
    pub fn with_thresholds(threshold_high: f64, threshold_low: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&threshold_high),
            "threshold_high must be in [0, 1]"
        );
        assert!(
            (0.0..=1.0).contains(&threshold_low),
            "threshold_low must be in [0, 1]"
        );
        assert!(
            threshold_low <= threshold_high,
            "threshold_low must be <= threshold_high"
        );

        Self {
            threshold_high,
            threshold_low,
        }
    }

    /// Computes subspace alignment via principal angles
    ///
    /// Returns the largest singular value of Q_local^T × Q_remote,
    /// which corresponds to cos(θ_min) where θ_min is the smallest principal angle.
    ///
    /// # Arguments
    ///
    /// * `local_eigenvectors` - Local eigenvector matrix (d × k)
    /// * `remote_eigenvectors` - Remote eigenvector matrix (d × k)
    ///
    /// # Returns
    ///
    /// Alignment score ∈ [0, 1]:
    /// - 1.0: Perfect alignment (identical subspaces)
    /// - 0.0: Orthogonal subspaces (no overlap)
    ///
    /// # Complexity
    ///
    /// O(k²d) for matrix multiplication + O(k³) for SVD
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::alignment::CoherenceMonitor;
    /// use nalgebra::DMatrix;
    ///
    /// let q_local = DMatrix::identity(100, 50);
    /// let q_remote = DMatrix::identity(100, 50);
    ///
    /// let score = CoherenceMonitor::compute_subspace_alignment(&q_local, &q_remote).unwrap();
    /// assert!((score - 1.0).abs() < 1e-10);
    /// ```
    pub fn compute_subspace_alignment(
        local_eigenvectors: &DMatrix<f64>,
        remote_eigenvectors: &DMatrix<f64>,
    ) -> Result<f64> {
        let (d_local, k_local) = local_eigenvectors.shape();
        let (d_remote, k_remote) = remote_eigenvectors.shape();

        // Validate dimensions
        if d_local != d_remote {
            return Err(ConsolidationError::AlignmentError(format!(
                "Dimension mismatch: local has {} dimensions, remote has {}",
                d_local, d_remote
            )));
        }

        if k_local != k_remote {
            return Err(ConsolidationError::AlignmentError(format!(
                "Eigenvector count mismatch: local has {}, remote has {}",
                k_local, k_remote
            )));
        }

        // Compute cross-product: Q_local^T × Q_remote
        // Shape: (k × d) × (d × k) = (k × k)
        // Complexity: O(k² × d)
        let cross_product = local_eigenvectors.transpose() * remote_eigenvectors;

        // Compute SVD to get singular values
        // Complexity: O(k³)
        let svd = SVD::new(cross_product, false, false);

        // Get largest singular value (corresponds to smallest principal angle)
        // σ_max = cos(θ_min)
        let largest_singular_value = svd
            .singular_values
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .copied()
            .unwrap_or(0.0);

        // Clamp to [0, 1] to handle numerical errors
        let alignment_score = largest_singular_value.clamp(0.0, 1.0);

        Ok(alignment_score)
    }

    /// Decides whether to merge two nodes based on their alignment score
    ///
    /// # Arguments
    ///
    /// * `local` - Local server node (mutable to compute eigendecomposition)
    /// * `remote` - Remote server node (mutable to compute eigendecomposition)
    ///
    /// # Returns
    ///
    /// - `MergeDecision::Merge`: Alignment > threshold_high
    /// - `MergeDecision::Review`: threshold_low ≤ alignment ≤ threshold_high
    /// - `MergeDecision::Separate`: alignment < threshold_low
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::alignment::CoherenceMonitor;
    /// use libmemento::consolidation::node::ServerNode;
    ///
    /// let monitor = CoherenceMonitor::new();
    /// let mut node1 = ServerNode::new("medical".to_string(), 1000);
    /// let mut node2 = ServerNode::new("legal".to_string(), 1000);
    ///
    /// let decision = monitor.should_merge(&mut node1, &mut node2).unwrap();
    /// println!("Merge decision: {:?}", decision);
    /// ```
    pub fn should_merge(
        &self,
        local: &mut ServerNode,
        remote: &mut ServerNode,
    ) -> Result<MergeDecision> {
        // Compute eigendecompositions for both nodes
        let k = 100; // Top 100 eigenvectors
        let local_eigen = local.matrix.compute_eigendecomposition(k).map_err(|e| {
            ConsolidationError::AlignmentError(format!(
                "Failed to compute local eigendecomposition: {}",
                e
            ))
        })?;
        let remote_eigen = remote.matrix.compute_eigendecomposition(k).map_err(|e| {
            ConsolidationError::AlignmentError(format!(
                "Failed to compute remote eigendecomposition: {}",
                e
            ))
        })?;

        // Extract eigenvectors
        let local_eigenvectors = &local_eigen.eigenvectors;
        let remote_eigenvectors = &remote_eigen.eigenvectors;

        // Compute alignment score
        let score = Self::compute_subspace_alignment(local_eigenvectors, remote_eigenvectors)?;

        // Apply decision logic
        let decision = if score >= self.threshold_high {
            MergeDecision::Merge
        } else if score >= self.threshold_low {
            MergeDecision::Review
        } else {
            MergeDecision::Separate
        };

        Ok(decision)
    }

    /// Finds all coherent peers for a target node
    ///
    /// Returns a list of (peer_id, alignment_score) pairs for all peers
    /// with alignment score ≥ threshold_low.
    ///
    /// # Arguments
    ///
    /// * `target` - Target node to find peers for (mutable to compute eigendecomposition)
    /// * `peers` - List of candidate peer nodes (mutable to compute eigendecompositions)
    ///
    /// # Returns
    ///
    /// Sorted list of (UUID, alignment_score) tuples, ordered by score (descending)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::alignment::CoherenceMonitor;
    /// use libmemento::consolidation::node::ServerNode;
    ///
    /// let monitor = CoherenceMonitor::new();
    /// let mut target = ServerNode::new("medical".to_string(), 1000);
    /// let mut peers = vec![
    ///     ServerNode::new("legal".to_string(), 1000),
    ///     ServerNode::new("physics".to_string(), 1000),
    /// ];
    ///
    /// let coherent_peers = monitor.find_coherent_peers(&mut target, &mut peers);
    /// println!("Found {} coherent peers", coherent_peers.len());
    /// ```
    pub fn find_coherent_peers(
        &self,
        target: &mut ServerNode,
        peers: &mut [ServerNode],
    ) -> Vec<(Uuid, f64)> {
        let mut coherent_peers = Vec::new();

        // Compute target eigendecomposition once
        let k = 100;
        let target_eigen = match target.matrix.compute_eigendecomposition(k) {
            Ok(eigen) => eigen,
            Err(_) => return coherent_peers, // Return empty if target eigendecomposition fails
        };

        for peer in peers.iter_mut() {
            // Skip self
            if peer.node_id == target.node_id {
                continue;
            }

            // Compute peer eigendecomposition
            let peer_eigen = match peer.matrix.compute_eigendecomposition(k) {
                Ok(eigen) => eigen,
                Err(_) => continue, // Skip peers where eigendecomposition fails
            };

            // Compute alignment
            if let Ok(score) = Self::compute_subspace_alignment(
                &target_eigen.eigenvectors,
                &peer_eigen.eigenvectors,
            ) {
                // Only include peers above low threshold
                if score >= self.threshold_low {
                    coherent_peers.push((peer.node_id, score));
                }
            }
        }

        // Sort by score (descending)
        coherent_peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        coherent_peers
    }
}

impl Default for CoherenceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function: Compute alignment score between two eigenvector matrices
///
/// This is a convenience function that wraps CoherenceMonitor::compute_subspace_alignment.
///
/// # Arguments
///
/// * `q1` - First eigenvector matrix (d × k)
/// * `q2` - Second eigenvector matrix (d × k)
///
/// # Returns
///
/// Alignment score ∈ [0, 1]
///
/// # Panics
///
/// Panics if matrices have incompatible dimensions.
pub fn compute_alignment_score(q1: &DMatrix<f64>, q2: &DMatrix<f64>) -> f64 {
    CoherenceMonitor::compute_subspace_alignment(q1, q2).expect("Failed to compute alignment score")
}

/// Helper function: Decide merge based on score
///
/// Uses default thresholds (0.9, 0.7).
///
/// # Arguments
///
/// * `score` - Alignment score ∈ [0, 1]
///
/// # Returns
///
/// Merge decision based on default thresholds
pub fn decide_merge(score: f64) -> MergeDecision {
    let monitor = CoherenceMonitor::new();
    if score >= monitor.threshold_high {
        MergeDecision::Merge
    } else if score >= monitor.threshold_low {
        MergeDecision::Review
    } else {
        MergeDecision::Separate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coherence_monitor_default_thresholds() {
        let monitor = CoherenceMonitor::new();
        assert_eq!(monitor.threshold_high, 0.9);
        assert_eq!(monitor.threshold_low, 0.7);
    }

    #[test]
    fn test_coherence_monitor_custom_thresholds() {
        let monitor = CoherenceMonitor::with_thresholds(0.95, 0.75);
        assert_eq!(monitor.threshold_high, 0.95);
        assert_eq!(monitor.threshold_low, 0.75);
    }

    #[test]
    #[should_panic(expected = "threshold_low must be <= threshold_high")]
    fn test_coherence_monitor_invalid_thresholds() {
        CoherenceMonitor::with_thresholds(0.7, 0.9);
    }

    #[test]
    fn test_identical_matrices_perfect_alignment() {
        let q = DMatrix::identity(10, 5);
        let score = compute_alignment_score(&q, &q);
        assert!(
            (score - 1.0).abs() < 1e-10,
            "Identical matrices should have score 1.0"
        );
    }

    #[test]
    fn test_decide_merge_thresholds() {
        assert_eq!(decide_merge(0.95), MergeDecision::Merge);
        assert_eq!(decide_merge(0.9), MergeDecision::Merge);
        assert_eq!(decide_merge(0.85), MergeDecision::Review);
        assert_eq!(decide_merge(0.7), MergeDecision::Review);
        assert_eq!(decide_merge(0.65), MergeDecision::Separate);
        assert_eq!(decide_merge(0.0), MergeDecision::Separate);
    }
}
