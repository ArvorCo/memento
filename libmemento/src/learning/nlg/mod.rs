//! Natural Language Generation (NLG) for AGINUS.
//!
//! Transforms retrieved knowledge into coherent natural language
//! using eigenspace-guided token generation.
//!
//! # Architecture
//!
//! - `sampler`: Eigenspace-guided token sampling
//! - `grounding`: Evidence-based hallucination prevention
//! - `formatter`: Markdown and code formatting
//! - `context`: Context/Answer separation

pub mod context;
pub mod formatter;
pub mod grounding;
pub mod sampler;

pub use context::{Answer, Context, ContextBuilder, Evidence, Source};
pub use formatter::{MarkdownFormatter, ResponseFormat};
pub use grounding::EvidenceGrounding;
pub use sampler::EigenspaceSampler;

use crate::learning::error::Result;
use crate::matrix::SemanticMatrix;

/// Complete NLG pipeline.
pub struct NLGEngine {
    sampler: EigenspaceSampler,
    #[allow(dead_code)] // Reserved for future evidence grounding enhancements
    grounding: EvidenceGrounding,
    #[allow(dead_code)] // Reserved for future markdown formatting enhancements
    formatter: MarkdownFormatter,
}

impl NLGEngine {
    pub fn new() -> Self {
        Self {
            sampler: EigenspaceSampler::new(),
            grounding: EvidenceGrounding::new(),
            formatter: MarkdownFormatter::new(),
        }
    }

    /// Generate natural language response from context.
    ///
    /// # Arguments
    ///
    /// * `query_tokens` - Tokenized query
    /// * `context` - Context with evidence and retrieval results
    /// * `matrix` - Semantic matrix for eigenspace guidance
    ///
    /// # Returns
    ///
    /// Synthesized Answer with markdown formatting
    pub fn generate(
        &mut self,
        query_tokens: &[usize],
        conversation_tokens: &[usize],
        context: &Context,
        matrix: &mut SemanticMatrix,
    ) -> Result<Answer> {
        // FIX 5 (P2): Re-enable sampler with fallback safety
        // Rationale: Use AGI generation when confident, evidence when not
        // Decision: confidence > 0.5 AND matrix has > 10K co-occurrences
        // Impact: Gradual rollout, safety net for failure cases

        let compressed = matrix.get_compressed()?;
        let nnz = compressed.nnz();
        let use_sampler = context.confidence > 0.5 && nnz > 10_000;

        if use_sampler {
            eprintln!(
                "📝 Using AGI-style token generation (sampler) - nnz: {}, confidence: {:.2}",
                nnz, context.confidence
            );

            // Try sampler with conversation context
            match self.generate_with_sampler(query_tokens, conversation_tokens, context, matrix) {
                Ok(answer) => return Ok(answer),
                Err(e) => {
                    eprintln!("⚠️  Sampler failed: {}, falling back to evidence", e);
                    // Fall through to fallback
                }
            }
        } else {
            eprintln!(
                "📝 Using evidence-based generation (fallback) - nnz: {}, confidence: {:.2}",
                nnz, context.confidence
            );
        }

        // Fallback: evidence extraction
        self.generate_fallback_answer(context)
    }

    /// Generate answer using eigenspace sampler.
    fn generate_with_sampler(
        &mut self,
        query_tokens: &[usize],
        conversation_tokens: &[usize],
        context: &Context,
        matrix: &mut SemanticMatrix,
    ) -> Result<Answer> {
        // Generate paragraph for each key concept
        let mut paragraphs = Vec::new();

        for concept in context.key_concepts.iter().take(1) {
            let generated_tokens = self.sampler.generate_paragraph(
                concept,
                query_tokens,
                conversation_tokens,
                context,
                matrix,
            )?;

            paragraphs.push(generated_tokens);
        }

        if paragraphs.is_empty() {
            return Err(crate::learning::error::LearningError::GenerationError(
                "No paragraphs generated".to_string(),
            ));
        }

        // FIX 4 (P1): Learn from generated text
        // Rationale: User wants "improves with more and more text and co-occurences at each request"
        // Current: Learn from fetched content only
        // Target: Learn from generated text → improve next generation
        // Impact: Incremental improvement, closes learning loop
        for generated_tokens in &paragraphs {
            self.learn_from_generation(generated_tokens, matrix)?;
        }

        // Convert tokens to text
        let text = self.tokens_to_text(&paragraphs, &context.vocabulary);

        // Compute confidence
        let confidence = self.compute_confidence(&paragraphs, context, matrix)?;

        Ok(Answer {
            text,
            format: ResponseFormat::Markdown,
            citations: context.sources.clone(),
            confidence,
        })
    }

