//! MementoManager — manages .memento files and the semantic matrix
mod document_access;
mod document_graph;
mod ingest;
mod lexical_index;
mod persistence;
mod query_pipeline;
mod query_results;
mod runtime_state;
mod source_sync;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_runtime;

use self::document_graph::DocumentGraph;
use self::ingest::{
    chunker_profile_for_file_size, prepare_document_from_grouped_chunks,
    prepare_documents_from_chunks, PreparedDocument,
};
use self::lexical_index::{metadata_terms as lexical_metadata_terms, LexicalIndex};
use self::query_results::{build_evidence, build_result_bundles, bundle_content};
use self::runtime_state::{
    build_embedding_segment, build_embedding_segment_from, build_graph_segment,
    cosine_similarity_f32, embedding_state_from_segment, engine_state_from_memento,
    engine_state_from_recovery_snapshot, memento_from_runtime_segments, rebuild_state_from_chunks,
};
#[cfg(test)]
use self::source_sync::should_skip_indexing_dir;
use self::source_sync::{
    chunk_belongs_to_source, source_key, source_record_matches, source_type_from_str,
};
use crate::ignore_rules::IgnoreRules;
#[cfg(test)]
use crate::memory_classification::{classification_rules_path, ClassificationRule};
use crate::memory_classification::{
    classify_memory, default_classification_rules, entity_lookup_score,
    load_or_bootstrap_classification_rules, memory_class_score, ClassificationRulesConfig,
    MemoryClass,
};
use crate::operation_checkpoint::{OperationCheckpoint, OperationTracker};
use crate::query_scoring::{
    aggregate_memory_score, contextual_freshness_score, episodic_memory_score,
    evergreen_memory_score, metadata_exact_match_bonus, metadata_exactness_score,
    metadata_overlap_score, retrieval_confidence_score, session_note_score,
    source_compactness_score, temporal_match_score,
};
use crate::recovery_snapshot::{RecoverySnapshot, RecoverySnapshotStore};
use crate::scheduler::{ScheduledJobState, SchedulerSnapshot};
use crate::text_utils::{
    detect_query_mode, has_recall_intent, is_low_signal_query_term, lexical_query_alternatives,
    parse_date_tokens, tokenize_folded_text, tokenize_text, QueryMode,
};
use anyhow::Result;
use libmemento::chunker::smart::SmartChunker;
use libmemento::chunker::Chunk;
use libmemento::format::{
    DocId, MementoFile, SourceRecord, SourceType, StoredChunk, StoredDocument, TextSpan,
};
use libmemento::learning::reasoning_trace::EvidenceChunk;
use libmemento::learning::{compute_query_confidence_cached, Generator};
use libmemento::matrix::{project_tokens_to_eigenspace, RetrievalConfig, SemanticMatrix};
use libmemento::storage::{
    ChunkTokenAdjacency, DocChunkEdge, EigenSegmentFile, EmbeddingSegmentFile, GraphSegmentFile,
    LexicalSegmentFile, ManifestMetadata, ManifestStore, MetadataSegmentFile,
    QuantizedChunkEmbedding, SegmentDescriptor, SegmentKind, SegmentStats, TokenGraphEdge,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

const INCREMENTAL_SYNC_BATCH_SIZE: usize = 64;
const INCREMENTAL_SYNC_PROGRESS_SLICE_SIZE: usize = 8;
const INCREMENTAL_SYNC_CHECKPOINT_BATCH_INTERVAL: usize = 8;
const INCREMENTAL_SYNC_CHECKPOINT_MAX_AGE: Duration = Duration::from_secs(20);
const DEFAULT_CHUNK_MAX_TOKENS: usize = 512;
const DEFAULT_CHUNK_OVERLAP_TOKENS: usize = 64;
const MEDIUM_FILE_CHUNK_MAX_TOKENS: usize = 1024;
const MEDIUM_FILE_CHUNK_OVERLAP_TOKENS: usize = 128;
const LARGE_FILE_CHUNK_MAX_TOKENS: usize = 2048;
const LARGE_FILE_CHUNK_OVERLAP_TOKENS: usize = 256;
const EXTRA_LARGE_FILE_CHUNK_MAX_TOKENS: usize = 3072;
const EXTRA_LARGE_FILE_CHUNK_OVERLAP_TOKENS: usize = 384;
const MEDIUM_FILE_THRESHOLD_BYTES: u64 = 512 * 1024;
const LARGE_FILE_THRESHOLD_BYTES: u64 = 2 * 1024 * 1024;
const EXTRA_LARGE_FILE_THRESHOLD_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone)]
pub struct MementoManager {
    data_dir: PathBuf,
    state: Arc<RwLock<EngineState>>,
    scheduler: Arc<RwLock<SchedulerSnapshot>>,
    operation_lock: Arc<Mutex<()>>,
}

