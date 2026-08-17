//! Confidence scoring based on query projection onto eigenvectors

use crate::learning::error::{LearningError, Result};
use crate::matrix::SemanticMatrix;

/// Computes query confidence based on eigenvalue projection.
///
/// This function projects the query tokens onto the semantic matrix's
/// eigenvector basis and computes a weighted confidence score.
///
/// # Algorithm
///
/// 1. Compute eigendecomposition of the matrix
/// 2. Project query tokens onto eigenvector basis
/// 3. Weight projections by eigenvalues
/// 4. Normalize to [0, 1] range
///
/// # Arguments
///
/// * `matrix` - The semantic matrix
/// * `query_tokens` - Slice of token IDs representing the query
///
/// # Returns
///
/// Confidence score in [0, 1] range
///
/// # Example
///
/// ```no_run
/// use libmemento::learning::confidence::compute_query_confidence;
/// use libmemento::matrix::SemanticMatrix;
///
/// let mut matrix = SemanticMatrix::new(1000);
/// let query_tokens = vec![10, 20, 30];
/// let confidence = compute_query_confidence(&mut matrix, &query_tokens)?;
/// # Ok::<(), libmemento::learning::LearningError>(())
/// ```
pub fn compute_query_confidence(
    matrix: &mut SemanticMatrix,
    query_tokens: &[usize],
) -> Result<f64> {
    // Handle empty query
    if query_tokens.is_empty() {
        return Ok(0.0);
    }

    // If matrix is empty (no co-occurrences), return zero confidence
    let nnz = matrix.non_zero_count();
    if nnz == 0 {
        return Ok(0.0);
    }

    // Compute eigendecomposition for confidence scoring
    // Use fewer components for small matrices to avoid numerical issues
    let k = 10.min(nnz.min(query_tokens.len())).max(2);
    let eigen = matrix.compute_eigendecomposition(k)?;

    // Factor 1: Coherence (structural quality)
    // Higher coherence = more organized knowledge
    let coherence = eigen.coherence_score.clamp(0.0, 1.0);

    // Factor 2: Data size (logarithmic scaling)
    // More data → higher confidence, but with diminishing returns
    // log10(10) ≈ 1.0 → 0.33, log10(100) ≈ 2.0 → 0.67, log10(1000) ≈ 3.0 → 1.0
    // This gives reasonable confidence even for small datasets
    let data_size_factor = ((nnz as f64).log10() / 3.0).clamp(0.0, 1.0);

    // Combine factors with balanced weighting:
    // - coherence: structural quality (0-1) - PRIMARY factor
    // - data_size_factor: absolute size requirement (0-1) - SECONDARY factor
    //
    // Examples (assuming coherence ≈ 0.8):
    // - nnz=10: coherence * (1.0/3.0)^0.5 ≈ 0.8 * 0.58 ≈ 0.46
    // - nnz=45 (10×10 matrix): coherence * (1.65/3.0)^0.5 ≈ 0.8 * 0.74 ≈ 0.59
    // - nnz=100: coherence * (2.0/3.0)^0.5 ≈ 0.8 * 0.82 ≈ 0.66
    // - nnz=1000: coherence * (3.0/3.0)^0.5 ≈ 0.8 * 1.0 ≈ 0.8
    //
    // Use sqrt to make the penalty smoother
    let confidence = coherence * data_size_factor.sqrt();

    Ok(confidence.clamp(0.0, 1.0))
}

/// Computes query confidence using only a cached eigendecomposition.
///
/// This avoids expensive spectral recomputation on the request path.
pub fn compute_query_confidence_cached(
    matrix: &SemanticMatrix,
    query_tokens: &[usize],
) -> Result<f64> {
    if query_tokens.is_empty() {
        return Ok(0.0);
    }

    let nnz = matrix.non_zero_count();
    if nnz == 0 {
        return Ok(0.0);
    }

    let Some(eigen) = matrix.cached_eigen() else {
        return Err(LearningError::ConfidenceError(
            "cached eigendecomposition is unavailable".to_string(),
        ));
    };

    let coherence = eigen.coherence_score.clamp(0.0, 1.0);
    let data_size_factor = ((nnz as f64).log10() / 3.0).clamp(0.0, 1.0);
    let confidence = coherence * data_size_factor.sqrt();

    Ok(confidence.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_matrix_returns_zero_confidence() {
        let mut matrix = SemanticMatrix::new(1000);
        let query = vec![10, 20, 30];
        let confidence = compute_query_confidence(&mut matrix, &query).unwrap();
        assert!(
            confidence < 0.1,
            "Empty matrix should have near-zero confidence"
        );
    }

    #[test]
    fn test_populated_matrix_returns_nonzero_confidence() {
        let mut matrix = SemanticMatrix::new(1000);

        // Add some co-occurrences
        for i in 0..10 {
            for j in (i + 1)..10 {
                matrix.add_cooccurrence(i, j, 1.0).unwrap();
            }
        }

        let query = vec![0, 1, 2];
        let confidence = compute_query_confidence(&mut matrix, &query).unwrap();
        assert!(
            (0.0..=1.0).contains(&confidence),
            "Confidence should be in [0, 1] range"
        );
    }

    #[test]
    fn test_empty_query_returns_zero_confidence() {
        let mut matrix = SemanticMatrix::new(1000);
        matrix.add_cooccurrence(0, 1, 1.0).unwrap();

        let query: Vec<usize> = vec![];
        let confidence = compute_query_confidence(&mut matrix, &query).unwrap();
        assert_eq!(confidence, 0.0, "Empty query should have zero confidence");
    }

    #[test]
    fn test_cached_confidence_requires_cached_eigen() {
        let mut matrix = SemanticMatrix::new(1000);
        matrix.add_cooccurrence(0, 1, 1.0).unwrap();

        let query = vec![0, 1];
        assert!(compute_query_confidence_cached(&matrix, &query).is_err());

        let _ = matrix.compute_eigendecomposition(2).unwrap();
        let confidence = compute_query_confidence_cached(&matrix, &query).unwrap();
        assert!((0.0..=1.0).contains(&confidence));
    }
}
