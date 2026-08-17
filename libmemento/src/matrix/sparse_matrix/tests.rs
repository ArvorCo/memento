use super::*;

#[test]
fn test_new_matrix() {
    let matrix = SemanticMatrix::new(1000);
    assert_eq!(matrix.vocabulary_size(), 1000);
    assert_eq!(matrix.non_zero_count(), 0);
}

#[test]
fn test_add_cooccurrence() {
    let mut matrix = SemanticMatrix::new(100);
    matrix.add_cooccurrence(0, 1, 1.0).unwrap();
    assert_eq!(matrix.non_zero_count(), 2);
}

#[test]
fn test_out_of_bounds() {
    let mut matrix = SemanticMatrix::new(100);
    let result = matrix.add_cooccurrence(200, 1, 1.0);
    assert!(result.is_err());
}

#[test]
fn test_confidence_tracking() {
    let mut matrix = SemanticMatrix::new(100);

    assert_eq!(matrix.query_count(), 0);
    assert_eq!(matrix.confidence_history().len(), 0);

    matrix.record_confidence(0.2);
    assert_eq!(matrix.query_count(), 1);
    assert_eq!(matrix.confidence_history().len(), 1);

    matrix.record_confidence(0.5);
    matrix.record_confidence(0.8);
    assert_eq!(matrix.query_count(), 3);
    assert_eq!(matrix.confidence_history().len(), 3);

    let avg = matrix.average_confidence(None);
    assert!(
        (avg - 0.5).abs() < 0.01,
        "Average should be ~0.5, got {}",
        avg
    );

    let recent_avg = matrix.average_confidence(Some(2));
    assert!(
        (recent_avg - 0.65).abs() < 0.01,
        "Recent average should be ~0.65, got {}",
        recent_avg
    );
}

#[test]
fn test_confidence_improving() {
    let mut matrix = SemanticMatrix::new(100);

    matrix.record_confidence(0.1);
    matrix.record_confidence(0.2);
    assert!(
        !matrix.is_confidence_improving(2),
        "Not enough data for window_size=2"
    );

    matrix.record_confidence(0.3);
    matrix.record_confidence(0.4);
    matrix.record_confidence(0.5);
    matrix.record_confidence(0.6);

    assert!(
        matrix.is_confidence_improving(2),
        "Confidence should be improving"
    );
}

#[test]
fn test_confidence_not_improving() {
    let mut matrix = SemanticMatrix::new(100);

    matrix.record_confidence(0.9);
    matrix.record_confidence(0.8);
    matrix.record_confidence(0.5);
    matrix.record_confidence(0.4);

    assert!(
        !matrix.is_confidence_improving(2),
        "Confidence should not be improving"
    );
}

#[test]
fn test_compute_coherence() {
    let mut matrix = SemanticMatrix::new(100);

    for i in 0..10 {
        for j in (i + 1)..10 {
            matrix.add_cooccurrence(i, j, 1.0).unwrap();
        }
    }

    let coherence = matrix.compute_coherence().unwrap();

    assert!(
        (0.0..=1.0).contains(&coherence),
        "Coherence should be in [0, 1] range, got {}",
        coherence
    );
    assert!(
        coherence > 0.1,
        "Structured matrix should have coherence > 0.1, got {}",
        coherence
    );
}

#[test]
fn test_retrieve_related_cached_requires_cached_eigen() {
    let mut matrix = SemanticMatrix::new(100);
    matrix.add_cooccurrence(0, 1, 1.0).unwrap();
    matrix.add_cooccurrence(1, 2, 1.0).unwrap();

    let config = crate::matrix::RetrievalConfig::default();
    let empty = matrix.retrieve_related_cached(&[0, 1], &config).unwrap();
    assert!(empty.is_empty());

    let _ = matrix.compute_eigendecomposition(2).unwrap();
    let related = matrix.retrieve_related_cached(&[0, 1], &config).unwrap();
    assert!(!related.is_empty());
}

#[test]
fn test_from_triplets_restores_sparse_matrix_without_replay() {
    let triplets = vec![(0, 1, 1.0), (1, 0, 1.0), (1, 2, 0.5), (2, 1, 0.5)];
    let matrix = SemanticMatrix::from_triplets(8, &triplets).unwrap();

    assert_eq!(matrix.non_zero_count(), 4);
    assert!(matrix.compressed_view().is_some());
    assert_eq!(matrix.to_triplets().unwrap().len(), 4);
}
