use super::*;

impl DiagnosticReporter {
    /// Creates a new diagnostic reporter with default thresholds.
    pub fn new() -> Self {
        Self {
            gap_threshold: 5,
            proposal_threshold: 10,
        }
    }

    /// Generates a comprehensive diagnostic report.
    pub fn generate_report(
        &self,
        aggregator: &FailureAggregator,
        window: TimeWindow,
    ) -> DiagnosticReport {
        let stats = aggregator.get_window_stats(window);
        let now = Utc::now();
        let start = now - window.duration();

        let failure_summary = self.build_failure_summary(aggregator, window);
        let knowledge_gaps = aggregator.identify_knowledge_gaps();
        let constitutional_proposals = self.generate_constitutional_proposals(
            &knowledge_gaps,
            &aggregator.identify_systemic_issues(),
        );
        let actionable_recommendations =
            self.generate_recommendations(&knowledge_gaps, &aggregator.identify_systemic_issues());

        let trend_analysis = stats.trend.map(|trend| TrendAnalysis {
            direction: match trend {
                FailureTrend::Increasing => "increasing".to_string(),
                FailureTrend::Decreasing => "decreasing".to_string(),
                FailureTrend::Stable => "stable".to_string(),
            },
            confidence: 0.8,
            description: format!("Failure rate trend: {:?}", trend),
        });

        DiagnosticReport {
            period: TimeRange { start, end: now },
            failure_summary,
            knowledge_gaps,
            constitutional_proposals,
            actionable_recommendations,
            trend_analysis,
        }
    }

    fn build_failure_summary(
        &self,
        aggregator: &FailureAggregator,
        window: TimeWindow,
    ) -> Vec<FailureSummaryItem> {
        let cutoff = Utc::now() - window.duration();
        let recent_failures: Vec<_> = aggregator
            .failure_history
            .iter()
            .filter(|e| e.evidence.timestamp >= cutoff)
            .collect();

        recent_failures
            .iter()
            .map(|f| FailureSummaryItem {
                query: f.evidence.query_text.clone(),
                reason: f.evidence.reason,
                timestamp: f.evidence.timestamp,
            })
            .collect()
    }

    fn generate_constitutional_proposals(
        &self,
        gaps: &[KnowledgeGap],
        issues: &[SystemicIssue],
    ) -> Vec<ConstitutionalProposal> {
        let mut proposals = Vec::new();

        for gap in gaps {
            if gap.failure_count >= self.proposal_threshold {
                proposals.push(ConstitutionalProposal {
                    proposal_type: "domain_expansion".to_string(),
                    rationale: format!(
                        "Detected {} failures in '{}' domain, indicating significant knowledge gap",
                        gap.failure_count, gap.domain
                    ),
                    proposed_principle: format!(
                        "Principle: Expand knowledge base to cover '{}' domain through corpus ingestion or external API integration",
                        gap.domain
                    ),
                    evidence_refs: gap.example_queries.clone(),
                    priority: match gap.severity {
                        GapSeverity::High => 3,
                        GapSeverity::Medium => 2,
                        GapSeverity::Low => 1,
                    },
                });
            }
        }

        for issue in issues {
            if issue.affected_count >= self.proposal_threshold {
                let (proposal_type, principle) = match issue.issue_type.as_str() {
                    "ranking_quality" => (
                        "ranking_threshold".to_string(),
                        "Principle: Increase minimum dwell time threshold for click validation from 10s to 30s".to_string(),
                    ),
                    "query_ambiguity" => (
                        "disambiguation".to_string(),
                        "Principle: Implement query expansion system to handle single-word queries with context prompts".to_string(),
                    ),
                    _ => continue,
                };

                proposals.push(ConstitutionalProposal {
                    proposal_type,
                    rationale: issue.description.clone(),
                    proposed_principle: principle,
                    evidence_refs: vec![format!("{} affected queries", issue.affected_count)],
                    priority: match issue.severity {
                        IssueSeverity::High => 3,
                        IssueSeverity::Medium => 2,
                        IssueSeverity::Low => 1,
                    },
                });
            }
        }

        proposals.sort_by_key(|p| std::cmp::Reverse(p.priority));
        proposals
    }

    fn generate_recommendations(
        &self,
        gaps: &[KnowledgeGap],
        issues: &[SystemicIssue],
    ) -> Vec<ActionableRecommendation> {
        let mut recommendations = Vec::new();

        for gap in gaps {
            recommendations.push(ActionableRecommendation {
                action: format!(
                    "Ingest domain-specific corpus for '{}' or connect external knowledge source",
                    gap.domain
                ),
                expected_impact: format!(
                    "Expected to resolve {} failures and improve coverage in '{}' domain",
                    gap.failure_count, gap.domain
                ),
                priority: match gap.severity {
                    GapSeverity::High => 3,
                    GapSeverity::Medium => 2,
                    GapSeverity::Low => 1,
                },
                estimated_effort: "Medium - requires corpus preparation and ingestion".to_string(),
            });
        }

        for issue in issues {
            let (action, impact, effort) = match issue.issue_type.as_str() {
                "ranking_quality" => (
                    "Review and tune ranking algorithm, increase dwell time threshold".to_string(),
                    format!("Expected to improve {} query results", issue.affected_count),
                    "Low - configuration change".to_string(),
                ),
                "query_ambiguity" => (
                    "Implement query expansion and disambiguation prompts".to_string(),
                    format!(
                        "Expected to clarify {} ambiguous queries",
                        issue.affected_count
                    ),
                    "High - requires new feature implementation".to_string(),
                ),
                _ => continue,
            };

            recommendations.push(ActionableRecommendation {
                action,
                expected_impact: impact,
                priority: match issue.severity {
                    IssueSeverity::High => 3,
                    IssueSeverity::Medium => 2,
                    IssueSeverity::Low => 1,
                },
                estimated_effort: effort,
            });
        }

        recommendations.sort_by_key(|r| std::cmp::Reverse(r.priority));
        recommendations
    }
}

impl Default for DiagnosticReporter {
    fn default() -> Self {
        Self::new()
    }
}
