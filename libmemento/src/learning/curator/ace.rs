use super::*;

impl ACECurator {
    /// Creates a new ACE curator with default settings.
    pub fn new() -> Self {
        Self {
            base_learning_rate: 0.01,
            confidence_multiplier: 0.5,
            distribution_shift_boost: 2.0,
            validator: EditValidator::new(0.5),
            success_tracker: UpdateSuccessTracker::new(),
            collapse_preventer: CollapsePreventionSystem::new(),
            eigenspace_stabilizer: EigenspaceStabilizer::new(),
            learning_phase: LearningPhase::Babbling,
            phase_aware_rate: PhaseAwareLearningRate::new(),
        }
    }

    /// Creates a custom ACE curator.
    pub fn with_config(
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
            collapse_preventer: CollapsePreventionSystem::new(),
            eigenspace_stabilizer: EigenspaceStabilizer::new(),
            learning_phase: LearningPhase::Babbling,
            phase_aware_rate: PhaseAwareLearningRate::new(),
        }
    }

    /// Apply delta update with ACE pattern.
    pub fn apply_delta_update(
        &mut self,
        matrix: &mut SemanticMatrix,
        delta: &ACEDelta,
        eigen: Option<&EigenDecomposition>,
    ) -> Result<UpdateMetrics, CuratorError> {
        if let Some(eigen_data) = eigen {
            self.collapse_preventer
                .validate_delta(
                    delta,
                    &eigen_data.eigenvectors,
                    &eigen_data.eigenvalues,
                    delta.learning_rate_multiplier,
                )
                .map_err(|e| CuratorError::CollapsePrevention(e.to_string()))?;
        }

        let coherence = matrix.coherence_score;
        let beta =
            self.get_adaptive_rate(coherence, delta.confidence) * delta.learning_rate_multiplier;
        let _alpha = 1.0 - beta;

        let mut applied_count = 0;
        for (token_i, token_j, weight) in &delta.co_occurrences {
            let effective_weight = weight * beta;
            match matrix.add_cooccurrence(*token_i, *token_j, effective_weight) {
                Ok(()) => applied_count += 1,
                Err(_) => continue,
            }
        }

        if applied_count == 0 {
            return Err(CuratorError::NoUpdatesApplied);
        }

        self.success_tracker.record_success();

        let (spectral_gap, eigenvector_drift) = if let Some(eigen_data) = eigen {
            let gap = if eigen_data.eigenvalues.len() >= 2 {
                eigen_data.eigenvalues[0] - eigen_data.eigenvalues[1]
            } else {
                0.0
            };

            let delta_magnitude: f64 = delta
                .co_occurrences
                .iter()
                .map(|(_, _, w)| w * w)
                .sum::<f64>()
                .sqrt();
            let drift = (delta_magnitude * beta) / eigen_data.eigenvalues[0].max(1e-10);

            (gap, drift)
        } else {
            (coherence, 0.0)
        };

        self.eigenspace_stabilizer.record_drift(eigenvector_drift);

        Ok(UpdateMetrics {
            spectral_gap,
            eigenvector_drift,
            learning_rate_used: beta,
            applied_count,
        })
    }

    /// Apply strategic guideline updates (ACE enhancement).
    pub fn apply_strategic_guidelines(
        &mut self,
        matrix: &mut SemanticMatrix,
        guidelines: &[StrategicGuideline],
    ) -> Result<Vec<UpdateMetrics>, CuratorError> {
        let mut results = Vec::with_capacity(guidelines.len());

        for guideline in guidelines {
            let delta = self.guideline_to_delta(guideline)?;
            let metrics = self.apply_delta_update(matrix, &delta, None)?;
            results.push(metrics);
        }

        Ok(results)
    }

    fn guideline_to_delta(&self, guideline: &StrategicGuideline) -> Result<ACEDelta, CuratorError> {
        match &guideline.guideline_type {
            GuidelineType::Synonym {
                source_token,
                target_token,
                strength,
            } => Ok(ACEDelta {
                source: DeltaSource::ExplicitFeedback,
                co_occurrences: vec![(*source_token, *target_token, *strength)],
                confidence: guideline.confidence,
                learning_rate_multiplier: 1.0,
            }),
            GuidelineType::CategoryMembership {
                item_token,
                category_tokens,
                strength,
            } => {
                let co_occurrences = category_tokens
                    .iter()
                    .map(|&cat| (*item_token, cat, *strength))
                    .collect();
                Ok(ACEDelta {
                    source: DeltaSource::ExplicitFeedback,
                    co_occurrences,
                    confidence: guideline.confidence,
                    learning_rate_multiplier: 1.0,
                })
            }
            GuidelineType::RelationalPattern {
                token_pairs,
                strength,
            } => {
                let co_occurrences = token_pairs
                    .iter()
                    .map(|(a, b)| (*a, *b, *strength))
                    .collect();
                Ok(ACEDelta {
                    source: DeltaSource::ExplicitFeedback,
                    co_occurrences,
                    confidence: guideline.confidence,
                    learning_rate_multiplier: 1.5,
                })
            }
        }
    }

    /// Checks if eigenspace recomputation is needed.
    pub fn needs_eigenspace_recomputation(&self) -> bool {
        self.eigenspace_stabilizer.needs_recomputation()
    }

    /// Resets eigenspace stabilizer after recomputation.
    pub fn reset_eigenspace_stabilizer(&mut self) {
        self.eigenspace_stabilizer.reset();
    }

    /// Get adaptive learning rate (ACE + ATLAS + Early Experience).
    pub fn get_adaptive_rate(&self, _coherence: f64, confidence: f64) -> f64 {
        let phase_rate = self
            .phase_aware_rate
            .get_rate_for_phase(&self.learning_phase);
        let confidence_scaled =
            phase_rate * (1.0 + (confidence - 0.5) * self.confidence_multiplier);

        confidence_scaled.clamp(self.base_learning_rate, 1.0)
    }

    /// Gets the base learning rate.
    pub fn base_learning_rate(&self) -> f64 {
        self.base_learning_rate
    }

    /// Gets the distribution shift boost factor.
    pub fn distribution_shift_boost(&self) -> f64 {
        self.distribution_shift_boost
    }

    /// Validates a delta item before application.
    pub fn validate_delta_quality(&self, item: &DeltaContextItem) -> bool {
        self.validator.validate_quality(item)
    }

    /// Gets the eigenspace stabilizer reference.
    pub fn eigenspace_stabilizer(&self) -> &EigenspaceStabilizer {
        &self.eigenspace_stabilizer
    }

    /// Update learning phase based on coherence (sync with reflector).
    pub fn update_learning_phase(&mut self, coherence: f64) {
        self.learning_phase = match coherence {
            c if c < 0.3 => LearningPhase::Babbling,
            c if c < 0.7 => LearningPhase::FirstWords,
            _ => LearningPhase::Grammar,
        };
    }

    /// Gets the current learning phase.
    pub fn learning_phase(&self) -> &LearningPhase {
        &self.learning_phase
    }

    /// Gets the success tracker.
    pub fn success_tracker(&self) -> &UpdateSuccessTracker {
        &self.success_tracker
    }

    /// Applies a DeltaContextItem (legacy interface compatibility).
    pub fn apply_delta_context_item(
        &mut self,
        matrix: &mut SemanticMatrix,
        item: &DeltaContextItem,
    ) -> Result<UpdateMetrics, CuratorError> {
        let ace_delta = self.convert_to_ace_delta(item)?;
        self.apply_delta_update(matrix, &ace_delta, None)
    }

    fn convert_to_ace_delta(&self, item: &DeltaContextItem) -> Result<ACEDelta, CuratorError> {
        let co_occurrences: Vec<CooccurrenceTriple> =
            serde_json::from_value(item.suggested_update.clone())
                .map_err(|e| CuratorError::InvalidDeltaFormat(e.to_string()))?;

        let source = match item.item_type {
            DeltaItemType::SuccessPattern => DeltaSource::UserClick,
            DeltaItemType::FailurePattern => DeltaSource::AbandonedQuery,
            DeltaItemType::KnowledgeGap => DeltaSource::ExplicitFeedback,
        };

        Ok(ACEDelta {
            source,
            co_occurrences,
            confidence: item.confidence_score,
            learning_rate_multiplier: 1.0,
        })
    }
}

impl Default for ACECurator {
    fn default() -> Self {
        Self::new()
    }
}
