use super::*;

const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;
const MAX_SEMANTIC_EXPANSION_TERMS: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Posting {
    chunk_idx: usize,
    term_frequency: u32,
}

#[derive(Debug, Default)]
struct CandidateAccumulator {
    bm25: f64,
    matched_idf: f64,
    matched_groups: usize,
    semantic_score: f64,
    metadata_hits: usize,
    matched_metadata_weight: f64,
    numeric_metadata_hits: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LexicalCandidate {
    pub chunk_idx: usize,
    pub lexical_score: f64,
    pub semantic_score: f64,
    pub query_coverage: f64,
    pub graph_score: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct DocumentLexicalScores {
    pub lexical_score: f64,
    pub coverage_score: f64,
    pub specificity_bonus: f64,
}

/// Rebuildable query-time lexical view over canonical chunks and stable token IDs.
#[derive(Debug, Default)]
pub(super) struct LexicalIndex {
    body_postings: HashMap<usize, Vec<Posting>>,
    metadata_postings: HashMap<String, Vec<usize>>,
    metadata_document_frequency: HashMap<String, usize>,
    document_token_ids: HashMap<DocId, Vec<usize>>,
    document_metadata_terms: HashMap<DocId, Vec<String>>,
    chunk_doc_ids: Vec<DocId>,
    document_chunk_indices: HashMap<DocId, Vec<usize>>,
    chunk_lengths: Vec<u32>,
    source_chunk_counts: HashMap<String, usize>,
    document_count: usize,
    total_tokens: u64,
    latest_recency_ordinal: Option<i64>,
}

impl LexicalIndex {
    pub fn build(chunks: &[StoredChunk], documents: &[StoredDocument]) -> Self {
        let documents_by_id = documents
            .iter()
            .map(|document| (document.doc_id, document))
            .collect::<HashMap<_, _>>();
        let mut index = Self::default();
        let mut document_metadata = HashMap::<DocId, HashSet<String>>::new();
        let mut document_tokens = HashMap::<DocId, HashSet<usize>>::new();

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let document = documents_by_id.get(&chunk.doc_id).copied();
            let metadata_terms = metadata_terms(chunk, document);
            index.append_chunk(
                chunk_idx,
                chunk.doc_id,
                &chunk.token_ids,
                &metadata_terms,
                &chunk.source_path,
            );
            document_metadata
                .entry(chunk.doc_id)
                .or_default()
                .extend(metadata_terms);
            document_tokens
                .entry(chunk.doc_id)
                .or_default()
                .extend(chunk.token_ids.iter().copied());
        }

        index.document_count = documents.len();
        for terms in document_metadata.values() {
            index.record_document_metadata(terms);
        }
        index.document_token_ids = document_tokens
            .into_iter()
            .map(|(doc_id, terms)| {
                let mut terms = terms.into_iter().collect::<Vec<_>>();
                terms.sort_unstable();
                (doc_id, terms)
            })
            .collect();
        index.document_metadata_terms = document_metadata
            .into_iter()
            .map(|(doc_id, terms)| {
                let mut terms = terms.into_iter().collect::<Vec<_>>();
                terms.sort_unstable();
                (doc_id, terms)
            })
            .collect();
        index.latest_recency_ordinal = documents.iter().filter_map(document_recency_ordinal).max();
        index
    }

    pub fn append_chunk(
        &mut self,
        chunk_idx: usize,
        doc_id: DocId,
        token_ids: &[usize],
        metadata_terms: &[String],
        source_path: &str,
    ) {
        debug_assert_eq!(chunk_idx, self.chunk_lengths.len());
        self.chunk_lengths.push(token_ids.len() as u32);
        self.chunk_doc_ids.push(doc_id);
        self.document_chunk_indices
            .entry(doc_id)
            .or_default()
            .push(chunk_idx);
        self.total_tokens += token_ids.len() as u64;
        *self
            .source_chunk_counts
            .entry(source_path.to_string())
            .or_default() += 1;

        let mut frequencies = HashMap::<usize, u32>::new();
        for token_id in token_ids {
            *frequencies.entry(*token_id).or_default() += 1;
        }
        for (token_id, term_frequency) in frequencies {
            self.body_postings
                .entry(token_id)
                .or_default()
                .push(Posting {
                    chunk_idx,
                    term_frequency,
                });
        }

        for term in metadata_terms {
            self.metadata_postings
                .entry(term.clone())
                .or_default()
                .push(chunk_idx);
        }
    }

    pub fn finish_document(
        &mut self,
        doc_id: DocId,
        token_ids: &HashSet<usize>,
        metadata_terms: &HashSet<String>,
        source_text: &str,
    ) {
        self.document_count += 1;
        self.record_document_metadata(metadata_terms);
        let mut token_ids = token_ids.iter().copied().collect::<Vec<_>>();
        token_ids.sort_unstable();
        self.document_token_ids.insert(doc_id, token_ids);
        let mut metadata_terms = metadata_terms.iter().cloned().collect::<Vec<_>>();
        metadata_terms.sort_unstable();
        self.document_metadata_terms.insert(doc_id, metadata_terms);
        if let Some((year, month, day)) = parse_date_tokens(source_text) {
            let ordinal = date_ordinal(year, month, day);
            self.latest_recency_ordinal = Some(
                self.latest_recency_ordinal
                    .map_or(ordinal, |current| current.max(ordinal)),
            );
        }
    }

    pub fn candidates(
        &self,
        query_token_groups: &[Vec<usize>],
        query_metadata_terms: &[String],
        semantic_boosts: &HashMap<usize, f64>,
        limit: usize,
    ) -> Vec<LexicalCandidate> {
        if limit == 0 {
            return Vec::new();
        }

        let mut accumulators = HashMap::<usize, CandidateAccumulator>::new();
        let average_chunk_length = if self.chunk_lengths.is_empty() {
            1.0
        } else {
            (self.total_tokens as f64 / self.chunk_lengths.len() as f64).max(1.0)
        };
        let mut total_query_idf = 0.0;
        for group in query_token_groups {
            let mut token_ids = group.clone();
            token_ids.sort_unstable();
            token_ids.dedup();
            let mut group_scores = HashMap::<usize, (f64, f64)>::new();
            let mut group_max_idf: f64 = 0.0;

            for token_id in token_ids {
                let Some(postings) = self.body_postings.get(&token_id) else {
                    continue;
                };
                let idf = bm25_idf(self.chunk_lengths.len(), postings.len());
                group_max_idf = group_max_idf.max(idf);
                for posting in postings {
                    let chunk_length = self
                        .chunk_lengths
                        .get(posting.chunk_idx)
                        .copied()
                        .unwrap_or(0) as f64;
                    let tf = posting.term_frequency as f64;
                    let norm =
                        BM25_K1 * (1.0 - BM25_B + BM25_B * chunk_length / average_chunk_length);
                    let score = idf * (tf * (BM25_K1 + 1.0)) / (tf + norm);
                    let entry = group_scores.entry(posting.chunk_idx).or_default();
                    if score > entry.0 {
                        *entry = (score, idf);
                    }
                }
            }

            total_query_idf += group_max_idf;
            for (chunk_idx, (score, idf)) in group_scores {
                let accumulator = accumulators.entry(chunk_idx).or_default();
                accumulator.bm25 += score;
                accumulator.matched_idf += idf;
                accumulator.matched_groups += 1;
            }
        }

        let mut query_metadata_terms = query_metadata_terms.to_vec();
        query_metadata_terms.sort_unstable();
        query_metadata_terms.dedup();
        let numeric_metadata_terms = query_metadata_terms
            .iter()
            .filter(|term| term.chars().all(|character| character.is_ascii_digit()))
            .count();
        let mut total_metadata_weight = 0.0;
        for term in &query_metadata_terms {
            let document_frequency = self
                .metadata_document_frequency
                .get(term)
                .copied()
                .unwrap_or(0) as f64;
            let rarity =
                ((self.document_count.max(1) as f64 + 1.0) / (document_frequency + 1.0)).ln() + 1.0;
            let numeric = term.chars().all(|character| character.is_ascii_digit());
            let weight = rarity * if numeric { 2.5 } else { 1.0 };
            total_metadata_weight += weight;
            if let Some(chunk_indices) = self.metadata_postings.get(term) {
                for chunk_idx in chunk_indices {
                    let accumulator = accumulators.entry(*chunk_idx).or_default();
                    accumulator.metadata_hits += 1;
                    accumulator.matched_metadata_weight += weight;
                    accumulator.numeric_metadata_hits += usize::from(numeric);
                }
            }
        }

        let mut expansion_terms = semantic_boosts.iter().collect::<Vec<_>>();
        expansion_terms.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        expansion_terms.truncate(MAX_SEMANTIC_EXPANSION_TERMS);
        for (token_id, boost) in expansion_terms {
            if let Some(postings) = self.body_postings.get(token_id) {
                for posting in postings {
                    let accumulator = accumulators.entry(posting.chunk_idx).or_default();
                    accumulator.semantic_score = accumulator.semantic_score.max(*boost);
                }
            }
        }

        let max_bm25 = accumulators
            .values()
            .map(|accumulator| accumulator.bm25)
            .fold(0.0, f64::max);
        let query_group_count = query_token_groups.len().max(1) as f64;
        let candidates = accumulators
            .into_iter()
            .map(|(chunk_idx, accumulator)| {
                let normalized_bm25 = if max_bm25 <= f64::EPSILON {
                    0.0
                } else {
                    accumulator.bm25 / max_bm25
                };
                let idf_coverage = if total_query_idf <= f64::EPSILON {
                    0.0
                } else {
                    accumulator.matched_idf / total_query_idf
                };
                let lexical_score =
                    ((normalized_bm25 * 0.72) + (idf_coverage * 0.28)).clamp(0.0, 1.0);
                let query_coverage = accumulator.matched_groups as f64 / query_group_count;
                let metadata_score = if total_metadata_weight <= f64::EPSILON {
                    0.0
                } else {
                    accumulator.matched_metadata_weight / total_metadata_weight
                };
                let has_direct_evidence =
                    accumulator.matched_groups > 0 || accumulator.metadata_hits > 0;
                let satisfies_numeric_constraint = numeric_metadata_terms >= 3
                    && accumulator.numeric_metadata_hits == numeric_metadata_terms;
                let prefilter_score = (lexical_score * 0.80)
                    + (metadata_score * 0.40)
                    + (accumulator.semantic_score * 0.10);
                (
                    LexicalCandidate {
                        chunk_idx,
                        lexical_score,
                        semantic_score: accumulator.semantic_score,
                        query_coverage: query_coverage.clamp(0.0, 1.0),
                        graph_score: 0.0,
                    },
                    prefilter_score,
                    has_direct_evidence,
                    satisfies_numeric_constraint,
                )
            })
            .collect::<Vec<_>>();

        // Semantic expansion is deliberately recall-only. It may fill unused
        // capacity, but it must never evict a chunk that matched query text or
        // metadata. Learned components can change dimensionality after `learn`;
        // direct lexical recall must remain stable across those changes.
        let (mut numeric_constraint, remaining): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|(_, _, _, satisfies_numeric_constraint)| *satisfies_numeric_constraint);
        let (mut direct, mut semantic_only): (Vec<_>, Vec<_>) = remaining
            .into_iter()
            .partition(|(_, _, has_direct_evidence, _)| *has_direct_evidence);
        let sort_candidates = |candidates: &mut Vec<(LexicalCandidate, f64, bool, bool)>| {
            candidates.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.chunk_idx.cmp(&b.0.chunk_idx))
            });
        };
        sort_candidates(&mut numeric_constraint);
        sort_candidates(&mut direct);
        sort_candidates(&mut semantic_only);
        numeric_constraint.extend(direct);
        let mut direct = numeric_constraint;
        direct.extend(semantic_only);
        direct.truncate(limit);
        direct
            .into_iter()
            .map(|(candidate, _, _, _)| candidate)
            .collect()
    }

    pub fn metadata_term_weights(&self, terms: &[String]) -> HashMap<String, f64> {
        let total_documents = self.document_count.max(1) as f64;
        terms
            .iter()
            .cloned()
            .map(|term| {
                let document_frequency = self
                    .metadata_document_frequency
                    .get(&term)
                    .copied()
                    .unwrap_or(0) as f64;
                let weight = ((total_documents + 1.0) / (document_frequency + 1.0)).ln() + 1.0;
                (term, weight)
            })
            .collect()
    }

    pub fn source_chunk_count(&self, source_path: &str) -> usize {
        self.source_chunk_counts
            .get(source_path)
            .copied()
            .unwrap_or(0)
    }

    pub fn latest_recency_ordinal(&self) -> Option<i64> {
        self.latest_recency_ordinal
    }

    pub fn document_id_for_chunk(&self, chunk_idx: usize) -> Option<DocId> {
        self.chunk_doc_ids.get(chunk_idx).copied()
    }

    pub fn chunk_indices_for_document(&self, doc_id: DocId) -> &[usize] {
        self.document_chunk_indices
            .get(&doc_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn apply_graph_scores(
        &self,
        candidates: &mut Vec<LexicalCandidate>,
        scores: &HashMap<DocId, f64>,
        max_extra_documents: usize,
    ) {
        let mut existing = candidates
            .iter()
            .map(|candidate| candidate.chunk_idx)
            .collect::<HashSet<_>>();
        for candidate in candidates.iter_mut() {
            let Some(doc_id) = self.document_id_for_chunk(candidate.chunk_idx) else {
                continue;
            };
            candidate.graph_score = scores.get(&doc_id).copied().unwrap_or(0.0);
        }

        let mut ranked = scores.iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(right.0))
        });
        for (doc_id, graph_score) in ranked.into_iter().take(max_extra_documents) {
            let Some(chunk_idx) = self
                .document_chunk_indices
                .get(doc_id)
                .and_then(|indices| indices.first())
                .copied()
            else {
                continue;
            };
            if existing.insert(chunk_idx) {
                candidates.push(LexicalCandidate {
                    chunk_idx,
                    lexical_score: 0.0,
                    semantic_score: 0.0,
                    query_coverage: 0.0,
                    graph_score: *graph_score,
                });
            }
        }
    }

    pub fn document_scores(
        &self,
        doc_id: DocId,
        query_token_groups: &[Vec<usize>],
        query_metadata_terms: &[String],
    ) -> DocumentLexicalScores {
        if query_metadata_terms.is_empty() {
            return DocumentLexicalScores::default();
        }

        let token_ids = self.document_token_ids.get(&doc_id);
        let metadata_terms = self.document_metadata_terms.get(&doc_id);
        let content_hits = query_token_groups
            .iter()
            .filter(|group| {
                token_ids.is_some_and(|tokens| {
                    group
                        .iter()
                        .any(|token_id| tokens.binary_search(token_id).is_ok())
                })
            })
            .count();
        let metadata_hits = query_metadata_terms
            .iter()
            .filter(|term| metadata_terms.is_some_and(|terms| terms.binary_search(term).is_ok()))
            .count();
        let query_term_count = query_metadata_terms.len() as f64;
        let lexical_score =
            ((metadata_hits as f64 * 2.0) + content_hits as f64) / (query_term_count * 3.0);
        let coverage_score = metadata_hits.max(content_hits) as f64 / query_term_count;
        let specificity_bonus = if coverage_score >= 0.99 {
            0.18
        } else if coverage_score >= 0.75 {
            0.08
        } else {
            0.0
        };

        DocumentLexicalScores {
            lexical_score: lexical_score.clamp(0.0, 1.0),
            coverage_score: coverage_score.clamp(0.0, 1.0),
            specificity_bonus,
        }
    }

    fn record_document_metadata(&mut self, terms: &HashSet<String>) {
        for term in terms {
            *self
                .metadata_document_frequency
                .entry(term.clone())
                .or_default() += 1;
        }
    }
}

