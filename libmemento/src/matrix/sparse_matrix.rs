//! Sparse co-occurrence matrix implementation with hybrid storage.
//!
//! Uses TriMat (Triplet format) for fast updates and CsMat (CSR format)
//! for efficient eigenvalue computation.

mod serde_impl;
#[cfg(test)]
mod tests;

use crate::matrix::error::{Result, SemanticMatrixError};
use sprs::{CsMat, TriMat};
use std::time::SystemTime;

/// Semantic co-occurrence matrix with cached eigendecomposition.
#[derive(Debug)]
pub struct SemanticMatrix {
    /// Unique identifier for this matrix
    pub matrix_id: uuid::Uuid,

    /// Domain specialization label (e.g., "medical", "legal")
    pub domain_label: String,

    /// Triplet format for fast updates
    updates: TriMat<f64>,

    /// Compressed sparse row format for eigenvalue computation
    compressed: Option<CsMat<f64>>,

    /// Vocabulary size (number of unique tokens)
    vocabulary_size: usize,

    /// Number of updates since last consolidation
    update_count: usize,

    /// Trigger consolidation after this many updates
    consolidation_threshold: usize,

    /// Coherence score (spectral gap)
    pub coherence_score: f64,

    /// Creation timestamp
    pub created_at: SystemTime,

    /// Last modification timestamp
    pub updated_at: SystemTime,

    /// Confidence history: (timestamp, confidence_score) pairs
    /// Tracks how confidence evolves as the matrix learns
    confidence_history: Vec<(SystemTime, f64)>,

    /// Total number of queries processed
    query_count: u64,

    /// Cached eigendecomposition (invalidated on updates)
    ///
    /// Stores the most recent eigendecomposition to avoid recomputing
    /// expensive SVD on every query. Invalidated when matrix is updated.
    cached_eigen: Option<crate::matrix::eigen::EigenDecomposition>,

    /// Number of updates when eigendecomposition was last computed
    ///
    /// Used to determine if cache is stale.
    eigen_cache_update_count: usize,
}

impl SemanticMatrix {
    /// Creates a new semantic matrix with the specified vocabulary size.
    ///
    /// # Arguments
    ///
    /// * `vocabulary_size` - Maximum number of unique tokens
    ///
    /// # Example
    ///
    /// ```
    /// use libmemento::matrix::SemanticMatrix;
    ///
    /// let matrix = SemanticMatrix::new(10000);
    /// ```
    pub fn new(vocabulary_size: usize) -> Self {
        Self {
            matrix_id: uuid::Uuid::new_v4(),
            domain_label: "default".to_string(),
            updates: TriMat::new((vocabulary_size, vocabulary_size)),
            compressed: None,
            vocabulary_size,
            update_count: 0,
            consolidation_threshold: 1000, // Consolidate every 1000 updates
            coherence_score: 0.0,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            confidence_history: Vec::new(),
            query_count: 0,
            cached_eigen: None,
            eigen_cache_update_count: 0,
        }
    }

    /// Creates a new semantic matrix with custom consolidation threshold.
    ///
    /// # Arguments
    ///
    /// * `vocabulary_size` - Maximum number of unique tokens
    /// * `consolidation_threshold` - Number of updates before consolidation
    pub fn with_threshold(vocabulary_size: usize, consolidation_threshold: usize) -> Self {
        Self {
            matrix_id: uuid::Uuid::new_v4(),
            domain_label: "default".to_string(),
            updates: TriMat::new((vocabulary_size, vocabulary_size)),
            compressed: None,
            vocabulary_size,
            update_count: 0,
            consolidation_threshold,
            coherence_score: 0.0,
            created_at: SystemTime::now(),
            updated_at: SystemTime::now(),
            confidence_history: Vec::new(),
            query_count: 0,
            cached_eigen: None,
            eigen_cache_update_count: 0,
        }
    }

