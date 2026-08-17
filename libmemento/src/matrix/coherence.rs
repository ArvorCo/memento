//! Coherence measurement and drift detection.

use nalgebra::{DMatrix, DVector};
use std::time::SystemTime;

/// Computes the spectral gap (coherence score) from eigenvalues.
///
/// The spectral gap is defined as (λ₁ - λ₂)/λ₁, where λ₁ and λ₂ are
/// the largest and second-largest eigenvalues.
///
/// # Arguments
///
/// * `eigenvalues` - Vector of eigenvalues (must be sorted descending)
///
/// # Returns
///
/// Spectral gap in the range [0, 1]. Returns 0.0 if fewer than 2 eigenvalues.
pub fn compute_spectral_gap(eigenvalues: &DVector<f64>) -> f64 {
    if eigenvalues.len() < 2 {
        return 0.0;
    }

    let lambda1 = eigenvalues[0];
    let lambda2 = eigenvalues[1];

    if lambda1 > 0.0 {
        (lambda1 - lambda2) / lambda1
    } else {
        0.0
    }
}

/// Computes the Frobenius norm of the drift between two eigenvector matrices.
///
/// This measures how much the eigenvector subspace has shifted between
/// two eigendecompositions.
///
/// # Arguments
///
/// * `old_eigenvectors` - Previous eigenvector matrix (d×k)
/// * `new_eigenvectors` - Current eigenvector matrix (d×k)
///
/// # Returns
///
/// Frobenius norm of the difference: ||V_new - V_old||_F
pub fn compute_frobenius_drift(
    old_eigenvectors: &DMatrix<f64>,
    new_eigenvectors: &DMatrix<f64>,
) -> f64 {
    (new_eigenvectors - old_eigenvectors).norm()
}

/// Represents the health state of a semantic matrix based on coherence score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoherenceState {
    /// Matrix has not been evaluated yet.
    Uninitialized,
    /// Matrix has healthy coherence (> production threshold).
    Healthy,
    /// Matrix coherence is degraded but still usable (> degraded threshold).
    Degraded,
    /// Matrix coherence has fallen below acceptable levels (requires intervention).
    Quarantined,
}

/// Monitors coherence evolution over time and determines matrix health state.
///
/// The monitor tracks a history of coherence scores and classifies the matrix
/// state based on configurable thresholds.
pub struct CoherenceMonitor {
    history: Vec<(SystemTime, f64)>,
    threshold_production: f64, // 0.5 - healthy operation threshold
    threshold_degraded: f64,   // 0.3 - degraded but usable threshold
}

