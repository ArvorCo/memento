use super::*;

pub(super) fn cosine_similarity_f32(lhs: &[f32], rhs: &[f32]) -> f32 {
    if lhs.is_empty() || rhs.is_empty() || lhs.len() != rhs.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut lhs_norm = 0.0f32;
    let mut rhs_norm = 0.0f32;
    for (a, b) in lhs.iter().zip(rhs.iter()) {
        dot += a * b;
        lhs_norm += a * a;
        rhs_norm += b * b;
    }

    if lhs_norm <= f32::EPSILON || rhs_norm <= f32::EPSILON {
        return 0.0;
    }

    (dot / (lhs_norm.sqrt() * rhs_norm.sqrt())).clamp(-1.0, 1.0)
}

pub(super) fn rebuild_state_from_chunks(state: &mut EngineState) {
    state.matrix = SemanticMatrix::new(10_000);
    state.vocabulary.clear();
    state.next_token_id = 0;
    state.coherence_score = 0.0;
    state.chunk_embeddings.clear();
    state.document_embeddings.clear();
    let documents = state
        .documents
        .iter()
        .map(|document| (document.doc_id, document))
        .collect::<HashMap<_, _>>();

    for chunk in &mut state.chunks {
        let token_ids: Vec<usize> = tokenize_text(chunk.resolve_content(&documents))
            .into_iter()
            .map(|token| {
                *state.vocabulary.entry(token).or_insert_with(|| {
                    let token_id = state.next_token_id;
                    state.next_token_id += 1;
                    token_id
                })
            })
            .collect();

        if token_ids.len() >= 2 {
            let _ = state.matrix.ingest_document(&token_ids);
        }

        chunk.token_count = token_ids.len();
        chunk.token_ids = token_ids;
        chunk.content.clear();
    }
    state.lexical_index = LexicalIndex::build(&state.chunks, &state.documents);
    state.document_graph = DocumentGraph::build(&state.documents);
}

pub(super) fn build_graph_segment(
    state: &EngineState,
    triplets: &[(usize, usize, f64)],
) -> GraphSegmentFile {
    let doc_chunk_edges = state
        .chunks
        .iter()
        .map(|chunk| DocChunkEdge {
            doc_id: chunk.doc_id,
            chunk_id: chunk.chunk_id,
        })
        .collect();

    let chunk_token_adjacency = state
        .chunks
        .iter()
        .map(|chunk| {
            let mut token_ids = chunk.token_ids.clone();
            token_ids.sort_unstable();
            token_ids.dedup();
            ChunkTokenAdjacency {
                chunk_id: chunk.chunk_id,
                token_ids,
            }
        })
        .collect();

    let token_graph_edges = triplets
        .iter()
        .filter(|(a, b, weight)| a != b && *weight > 0.0)
        .map(|(token_a, token_b, weight)| TokenGraphEdge {
            token_a: *token_a,
            token_b: *token_b,
            weight: *weight,
        })
        .collect();

    GraphSegmentFile {
        domain: state.domain.clone(),
        doc_chunk_edges,
        chunk_token_adjacency,
        token_graph_edges,
    }
}

fn quantize_projection(values: &[f64]) -> Vec<i16> {
    values
        .iter()
        .map(|value| {
            let scaled = (value.clamp(-1.0, 1.0) * i16::MAX as f64).round();
            scaled as i16
        })
        .collect()
}

fn dequantize_embedding(values: &[i16], quantization_max: i16) -> Vec<f32> {
    let scale = quantization_max.max(1) as f32;
    values.iter().map(|value| *value as f32 / scale).collect()
}

fn aggregate_document_embeddings(
    chunks: &[StoredChunk],
    chunk_embeddings: &HashMap<u64, Vec<f32>>,
) -> HashMap<DocId, Vec<f32>> {
    let mut sums = HashMap::<DocId, Vec<f32>>::new();
    let mut counts = HashMap::<DocId, usize>::new();

    for chunk in chunks {
        let Some(embedding) = chunk_embeddings.get(&chunk.chunk_id) else {
            continue;
        };
        let entry = sums
            .entry(chunk.doc_id)
            .or_insert_with(|| vec![0.0; embedding.len()]);
        for (idx, value) in embedding.iter().enumerate() {
            entry[idx] += *value;
        }
        *counts.entry(chunk.doc_id).or_insert(0) += 1;
    }

    for (doc_id, values) in &mut sums {
        let count = counts.get(doc_id).copied().unwrap_or(1) as f32;
        for value in values.iter_mut() {
            *value /= count;
        }
    }

    sums
}

