//! Checkpoint and restore operations for semantic matrices
//!
//! Provides serialization and deserialization of SemanticMatrix structures
//! to/from disk with compression for efficient storage.

use crate::matrix::SemanticMatrix;
use crate::storage::{Result, StorageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};

/// Checkpoint structure for semantic matrix serialization
///
/// This structure contains all the necessary data to fully restore
/// a SemanticMatrix to its exact state at the time of checkpointing.
///
/// **CRITICAL FIX**: Now includes vocabulary to prevent token ID scrambling on restart
#[derive(Serialize, Deserialize)]
pub struct MatrixCheckpoint {
    /// Checkpoint creation timestamp
    pub timestamp: SystemTime,

    /// Checkpoint format version for future compatibility
    pub version: u32,

    /// Matrix unique identifier
    pub matrix_id: uuid::Uuid,

    /// Domain specialization label
    pub domain_label: String,

    /// Vocabulary size
    pub vocabulary_size: usize,

    /// Number of updates since last consolidation
    pub update_count: usize,

    /// Consolidation threshold
    pub consolidation_threshold: usize,

    /// Coherence score (spectral gap)
    pub coherence_score: f64,

    /// Creation timestamp
    pub created_at: SystemTime,

    /// Last modification timestamp
    pub updated_at: SystemTime,

    /// Matrix data as COO (Coordinate) format triplets: (row, col, value)
    pub triplets: Vec<(usize, usize, f64)>,

    /// Confidence history: (timestamp, confidence) pairs
    pub confidence_history: Vec<(SystemTime, f64)>,

    /// Total number of queries processed
    pub query_count: u64,

    /// **NEW**: Vocabulary mapping (word → token_id)
    /// This preserves token IDs across restarts, preventing knowledge loss
    pub vocabulary: std::collections::HashMap<String, usize>,

    /// **NEW**: Next token ID for new words
    /// Ensures new tokens don't collide with existing ones
    pub next_token_id: usize,
}

/// Compression configuration for checkpoints
#[derive(Debug, Clone, Copy, Default)]
pub enum CompressionLevel {
    /// Fastest compression, larger files (zstd level 1)
    Fast,
    /// Balanced compression and speed (zstd level 6) - Default
    #[default]
    Balanced,
    /// Maximum compression, slower (zstd level 19)
    High,
}

impl CompressionLevel {
    fn to_zstd_level(self) -> i32 {
        match self {
            CompressionLevel::Fast => 1,
            CompressionLevel::Balanced => 6,
            CompressionLevel::High => 19,
        }
    }
}

/// Checkpoint configuration
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Compression level to use
    pub compression: CompressionLevel,
    /// Include confidence history in checkpoint
    pub include_history: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            compression: CompressionLevel::Balanced,
            include_history: true,
        }
    }
}

/// Checkpoint a semantic matrix to disk with default configuration (DEPRECATED)
///
/// **WARNING**: This function does NOT save vocabulary and will cause token ID scrambling.
/// Use `checkpoint_matrix_with_vocab()` instead for full persistence.
///
/// Kept for backward compatibility only.
///
/// # Arguments
///
/// * `matrix` - Reference to the SemanticMatrix to checkpoint
/// * `path` - Filesystem path where the checkpoint should be written
///
/// # Returns
///
/// * `Ok(())` on successful checkpoint
/// * `Err(StorageError)` if serialization, compression, or file writing fails
pub fn checkpoint_matrix(matrix: &SemanticMatrix, path: &Path) -> Result<()> {
    // Use empty vocabulary for backward compatibility
    let empty_vocab = std::collections::HashMap::new();
    checkpoint_matrix_with_vocab(matrix, &empty_vocab, 0, path, CheckpointConfig::default())
}

