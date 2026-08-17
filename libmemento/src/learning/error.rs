//! Error types for learning engine

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LearningError {
    #[error("Matrix error: {0}")]
    MatrixError(#[from] crate::matrix::SemanticMatrixError),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Confidence computation failed: {0}")]
    ConfidenceError(String),

    #[error("Feature not yet implemented: {0}")]
    NotImplemented(String),

    #[error("Bootstrap failed: {0}")]
    BootstrapFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Generation error: {0}")]
    GenerationError(String),
}

pub type Result<T> = std::result::Result<T, LearningError>;
