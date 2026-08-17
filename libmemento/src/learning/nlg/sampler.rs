//! Eigenspace-guided token sampling for NLG.
//!
//! Uses semantic matrix eigenspace to guide token generation,
//! ensuring semantic coherence and staying within learned knowledge.

use crate::learning::error::{LearningError, Result};
use crate::learning::nlg::context::Context;
use crate::matrix::SemanticMatrix;
use nalgebra::DVector;
use rand::distributions::WeightedIndex;
use rand::prelude::*;

pub struct EigenspaceSampler {
    /// Temperature for sampling (0.0 = greedy, 1.0 = uniform)
    temperature: f64,

    /// Maximum paragraph length in tokens
    max_length: usize,

    /// Minimum eigenspace magnitude threshold (hallucination prevention)
    min_magnitude: f64,
}

impl EigenspaceSampler {
    pub fn new() -> Self {
        Self {
            temperature: 0.7,
            max_length: 50,
            min_magnitude: 0.01,
        }
    }

    /// Generate a paragraph about a concept.
    ///
    /// # Algorithm
    ///
    /// 1. Start with concept tokens as seed
    /// 2. For each position:
    ///    a. Compute semantic direction (query + context in eigenspace)
    ///    b. Score vocabulary by alignment + co-occurrence
    ///    c. Sample next token with temperature
    ///    d. Penalize recently used tokens (diversity)
    /// 3. Stop at sentence boundary, max length, or loop detection
    ///
    /// # Arguments
    ///
    /// * `concept` - Concept to generate paragraph about
    /// * `query_tokens` - Original query tokens
    /// * `context` - Context with evidence
    /// * `matrix` - Semantic matrix
    ///
    /// # Returns
    ///
    /// Generated token sequence
    pub fn generate_paragraph(
        &self,
        concept: &str,
        query_tokens: &[usize],
        conversation_tokens: &[usize],
        context: &Context,
        matrix: &mut SemanticMatrix,
    ) -> Result<Vec<usize>> {
        // FIX 7 (P0): Continuation-based generation (use query as seed)
        // Rationale: Reply should flow naturally from query as continuation
        // Current: "who founded Amazon?" → seed ["Oct", "18,", "2023"] (from concepts) ❌
        // Target: "who founded Amazon?" → seed ["founded", "amazon"] (continuation) ✅
        // Impact: Natural query → reply flow, not random topic jump

        // PRIMARY: Use last 2-3 query tokens as seed (continuation mode)
        let mut tokens = if !query_tokens.is_empty() {
            // Use last 2-3 tokens for natural continuation
            let start_idx = query_tokens.len().saturating_sub(3);
            query_tokens[start_idx..].to_vec()
        } else {
            Vec::new()
        };

        // FALLBACK: If query is empty, try concept words
        if tokens.is_empty() {
            let concept_words: Vec<&str> = concept.split_whitespace().take(3).collect();
            for word in concept_words {
                let normalized = word.to_lowercase();
                if let Some(&token_id) = context.vocabulary.get(&normalized) {
                    tokens.push(token_id);
                }
            }
        }

        if tokens.is_empty() {
            return Err(LearningError::GenerationError(
                "No valid seed tokens (query or concept)".to_string(),
            ));
        }

        // Compute eigendecomposition
        let eigen = matrix.compute_eigendecomposition(100)?;

        // Project query into eigenspace
        let query_embedding = self.project_tokens(query_tokens, &eigen)?;

        // FIX 8 (P0): Project full conversation into eigenspace
        // Rationale: Conversation context provides history for continuation
        // Impact: Enables follow-up questions ("what else did he do?")
        let conversation_embedding = if !conversation_tokens.is_empty() {
            self.project_tokens(conversation_tokens, &eigen)?
        } else {
            DVector::zeros(eigen.num_components())
        };

        // Track token usage for diversity (last 5 tokens)
        let mut recent_tokens: Vec<usize> = Vec::new();
        let diversity_window = 5;

        // Generate tokens
        for _ in 0..self.max_length {
            // FIX 9 (P0): Expand context window from 5 to 10 tokens
            // Rationale: Longer n-gram patterns for better grammar
            // Current: 5 tokens → basic phrases
            // Target: 10 tokens → complex sentence structures
            // Impact: Doubled context window for richer patterns
            let context_window_size = 10;

            // Combine conversation history + generated tokens for full context
            let mut full_context = Vec::with_capacity(context_window_size);

            // Build combined context manually (conversation + generated)
            let combined: Vec<usize> = conversation_tokens
                .iter()
                .chain(tokens.iter())
                .copied()
                .collect();

            // Take last N tokens (most recent context)
            let start_idx = combined.len().saturating_sub(context_window_size);
            full_context.extend_from_slice(&combined[start_idx..]);

            // Project full context window
            let context_embedding = self.project_tokens(&full_context, &eigen)?;

            // Semantic direction (weighted combination: query + conversation + generated)
            // 50% query (what to generate about)
            // 30% conversation (history for continuation)
            // 20% generated (immediate context for grammar)
            let direction =
                0.5 * &query_embedding + 0.3 * &conversation_embedding + 0.2 * &context_embedding;

            // Score vocabulary with FULL context (conversation + generated)
            let mut scores = self.score_vocabulary(&direction, &full_context, &eigen, matrix)?;

            if scores.is_empty() {
                break; // No valid continuations
            }

            // DIVERSITY: Penalize recently used tokens
            for (token_id, score) in scores.iter_mut() {
                let penalty_count = recent_tokens.iter().filter(|&&t| t == *token_id).count();
                if penalty_count > 0 {
                    *score *= 0.3_f64.powi(penalty_count as i32); // Strong penalty for repetition
                }
            }

            // Re-normalize scores after penalty
            let total: f64 = scores.iter().map(|(_, s)| s).sum();
            if total < 1e-10 {
                break; // All scores penalized to zero
            }
            for (_, score) in scores.iter_mut() {
                *score /= total;
            }

            // Sample next token
            let next_token = self.sample_token(&scores)?;

            // Check for sentence end
            if self.is_sentence_end(next_token) {
                tokens.push(next_token);
                break;
            }

            // LOOP DETECTION: If we're generating the same pattern, stop
            if tokens.len() >= 10 {
                let last_5 = &tokens[tokens.len() - 5..];
                let prev_5 = &tokens[tokens.len() - 10..tokens.len() - 5];
                if last_5 == prev_5 {
                    // Detected loop pattern, stop generation
                    break;
                }
            }

            tokens.push(next_token);

            // Track recent tokens for diversity
            recent_tokens.push(next_token);
            if recent_tokens.len() > diversity_window {
                recent_tokens.remove(0);
            }
        }

        Ok(tokens)
    }