    /// Restores a matrix from persisted triplets without replaying incremental updates.
    ///
    /// This is the fast path for loading `.memento` snapshots and runtime segments.
    pub fn from_triplets(vocabulary_size: usize, triplets: &[(usize, usize, f64)]) -> Result<Self> {
        let now = SystemTime::now();
        let mut updates = TriMat::with_capacity((vocabulary_size, vocabulary_size), triplets.len());
        for &(row, col, value) in triplets {
            if row >= vocabulary_size || col >= vocabulary_size {
                return Err(SemanticMatrixError::TokenOutOfBounds(
                    row.max(col),
                    vocabulary_size,
                ));
            }
            updates.add_triplet(row, col, value);
        }

        let compressed = Some(updates.to_csr());
        Ok(Self {
            matrix_id: uuid::Uuid::new_v4(),
            domain_label: "default".to_string(),
            updates,
            compressed,
            vocabulary_size,
            update_count: 0,
            consolidation_threshold: 1000,
            coherence_score: 0.0,
            created_at: now,
            updated_at: now,
            confidence_history: Vec::new(),
            query_count: 0,
            cached_eigen: None,
            eigen_cache_update_count: 0,
        })
    }

    /// Returns the vocabulary size.
    pub fn vocabulary_size(&self) -> usize {
        self.vocabulary_size
    }

    /// Returns the number of non-zero entries.
    pub fn non_zero_count(&self) -> usize {
        self.updates.nnz()
    }

    /// Returns the update count (number of updates since last consolidation).
    pub fn update_count(&self) -> usize {
        self.update_count
    }

    /// Returns the consolidation threshold.
    pub fn consolidation_threshold(&self) -> usize {
        self.consolidation_threshold
    }

    /// Sets the consolidation threshold used to decide when updates should
    /// be compacted into CSR form.
    pub fn set_consolidation_threshold(&mut self, threshold: usize) {
        self.consolidation_threshold = threshold.max(1);
    }

    /// Sets the update count (used during checkpoint restoration).
    pub fn set_update_count(&mut self, count: usize) {
        self.update_count = count;
    }

    /// Sets the query count (used during checkpoint restoration).
    pub fn set_query_count(&mut self, count: u64) {
        self.query_count = count;
    }

    /// Returns the cached eigendecomposition, if present.
    pub fn cached_eigen(&self) -> Option<&crate::matrix::eigen::EigenDecomposition> {
        self.cached_eigen.as_ref()
    }

    /// Restores a cached eigendecomposition for the current matrix state.
    pub fn restore_cached_eigen(&mut self, eigen: crate::matrix::eigen::EigenDecomposition) {
        self.coherence_score = eigen.coherence_score;
        self.cached_eigen = Some(eigen);
        self.eigen_cache_update_count = self.update_count;
    }

    /// Adds a confidence record with explicit timestamp (used during checkpoint restoration).
    pub fn add_confidence_record(&mut self, timestamp: SystemTime, confidence: f64) {
        self.confidence_history.push((timestamp, confidence));
    }

    /// Extracts matrix data as COO (Coordinate) format triplets.
    ///
    /// Returns a vector of (row, col, value) tuples representing all non-zero entries.
    pub fn to_triplets(&self) -> Result<Vec<(usize, usize, f64)>> {
        let mut triplets = Vec::new();

        // Extract from updates TriMat
        for (&value, (&row, &col)) in self.updates.data().iter().zip(
            self.updates
                .row_inds()
                .iter()
                .zip(self.updates.col_inds().iter()),
        ) {
            triplets.push((row, col, value));
        }

        // If we have compressed data with pending updates, also extract those
        if let Some(ref compressed) = self.compressed {
            if self.update_count == 0 {
                // No pending updates, compressed is authoritative
                triplets.clear();
                for (value, (row, col)) in compressed.iter() {
                    triplets.push((row, col, *value));
                }
            }
        }

        Ok(triplets)
    }

