use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Natural-language question or exact terms to retrieve.
    pub query: String,
    /// Number of ranked source documents to return (1-20, default 5).
    pub limit: Option<usize>,
    /// Maximum Unicode characters per evidence excerpt (80-4000, default 800).
    pub max_chars_per_result: Option<usize>,
    /// Include Memento's grounded extractive answer (default true).
    pub include_answer: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    pub answer: Option<String>,
    pub confidence: f64,
    pub query_tokens: usize,
    pub concepts: Vec<String>,
    pub evidence: Vec<EvidenceResult>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceResult {
    pub source_path: String,
    pub chunk_index: usize,
    pub score: f64,
    pub excerpt: String,
    pub excerpt_truncated: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct QueryRequest {
    pub query: String,
    pub top_k: usize,
}

#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    pub answer: String,
    pub results: Vec<QueryResult>,
    pub confidence: f64,
    pub query_tokens: usize,
    pub key_concepts: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub content: String,
    pub score: f64,
    pub source_path: String,
    pub chunk_index: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDocumentRequest {
    /// Exact source_path returned by memento_search_memory.
    pub source_path: String,
    /// Unicode character offset for pagination (default 0).
    pub offset_chars: Option<usize>,
    /// Maximum Unicode characters to return (1-20000, default 4000).
    pub max_chars: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentRequest {
    pub source_path: String,
    pub offset_chars: usize,
    pub max_chars: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StatusResponse {
    pub vocabulary_size: usize,
    pub non_zero_count: usize,
    pub coherence_score: f64,
    pub total_chunks: usize,
    pub total_sources: usize,
    pub document_graph_edges: usize,
    pub domain: String,
    pub runtime_manifest_generation: u64,
    pub runtime_segment_count: usize,
    pub runtime_segments_ready: bool,
    pub runtime_graph_ready: bool,
    pub runtime_embedding_ready: bool,
    pub scheduler_enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SyncSourceRequest {
    /// Source kind: file, folder, obsidian, claude, or codex.
    pub source: String,
    /// Required for file, folder, and obsidian; omitted for local session stores.
    pub path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportRequest {
    pub source: String,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LearnResponse {
    pub coherence_before: f64,
    pub coherence_after: f64,
    pub eigenvectors_computed: usize,
}
