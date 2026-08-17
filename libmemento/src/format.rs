//! .memento file format v3
//!
//! Binary format: MNTO magic + header + bincode + zstd payload
//!
//! ```text
//! [4 bytes] magic: b"MNTO"
//! [2 bytes] version: u16 = 3
//! [2 bytes] flags: u16 = 0 (reserved)
//! [8 bytes] header_size: u64
//! [N bytes] zstd-compressed bincode payload (MementoFile)
//! ```

use crate::chunker::Chunk;
use crate::matrix::SemanticMatrix;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read, Write};
use std::time::SystemTime;
use uuid::Uuid;

pub type DocId = u64;
pub type ChunkId = u64;

const MAGIC: &[u8; 4] = b"MNTO";
const VERSION: u16 = 3;
const LEGACY_VERSION: u16 = 2;
const HEADER_SIZE: u64 = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MementoFile {
    pub id: Uuid,
    pub domain: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,

    // Knowledge (the "weights")
    pub vocabulary: HashMap<String, usize>,
    pub next_token_id: usize,
    pub vocabulary_size: usize,
    pub triplets: Vec<(usize, usize, f64)>,
    pub coherence_score: f64,
    pub confidence_history: Vec<(SystemTime, f64)>,

    // Cached computation
    pub eigenvectors: Option<Vec<Vec<f64>>>,
    pub eigenvalues: Option<Vec<f64>>,

    // Provenance
    pub sources: Vec<SourceRecord>,
    pub query_count: u64,
    pub update_count: usize,

    // Canonical text substrate
    #[serde(default)]
    pub documents: Vec<StoredDocument>,
    #[serde(default)]
    pub next_doc_id: DocId,
    #[serde(default)]
    pub next_chunk_id: ChunkId,

    // Retrieval spans over the substrate
    pub chunks: Vec<StoredChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredDocument {
    pub doc_id: DocId,
    pub source_path: String,
    pub canonical_text: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredChunk {
    #[serde(default)]
    pub chunk_id: ChunkId,
    #[serde(default)]
    pub doc_id: DocId,
    #[serde(default)]
    pub span: Option<TextSpan>,
    #[serde(default)]
    pub content: String,
    pub chunk_index: usize,
    pub token_count: usize,
    pub source_path: String,
    pub section_title: Option<String>,
    pub chunk_type: String,
    pub token_ids: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRecord {
    pub path: String,
    pub source_type: SourceType,
    pub ingested_at: SystemTime,
    pub chunk_count: usize,
    pub token_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    Claude,
    Codex,
    File,
    Url,
    Folder,
    Obsidian,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Claude => write!(f, "claude"),
            SourceType::Codex => write!(f, "codex"),
            SourceType::File => write!(f, "file"),
            SourceType::Url => write!(f, "url"),
            SourceType::Folder => write!(f, "folder"),
            SourceType::Obsidian => write!(f, "obsidian"),
        }
    }
}

impl MementoFile {
    pub fn new(domain: &str, vocabulary_size: usize) -> Self {
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4(),
            domain: domain.to_string(),
            created_at: now,
            updated_at: now,
            vocabulary: HashMap::new(),
            next_token_id: 0,
            vocabulary_size,
            triplets: Vec::new(),
            coherence_score: 0.0,
            confidence_history: Vec::new(),
            eigenvectors: None,
            eigenvalues: None,
            sources: Vec::new(),
            query_count: 0,
            update_count: 0,
            documents: Vec::new(),
            next_doc_id: 0,
            next_chunk_id: 0,
            chunks: Vec::new(),
        }
    }

    pub fn from_matrix(
        matrix: &SemanticMatrix,
        vocabulary: HashMap<String, usize>,
        next_token_id: usize,
        domain: &str,
    ) -> Self {
        let triplets = matrix.to_triplets().unwrap_or_default();
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4(),
            domain: domain.to_string(),
            created_at: now,
            updated_at: now,
            vocabulary,
            next_token_id,
            vocabulary_size: matrix.vocabulary_size(),
            triplets,
            coherence_score: matrix.coherence_score,
            confidence_history: matrix
                .confidence_history()
                .iter()
                .map(|&(t, c)| (t, c))
                .collect(),
            eigenvectors: None,
            eigenvalues: None,
            sources: Vec::new(),
            query_count: 0,
            update_count: 0,
            documents: Vec::new(),
            next_doc_id: 0,
            next_chunk_id: 0,
            chunks: Vec::new(),
        }
    }

    pub fn add_source(&mut self, source: SourceRecord) {
        self.sources.push(source);
    }

    pub fn add_document(&mut self, document: StoredDocument) {
        self.next_doc_id = self.next_doc_id.max(document.doc_id.saturating_add(1));
        self.documents.push(document);
    }

    pub fn add_chunk(&mut self, chunk: StoredChunk) {
        self.next_chunk_id = self.next_chunk_id.max(chunk.chunk_id.saturating_add(1));
        self.chunks.push(chunk);
    }

    pub fn set_eigen(&mut self, eigenvectors: &DMatrix<f64>, eigenvalues: &DVector<f64>) {
        let rows = eigenvectors.nrows();
        let cols = eigenvectors.ncols();
        let mut vecs = Vec::with_capacity(cols);
        for c in 0..cols {
            let mut col_vec = Vec::with_capacity(rows);
            for r in 0..rows {
                col_vec.push(eigenvectors[(r, c)]);
            }
            vecs.push(col_vec);
        }
        self.eigenvectors = Some(vecs);
        self.eigenvalues = Some(eigenvalues.iter().copied().collect());
    }

    pub fn cached_eigen(&self) -> Option<crate::matrix::EigenDecomposition> {
        let eigenvectors = self.eigenvectors.as_ref()?;
        let eigenvalues = self.eigenvalues.as_ref()?;
        if eigenvectors.is_empty() || eigenvalues.is_empty() {
            return None;
        }

        let rows = eigenvectors[0].len();
        let cols = eigenvectors.len();
        let mut matrix = DMatrix::zeros(rows, cols);
        for (col_idx, column) in eigenvectors.iter().enumerate() {
            if column.len() != rows {
                return None;
            }
            for (row_idx, value) in column.iter().enumerate() {
                matrix[(row_idx, col_idx)] = *value;
            }
        }

        let values = DVector::from_vec(eigenvalues.clone());
        crate::matrix::EigenDecomposition::new(matrix, values).ok()
    }

    pub fn to_matrix(&self) -> SemanticMatrix {
        let mut matrix = SemanticMatrix::from_triplets(self.vocabulary_size, &self.triplets)
            .unwrap_or_else(|_| SemanticMatrix::new(self.vocabulary_size));
        for &(t, c) in &self.confidence_history {
            matrix.add_confidence_record(t, c);
        }
        matrix.set_query_count(self.query_count);
        matrix.set_update_count(self.update_count);
        matrix
    }

    pub fn document_map(&self) -> HashMap<DocId, &StoredDocument> {
        self.documents.iter().map(|doc| (doc.doc_id, doc)).collect()
    }

    pub fn save<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.save_with_compression(writer, 6)
    }

    pub fn save_with_compression<W: Write>(
        &self,
        writer: &mut W,
        compression_level: i32,
    ) -> io::Result<()> {
        writer.write_all(MAGIC)?;
        writer.write_all(&VERSION.to_le_bytes())?;
        writer.write_all(&0u16.to_le_bytes())?;
        writer.write_all(&HEADER_SIZE.to_le_bytes())?;

        let mut encoder =
            zstd::Encoder::new(writer, compression_level).map_err(io::Error::other)?;
        bincode::serialize_into(&mut encoder, self).map_err(io::Error::other)?;
        encoder.finish().map_err(io::Error::other)?;

        Ok(())
    }

    pub fn load<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid magic bytes: expected MNTO",
            ));
        }

        let mut version_bytes = [0u8; 2];
        reader.read_exact(&mut version_bytes)?;
        let version = u16::from_le_bytes(version_bytes);
        if version != VERSION && version != LEGACY_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported version: {version}, expected {VERSION} or {LEGACY_VERSION}"),
            ));
        }

        let mut skip = [0u8; 10];
        reader.read_exact(&mut skip)?;
        let mut decoder = zstd::Decoder::new(reader).map_err(io::Error::other)?;

        if version == VERSION {
            let mut file: MementoFile =
                bincode::deserialize_from(&mut decoder).map_err(io::Error::other)?;
            file.repair_ids();
            return Ok(file);
        }

        let legacy: MementoFileV2 =
            bincode::deserialize_from(&mut decoder).map_err(io::Error::other)?;
        Ok(legacy.into_current())
    }

    fn repair_ids(&mut self) {
        self.next_doc_id = self
            .documents
            .iter()
            .map(|doc| doc.doc_id.saturating_add(1))
            .max()
            .unwrap_or(self.next_doc_id);
        self.next_chunk_id = self
            .chunks
            .iter()
            .map(|chunk| chunk.chunk_id.saturating_add(1))
            .max()
            .unwrap_or(self.next_chunk_id);
    }
}

