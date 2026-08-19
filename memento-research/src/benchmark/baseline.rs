//! Deterministic fielded BM25 lexical baseline.

use super::{extract_title, is_generic_term, tokenize};
use anyhow::Result;
use libmemento::parser::document::DocumentParser;
use libmemento::sync::discovery::DiscoveredDocument;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const TITLE_WEIGHT: f64 = 3.0;
const PATH_WEIGHT: f64 = 1.5;
const BODY_WEIGHT: f64 = 1.0;

#[derive(Debug)]
struct Document {
    path: String,
    title: String,
    content: String,
    title_len: usize,
    path_len: usize,
    body_len: usize,
}

#[derive(Debug, Default)]
struct Posting {
    document_id: usize,
    title_frequency: usize,
    path_frequency: usize,
    body_frequency: usize,
}

#[derive(Debug)]
pub(super) struct BaselineIndex {
    documents: Vec<Document>,
    postings: HashMap<String, Vec<Posting>>,
    average_title_len: f64,
    average_path_len: f64,
    average_body_len: f64,
    pub(super) build_ms: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BaselineResult<'a> {
    pub(super) path: &'a str,
    pub(super) content: &'a str,
}

impl BaselineIndex {
    pub(super) fn build(documents: &[DiscoveredDocument]) -> Result<Self> {
        let started = Instant::now();
        let parser = DocumentParser::new();
        let mut parsed_documents = Vec::with_capacity(documents.len());

        for discovered in documents {
            let Ok(content) = parser.parse_file(&discovered.path) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }
            let title = extract_title(&discovered.path, &content);
            let title_tokens = tokenize(&title);
            let path_tokens = path_tokens(&discovered.path);
            let body_tokens = tokenize(&content);
            parsed_documents.push((
                Document {
                    path: super::canonicalize_loose(&discovered.path.to_string_lossy()),
                    title,
                    content,
                    title_len: title_tokens.len(),
                    path_len: path_tokens.len(),
                    body_len: body_tokens.len(),
                },
                title_tokens,
                path_tokens,
                body_tokens,
            ));
        }

        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        let mut final_documents = Vec::with_capacity(parsed_documents.len());
        for (document_id, (document, title, path, body)) in parsed_documents.into_iter().enumerate()
        {
            let mut frequencies: HashMap<String, Posting> = HashMap::new();
            add_field_frequencies(&mut frequencies, title, |posting| {
                &mut posting.title_frequency
            });
            add_field_frequencies(&mut frequencies, path, |posting| {
                &mut posting.path_frequency
            });
            add_field_frequencies(&mut frequencies, body, |posting| {
                &mut posting.body_frequency
            });
            for (term, mut posting) in frequencies {
                posting.document_id = document_id;
                postings.entry(term).or_default().push(posting);
            }
            final_documents.push(document);
        }

        let count = final_documents.len().max(1) as f64;
        let average_title_len = final_documents
            .iter()
            .map(|document| document.title_len)
            .sum::<usize>() as f64
            / count;
        let average_path_len = final_documents
            .iter()
            .map(|document| document.path_len)
            .sum::<usize>() as f64
            / count;
        let average_body_len = final_documents
            .iter()
            .map(|document| document.body_len)
            .sum::<usize>() as f64
            / count;