    /// Adds a co-occurrence between two tokens.
    ///
    /// # Arguments
    ///
    /// * `i` - First token ID
    /// * `j` - Second token ID
    /// * `weight` - Co-occurrence weight
    ///
    /// # Returns
    ///
    /// Returns an error if token IDs are out of bounds.
    pub fn add_cooccurrence(&mut self, i: usize, j: usize, weight: f64) -> Result<()> {
        if i >= self.vocabulary_size || j >= self.vocabulary_size {
            return Err(SemanticMatrixError::TokenOutOfBounds(
                i.max(j),
                self.vocabulary_size,
            ));
        }

        // Symmetric update
        self.updates.add_triplet(i, j, weight);
        if i != j {
            self.updates.add_triplet(j, i, weight);
        }

        self.update_count += 1;

        // Invalidate eigendecomposition cache (matrix has changed)
        self.cached_eigen = None;

        // Optimization: Batch timestamp updates (amortize syscall cost)
        // Update timestamp every 100 updates instead of every update
        if self.update_count.is_multiple_of(100) {
            self.updated_at = SystemTime::now();
        }

        // Trigger consolidation if threshold reached
        if self.update_count >= self.consolidation_threshold {
            self.consolidate()?;
        }

        Ok(())
    }

    /// Ingests a document by extracting co-occurrences using a sliding window.
    ///
    /// This method processes a sequence of token IDs and registers co-occurrence
    /// relationships between tokens that appear in the same context window.
    ///
    /// # Algorithm
    ///
    /// Uses a sliding window approach where:
    /// - Window size: 5 tokens
    /// - Center token: token at index `window_size / 2`
    /// - Weight: `1.0 / window_size` for each co-occurrence
    ///
    /// For each window, the center token is paired with all other tokens in the window,
    /// creating symmetric co-occurrence entries in the matrix.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Slice of token IDs representing the document
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if any token ID is out of bounds.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::matrix::SemanticMatrix;
    ///
    /// let mut matrix = SemanticMatrix::new(1000);
    /// let document_tokens = vec![0, 1, 2, 3, 4, 5];
    /// matrix.ingest_document(&document_tokens)?;
    /// # Ok::<(), libmemento::matrix::SemanticMatrixError>(())
    /// ```
    pub fn ingest_document(&mut self, tokens: &[usize]) -> Result<()> {
        let window_size = 5;

        // If document is shorter than window size, we can't process it with standard windowing
        if tokens.len() < window_size {
            // For short documents, treat all tokens as co-occurring with each other
            for (i, &token_i) in tokens.iter().enumerate() {
                for &token_j in tokens.iter().skip(i + 1) {
                    self.add_cooccurrence(token_i, token_j, 1.0 / (tokens.len() as f64))?;
                }
            }
            return Ok(());
        }

        // Optimization: Batch collection to reduce syscall overhead
        // Hoist weight computation out of loop
        let weight = 1.0 / (window_size as f64);
        let mut batch = Vec::with_capacity(tokens.len() * window_size);

        // Collect all co-occurrences first
        for window in tokens.windows(window_size) {
            let center_idx = window_size / 2;
            let center_token = window[center_idx];

            // Register co-occurrence between center token and all context tokens
            for (idx, &context_token) in window.iter().enumerate() {
                if idx != center_idx && context_token != center_token {
                    batch.push((center_token, context_token, weight));
                }
            }
        }

        // Bulk insert with amortized timestamp update
        for (i, j, w) in batch {
            if i >= self.vocabulary_size || j >= self.vocabulary_size {
                return Err(SemanticMatrixError::TokenOutOfBounds(
                    i.max(j),
                    self.vocabulary_size,
                ));
            }

            // Symmetric update
            self.updates.add_triplet(i, j, w);
            if i != j {
                self.updates.add_triplet(j, i, w);
            }
        }

        // Update count and timestamp once
        self.update_count += tokens.len() * (window_size - 1); // Approximate
        self.updated_at = SystemTime::now();

        // Invalidate eigendecomposition cache (matrix has changed)
        self.cached_eigen = None;

        // Trigger consolidation if threshold reached
        if self.update_count >= self.consolidation_threshold {
            self.consolidate()?;
        }

        Ok(())
    }

