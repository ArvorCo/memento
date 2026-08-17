//! Agent orchestration - integrates matrix, confidence scoring, and generation.

use crate::learning::confidence::compute_query_confidence;
use crate::learning::error::{LearningError, Result};
use crate::learning::generator::Generator;
use crate::learning::reasoning_trace::{Citation, EvidenceChunk, ReasoningTrace};
use crate::matrix::SemanticMatrix;
use std::path::Path;

/// Response from processing a query.
#[derive(Debug, Clone)]
pub struct QueryResponse {
    pub answer: String,
    pub confidence: f64,
    pub triggered_retrieval: bool,
    pub retrieved_new_content: bool,
    pub matrix_updated: bool,
    pub provenance: Vec<Citation>,
    pub uncertainty_communicated: bool,
    pub reasoning_trace: Option<ReasoningTrace>,
}

/// Agent that integrates semantic matrix with query processing.
pub struct Agent {
    matrix: SemanticMatrix,
    generator: Generator,
    confidence_threshold: f64,

    // Session management (T041: Cross-Session Learning)
    session_id: String,
    query_count: usize,
    auto_checkpoint_interval: usize,
    checkpoint_path: Option<std::path::PathBuf>,
}

impl Agent {
    /// Creates a new agent with an empty semantic matrix.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::learning::Agent;
    ///
    /// let agent = Agent::with_empty_matrix();
    /// ```
    pub fn with_empty_matrix() -> Self {
        use uuid::Uuid;

        Self {
            matrix: SemanticMatrix::new(5000), // 5k vocabulary (optimized for test speed)
            generator: Generator::new(),
            confidence_threshold: 0.5,
            session_id: Uuid::new_v4().to_string(),
            query_count: 0,
            auto_checkpoint_interval: 10, // Auto-checkpoint every 10 queries
            checkpoint_path: None,
        }
    }

    /// Creates a new agent with a specific matrix and confidence threshold.
    pub fn new(matrix: SemanticMatrix, confidence_threshold: f64) -> Self {
        use uuid::Uuid;

        Self {
            matrix,
            generator: Generator::new(),
            confidence_threshold,
            session_id: Uuid::new_v4().to_string(),
            query_count: 0,
            auto_checkpoint_interval: 10,
            checkpoint_path: None,
        }
    }

    /// Creates a new agent with a checkpoint path for auto-saving.
    pub fn with_checkpoint_path(
        matrix: SemanticMatrix,
        checkpoint_path: std::path::PathBuf,
    ) -> Self {
        use uuid::Uuid;

        Self {
            matrix,
            generator: Generator::new(),
            confidence_threshold: 0.5,
            session_id: Uuid::new_v4().to_string(),
            query_count: 0,
            auto_checkpoint_interval: 10,
            checkpoint_path: Some(checkpoint_path),
        }
    }

    /// Processes a query and returns a response.
    ///
    /// This method:
    /// 1. Computes confidence score for the query
    /// 2. Triggers retrieval if confidence is below threshold
    /// 3. Generates an answer with uncertainty communication
    /// 4. Updates the matrix if new content was retrieved
    ///
    /// # Arguments
    ///
    /// * `query` - The user's query string
    ///
    /// # Returns
    ///
    /// A `QueryResponse` with the answer, confidence, and metadata
    pub async fn process_query(&mut self, query: &str) -> Result<QueryResponse> {
        // For now, this is a simplified implementation
        // Full implementation would integrate with tools crate for actual retrieval

        // Step 1: Tokenize query (simplified - hash words to token IDs)
        let query_tokens: Vec<usize> = self.tokenize(query);

        // Step 2: Compute confidence
        let confidence = if !query_tokens.is_empty() {
            compute_query_confidence(&mut self.matrix, &query_tokens)?
        } else {
            0.0
        };

        // Step 3: Determine if retrieval is needed
        let triggered_retrieval = confidence < self.confidence_threshold;
        let mut retrieved_new_content = false;
        let mut matrix_updated = false;

        // Step 4: If confidence is low, simulate retrieval (TODO: integrate with tools)
        if triggered_retrieval {
            // In full implementation, this would call tools to fetch content
            // For now, we just mark it as triggered
            retrieved_new_content = true;

            // Simulate adding new content to matrix
            if !query_tokens.is_empty() {
                self.matrix.ingest_document(&query_tokens)?;
                matrix_updated = true;
            }
        }

        // Step 5: Generate answer with uncertainty communication
        let evidence: Vec<EvidenceChunk> = vec![]; // Placeholder
        let answer = self.generator.generate_answer(&evidence, confidence);
        let uncertainty_communicated = confidence < 0.6;

        // Step 6: Create provenance (placeholder)
        let provenance = vec![];

        // Step 7: Increment query count and auto-checkpoint if needed (T041)
        self.query_count += 1;
        if let Some(ref checkpoint_path) = self.checkpoint_path {
            if self
                .query_count
                .is_multiple_of(self.auto_checkpoint_interval)
            {
                // Auto-checkpoint every N queries
                if let Err(e) = self.checkpoint(checkpoint_path).await {
                    eprintln!("Warning: Auto-checkpoint failed: {}", e);
                }
            }
        }

        Ok(QueryResponse {
            answer,
            confidence,
            triggered_retrieval,
            retrieved_new_content,
            matrix_updated,
            provenance,
            uncertainty_communicated,
            reasoning_trace: None,
        })
    }

