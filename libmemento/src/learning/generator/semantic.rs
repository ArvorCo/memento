use super::text::tokenize_terms;
use super::*;
use crate::matrix::EigenDecomposition;
use std::collections::{HashMap, HashSet};

impl Generator {
    pub(super) fn project_into_eigenspace(
        &self,
        token_ids: &[usize],
        eigen: &EigenDecomposition,
    ) -> Vec<f64> {
        let k = eigen.num_components();
        let mut projection = vec![0.0; k];

        for &token_id in token_ids {
            if token_id < eigen.eigenvectors.nrows() {
                for (j, p) in projection.iter_mut().enumerate().take(k) {
                    *p += eigen.eigenvectors[(token_id, j)];
                }
            }
        }

        let norm: f64 = projection.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 1e-10 {
            for x in &mut projection {
                *x /= norm;
            }
        }

        projection
    }

    pub(super) fn extract_concepts_from_evidence(
        &self,
        evidence: &[EvidenceChunk],
        query_terms: &[String],
    ) -> Vec<String> {
        let query_term_set: HashSet<&str> = query_terms.iter().map(String::as_str).collect();
        let mut concept_scores: HashMap<String, f64> = HashMap::new();
        let mut concept_sources: HashMap<String, HashSet<String>> = HashMap::new();

        for chunk in evidence.iter().take(5) {
            let unique_terms = tokenize_terms(&chunk.text)
                .into_iter()
                .collect::<HashSet<_>>();
            for term in unique_terms {
                if query_term_set.contains(term.as_str()) {
                    continue;
                }

                let score = chunk.retrieval_score.max(chunk.relevance_score);
                *concept_scores.entry(term.clone()).or_insert(0.0) += score;
                concept_sources
                    .entry(term)
                    .or_default()
                    .insert(chunk.source_document_id.clone());
            }
        }

        let mut concepts: Vec<(String, f64)> = concept_scores
            .into_iter()
            .map(|(term, score)| {
                let source_bonus = concept_sources
                    .get(&term)
                    .map(|sources| sources.len() as f64 * 0.15)
                    .unwrap_or(0.0);
                (term, score + source_bonus)
            })
            .collect();

        concepts.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        concepts.truncate(self.max_concepts_per_answer);
        concepts.into_iter().map(|(term, _)| term).collect()
    }

    pub(super) fn rank_evidence_by_semantics(
        &self,
        evidence: &[EvidenceChunk],
        _query_projection: &[f64],
        _eigen: &EigenDecomposition,
        _query_terms: &[String],
    ) -> Vec<EvidenceChunk> {
        // The daemon's hybrid ranker already combined BM25, metadata, graph,
        // temporal and learned signals. Re-ranking its results from rendered
        // words here duplicates weaker logic and can move the grounded source
        // behind a merely similar document.
        let mut ranked = evidence.to_vec();
        ranked.sort_by(|left, right| {
            right
                .retrieval_score
                .partial_cmp(&left.retrieval_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.source_document_id.cmp(&right.source_document_id))
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        ranked
    }

    pub(super) fn compute_concept_similarities(
        &self,
        concepts: &[String],
        _query_projection: &[f64],
    ) -> Vec<(String, f64)> {
        concepts
            .iter()
            .enumerate()
            .map(|(i, c)| (c.clone(), 1.0 - (i as f64 * 0.1)))
            .collect()
    }
}