    /// Consolidates updates from TriMat to CsMat format.
    ///
    /// This operation converts the triplet format (fast updates) to
    /// compressed sparse row format (fast linear algebra).
    pub fn consolidate(&mut self) -> Result<()> {
        self.compressed = Some(self.updates.to_csr());
        self.update_count = 0;
        Ok(())
    }

    /// Gets the compressed sparse matrix for eigenvalue computation.
    ///
    /// Triggers consolidation if necessary.
    pub fn get_compressed(&mut self) -> Result<&CsMat<f64>> {
        if self.compressed.is_none() || self.update_count > 0 {
            self.consolidate()?;
        }
        Ok(self.compressed.as_ref().unwrap())
    }

    /// Returns the compressed sparse matrix only when it is already current.
    pub fn compressed_view(&self) -> Option<&CsMat<f64>> {
        if self.update_count == 0 {
            self.compressed.as_ref()
        } else {
            None
        }
    }

    /// Computes eigenvalue decomposition with automatic algorithm selection.
    ///
    /// Returns cached eigendecomposition if available and matrix hasn't changed.
    /// Otherwise computes fresh eigendecomposition using the optimal algorithm
    /// (Lanczos for sparse matrices, Dense SVD for dense/small matrices).
    ///
    /// # Arguments
    ///
    /// * `k` - Number of top eigenvectors to compute
    ///
    /// # Returns
    ///
    /// EigenDecomposition with top-k eigenvectors and eigenvalues
    ///
    /// # Performance
    ///
    /// For 10K vocabulary, 2K non-zeros:
    /// - Lanczos: ~40ms (auto-selected)
    /// - Dense SVD: ~2000ms (fallback)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::matrix::SemanticMatrix;
    ///
    /// let mut matrix = SemanticMatrix::new(1000);
    /// // ... add co-occurrences ...
    /// let eigen = matrix.compute_eigendecomposition(100)?;
    /// println!("Coherence score: {}", eigen.coherence_score);
    /// # Ok::<(), libmemento::matrix::SemanticMatrixError>(())
    /// ```
    pub fn compute_eigendecomposition(
        &mut self,
        k: usize,
    ) -> Result<crate::matrix::eigen::EigenDecomposition> {
        use nalgebra::{DMatrix, DVector};

        // Check if we have a valid cached eigendecomposition
        if let Some(ref cached) = self.cached_eigen {
            // Cache is valid if:
            // 1. Matrix hasn't been updated since cache was computed
            // 2. Cached k is >= requested k (we can just take subset)
            if self.eigen_cache_update_count == self.update_count && cached.num_components() >= k {
                eprintln!("✓ Using cached eigendecomposition");
                return Ok(cached.clone());
            }
        }

        // Get vocabulary size and compressed matrix
        let vocab_size = self.vocabulary_size;
        let compressed = self.get_compressed()?;
        let nnz = compressed.nnz();

        eprintln!(
            "🔄 Computing eigendecomposition: {} non-zeros, k={}",
            nnz, k
        );

        // Early return for trivial case
        if nnz < 10 {
            eprintln!(
                "⚠️  Matrix too sparse ({} non-zeros) - using identity eigenspace",
                nnz
            );

            let k_actual = k.min(vocab_size);
            let eigenvectors = DMatrix::identity(vocab_size, k_actual);
            let eigenvalues = DVector::from_element(k_actual, 1.0);

            let eigen = crate::matrix::eigen::EigenDecomposition::new(eigenvectors, eigenvalues)?;
            self.cached_eigen = Some(eigen.clone());
            self.eigen_cache_update_count = self.update_count;

            return Ok(eigen);
        }

        // Auto-select optimal eigensolver based on matrix properties
        let solver = crate::matrix::eigensolver::select_eigensolver(nnz, vocab_size);
        eprintln!("🔧 Using {} solver", solver.name());

        // Compute eigendecomposition
        let eigen = solver.decompose(compressed, k)?;

        // Cache result
        self.cached_eigen = Some(eigen.clone());
        self.eigen_cache_update_count = self.update_count;

        eprintln!("✓ {} eigendecomposition computed and cached", solver.name());

        Ok(eigen)
    }