        Ok(Self {
            documents: final_documents,
            postings,
            average_title_len,
            average_path_len,
            average_body_len,
            build_ms: started.elapsed().as_secs_f64() * 1_000.0,
        })
    }

    pub(super) fn document_count(&self) -> usize {
        self.documents.len()
    }

    pub(super) fn search(&self, query: &str, top_k: usize) -> Vec<BaselineResult<'_>> {
        let mut query_terms: Vec<String> = tokenize(query)
            .into_iter()
            .filter(|term| term.len() >= 2 && !is_generic_term(term))
            .collect();
        query_terms.sort();
        query_terms.dedup();
        let mut scores = vec![0.0; self.documents.len()];
        let document_count = self.documents.len() as f64;

        for term in &query_terms {
            let Some(postings) = self.postings.get(term) else {
                continue;
            };
            let document_frequency = postings.len() as f64;
            let inverse_document_frequency = (1.0
                + (document_count - document_frequency + 0.5) / (document_frequency + 0.5))
                .ln();

            for posting in postings {
                let document = &self.documents[posting.document_id];
                let weighted_frequency = TITLE_WEIGHT
                    * normalized_frequency(
                        posting.title_frequency,
                        document.title_len,
                        self.average_title_len,
                    )
                    + PATH_WEIGHT
                        * normalized_frequency(
                            posting.path_frequency,
                            document.path_len,
                            self.average_path_len,
                        )
                    + BODY_WEIGHT
                        * normalized_frequency(
                            posting.body_frequency,
                            document.body_len,
                            self.average_body_len,
                        );
                scores[posting.document_id] += inverse_document_frequency
                    * (weighted_frequency * (K1 + 1.0))
                    / (K1 + weighted_frequency);
            }
        }

        if !query_terms.is_empty() {
            for (document_id, document) in self.documents.iter().enumerate() {
                let title_terms = tokenize(&document.title);
                if query_terms.iter().all(|term| title_terms.contains(term)) {
                    scores[document_id] += 1.5;
                }
            }
        }

        let mut ranked: Vec<(usize, f64)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score > 0.0)
            .collect();
        ranked.sort_by(|(left_id, left_score), (right_id, right_score)| {
            right_score.total_cmp(left_score).then_with(|| {
                self.documents[*left_id]
                    .path
                    .cmp(&self.documents[*right_id].path)
            })
        });
        ranked
            .into_iter()
            .take(top_k)
            .map(|(document_id, _)| {
                let document = &self.documents[document_id];
                BaselineResult {
                    path: &document.path,
                    content: &document.content,
                }
            })
            .collect()
    }
}

fn add_field_frequencies<F>(
    frequencies: &mut HashMap<String, Posting>,
    tokens: Vec<String>,
    mut field: F,
) where
    F: FnMut(&mut Posting) -> &mut usize,
{
    for token in tokens {
        *field(frequencies.entry(token).or_default()) += 1;
    }
}

fn normalized_frequency(frequency: usize, length: usize, average_length: f64) -> f64 {
    if frequency == 0 {
        return 0.0;
    }
    let length_ratio = length as f64 / average_length.max(1.0);
    frequency as f64 / (1.0 - B + B * length_ratio)
}

fn path_tokens(path: &Path) -> Vec<String> {
    path.components()
        .flat_map(|component| tokenize(&component.as_os_str().to_string_lossy()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn bm25f_prefers_exact_title_over_body_mentions() {
        let dir = tempfile::tempdir().unwrap();
        let exact = dir.path().join("jose-roberto.md");
        let mention = dir.path().join("daily.md");
        fs::write(&exact, "# Jose Roberto IT Profile\nFocused profile").unwrap();
        fs::write(
            &mention,
            "# Daily Note\nJose Roberto IT profile appeared in a meeting.",
        )
        .unwrap();
        let discovered = libmemento::sync::discovery::discover_documents(dir.path()).unwrap();
        let index = BaselineIndex::build(&discovered).unwrap();

        let results = index.search("Jose Roberto IT Profile", 2);

        assert_eq!(
            results[0].path,
            super::super::canonicalize_loose(&exact.to_string_lossy())
        );
    }

    #[test]
    fn rare_terms_receive_more_weight_than_common_terms() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("rare.md"), "# Note\ncommon quasar").unwrap();
        fs::write(dir.path().join("common-a.md"), "# Note\ncommon common").unwrap();
        fs::write(dir.path().join("common-b.md"), "# Note\ncommon common").unwrap();
        let discovered = libmemento::sync::discovery::discover_documents(dir.path()).unwrap();
        let index = BaselineIndex::build(&discovered).unwrap();

        let results = index.search("common quasar", 3);

        assert!(results[0].path.ends_with("rare.md"));
    }
}
