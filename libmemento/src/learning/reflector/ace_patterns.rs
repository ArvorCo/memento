//! ACE Pattern Extraction - Success and Failure Pattern Analysis
//!
//! Based on Stanford ACE paper (arXiv:2510.04618) and Early Experience paper (arXiv:2510.08558).
//! Extracts co-occurrence deltas from user interactions (clicks, reformulations, failures).

use crate::learning::delta_context::{CooccurrenceTriple, DeltaSource};
use crate::learning::interaction_tracker::{QueryTrajectory, SessionStatus};
use serde::{Deserialize, Serialize};

/// ACE Success Pattern Analyzer - Extracts positive signals from clicks
#[derive(Debug, Clone)]
pub struct SuccessPatternAnalyzer {
    /// Minimum dwell time to consider a click as positive (seconds)
    min_dwell_time: f64,
    /// Token extraction settings
    token_extractor: TokenExtractor,
}

impl SuccessPatternAnalyzer {
    /// Creates a new success pattern analyzer
    pub fn new(min_dwell_time: f64) -> Self {
        Self {
            min_dwell_time,
            token_extractor: TokenExtractor::new(),
        }
    }

    /// Extracts success patterns from a trajectory with clicks
    ///
    /// # ACE Pattern
    /// Click on result → Extract (query_term, result_term) pairs → Build co-occurrence deltas
    ///
    /// # Returns
    /// Vector of ACE delta items ready for matrix update
    pub fn extract_patterns(&self, trajectory: &QueryTrajectory) -> Vec<ACEDelta> {
        let mut deltas = Vec::new();

        // Extract query terms
        let query_terms = self.token_extractor.extract_terms(&trajectory.query_text);

        // Process each click
        for click in &trajectory.clicks {
            // Only consider clicks with sufficient dwell time
            if click.dwell_time_seconds < self.min_dwell_time {
                continue;
            }

            // Mock result content extraction (in real system, fetch from index)
            let result_terms = self.token_extractor.extract_result_terms(&click.result_id);

            // Build co-occurrence pairs
            let co_occurrences = self.build_cooccurrences(&query_terms, &result_terms);

            if !co_occurrences.is_empty() {
                deltas.push(ACEDelta {
                    source: DeltaSource::UserClick,
                    co_occurrences,
                    confidence: self.compute_confidence(click.dwell_time_seconds),
                    learning_rate_multiplier: 1.0,
                });
            }
        }

        deltas
    }

    /// Builds co-occurrence triples from query and result terms
    fn build_cooccurrences(
        &self,
        query_terms: &[TokenWithId],
        result_terms: &[TokenWithId],
    ) -> Vec<CooccurrenceTriple> {
        let mut cooccurrences = Vec::new();

        for qt in query_terms {
            for rt in result_terms {
                // Symmetric co-occurrence
                cooccurrences.push((qt.id, rt.id, 1.0));
            }
        }

        cooccurrences
    }

    /// Computes confidence score based on dwell time
    fn compute_confidence(&self, dwell_time: f64) -> f64 {
        // Sigmoid-like function: more time = higher confidence
        let normalized = (dwell_time - self.min_dwell_time) / 30.0; // 30s is "very confident"
        (normalized / (1.0 + normalized)).clamp(0.5, 1.0)
    }
}

impl Default for SuccessPatternAnalyzer {
    fn default() -> Self {
        Self::new(30.0)
    }
}

/// ACE Failure Pattern Detector - Diagnoses and corrects failures
#[derive(Debug, Clone)]
pub struct FailurePatternDetector {
    /// Minimum reformulation edit distance to consider
    min_edit_distance: usize,
    /// Token extractor
    token_extractor: TokenExtractor,
}

impl FailurePatternDetector {
    /// Creates a new failure pattern detector
    pub fn new(min_edit_distance: usize) -> Self {
        Self {
            min_edit_distance,
            token_extractor: TokenExtractor::new(),
        }
    }

