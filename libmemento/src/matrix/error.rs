//! Error types for semantic matrix operations.

use thiserror::Error;

/// Result type alias for semantic matrix operations.
pub type Result<T> = std::result::Result<T, SemanticMatrixError>;

/// Errors that can occur during semantic matrix operations.
#[derive(Error, Debug)]
pub enum SemanticMatrixError {
    /// Matrix dimensions are invalid or incompatible.
    #[error("Invalid matrix dimensions: {0}")]
    InvalidDimensions(String),

    /// Token ID is out of bounds for the vocabulary.
    #[error("Token ID {0} out of bounds (vocabulary size: {1})")]
    TokenOutOfBounds(usize, usize),

    /// Eigenvalue decomposition failed.
    #[error("Eigenvalue decomposition failed: {0}")]
    EigenDecompositionFailed(String),

    /// Matrix invariant violated (e.g., symmetry broken).
    #[error("Matrix invariant violated: {0}")]
    InvariantViolation(String),

    /// Coherence score below production threshold.
    #[error("Coherence score {0} below threshold {1}")]
    LowCoherence(f64, f64),

    /// Excessive eigenvector drift detected.
    #[error("Eigenvector drift {0} exceeds threshold {1}")]
    ExcessiveDrift(f64, f64),

    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Generic error for other failures.
    #[error("Semantic matrix error: {0}")]
    Other(String),
}
