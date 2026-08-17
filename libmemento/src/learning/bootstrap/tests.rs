use super::*;

fn create_test_doc(id: &str, tokens: Vec<&str>) -> Document {
    Document {
        id: id.to_string(),
        title: format!("Test Doc {}", id),
        content: tokens.join(" "),
        tokens: tokens.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn test_learning_phase_rates() {
    assert_eq!(LearningPhase::Babbling.learning_rate(), 0.5);
    assert_eq!(LearningPhase::FirstWords.learning_rate(), 0.1);
    assert_eq!(LearningPhase::Grammar.learning_rate(), 0.01);
}

#[test]
fn test_learning_phase_thresholds() {
    assert_eq!(LearningPhase::Babbling.transition_threshold(), 0.3);
    assert_eq!(LearningPhase::FirstWords.transition_threshold(), 0.7);
    assert_eq!(LearningPhase::Grammar.transition_threshold(), 1.0);
}

#[test]
fn test_bootstrap_engine_creation() {
    let corpus = vec![create_test_doc("1", vec!["test", "document"])];
    let engine = BootstrapEngine::new(corpus, 1000).unwrap();

    assert_eq!(engine.learning_phase, LearningPhase::Babbling);
    assert_eq!(engine.current_coherence(), 0.0);
    assert_eq!(engine.coherence_history.len(), 0);
}

#[test]
fn test_get_learning_rate_adaptive() {
    let corpus = vec![create_test_doc("1", vec!["test"])];
    let engine = BootstrapEngine::new(corpus, 1000).unwrap();

    assert_eq!(engine.get_learning_rate(0.1), 0.5);
    assert_eq!(engine.get_learning_rate(0.5), 0.1);
    assert_eq!(engine.get_learning_rate(0.8), 0.01);
}

#[test]
fn test_tokenization() {
    let corpus = vec![create_test_doc("1", vec!["test", "document"])];
    let mut engine = BootstrapEngine::new(corpus, 1000).unwrap();

    let token_ids = engine.tokenize("test document");
    assert_eq!(token_ids.len(), 2);
}

#[test]
fn test_query_execution() {
    let corpus = vec![
        create_test_doc("1", vec!["semantic", "matrix"]),
        create_test_doc("2", vec!["learning", "engine"]),
        create_test_doc("3", vec!["semantic", "learning"]),
    ];

    let mut engine = BootstrapEngine::new(corpus, 1000).unwrap();
    engine.build_vocabulary().unwrap();

    let results = engine.execute_query("semantic learning").unwrap();
    assert!(!results.is_empty());
    assert!(results[0].doc_id == "3" || results[0].score > 0.5);
}
