//! Context and Answer structures for NLG.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Source of evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

/// Evidence chunk from retrieval.
#[derive(Debug, Clone)]
pub struct Evidence {
    pub text: String,
    pub source_url: String,
    pub confidence: f64,
}

/// Internal reasoning context (not shown to user directly).
#[derive(Debug, Clone)]
pub struct Context {
    /// Sources of evidence
    pub sources: Vec<Source>,

    /// Evidence chunks
    pub evidence: Vec<Evidence>,

    /// Retrieved token pairs from semantic matrix
    pub retrieved_pairs: Vec<(usize, usize, f64)>,

    /// Key concepts extracted
    pub key_concepts: Vec<String>,

    /// Query confidence
    pub confidence: f64,

    /// Reasoning trace (tool calls, steps)
    pub reasoning_trace: Vec<String>,

    /// Vocabulary mapping (word → token_id)
    pub vocabulary: HashMap<String, usize>,

    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            evidence: Vec::new(),
            retrieved_pairs: Vec::new(),
            key_concepts: Vec::new(),
            confidence: 0.0,
            reasoning_trace: Vec::new(),
            vocabulary: HashMap::new(),
            timestamp: Utc::now(),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Response format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    PlainText,
    Markdown,
    Code(CodeLanguage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    Other,
}

/// Synthesized answer.
#[derive(Debug, Clone)]
pub struct Answer {
    /// Generated text (markdown formatted)
    pub text: String,

    /// Format
    pub format: ResponseFormat,

    /// Citations
    pub citations: Vec<Source>,

    /// Generation confidence
    pub confidence: f64,
}

/// Builder for Context.
pub struct ContextBuilder {
    context: Context,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            context: Context::new(),
        }
    }

    pub fn add_source(mut self, source: Source) -> Self {
        self.context.sources.push(source);
        self
    }

    pub fn add_evidence(mut self, evidence: Evidence) -> Self {
        self.context.evidence.push(evidence);
        self
    }

    pub fn add_retrieved_pair(mut self, token_a: usize, token_b: usize, score: f64) -> Self {
        self.context.retrieved_pairs.push((token_a, token_b, score));
        self
    }

    pub fn add_key_concept(mut self, concept: String) -> Self {
        self.context.key_concepts.push(concept);
        self
    }

    pub fn set_confidence(mut self, confidence: f64) -> Self {
        self.context.confidence = confidence;
        self
    }

    pub fn add_reasoning_step(mut self, step: String) -> Self {
        self.context.reasoning_trace.push(step);
        self
    }

    pub fn set_vocabulary(mut self, vocab: HashMap<String, usize>) -> Self {
        self.context.vocabulary = vocab;
        self
    }

    pub fn build(self) -> Context {
        self.context
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