    /// Convert generated token sequences to readable text.
    fn tokens_to_text(
        &self,
        paragraphs: &[Vec<usize>],
        vocabulary: &std::collections::HashMap<String, usize>,
    ) -> String {
        // Create reverse mapping
        let id_to_word: std::collections::HashMap<usize, &str> = vocabulary
            .iter()
            .map(|(word, &id)| (id, word.as_str()))
            .collect();

        let mut output = String::new();

        for (i, paragraph) in paragraphs.iter().enumerate() {
            if i > 0 {
                output.push_str("\n\n");
            }

            for &token_id in paragraph {
                if let Some(word) = id_to_word.get(&token_id) {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push(' ');
                    }
                    output.push_str(word);
                }
            }
        }

        output
    }

    /// Learn co-occurrences from generated text.
    ///
    /// Extracts n-gram patterns and updates semantic matrix.
    /// This closes the learning loop: generate → learn → improve next time.
    fn learn_from_generation(
        &mut self,
        tokens: &[usize],
        matrix: &mut SemanticMatrix,
    ) -> Result<()> {
        // Extract co-occurrences from generated tokens
        let window_size = 5;
        for window in tokens.windows(window_size) {
            for i in 0..window.len() {
                for j in (i + 1)..window.len() {
                    let (a, b) = if window[i] < window[j] {
                        (window[i], window[j])
                    } else {
                        (window[j], window[i])
                    };

                    // Weight by inverse distance
                    let weight = 1.0 / (j - i) as f64;

                    // Update matrix (small learning rate for generated text)
                    matrix.add_cooccurrence(a, b, weight * 0.1)?;
                }
            }
        }

        Ok(())
    }

    fn compute_confidence(
        &self,
        paragraphs: &[Vec<usize>],
        context: &Context,
        matrix: &mut SemanticMatrix,
    ) -> Result<f64> {
        // Confidence = (query confidence + generation coherence) / 2
        let generation_coherence = self.sampler.measure_coherence(paragraphs, matrix)?;
        Ok((context.confidence + generation_coherence) / 2.0)
    }

    fn generate_fallback_answer(&self, context: &Context) -> Result<Answer> {
        // Intelligent fallback: use evidence snippets with smart extraction
        let text = if context.evidence.is_empty() {
            "No sufficient information found to generate an answer.".to_string()
        } else {
            let mut output = String::new();

            // FIX 1: Increase evidence usage from 3 to 10 items (4x improvement)
            // Rationale: We collect 80+ evidence items but only used 3 (96% waste!)
            // Impact: 10 evidence items × 4 sentences = 40 sentences (vs 6 before)
            let evidence_count = 10;

            // Extract key sentences from evidence (with paragraph structure)
            for (idx, evidence) in context.evidence.iter().take(evidence_count).enumerate() {
                // Split into sentences
                // FIX 3: Relax sentence filter from 20 to 10 chars (accept shorter facts)
                // Rationale: "He was born in 1964." (19 chars) was being removed
                // Impact: Keep more valid short sentences → more complete answers
                let sentences: Vec<&str> = evidence
                    .text
                    .split(&['.', '!', '?'][..])
                    .filter(|s| {
                        let trimmed = s.trim();
                        // Accept if:
                        // - Length > 10 chars (reduced from 20)
                        // - Don't count leading/trailing whitespace
                        !trimmed.is_empty() && trimmed.len() > 10
                    })
                    // FIX 2: Increase sentences per evidence from 2 to 4 (2x improvement)
                    // Rationale: 10 evidence × 4 sentences = 40 sentences (vs 6 before)
                    // Impact: Much more comprehensive answers
                    .take(4) // Max 4 sentences per evidence (was 2)
                    .collect();

                // Skip if no valid sentences
                if sentences.is_empty() {
                    continue;
                }

                // FIX 4: Add paragraph structure (each evidence = 1 paragraph)
                // Rationale: Wall of text is hard to read, paragraphs improve engagement
                // Impact: Better readability, scannable structure
                if idx > 0 {
                    output.push_str("\n\n"); // Paragraph break between evidences
                }

                // FIX 5: Improve sentence concatenation within same paragraph
                // Rationale: Proper punctuation and spacing for coherent paragraphs
                for sentence in sentences {
                    let clean = sentence.trim();
                    if !clean.is_empty() {
                        output.push_str(clean);
                        // Ensure sentence ends with punctuation
                        if !clean.ends_with(&['.', '!', '?'][..]) {
                            output.push('.');
                        }
                        output.push(' '); // Space after sentence (within paragraph)
                    }
                }
            }

            // Add key concepts as summary if available
            if !context.key_concepts.is_empty() && !output.is_empty() {
                output.push_str("\n\n**Key topics**: ");
                output.push_str(
                    &context
                        .key_concepts
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }

            output
        };

        Ok(Answer {
            text,
            format: ResponseFormat::Markdown,
            citations: context.sources.clone(),
            confidence: context.confidence * 0.8, // Better confidence for evidence-based answer
        })
    }
}

impl Default for NLGEngine {
    fn default() -> Self {
        Self::new()
    }
}
