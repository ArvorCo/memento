use crate::memory_classification::{exact_metadata_text, source_profile};
use crate::text_utils::{
    ascii_fold, parse_date_tokens, query_exactness_terms, tokenize_folded_text, tokenize_text,
    QueryMode,
};
use libmemento::format::{DocId, StoredChunk, StoredDocument};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn metadata_terms_for_chunk(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> Vec<String> {
    let mut terms = tokenize_text(&chunk.source_path);
    if let Some(section_title) = &chunk.section_title {
        terms.extend(tokenize_text(section_title));
    }
    if let Some(document) = documents.get(&chunk.doc_id) {
        if let Some(title) = &document.title {
            terms.extend(tokenize_text(title));
        }
    }
    terms.sort_unstable();
    terms.dedup();
    terms
}

pub(crate) fn metadata_overlap_score(
    query_terms: &[String],
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
    term_weights: &HashMap<String, f64>,
) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let metadata_terms = metadata_terms_for_chunk(chunk, documents);
    if metadata_terms.is_empty() {
        return 0.0;
    }

    let mut matched_weight = 0.0;
    let mut total_weight = 0.0;
    for term in query_terms {
        let base_weight = if term.chars().all(|char| char.is_ascii_digit()) {
            2.5
        } else {
            1.0
        };
        let weight = base_weight * term_weights.get(term).copied().unwrap_or(1.0);
        total_weight += weight;
        if metadata_terms.contains(term) {
            matched_weight += weight;
        }
    }

    if total_weight <= f64::EPSILON {
        0.0
    } else {
        matched_weight / total_weight
    }
}

pub(crate) fn source_compactness_score(source_chunk_count: usize) -> f64 {
    if source_chunk_count == 0 {
        return 0.0;
    }
    (1.0 / (source_chunk_count as f64).sqrt()).clamp(0.0, 1.0)
}

pub(crate) fn retrieval_confidence_score(
    top_score: Option<f64>,
    second_score: Option<f64>,
    query_coverage: f64,
    matrix_confidence: f64,
) -> f64 {
    let Some(top_score) = top_score else {
        return 0.0;
    };
    let absolute_strength = top_score.clamp(0.0, 1.0);
    let margin = second_score
        .map(|second| ((top_score - second).max(0.0) / top_score.max(0.1)).clamp(0.0, 1.0))
        .unwrap_or(1.0);

    ((absolute_strength * 0.45)
        + (query_coverage.clamp(0.0, 1.0) * 0.35)
        + (margin * 0.15)
        + (matrix_confidence.clamp(0.0, 1.0) * 0.05))
        .clamp(0.0, 1.0)
}

pub(crate) fn metadata_exact_match_bonus(
    query_terms: &[String],
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let metadata_terms = metadata_terms_for_chunk(chunk, documents);
    if query_terms.iter().all(|term| metadata_terms.contains(term)) {
        0.35
    } else {
        0.0
    }
}

pub(crate) fn metadata_exactness_score(
    query: &str,
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let query_folded = ascii_fold(query);
    let query_terms = query_exactness_terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }

    let metadata_text = exact_metadata_text(chunk, documents);
    if metadata_text.is_empty() {
        return 0.0;
    }

    if metadata_text.contains(query_folded.trim()) {
        return 1.0;
    }

    let metadata_terms = tokenize_folded_text(&metadata_text);
    if metadata_terms.is_empty() {
        return 0.0;
    }

    let matched_terms = query_terms
        .iter()
        .filter(|term| metadata_terms.contains(term))
        .count();
    let coverage = matched_terms as f64 / query_terms.len() as f64;

    let path_stem = Path::new(&chunk.source_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ascii_fold)
        .unwrap_or_default();
    let filename_bonus = if !path_stem.is_empty()
        && query_terms
            .iter()
            .all(|term| path_stem.contains(term) || metadata_text.contains(term))
    {
        0.25
    } else {
        0.0
    };

    (coverage + filename_bonus).clamp(0.0, 1.0)
}

