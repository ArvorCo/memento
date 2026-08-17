//! Self-Reflection Engine - Learn from failures without ground truth
//!
//! Based on Early Experience paper (arXiv:2510.08558).
//! Enables self-improvement by hypothesizing failure reasons and testing hypotheses.

use crate::learning::interaction_tracker::QueryTrajectory;
use serde::{Deserialize, Serialize};

/// Suboptimal Action Analyzer - Self-reflects on failures
#[derive(Debug, Clone)]
pub struct SuboptimalActionAnalyzer {
    /// Historical failure patterns for hypothesis testing
    failure_history: Vec<FailureCase>,
    /// Hypothesis testing threshold (reserved for future use)
    _confidence_threshold: f64,
}

impl SuboptimalActionAnalyzer {
    /// Creates a new self-reflection engine
    pub fn new(confidence_threshold: f64) -> Self {
        Self {
            failure_history: Vec::new(),
            _confidence_threshold: confidence_threshold,
        }
    }

    /// Reflects on a failed trajectory without ground truth
    ///
    /// # Early Experience Pattern
    /// 1. Hypothesize failure reasons (ambiguous, wrong domain, missing concepts)
    /// 2. Test each hypothesis against historical success patterns
    /// 3. Generate self-improvement edits for validated hypotheses
    ///
    /// # Returns
    /// Vector of self-improvement edits to apply
    pub fn reflect_on_failure(&mut self, trajectory: &QueryTrajectory) -> Vec<SelfImprovementEdit> {
        let mut edits = Vec::new();

        // Record this failure
        self.failure_history.push(FailureCase {
            query_text: trajectory.query_text.clone(),
            _query_pattern: self.extract_pattern(&trajectory.query_text),
            _click_count: trajectory.clicks.len(),
            _reformulation_count: trajectory.reformulations.len(),
        });

        // Generate hypotheses
        let hypotheses = self.generate_hypotheses(trajectory);

        // Test and generate edits
        for hypothesis in hypotheses {
            if self.test_hypothesis(&hypothesis, trajectory) {
                if let Some(edit) = self.generate_edit_from_hypothesis(&hypothesis, trajectory) {
                    edits.push(edit);
                }
            }
        }

        edits
    }

    /// Generates failure hypotheses based on trajectory characteristics
    fn generate_hypotheses(&self, trajectory: &QueryTrajectory) -> Vec<FailureHypothesis> {
        let mut hypotheses = Vec::new();

        let term_count = trajectory.query_text.split_whitespace().count();

        // Hypothesis 1: Query is ambiguous (too few terms)
        if term_count <= 2 {
            hypotheses.push(FailureHypothesis::AmbiguousQuery {
                query: trajectory.query_text.clone(),
                reason: "Query too short, needs more context".to_string(),
            });
        }

        // Hypothesis 2: Query is in wrong domain (no similar past successes)
        if !self.has_similar_success(&trajectory.query_text) {
            hypotheses.push(FailureHypothesis::WrongDomain {
                query: trajectory.query_text.clone(),
                suggested_domain: "unknown".to_string(),
            });
        }

        // Hypothesis 3: Missing key concepts (reformulation attempted but failed)
        if !trajectory.reformulations.is_empty() {
            hypotheses.push(FailureHypothesis::MissingConcepts {
                query: trajectory.query_text.clone(),
                missing_concepts: self.infer_missing_concepts(&trajectory.reformulations),
            });
        }

        hypotheses
    }

    /// Tests a hypothesis against historical patterns
    fn test_hypothesis(
        &self,
        hypothesis: &FailureHypothesis,
        trajectory: &QueryTrajectory,
    ) -> bool {
        match hypothesis {
            FailureHypothesis::AmbiguousQuery { .. } => {
                // Test: Do longer queries succeed more often?
                self.test_length_hypothesis(trajectory)
            }
            FailureHypothesis::WrongDomain { .. } => {
                // Test: Are there any similar successful queries?
                !self.has_similar_success(&trajectory.query_text)
            }
            FailureHypothesis::MissingConcepts { .. } => {
                // Test: Did reformulation add terms that we recognize?
                self.test_reformulation_hypothesis(trajectory)
            }
        }
    }

