use super::*;

#[test]
fn test_compute_effective_learning_rate_with_high_confidence() {
    let curator = AdaptiveCurator::new(0.01, 0.5, 2.0);
    let rate = curator.compute_effective_learning_rate(0.9, false);
    assert!((rate - 0.012).abs() < 0.001);
}

#[test]
fn test_compute_effective_learning_rate_with_distribution_shift() {
    let curator = AdaptiveCurator::new(0.01, 0.5, 2.0);
    let rate = curator.compute_effective_learning_rate(0.9, true);
    assert!((rate - 0.024).abs() < 0.001);
}

#[test]
fn test_validator_rejects_low_confidence_edits() {
    use crate::learning::delta_context::{DeltaContextItem, DeltaItemType};
    use uuid::Uuid;

    let validator = EditValidator::new(0.5);
    let item = DeltaContextItem::new(
        Uuid::new_v4(),
        DeltaItemType::KnowledgeGap,
        "test".to_string(),
        serde_json::json!({}),
        0.3,
        0.1,
    );

    assert!(!validator.validate_quality(&item));
}

#[test]
fn test_success_tracker_calculates_rate() {
    let mut tracker = UpdateSuccessTracker::new();
    tracker.record_success();
    tracker.record_success();
    tracker.record_failure();

    assert_eq!(tracker.success_rate(), 2.0 / 3.0);
}

#[test]
fn test_ace_curator_creation() {
    let curator = ACECurator::new();
    assert_eq!(curator.learning_phase(), &LearningPhase::Babbling);
}

#[test]
fn test_ace_curator_learning_phase_transitions() {
    let mut curator = ACECurator::new();
    assert_eq!(curator.learning_phase(), &LearningPhase::Babbling);
    curator.update_learning_phase(0.5);
    assert_eq!(curator.learning_phase(), &LearningPhase::FirstWords);
    curator.update_learning_phase(0.8);
    assert_eq!(curator.learning_phase(), &LearningPhase::Grammar);
}

#[test]
fn test_ace_curator_adaptive_rate() {
    let curator = ACECurator::new();
    let rate_babbling = curator.get_adaptive_rate(0.2, 0.8);
    assert!(
        rate_babbling > 0.1,
        "Babbling rate should be high: {}",
        rate_babbling
    );

    let mut curator = ACECurator::new();
    curator.update_learning_phase(0.9);
    let rate_grammar = curator.get_adaptive_rate(0.9, 0.8);
    assert!(
        rate_grammar < rate_babbling,
        "Grammar rate should be lower than Babbling"
    );
}

#[test]
fn test_ace_curator_apply_delta() {
    use crate::learning::delta_context::DeltaSource;

    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);

    let delta = ACEDelta {
        source: DeltaSource::UserClick,
        co_occurrences: vec![(0, 1, 1.0), (1, 2, 1.0)],
        confidence: 0.8,
        learning_rate_multiplier: 1.0,
    };

    let result = curator.apply_delta_update(&mut matrix, &delta, None);
    assert!(result.is_ok(), "Delta update should succeed");

    let metrics = result.unwrap();
    assert_eq!(metrics.applied_count, 2, "Should apply 2 co-occurrences");
    assert!(
        metrics.learning_rate_used > 0.0,
        "Should use positive learning rate"
    );
}

#[test]
fn test_eigenspace_stabilizer() {
    let mut stabilizer = EigenspaceStabilizer::new();
    assert!(!stabilizer.needs_recomputation());
    stabilizer.record_drift(0.05);
    assert!(!stabilizer.needs_recomputation(), "Below threshold");
    stabilizer.record_drift(0.06);
    assert!(stabilizer.needs_recomputation(), "Above threshold");
    stabilizer.reset();
    assert!(!stabilizer.needs_recomputation(), "After reset");
}

#[test]
fn test_phase_aware_learning_rate() {
    let rate_config = PhaseAwareLearningRate::new();
    let babbling_rate = rate_config.get_rate_for_phase(&LearningPhase::Babbling);
    let first_words_rate = rate_config.get_rate_for_phase(&LearningPhase::FirstWords);
    let grammar_rate = rate_config.get_rate_for_phase(&LearningPhase::Grammar);

    assert_eq!(babbling_rate, 0.5);
    assert_eq!(first_words_rate, 0.1);
    assert_eq!(grammar_rate, 0.01);
    assert!(babbling_rate > first_words_rate);
    assert!(first_words_rate > grammar_rate);
}

#[test]
fn test_ace_delta_update_formula() {
    use crate::learning::delta_context::DeltaSource;

    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);
    matrix.coherence_score = 0.5;

    let delta = ACEDelta {
        source: DeltaSource::UserClick,
        co_occurrences: vec![(0, 1, 1.0), (1, 2, 0.5)],
        confidence: 0.8,
        learning_rate_multiplier: 1.0,
    };

    let result = curator.apply_delta_update(&mut matrix, &delta, None);
    assert!(result.is_ok(), "Delta update should succeed");

    let metrics = result.unwrap();
    assert_eq!(metrics.applied_count, 2, "Should apply 2 co-occurrences");
    assert!(
        metrics.learning_rate_used > 0.0,
        "Should use positive learning rate"
    );
    assert!(
        metrics.learning_rate_used < 1.0,
        "Learning rate should be bounded"
    );
}

#[test]
fn test_ace_drift_tracking() {
    use crate::learning::delta_context::DeltaSource;

    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);
    matrix.coherence_score = 0.8;

    let delta = ACEDelta {
        source: DeltaSource::UserClick,
        co_occurrences: vec![(0, 1, 1.0)],
        confidence: 0.9,
        learning_rate_multiplier: 1.0,
    };

    let result = curator.apply_delta_update(&mut matrix, &delta, None);
    assert!(result.is_ok(), "Update should succeed");

    let metrics = result.unwrap();
    assert!(
        metrics.eigenvector_drift >= 0.0,
        "Drift should be non-negative"
    );
}

