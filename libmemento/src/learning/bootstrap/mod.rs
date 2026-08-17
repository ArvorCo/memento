//! Bootstrap engine for cold-start learning from a seed corpus.

mod phases;
mod query_generation;
mod retrieval;
#[cfg(test)]
mod tests;

use crate::learning::error::{LearningError, Result};
use crate::matrix::{CoherenceMonitor, SemanticMatrix};
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Document representation for bootstrap corpus.
#[derive(Debug, Clone)]
pub struct Document {
    /// Document identifier.
    pub id: String,
    /// Document title.
    pub title: String,
    /// Full document content.
    pub content: String,
    /// Tokenized content.
    pub tokens: Vec<String>,
}

/// Search result from query execution.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Document ID.
    pub doc_id: String,
    /// Relevance score.
    pub score: f64,
    /// Result content snippet.
    pub content: String,
}

/// Learning phases for bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningPhase {
    /// Rapid structure building.
    Babbling,
    /// Pattern refinement.
    FirstWords,
    /// Stable knowledge consolidation.
    Grammar,
}

impl LearningPhase {
    /// Gets learning rate for the current phase.
    pub fn learning_rate(&self) -> f64 {
        match self {
            LearningPhase::Babbling => 0.5,
            LearningPhase::FirstWords => 0.1,
            LearningPhase::Grammar => 0.01,
        }
    }

    /// Gets coherence threshold for transitioning to the next phase.
    pub fn transition_threshold(&self) -> f64 {
        match self {
            LearningPhase::Babbling => 0.3,
            LearningPhase::FirstWords => 0.7,
            LearningPhase::Grammar => 1.0,
        }
    }
}

/// Bootstrap engine for cold-start from zero knowledge.
pub struct BootstrapEngine {
    seed_corpus: Vec<Document>,
    semantic_matrix: SemanticMatrix,
    vocabulary: HashMap<String, usize>,
    reverse_vocab: HashMap<usize, String>,
    next_token_id: usize,
    pub learning_phase: LearningPhase,
    pub coherence_history: Vec<f64>,
    coherence_monitor: CoherenceMonitor,
    total_tokens_processed: usize,
    synthetic_query_count: usize,
    rng: rand::rngs::ThreadRng,
}

impl BootstrapEngine {
    /// Creates a new bootstrap engine.
    pub fn new(seed_corpus: Vec<Document>, vocabulary_size: usize) -> Result<Self> {
        let semantic_matrix = SemanticMatrix::new(vocabulary_size);
        let coherence_monitor = CoherenceMonitor::with_thresholds(0.5, 0.3);

        Ok(Self {
            seed_corpus,
            semantic_matrix,
            vocabulary: HashMap::new(),
            reverse_vocab: HashMap::new(),
            next_token_id: 0,
            learning_phase: LearningPhase::Babbling,
            coherence_history: Vec::new(),
            coherence_monitor,
            total_tokens_processed: 0,
            synthetic_query_count: 0,
            rng: rand::thread_rng(),
        })
    }

    /// Boots from empty matrix to operational coherence.
    pub fn bootstrap(&mut self) -> Result<f64> {
        let start_time = Instant::now();

        println!("=== Bootstrap Engine Starting ===");
        println!("Seed corpus: {} documents", self.seed_corpus.len());
        println!("Target: coherence >0.5 in <3600 seconds (SC-037)");
        println!();

        self.build_vocabulary()?;
        println!("Vocabulary built: {} unique tokens", self.vocabulary.len());

        self.phase_1_babbling()?;
        if self.current_coherence() < 0.7 {
            self.phase_2_first_words()?;
        }
        if self.current_coherence() >= 0.7 {
            self.phase_3_grammar()?;
        }

        let elapsed = start_time.elapsed();
        let final_coherence = self.current_coherence();

        println!();
        println!("=== Bootstrap Summary ===");
        println!("  Time elapsed: {:?}", elapsed);
        println!("  Final coherence: {:.3}", final_coherence);
        println!("  Learning phase: {:?}", self.learning_phase);
        println!("  Tokens processed: {}", self.total_tokens_processed);
        println!("  Synthetic queries: {}", self.synthetic_query_count);
        println!(
            "  Coherence history: {} measurements",
            self.coherence_history.len()
        );

        if final_coherence <= 0.5 {
            return Err(LearningError::BootstrapFailed(format!(
                "Bootstrap coherence too low: {:.3} (target: >0.5)",
                final_coherence
            )));
        }

        if elapsed >= Duration::from_secs(3600) {
            return Err(LearningError::BootstrapFailed(format!(
                "Bootstrap too slow: {:?} (target: <3600s)",
                elapsed
            )));
        }

        println!("✓ Bootstrap successful (SC-037 validated)");
        Ok(final_coherence)
    }

    /// Gets the current coherence score.
    pub fn current_coherence(&self) -> f64 {
        self.coherence_history.last().copied().unwrap_or(0.0)
    }

    /// Gets learning rate for a given coherence level.
    pub fn get_learning_rate(&self, coherence: f64) -> f64 {
        if coherence < 0.3 {
            LearningPhase::Babbling.learning_rate()
        } else if coherence < 0.7 {
            LearningPhase::FirstWords.learning_rate()
        } else {
            LearningPhase::Grammar.learning_rate()
        }
    }

    fn build_vocabulary(&mut self) -> Result<()> {
        for doc in &self.seed_corpus {
            for token in &doc.tokens {
                if !self.vocabulary.contains_key(token) {
                    let token_id = self.next_token_id;
                    self.vocabulary.insert(token.clone(), token_id);
                    self.reverse_vocab.insert(token_id, token.clone());
                    self.next_token_id += 1;
                }
            }
        }
        Ok(())
    }

    fn get_token_id(&mut self, token: &str) -> usize {
        if let Some(&id) = self.vocabulary.get(token) {
            id
        } else {
            let id = self.next_token_id;
            self.vocabulary.insert(token.to_string(), id);
            self.reverse_vocab.insert(id, token.to_string());
            self.next_token_id += 1;
            id
        }
    }

    fn tokenize(&mut self, text: &str) -> Vec<usize> {
        text.split_whitespace()
            .map(|token| self.get_token_id(&token.to_lowercase()))
            .collect()
    }

    fn measure_coherence(&mut self) -> Result<f64> {
        let coherence = self.semantic_matrix.compute_coherence()?;
        self.coherence_history.push(coherence);
        self.coherence_monitor.record_coherence(coherence);
        Ok(coherence)
    }
}