/// Checkpoint a semantic matrix with vocabulary to disk (RECOMMENDED)
///
/// **This is the correct function to use for full persistence.**
/// Saves both matrix data AND vocabulary to prevent token ID scrambling on restart.
///
/// # Arguments
///
/// * `matrix` - Reference to the SemanticMatrix to checkpoint
/// * `vocabulary` - Word → token_id mapping to preserve
/// * `next_token_id` - Next available token ID for new words
/// * `path` - Filesystem path where the checkpoint should be written
/// * `config` - Checkpoint configuration
///
/// # Returns
///
/// * `Ok(())` on successful checkpoint
/// * `Err(StorageError)` if serialization, compression, or file writing fails
///
/// # Example
///
/// ```rust,ignore
/// use libmemento::storage::checkpoint_matrix_with_vocab;
/// use libmemento::matrix::SemanticMatrix;
/// use std::collections::HashMap;
/// use std::path::Path;
///
/// let matrix = SemanticMatrix::new(1000);
/// let mut vocab = HashMap::new();
/// vocab.insert("amazon".to_string(), 0);
/// vocab.insert("jeff".to_string(), 1);
/// let next_id = 2;
///
/// let path = Path::new("/tmp/matrix_checkpoint.bin");
/// checkpoint_matrix_with_vocab(&matrix, &vocab, next_id, path, Default::default())?;
/// ```
pub fn checkpoint_matrix_with_vocab(
    matrix: &SemanticMatrix,
    vocabulary: &std::collections::HashMap<String, usize>,
    next_token_id: usize,
    path: &Path,
    config: CheckpointConfig,
) -> Result<()> {
    // Extract matrix data as COO triplets
    let triplets = matrix.to_triplets().map_err(|e| {
        StorageError::SerializationError(format!("Failed to extract triplets: {}", e))
    })?;

    // Extract confidence history if requested
    let confidence_history = if config.include_history {
        matrix.confidence_history().to_vec()
    } else {
        Vec::new()
    };

    let checkpoint = MatrixCheckpoint {
        timestamp: SystemTime::now(),
        version: 2, // Bumped version for vocabulary support
        matrix_id: matrix.matrix_id,
        domain_label: matrix.domain_label.clone(),
        vocabulary_size: matrix.vocabulary_size(),
        update_count: matrix.update_count(),
        consolidation_threshold: matrix.consolidation_threshold(),
        coherence_score: matrix.coherence_score,
        created_at: matrix.created_at,
        updated_at: matrix.updated_at,
        triplets,
        confidence_history,
        query_count: matrix.query_count(),
        vocabulary: vocabulary.clone(),
        next_token_id,
    };

    // Serialize to bincode
    let serialized = bincode::serialize(&checkpoint).map_err(|e| {
        StorageError::SerializationError(format!("Bincode serialization failed: {}", e))
    })?;

    // Compress with zstd using configured level
    let compression_level = config.compression.to_zstd_level();
    let compressed = zstd::encode_all(&serialized[..], compression_level)
        .map_err(|e| StorageError::SerializationError(format!("Zstd compression failed: {}", e)))?;

    // Write to disk atomically (tmp + rename)
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, compressed)?;
    fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Checkpoint a semantic matrix to disk with custom configuration (DEPRECATED)
///
/// **WARNING**: This function does NOT save vocabulary.
/// Use `checkpoint_matrix_with_vocab()` instead.
///
/// # Arguments
///
/// * `matrix` - Reference to the SemanticMatrix to checkpoint
/// * `path` - Filesystem path where the checkpoint should be written
/// * `config` - Checkpoint configuration
///
/// # Returns
///
/// * `Ok(())` on successful checkpoint
/// * `Err(StorageError)` if serialization, compression, or file writing fails
pub fn checkpoint_matrix_with_config(
    matrix: &SemanticMatrix,
    path: &Path,
    config: CheckpointConfig,
) -> Result<()> {
    // Use empty vocabulary for backward compatibility
    let empty_vocab = std::collections::HashMap::new();
    checkpoint_matrix_with_vocab(matrix, &empty_vocab, 0, path, config)
}

/// Restore a semantic matrix from a checkpoint file (DEPRECATED)
///
/// **WARNING**: This function does NOT restore vocabulary.
/// Use `restore_matrix_with_vocab()` instead to prevent token ID scrambling.
///
/// Kept for backward compatibility only.
///
/// # Arguments
///
/// * `path` - Filesystem path to the checkpoint file
///
/// # Returns
///
/// * `Ok(SemanticMatrix)` on successful restore
/// * `Err(StorageError)` if file reading, decompression, or deserialization fails
pub fn restore_matrix(path: &Path) -> Result<SemanticMatrix> {
    let (matrix, _vocab, _next_id) = restore_matrix_with_vocab(path)?;
    Ok(matrix)
}

