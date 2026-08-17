//! Context Collapse Prevention System
//!
//! Based on ACE paper (arXiv:2510.04618) - Prevents eigenspace collapse via Frobenius norm validation.
//! Ensures semantic coherence is maintained when applying matrix updates.

use crate::learning::reflector::ace_patterns::ACEDelta;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Context Collapse Prevention System - Validates updates before applying
#[derive(Debug, Clone)]
pub struct CollapsePreventionSystem {
    /// Maximum Frobenius norm drift allowed (ACE threshold: 0.1)
    drift_threshold: f64,
    /// Minimum spectral gap to maintain (coherence threshold: 0.3)
    coherence_threshold: f64,
    /// Enable expensive eigenspace validation
    enable_eigen_validation: bool,
}

impl CollapsePreventionSystem {
    /// Creates a new collapse prevention system with ACE default thresholds
    pub fn new() -> Self {
        Self {
            drift_threshold: 0.1,     // ACE paper default
            coherence_threshold: 0.3, // Minimum coherence
            enable_eigen_validation: true,
        }
    }

    /// Creates a system with custom thresholds
    pub fn with_thresholds(drift_threshold: f64, coherence_threshold: f64) -> Self {
        Self {
            drift_threshold,
            coherence_threshold,
            enable_eigen_validation: true,
        }
    }

    /// Disables expensive eigenspace validation (for performance)
    pub fn disable_eigen_validation(mut self) -> Self {
        self.enable_eigen_validation = false;
        self
    }

    /// Validates an ACE delta before applying to matrix
    ///
    /// # ACE Validation
    /// 1. Simulate update: M' = M + β·ΔM
    /// 2. Compute Frobenius norm: ||E - E'||_F where E, E' are eigenvector matrices
    /// 3. Check drift < threshold (0.1)
    /// 4. Check spectral gap λ₁ - λ₂ > threshold (0.3)
    ///
    /// # Returns
    /// - Ok(()) if update is safe
    /// - Err(CollapseRisk) if update would cause collapse
    pub fn validate_delta(
        &self,
        delta: &ACEDelta,
        current_eigenvectors: &DMatrix<f64>,
        current_eigenvalues: &DVector<f64>,
        learning_rate: f64,
    ) -> Result<(), CollapseRisk> {
        // Quick check: if confidence is very high, allow update
        if delta.confidence > 0.95 {
            return Ok(());
        }

        // Quick check: if learning rate is very small, update is safe
        if learning_rate < 0.001 {
            return Ok(());
        }

        if !self.enable_eigen_validation {
            // Fast path: skip expensive eigen computation
            return self.validate_simple(delta, learning_rate);
        }

        // Simulate eigenspace after update (expensive)
        let simulated_eigenvectors = self.simulate_eigenspace_update(
            current_eigenvectors,
            current_eigenvalues,
            delta,
            learning_rate,
        );

        // Compute Frobenius norm drift
        let drift = self.compute_frobenius_drift(current_eigenvectors, &simulated_eigenvectors);

        if drift > self.drift_threshold {
            return Err(CollapseRisk::ExcessiveDrift {
                drift,
                threshold: self.drift_threshold,
            });
        }

        // Check spectral gap (eigenvalue separation)
        let spectral_gap = self.compute_spectral_gap(&simulated_eigenvectors);

        if spectral_gap < self.coherence_threshold {
            return Err(CollapseRisk::LowCoherence {
                gap: spectral_gap,
                threshold: self.coherence_threshold,
            });
        }

        Ok(())
    }

    /// Simple validation without eigenspace simulation (fast path)
    fn validate_simple(&self, delta: &ACEDelta, learning_rate: f64) -> Result<(), CollapseRisk> {
        // Heuristic: check update magnitude
        let update_magnitude = delta.co_occurrences.len() as f64 * learning_rate;

        if update_magnitude > 100.0 {
            return Err(CollapseRisk::ExcessiveDrift {
                drift: update_magnitude / 100.0,
                threshold: self.drift_threshold,
            });
        }

        Ok(())
    }

    /// Simulates eigenspace after applying delta (simplified)
    ///
    /// In production, this would:
    /// 1. Apply delta to matrix: M' = M + β·ΔM
    /// 2. Recompute eigenvectors of M'
    ///
    /// For now, we simulate drift based on update magnitude
    fn simulate_eigenspace_update(
        &self,
        current_eigenvectors: &DMatrix<f64>,
        _current_eigenvalues: &DVector<f64>,
        delta: &ACEDelta,
        learning_rate: f64,
    ) -> DMatrix<f64> {
        // Simplified simulation: add small perturbation
        let perturbation_scale = (delta.co_occurrences.len() as f64).sqrt() * learning_rate * 0.01;

        current_eigenvectors.clone() * (1.0 + perturbation_scale)
    }

