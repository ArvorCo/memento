//! Implicit World Model - Query-Result Pattern Learning
//!
//! Based on Early Experience paper (arXiv:2510.08558).
//! Learns "what works" by tracking query patterns that lead to successful results.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Query-Result World Model - Learns implicit mappings from interactions
#[derive(Debug, Clone)]
pub struct QueryResultWorldModel {
    /// Pattern mappings: query pattern → result pattern
    pattern_mappings: HashMap<QueryPattern, Vec<ResultPatternWithScore>>,
    /// Success threshold for pattern confidence
    success_threshold: f64,
}

impl QueryResultWorldModel {
    /// Creates a new world model
    pub fn new(success_threshold: f64) -> Self {
        Self {
            pattern_mappings: HashMap::new(),
            success_threshold,
        }
    }

    /// Learns a mapping from user interaction
    ///
    /// # Arguments
    /// * `query_pattern` - Extracted pattern from query
    /// * `result_pattern` - Pattern from clicked result
    /// * `dwell_time_ms` - How long user spent on result (confidence signal)
    pub fn learn_mapping(
        &mut self,
        query_pattern: QueryPattern,
        result_pattern: ResultPattern,
        dwell_time_ms: u64,
    ) {
        let confidence = self.compute_confidence_from_dwell(dwell_time_ms);

        let entry = self
            .pattern_mappings
            .entry(query_pattern.clone())
            .or_default();

        // Check if result pattern already exists
        if let Some(existing) = entry
            .iter_mut()
            .find(|r| r.pattern.signature == result_pattern.signature)
        {
            // Update existing pattern
            existing.confidence = (existing.confidence + confidence) / 2.0;
            existing.observation_count += 1;
        } else {
            // Add new pattern
            entry.push(ResultPatternWithScore {
                pattern: result_pattern,
                confidence,
                observation_count: 1,
            });
        }
    }

    /// Predicts likely result patterns for a query pattern
    ///
    /// # Returns
    /// Vector of predicted result patterns with confidence scores
    pub fn predict_results(&self, query_pattern: &QueryPattern) -> Vec<ResultPatternWithScore> {
        self.pattern_mappings
            .get(query_pattern)
            .cloned()
            .unwrap_or_default()
    }

    /// Gets prediction accuracy for a query pattern
    ///
    /// # Returns
    /// Success rate [0, 1] based on historical observations
    pub fn get_prediction_accuracy(&self, query_pattern: &QueryPattern) -> f64 {
        if let Some(results) = self.pattern_mappings.get(query_pattern) {
            let successful = results
                .iter()
                .filter(|r| r.confidence >= self.success_threshold)
                .count();
            successful as f64 / results.len() as f64
        } else {
            0.0
        }
    }

    /// Extracts pattern from query text
    pub fn extract_query_pattern(&self, query_text: &str) -> QueryPattern {
        // Simple pattern extraction based on term count and structure
        let terms: Vec<String> = query_text
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();

        let complexity = match terms.len() {
            0..=2 => PatternComplexity::Simple,
            3..=5 => PatternComplexity::Medium,
            _ => PatternComplexity::Complex,
        };

        QueryPattern {
            signature: self.compute_signature(&terms),
            term_count: terms.len(),
            complexity,
        }
    }

    /// Extracts pattern from clicked result
    pub fn extract_result_pattern(&self, result_id: &str, _result_score: f64) -> ResultPattern {
        // In real system, extract key terms from document
        // For now, create simple pattern based on result_id
        ResultPattern {
            signature: result_id.to_string(),
            relevance_score: 0.8,
        }
    }

    /// Computes confidence from dwell time (sigmoid-like)
    fn compute_confidence_from_dwell(&self, dwell_time_ms: u64) -> f64 {
        let seconds = dwell_time_ms as f64 / 1000.0;
        let normalized = seconds / 30.0; // 30 seconds is "full confidence"
        (normalized / (1.0 + normalized)).clamp(0.0, 1.0)
    }

    /// Computes signature from terms (simple hash)
    fn compute_signature(&self, terms: &[String]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for term in terms {
            term.hash(&mut hasher);
        }
        format!("sig_{:x}", hasher.finish())
    }

    /// Gets total number of learned patterns
    pub fn pattern_count(&self) -> usize {
        self.pattern_mappings.len()
    }

    /// Gets total number of observations
    pub fn observation_count(&self) -> usize {
        self.pattern_mappings
            .values()
            .map(|v| v.iter().map(|r| r.observation_count).sum::<usize>())
            .sum()
    }
}

