use super::*;

fn semantic_token_boosts(
    matrix: &SemanticMatrix,
    query_tokens: &[usize],
    top_k: usize,
) -> HashMap<usize, f64> {
    let config = RetrievalConfig {
        top_k: top_k.max(10) * 8,
        min_score: 0.05,
        projection_k: top_k.max(32),
        recency_weight: 0.0,
    };

    let mut boosts = HashMap::new();
    let related = match matrix.retrieve_related_cached(query_tokens, &config) {
        Ok(results) => results,
        Err(_) => return boosts,
    };

    for pair in related {
        for token_id in [pair.token_a, pair.token_b] {
            if query_tokens.contains(&token_id) {
                continue;
            }

            boosts
                .entry(token_id)
                .and_modify(|score| *score = score.max(pair.relevance_score))
                .or_insert(pair.relevance_score);
        }
    }

    boosts
}

impl MementoManager {
    pub async fn query(&self, req: &QueryRequest) -> Result<QueryResponse> {
        let state = self.state.read().await;
        let query_mode = detect_query_mode(&req.query);
        let recall_intent = has_recall_intent(&req.query);
        let query_date = parse_date_constraint(&req.query);
        let has_explicit_date = query_date.is_some();
        let mut ranking_query_terms = tokenize_folded_text(&req.query)
            .into_iter()
            .filter(|term| term.len() >= 2 && !is_low_signal_query_term(term))
            .collect::<Vec<_>>();
        ranking_query_terms.sort_unstable();
        ranking_query_terms.dedup();

        let mut query_token_terms = tokenize_text(&req.query);
        query_token_terms.extend(tokenize_folded_text(&req.query));
        query_token_terms.sort_unstable();
        query_token_terms.dedup();

        let mut query_tokens: Vec<usize> = query_token_terms
            .into_iter()
            .filter_map(|token| state.vocabulary.get(&token).copied())
            .collect();
        query_tokens.sort_unstable();
        query_tokens.dedup();

        if query_tokens.is_empty() && ranking_query_terms.is_empty() {
            return Ok(QueryResponse {
                answer: "I could not match this query to any learned memory yet.".to_string(),
                results: Vec::new(),
                confidence: 0.0,
                query_tokens: 0,
                key_concepts: Vec::new(),
            });
        }

        let matrix_confidence = if query_tokens.is_empty() {
            0.0
        } else {
            compute_query_confidence_cached(&state.matrix, &query_tokens).unwrap_or(0.0)
        };
        let semantic_boosts = if query_tokens.is_empty() {
            HashMap::new()
        } else {
            semantic_token_boosts(&state.matrix, &query_tokens, req.top_k)
        };
        let documents = state
            .documents
            .iter()
            .map(|document| (document.doc_id, document))
            .collect::<HashMap<_, _>>();
        let query_projection = state.matrix.cached_eigen().map(|eigen| {
            let dims = eigen.eigenvectors.ncols();
            project_tokens_to_eigenspace(&query_tokens, &eigen.eigenvectors, dims)
                .iter()
                .copied()
                .collect::<Vec<_>>()
        });
        let metadata_term_weights = state
            .lexical_index
            .metadata_term_weights(&ranking_query_terms);
        let lexical_query_groups = ranking_query_terms
            .iter()
            .filter_map(|term| {
                let mut token_ids = lexical_query_alternatives(term)
                    .into_iter()
                    .filter_map(|alternative| state.vocabulary.get(&alternative).copied())
                    .collect::<Vec<_>>();
                token_ids.sort_unstable();
                token_ids.dedup();
                (!token_ids.is_empty()).then_some(token_ids)
            })
            .collect::<Vec<_>>();
        let max_document_ordinal = state.lexical_index.latest_recency_ordinal();
        // The postings stage already combines BM25, IDF coverage, metadata and
        // semantic expansion. A bounded rerank pool keeps expensive structural
        // features proportional to the request rather than the vault size.
        let candidate_limit = req.top_k.max(10) * 32;
        let mut candidate_chunks = state.lexical_index.candidates(
            &lexical_query_groups,
            &ranking_query_terms,
            &semantic_boosts,
            candidate_limit,
        );
        if state.document_graph.edge_count() > 0 {
            let mut seed_scores = HashMap::<DocId, f64>::new();
            for candidate in candidate_chunks.iter().take(64) {
                let Some(doc_id) = state
                    .lexical_index
                    .document_id_for_chunk(candidate.chunk_idx)
                else {
                    continue;
                };
                let score = (candidate.lexical_score * 0.75)
                    + (candidate.semantic_score * 0.15)
                    + (candidate.query_coverage * 0.10);
                seed_scores
                    .entry(doc_id)
                    .and_modify(|current| *current = current.max(score))
                    .or_insert(score);
            }
            let seeds = seed_scores.into_iter().collect::<Vec<_>>();
            let graph_scores = state.document_graph.spread(&seeds, req.top_k.max(10) * 4);
            state.lexical_index.apply_graph_scores(
                &mut candidate_chunks,
                &graph_scores,
                req.top_k.max(10) * 2,
            );
        }

        let mut chunk_rankings: Vec<ChunkRanking> = candidate_chunks
            .into_iter()
            .filter_map(|candidate| {
                let i = candidate.chunk_idx;
                let lexical_score = candidate.lexical_score;
                let semantic_score = candidate.semantic_score;
                let graph_score = candidate.graph_score;
                let chunk = &state.chunks[i];
                let metadata_score = metadata_overlap_score(
                    &ranking_query_terms,
                    chunk,
                    &documents,
                    &metadata_term_weights,
                );
                let metadata_bonus =
                    metadata_exact_match_bonus(&ranking_query_terms, chunk, &documents);
                let exactness_score = metadata_exactness_score(&req.query, chunk, &documents);
                let entity_score = entity_lookup_score(&req.query, chunk, &documents);
                let query_coverage_score = candidate.query_coverage;
                let compactness_score = source_compactness_score(
                    state.lexical_index.source_chunk_count(&chunk.source_path),
                );
                let embedding_score = query_projection
                    .as_ref()
                    .and_then(|projection| {
                        state
                            .chunk_embeddings
                            .get(&chunk.chunk_id)
                            .map(|embedding| {
                                let lhs = projection
                                    .iter()
                                    .map(|value| *value as f32)
                                    .collect::<Vec<_>>();
                                cosine_similarity_f32(&lhs, embedding).max(0.0) as f64
                            })
                    })
                    .unwrap_or(0.0);
                let score = match query_mode {
                    QueryMode::DocumentLookup => {
                        (lexical_score * 0.22)
                            + (metadata_score * 0.16)
                            + (exactness_score * 0.18)
                            + (entity_score * 0.24)
                            + (query_coverage_score * 0.05)
                            + (semantic_score * 0.05)
                            + (embedding_score * 0.05)
                            + (compactness_score * 0.03)
                            + (graph_score * 0.06)
                            + metadata_bonus
                    }
                    QueryMode::EpisodicRecall => {
                        (lexical_score * 0.40)
                            + (semantic_score * 0.15)
                            + (metadata_score * 0.15)
                            + (exactness_score * 0.10)
                            + (embedding_score * 0.10)
                            + (query_coverage_score * 0.08)
                            + (compactness_score * 0.05)
                            + (graph_score * 0.06)
                            + metadata_bonus
                    }
                    QueryMode::ConceptSearch => {
                        (lexical_score * 0.34)
                            + (semantic_score * 0.18)
                            + (metadata_score * 0.15)
                            + (exactness_score * 0.08)
                            + (embedding_score * 0.14)
                            + (query_coverage_score * 0.08)
                            + (compactness_score * 0.07)
                            + (graph_score * 0.06)
                            + metadata_bonus
                    }
                };

                if score > 0.0 || metadata_score > 0.0 {
                    Some(ChunkRanking {
                        idx: i,
                        doc_id: chunk.doc_id,
                        source_path: chunk.source_path.clone(),
                        score: score.min(1.0),
                        metadata_score,
                        metadata_bonus,
                        exactness_score,
                        entity_score,
                        query_coverage_score,
                        graph_score,
                    })
                } else {
                    None
                }
            })
            .collect();

        chunk_rankings.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut by_document = BTreeMap::<String, Vec<ChunkRanking>>::new();
        for ranking in chunk_rankings {
            by_document
                .entry(ranking.source_path.clone())
                .or_default()
                .push(ranking);
        }

        let mut document_rankings: Vec<(String, DocId, f64, usize)> = by_document
            .iter_mut()
            .map(|(source_path, rankings)| {
                rankings.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let best = rankings.first().map(|ranking| ranking.score).unwrap_or(0.0);
                let second = rankings.get(1).map(|ranking| ranking.score).unwrap_or(0.0);
                let metadata = rankings
                    .iter()
                    .map(|ranking| ranking.metadata_score)
                    .fold(0.0, f64::max);
                let exact_bonus = rankings
                    .iter()
                    .map(|ranking| ranking.metadata_bonus)
                    .fold(0.0, f64::max);
                let exactness = rankings
                    .iter()
                    .map(|ranking| ranking.exactness_score)
                    .fold(0.0, f64::max);
                let entity = rankings
                    .iter()
                    .map(|ranking| ranking.entity_score)
                    .fold(0.0, f64::max);
                let query_coverage = rankings
                    .iter()
                    .map(|ranking| ranking.query_coverage_score)
                    .fold(0.0, f64::max);
                let graph = rankings
                    .iter()
                    .map(|ranking| ranking.graph_score)
                    .fold(0.0, f64::max);
                let doc_id = rankings.first().map(|ranking| ranking.doc_id).unwrap_or(0);
                let best_chunk = rankings.first().map(|ranking| &state.chunks[ranking.idx]);
                let temporal = best_chunk
                    .map(|chunk| temporal_match_score(query_date, chunk, &documents))
                    .unwrap_or(0.0);
                let temporal_identity = if has_explicit_date {
                    temporal_identity_score(temporal, metadata, exactness, entity)
                } else {
                    0.0
                };
                let episodic = best_chunk
                    .map(|chunk| episodic_memory_score(chunk, &documents))
                    .unwrap_or(0.0);
                let session_note = best_chunk
                    .map(|chunk| session_note_score(chunk, &documents))
                    .unwrap_or(0.0);
                let aggregate = best_chunk
                    .map(|chunk| aggregate_memory_score(chunk, &documents))
                    .unwrap_or(0.0);
                let memory_class = best_chunk
                    .map(|chunk| classify_memory(chunk, &documents, &state.classification_rules))
                    .unwrap_or(MemoryClass::Other);
                let evergreen = best_chunk
                    .map(|chunk| evergreen_memory_score(chunk, &documents))
                    .unwrap_or(0.0);
                let document_scores = state.lexical_index.document_scores(
                    doc_id,
                    &lexical_query_groups,
                    &ranking_query_terms,
                );
                let lexical_document = document_scores.lexical_score;
                let document_query_coverage = document_scores.coverage_score;
                let specificity_bonus = document_scores.specificity_bonus;
                let score = match query_mode {
                    QueryMode::DocumentLookup => {
                        (best * 0.35)
                            + (second * 0.05)
                            + (metadata * 0.10)
                            + (exact_bonus * 0.08)
                            + (exactness * 0.18)
                            + (entity * 0.24)
                            + (lexical_document * 0.12)
                            + (document_query_coverage * 0.08)
                            + (graph * 0.10)
                            + specificity_bonus
                    }
                    QueryMode::EpisodicRecall => {
                        (best * 0.50)
                            + (second * 0.1)
                            + (metadata * 0.1)
                            + (exact_bonus * 0.08)
                            + (exactness * 0.12)
                            + (query_coverage * 0.05)
                            + (lexical_document * 0.15)
                            + (document_query_coverage * 0.10)
                            + (graph * 0.10)
                            + specificity_bonus
                    }
                    QueryMode::ConceptSearch => {
                        (best * 0.45)
                            + (second * 0.1)
                            + (metadata * 0.1)
                            + (exact_bonus * 0.08)
                            + (exactness * 0.10)
                            + (entity * 0.07)
                            + (query_coverage * 0.05)
                            + (lexical_document * 0.15)
                            + (document_query_coverage * 0.06)
                            + (graph * 0.10)
                            + specificity_bonus
                    }
                };
                let embedding = query_projection
                    .as_ref()
                    .and_then(|projection| {
                        let doc_embedding = state.document_embeddings.get(&doc_id)?;
                        let lhs = projection
                            .iter()
                            .map(|value| *value as f32)
                            .collect::<Vec<_>>();
                        Some(cosine_similarity_f32(&lhs, doc_embedding).max(0.0) as f64)
                    })
                    .unwrap_or(0.0);
                let recall_bias = if recall_intent {
                    (episodic * 0.14) + (session_note * 0.12)
                        - (aggregate * 0.12)
                        - (evergreen * 0.10)
                } else {
                    0.0
                };
                let class_bias =
                    memory_class_score(&ranking_query_terms, recall_intent, memory_class);
                let freshness_bias = best_chunk
                    .map(|chunk| {
                        contextual_freshness_score(
                            query_mode,
                            lexical_document,
                            document_query_coverage,
                            chunk,
                            &documents,
                            max_document_ordinal,
                        )
                    })
                    .unwrap_or(0.0);
                let mode_bias = match query_mode {
                    QueryMode::DocumentLookup => {
                        (embedding * 0.06)
                            + (temporal * if has_explicit_date { 0.40 } else { 0.02 })
                            + temporal_identity
                            + class_bias.max(0.0)
                            + freshness_bias
                    }
                    QueryMode::EpisodicRecall => {
                        (embedding * 0.15)
                            + (temporal * if has_explicit_date { 0.40 } else { 0.12 })
                            + temporal_identity
                            + recall_bias
                            + class_bias
                            + freshness_bias
                    }
                    QueryMode::ConceptSearch => {
                        (embedding * 0.18)
                            + (temporal * if has_explicit_date { 0.40 } else { 0.05 })
                            + temporal_identity
                            + class_bias
                            + freshness_bias
                    }
                };
                let score = score + mode_bias;
                let best_idx = rankings.first().map(|ranking| ranking.idx).unwrap_or(0);
                (source_path.clone(), doc_id, score, best_idx)
            })
            .collect();
        document_rankings
            .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        let top_query_coverage = document_rankings
            .first()
            .map(|(_, doc_id, _, _)| {
                state
                    .lexical_index
                    .document_scores(*doc_id, &lexical_query_groups, &ranking_query_terms)
                    .coverage_score
            })
            .unwrap_or(0.0);
        let confidence = retrieval_confidence_score(
            document_rankings.first().map(|ranking| ranking.2),
            document_rankings.get(1).map(|ranking| ranking.2),
            top_query_coverage,
            matrix_confidence,
        );

        let result_bundles = build_result_bundles(
            &document_rankings,
            &by_document,
            &state.chunks,
            &state.lexical_index,
            req.top_k,
        );

        let results: Vec<QueryResult> = result_bundles
            .iter()
            .filter_map(|bundle| {
                let first_idx = *bundle.chunk_indices.first()?;
                let first_chunk = &state.chunks[first_idx];
                Some(QueryResult {
                    content: bundle_content(bundle, &state.chunks, &documents),
                    score: bundle.score.clamp(0.0, 1.0),
                    source_path: bundle.source_path.clone(),
                    chunk_index: first_chunk.chunk_index,
                })
            })
            .collect();

        let evidence = build_evidence(&results);
        let generator = Generator::new();
        let (answer, key_concepts) = if evidence.is_empty() {
            (
                "No relevant memories were found for this query.".to_string(),
                Vec::new(),
            )
        } else {
            match generator.generate_with_semantics(
                &req.query,
                &query_tokens,
                &evidence,
                &state.matrix,
                confidence,
            ) {
                Ok(generated) => (generated.text, generated.concepts),
                Err(_) => (generator.generate_answer(&evidence, confidence), Vec::new()),
            }
        };

        Ok(QueryResponse {
            answer,
            results,
            confidence,
            query_tokens: query_tokens.len(),
            key_concepts,
        })
    }
}