    /// Project tokens into eigenspace.
    fn project_tokens(
        &self,
        tokens: &[usize],
        eigen: &crate::matrix::EigenDecomposition,
    ) -> Result<DVector<f64>> {
        let k = eigen.num_components();
        let mut projection = DVector::zeros(k);

        for &token_id in tokens {
            if token_id < eigen.eigenvectors.nrows() {
                for j in 0..k {
                    projection[j] += eigen.eigenvectors[(token_id, j)];
                }
            }
        }

        // Normalize
        let norm = projection.norm();
        if norm > 1e-10 {
            projection /= norm;
        }

        Ok(projection)
    }

    /// Score all vocabulary tokens for next token selection.
    ///
    /// FIX 2 (P0): N-gram scoring with context window
    /// Rationale: Co-occurrence of full context → grammatical patterns
    /// Current: P(token | last_token) → "amazon bezos"
    /// Target: P(token | last_5_tokens) → "Amazon was founded by Jeff"
    /// Impact: Models phrases like "was founded by", "in the year"
    fn score_vocabulary(
        &self,
        direction: &DVector<f64>,
        context_window: &[usize],
        eigen: &crate::matrix::EigenDecomposition,
        matrix: &mut SemanticMatrix,
    ) -> Result<Vec<(usize, f64)>> {
        let vocab_size = eigen.eigenvectors.nrows();
        let k = eigen.num_components();
        let mut scores = Vec::with_capacity(vocab_size);

        for token_id in 0..vocab_size {
            // Extract token eigenvector
            let mut token_embedding = DVector::zeros(k);
            for j in 0..k {
                token_embedding[j] = eigen.eigenvectors[(token_id, j)];
            }

            // Check magnitude (hallucination prevention)
            let magnitude = token_embedding.norm();
            if magnitude < self.min_magnitude {
                continue; // Token not learned enough
            }

            // 1. Semantic similarity (cosine)
            let semantic_score = self.cosine_similarity(direction, &token_embedding);

            // 2. N-gram co-occurrence score (NEW!)
            let ngram_score = self.compute_ngram_score(context_window, token_id, matrix)?;

            // FIX 3 (P1): Dynamic weight adjustment
            // Rationale: Start of sentence needs semantic guidance, middle/end needs grammar
            // Position 0-5: 0.6 semantic + 0.4 ngram (find topic)
            // Position 6+:  0.3 semantic + 0.7 ngram (grammar)
            // Impact: Natural topic introduction + grammatical completion
            let context_length = context_window.len();
            let semantic_weight = if context_length < 5 {
                0.6 // Early: semantic guidance
            } else {
                0.3 // Late: grammar guidance
            };
            let ngram_weight = 1.0 - semantic_weight;

            // 3. Combined score with adjusted weights
            let score = semantic_weight * semantic_score + ngram_weight * ngram_score;

            if score > 0.01 {
                scores.push((token_id, score));
            }
        }

        Ok(scores)
    }