/// Restore a semantic matrix with vocabulary from checkpoint (RECOMMENDED)
///
/// **This is the correct function to use for full restoration.**
/// Restores both matrix data AND vocabulary to prevent token ID scrambling.
///
/// # Arguments
///
/// * `path` - Filesystem path to the checkpoint file
///
/// # Returns
///
/// * `Ok((matrix, vocabulary, next_token_id))` - Tuple of restored components
/// * `Err(StorageError)` if file reading, decompression, or deserialization fails
///
/// # Example
///
/// ```rust,ignore
/// use libmemento::storage::restore_matrix_with_vocab;
/// use std::path::Path;
///
/// let path = Path::new("/tmp/matrix_checkpoint.bin");
/// let (matrix, vocab, next_id) = restore_matrix_with_vocab(path)?;
///
/// // Now you can use vocab to tokenize queries with same IDs
/// let token_id = vocab.get("amazon"); // Returns Some(0) if amazon was token 0
/// ```
pub fn restore_matrix_with_vocab(
    path: &Path,
) -> Result<(
    SemanticMatrix,
    std::collections::HashMap<String, usize>,
    usize,
)> {
    // Read compressed data from disk
    let compressed = fs::read(path)?;

    // Decompress with zstd
    let serialized = zstd::decode_all(&compressed[..]).map_err(|e| {
        StorageError::SerializationError(format!("Zstd decompression failed: {}", e))
    })?;

    // Deserialize from bincode
    let checkpoint: MatrixCheckpoint = bincode::deserialize(&serialized).map_err(|e| {
        StorageError::SerializationError(format!("Bincode deserialization failed: {}", e))
    })?;

    // Check version compatibility
    if checkpoint.version == 1 {
        eprintln!("⚠️  Warning: Checkpoint version 1 (no vocabulary) - token IDs will be reset");
    }

    // Create a new matrix with the same configuration
    let mut matrix = SemanticMatrix::with_threshold(
        checkpoint.vocabulary_size,
        checkpoint.consolidation_threshold,
    );

    // Restore all matrix data from triplets
    for (row, col, value) in checkpoint.triplets {
        // Use add_cooccurrence but we need to prevent double-counting symmetric entries
        // Only add if row <= col to avoid symmetric duplicates
        if row <= col {
            matrix.add_cooccurrence(row, col, value).map_err(|e| {
                StorageError::SerializationError(format!(
                    "Failed to restore triplet ({}, {}): {}",
                    row, col, e
                ))
            })?;
        }
    }

    // Restore metadata
    matrix.set_update_count(checkpoint.update_count);
    matrix.set_query_count(checkpoint.query_count);

    // Restore confidence history
    for (timestamp, confidence) in checkpoint.confidence_history {
        matrix.add_confidence_record(timestamp, confidence);
    }

    // Return matrix + vocabulary + next_token_id
    Ok((matrix, checkpoint.vocabulary, checkpoint.next_token_id))
}

/// Async checkpoint manager for background matrix persistence
///
/// Manages periodic checkpointing of a semantic matrix to disk without blocking
/// concurrent read/write operations on the matrix.
pub struct CheckpointManager {
    matrix: Arc<RwLock<SemanticMatrix>>,
    checkpoint_path: PathBuf,
    config: CheckpointConfig,
}

impl CheckpointManager {
    /// Create a new CheckpointManager with default configuration
    ///
    /// # Arguments
    ///
    /// * `matrix` - Shared reference to the SemanticMatrix
    /// * `checkpoint_path` - Path where checkpoints should be written
    pub fn new(matrix: Arc<RwLock<SemanticMatrix>>, checkpoint_path: PathBuf) -> Self {
        Self {
            matrix,
            checkpoint_path,
            config: CheckpointConfig::default(),
        }
    }

    /// Create a new CheckpointManager with custom configuration
    pub fn with_config(
        matrix: Arc<RwLock<SemanticMatrix>>,
        checkpoint_path: PathBuf,
        config: CheckpointConfig,
    ) -> Self {
        Self {
            matrix,
            checkpoint_path,
            config,
        }
    }

    /// Perform a single checkpoint synchronously
    ///
    /// This method acquires a read lock on the matrix and writes it to disk.
    pub async fn checkpoint_once(&self) -> Result<()> {
        let matrix = self.matrix.read().await;
        checkpoint_matrix_with_config(&matrix, &self.checkpoint_path, self.config.clone())
    }

    /// Start a background task that periodically checkpoints the matrix
    ///
    /// The task will checkpoint the matrix at the specified interval until
    /// the returned JoinHandle is aborted or dropped.
    ///
    /// # Arguments
    ///
    /// * `interval_duration` - Time between checkpoint attempts
    ///
    /// # Returns
    ///
    /// A JoinHandle that can be used to stop the background task
    pub async fn start_background_task(&self, interval_duration: Duration) -> JoinHandle<()> {
        let matrix = self.matrix.clone();
        let checkpoint_path = self.checkpoint_path.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut timer = interval(interval_duration);

            loop {
                timer.tick().await;

                // Acquire read lock (non-blocking for other readers)
                let matrix_snapshot = matrix.read().await;

                // Checkpoint to disk with configured compression
                if let Err(e) = checkpoint_matrix_with_config(
                    &matrix_snapshot,
                    &checkpoint_path,
                    config.clone(),
                ) {
                    eprintln!("Checkpoint failed: {}", e);
                    // Continue running despite errors
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_roundtrip_metadata() {
        let matrix = SemanticMatrix::new(1000);
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("test_checkpoint_metadata.bin");

        // Checkpoint
        checkpoint_matrix(&matrix, &temp_path).unwrap();

        // Restore
        let restored = restore_matrix(&temp_path).unwrap();

        // Verify metadata
        assert_eq!(restored.vocabulary_size(), matrix.vocabulary_size());
    }
}
