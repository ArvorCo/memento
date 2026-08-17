use super::*;
use rand::seq::SliceRandom;
use std::collections::HashSet;

impl BootstrapEngine {
    pub(super) fn generate_synthetic_queries(&mut self, count: usize) -> Vec<String> {
        let mut queries = Vec::new();
        let mut used_terms = HashSet::new();
        let mut term_freq: HashMap<String, usize> = HashMap::new();

        for doc in &self.seed_corpus {
            for token in &doc.tokens {
                *term_freq.entry(token.to_lowercase()).or_insert(0) += 1;
            }
        }

        let mut terms: Vec<_> = term_freq.iter().collect();
        terms.sort_by(|a, b| b.1.cmp(a.1));

        for _ in 0..count {
            let query_len = self.rng.gen_range(1..=3);
            let mut query_terms = Vec::new();

            for _ in 0..query_len {
                if let Some((term, _)) = terms.choose(&mut self.rng) {
                    if !used_terms.contains(*term) {
                        query_terms.push(term.to_string());
                        used_terms.insert(term.to_string());
                    }
                }
            }

            if !query_terms.is_empty() {
                queries.push(query_terms.join(" "));
                self.synthetic_query_count += 1;
            }
        }

        queries
    }

    pub(super) fn generate_complex_queries(&mut self, count: usize) -> Vec<String> {
        let mut queries = Vec::new();

        for _ in 0..count {
            let query_len = self.rng.gen_range(2..=4);
            let mut query_terms = Vec::new();

            if let Some(doc) = self.seed_corpus.choose(&mut self.rng) {
                let sampled_tokens: Vec<_> = doc
                    .tokens
                    .choose_multiple(&mut self.rng, query_len)
                    .cloned()
                    .collect();

                query_terms.extend(sampled_tokens);
            }

            if !query_terms.is_empty() {
                queries.push(query_terms.join(" "));
                self.synthetic_query_count += 1;
            }
        }

        queries
    }

    pub(super) fn generate_cross_domain_queries(&mut self, count: usize) -> Vec<String> {
        let mut queries = Vec::new();

        for _ in 0..count {
            let mut query_terms = Vec::new();
            let num_docs = self.rng.gen_range(2..=3);
            let sampled_docs: Vec<_> = self
                .seed_corpus
                .choose_multiple(&mut self.rng, num_docs)
                .collect();

            for doc in sampled_docs {
                if let Some(term) = doc.tokens.choose(&mut self.rng) {
                    query_terms.push(term.clone());
                }
            }

            if !query_terms.is_empty() {
                queries.push(query_terms.join(" "));
                self.synthetic_query_count += 1;
            }
        }

        queries
    }
}
