pub mod coherence;
pub mod eigen;
pub mod eigensolver;
pub mod error;
pub mod retrieval;
pub mod sparse_matrix;
pub mod update;

pub use coherence::{
    compute_frobenius_drift, compute_spectral_gap, CoherenceMonitor, CoherenceState,
};
pub use eigen::EigenDecomposition;
pub use eigensolver::{select_eigensolver, DenseSvdEigenSolver, EigenSolver, LanczosEigenSolver};
pub use error::{Result, SemanticMatrixError};
pub use retrieval::{project_tokens_to_eigenspace, RetrievalConfig, RetrievalResult, TokenScore};
pub use sparse_matrix::SemanticMatrix;
pub use update::IncrementalEigen;
