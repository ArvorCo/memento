//! Provenance and reasoning trace structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A span within a text document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Citation linking an answer to source material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub chunk_id: String,
    pub source_url: String,
    pub retrieval_score: f64,
    pub span: Span,
}

/// A chunk of retrieved evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceChunk {
    pub chunk_id: String,
    pub source_document_id: String,
    pub text: String,
    pub retrieval_score: f64,
    pub relevance_score: f64,
}

/// A step in the reasoning process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step_id: Uuid,
    pub description: String,
    pub inputs: Vec<String>,
    pub output: String,
}

/// A sub-query in multi-hop reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubQuery {
    pub query: String,
    pub answer: String,
    pub evidence: Vec<EvidenceChunk>,
}

/// Complete reasoning trace for provenance tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningTrace {
    pub query: String,
    pub evidence: Vec<EvidenceChunk>,
    pub reasoning_steps: Vec<ReasoningStep>,
    pub answer: String,
    pub confidence: f64,
    pub citations: Vec<Citation>,
    pub timestamp: DateTime<Utc>,
    pub sub_queries: Vec<SubQuery>,
}

impl ReasoningTrace {
    /// Creates a new reasoning trace.
    pub fn new(query: String, answer: String, confidence: f64) -> Self {
        Self {
            query,
            answer,
            confidence,
            evidence: Vec::new(),
            reasoning_steps: Vec::new(),
            citations: Vec::new(),
            timestamp: Utc::now(),
            sub_queries: Vec::new(),
        }
    }

    /// Adds evidence to the trace.
    pub fn add_evidence(&mut self, evidence: EvidenceChunk) {
        self.evidence.push(evidence);
    }

    /// Adds a reasoning step.
    pub fn add_reasoning_step(&mut self, step: ReasoningStep) {
        self.reasoning_steps.push(step);
    }

    /// Adds a citation.
    pub fn add_citation(&mut self, citation: Citation) {
        self.citations.push(citation);
    }

    /// Adds a sub-query.
    pub fn add_sub_query(&mut self, sub_query: SubQuery) {
        self.sub_queries.push(sub_query);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_trace_creation() {
        let trace = ReasoningTrace::new("test query".to_string(), "test answer".to_string(), 0.8);

        assert_eq!(trace.query, "test query");
        assert_eq!(trace.answer, "test answer");
        assert_eq!(trace.confidence, 0.8);
        assert!(trace.evidence.is_empty());
        assert!(trace.citations.is_empty());
    }

    #[test]
    fn test_add_evidence() {
        let mut trace = ReasoningTrace::new("query".to_string(), "answer".to_string(), 0.5);

        let evidence = EvidenceChunk {
            chunk_id: "chunk1".to_string(),
            source_document_id: "doc1".to_string(),
            text: "evidence text".to_string(),
            retrieval_score: 0.9,
            relevance_score: 0.85,
        };

        trace.add_evidence(evidence);
        assert_eq!(trace.evidence.len(), 1);
        assert_eq!(trace.evidence[0].chunk_id, "chunk1");
    }

    #[test]
    fn test_add_citation() {
        let mut trace = ReasoningTrace::new("query".to_string(), "answer".to_string(), 0.5);

        let citation = Citation {
            chunk_id: "chunk1".to_string(),
            source_url: "https://example.com".to_string(),
            retrieval_score: 0.9,
            span: Span { start: 0, end: 10 },
        };

        trace.add_citation(citation);
        assert_eq!(trace.citations.len(), 1);
        assert_eq!(trace.citations[0].source_url, "https://example.com");
    }
}