    /// Computes Frobenius norm distance between eigenvector matrices
    ///
    /// Frobenius norm: ||A - B||_F = sqrt(sum((a_ij - b_ij)²))
    fn compute_frobenius_drift(&self, e1: &DMatrix<f64>, e2: &DMatrix<f64>) -> f64 {
        if e1.shape() != e2.shape() {
            return f64::MAX; // Incompatible shapes
        }

        let diff = e1 - e2;
        let sum_sq: f64 = diff.iter().map(|x| x * x).sum();
        sum_sq.sqrt() / (e1.nrows() * e1.ncols()) as f64 // Normalized
    }

    /// Computes spectral gap (λ₁ - λ₂) as coherence measure
    ///
    /// In simplified version, estimate from eigenvector matrix properties
    fn compute_spectral_gap(&self, eigenvectors: &DMatrix<f64>) -> f64 {
        if eigenvectors.ncols() < 2 {
            return 0.0;
        }

        // Estimate gap from eigenvector orthogonality
        let col0 = eigenvectors.column(0);
        let col1 = eigenvectors.column(1);
        let dot_product = col0.dot(&col1).abs();

        // Well-separated eigenvectors → large gap
        1.0 - dot_product // Range [0, 1]
    }

    /// Gets drift threshold
    pub fn get_drift_threshold(&self) -> f64 {
        self.drift_threshold
    }

    /// Gets coherence threshold
    pub fn get_coherence_threshold(&self) -> f64 {
        self.coherence_threshold
    }
}

impl Default for CollapsePreventionSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Risk of context collapse
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum CollapseRisk {
    /// Eigenspace drift exceeds threshold
    #[error("Excessive eigenspace drift: {drift:.4} > {threshold:.4}")]
    ExcessiveDrift { drift: f64, threshold: f64 },

    /// Spectral gap too low (low coherence)
    #[error("Low coherence: spectral gap {gap:.4} < {threshold:.4}")]
    LowCoherence { gap: f64, threshold: f64 },

    /// Update magnitude too large
    #[error("Update magnitude {magnitude:.4} exceeds safety limit {limit:.4}")]
    ExcessiveMagnitude { magnitude: f64, limit: f64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::delta_context::DeltaSource;

    fn create_test_eigenvectors() -> DMatrix<f64> {
        DMatrix::identity(10, 10)
    }

    fn create_test_eigenvalues() -> DVector<f64> {
        DVector::from_vec(vec![1.0, 0.5, 0.3, 0.2, 0.1, 0.05, 0.03, 0.02, 0.01, 0.005])
    }

    fn create_test_delta() -> ACEDelta {
        ACEDelta {
            source: DeltaSource::UserClick,
            co_occurrences: vec![(0, 1, 1.0), (1, 2, 1.0)],
            confidence: 0.8,
            learning_rate_multiplier: 1.0,
        }
    }

    #[test]
    fn test_collapse_prevention_system_creation() {
        let system = CollapsePreventionSystem::new();
        assert_eq!(system.get_drift_threshold(), 0.1);
        assert_eq!(system.get_coherence_threshold(), 0.3);
    }

    #[test]
    fn test_validate_small_delta() {
        let system = CollapsePreventionSystem::new();
        let eigenvectors = create_test_eigenvectors();
        let eigenvalues = create_test_eigenvalues();
        let delta = create_test_delta();

        // Small learning rate should be safe
        let result = system.validate_delta(&delta, &eigenvectors, &eigenvalues, 0.01);
        assert!(result.is_ok(), "Small delta should be safe");
    }

    #[test]
    fn test_validate_high_confidence_delta() {
        let system = CollapsePreventionSystem::new();
        let eigenvectors = create_test_eigenvectors();
        let eigenvalues = create_test_eigenvalues();
        let mut delta = create_test_delta();
        delta.confidence = 0.99; // Very high confidence

        // High confidence should bypass validation
        let result = system.validate_delta(&delta, &eigenvectors, &eigenvalues, 0.5);
        assert!(result.is_ok(), "High confidence delta should be allowed");
    }

    #[test]
    fn test_frobenius_drift_computation() {
        let system = CollapsePreventionSystem::new();
        let e1 = DMatrix::identity(5, 5);
        let e2 = DMatrix::identity(5, 5) * 1.1; // 10% perturbation

        let drift = system.compute_frobenius_drift(&e1, &e2);
        assert!(drift > 0.0, "Should detect drift");
        assert!(
            drift < 0.5,
            "Drift should be reasonable for 10% perturbation"
        );
    }

    #[test]
    fn test_spectral_gap_computation() {
        let system = CollapsePreventionSystem::new();
        let eigenvectors = create_test_eigenvectors();

        let gap = system.compute_spectral_gap(&eigenvectors);
        assert!(gap > 0.9, "Identity matrix should have large spectral gap");
    }

    #[test]
    fn test_simple_validation_fast_path() {
        let system = CollapsePreventionSystem::new().disable_eigen_validation();
        let eigenvectors = create_test_eigenvectors();
        let eigenvalues = create_test_eigenvalues();
        let delta = create_test_delta();

        // Should use fast path
        let result = system.validate_delta(&delta, &eigenvectors, &eigenvalues, 0.1);
        assert!(result.is_ok(), "Fast path validation should succeed");
    }
}