    /// Computes the coherence score of the matrix.
    ///
    /// Coherence is measured as the spectral gap: (λ₁ - λ₂)/λ₁ where λ₁ and λ₂
    /// are the largest and second-largest eigenvalues. Higher coherence indicates
    /// stronger semantic structure.
    ///
    /// # Returns
    ///
    /// Coherence score in [0, 1] range
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::matrix::SemanticMatrix;
    ///
    /// let mut matrix = SemanticMatrix::new(1000);
    /// // ... add co-occurrences ...
    /// let coherence = matrix.compute_coherence()?;
    /// println!("Matrix coherence: {:.3}", coherence);
    /// # Ok::<(), libmemento::matrix::SemanticMatrixError>(())
    /// ```
    pub fn compute_coherence(&mut self) -> Result<f64> {
        // Compute eigendecomposition with reasonable number of components
        let eigen = self.compute_eigendecomposition(10)?;

        // Extract coherence score from eigendecomposition
        // (already computed during eigendecomposition)
        Ok(eigen.coherence_score)
    }

    /// Records a confidence score in the history.
    ///
    /// # Arguments
    ///
    /// * `confidence` - Confidence score in [0, 1] range
    ///
    /// # Example
    ///
    /// ```
    /// use libmemento::matrix::SemanticMatrix;
    /// use libmemento::learning::compute_query_confidence;
    ///
    /// let mut matrix = SemanticMatrix::new(1000);
    /// // ... ingest documents ...
    /// let query = vec![10, 20, 30];
    /// let confidence = compute_query_confidence(&mut matrix, &query).unwrap();
    /// matrix.record_confidence(confidence);
    /// ```
    pub fn record_confidence(&mut self, confidence: f64) {
        self.confidence_history
            .push((SystemTime::now(), confidence));
        self.query_count += 1;
    }

    /// Returns the confidence history.
    ///
    /// # Returns
    ///
    /// A slice of (timestamp, confidence) pairs showing how confidence evolved
    pub fn confidence_history(&self) -> &[(SystemTime, f64)] {
        &self.confidence_history
    }

    /// Returns the total number of queries processed.
    pub fn query_count(&self) -> u64 {
        self.query_count
    }

    /// Returns the average confidence from recent history.
    ///
    /// # Arguments
    ///
    /// * `n` - Number of recent entries to average (default: all)
    ///
    /// # Returns
    ///
    /// Average confidence or 0.0 if history is empty
    pub fn average_confidence(&self, n: Option<usize>) -> f64 {
        if self.confidence_history.is_empty() {
            return 0.0;
        }

        let count = n.unwrap_or(self.confidence_history.len());
        let start = self.confidence_history.len().saturating_sub(count);
        let recent = &self.confidence_history[start..];

        let sum: f64 = recent.iter().map(|(_, conf)| conf).sum();
        sum / (recent.len() as f64)
    }

    /// Checks if confidence is improving over time.
    ///
    /// # Arguments
    ///
    /// * `window_size` - Number of recent queries to compare (default: 5)
    ///
    /// # Returns
    ///
    /// True if average confidence in recent window is higher than earlier window
    pub fn is_confidence_improving(&self, window_size: usize) -> bool {
        if self.confidence_history.len() < window_size * 2 {
            return false; // Not enough data
        }

        let len = self.confidence_history.len();
        let recent = &self.confidence_history[len - window_size..];
        let earlier = &self.confidence_history[len - (window_size * 2)..len - window_size];

        let recent_avg: f64 =
            recent.iter().map(|(_, conf)| conf).sum::<f64>() / (window_size as f64);
        let earlier_avg: f64 =
            earlier.iter().map(|(_, conf)| conf).sum::<f64>() / (window_size as f64);

        recent_avg > earlier_avg
    }

