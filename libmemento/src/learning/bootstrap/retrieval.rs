use super::*;
use std::collections::HashSet;

impl BootstrapEngine {
    pub(super) fn ingest_document_with_rate(
        &mut self,
        doc: &Document,
        learning_rate: f64,
    ) -> Result<()> {
        let token_ids: Vec<usize> = doc
            .tokens
            .iter()
            .map(|t| self.get_token_id(&t.to_lowercase()))
            .collect();

        let window_size = 5;
        for window in token_ids.windows(window_size) {
            let center_idx = window_size / 2;
            let center_token = window[center_idx];

            for (idx, &context_token) in window.iter().enumerate() {
                if idx != center_idx && context_token != center_token {
                    let weight = learning_rate / (window_size as f64);
                    self.semantic_matrix
                        .add_cooccurrence(center_token, context_token, weight)?;
                }
            }
        }

        self.total_tokens_processed += token_ids.len();
        Ok(())
    }

    pub(super) fn execute_query(&mut self, query: &str) -> Result<Vec<SearchResult>> {
        let query_terms: HashSet<String> =
            query.split_whitespace().map(|t| t.to_lowercase()).collect();

        let mut scored_docs: Vec<(f64, &Document)> = self
            .seed_corpus
            .iter()
            .map(|doc| {
                let doc_terms: HashSet<String> =
                    doc.tokens.iter().map(|t| t.to_lowercase()).collect();
                let overlap = query_terms.intersection(&doc_terms).count();
                let score = (overlap as f64) / (query_terms.len() as f64).max(1.0);
                (score, doc)
            })
            .collect();

        scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        Ok(scored_docs
            .iter()
            .take(10)
            .filter(|(score, _)| *score > 0.0)
            .map(|(score, doc)| SearchResult {
                doc_id: doc.id.clone(),
                score: *score,
                content: doc.content.clone(),
            })
            .collect())
    }

    pub(super) fn simulate_click(&mut self, results: &[SearchResult]) -> Result<SearchResult> {
        results
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .cloned()
            .ok_or_else(|| LearningError::BootstrapFailed("No results to click".to_string()))
    }

    pub(super) fn apply_learning_delta(
        &mut self,
        query: &str,
        clicked_result: &SearchResult,
        learning_rate: f64,
    ) -> Result<()> {
        let query_token_ids = self.tokenize(query);
        let clicked_doc = self
            .seed_corpus
            .iter()
            .find(|d| d.id == clicked_result.doc_id)
            .ok_or_else(|| {
                LearningError::BootstrapFailed("Clicked document not found".to_string())
            })?
            .clone();

        let result_tokens: Vec<String> = clicked_doc
            .tokens
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let result_token_ids: Vec<usize> =
            result_tokens.iter().map(|t| self.get_token_id(t)).collect();

        for &qt in &query_token_ids {
            for &rt in &result_token_ids {
                if qt != rt {
                    self.semantic_matrix
                        .add_cooccurrence(qt, rt, learning_rate)?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn reflect_on_failure(&mut self, _query: &str, _learning_rate: f64) -> Result<()> {
        Ok(())
    }
}
