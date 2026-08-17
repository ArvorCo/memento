//! Semantic retrieval from learned co-occurrence matrix.
//!
//! This module provides algorithms for retrieving semantically related token pairs
//! from a learned semantic matrix using eigenspace projections and BM25-inspired scoring.
//!
//! # Architecture
//!
//! The retrieval process follows these steps:
//! 1. **Query Projection**: Project query tokens into eigenspace
//! 2. **Vocabulary Scoring**: Score all vocabulary tokens by semantic similarity
//! 3. **Top-N Selection**: Select top-scored tokens above threshold
//! 4. **Pair Retrieval**: Retrieve co-occurrence pairs for top tokens
//! 5. **Ranking**: Rank pairs by combined score (semantic + co-occurrence)
//!
//! # Mathematical Foundation
//!
//! Given query tokens `q = [q₁, q₂, ..., qₙ]`:
//!
//! 1. **Query Projection** into eigenspace:
//!    ```text
//!    v_query = Σᵢ U[qᵢ, :] / ||Σᵢ U[qᵢ, :]||
//!    ```
//!
//! 2. **Token Scoring** via cosine similarity:
//!    ```text
//!    score(t) = cos_sim(v_query, U[t, :]) * λ₀/(Σ λᵢ)
//!    ```
//!
//! 3. **Combined Relevance**:
//!    ```text
//!    relevance = α·semantic_sim + β·cooccurrence_weight + γ·recency
//!    ```
//!    Default weights: α=0.5, β=0.3, γ=0.2
//!
//! # Performance
//!
//! - **Time Complexity**: O(vocab × k + m log m) where k=eigenspace dimensions, m=retrieved pairs
//! - **Space Complexity**: O(nnz + vocab × k) for sparse matrix + eigenvectors
//! - **Target Latency**: < 50ms (p95) for vocab_size=10K
//!
//! # Example
//!
//! ```no_run
//! use libmemento::matrix::{SemanticMatrix, RetrievalConfig};
//!
//! let mut matrix = SemanticMatrix::new(10_000);
//! // ... populate matrix ...
//!
//! let query_tokens = vec![42, 123, 456];
//! let config = RetrievalConfig::default();
//!
//! let results = matrix.retrieve_related(&query_tokens, &config)?;
//! for result in results.iter().take(10) {
//!     println!("({}, {}): {:.3}", result.token_a, result.token_b, result.relevance_score);
//! }
//! # Ok::<(), libmemento::matrix::SemanticMatrixError>(())
//! ```

use nalgebra::{DMatrix, DVector};
use sprs::CsMat;

/// Retrieved token pair with relevance scores.
///
/// Represents a semantically related pair of tokens discovered through
/// eigenspace projection and co-occurrence analysis.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    /// First token ID
    pub token_a: usize,

    /// Second token ID
    pub token_b: usize,

    /// Combined relevance score [0, 1]
    ///
    /// Computed as weighted combination of semantic similarity,
    /// co-occurrence weight, and recency (if available).
    pub relevance_score: f64,

    /// Semantic similarity to query (eigenspace) [0, 1]
    pub semantic_score: f64,

    /// Co-occurrence weight from matrix [0, ∞)
    pub cooccurrence_score: f64,
}

/// Scored token for intermediate ranking.
///
/// Used internally during vocabulary scoring phase.
#[derive(Debug, Clone)]
pub struct TokenScore {
    /// Token ID
    pub token_id: usize,

    /// Semantic similarity score [0, 1]
    pub score: f64,
}

/// Retrieval configuration parameters.
///
/// Controls the retrieval algorithm's behavior including thresholds,
/// limits, and scoring weights.
#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    /// Maximum number of results to return
    ///
    /// Algorithm will return at most this many token pairs,
    /// sorted by relevance (descending).
    pub top_k: usize,

    /// Minimum relevance score threshold [0, 1]
    ///
    /// Pairs with relevance below this threshold are filtered out.
    pub min_score: f64,

    /// Number of top tokens to consider for pair retrieval
    ///
    /// After vocabulary scoring, select top-N tokens for
    /// co-occurrence pair retrieval. Higher values = more comprehensive
    /// but slower retrieval.
    pub projection_k: usize,

    /// Recency weight [0, 1]
    ///
    /// Weight for recency factor in combined score.
    /// Currently unused (future enhancement).
    pub recency_weight: f64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 50,
            min_score: 0.1,
            projection_k: 100,
            recency_weight: 0.2,
        }
    }
}