impl CoherenceMonitor {
    /// Creates a new coherence monitor with default thresholds.
    ///
    /// Default thresholds:
    /// - Production: 0.5 (healthy semantic structure)
    /// - Degraded: 0.3 (weak structure, requires attention)
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            threshold_production: 0.5,
            threshold_degraded: 0.3,
        }
    }

    /// Creates a new coherence monitor with custom thresholds.
    ///
    /// # Arguments
    ///
    /// * `threshold_production` - Minimum score for healthy state
    /// * `threshold_degraded` - Minimum score before quarantine
    pub fn with_thresholds(threshold_production: f64, threshold_degraded: f64) -> Self {
        Self {
            history: Vec::new(),
            threshold_production,
            threshold_degraded,
        }
    }

    /// Records a coherence score at the current time.
    ///
    /// # Arguments
    ///
    /// * `score` - Coherence score in [0, 1] range
    pub fn record_coherence(&mut self, score: f64) {
        self.history.push((SystemTime::now(), score));
    }

    /// Determines the current health state based on the most recent coherence score.
    ///
    /// # Returns
    ///
    /// The current coherence state classification.
    pub fn get_state(&self) -> CoherenceState {
        if let Some(&(_, latest_score)) = self.history.last() {
            if latest_score > self.threshold_production {
                CoherenceState::Healthy
            } else if latest_score > self.threshold_degraded {
                CoherenceState::Degraded
            } else {
                CoherenceState::Quarantined
            }
        } else {
            CoherenceState::Uninitialized
        }
    }

    /// Returns the coherence history.
    ///
    /// # Returns
    ///
    /// Slice of (timestamp, score) tuples ordered chronologically.
    pub fn history(&self) -> &[(SystemTime, f64)] {
        &self.history
    }

    /// Returns the most recent coherence score, if any.
    pub fn latest_score(&self) -> Option<f64> {
        self.history.last().map(|(_, score)| *score)
    }

    /// Compute coherence evolution metrics for monitoring and analysis.
    ///
    /// # Returns
    ///
    /// `CoherenceEvolutionMetrics` with statistical analysis, or None if insufficient data.
    pub fn evolution_metrics(&self) -> Option<CoherenceEvolutionMetrics> {
        if self.history.len() < 2 {
            return None;
        }

        let scores: Vec<f64> = self.history.iter().map(|(_, score)| *score).collect();
        let first = scores.first()?;
        let last = scores.last()?;

        // Compute statistics
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        let std_dev = variance.sqrt();

        // Compute trend (linear regression slope)
        let n = scores.len() as f64;
        let x_mean = (n - 1.0) / 2.0; // indices 0..n-1
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for (i, score) in scores.iter().enumerate() {
            let x_diff = i as f64 - x_mean;
            numerator += x_diff * (score - mean);
            denominator += x_diff * x_diff;
        }
        let trend = if denominator > 0.0 {
            numerator / denominator
        } else {
            0.0
        };

        Some(CoherenceEvolutionMetrics {
            total_measurements: self.history.len(),
            initial_score: *first,
            current_score: *last,
            delta: last - first,
            mean,
            std_dev,
            trend,
            min: scores.iter().cloned().fold(f64::INFINITY, f64::min),
            max: scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    /// Check if coherence is improving (positive trend over last N measurements)
    pub fn is_improving(&self, window_size: usize) -> bool {
        if self.history.len() < 2 {
            return false;
        }

        let start = self.history.len().saturating_sub(window_size);
        let recent: Vec<f64> = self.history[start..]
            .iter()
            .map(|(_, score)| *score)
            .collect();

        if recent.len() < 2 {
            return false;
        }

        // Simple linear trend check
        let first = recent.first().unwrap();
        let last = recent.last().unwrap();
        last > first
    }
}

/// Coherence evolution metrics for monitoring and analysis
#[derive(Debug, Clone)]
pub struct CoherenceEvolutionMetrics {
    /// Total number of coherence measurements recorded
    pub total_measurements: usize,
    /// Initial coherence score
    pub initial_score: f64,
    /// Current (most recent) coherence score
    pub current_score: f64,
    /// Change from initial to current score
    pub delta: f64,
    /// Mean coherence score across all measurements
    pub mean: f64,
    /// Standard deviation of coherence scores
    pub std_dev: f64,
    /// Linear trend (slope) of coherence evolution
    /// Positive = improving, Negative = degrading
    pub trend: f64,
    /// Minimum coherence score observed
    pub min: f64,
    /// Maximum coherence score observed
    pub max: f64,
}

impl Default for CoherenceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spectral_gap_high_coherence() {
        let eigenvalues = DVector::from_vec(vec![10.0, 1.0, 0.5, 0.1]);
        let gap = compute_spectral_gap(&eigenvalues);
        assert!((gap - 0.9).abs() < 1e-6); // (10-1)/10 = 0.9
    }

    #[test]
    fn test_spectral_gap_low_coherence() {
        let eigenvalues = DVector::from_vec(vec![10.0, 9.0, 8.0, 7.0]);
        let gap = compute_spectral_gap(&eigenvalues);
        assert!((gap - 0.1).abs() < 1e-6); // (10-9)/10 = 0.1
    }

    #[test]
    fn test_frobenius_drift_identical() {
        let v1 = DMatrix::identity(5, 3);
        let v2 = DMatrix::identity(5, 3);
        let drift = compute_frobenius_drift(&v1, &v2);
        assert!(drift < 1e-10);
    }

    #[test]
    fn test_coherence_monitor_uninitialized() {
        let monitor = CoherenceMonitor::new();
        assert_eq!(monitor.get_state(), CoherenceState::Uninitialized);
        assert!(monitor.latest_score().is_none());
    }

    #[test]
    fn test_coherence_monitor_healthy() {
        let mut monitor = CoherenceMonitor::new();
        monitor.record_coherence(0.8);
        assert_eq!(monitor.get_state(), CoherenceState::Healthy);
        assert_eq!(monitor.latest_score(), Some(0.8));
    }

    #[test]
    fn test_coherence_monitor_degraded() {
        let mut monitor = CoherenceMonitor::new();
        monitor.record_coherence(0.4); // Between 0.3 and 0.5
        assert_eq!(monitor.get_state(), CoherenceState::Degraded);
    }

    #[test]
    fn test_coherence_monitor_quarantined() {
        let mut monitor = CoherenceMonitor::new();
        monitor.record_coherence(0.2); // Below 0.3
        assert_eq!(monitor.get_state(), CoherenceState::Quarantined);
    }

    #[test]
    fn test_coherence_monitor_history() {
        let mut monitor = CoherenceMonitor::new();
        monitor.record_coherence(0.3);
        monitor.record_coherence(0.5);
        monitor.record_coherence(0.7);

        let history = monitor.history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].1, 0.3);
        assert_eq!(history[1].1, 0.5);
        assert_eq!(history[2].1, 0.7);
    }

    #[test]
    fn test_coherence_monitor_custom_thresholds() {
        let mut monitor = CoherenceMonitor::with_thresholds(0.6, 0.4);

        monitor.record_coherence(0.7);
        assert_eq!(monitor.get_state(), CoherenceState::Healthy);

        monitor.record_coherence(0.5);
        assert_eq!(monitor.get_state(), CoherenceState::Degraded);

        monitor.record_coherence(0.3);
        assert_eq!(monitor.get_state(), CoherenceState::Quarantined);
    }
}