    /// Retrieve semantically related token pairs for a query.
    ///
    /// This is the main semantic retrieval API that integrates all retrieval components:
    /// 1. Compute eigendecomposition (cached if available)
    /// 2. Project query tokens into eigenspace
    /// 3. Score all vocabulary tokens by semantic similarity
    /// 4. Retrieve co-occurrence pairs for top-scored tokens
    /// 5. Rank pairs by combined score
    /// 6. Return top-K pairs
    ///
    /// # Arguments
    ///
    /// * `query_tokens` - Token IDs representing the query
    /// * `config` - Retrieval parameters (top_k, thresholds, weights)
    ///
    /// # Returns
    ///
    /// Vector of `RetrievalResult` sorted by relevance (descending)
    ///
    /// # Time Complexity
    ///
    /// - Eigendecomposition: O(n²k) amortized (cached)
    /// - Projection: O(|query| × k)
    /// - Scoring: O(vocab_size × k)
    /// - Retrieval: O(top_n × avg_degree)
    /// - Ranking: O(m log m) where m = retrieved pairs
    ///
    /// **Total**: O(vocab_size × k + m log m) ≈ 10-50ms for vocab_size=10K
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::matrix::{SemanticMatrix, RetrievalConfig};
    ///
    /// let mut matrix = SemanticMatrix::new(10_000);
    /// // ... populate matrix ...
    ///
    /// let query_tokens = vec![42, 123, 456];
    /// let config = RetrievalConfig::default();
    ///
    /// let pairs = matrix.retrieve_related(&query_tokens, &config)?;
    /// for pair in pairs.iter().take(10) {
    ///     println!("({}, {}): {:.3}", pair.token_a, pair.token_b, pair.relevance_score);
    /// }
    /// # Ok::<(), libmemento::matrix::SemanticMatrixError>(())
    /// ```
    pub fn retrieve_related(
        &mut self,
        query_tokens: &[usize],
        config: &crate::matrix::retrieval::RetrievalConfig,
    ) -> Result<Vec<crate::matrix::retrieval::RetrievalResult>> {
        if query_tokens.is_empty() || self.non_zero_count() == 0 {
            return Ok(Vec::new());
        }
        let k = 10.min(self.non_zero_count());
        self.compute_eigendecomposition(k)?;
        self.retrieve_related_cached(query_tokens, config)
    }

    pub fn retrieve_related_cached(
        &self,
        query_tokens: &[usize],
        config: &crate::matrix::retrieval::RetrievalConfig,
    ) -> Result<Vec<crate::matrix::retrieval::RetrievalResult>> {
        use crate::matrix::retrieval::{
            project_tokens_to_eigenspace, rank_pairs, retrieve_cooccurrence_pairs,
            score_vocabulary_tokens, select_top_tokens,
        };

        // Step 1: Early exit for empty query or matrix
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        if self.non_zero_count() == 0 {
            return Ok(Vec::new());
        }

        // Step 2: Use cached eigendecomposition only.
        let Some(eigen) = self.cached_eigen() else {
            return Ok(Vec::new());
        };
        let k = eigen.eigenvectors.ncols();
        if k == 0 {
            return Ok(Vec::new());
        }

        // Step 3: Project query into eigenspace
        let query_projection = project_tokens_to_eigenspace(query_tokens, &eigen.eigenvectors, k);

        // Step 4: Score all tokens by semantic similarity
        let token_scores = score_vocabulary_tokens(
            &query_projection,
            &eigen.eigenvectors,
            &eigen.eigenvalues,
            self.vocabulary_size(),
        );

        // Step 5: Select top-N tokens for pair retrieval
        let top_tokens = select_top_tokens(&token_scores, config.projection_k, 0.0);

        // Step 6: Build score lookup map
        let token_score_map: std::collections::HashMap<usize, f64> = token_scores
            .iter()
            .map(|ts| (ts.token_id, ts.score))
            .collect();

        // Step 7: Retrieve co-occurrence pairs
        let Some(compressed) = self.compressed_view() else {
            return Ok(Vec::new());
        };
        let pairs = retrieve_cooccurrence_pairs(&top_tokens, compressed, &token_score_map, config);

        // Step 8: Rank and return top-K
        Ok(rank_pairs(pairs, config))
    }
}