pub(super) fn metadata_terms(
    chunk: &StoredChunk,
    document: Option<&StoredDocument>,
) -> Vec<String> {
    let mut text = chunk.source_path.clone();
    if let Some(title) = document.and_then(|document| document.title.as_deref()) {
        text.push(' ');
        text.push_str(title);
    }
    if let Some(section_title) = &chunk.section_title {
        text.push(' ');
        text.push_str(section_title);
    }
    let mut terms = tokenize_folded_text(&text);
    terms.sort_unstable();
    terms.dedup();
    terms
}

fn document_recency_ordinal(document: &StoredDocument) -> Option<i64> {
    let mut text = document.source_path.clone();
    if let Some(title) = &document.title {
        text.push(' ');
        text.push_str(title);
    }
    parse_date_tokens(&text).map(|(year, month, day)| date_ordinal(year, month, day))
}

fn date_ordinal(year: u32, month: u32, day: u32) -> i64 {
    (year as i64 * 372) + (month as i64 * 31) + day as i64
}

fn bm25_idf(total_chunks: usize, matching_chunks: usize) -> f64 {
    let total_chunks = total_chunks.max(1) as f64;
    let matching_chunks = matching_chunks as f64;
    (1.0 + (total_chunks - matching_chunks + 0.5) / (matching_chunks + 0.5)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(chunk_id: u64, doc_id: u64, path: &str, token_ids: Vec<usize>) -> StoredChunk {
        StoredChunk {
            chunk_id,
            doc_id,
            span: None,
            content: String::new(),
            chunk_index: 0,
            token_count: token_ids.len(),
            source_path: path.to_string(),
            section_title: None,
            chunk_type: "document".to_string(),
            token_ids,
        }
    }

    fn document(doc_id: u64, path: &str, title: &str) -> StoredDocument {
        StoredDocument {
            doc_id,
            source_path: path.to_string(),
            canonical_text: String::new(),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn bm25_prefers_concentrated_term_evidence() {
        let chunks = vec![
            chunk(0, 0, "generic.md", vec![1, 2, 3, 4, 5, 6, 7, 8]),
            chunk(1, 1, "focused.md", vec![7, 7, 7]),
        ];
        let documents = vec![
            document(0, "generic.md", "Generic"),
            document(1, "focused.md", "Focused"),
        ];
        let index = LexicalIndex::build(&chunks, &documents);

        let results = index.candidates(&[vec![7]], &[], &HashMap::new(), 10);

        assert_eq!(results[0].chunk_idx, 1);
        assert!(results[0].lexical_score > results[1].lexical_score);
        assert_eq!(results[0].query_coverage, 1.0);
    }

    #[test]
    fn metadata_can_recall_a_chunk_without_body_match() {
        let chunks = vec![chunk(0, 0, "memory/current-state.md", vec![1, 2])];
        let documents = vec![document(0, "memory/current-state.md", "Current State")];
        let index = LexicalIndex::build(&chunks, &documents);

        let results = index.candidates(&[], &["current".to_string()], &HashMap::new(), 10);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_idx, 0);
    }

    #[test]
    fn semantic_expansion_uses_postings_without_direct_match() {
        let chunks = vec![chunk(0, 0, "concept.md", vec![42, 42])];
        let documents = vec![document(0, "concept.md", "Concept")];
        let index = LexicalIndex::build(&chunks, &documents);
        let boosts = HashMap::from([(42, 0.8)]);

        let results = index.candidates(&[], &[], &boosts, 10);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].semantic_score, 0.8);
    }

    #[test]
    fn semantic_only_candidates_never_evict_direct_matches() {
        let chunks = vec![
            chunk(0, 0, "direct.md", vec![7]),
            chunk(1, 1, "semantic-a.md", vec![42, 42]),
            chunk(2, 2, "semantic-b.md", vec![42, 42, 42]),
        ];
        let documents = vec![
            document(0, "direct.md", "Direct"),
            document(1, "semantic-a.md", "Semantic A"),
            document(2, "semantic-b.md", "Semantic B"),
        ];
        let index = LexicalIndex::build(&chunks, &documents);
        let boosts = HashMap::from([(42, 1.0)]);

        let results = index.candidates(&[vec![7]], &[], &boosts, 1);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_idx, 0);
        assert_eq!(results[0].query_coverage, 1.0);
    }

    #[test]
    fn complete_numeric_metadata_constraint_survives_a_small_limit() {
        let mut chunks = Vec::new();
        let mut documents = Vec::new();
        for id in 0..20 {
            chunks.push(chunk(id, id, &format!("generic-{id}.md"), vec![7, 7]));
            documents.push(document(id, &format!("generic-{id}.md"), "Sprint"));
        }
        chunks.push(chunk(20, 20, "2026-04-05-sprint.md", vec![7]));
        documents.push(document(20, "2026-04-05-sprint.md", "Sprint"));
        let index = LexicalIndex::build(&chunks, &documents);

        let results = index.candidates(
            &[vec![7]],
            &["2026".to_string(), "04".to_string(), "05".to_string()],
            &HashMap::new(),
            1,
        );

        assert_eq!(results[0].chunk_idx, 20);
    }

    #[test]
    fn synonym_group_counts_as_one_coverage_concept() {
        let chunks = vec![
            chunk(0, 0, "both.md", vec![10, 11]),
            chunk(1, 1, "single.md", vec![11]),
        ];
        let documents = vec![
            document(0, "both.md", "Both forms"),
            document(1, "single.md", "Translated form"),
        ];
        let index = LexicalIndex::build(&chunks, &documents);

        let results = index.candidates(&[vec![10, 11]], &[], &HashMap::new(), 10);

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.query_coverage == 1.0));
    }

    #[test]
    fn document_frequency_counts_metadata_once_per_document() {
        let chunks = vec![
            chunk(0, 0, "same.md", vec![1]),
            chunk(1, 0, "same.md", vec![2]),
            chunk(2, 1, "other.md", vec![3]),
        ];
        let documents = vec![
            document(0, "same.md", "Shared title"),
            document(1, "other.md", "Other"),
        ];
        let index = LexicalIndex::build(&chunks, &documents);

        assert_eq!(index.metadata_document_frequency.get("shared"), Some(&1));
    }
}