pub(crate) fn recency_ordinal(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> Option<i64> {
    let (_, date) = source_profile(chunk, documents);
    date.map(|(year, month, day)| (year as i64 * 372) + (month as i64 * 31) + day as i64)
}

pub(crate) fn contextual_freshness_score(
    query_mode: QueryMode,
    lexical_document: f64,
    query_coverage: f64,
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
    max_ordinal: Option<i64>,
) -> f64 {
    let Some(max_ordinal) = max_ordinal else {
        return 0.0;
    };
    let Some(ordinal) = recency_ordinal(chunk, documents) else {
        return 0.0;
    };
    if lexical_document < 0.45 && query_coverage < 0.50 {
        return 0.0;
    }

    let distance = (max_ordinal - ordinal).max(0) as f64;
    let normalized = (1.0 / (1.0 + distance / 7.0)).clamp(0.0, 1.0);

    match query_mode {
        QueryMode::DocumentLookup => normalized * 0.03,
        QueryMode::EpisodicRecall => normalized * 0.12,
        QueryMode::ConceptSearch => normalized * 0.04,
    }
}

pub(crate) fn temporal_match_score(
    query: &str,
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let Some(query_date) = parse_date_tokens(query) else {
        return 0.0;
    };
    let (_, source_date) = source_profile(chunk, documents);
    f64::from(source_date == Some(query_date))
}

pub(crate) fn episodic_memory_score(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let (profile, date) = source_profile(chunk, documents);
    let mut score: f64 = if date.is_some() { 0.7 } else { 0.0 };
    if [
        "daily", "review", "report", "council", "log", "journal", "notes",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        score += 0.3;
    }
    score.clamp(0.0, 1.0)
}

pub(crate) fn aggregate_memory_score(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let (profile, _) = source_profile(chunk, documents);
    if [
        "summary", "overview", "digest", "intel", "weekly", "launch", "recap", "review",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        1.0
    } else {
        0.0
    }
}

pub(crate) fn session_note_score(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let (profile, date) = source_profile(chunk, documents);
    if date.is_none() {
        return 0.0;
    }

    if [
        "summary", "overview", "digest", "intel", "weekly", "launch", "recap", "review",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        0.0
    } else {
        1.0
    }
}

pub(crate) fn evergreen_memory_score(
    chunk: &StoredChunk,
    documents: &HashMap<DocId, &StoredDocument>,
) -> f64 {
    let (profile, _) = source_profile(chunk, documents);
    if [
        "guide",
        "protocol",
        "fundamentals",
        "strategy",
        "playbook",
        "template",
        "blueprint",
    ]
    .iter()
    .any(|term| profile.contains(term))
    {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dated_chunk(path: &str) -> StoredChunk {
        StoredChunk {
            chunk_id: 1,
            doc_id: 1,
            span: None,
            content: String::new(),
            chunk_index: 0,
            token_count: 0,
            source_path: path.to_string(),
            section_title: None,
            chunk_type: "document".to_string(),
            token_ids: Vec::new(),
        }
    }

    #[test]
    fn temporal_match_requires_the_complete_date() {
        let documents = HashMap::new();
        let exact = dated_chunk("memory/2026-04-05-ceo-sprint.md");
        let different_day = dated_chunk("memory/2026-05-04-ceo-sprint.md");
        let query = "what happened in the sprint on 2026-04-05?";

        assert_eq!(temporal_match_score(query, &exact, &documents), 1.0);
        assert_eq!(temporal_match_score(query, &different_day, &documents), 0.0);
    }

    #[test]
    fn retrieval_confidence_rewards_strength_coverage_and_margin() {
        let strong = retrieval_confidence_score(Some(0.95), Some(0.55), 0.9, 0.6);
        let ambiguous = retrieval_confidence_score(Some(0.95), Some(0.94), 0.5, 0.6);
        let weak = retrieval_confidence_score(Some(0.25), Some(0.20), 0.2, 0.2);

        assert!(strong > 0.8, "{strong}");
        assert!(ambiguous < strong, "{ambiguous} >= {strong}");
        assert!(weak < 0.3, "{weak}");
        assert_eq!(retrieval_confidence_score(None, None, 1.0, 1.0), 0.0);
    }
}
