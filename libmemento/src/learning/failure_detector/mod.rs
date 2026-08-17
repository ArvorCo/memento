//! Failure Pattern Detection and Diagnostic System (T083).
//!
//! Implements FR-026-030:
//! - Detects patterns in failed queries (no clicks, abandoned, low dwell time)
//! - Classifies failure reasons: Ambiguous, Out-of-domain, Low confidence, Poor ranking
//! - Aggregates failure patterns across time windows (1h, 24h, 7d)
//! - Generates actionable diagnostics with evidence
//!
//! Success Criteria (SC-019-020):
//! - Gap detection accuracy ≥80%
//! - Diagnostic proposal quality validates correctly

mod aggregator;
mod reporter;
#[cfg(test)]
mod tests;

use crate::learning::interaction_tracker::QueryTrajectory;
use crate::learning::reflector::ace_patterns::{
    FailurePatternDetector as ACEFailureDetector, FailureReason,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Time window for failure aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeWindow {
    /// Last hour.
    OneHour,
    /// Last 24 hours.
    TwentyFourHours,
    /// Last 7 days.
    SevenDays,
}

impl TimeWindow {
    /// Gets the duration for this time window.
    pub fn duration(&self) -> Duration {
        match self {
            TimeWindow::OneHour => Duration::hours(1),
            TimeWindow::TwentyFourHours => Duration::hours(24),
            TimeWindow::SevenDays => Duration::days(7),
        }
    }
}

/// Failure aggregator - Tracks and analyzes failure patterns over time.
#[derive(Debug)]
pub struct FailureAggregator {
    /// History of failed trajectories.
    failure_history: VecDeque<FailureEvent>,
    /// ACE pattern detector for classification (reserved for future advanced classification).
    #[allow(dead_code)]
    pattern_detector: ACEFailureDetector,
    /// Maximum history size.
    max_history_size: usize,
}

/// Failure event with evidence.
#[derive(Debug, Clone)]
pub struct FailureEvent {
    pub trajectory: QueryTrajectory,
    pub evidence: FailureEvidence,
    pub detected_at: DateTime<Utc>,
}

/// Evidence for a single failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvidence {
    pub query_text: String,
    pub query_id: uuid::Uuid,
    pub reason: FailureReason,
    pub timestamp: DateTime<Utc>,
    pub had_clicks: bool,
    pub had_reformulations: bool,
    pub avg_dwell_time: f64,
}

/// Failure statistics for a time window.
#[derive(Debug, Clone)]
pub struct WindowStats {
    pub total_failures: usize,
    pub failure_rate: f64,
    pub most_common_reason: Option<FailureReason>,
    pub trend: Option<FailureTrend>,
}

/// Failure trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureTrend {
    Increasing,
    Decreasing,
    Stable,
}

/// Knowledge gap identified from failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    pub domain: String,
    pub failure_count: usize,
    pub example_queries: Vec<String>,
    pub severity: GapSeverity,
}

/// Gap severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapSeverity {
    Low,
    Medium,
    High,
}

/// Systemic issue identified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemicIssue {
    pub issue_type: String,
    pub description: String,
    pub affected_count: usize,
    pub severity: IssueSeverity,
}

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
}

/// Actionable diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub diagnostic_type: DiagnosticType,
    pub title: String,
    pub description: String,
    pub evidence: Vec<String>,
    pub recommendation: String,
    pub severity: DiagnosticSeverity,
}

/// Type of diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticType {
    KnowledgeGap,
    SystemicIssue,
    PerformanceDegradation,
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Diagnostic reporter - Generates comprehensive reports from failure data.
#[derive(Debug, Clone)]
pub struct DiagnosticReporter {
    /// Minimum failures to trigger knowledge gap identification (reserved for future filtering).
    #[allow(dead_code)]
    gap_threshold: usize,
    /// Minimum failures to trigger constitutional proposal.
    proposal_threshold: usize,
}

/// Complete diagnostic report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// Time period covered by this report.
    pub period: TimeRange,
    /// Summary of all failures in the period.
    pub failure_summary: Vec<FailureSummaryItem>,
    /// Identified knowledge gaps.
    pub knowledge_gaps: Vec<KnowledgeGap>,
    /// Proposed constitutional updates.
    pub constitutional_proposals: Vec<ConstitutionalProposal>,
    /// Actionable recommendations.
    pub actionable_recommendations: Vec<ActionableRecommendation>,
    /// Trend analysis.
    pub trend_analysis: Option<TrendAnalysis>,
}

/// Time range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Summary item for a single failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureSummaryItem {
    pub query: String,
    pub reason: FailureReason,
    pub timestamp: DateTime<Utc>,
}

/// Constitutional proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstitutionalProposal {
    /// Type of proposal (e.g., "domain_expansion", "ranking_threshold").
    pub proposal_type: String,
    /// Rationale for this proposal.
    pub rationale: String,
    /// Proposed principle text.
    pub proposed_principle: String,
    /// References to supporting evidence.
    pub evidence_refs: Vec<String>,
    /// Priority (1=low, 2=medium, 3=high).
    pub priority: u8,
}

/// Actionable recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionableRecommendation {
    /// Recommended action to take.
    pub action: String,
    /// Expected impact of this action.
    pub expected_impact: String,
    /// Priority (1=low, 2=medium, 3=high).
    pub priority: u8,
    /// Estimated effort required.
    pub estimated_effort: String,
}

/// Trend analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    /// Direction: "increasing", "decreasing", "stable".
    pub direction: String,
    /// Confidence in trend analysis [0, 1].
    pub confidence: f64,
    /// Textual description.
    pub description: String,
}