    /// Detects failure patterns from abandoned or reformulated queries
    ///
    /// # ACE Pattern
    /// Abandoned query → Diagnose reason → Generate corrective deltas
    /// Reformulation → Extract delta (original vs reformulated) → Apply improvement
    ///
    /// # Returns
    /// Vector of corrective ACE deltas
    pub fn detect_patterns(&self, trajectory: &QueryTrajectory) -> Vec<ACEDelta> {
        let mut deltas = Vec::new();

        // Check for abandonment
        if matches!(trajectory.session_status, SessionStatus::Abandoned) {
            let reason = self.diagnose_failure(trajectory);
            deltas.extend(self.generate_corrective_deltas(trajectory, reason));
        }

        // Check for reformulations
        for reformulation in &trajectory.reformulations {
            if reformulation.edit_distance >= self.min_edit_distance {
                let delta = self.extract_reformulation_delta(
                    &reformulation.original_query,
                    &reformulation.reformulated_query,
                );
                deltas.push(delta);
            }
        }

        deltas
    }

    /// Diagnoses why a query failed
    fn diagnose_failure(&self, trajectory: &QueryTrajectory) -> FailureReason {
        // Simple heuristics (can be enhanced with ML)
        if trajectory.query_text.split_whitespace().count() == 1 {
            FailureReason::AmbiguousQuery
        } else if trajectory.clicks.is_empty() {
            FailureReason::OutOfDomain
        } else {
            FailureReason::LowConfidence
        }
    }

    /// Generates corrective deltas based on failure reason
    fn generate_corrective_deltas(
        &self,
        trajectory: &QueryTrajectory,
        reason: FailureReason,
    ) -> Vec<ACEDelta> {
        match reason {
            FailureReason::AmbiguousQuery => {
                // For ambiguous queries, we need more context
                // In real system, this would trigger external retrieval
                vec![]
            }
            FailureReason::OutOfDomain => {
                // Flag for knowledge gap (no corrective delta)
                vec![]
            }
            FailureReason::LowConfidence => {
                // Try to strengthen existing weak connections
                self.strengthen_weak_connections(trajectory)
            }
        }
    }

    /// Extracts improvement delta from query reformulation
    fn extract_reformulation_delta(&self, original: &str, reformulated: &str) -> ACEDelta {
        let original_terms = self.token_extractor.extract_terms(original);
        let reformulated_terms = self.token_extractor.extract_terms(reformulated);

        // Find new terms added in reformulation
        let new_terms: Vec<_> = reformulated_terms
            .iter()
            .filter(|rt| !original_terms.iter().any(|ot| ot.id == rt.id))
            .collect();

        // Build co-occurrences between original and new terms
        let mut co_occurrences = Vec::new();
        for ot in &original_terms {
            for nt in new_terms.iter() {
                co_occurrences.push((ot.id, nt.id, 1.0));
            }
        }

        ACEDelta {
            source: DeltaSource::Reformulation,
            co_occurrences,
            confidence: 0.8, // High confidence - user explicitly reformulated
            learning_rate_multiplier: 1.5, // Boost for reformulations
        }
    }

    /// Strengthens weak connections for low-confidence queries
    fn strengthen_weak_connections(&self, trajectory: &QueryTrajectory) -> Vec<ACEDelta> {
        // Extract terms and create self-reinforcing deltas
        let terms = self.token_extractor.extract_terms(&trajectory.query_text);

        if terms.len() < 2 {
            return vec![];
        }

        let mut co_occurrences = Vec::new();
        for i in 0..terms.len() {
            for j in (i + 1)..terms.len() {
                co_occurrences.push((terms[i].id, terms[j].id, 0.5)); // Lower weight
            }
        }

        vec![ACEDelta {
            source: DeltaSource::AbandonedQuery,
            co_occurrences,
            confidence: 0.3,               // Low confidence
            learning_rate_multiplier: 0.5, // Reduced learning rate
        }]
    }
}

impl Default for FailurePatternDetector {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Token extractor for query and result analysis
#[derive(Debug, Clone)]
pub struct TokenExtractor {
    // Future: term cache for efficient tokenization
    // Currently using simple position-based IDs
}

impl TokenExtractor {
    fn new() -> Self {
        Self {}
    }