    /// Returns a reference to the semantic matrix.
    pub fn matrix(&self) -> &SemanticMatrix {
        &self.matrix
    }

    /// Returns a mutable reference to the semantic matrix.
    pub fn matrix_mut(&mut self) -> &mut SemanticMatrix {
        &mut self.matrix
    }

    /// Saves the agent's state to a checkpoint file.
    ///
    /// This method persists the agent's semantic matrix to disk using
    /// the matrix-storage crate. The checkpoint includes:
    /// - Sparse matrix data (co-occurrences)
    /// - Metadata (vocabulary size, update counts)
    /// - Confidence history
    ///
    /// The checkpoint is serialized with bincode and compressed with zstd.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the checkpoint should be saved
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful checkpoint
    /// * `Err(LearningError)` if serialization or I/O fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::learning::Agent;
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), libmemento::learning::LearningError> {
    /// let agent = Agent::with_empty_matrix();
    /// agent.checkpoint(Path::new("/tmp/agent.bin")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn checkpoint(&self, path: &Path) -> Result<()> {
        crate::storage::checkpoint_matrix(&self.matrix, path)
            .map_err(|e| LearningError::ConfigError(format!("Checkpoint failed: {}", e)))
    }

    /// Restores an agent from a checkpoint file.
    ///
    /// This method loads a previously checkpointed agent from disk,
    /// restoring the semantic matrix with all its state:
    /// - Co-occurrence data
    /// - Vocabulary size and update counts
    /// - Confidence history
    ///
    /// A new Generator is created with default settings.
    ///
    /// # Arguments
    ///
    /// * `checkpoint_path` - Path to the checkpoint file
    ///
    /// # Returns
    ///
    /// * `Ok(Agent)` with the restored agent
    /// * `Err(LearningError)` if the file doesn't exist or deserialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::learning::Agent;
    /// use std::path::Path;
    ///
    /// # async fn example() -> Result<(), libmemento::learning::LearningError> {
    /// let agent = Agent::restore_from(Path::new("/tmp/agent.bin")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn restore_from(checkpoint_path: &Path) -> Result<Self> {
        use uuid::Uuid;

        let matrix = crate::storage::restore_matrix(checkpoint_path)
            .map_err(|e| LearningError::ConfigError(format!("Restore failed: {}", e)))?;

        Ok(Self {
            matrix,
            generator: Generator::new(),
            confidence_threshold: 0.5,              // Default threshold
            session_id: Uuid::new_v4().to_string(), // New session ID on restore
            query_count: 0,                         // Reset query count for new session
            auto_checkpoint_interval: 10,
            checkpoint_path: Some(checkpoint_path.to_path_buf()),
        })
    }

    /// Simple tokenization - converts string to token IDs using hashing
    fn tokenize(&self, text: &str) -> Vec<usize> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        text.split_whitespace()
            .map(|word| {
                let mut hasher = DefaultHasher::new();
                word.to_lowercase().hash(&mut hasher);
                (hasher.finish() % self.matrix.vocabulary_size() as u64) as usize
            })
            .collect()
    }

    /// Returns the current session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Returns the number of queries processed in this session.
    pub fn query_count(&self) -> usize {
        self.query_count
    }

    /// Sets the auto-checkpoint interval (number of queries between checkpoints).
    pub fn set_auto_checkpoint_interval(&mut self, interval: usize) {
        self.auto_checkpoint_interval = interval;
    }

    /// Sets the checkpoint path for auto-saving.
    pub fn set_checkpoint_path(&mut self, path: std::path::PathBuf) {
        self.checkpoint_path = Some(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let agent = Agent::with_empty_matrix();
        assert_eq!(agent.matrix().vocabulary_size(), 5000);
    }

    #[tokio::test]
    async fn test_process_query_basic() {
        let mut agent = Agent::with_empty_matrix();
        let response = agent.process_query("test query").await.unwrap();

        // With empty matrix, confidence should be low
        assert!(response.confidence < 0.5);
        assert!(response.triggered_retrieval);
    }
}