    /// Compute n-gram co-occurrence score for candidate token.
    ///
    /// Uses weighted average of co-occurrences with context window:
    /// - Most recent token: weight 0.4
    /// - 2nd most recent: weight 0.3
    /// - 3rd most recent: weight 0.2
    /// - 4th most recent: weight 0.1
    /// - 5th most recent: weight 0.05
    ///
    /// This models n-gram patterns like "was founded by" where recent context matters most.
    fn compute_ngram_score(
        &self,
        context_window: &[usize],
        candidate_token: usize,
        matrix: &mut SemanticMatrix,
    ) -> Result<f64> {
        if context_window.is_empty() {
            return Ok(0.0);
        }

        // Weight decay: recent tokens matter more
        let weights = [0.4, 0.3, 0.2, 0.1, 0.05];
        let mut weighted_score = 0.0;
        let mut total_weight = 0.0;

        // Iterate through context window (most recent first)
        for (i, &context_token) in context_window.iter().rev().enumerate() {
            if i >= weights.len() {
                break;
            }

            let weight = weights[i];
            let cooccur_score = self.get_cooccurrence(context_token, candidate_token, matrix)?;

            weighted_score += weight * cooccur_score;
            total_weight += weight;
        }

        // Normalize by total weight
        if total_weight > 0.0 {
            Ok(weighted_score / total_weight)
        } else {
            Ok(0.0)
        }
    }

    /// Sample token using temperature-controlled distribution.
    fn sample_token(&self, scores: &[(usize, f64)]) -> Result<usize> {
        if scores.is_empty() {
            return Err(LearningError::GenerationError(
                "No valid tokens to sample".to_string(),
            ));
        }

        // Apply temperature
        let adjusted_scores: Vec<f64> = scores
            .iter()
            .map(|(_, score)| (score / self.temperature).exp())
            .collect();

        // Sample
        let dist = WeightedIndex::new(&adjusted_scores).map_err(|e| {
            LearningError::GenerationError(format!("Failed to create distribution: {}", e))
        })?;

        let mut rng = thread_rng();
        let idx = dist.sample(&mut rng);

        Ok(scores[idx].0)
    }

    /// Compute cosine similarity between vectors.
    fn cosine_similarity(&self, a: &DVector<f64>, b: &DVector<f64>) -> f64 {
        let dot = a.dot(b);
        let norm_a = a.norm();
        let norm_b = b.norm();

        if norm_a > 1e-10 && norm_b > 1e-10 {
            (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }

    /// Get co-occurrence weight from matrix.
    fn get_cooccurrence(
        &self,
        token_a: usize,
        token_b: usize,
        matrix: &mut SemanticMatrix,
    ) -> Result<f64> {
        // Get compressed matrix
        let compressed = matrix.get_compressed()?;

        // Look up co-occurrence
        if token_a < compressed.rows() {
            if let Some(row) = compressed.outer_view(token_a) {
                for (col, &value) in row.iter() {
                    if col == token_b {
                        return Ok(value);
                    }
                }
            }
        }

        Ok(0.0)
    }

    /// Check if token represents sentence end.
    fn is_sentence_end(&self, token_id: usize) -> bool {
        // Simplified: check against common punctuation token IDs
        // In production, use proper vocabulary mapping
        token_id == 0 || token_id == usize::MAX
    }

    /// Measure semantic coherence of generated paragraphs.
    pub fn measure_coherence(
        &self,
        paragraphs: &[Vec<usize>],
        matrix: &mut SemanticMatrix,
    ) -> Result<f64> {
        if paragraphs.is_empty() {
            return Ok(0.0);
        }

        let eigen = matrix.compute_eigendecomposition(100)?;
        let mut total_coherence = 0.0;

        for paragraph in paragraphs {
            // Project paragraph
            let embedding = self.project_tokens(paragraph, &eigen)?;

            // Coherence = magnitude in eigenspace (how well does it align with learned structure)
            let magnitude = embedding.norm();
            total_coherence += magnitude;
        }

        Ok(total_coherence / (paragraphs.len() as f64))
    }
}

impl Default for EigenspaceSampler {
    fn default() -> Self {
        Self::new()
    }
}