/// Project query tokens into eigenspace.
///
/// # Algorithm
///
/// For query tokens q₁, q₂, ..., qₙ:
/// 1. Sum eigenvector contributions: v_query = Σᵢ eigenvectors[qᵢ]
/// 2. Normalize: v_query / ||v_query||
///
/// # Arguments
///
/// * `tokens` - Query token IDs
/// * `eigenvectors` - Matrix eigenvectors (vocab_size × k)
/// * `k` - Number of eigenspace dimensions
///
/// # Returns
///
/// Normalized query projection vector (k-dimensional)
///
/// # Time Complexity
///
/// O(|tokens| × k)
pub fn project_tokens_to_eigenspace(
    tokens: &[usize],
    eigenvectors: &DMatrix<f64>,
    k: usize,
) -> DVector<f64> {
    let mut projection = DVector::zeros(k);

    // Sum eigenvector contributions
    for &token_id in tokens {
        if token_id < eigenvectors.nrows() {
            for j in 0..k {
                projection[j] += eigenvectors[(token_id, j)];
            }
        }
    }

    // Normalize
    let norm = projection.norm();
    if norm > 1e-10 {
        projection /= norm;
    }

    projection
}

/// Score all vocabulary tokens by semantic similarity to query.
///
/// # Algorithm
///
/// For each token t in vocabulary:
/// 1. Extract token eigenvector: v_t = eigenvectors[t, :]
/// 2. Compute cosine similarity: sim = cos(v_query, v_t)
/// 3. Weight by eigenvalue importance: score = sim × λ₀/(Σ λᵢ)
///
/// # Arguments
///
/// * `query_projection` - Query vector in eigenspace
/// * `eigenvectors` - Matrix eigenvectors
/// * `eigenvalues` - Matrix eigenvalues (for weighting)
/// * `vocab_size` - Total vocabulary size
///
/// # Returns
///
/// Vector of `TokenScore` (token_id, score) pairs
///
/// # Time Complexity
///
/// O(vocab_size × k)
pub fn score_vocabulary_tokens(
    query_projection: &DVector<f64>,
    eigenvectors: &DMatrix<f64>,
    eigenvalues: &DVector<f64>,
    vocab_size: usize,
) -> Vec<TokenScore> {
    let k = query_projection.len();
    let mut scores = Vec::with_capacity(vocab_size);

    // Eigenvalue weighting (emphasize dominant modes)
    let eigenvalue_sum: f64 = eigenvalues.iter().sum();
    let eigenvalue_weight = if eigenvalue_sum > 1e-10 {
        eigenvalues[0] / eigenvalue_sum
    } else {
        1.0
    };

    for token_id in 0..vocab_size {
        if token_id >= eigenvectors.nrows() {
            break;
        }

        // Extract token eigenvector as owned DVector
        let token_projection: DVector<f64> =
            DVector::from_iterator(k, (0..k).map(|j| eigenvectors[(token_id, j)]));

        // Compute cosine similarity
        let similarity = cosine_similarity_dvector(query_projection, &token_projection);

        // Weight by eigenvalue importance
        let weighted_score = similarity * eigenvalue_weight;

        scores.push(TokenScore {
            token_id,
            score: weighted_score,
        });
    }

    scores
}

/// Select top-N tokens above similarity threshold.
///
/// # Arguments
///
/// * `scores` - Token scores from vocabulary scoring
/// * `top_n` - Number of top tokens to select
/// * `min_similarity` - Minimum score threshold
///
/// # Returns
///
/// Top-N scored tokens sorted by score (descending)
///
/// # Time Complexity
///
/// O(vocab × log(vocab)) for sorting
pub fn select_top_tokens(
    scores: &[TokenScore],
    top_n: usize,
    min_similarity: f64,
) -> Vec<TokenScore> {
    let mut filtered: Vec<TokenScore> = scores
        .iter()
        .filter(|ts| ts.score >= min_similarity)
        .cloned()
        .collect();

    // Sort by score descending
    filtered.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Take top-N
    filtered.into_iter().take(top_n).collect()
}

/// Retrieve co-occurrence pairs for top tokens.
///
/// # Algorithm
///
/// For each top-scored token t:
/// 1. Find all (t, u) pairs with M[t, u] > threshold
/// 2. Compute combined semantic score: (score(t) + score(u))/2
/// 3. Compute relevance: α·semantic + β·cooccurrence
///
/// # Arguments
///
/// * `top_tokens` - Top-scored tokens from vocabulary
/// * `compressed` - Compressed sparse matrix (CSR format)
/// * `token_scores` - Score lookup map
/// * `config` - Retrieval configuration
///
/// # Returns
///
/// Vector of `RetrievalResult` pairs
///
/// # Time Complexity
///
/// O(top_n × avg_degree)
pub fn retrieve_cooccurrence_pairs(
    top_tokens: &[TokenScore],
    compressed: &CsMat<f64>,
    token_scores: &std::collections::HashMap<usize, f64>,
    config: &RetrievalConfig,
) -> Vec<RetrievalResult> {
    let mut pairs = Vec::new();

    // Scoring weights
    let alpha = 0.5; // Semantic weight
    let beta = 0.3; // Co-occurrence weight
    let _gamma = config.recency_weight; // Recency (future)

    for token_score in top_tokens {
        let token_a = token_score.token_id;
        let score_a = token_score.score;

        // Get row for this token (if exists)
        if token_a >= compressed.rows() {
            continue;
        }

        let row = compressed.outer_view(token_a).unwrap();

        // Iterate over non-zero co-occurrences
        for (token_b, &weight) in row.iter() {
            if weight < config.min_score {
                continue;
            }

            // Get semantic score for token_b
            let score_b = token_scores.get(&token_b).copied().unwrap_or(0.0);

            // Combined semantic similarity
            let semantic_similarity = (score_a + score_b) / 2.0;

            // Normalize co-occurrence weight to [0, 1] (simple sigmoid)
            let normalized_cooc = weight / (1.0 + weight);

            // Combined relevance score
            let relevance_score = alpha * semantic_similarity + beta * normalized_cooc;

            pairs.push(RetrievalResult {
                token_a,
                token_b,
                relevance_score,
                semantic_score: semantic_similarity,
                cooccurrence_score: normalized_cooc,
            });
        }
    }

    pairs
}

