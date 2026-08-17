//! libmemento — Core engine library for Memento.
//!
//! Provides semantic co-occurrence matrix, eigendecomposition, learning engine,
//! knowledge consolidation, storage/checkpointing, parsers, and chunking.

pub mod chunker;
pub mod consolidation;
pub mod format;
pub mod learning;
pub mod matrix;
pub mod parser;
pub mod storage;
pub mod sync;

// Re-export key types for convenience
pub use format::{MementoFile, SourceRecord, SourceType};
pub use matrix::SemanticMatrix;
pub use storage::{ManifestFile, ManifestStore, RuntimeLayout, SegmentDescriptor, SegmentKind};
