//! Generator: execute queries with semantic synthesis grounded in evidence.

mod semantic;
mod synthesis;
#[cfg(test)]
mod tests;
mod text;

use crate::learning::reasoning_trace::EvidenceChunk;
use crate::matrix::SemanticMatrix;
use serde::{Deserialize, Serialize};

/// Generated answer with semantic synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedAnswer {
    /// Synthesized answer text.
    pub text: String,
    /// Confidence score [0, 1].
    pub confidence: f64,
    /// Key concepts extracted from evidence.
    pub concepts: Vec<String>,
    /// Evidence chunks ranked by semantic relevance.
    pub ranked_evidence: Vec<EvidenceChunk>,
    /// Concept similarity scores (concept → score).
    pub concept_similarities: Vec<(String, f64)>,
}

/// Generator executes queries using the current semantic matrix.
#[derive(Debug, Clone)]
pub struct Generator {
    /// Concepts below this threshold are filtered out.
    pub concept_similarity_threshold: f64,
    /// Maximum number of concepts to extract per answer.
    pub max_concepts_per_answer: usize,
    /// Maximum number of sentences in synthesized answer.
    pub max_sentences: usize,
}

impl Generator {
    /// Creates a new generator with default settings.
    pub fn new() -> Self {
        Self {
            concept_similarity_threshold: 0.3,
            max_concepts_per_answer: 10,
            max_sentences: 4,
        }
    }

    /// Creates a generator with custom settings.
    pub fn with_config(
        concept_similarity_threshold: f64,
        max_concepts_per_answer: usize,
        max_sentences: usize,
    ) -> Self {
        Self {
            concept_similarity_threshold,
            max_concepts_per_answer,
            max_sentences,
        }
    }

    /// Legacy synthesis API with uncertainty phrasing based on confidence.
    pub fn generate_answer(&self, evidence: &[EvidenceChunk], confidence: f64) -> String {
        if confidence < 0.3 {
            let synthesized = self.synthesize_evidence_simple(evidence);
            if synthesized.is_empty() {
                format!(
                    "I have insufficient knowledge about this topic (confidence: {:.2}). No relevant information was found.",
                    confidence
                )
            } else {
                format!(
                    "I have insufficient knowledge about this topic (confidence: {:.2}). I've gathered some preliminary information: {}",
                    confidence, synthesized
                )
            }
        } else if confidence < 0.6 {
            let synthesized = self.synthesize_evidence_simple(evidence);
            format!(
                "Based on available evidence (confidence: {:.2}), {}. Note: This answer may be incomplete.",
                confidence, synthesized
            )
        } else {
            self.synthesize_evidence_simple(evidence)
        }
    }

    /// Generates an answer with semantic understanding using eigenspace projections.
    pub fn generate_with_semantics(
        &self,
        query: &str,
        query_tokens: &[usize],
        evidence: &[EvidenceChunk],
        matrix: &SemanticMatrix,
        confidence: f64,
    ) -> Result<GeneratedAnswer, crate::learning::error::LearningError> {
        let eigen = matrix
            .cached_eigen()
            .ok_or_else(|| {
                crate::learning::error::LearningError::GenerationError(
                    "cached eigendecomposition is unavailable".to_string(),
                )
            })?
            .clone();

        let query_projection = self.project_into_eigenspace(query_tokens, &eigen);
        let query_terms = text::tokenize_terms(query);
        let concepts = self.extract_concepts_from_evidence(evidence, &query_terms);
        let ranked_evidence =
            self.rank_evidence_by_semantics(evidence, &query_projection, &eigen, &query_terms);
        let synthesized_text =
            self.synthesize_multi_sentence(query, &ranked_evidence, &concepts, confidence);
        let concept_similarities = self.compute_concept_similarities(&concepts, &query_projection);

        Ok(GeneratedAnswer {
            text: synthesized_text,
            confidence,
            concepts,
            ranked_evidence,
            concept_similarities,
        })
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}