/// Rank pairs by combined score and return top-K.
///
/// # Arguments
///
/// * `pairs` - Retrieved pairs to rank
/// * `config` - Retrieval configuration
///
/// # Returns
///
/// Top-K pairs sorted by relevance (descending)
///
/// # Time Complexity
///
/// O(m log m) where m = number of pairs
pub fn rank_pairs(
    mut pairs: Vec<RetrievalResult>,
    config: &RetrievalConfig,
) -> Vec<RetrievalResult> {
    // Sort by relevance_score descending
    pairs.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Filter by min_score and return top-K
    pairs
        .into_iter()
        .filter(|p| p.relevance_score >= config.min_score)
        .take(config.top_k)
        .collect()
}

/// Compute cosine similarity between two DVector instances.
///
/// # Arguments
///
/// * `a` - First vector
/// * `b` - Second vector
///
/// # Returns
///
/// Cosine similarity in [-1, 1], clamped
///
/// # Time Complexity
///
/// O(k) where k = vector dimension
pub fn cosine_similarity_dvector(a: &DVector<f64>, b: &DVector<f64>) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot = a.dot(b);
    let norm_a = a.norm();
    let norm_b = b.norm();

    if norm_a < 1e-10 || norm_b < 1e-10 {
        0.0
    } else {
        (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = DVector::from_vec(vec![1.0, 0.0, 0.0]);
        let b = DVector::from_vec(vec![1.0, 0.0, 0.0]);

        let sim = cosine_similarity_dvector(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical vectors should have similarity 1.0"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = DVector::from_vec(vec![1.0, 0.0, 0.0]);
        let b = DVector::from_vec(vec![0.0, 1.0, 0.0]);

        let sim = cosine_similarity_dvector(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "Orthogonal vectors should have similarity 0.0"
        );
    }

    #[test]
    fn test_project_tokens_normalizes() {
        let mut eigenvectors = DMatrix::zeros(10, 3);
        eigenvectors[(0, 0)] = 1.0;
        eigenvectors[(1, 1)] = 1.0;

        let tokens = vec![0, 1];
        let projection = project_tokens_to_eigenspace(&tokens, &eigenvectors, 3);

        // Should be normalized
        let norm = projection.norm();
        assert!((norm - 1.0).abs() < 1e-6, "Projection should be normalized");
    }

    #[test]
    fn test_select_top_tokens_filters_and_limits() {
        let scores = vec![
            TokenScore {
                token_id: 0,
                score: 0.9,
            },
            TokenScore {
                token_id: 1,
                score: 0.5,
            },
            TokenScore {
                token_id: 2,
                score: 0.3,
            },
            TokenScore {
                token_id: 3,
                score: 0.1,
            },
        ];

        let top = select_top_tokens(&scores, 2, 0.4);

        assert_eq!(top.len(), 2, "Should select top 2");
        assert_eq!(top[0].token_id, 0, "Top token should have highest score");
        assert_eq!(top[1].token_id, 1, "Second token should be next highest");
    }

    #[test]
    fn test_rank_pairs_sorts_descending() {
        let pairs = vec![
            RetrievalResult {
                token_a: 0,
                token_b: 1,
                relevance_score: 0.5,
                semantic_score: 0.5,
                cooccurrence_score: 0.5,
            },
            RetrievalResult {
                token_a: 2,
                token_b: 3,
                relevance_score: 0.9,
                semantic_score: 0.9,
                cooccurrence_score: 0.9,
            },
            RetrievalResult {
                token_a: 4,
                token_b: 5,
                relevance_score: 0.3,
                semantic_score: 0.3,
                cooccurrence_score: 0.3,
            },
        ];

        let config = RetrievalConfig {
            top_k: 10,
            min_score: 0.0,
            projection_k: 100,
            recency_weight: 0.0,
        };

        let ranked = rank_pairs(pairs, &config);

        assert_eq!(ranked.len(), 3);
        assert!(
            ranked[0].relevance_score >= ranked[1].relevance_score,
            "Should be sorted descending"
        );
        assert!(
            ranked[1].relevance_score >= ranked[2].relevance_score,
            "Should be sorted descending"
        );
    }
}
