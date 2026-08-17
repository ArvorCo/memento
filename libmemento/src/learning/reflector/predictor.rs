//! Edit Quality Predictor - Lightweight quality prediction (ATLAS speculative validation)

use super::DisambiguationEdit;
use nalgebra::DVector;

/// Lightweight predictor for edit quality
#[derive(Debug, Clone)]
pub struct EditQualityPredictor {
    /// Learned weights for quality prediction
    weights: DVector<f64>,
}

impl EditQualityPredictor {
    /// Creates a new edit quality predictor with default weights
    pub fn new() -> Self {
        // Initialize with simple heuristic weights
        Self {
            weights: DVector::from_vec(vec![0.5, 0.3, 0.2]),
        }
    }

    /// Predicts the quality of an edit
    ///
    /// # Arguments
    /// * `edit` - The disambiguation edit to evaluate
    ///
    /// # Returns
    /// Quality score in [0, 1] range
    pub fn predict_quality(&self, edit: &DisambiguationEdit) -> f64 {
        // Simple heuristic features:
        // 1. Number of additional context terms (more = better, up to a point)
        // 2. Length of original term (longer = potentially more specific)
        // 3. Average length of context terms

        let num_context = edit.additional_context.len() as f64;
        let original_len = edit.original_term.len() as f64;
        let avg_context_len = if edit.additional_context.is_empty() {
            0.0
        } else {
            edit.additional_context
                .iter()
                .map(|s| s.len())
                .sum::<usize>() as f64
                / num_context
        };

        // Normalize features
        let f1 = (num_context / 5.0).min(1.0); // Cap at 5 context terms
        let f2 = (original_len / 20.0).min(1.0); // Cap at 20 chars
        let f3 = (avg_context_len / 15.0).min(1.0); // Cap at 15 chars

        // Compute weighted score
        let features = DVector::from_vec(vec![f1, f2, f3]);
        let score = features.dot(&self.weights);

        score.clamp(0.0, 1.0)
    }

    /// Updates the weights based on feedback (for future learning)
    pub fn update_weights(&mut self, _features: &DVector<f64>, _target: f64) {
        // Placeholder for future learning implementation
        // Could use gradient descent or other online learning methods
    }
}

impl Default for EditQualityPredictor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predict_quality_returns_valid_range() {
        let predictor = EditQualityPredictor::new();
        let edit = DisambiguationEdit {
            original_term: "matrix".to_string(),
            additional_context: vec!["mathematics".to_string(), "linear".to_string()],
        };

        let quality = predictor.predict_quality(&edit);
        assert!((0.0..=1.0).contains(&quality));
    }

    #[test]
    fn test_more_context_increases_quality() {
        let predictor = EditQualityPredictor::new();

        let edit1 = DisambiguationEdit {
            original_term: "matrix".to_string(),
            additional_context: vec!["math".to_string()],
        };

        let edit2 = DisambiguationEdit {
            original_term: "matrix".to_string(),
            additional_context: vec![
                "mathematics".to_string(),
                "linear".to_string(),
                "algebra".to_string(),
            ],
        };

        let quality1 = predictor.predict_quality(&edit1);
        let quality2 = predictor.predict_quality(&edit2);

        assert!(quality2 > quality1, "More context should increase quality");
    }
}
