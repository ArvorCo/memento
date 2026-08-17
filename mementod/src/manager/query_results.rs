use super::*;

pub(super) fn build_evidence(results: &[QueryResult]) -> Vec<EvidenceChunk> {
    results
        .iter()
        .take(5)
        .map(|result| EvidenceChunk {
            chunk_id: format!("chunk-{}-{}", result.source_path, result.chunk_index),
            source_document_id: result.source_path.clone(),
            text: result.content.clone(),
            retrieval_score: result.score,
            relevance_score: result.score,
        })
        .collect()
}

fn should_attach_supporting_chunk(
    primary: &ChunkRanking,
    candidate: &ChunkRanking,
    chunks: &[StoredChunk],
) -> bool {
    if primary.idx == candidate.idx || primary.source_path != candidate.source_path {
        return false;
    }

    let primary_chunk = &chunks[primary.idx];
    let candidate_chunk = &chunks[candidate.idx];
    let adjacent = primary_chunk
        .chunk_index
        .abs_diff(candidate_chunk.chunk_index)
        <= 1;
    let strong_support = candidate.query_coverage_score >= 0.34
        || candidate.exactness_score >= 0.40
        || candidate.entity_score >= 0.50
        || candidate.metadata_score >= 0.60;

    adjacent || strong_support
}

pub(super) fn build_result_bundles(
    document_rankings: &[(String, DocId, f64, usize)],
    by_document: &BTreeMap<String, Vec<ChunkRanking>>,
    chunks: &[StoredChunk],
    lexical_index: &LexicalIndex,
    top_k: usize,
) -> Vec<ResultBundle> {
    let mut bundles = Vec::new();
    for (document_rank, (source_path, doc_id, doc_score, best_idx)) in
        document_rankings.iter().enumerate()
    {
        let Some(rankings) = by_document.get(source_path) else {
            continue;
        };

        let Some(primary) = rankings.iter().find(|ranking| ranking.idx == *best_idx) else {
            continue;
        };

        let mut chunk_indices = vec![primary.idx];
        let max_chunks = match document_rank {
            0 => 6,
            1..=2 => 4,
            _ => 2,
        };
        chunk_indices.extend(
            rankings
                .iter()
                .filter(|candidate| should_attach_supporting_chunk(primary, candidate, chunks))
                .take(max_chunks - 1)
                .map(|candidate| candidate.idx),
        );

        // Passage ranking finds the sharpest excerpt; questions about a plan,
        // sprint or catalog also need representative coverage of the canonical
        // document. Fill the bounded bundle from the existing doc→chunk map,
        // sampling across the document instead of scanning the vault.
        let document_chunks = lexical_index.chunk_indices_for_document(*doc_id);
        if chunk_indices.len() < max_chunks && !document_chunks.is_empty() {
            let sample_count = max_chunks.min(document_chunks.len());
            for sample in 0..sample_count {
                let position = if sample_count == 1 {
                    0
                } else {
                    sample * (document_chunks.len() - 1) / (sample_count - 1)
                };
                let idx = document_chunks[position];
                if !chunk_indices.contains(&idx) {
                    chunk_indices.push(idx);
                    if chunk_indices.len() == max_chunks {
                        break;
                    }
                }
            }
        }
        if chunk_indices.len() < max_chunks {
            for idx in document_chunks {
                if !chunk_indices.contains(idx) {
                    chunk_indices.push(*idx);
                    if chunk_indices.len() == max_chunks {
                        break;
                    }
                }
            }
        }

        chunk_indices.sort_by_key(|idx| chunks[*idx].chunk_index);
        chunk_indices.dedup();

        bundles.push(ResultBundle {
            source_path: source_path.clone(),
            chunk_indices,
            score: *doc_score,
        });

        if bundles.len() == top_k {
            break;
        }
    }
    bundles
}

pub(super) fn bundle_content(
    bundle: &ResultBundle,
    chunks: &[StoredChunk],
    documents: &HashMap<DocId, &StoredDocument>,
) -> String {
    let content = bundle
        .chunk_indices
        .iter()
        .map(|idx| chunks[*idx].resolve_content(documents).to_string())
        .collect::<Vec<_>>()
        .join("\n\n");
    clean_retrieval_text(&content)
}

fn clean_retrieval_text(content: &str) -> String {
    let mut lines = Vec::new();
    let mut in_frontmatter = content.trim_start().starts_with("---\n");
    let mut frontmatter_opening_seen = false;
    let mut in_memento_navigation = false;
    let mut previous_blank = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if in_frontmatter {
            if trimmed == "---" {
                if frontmatter_opening_seen {
                    in_frontmatter = false;
                } else {
                    frontmatter_opening_seen = true;
                }
            }
            continue;
        }
        if trimmed == "<!-- memento:nav:start -->" {
            in_memento_navigation = true;
            continue;
        }
        if trimmed == "<!-- memento:nav:end -->" {
            in_memento_navigation = false;
            continue;
        }
        if in_memento_navigation
            || matches!(
                trimmed,
                "<!-- memento:hub:start -->" | "<!-- memento:hub:end -->"
            )
        {
            continue;
        }
        let blank = trimmed.is_empty();
        if blank && previous_blank {
            continue;
        }
        lines.push(line.trim_end().to_string());
        previous_blank = blank;
    }

    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod clean_tests {
    use super::clean_retrieval_text;

    #[test]
    fn retrieval_text_drops_machine_frontmatter_and_navigation() {
        let content = r#"---
title: "Atlas"
memento_source: "database://decisions/1"
tags: ["database", "orbital-memory"]
---

<!-- memento:nav:start -->
> **Memento:** [[_memento|Hub]]
<!-- memento:nav:end -->

# Atlas

Use cobalt hummingbird because it survives offline edits.
"#;

        let cleaned = clean_retrieval_text(content);

        assert_eq!(
            cleaned,
            "# Atlas\n\nUse cobalt hummingbird because it survives offline edits."
        );
        assert!(!cleaned.contains("memento_source"));
        assert!(!cleaned.contains("[[_memento"));
    }
}
