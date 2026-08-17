use super::*;
use crate::learning::interaction_tracker::SessionStatus;
use nalgebra::DVector;
use uuid::Uuid;

fn make_trajectory(query_text: &str) -> QueryTrajectory {
    QueryTrajectory {
        query_id: Uuid::new_v4(),
        query_text: query_text.to_string(),
        query_embedding: DVector::zeros(10),
        clicks: vec![],
        reformulations: vec![],
        explicit_feedback: vec![],
        session_status: SessionStatus::Abandoned,
        created_at: Utc::now(),
    }
}

#[test]
fn test_failure_aggregator_creation() {
    let aggregator = FailureAggregator::new();
    assert_eq!(aggregator.failure_history.len(), 0);
}

#[test]
fn test_time_window_duration() {
    assert_eq!(TimeWindow::OneHour.duration(), Duration::hours(1));
    assert_eq!(TimeWindow::TwentyFourHours.duration(), Duration::hours(24));
    assert_eq!(TimeWindow::SevenDays.duration(), Duration::days(7));
}

#[test]
fn test_classify_failure_ambiguous() {
    let mut aggregator = FailureAggregator::new();
    aggregator.record_failure(make_trajectory("matrix"));

    let evidence = aggregator.get_evidence();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].reason, FailureReason::AmbiguousQuery);
}

#[test]
fn test_extract_domain_medical() {
    let mut aggregator = FailureAggregator::new();
    for _ in 0..5 {
        aggregator.record_failure(make_trajectory("medical diagnosis symptoms"));
    }

    let gaps = aggregator.identify_knowledge_gaps();
    assert!(!gaps.is_empty());
    assert_eq!(gaps[0].domain, "medical");
}

#[test]
fn test_extract_domain_unknown() {
    let mut aggregator = FailureAggregator::new();
    for _ in 0..5 {
        aggregator.record_failure(make_trajectory("random unrelated query"));
    }

    let gaps = aggregator.identify_knowledge_gaps();
    assert!(!gaps.is_empty());
    assert_eq!(gaps[0].domain, "unknown");
}
