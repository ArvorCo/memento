use super::text::{split_sentences, tokenize_terms};
use super::*;

#[test]
fn test_generator_creation() {
    let generator = Generator::new();
    assert_eq!(generator.concept_similarity_threshold, 0.3);
    assert_eq!(generator.max_concepts_per_answer, 10);
    assert_eq!(generator.max_sentences, 4);
}

#[test]
fn test_generator_custom_config() {
    let generator = Generator::with_config(0.5, 20, 10);
    assert_eq!(generator.concept_similarity_threshold, 0.5);
    assert_eq!(generator.max_concepts_per_answer, 20);
    assert_eq!(generator.max_sentences, 10);
}

#[test]
fn test_tokenize_terms_filters_noise() {
    let terms = tokenize_terms("What did we decide about Memento security and auth?");
    assert!(terms.contains(&"memento".to_string()));
    assert!(terms.contains(&"security".to_string()));
    assert!(!terms.contains(&"what".to_string()));
    assert!(!terms.contains(&"about".to_string()));
}

#[test]
fn test_simple_synthesis_legacy() {
    let generator = Generator::new();
    let evidence = vec![
        EvidenceChunk {
            chunk_id: "1".to_string(),
            source_document_id: "doc1".to_string(),
            text: "Evidence one.".to_string(),
            retrieval_score: 0.9,
            relevance_score: 0.8,
        },
        EvidenceChunk {
            chunk_id: "2".to_string(),
            source_document_id: "doc1".to_string(),
            text: "Evidence two.".to_string(),
            retrieval_score: 0.7,
            relevance_score: 0.6,
        },
    ];

    let answer = generator.generate_answer(&evidence, 0.8);
    assert!(!answer.is_empty());
    assert!(answer.contains("Evidence"));
}

#[test]
fn test_sentence_splitter_keeps_multiline_bullets_separate() {
    let text =
        "- Sprint shipped the parser\n- Security review found two blockers\n- Revenue stayed flat";

    let sentences = split_sentences(text);

    assert_eq!(sentences.len(), 3);
    assert!(sentences[0].contains("Sprint shipped"));
    assert!(sentences[1].contains("Security review"));
}

#[test]
fn test_sentence_splitter_preserves_decimals_and_labels_markdown_tables() {
    let text = "Cobalt achieved 13.5 ms p50. It passed.\n\n| experiment | latency_ms | result |\n| --- | --- | --- |\n| cobalt-hummingbird | 13.5 | pass |";

    let sentences = split_sentences(text);

    assert!(sentences.contains(&"Cobalt achieved 13.5 ms p50.".to_string()));
    assert!(sentences
        .contains(&"experiment: cobalt-hummingbird; latency ms: 13.5; result: pass".to_string()));
}

#[test]
fn test_semantic_synthesis_covers_distinct_facts_and_cites_sources() {
    let generator = Generator::new();
    let evidence = vec![EvidenceChunk {
        chunk_id: "1".to_string(),
        source_document_id: "/vault/sprint-nine.md".to_string(),
        text: "Project Atlas council was scheduled. Search became operational. Risk kept the conservative threshold. Security review found critical issues.".to_string(),
        retrieval_score: 0.9,
        relevance_score: 0.9,
    }];

    let answer = generator.synthesize_multi_sentence(
        "what happened in the sprint?",
        &evidence,
        &[
            "atlas".into(),
            "search".into(),
            "risk".into(),
            "security".into(),
        ],
        0.7,
    );

    for expected in ["Atlas", "Search", "Risk", "Security"] {
        assert!(answer.contains(expected), "missing {expected}: {answer}");
    }
    assert!(answer.contains("[Sources: sprint-nine.md]"));
}

#[test]
fn test_semantic_evidence_ranking_preserves_retrieval_authority() {
    use crate::matrix::EigenDecomposition;
    use nalgebra::{DMatrix, DVector};

    let generator = Generator::new();
    let evidence = vec![
        EvidenceChunk {
            chunk_id: "best".to_string(),
            source_document_id: "canonical.md".to_string(),
            text: "Canonical evidence without repeated query words.".to_string(),
            retrieval_score: 0.9,
            relevance_score: 0.9,
        },
        EvidenceChunk {
            chunk_id: "similar".to_string(),
            source_document_id: "similar.md".to_string(),
            text: "product product priorities catalog catalog".to_string(),
            retrieval_score: 0.8,
            relevance_score: 0.8,
        },
    ];
    let eigen = EigenDecomposition {
        eigenvalues: DVector::zeros(1),
        eigenvectors: DMatrix::zeros(1, 1),
        coherence_score: 0.0,
    };

    let ranked = generator.rank_evidence_by_semantics(
        &evidence,
        &[],
        &eigen,
        &["product".to_string(), "catalog".to_string()],
    );

    assert_eq!(ranked[0].chunk_id, "best");
}
