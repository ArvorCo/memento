//! Static Reflector - Pattern-based rules (heavyweight, stable)

use super::{CooccurrenceCandidate, DisambiguationEdit, InteractionPattern};
use crate::learning::interaction_tracker::{QueryTrajectory, SessionStatus};

/// Static reflector using predefined patterns
#[derive(Debug, Clone)]
pub struct StaticReflector {
    success_patterns: Vec<SuccessPatternRule>,
    failure_patterns: Vec<FailurePatternRule>,
    disambiguation_rules: Vec<DisambiguationRule>,
}

impl StaticReflector {
    /// Creates a new static reflector with predefined rules
    pub fn new() -> Self {
        Self {
            success_patterns: vec![
                SuccessPatternRule {
                    min_dwell_time: 30.0,
                    requires_positive_feedback: false,
                },
                SuccessPatternRule {
                    min_dwell_time: 10.0,
                    requires_positive_feedback: true,
                },
            ],
            failure_patterns: vec![
                FailurePatternRule {
                    max_clicks: 0,
                    requires_abandoned: true,
                },
                FailurePatternRule {
                    max_clicks: 3,
                    requires_abandoned: false,
                },
            ],
            disambiguation_rules: vec![DisambiguationRule {
                min_reformulation_length: 1,
                max_edit_distance: 100,
            }],
        }
    }

    /// Identifies success pattern from trajectory
    pub fn identify_success_pattern(&self, trajectory: &QueryTrajectory) -> InteractionPattern {
        let mut is_positive = false;
        let mut clicked_result_ids = Vec::new();

        // Check for clicks with significant dwell time
        for click in &trajectory.clicks {
            clicked_result_ids.push(click.result_id.clone());

            for pattern in &self.success_patterns {
                if click.dwell_time_seconds >= pattern.min_dwell_time
                    && !pattern.requires_positive_feedback
                {
                    is_positive = true;
                }
            }
        }

        // Check for explicit positive feedback
        for feedback in &trajectory.explicit_feedback {
            if feedback.is_positive {
                is_positive = true;
            }
        }

        InteractionPattern {
            is_positive,
            is_abandoned: matches!(trajectory.session_status, SessionStatus::Abandoned),
            clicked_result_ids,
        }
    }

    /// Identifies failure pattern from trajectory
    pub fn identify_failure_pattern(&self, trajectory: &QueryTrajectory) -> InteractionPattern {
        let is_abandoned = matches!(trajectory.session_status, SessionStatus::Abandoned);
        let click_count = trajectory.clicks.len();

        let mut is_positive = true; // Assume positive unless failure detected

        for pattern in &self.failure_patterns {
            if click_count <= pattern.max_clicks && pattern.requires_abandoned && is_abandoned {
                is_positive = false;
                break;
            }
        }

        InteractionPattern {
            is_positive,
            is_abandoned,
            clicked_result_ids: trajectory
                .clicks
                .iter()
                .map(|c| c.result_id.clone())
                .collect(),
        }
    }

    /// Extracts co-occurrence strengthening candidates from successful trajectory
    pub fn extract_cooccurrence_candidates(
        &self,
        trajectory: &QueryTrajectory,
    ) -> Vec<CooccurrenceCandidate> {
        let mut candidates = Vec::new();

        // Extract terms from query
        let terms: Vec<String> = trajectory
            .query_text
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();

        // Generate pairs
        for i in 0..terms.len() {
            for j in (i + 1)..terms.len() {
                // Higher confidence if positive feedback exists
                let confidence = if trajectory.explicit_feedback.iter().any(|f| f.is_positive) {
                    0.9
                } else if trajectory
                    .clicks
                    .iter()
                    .any(|c| c.dwell_time_seconds > 30.0)
                {
                    0.7
                } else {
                    0.5
                };

                candidates.push(CooccurrenceCandidate {
                    term1: terms[i].clone(),
                    term2: terms[j].clone(),
                    confidence,
                });
            }
        }

        candidates
    }

    /// Generates disambiguation edit from reformulation trajectory
    pub fn generate_disambiguation_edit(
        &self,
        trajectory: &QueryTrajectory,
    ) -> Option<DisambiguationEdit> {
        // Check if there are reformulations
        if trajectory.reformulations.is_empty() {
            return None;
        }

        let last_reformulation = &trajectory.reformulations[trajectory.reformulations.len() - 1];

        // Check against disambiguation rules
        for rule in &self.disambiguation_rules {
            if last_reformulation.edit_distance <= rule.max_edit_distance {
                // Extract additional terms from reformulation
                let original_terms: Vec<&str> = last_reformulation
                    .original_query
                    .split_whitespace()
                    .collect();
                let reformulated_terms: Vec<&str> = last_reformulation
                    .reformulated_query
                    .split_whitespace()
                    .collect();

                let mut additional_context = Vec::new();
                for term in &reformulated_terms {
                    if !original_terms.contains(term) {
                        additional_context.push(term.to_string());
                    }
                }

                if additional_context.len() >= rule.min_reformulation_length {
                    return Some(DisambiguationEdit {
                        original_term: last_reformulation.original_query.clone(),
                        additional_context,
                    });
                }
            }
        }

        None
    }

    /// Gets success patterns (for testing)
    pub fn get_success_patterns(&self) -> &[SuccessPatternRule] {
        &self.success_patterns
    }

    /// Gets failure patterns (for testing)
    pub fn get_failure_patterns(&self) -> &[FailurePatternRule] {
        &self.failure_patterns
    }
}

impl Default for StaticReflector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SuccessPatternRule {
    pub min_dwell_time: f64,
    pub requires_positive_feedback: bool,
}

#[derive(Debug, Clone)]
pub struct FailurePatternRule {
    pub max_clicks: usize,
    pub requires_abandoned: bool,
}

#[derive(Debug, Clone)]
pub struct DisambiguationRule {
    pub min_reformulation_length: usize,
    pub max_edit_distance: usize,
}