impl StoredDocument {
    pub fn slice(&self, span: TextSpan) -> Option<&str> {
        if span.start > span.end || span.end > self.canonical_text.len() {
            return None;
        }
        if !self.canonical_text.is_char_boundary(span.start)
            || !self.canonical_text.is_char_boundary(span.end)
        {
            return None;
        }
        self.canonical_text.get(span.start..span.end)
    }
}

impl StoredChunk {
    pub fn from_chunk(
        chunk: &Chunk,
        token_ids: Vec<usize>,
        chunk_id: ChunkId,
        doc_id: DocId,
        span: Option<TextSpan>,
    ) -> Self {
        Self {
            chunk_id,
            doc_id,
            span,
            content: String::new(),
            chunk_index: chunk.chunk_index,
            token_count: chunk.token_count,
            source_path: chunk.metadata.source_path.clone(),
            section_title: chunk.metadata.section_title.clone(),
            chunk_type: chunk.metadata.chunk_type.as_str().to_string(),
            token_ids,
        }
    }

    pub fn resolve_content<'a>(
        &'a self,
        documents: &'a HashMap<DocId, &StoredDocument>,
    ) -> &'a str {
        if !self.content.is_empty() {
            return &self.content;
        }

        if let (Some(doc), Some(span)) = (documents.get(&self.doc_id), self.span) {
            if let Some(text) = doc.slice(span) {
                return text;
            }
        }

        &self.content
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MementoFileV2 {
    id: Uuid,
    domain: String,
    created_at: SystemTime,
    updated_at: SystemTime,
    vocabulary: HashMap<String, usize>,
    next_token_id: usize,
    vocabulary_size: usize,
    triplets: Vec<(usize, usize, f64)>,
    coherence_score: f64,
    confidence_history: Vec<(SystemTime, f64)>,
    eigenvectors: Option<Vec<Vec<f64>>>,
    eigenvalues: Option<Vec<f64>>,
    sources: Vec<SourceRecord>,
    query_count: u64,
    update_count: usize,
    chunks: Vec<StoredChunkV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredChunkV2 {
    content: String,
    chunk_index: usize,
    token_count: usize,
    source_path: String,
    section_title: Option<String>,
    chunk_type: String,
    token_ids: Vec<usize>,
}

impl MementoFileV2 {
    fn into_current(self) -> MementoFile {
        let mut documents_by_path: BTreeMap<String, (DocId, String)> = BTreeMap::new();
        let mut next_doc_id: DocId = 0;

        for chunk in &self.chunks {
            let entry = documents_by_path
                .entry(chunk.source_path.clone())
                .or_insert_with(|| {
                    let doc_id = next_doc_id;
                    next_doc_id += 1;
                    (doc_id, String::new())
                });

            if !entry.1.is_empty() {
                entry.1.push_str("\n\n");
            }
            entry.1.push_str(&chunk.content);
        }

        let mut documents = Vec::new();
        let mut chunks = Vec::with_capacity(self.chunks.len());
        let mut doc_positions: HashMap<DocId, usize> = HashMap::new();

        for (source_path, (doc_id, canonical_text)) in &documents_by_path {
            documents.push(StoredDocument {
                doc_id: *doc_id,
                source_path: source_path.clone(),
                canonical_text: canonical_text.clone(),
                title: None,
            });
            doc_positions.insert(*doc_id, 0);
        }

        let path_to_doc_id: HashMap<String, DocId> = documents
            .iter()
            .map(|doc| (doc.source_path.clone(), doc.doc_id))
            .collect();

        for (chunk_id, chunk) in self.chunks.into_iter().enumerate() {
            let doc_id = *path_to_doc_id.get(&chunk.source_path).unwrap_or(&0);
            let doc = documents
                .iter()
                .find(|doc| doc.doc_id == doc_id)
                .expect("legacy conversion should have document");
            let position = doc_positions.entry(doc_id).or_insert(0);
            let span = doc.canonical_text[*position..]
                .find(&chunk.content)
                .map(|offset| {
                    let start = *position + offset;
                    let end = start + chunk.content.len();
                    *position = end;
                    TextSpan { start, end }
                });

            chunks.push(StoredChunk {
                chunk_id: chunk_id as ChunkId,
                doc_id,
                span,
                content: chunk.content,
                chunk_index: chunk.chunk_index,
                token_count: chunk.token_count,
                source_path: chunk.source_path,
                section_title: chunk.section_title,
                chunk_type: chunk.chunk_type,
                token_ids: chunk.token_ids,
            });
        }

        MementoFile {
            id: self.id,
            domain: self.domain,
            created_at: self.created_at,
            updated_at: self.updated_at,
            vocabulary: self.vocabulary,
            next_token_id: self.next_token_id,
            vocabulary_size: self.vocabulary_size,
            triplets: self.triplets,
            coherence_score: self.coherence_score,
            confidence_history: self.confidence_history,
            eigenvectors: self.eigenvectors,
            eigenvalues: self.eigenvalues,
            sources: self.sources,
            query_count: self.query_count,
            update_count: self.update_count,
            documents,
            next_doc_id,
            next_chunk_id: chunks.len() as ChunkId,
            chunks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memento_file_roundtrip() {
        let mut file = MementoFile::new("test", 100);
        file.add_source(SourceRecord {
            path: "/test/path".to_string(),
            source_type: SourceType::File,
            ingested_at: SystemTime::now(),
            chunk_count: 5,
            token_count: 100,
        });
        file.add_document(StoredDocument {
            doc_id: 0,
            source_path: "/test/path".to_string(),
            canonical_text: "hello world".to_string(),
            title: Some("test".to_string()),
        });
        file.add_chunk(StoredChunk {
            chunk_id: 0,
            doc_id: 0,
            span: Some(TextSpan { start: 0, end: 5 }),
            content: String::new(),
            chunk_index: 0,
            token_count: 2,
            source_path: "/test/path".to_string(),
            section_title: None,
            chunk_type: "paragraph".to_string(),
            token_ids: vec![0, 1],
        });

        let mut buf = Vec::new();
        file.save(&mut buf).unwrap();

        let loaded = MementoFile::load(&mut buf.as_slice()).unwrap();
        assert_eq!(loaded.domain, "test");
        assert_eq!(loaded.vocabulary_size, 100);
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.sources[0].source_type, SourceType::File);
        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.chunks.len(), 1);
    }

    #[test]
    fn test_eigen_roundtrip() {
        let mut file = MementoFile::new("test", 10);
        let eigenvectors = DMatrix::identity(3, 2);
        let eigenvalues = DVector::from_vec(vec![2.0, 1.0]);
        file.set_eigen(&eigenvectors, &eigenvalues);

        let restored = file.cached_eigen().unwrap();
        assert_eq!(restored.eigenvectors.nrows(), 3);
        assert_eq!(restored.eigenvectors.ncols(), 2);
        assert_eq!(restored.eigenvalues.len(), 2);
    }

    #[test]
    fn test_magic_bytes_validation() {
        let bad_data = b"BAAD\x03\x00\x00\x00\x10\x00\x00\x00\x00\x00\x00\x00";
        let result = MementoFile::load(&mut bad_data.as_slice());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("magic"));
    }

    #[test]
    fn test_version_validation() {
        let bad_version = b"MNTO\x99\x00\x00\x00\x10\x00\x00\x00\x00\x00\x00\x00";
        let result = MementoFile::load(&mut bad_version.as_slice());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_stored_chunk_resolves_text_from_canonical_document() {
        let document = StoredDocument {
            doc_id: 7,
            source_path: "/tmp/doc.md".to_string(),
            canonical_text: "alpha beta gamma".to_string(),
            title: None,
        };
        let chunk = StoredChunk {
            chunk_id: 1,
            doc_id: 7,
            span: Some(TextSpan { start: 6, end: 10 }),
            content: String::new(),
            chunk_index: 0,
            token_count: 1,
            source_path: "/tmp/doc.md".to_string(),
            section_title: None,
            chunk_type: "paragraph".to_string(),
            token_ids: vec![1],
        };
        let docs = HashMap::from([(7, &document)]);
        assert_eq!(chunk.resolve_content(&docs), "beta");
    }

    #[test]
    fn test_legacy_v2_payload_migrates_to_canonical_documents() {
        let legacy = MementoFileV2 {
            id: Uuid::new_v4(),
            domain: "legacy".to_string(),
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            vocabulary: HashMap::from([
                ("alpha".to_string(), 0),
                ("beta".to_string(), 1),
                ("gamma".to_string(), 2),
            ]),
            next_token_id: 3,
            vocabulary_size: 16,
            triplets: Vec::new(),
            coherence_score: 0.0,
            confidence_history: Vec::new(),
            eigenvectors: None,
            eigenvalues: None,
            sources: vec![SourceRecord {
                path: "/tmp/legacy.md".to_string(),
                source_type: SourceType::File,
                ingested_at: SystemTime::now(),
                chunk_count: 2,
                token_count: 3,
            }],
            query_count: 0,
            update_count: 0,
            chunks: vec![
                StoredChunkV2 {
                    content: "alpha beta".to_string(),
                    chunk_index: 0,
                    token_count: 2,
                    source_path: "/tmp/legacy.md".to_string(),
                    section_title: Some("Legacy".to_string()),
                    chunk_type: "paragraph".to_string(),
                    token_ids: vec![0, 1],
                },
                StoredChunkV2 {
                    content: "gamma".to_string(),
                    chunk_index: 1,
                    token_count: 1,
                    source_path: "/tmp/legacy.md".to_string(),
                    section_title: Some("Legacy".to_string()),
                    chunk_type: "paragraph".to_string(),
                    token_ids: vec![2],
                },
            ],
        };

        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&LEGACY_VERSION.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        let encoded = bincode::serialize(&legacy).unwrap();
        let compressed = zstd::encode_all(encoded.as_slice(), 6).unwrap();
        buf.extend_from_slice(&compressed);

        let loaded = MementoFile::load(&mut buf.as_slice()).unwrap();
        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.next_doc_id, 1);
        assert_eq!(loaded.next_chunk_id, 2);
        assert_eq!(loaded.documents[0].canonical_text, "alpha beta\n\ngamma");

        let docs = loaded.document_map();
        assert_eq!(loaded.chunks[0].resolve_content(&docs), "alpha beta");
        assert_eq!(loaded.chunks[1].resolve_content(&docs), "gamma");
    }
}
