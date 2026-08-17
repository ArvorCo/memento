use super::text::{
    clean_sentence, first_meaningful_sentence, normalize_sentence_key, split_sentences,
    tokenize_terms,
};
use super::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

struct SentenceCandidate {
    text: String,
    source: String,
    terms: HashSet<String>,
    base_score: f64,
}

impl Generator {
    pub(super) fn synthesize_multi_sentence(
        &self,
        query: &str,
        ranked_evidence: &[EvidenceChunk],
        concepts: &[String],
        confidence: f64,
    ) -> String {
        let top_chunks: Vec<_> = ranked_evidence.iter().take(5).collect();

        if top_chunks.is_empty() {
            return if confidence < 0.3 {
                "I have insufficient knowledge about this topic. No relevant information was found."
                    .to_string()
            } else {
                "No evidence available to answer this query.".to_string()
            };
        }

        let query_terms = tokenize_terms(query);
        let explicit_date = explicit_date_key(query);
        let query_term_set = query_terms.iter().cloned().collect::<HashSet<_>>();
        let concept_set = concepts.iter().cloned().collect::<HashSet<_>>();
        let mut seen_sentences = HashSet::new();
        let mut candidates = Vec::new();

        for (chunk_rank, chunk) in top_chunks.iter().enumerate() {
            for sentence in split_sentences(&chunk.text) {
                let cleaned = clean_sentence(&sentence);
                if cleaned.len() < 24 || !seen_sentences.insert(normalize_sentence_key(&cleaned)) {
                    continue;
                }
                let terms = tokenize_terms(&cleaned).into_iter().collect::<HashSet<_>>();
                if terms.is_empty() {
                    continue;
                }
                let query_hits = terms.intersection(&query_term_set).count() as f64;
                let concept_hits = terms.intersection(&concept_set).count() as f64;
                let length_penalty = if cleaned.len() > 320 {
                    ((cleaned.len() - 320) as f64 / 320.0).min(1.0)
                } else {
                    0.0
                };
                let source_date_bonus = if explicit_date
                    .as_ref()
                    .is_some_and(|date| chunk.source_document_id.contains(date))
                {
                    8.0
                } else {
                    0.0
                };
                let base_score = (query_hits * 2.4)
                    + (concept_hits * 0.45)
                    + (chunk.retrieval_score * 1.4)
                    + (4.0 / (chunk_rank as f64 + 1.0))
                    + source_date_bonus
                    - length_penalty;
                candidates.push(SentenceCandidate {
                    text: cleaned,
                    source: chunk.source_document_id.clone(),
                    terms,
                    base_score,
                });
            }
        }

        let mut selected = Vec::<SentenceCandidate>::new();
        let mut covered_query = HashSet::<String>::new();
        let mut covered_concepts = HashSet::<String>::new();
        let mut source_counts = HashMap::<String, usize>::new();
        while selected.len() < self.max_sentences && !candidates.is_empty() {
            let best_index = candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let new_query = candidate
                        .terms
                        .intersection(&query_term_set)
                        .filter(|term| !covered_query.contains(*term))
                        .count() as f64;
                    let new_concepts = candidate
                        .terms
                        .intersection(&concept_set)
                        .filter(|term| !covered_concepts.contains(*term))
                        .count() as f64;
                    let redundancy = selected
                        .iter()
                        .map(|chosen| jaccard_similarity(&candidate.terms, &chosen.terms))
                        .fold(0.0, f64::max);
                    let repeated_source =
                        source_counts.get(&candidate.source).copied().unwrap_or(0) as f64;
                    let score = candidate.base_score + (new_query * 3.0) + (new_concepts * 0.7)
                        - (redundancy * 1.6)
                        - (repeated_source * 0.10);
                    (index, score)
                })
                .max_by(|left, right| {
                    left.1
                        .partial_cmp(&right.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| right.0.cmp(&left.0))
                })
                .map(|(index, _)| index);
            let Some(best_index) = best_index else {
                break;
            };
            let candidate = candidates.swap_remove(best_index);
            covered_query.extend(candidate.terms.intersection(&query_term_set).cloned());
            covered_concepts.extend(candidate.terms.intersection(&concept_set).cloned());
            *source_counts.entry(candidate.source.clone()).or_default() += 1;
            selected.push(candidate);
        }

        let mut sentences = selected
            .iter()
            .map(|candidate| candidate.text.clone())
            .collect::<Vec<_>>();
        if sentences.is_empty() {
            sentences.extend(
                top_chunks
                    .iter()
                    .filter_map(|chunk| first_meaningful_sentence(&chunk.text))
                    .take(self.max_sentences),
            );
        }

        let mut answer = String::new();

        if confidence < 0.3 {
            answer.push_str(&format!(
                "I have limited knowledge about this topic (confidence: {:.2}). ",
                confidence
            ));
            answer.push_str("From available evidence: ");
        } else if confidence < 0.6 {
            answer.push_str(&format!(
                "Based on available evidence (confidence: {:.2}), ",
                confidence
            ));
        }

        for (i, sentence) in sentences.iter().enumerate() {
            if i > 0 {
                answer.push(' ');
            }
            answer.push_str(sentence);
        }

        if !concepts.is_empty() {
            let concept_list = concepts
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            answer.push_str(&format!(" [Key concepts: {}]", concept_list));
        }

        if let Some(primary) = top_chunks.first() {
            let sections = section_outline(&primary.text, 12);
            if !sections.is_empty() {
                answer.push_str(&format!(" [Sections: {}]", sections.join("; ")));
            }
        }

        let mut sources = selected
            .iter()
            .map(|candidate| source_label(&candidate.source))
            .collect::<Vec<_>>();
        sources.sort();
        sources.dedup();
        if !sources.is_empty() {
            answer.push_str(&format!(" [Sources: {}]", sources.join(", ")));
        }

        answer
    }

    pub(super) fn synthesize_evidence_simple(&self, evidence: &[EvidenceChunk]) -> String {
        if evidence.is_empty() {
            return String::new();
        }

        let mut sorted_evidence: Vec<_> = evidence.iter().collect();
        sorted_evidence.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted_evidence
            .iter()
            .take(3)
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn jaccard_similarity(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    let union = left.union(right).count();
    if union == 0 {
        0.0
    } else {
        left.intersection(right).count() as f64 / union as f64
    }
}

fn source_label(source: &str) -> String {
    Path::new(source)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source)
        .to_string()
}

fn explicit_date_key(text: &str) -> Option<String> {
    let numbers = text
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok())
        .collect::<Vec<_>>();
    numbers.windows(3).find_map(|window| match window {
        [year @ 1900..=2100, month @ 1..=12, day @ 1..=31] => {
            Some(format!("{year:04}-{month:02}-{day:02}"))
        }
        _ => None,
    })
}

fn section_outline(text: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .map(clean_sentence)
        .map(|heading| heading.trim_end_matches('.').to_string())
        .filter(|heading| heading.len() >= 4 && seen.insert(normalize_sentence_key(heading)))
        .take(limit)
        .collect()
}
