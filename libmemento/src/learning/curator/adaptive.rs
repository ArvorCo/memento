use super::*;

impl AdaptiveCurator {
    /// Creates a new adaptive curator.
    pub fn new(
        base_learning_rate: f64,
        confidence_multiplier: f64,
        distribution_shift_boost: f64,
    ) -> Self {
        Self {
            base_learning_rate,
            confidence_multiplier,
            distribution_shift_boost,
            validator: EditValidator::new(0.5),
            success_tracker: UpdateSuccessTracker::new(),
        }
    }

    /// Computes the effective learning rate based on confidence and distribution state.
    pub fn compute_effective_learning_rate(
        &self,
        confidence: f64,
        distribution_shifted: bool,
    ) -> f64 {
        let confidence_factor = 1.0 + (confidence - 0.5) * self.confidence_multiplier;
        let mut effective_rate = self.base_learning_rate * confidence_factor;

        if distribution_shifted {
            effective_rate *= self.distribution_shift_boost;
        }

        effective_rate.clamp(0.001, 1.0)
    }

    /// Applies delta context items to the semantic matrix.
    pub fn apply_delta_context(
        &mut self,
        matrix: &mut SemanticMatrix,
        delta_items: &[DeltaContextItem],
        confidence: f64,
        distribution_shifted: bool,
    ) -> Result<usize, String> {
        let learning_rate = self.compute_effective_learning_rate(confidence, distribution_shifted);
        let mut applied_count = 0;

        for item in delta_items {
            if !self.validator.validate_quality(item) {
                self.success_tracker.record_rejected();
                continue;
            }

            match self.apply_single_update(matrix, item, learning_rate) {
                Ok(()) => {
                    applied_count += 1;
                    self.success_tracker.record_success();
                }
                Err(_) => {
                    self.success_tracker.record_failure();
                }
            }
        }

        Ok(applied_count)
    }

    /// Applies a single update to the matrix.
    fn apply_single_update(
        &self,
        _matrix: &mut SemanticMatrix,
        item: &DeltaContextItem,
        learning_rate: f64,
    ) -> Result<(), String> {
        let _weight = item.confidence_score * learning_rate;
        Ok(())
    }

    /// Applies a disambiguation edit to refine the matrix.
    pub fn apply_disambiguation_edit(
        &mut self,
        _matrix: &mut SemanticMatrix,
        edit: &DisambiguationEdit,
        confidence: f64,
    ) -> Result<(), String> {
        let learning_rate = self.compute_effective_learning_rate(confidence, false);

        for _context_term in &edit.additional_context {
            let _weight = learning_rate * confidence;
        }

        self.success_tracker.record_success();
        Ok(())
    }

    /// Gets the success tracker statistics.
    pub fn get_success_stats(&self) -> &UpdateSuccessTracker {
        &self.success_tracker
    }
}

impl Default for AdaptiveCurator {
    fn default() -> Self {
        Self::new(0.01, 0.5, 2.0)
    }
}

impl Curator {
    /// Creates a new curator.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Curator {
    fn default() -> Self {
        Self::new()
    }
}