struct EngineState {
    matrix: SemanticMatrix,
    vocabulary: HashMap<String, usize>,
    next_token_id: usize,
    documents: Vec<StoredDocument>,
    next_doc_id: DocId,
    next_chunk_id: u64,
    chunks: Vec<StoredChunk>,
    lexical_index: LexicalIndex,
    document_graph: DocumentGraph,
    chunk_embeddings: HashMap<u64, Vec<f32>>,
    document_embeddings: HashMap<DocId, Vec<f32>>,
    sources: Vec<SourceRecord>,
    domain: String,
    coherence_score: f64,
    classification_rules: ClassificationRulesConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    10
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResult {
    pub content: String,
    pub score: f64,
    pub source_path: String,
    pub chunk_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResponse {
    pub answer: String,
    pub results: Vec<QueryResult>,
    pub confidence: f64,
    pub query_tokens: usize,
    pub key_concepts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentRequest {
    pub source_path: String,
    #[serde(default)]
    pub offset_chars: usize,
    #[serde(default = "default_document_max_chars")]
    pub max_chars: usize,
}

fn default_document_max_chars() -> usize {
    4_000
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentResponse {
    pub source_path: String,
    pub title: Option<String>,
    pub content: String,
    pub offset_chars: usize,
    pub returned_chars: usize,
    pub total_chars: usize,
    pub has_more: bool,
    pub next_offset_chars: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportRequest {
    pub source: String,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportResponse {
    pub chunks_imported: usize,
    pub source_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub vocabulary_size: usize,
    pub non_zero_count: usize,
    pub coherence_score: f64,
    pub total_chunks: usize,
    pub total_sources: usize,
    pub document_graph_edges: usize,
    pub domain: String,
    pub memento_file_exists: bool,
    pub runtime_manifest_generation: u64,
    pub runtime_segment_count: usize,
    pub runtime_segments_ready: bool,
    pub runtime_graph_ready: bool,
    pub runtime_embedding_ready: bool,
    pub active_operation: Option<OperationCheckpoint>,
    pub scheduler_enabled: bool,
    pub scheduled_jobs: Vec<ScheduledJobState>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LearnResponse {
    pub coherence_before: f64,
    pub coherence_after: f64,
    pub eigenvectors_computed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub chunks_synced: usize,
    pub source_type: String,
    pub removed_chunks: usize,
    pub removed_sources: usize,
    pub added_files: usize,
    pub updated_files: usize,
    pub removed_files: usize,
    pub unchanged_files: usize,
    pub coherence_after: f64,
    pub eigenvectors_computed: usize,
}

#[derive(Debug, Clone)]
struct ChunkRanking {
    idx: usize,
    doc_id: DocId,
    source_path: String,
    score: f64,
    metadata_score: f64,
    metadata_bonus: f64,
    exactness_score: f64,
    entity_score: f64,
    query_coverage_score: f64,
    graph_score: f64,
}

#[derive(Debug, Clone)]
struct ResultBundle {
    source_path: String,
    chunk_indices: Vec<usize>,
    score: f64,
}

fn normalize_path(path: &str) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
}

impl MementoManager {
    pub async fn import(&self, req: &ImportRequest) -> Result<ImportResponse> {
        let source_type = source_type_from_str(&req.source)?;
        if matches!(source_type, SourceType::Folder | SourceType::Obsidian) {
            let sync = self.sync(req).await?;
            return Ok(ImportResponse {
                chunks_imported: sync.chunks_synced,
                source_type: req.source.clone(),
            });
        }

        let _operation_guard = self.operation_lock.lock().await;
        let source_path = source_key(&source_type, req.path.as_deref())?;
        let documents = self
            .load_documents(&source_type, req.path.as_deref())
            .await?;
        let chunk_count: usize = documents.iter().map(|document| document.chunks.len()).sum();

        let mut state = self.state.write().await;
        self.ingest_prepared_documents(&documents, &mut state);
        state.document_graph = DocumentGraph::build(&state.documents);

        let token_count = state.next_token_id;
        state.sources.push(SourceRecord {
            path: source_path,
            source_type: source_type.clone(),
            ingested_at: SystemTime::now(),
            chunk_count,
            token_count,
        });

        // Save
        drop(state);
        self.save().await?;

        Ok(ImportResponse {
            chunks_imported: chunk_count,
            source_type: req.source.clone(),
        })
    }

    pub async fn sync(&self, req: &ImportRequest) -> Result<SyncResponse> {
        let source_type = source_type_from_str(&req.source)?;
        let source_path = source_key(&source_type, req.path.as_deref())?;

        if matches!(
            source_type,
            SourceType::File | SourceType::Folder | SourceType::Obsidian
        ) {
            return self
                .sync_local_source_incremental(req, &source_type, &source_path)
                .await;
        }

        let _operation_guard = self.operation_lock.lock().await;
        let documents = self
            .load_documents(&source_type, req.path.as_deref())
            .await?;
        let chunk_count: usize = documents.iter().map(|document| document.chunks.len()).sum();

        let mut state = self.state.write().await;
        let before_chunks = state.chunks.len();
        let before_sources = state.sources.len();

        state
            .chunks
            .retain(|chunk| !chunk_belongs_to_source(chunk, &source_type, &source_path));
        state
            .documents
            .retain(|document| document.source_path != source_path);
        state
            .sources
            .retain(|source| !source_record_matches(source, &source_type, &source_path));

        let removed_chunks = before_chunks.saturating_sub(state.chunks.len());
        let removed_sources = before_sources.saturating_sub(state.sources.len());

        rebuild_state_from_chunks(&mut state);
        self.ingest_prepared_documents(&documents, &mut state);
        state.document_graph = DocumentGraph::build(&state.documents);

        let token_count = state.next_token_id;
        state.sources.push(SourceRecord {
            path: source_path,
            source_type: source_type.clone(),
            ingested_at: SystemTime::now(),
            chunk_count,
            token_count,
        });

        drop(state);
        self.save().await?;
        let learn = self.learn_with_cap(Some(8)).await?;

        Ok(SyncResponse {
            chunks_synced: chunk_count,
            source_type: req.source.clone(),
            removed_chunks,
            removed_sources,
            added_files: 0,
            updated_files: 0,
            removed_files: 0,
            unchanged_files: 0,
            coherence_after: learn.coherence_after,
            eigenvectors_computed: learn.eigenvectors_computed,
        })
    }

    pub async fn learn(&self) -> Result<LearnResponse> {
        let _operation_guard = self.operation_lock.lock().await;
        self.learn_with_cap(None).await
    }

    async fn learn_with_cap(&self, max_components: Option<usize>) -> Result<LearnResponse> {
        let (coherence_before, nnz, vocabulary_size, triplets, chunks, domain) = {
            let state = self.state.read().await;
            (
                state.coherence_score,
                state.matrix.non_zero_count(),
                state.matrix.vocabulary_size(),
                state
                    .matrix
                    .to_triplets()
                    .map_err(|error| anyhow::anyhow!("{error}"))?,
                state.chunks.clone(),
                state.domain.clone(),
            )
        };
        let target_k = if nnz > 1_000_000 {
            12
        } else if nnz > 250_000 {
            16
        } else if nnz > 50_000 {
            24
        } else {
            32
        };
        let capped_target = max_components
            .map(|cap| target_k.min(cap))
            .unwrap_or(target_k);
        let k = capped_target.min(vocabulary_size.saturating_sub(1));
        if k < 2 {
            return Ok(LearnResponse {
                coherence_before,
                coherence_after: coherence_before,
                eigenvectors_computed: 0,
            });
        }

        let (eigen, chunk_embeddings, document_embeddings) =
            tokio::task::spawn_blocking(move || -> Result<_> {
                let mut matrix = SemanticMatrix::from_triplets(vocabulary_size, &triplets)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
                let eigen = matrix
                    .compute_eigendecomposition(k)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
                let segment = build_embedding_segment_from(&chunks, &domain, &eigen);
                let (chunk_embeddings, document_embeddings) =
                    embedding_state_from_segment(&chunks, segment.as_ref());
                Ok((eigen, chunk_embeddings, document_embeddings))
            })
            .await
            .map_err(|error| anyhow::anyhow!("learning worker failed: {error}"))??;

        let mut state = self.state.write().await;
        state.matrix.restore_cached_eigen(eigen.clone());
        state.coherence_score = eigen.coherence_score;
        state.chunk_embeddings = chunk_embeddings;
        state.document_embeddings = document_embeddings;
        let eigenvectors_computed = eigen.eigenvalues.len();

        drop(state);
        self.save().await?;

        Ok(LearnResponse {
            coherence_before,
            coherence_after: eigen.coherence_score,
            eigenvectors_computed,
        })
    }

    /// Ingest raw text from a conversational source (e.g. Aginus POST /ingest)
    pub async fn ingest_text(&self, text: &str, source: &str) -> Result<()> {
        use libmemento::chunker::smart::SmartChunker;

        let _operation_guard = self.operation_lock.lock().await;
        let chunker = SmartChunker::new(512, 64);
        let raw_chunks = chunker.chunk_document(text, source);
        let documents = prepare_documents_from_chunks(raw_chunks);

        let mut state = self.state.write().await;
        self.ingest_prepared_documents(&documents, &mut state);
        state.document_graph = DocumentGraph::build(&state.documents);
        drop(state);
        self.save().await
    }
}