    /// Generates self-improvement edit from validated hypothesis
    fn generate_edit_from_hypothesis(
        &self,
        hypothesis: &FailureHypothesis,
        _trajectory: &QueryTrajectory,
    ) -> Option<SelfImprovementEdit> {
        match hypothesis {
            FailureHypothesis::AmbiguousQuery { query, reason } => Some(SelfImprovementEdit {
                edit_type: EditType::ExpandQuery,
                target_query: query.clone(),
                proposed_changes: vec!["Add more specific terms".to_string()],
                confidence: 0.6,
                reasoning: reason.clone(),
            }),
            FailureHypothesis::WrongDomain {
                query,
                suggested_domain,
            } => Some(SelfImprovementEdit {
                edit_type: EditType::RedirectToDomain,
                target_query: query.clone(),
                proposed_changes: vec![format!("Consider domain: {}", suggested_domain)],
                confidence: 0.5,
                reasoning: "Query appears to be outside known domains".to_string(),
            }),
            FailureHypothesis::MissingConcepts {
                query,
                missing_concepts,
            } => {
                if missing_concepts.is_empty() {
                    return None;
                }
                Some(SelfImprovementEdit {
                    edit_type: EditType::AddConcepts,
                    target_query: query.clone(),
                    proposed_changes: missing_concepts.clone(),
                    confidence: 0.7,
                    reasoning: "Reformulation suggests missing concepts".to_string(),
                })
            }
        }
    }

    /// Tests if query length correlates with success
    fn test_length_hypothesis(&self, trajectory: &QueryTrajectory) -> bool {
        let current_length = trajectory.query_text.split_whitespace().count();

        // Check if past failures have similar length
        let similar_length_failures = self
            .failure_history
            .iter()
            .filter(|f| {
                let len = f.query_text.split_whitespace().count();
                (len as i32 - current_length as i32).abs() <= 1
            })
            .count();

        // If many similar-length queries fail, hypothesis is supported
        similar_length_failures as f64 / self.failure_history.len().max(1) as f64 > 0.3
    }

    /// Tests if reformulation hypothesis is valid
    fn test_reformulation_hypothesis(&self, trajectory: &QueryTrajectory) -> bool {
        if trajectory.reformulations.is_empty() {
            return false;
        }

        // Check if reformulation added terms (edit distance > 0)
        trajectory
            .reformulations
            .iter()
            .any(|r| r.edit_distance > 0)
    }

    /// Checks if there are similar successful queries in history
    fn has_similar_success(&self, query: &str) -> bool {
        // Simplified: check if any past successful query shares terms
        // In real system, use semantic similarity
        let terms: Vec<&str> = query.split_whitespace().collect();

        // For now, assume no history means no success (conservative)
        if self.failure_history.is_empty() {
            return false;
        }

        // Check if any failure has similar terms (pattern matching)
        self.failure_history.iter().any(|f| {
            let f_terms: Vec<&str> = f.query_text.split_whitespace().collect();
            terms.iter().any(|t| f_terms.contains(t))
        })
    }

    /// Infers missing concepts from reformulations
    fn infer_missing_concepts(
        &self,
        reformulations: &[crate::learning::interaction_tracker::ReformulationEvent],
    ) -> Vec<String> {
        let mut concepts = Vec::new();

        for reformulation in reformulations {
            let original_terms: Vec<&str> =
                reformulation.original_query.split_whitespace().collect();
            let new_terms: Vec<&str> = reformulation
                .reformulated_query
                .split_whitespace()
                .collect();

            // Find terms added in reformulation
            for term in new_terms {
                if !original_terms.contains(&term) {
                    concepts.push(term.to_string());
                }
            }
        }

        concepts.truncate(3); // Limit to top 3 concepts
        concepts
    }

    /// Extracts query pattern (simplified)
    fn extract_pattern(&self, query: &str) -> String {
        query
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_")
    }

    /// Gets failure history count
    pub fn failure_count(&self) -> usize {
        self.failure_history.len()
    }
}

impl Default for SuboptimalActionAnalyzer {
    fn default() -> Self {
        Self::new(0.6)
    }
}

