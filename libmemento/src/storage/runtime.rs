//! Runtime layout and manifest commit model for the next-generation Memento store.
//!
//! This module is the first storage-kernel block for the Language Index Runtime.
//! It introduces:
//! - stable on-disk runtime directories
//! - manifest publication via atomic CURRENT pointer updates
//! - segment descriptors that let legacy snapshots coexist with future segment files

use crate::format::{SourceRecord, StoredChunk, StoredDocument};
use crate::storage::{Result, StorageError};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STORE_FORMAT_VERSION: u32 = 1;
const SEGMENT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct RuntimeLayout {
    root: PathBuf,
}

impl RuntimeLayout {
    pub fn init(root: &Path) -> Result<Self> {
        let layout = Self {
            root: root.to_path_buf(),
        };

        fs::create_dir_all(&layout.root)?;
        fs::create_dir_all(layout.wal_dir())?;
        fs::create_dir_all(layout.manifests_dir())?;
        fs::create_dir_all(layout.segments_dir())?;
        fs::create_dir_all(layout.caches_dir())?;
        fs::create_dir_all(layout.snapshots_dir())?;
        fs::create_dir_all(layout.config_dir())?;

        Ok(layout)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn wal_dir(&self) -> PathBuf {
        self.root.join("wal")
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    pub fn segments_dir(&self) -> PathBuf {
        self.root.join("segments")
    }

    pub fn caches_dir(&self) -> PathBuf {
        self.root.join("caches")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn current_manifest_path(&self) -> PathBuf {
        self.manifests_dir().join("CURRENT")
    }

    pub fn legacy_snapshot_path(&self) -> PathBuf {
        self.root.join("default.memento")
    }

    pub fn manifest_path(&self, generation: u64) -> PathBuf {
        self.manifests_dir()
            .join(format!("manifest-{generation:020}.json"))
    }

    pub fn segment_file_name(&self, generation: u64, kind: SegmentKind) -> String {
        format!("segment-{generation:020}-{}.bin.zst", kind.as_str())
    }

    pub fn segment_path(&self, generation: u64, kind: SegmentKind) -> PathBuf {
        self.segments_dir()
            .join(self.segment_file_name(generation, kind))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    LegacySnapshot,
    Lexical,
    Metadata,
    Graph,
    Eigen,
    Embedding,
    Derived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentDescriptor {
    pub segment_id: String,
    pub generation: u64,
    pub kind: SegmentKind,
    pub relative_path: String,
    pub format_version: u32,
    pub created_unix_ms: u128,
    pub doc_count: u64,
    pub chunk_count: u64,
    pub token_count: u64,
    #[serde(default)]
    pub supersedes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestMetadata {
    pub domain: String,
    pub source_count: u64,
    pub vocabulary_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WalRange {
    pub first_sequence: u64,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    pub manifest_id: String,
    pub generation: u64,
    pub created_unix_ms: u128,
    pub store_format_version: u32,
    pub wal_range: WalRange,
    pub active_segments: Vec<SegmentDescriptor>,
    #[serde(default)]
    pub tombstones: Vec<String>,
    pub metadata: ManifestMetadata,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentStats {
    pub doc_count: u64,
    pub chunk_count: u64,
    pub token_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySnapshotStats {
    pub domain: String,
    pub source_count: u64,
    pub vocabulary_size: u64,
    pub chunk_count: u64,
    pub token_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LexicalSegmentFile {
    pub domain: String,
    pub vocabulary: std::collections::HashMap<String, usize>,
    pub next_token_id: usize,
    pub vocabulary_size: usize,
    pub triplets: Vec<(usize, usize, f64)>,
    pub coherence_score: f64,
    pub confidence_history: Vec<(SystemTime, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetadataSegmentFile {
    pub domain: String,
    pub documents: Vec<StoredDocument>,
    pub next_doc_id: u64,
    pub next_chunk_id: u64,
    pub chunks: Vec<StoredChunk>,
    pub sources: Vec<SourceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EigenSegmentFile {
    pub eigenvectors: Vec<Vec<f64>>,
    pub eigenvalues: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocChunkEdge {
    pub doc_id: u64,
    pub chunk_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkTokenAdjacency {
    pub chunk_id: u64,
    pub token_ids: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenGraphEdge {
    pub token_a: usize,
    pub token_b: usize,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphSegmentFile {
    pub domain: String,
    pub doc_chunk_edges: Vec<DocChunkEdge>,
    pub chunk_token_adjacency: Vec<ChunkTokenAdjacency>,
    pub token_graph_edges: Vec<TokenGraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuantizedChunkEmbedding {
    pub chunk_id: u64,
    pub vector: Vec<i16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingSegmentFile {
    pub domain: String,
    pub dimensions: usize,
    pub quantization_max: i16,
    pub embeddings: Vec<QuantizedChunkEmbedding>,
}

impl ManifestFile {
    pub fn legacy_snapshot(generation: u64, stats: LegacySnapshotStats) -> Self {
        Self {
            manifest_id: format!("manifest-{generation:020}"),
            generation,
            created_unix_ms: now_unix_ms(),
            store_format_version: STORE_FORMAT_VERSION,
            wal_range: WalRange::default(),
            active_segments: vec![SegmentDescriptor {
                segment_id: format!("legacy-default-{generation:020}"),
                generation,
                kind: SegmentKind::LegacySnapshot,
                relative_path: "default.memento".to_string(),
                format_version: 3,
                created_unix_ms: now_unix_ms(),
                doc_count: stats.source_count,
                chunk_count: stats.chunk_count,
                token_count: stats.token_count,
                supersedes: Vec::new(),
            }],
            tombstones: Vec::new(),
            metadata: ManifestMetadata {
                domain: stats.domain,
                source_count: stats.source_count,
                vocabulary_size: stats.vocabulary_size,
            },
        }
    }
}

pub struct ManifestStore {
    layout: RuntimeLayout,
}

impl ManifestStore {
    pub fn init(root: &Path) -> Result<Self> {
        Ok(Self {
            layout: RuntimeLayout::init(root)?,
        })
    }

    pub fn layout(&self) -> &RuntimeLayout {
        &self.layout
    }

    pub fn load_current(&self) -> Result<Option<ManifestFile>> {
        let current_path = self.layout.current_manifest_path();
        if !current_path.exists() {
            return Ok(None);
        }

        let pointer = fs::read_to_string(&current_path)?;
        let generation: u64 = pointer.trim().parse().map_err(|e| {
            StorageError::SerializationError(format!("Invalid CURRENT pointer: {e}"))
        })?;

        self.load_generation(generation)
    }

    pub fn load_generation(&self, generation: u64) -> Result<Option<ManifestFile>> {
        let path = self.layout.manifest_path(generation);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(path)?;
        let manifest = serde_json::from_slice(&bytes).map_err(|e| {
            StorageError::SerializationError(format!("Failed to decode manifest: {e}"))
        })?;
        Ok(Some(manifest))
    }

    pub fn next_generation(&self) -> Result<u64> {
        Ok(self
            .load_current()?
            .map(|manifest| manifest.generation + 1)
            .unwrap_or(1))
    }

    pub fn publish(&self, manifest: &ManifestFile) -> Result<()> {
        let manifest_path = self.layout.manifest_path(manifest.generation);
        let payload = serde_json::to_vec_pretty(manifest).map_err(|e| {
            StorageError::SerializationError(format!("Failed to encode manifest: {e}"))
        })?;
        write_atomic(&manifest_path, &payload)?;

        let pointer_path = self.layout.current_manifest_path();
        write_atomic(&pointer_path, manifest.generation.to_string().as_bytes())?;
        Ok(())
    }

    pub fn publish_legacy_snapshot(&self, stats: LegacySnapshotStats) -> Result<ManifestFile> {
        let generation = self.next_generation()?;
        let manifest = ManifestFile::legacy_snapshot(generation, stats);
        self.publish(&manifest)?;
        Ok(manifest)
    }

    pub fn write_segment<T: Serialize>(
        &self,
        generation: u64,
        kind: SegmentKind,
        payload: &T,
        stats: SegmentStats,
        supersedes: Vec<String>,
    ) -> Result<SegmentDescriptor> {
        let relative_path = format!(
            "segments/{}",
            self.layout.segment_file_name(generation, kind)
        );
        let full_path = self.layout.segment_path(generation, kind);
        let encoded = bincode::serialize(payload).map_err(|e| {
            StorageError::SerializationError(format!("Failed to encode segment: {e}"))
        })?;
        let compressed = zstd::encode_all(encoded.as_slice(), 6).map_err(|e| {
            StorageError::SerializationError(format!("Failed to compress segment: {e}"))
        })?;
        write_atomic(&full_path, &compressed)?;

        Ok(SegmentDescriptor {
            segment_id: format!("segment-{generation:020}-{}", kind.as_str()),
            generation,
            kind,
            relative_path,
            format_version: SEGMENT_FORMAT_VERSION,
            created_unix_ms: now_unix_ms(),
            doc_count: stats.doc_count,
            chunk_count: stats.chunk_count,
            token_count: stats.token_count,
            supersedes,
        })
    }

    pub fn read_segment<T: DeserializeOwned>(&self, descriptor: &SegmentDescriptor) -> Result<T> {
        let path = self.layout.root().join(&descriptor.relative_path);
        let compressed = fs::read(path)?;
        let decoded = zstd::decode_all(compressed.as_slice()).map_err(|e| {
            StorageError::SerializationError(format!("Failed to decompress segment: {e}"))
        })?;
        bincode::deserialize(&decoded)
            .map_err(|e| StorageError::SerializationError(format!("Failed to decode segment: {e}")))
    }

    pub fn publish_runtime_segments(
        &self,
        metadata: ManifestMetadata,
        active_segments: Vec<SegmentDescriptor>,
        tombstones: Vec<String>,
    ) -> Result<ManifestFile> {
        let generation = self.next_generation()?;
        let manifest = ManifestFile {
            manifest_id: format!("manifest-{generation:020}"),
            generation,
            created_unix_ms: now_unix_ms(),
            store_format_version: STORE_FORMAT_VERSION,
            wal_range: WalRange::default(),
            active_segments,
            tombstones,
            metadata,
        };
        self.publish(&manifest)?;
        Ok(manifest)
    }
}

impl SegmentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentKind::LegacySnapshot => "legacy_snapshot",
            SegmentKind::Lexical => "lexical",
            SegmentKind::Metadata => "metadata",
            SegmentKind::Graph => "graph",
            SegmentKind::Eigen => "eigen",
            SegmentKind::Embedding => "embedding",
            SegmentKind::Derived => "derived",
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn runtime_layout_initializes_kernel_directories() {
        let dir = tempdir().unwrap();
        let layout = RuntimeLayout::init(dir.path()).unwrap();

        assert!(layout.wal_dir().exists());
        assert!(layout.manifests_dir().exists());
        assert!(layout.segments_dir().exists());
        assert!(layout.caches_dir().exists());
        assert!(layout.snapshots_dir().exists());
        assert!(layout.config_dir().exists());
    }

    #[test]
    fn manifest_store_publishes_and_loads_current_manifest() {
        let dir = tempdir().unwrap();
        let store = ManifestStore::init(dir.path()).unwrap();

        let manifest = store
            .publish_legacy_snapshot(LegacySnapshotStats {
                domain: "default".to_string(),
                source_count: 3,
                vocabulary_size: 120,
                chunk_count: 42,
                token_count: 256,
            })
            .unwrap();

        let current = store.load_current().unwrap().unwrap();
        assert_eq!(current, manifest);
        assert_eq!(current.generation, 1);
        assert_eq!(current.active_segments.len(), 1);
        assert_eq!(current.active_segments[0].kind, SegmentKind::LegacySnapshot);
    }

    #[test]
    fn manifest_generation_increments_monotonically() {
        let dir = tempdir().unwrap();
        let store = ManifestStore::init(dir.path()).unwrap();

        let first = store
            .publish_legacy_snapshot(LegacySnapshotStats {
                domain: "default".to_string(),
                source_count: 1,
                vocabulary_size: 10,
                chunk_count: 5,
                token_count: 20,
            })
            .unwrap();

        let second = store
            .publish_legacy_snapshot(LegacySnapshotStats {
                domain: "default".to_string(),
                source_count: 2,
                vocabulary_size: 20,
                chunk_count: 10,
                token_count: 40,
            })
            .unwrap();

        assert_eq!(first.generation, 1);
        assert_eq!(second.generation, 2);
        assert_eq!(store.next_generation().unwrap(), 3);
    }

    #[test]
    fn runtime_segments_roundtrip_payloads() {
        let dir = tempdir().unwrap();
        let store = ManifestStore::init(dir.path()).unwrap();
        let generation = store.next_generation().unwrap();

        let lexical = LexicalSegmentFile {
            domain: "default".to_string(),
            vocabulary: HashMap::from([("memory".to_string(), 0usize)]),
            next_token_id: 1,
            vocabulary_size: 32,
            triplets: vec![(0, 0, 1.0)],
            coherence_score: 0.75,
            confidence_history: vec![(SystemTime::now(), 0.75)],
        };
        let lexical_descriptor = store
            .write_segment(
                generation,
                SegmentKind::Lexical,
                &lexical,
                SegmentStats {
                    token_count: 1,
                    ..SegmentStats::default()
                },
                Vec::new(),
            )
            .unwrap();
        let restored_lexical: LexicalSegmentFile = store.read_segment(&lexical_descriptor).unwrap();
        assert_eq!(restored_lexical, lexical);

        let metadata = MetadataSegmentFile {
            domain: "default".to_string(),
            documents: vec![StoredDocument {
                doc_id: 0,
                source_path: "/tmp/note.md".to_string(),
                canonical_text: "memory substrate".to_string(),
                title: Some("note".to_string()),
            }],
            next_doc_id: 1,
            next_chunk_id: 1,
            chunks: vec![StoredChunk {
                chunk_id: 0,
                doc_id: 0,
                span: Some(crate::format::TextSpan { start: 0, end: 6 }),
                content: String::new(),
                chunk_index: 0,
                token_count: 1,
                source_path: "/tmp/note.md".to_string(),
                section_title: None,
                chunk_type: "paragraph".to_string(),
                token_ids: vec![0],
            }],
            sources: vec![SourceRecord {
                path: "/tmp/note.md".to_string(),
                source_type: crate::format::SourceType::File,
                ingested_at: SystemTime::now(),
                chunk_count: 1,
                token_count: 1,
            }],
        };
        let metadata_descriptor = store
            .write_segment(
                generation,
                SegmentKind::Metadata,
                &metadata,
                SegmentStats {
                    doc_count: 1,
                    chunk_count: 1,
                    token_count: 1,
                },
                Vec::new(),
            )
            .unwrap();
        let restored_metadata: MetadataSegmentFile =
            store.read_segment(&metadata_descriptor).unwrap();
        assert_eq!(restored_metadata, metadata);

        let graph = GraphSegmentFile {
            domain: "default".to_string(),
            doc_chunk_edges: vec![DocChunkEdge {
                doc_id: 0,
                chunk_id: 0,
            }],
            chunk_token_adjacency: vec![ChunkTokenAdjacency {
                chunk_id: 0,
                token_ids: vec![0, 1],
            }],
            token_graph_edges: vec![TokenGraphEdge {
                token_a: 0,
                token_b: 1,
                weight: 0.8,
            }],
        };
        let graph_descriptor = store
            .write_segment(
                generation,
                SegmentKind::Graph,
                &graph,
                SegmentStats {
                    doc_count: 1,
                    chunk_count: 1,
                    token_count: 2,
                },
                Vec::new(),
            )
            .unwrap();
        let restored_graph: GraphSegmentFile = store.read_segment(&graph_descriptor).unwrap();
        assert_eq!(restored_graph, graph);

        let embeddings = EmbeddingSegmentFile {
            domain: "default".to_string(),
            dimensions: 3,
            quantization_max: i16::MAX,
            embeddings: vec![QuantizedChunkEmbedding {
                chunk_id: 0,
                vector: vec![1024, -2048, 512],
            }],
        };
        let embedding_descriptor = store
            .write_segment(
                generation,
                SegmentKind::Embedding,
                &embeddings,
                SegmentStats {
                    chunk_count: 1,
                    token_count: 2,
                    ..SegmentStats::default()
                },
                Vec::new(),
            )
            .unwrap();
        let restored_embeddings: EmbeddingSegmentFile =
            store.read_segment(&embedding_descriptor).unwrap();
        assert_eq!(restored_embeddings, embeddings);
    }
}