#[test]
fn test_ace_collapse_prevention_integration() {
    use crate::learning::delta_context::DeltaSource;
    use crate::matrix::EigenDecomposition;
    use nalgebra::{DMatrix, DVector};

    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);
    matrix.coherence_score = 0.8;

    let eigenvectors = DMatrix::identity(100, 10);
    let eigenvalues = DVector::from_vec((0..10).rev().map(|i| (i + 1) as f64).collect());
    let eigen = EigenDecomposition::new(eigenvectors, eigenvalues).unwrap();

    let delta = ACEDelta {
        source: DeltaSource::UserClick,
        co_occurrences: vec![(0, 1, 0.1)],
        confidence: 0.8,
        learning_rate_multiplier: 1.0,
    };

    let result = curator.apply_delta_update(&mut matrix, &delta, Some(&eigen));
    assert!(
        result.is_ok(),
        "Update with collapse prevention should succeed"
    );
}

#[test]
fn test_strategic_guideline_synonym() {
    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);

    let guideline = StrategicGuideline {
        guideline_type: GuidelineType::Synonym {
            source_token: 0,
            target_token: 1,
            strength: 1.0,
        },
        confidence: 0.9,
    };

    let result = curator.apply_strategic_guidelines(&mut matrix, &[guideline]);
    assert!(
        result.is_ok(),
        "Strategic guideline should apply successfully"
    );

    let metrics = result.unwrap();
    assert_eq!(metrics.len(), 1, "Should have one metric result");
    assert_eq!(
        metrics[0].applied_count, 1,
        "Should apply one co-occurrence"
    );
}

#[test]
fn test_strategic_guideline_category_membership() {
    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);

    let guideline = StrategicGuideline {
        guideline_type: GuidelineType::CategoryMembership {
            item_token: 0,
            category_tokens: vec![10, 11, 12],
            strength: 0.8,
        },
        confidence: 0.85,
    };

    let result = curator.apply_strategic_guidelines(&mut matrix, &[guideline]);
    assert!(result.is_ok(), "Category guideline should apply");

    let metrics = result.unwrap();
    assert_eq!(
        metrics[0].applied_count, 3,
        "Should apply 3 category co-occurrences"
    );
}

#[test]
fn test_strategic_guideline_relational_pattern() {
    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);

    let guideline = StrategicGuideline {
        guideline_type: GuidelineType::RelationalPattern {
            token_pairs: vec![(0, 1), (2, 3), (4, 5)],
            strength: 1.0,
        },
        confidence: 0.95,
    };

    let result = curator.apply_strategic_guidelines(&mut matrix, &[guideline]);
    assert!(result.is_ok(), "Relational pattern should apply");

    let metrics = result.unwrap();
    assert_eq!(
        metrics[0].applied_count, 3,
        "Should apply 3 relational pairs"
    );
}

#[test]
fn test_eigenspace_recomputation_trigger() {
    let mut curator = ACECurator::new();

    assert!(
        !curator.needs_eigenspace_recomputation(),
        "Initially no recomputation needed"
    );

    for _ in 0..20 {
        curator.eigenspace_stabilizer.record_drift(0.01);
    }

    assert!(
        curator.needs_eigenspace_recomputation(),
        "Should need recomputation after drift accumulation"
    );
    curator.reset_eigenspace_stabilizer();
    assert!(
        !curator.needs_eigenspace_recomputation(),
        "Should not need recomputation after reset"
    );
}

#[test]
fn test_ace_learning_rate_phase_aware() {
    let mut curator = ACECurator::new();

    let rate_babbling = curator.get_adaptive_rate(0.2, 0.8);
    curator.update_learning_phase(0.5);
    let rate_first_words = curator.get_adaptive_rate(0.5, 0.8);
    curator.update_learning_phase(0.8);
    let rate_grammar = curator.get_adaptive_rate(0.8, 0.8);

    assert!(
        rate_babbling > rate_first_words,
        "Babbling rate ({}) should be > FirstWords rate ({})",
        rate_babbling,
        rate_first_words
    );
    assert!(
        rate_first_words > rate_grammar,
        "FirstWords rate ({}) should be > Grammar rate ({})",
        rate_first_words,
        rate_grammar
    );
}

#[test]
fn test_ace_multiple_deltas_application() {
    use crate::learning::delta_context::DeltaSource;

    let mut curator = ACECurator::new();
    let mut matrix = SemanticMatrix::new(100);
    matrix.coherence_score = 0.6;

    let deltas = vec![
        ACEDelta {
            source: DeltaSource::UserClick,
            co_occurrences: vec![(0, 1, 1.0)],
            confidence: 0.8,
            learning_rate_multiplier: 1.0,
        },
        ACEDelta {
            source: DeltaSource::Reformulation,
            co_occurrences: vec![(1, 2, 0.8)],
            confidence: 0.9,
            learning_rate_multiplier: 1.5,
        },
        ACEDelta {
            source: DeltaSource::ExplicitFeedback,
            co_occurrences: vec![(2, 3, 1.0)],
            confidence: 0.95,
            learning_rate_multiplier: 1.0,
        },
    ];

    let mut total_applied = 0;
    for delta in deltas {
        let result = curator.apply_delta_update(&mut matrix, &delta, None);
        assert!(result.is_ok(), "Each delta should apply successfully");
        total_applied += result.unwrap().applied_count;
    }

    assert_eq!(total_applied, 3, "Should apply all 3 deltas");
    assert_eq!(
        curator.success_tracker().successes,
        3,
        "Should track 3 successes"
    );
}
