//! Error types for knowledge consolidation

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConsolidationError {
    #[error("Matrix error: {0}")]
    MatrixError(#[from] crate::matrix::SemanticMatrixError),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("Alignment failed: {0}")]
    AlignmentError(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Invalid checkpoint: {0}")]
    InvalidCheckpoint(String),
}

pub type Result<T> = std::result::Result<T, ConsolidationError>;