pub(super) fn embedding_state_from_segment(
    chunks: &[StoredChunk],
    segment: Option<&EmbeddingSegmentFile>,
) -> (HashMap<u64, Vec<f32>>, HashMap<DocId, Vec<f32>>) {
    let Some(segment) = segment else {
        return (HashMap::new(), HashMap::new());
    };

    let chunk_embeddings = segment
        .embeddings
        .iter()
        .map(|embedding| {
            (
                embedding.chunk_id,
                dequantize_embedding(&embedding.vector, segment.quantization_max),
            )
        })
        .collect::<HashMap<_, _>>();
    let document_embeddings = aggregate_document_embeddings(chunks, &chunk_embeddings);
    (chunk_embeddings, document_embeddings)
}

pub(super) fn build_embedding_segment(state: &EngineState) -> Option<EmbeddingSegmentFile> {
    build_embedding_segment_from(&state.chunks, &state.domain, state.matrix.cached_eigen()?)
}

pub(super) fn build_embedding_segment_from(
    chunks: &[StoredChunk],
    domain: &str,
    eigen: &libmemento::matrix::EigenDecomposition,
) -> Option<EmbeddingSegmentFile> {
    let dimensions = eigen.eigenvectors.ncols();
    if dimensions == 0 {
        return None;
    }

    let embeddings = chunks
        .iter()
        .filter(|chunk| !chunk.token_ids.is_empty())
        .map(|chunk| {
            let projection =
                project_tokens_to_eigenspace(&chunk.token_ids, &eigen.eigenvectors, dimensions);
            QuantizedChunkEmbedding {
                chunk_id: chunk.chunk_id,
                vector: quantize_projection(projection.as_slice()),
            }
        })
        .collect();

    Some(EmbeddingSegmentFile {
        domain: domain.to_string(),
        dimensions,
        quantization_max: i16::MAX,
        embeddings,
    })
}

pub(super) fn engine_state_from_memento(mf: MementoFile) -> EngineState {
    let cached_eigen = mf.cached_eigen();
    let mut matrix = mf.to_matrix();
    if let Some(eigen) = cached_eigen {
        matrix.restore_cached_eigen(eigen);
    }

    let chunks = mf.chunks;
    let lexical_index = LexicalIndex::build(&chunks, &mf.documents);
    let document_graph = DocumentGraph::build(&mf.documents);
    let (chunk_embeddings, document_embeddings) = embedding_state_from_segment(&chunks, None);
    EngineState {
        matrix,
        vocabulary: mf.vocabulary,
        next_token_id: mf.next_token_id,
        documents: mf.documents,
        next_doc_id: mf.next_doc_id,
        next_chunk_id: mf.next_chunk_id,
        chunks,
        lexical_index,
        document_graph,
        chunk_embeddings,
        document_embeddings,
        sources: mf.sources,
        domain: mf.domain,
        coherence_score: mf.coherence_score,
        classification_rules: default_classification_rules(),
    }
}

pub(super) fn engine_state_from_recovery_snapshot(
    snapshot: RecoverySnapshot,
    classification_rules: ClassificationRulesConfig,
) -> EngineState {
    let mut state = EngineState {
        matrix: SemanticMatrix::new(10_000),
        vocabulary: HashMap::new(),
        next_token_id: 0,
        documents: snapshot.documents,
        next_doc_id: snapshot.next_doc_id,
        next_chunk_id: snapshot.next_chunk_id,
        chunks: snapshot.chunks,
        lexical_index: LexicalIndex::default(),
        document_graph: DocumentGraph::default(),
        chunk_embeddings: HashMap::new(),
        document_embeddings: HashMap::new(),
        sources: snapshot.sources,
        domain: snapshot.domain,
        coherence_score: 0.0,
        classification_rules,
    };
    rebuild_state_from_chunks(&mut state);
    state
}

pub(super) fn memento_from_runtime_segments(
    lexical: LexicalSegmentFile,
    metadata: MetadataSegmentFile,
    eigen: Option<EigenSegmentFile>,
) -> MementoFile {
    MementoFile {
        id: Uuid::new_v4(),
        domain: metadata.domain,
        created_at: SystemTime::now(),
        updated_at: SystemTime::now(),
        vocabulary: lexical.vocabulary,
        next_token_id: lexical.next_token_id,
        vocabulary_size: lexical.vocabulary_size,
        triplets: lexical.triplets,
        coherence_score: lexical.coherence_score,
        confidence_history: lexical.confidence_history,
        eigenvectors: eigen.as_ref().map(|segment| segment.eigenvectors.clone()),
        eigenvalues: eigen.map(|segment| segment.eigenvalues),
        sources: metadata.sources,
        query_count: 0,
        update_count: 0,
        documents: metadata.documents,
        next_doc_id: metadata.next_doc_id,
        next_chunk_id: metadata.next_chunk_id,
        chunks: metadata.chunks,
    }
}
