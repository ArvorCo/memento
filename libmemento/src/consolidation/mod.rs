pub mod alignment;
pub mod error;
pub mod merge;
pub mod node;
pub mod signature;

pub use alignment::{CoherenceMonitor, MergeDecision};
pub use error::{ConsolidationError, Result};
pub use merge::{ConsolidationCheckpoint, ConsolidationEngine, ConsolidationType};
pub use node::{CoherenceState, ServerNode};
pub use signature::{CoherenceSignature, SparseVec};