impl Default for QueryResultWorldModel {
    fn default() -> Self {
        Self::new(0.7)
    }
}

/// Query pattern extracted from user query
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryPattern {
    /// Pattern signature (hash of terms)
    pub signature: String,
    /// Number of terms in query
    pub term_count: usize,
    /// Query complexity
    pub complexity: PatternComplexity,
}

/// Result pattern from clicked document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPattern {
    /// Pattern signature
    pub signature: String,
    /// Relevance score
    pub relevance_score: f64,
}

/// Result pattern with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPatternWithScore {
    /// The result pattern
    pub pattern: ResultPattern,
    /// Confidence in this mapping [0, 1]
    pub confidence: f64,
    /// Number of times observed
    pub observation_count: usize,
}

/// Query complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternComplexity {
    /// 1-2 terms
    Simple,
    /// 3-5 terms
    Medium,
    /// 6+ terms
    Complex,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_model_creation() {
        let model = QueryResultWorldModel::new(0.7);
        assert_eq!(model.pattern_count(), 0);
    }

    #[test]
    fn test_learn_mapping() {
        let mut model = QueryResultWorldModel::new(0.7);

        let query_pattern = model.extract_query_pattern("semantic matrix");
        let result_pattern = model.extract_result_pattern("doc-123", 0.9);

        model.learn_mapping(query_pattern.clone(), result_pattern, 45000); // 45 seconds

        assert_eq!(model.pattern_count(), 1);
        assert_eq!(model.observation_count(), 1);
    }

    #[test]
    fn test_predict_results() {
        let mut model = QueryResultWorldModel::new(0.7);

        let query_pattern = model.extract_query_pattern("semantic matrix");
        let result_pattern = model.extract_result_pattern("doc-123", 0.9);

        model.learn_mapping(query_pattern.clone(), result_pattern, 90000); // 90 seconds for high confidence

        let predictions = model.predict_results(&query_pattern);
        assert_eq!(predictions.len(), 1);
        assert!(
            predictions[0].confidence > 0.7,
            "Confidence: {}",
            predictions[0].confidence
        );
    }

    #[test]
    fn test_prediction_accuracy() {
        let mut model = QueryResultWorldModel::new(0.7);

        let query_pattern = model.extract_query_pattern("machine learning");

        // Add successful observations with high dwell time for high confidence
        for _ in 0..3 {
            let result = model.extract_result_pattern("doc-good", 0.9);
            model.learn_mapping(query_pattern.clone(), result, 90000); // 90 seconds for high confidence (>0.75)
        }

        let accuracy = model.get_prediction_accuracy(&query_pattern);
        assert!(
            accuracy > 0.9,
            "Should have high accuracy with consistent observations"
        );
    }

    #[test]
    fn test_pattern_extraction() {
        let model = QueryResultWorldModel::new(0.7);

        let simple = model.extract_query_pattern("matrix");
        assert_eq!(simple.complexity, PatternComplexity::Simple);
        assert_eq!(simple.term_count, 1);

        let medium = model.extract_query_pattern("semantic matrix operations advanced");
        assert_eq!(medium.complexity, PatternComplexity::Medium);
        assert_eq!(medium.term_count, 4);

        let complex =
            model.extract_query_pattern("how to compute eigenvalues of large sparse matrices");
        assert_eq!(complex.complexity, PatternComplexity::Complex);
        assert_eq!(complex.term_count, 8); // "how to compute eigenvalues of large sparse matrices" = 8 tokens
    }

    #[test]
    fn test_multiple_observations_increase_confidence() {
        let mut model = QueryResultWorldModel::new(0.7);

        let query_pattern = model.extract_query_pattern("test query");
        let result_pattern = model.extract_result_pattern("doc-1", 0.9);

        // First observation
        model.learn_mapping(query_pattern.clone(), result_pattern.clone(), 30000);
        let predictions1 = model.predict_results(&query_pattern);
        let confidence1 = predictions1[0].confidence;

        // Second observation
        model.learn_mapping(query_pattern.clone(), result_pattern, 50000);
        let predictions2 = model.predict_results(&query_pattern);
        let confidence2 = predictions2[0].confidence;
        let observations = predictions2[0].observation_count;

        assert_eq!(observations, 2);
        assert!(
            (confidence2 - confidence1).abs() < 0.5,
            "Confidence should be averaged"
        );
    }
}