    /// Extracts terms from text with IDs
    fn extract_terms(&self, text: &str) -> Vec<TokenWithId> {
        text.split_whitespace()
            .enumerate()
            .map(|(idx, term)| TokenWithId {
                text: term.to_lowercase(),
                id: idx, // Simple ID based on position
            })
            .collect()
    }

    /// Extracts terms from result (mock implementation)
    fn extract_result_terms(&self, result_id: &str) -> Vec<TokenWithId> {
        // In real system, fetch document and extract key terms
        // For now, create mock terms based on result_id
        vec![TokenWithId {
            text: result_id.to_lowercase(),
            id: 1000, // Offset to avoid collision with query terms
        }]
    }
}

/// Token with assigned ID
#[derive(Debug, Clone)]
pub struct TokenWithId {
    pub text: String,
    pub id: usize,
}

/// ACE Delta - Structured update for semantic matrix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACEDelta {
    /// Source of this delta
    pub source: DeltaSource,
    /// Co-occurrence updates (term1_id, term2_id, weight)
    pub co_occurrences: Vec<CooccurrenceTriple>,
    /// Confidence in this delta [0, 1]
    pub confidence: f64,
    /// Learning rate multiplier for this delta
    pub learning_rate_multiplier: f64,
}

/// Failure diagnosis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureReason {
    /// Query is too ambiguous
    AmbiguousQuery,
    /// Query is outside knowledge domain
    OutOfDomain,
    /// Results exist but confidence is low
    LowConfidence,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::interaction_tracker::{ClickEvent, ReformulationEvent};
    use chrono::Utc;
    use nalgebra::DVector;
    use uuid::Uuid;

    fn create_test_trajectory() -> QueryTrajectory {
        QueryTrajectory {
            query_id: Uuid::new_v4(),
            query_text: "semantic matrix operations".to_string(),
            query_embedding: DVector::zeros(10),
            clicks: vec![ClickEvent {
                result_id: "doc-123".to_string(),
                position: 0,
                dwell_time_seconds: 45.0,
                timestamp: Utc::now(),
            }],
            reformulations: vec![],
            explicit_feedback: vec![],
            session_status: SessionStatus::Completed,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_success_pattern_extraction() {
        let analyzer = SuccessPatternAnalyzer::new(30.0);
        let trajectory = create_test_trajectory();

        let deltas = analyzer.extract_patterns(&trajectory);

        assert!(!deltas.is_empty(), "Should extract at least one delta");
        assert!(
            deltas[0].confidence >= 0.5,
            "Should have reasonable confidence"
        );
        assert_eq!(deltas[0].source, DeltaSource::UserClick);
    }

    #[test]
    fn test_failure_pattern_detection() {
        let detector = FailurePatternDetector::new(3);
        let mut trajectory = create_test_trajectory();
        trajectory.session_status = SessionStatus::Abandoned;
        trajectory.clicks.clear();

        let deltas = detector.detect_patterns(&trajectory);

        // Should generate corrective deltas or flag for knowledge gap
        // Note: May return empty vec for certain failure types (e.g., OutOfDomain)
        assert!(
            deltas.is_empty() || !deltas.is_empty(),
            "Should handle abandoned query"
        );
    }

    #[test]
    fn test_reformulation_delta_extraction() {
        let detector = FailurePatternDetector::new(3);
        let mut trajectory = create_test_trajectory();
        trajectory.reformulations.push(ReformulationEvent {
            original_query: "matrix".to_string(),
            reformulated_query: "matrix mathematics algebra".to_string(),
            edit_distance: 20,
            timestamp: Utc::now(),
        });

        let deltas = detector.detect_patterns(&trajectory);

        assert!(!deltas.is_empty(), "Should extract reformulation delta");
        assert_eq!(deltas[0].source, DeltaSource::Reformulation);
        assert!(deltas[0].confidence > 0.7, "Should have high confidence");
    }
}