/// Failure case recorded for hypothesis testing
#[derive(Debug, Clone)]
struct FailureCase {
    query_text: String,
    _query_pattern: String,
    _click_count: usize,
    _reformulation_count: usize,
}

/// Failure hypothesis for self-reflection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureHypothesis {
    /// Query is too ambiguous
    AmbiguousQuery { query: String, reason: String },

    /// Query is in wrong domain
    WrongDomain {
        query: String,
        suggested_domain: String,
    },

    /// Query is missing key concepts
    MissingConcepts {
        query: String,
        missing_concepts: Vec<String>,
    },
}

/// Self-improvement edit generated from reflection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfImprovementEdit {
    /// Type of edit
    pub edit_type: EditType,
    /// Target query to improve
    pub target_query: String,
    /// Proposed changes
    pub proposed_changes: Vec<String>,
    /// Confidence in this edit [0, 1]
    pub confidence: f64,
    /// Reasoning for this edit
    pub reasoning: String,
}

/// Type of self-improvement edit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditType {
    /// Expand query with more terms
    ExpandQuery,
    /// Redirect to different domain
    RedirectToDomain,
    /// Add missing concepts
    AddConcepts,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::interaction_tracker::{ReformulationEvent, SessionStatus};
    use chrono::Utc;
    use nalgebra::DVector;
    use uuid::Uuid;

    fn create_failed_trajectory(query: &str) -> QueryTrajectory {
        QueryTrajectory {
            query_id: Uuid::new_v4(),
            query_text: query.to_string(),
            query_embedding: DVector::zeros(10),
            clicks: vec![],
            reformulations: vec![],
            explicit_feedback: vec![],
            session_status: SessionStatus::Abandoned,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_self_reflection_creation() {
        let analyzer = SuboptimalActionAnalyzer::new(0.6);
        assert_eq!(analyzer.failure_count(), 0);
    }

    #[test]
    fn test_reflect_on_ambiguous_query() {
        let mut analyzer = SuboptimalActionAnalyzer::new(0.6);
        let trajectory = create_failed_trajectory("matrix"); // Single term = ambiguous

        let edits = analyzer.reflect_on_failure(&trajectory);

        assert!(
            !edits.is_empty(),
            "Should generate edits for ambiguous query"
        );
        assert_eq!(analyzer.failure_count(), 1);
    }

    #[test]
    fn test_reflect_on_reformulation() {
        let mut analyzer = SuboptimalActionAnalyzer::new(0.6);
        let mut trajectory = create_failed_trajectory("matrix");

        trajectory.reformulations.push(ReformulationEvent {
            original_query: "matrix".to_string(),
            reformulated_query: "matrix mathematics".to_string(),
            edit_distance: 11,
            timestamp: Utc::now(),
        });

        let edits = analyzer.reflect_on_failure(&trajectory);

        assert!(
            !edits.is_empty(),
            "Should generate edits from reformulation"
        );
    }

    #[test]
    fn test_hypothesis_generation() {
        let analyzer = SuboptimalActionAnalyzer::new(0.6);
        let trajectory = create_failed_trajectory("test");

        let hypotheses = analyzer.generate_hypotheses(&trajectory);

        assert!(!hypotheses.is_empty(), "Should generate hypotheses");
    }

    #[test]
    fn test_missing_concepts_inference() {
        let analyzer = SuboptimalActionAnalyzer::new(0.6);
        let reformulations = vec![ReformulationEvent {
            original_query: "matrix".to_string(),
            reformulated_query: "matrix algebra linear".to_string(),
            edit_distance: 13,
            timestamp: Utc::now(),
        }];

        let concepts = analyzer.infer_missing_concepts(&reformulations);

        assert!(!concepts.is_empty(), "Should infer missing concepts");
        assert!(concepts.contains(&"algebra".to_string()));
        assert!(concepts.contains(&"linear".to_string()));
    }

    #[test]
    fn test_failure_history_accumulation() {
        let mut analyzer = SuboptimalActionAnalyzer::new(0.6);

        for i in 0..5 {
            let trajectory = create_failed_trajectory(&format!("query {}", i));
            analyzer.reflect_on_failure(&trajectory);
        }

        assert_eq!(analyzer.failure_count(), 5);
    }
}
