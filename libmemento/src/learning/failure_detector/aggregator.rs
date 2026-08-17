use super::*;
use std::collections::HashMap;

impl FailureAggregator {
    /// Creates a new failure aggregator.
    pub fn new() -> Self {
        Self {
            failure_history: VecDeque::with_capacity(10000),
            pattern_detector: ACEFailureDetector::default(),
            max_history_size: 10000,
        }
    }

    /// Records a failed query trajectory.
    pub fn record_failure(&mut self, trajectory: QueryTrajectory) {
        let reason = self.classify_failure(&trajectory);

        let evidence = FailureEvidence {
            query_text: trajectory.query_text.clone(),
            query_id: trajectory.query_id,
            reason,
            timestamp: trajectory.created_at,
            had_clicks: !trajectory.clicks.is_empty(),
            had_reformulations: !trajectory.reformulations.is_empty(),
            avg_dwell_time: if trajectory.clicks.is_empty() {
                0.0
            } else {
                trajectory
                    .clicks
                    .iter()
                    .map(|c| c.dwell_time_seconds)
                    .sum::<f64>()
                    / trajectory.clicks.len() as f64
            },
        };

        let event = FailureEvent {
            trajectory,
            evidence,
            detected_at: Utc::now(),
        };

        self.failure_history.push_back(event);
        if self.failure_history.len() > self.max_history_size {
            self.failure_history.pop_front();
        }
    }

    /// Gets failure statistics for a time window.
    pub fn get_window_stats(&self, window: TimeWindow) -> WindowStats {
        let cutoff = Utc::now() - window.duration();
        let failures: Vec<_> = self
            .failure_history
            .iter()
            .filter(|e| e.evidence.timestamp >= cutoff)
            .collect();

        let total_failures = failures.len();
        let mut reason_counts: HashMap<FailureReason, usize> = HashMap::new();
        for failure in &failures {
            *reason_counts.entry(failure.evidence.reason).or_insert(0) += 1;
        }

        let most_common_reason = reason_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&reason, _)| reason);

        let hours = window.duration().num_hours() as f64;
        let failure_rate = if hours > 0.0 {
            total_failures as f64 / hours
        } else {
            0.0
        };

        let trend = if total_failures >= 4 {
            let mid = total_failures / 2;
            let first_half = mid;
            let second_half = total_failures - mid;

            if second_half > first_half {
                Some(FailureTrend::Increasing)
            } else if second_half < first_half {
                Some(FailureTrend::Decreasing)
            } else {
                Some(FailureTrend::Stable)
            }
        } else {
            None
        };

        WindowStats {
            total_failures,
            failure_rate,
            most_common_reason,
            trend,
        }
    }

    /// Gets pattern summary (count by reason).
    pub fn get_pattern_summary(&self) -> HashMap<FailureReason, usize> {
        let mut summary = HashMap::new();
        for event in &self.failure_history {
            *summary.entry(event.evidence.reason).or_insert(0) += 1;
        }
        summary
    }

    /// Gets evidence for all failures (for diagnostic reporting).
    pub fn get_evidence(&self) -> Vec<&FailureEvidence> {
        self.failure_history.iter().map(|e| &e.evidence).collect()
    }

    /// Identifies knowledge gaps (domains with high failure rates).
    pub fn identify_knowledge_gaps(&self) -> Vec<KnowledgeGap> {
        let mut domain_failures: HashMap<String, Vec<&FailureEvidence>> = HashMap::new();

        for event in &self.failure_history {
            if event.evidence.reason == FailureReason::OutOfDomain {
                let domain = self.extract_domain(&event.evidence.query_text);
                domain_failures
                    .entry(domain)
                    .or_default()
                    .push(&event.evidence);
            }
        }

        let mut gaps = Vec::new();
        for (domain, failures) in domain_failures {
            if failures.len() >= 5 {
                gaps.push(KnowledgeGap {
                    domain: domain.clone(),
                    failure_count: failures.len(),
                    example_queries: failures
                        .iter()
                        .take(3)
                        .map(|e| e.query_text.clone())
                        .collect(),
                    severity: if failures.len() >= 15 {
                        GapSeverity::High
                    } else if failures.len() >= 10 {
                        GapSeverity::Medium
                    } else {
                        GapSeverity::Low
                    },
                });
            }
        }

        gaps.sort_by_key(|g| std::cmp::Reverse(g.failure_count));
        gaps
    }

    /// Identifies systemic issues (poor ranking, ambiguity, etc.).
    pub fn identify_systemic_issues(&self) -> Vec<SystemicIssue> {
        let mut issues = Vec::new();
        let summary = self.get_pattern_summary();

        let low_dwell_failures = self
            .failure_history
            .iter()
            .filter(|e| e.evidence.had_clicks && e.evidence.avg_dwell_time < 10.0)
            .count();

        if low_dwell_failures >= 10 {
            issues.push(SystemicIssue {
                issue_type: "ranking_quality".to_string(),
                description: format!(
                    "Poor ranking quality detected: {} queries with clicks but low dwell time (<10s)",
                    low_dwell_failures
                ),
                affected_count: low_dwell_failures,
                severity: if low_dwell_failures >= 20 {
                    IssueSeverity::High
                } else {
                    IssueSeverity::Medium
                },
            });
        }

        if let Some(&ambiguous_count) = summary.get(&FailureReason::AmbiguousQuery) {
            if ambiguous_count >= 10 {
                issues.push(SystemicIssue {
                    issue_type: "query_ambiguity".to_string(),
                    description: format!(
                        "High query ambiguity: {} single-word or vague queries failing",
                        ambiguous_count
                    ),
                    affected_count: ambiguous_count,
                    severity: IssueSeverity::Medium,
                });
            }
        }

        issues
    }

    /// Generates actionable diagnostics.
    pub fn generate_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for gap in self.identify_knowledge_gaps() {
            diagnostics.push(Diagnostic {
                diagnostic_type: DiagnosticType::KnowledgeGap,
                title: format!("Knowledge gap in '{}' domain", gap.domain),
                description: format!(
                    "Detected {} failures in '{}' domain. System lacks knowledge in this area.",
                    gap.failure_count, gap.domain
                ),
                evidence: gap
                    .example_queries
                    .iter()
                    .map(|q| format!("Query: '{}'", q))
                    .collect(),
                recommendation: format!(
                    "Recommendation: Ingest domain-specific corpus for '{}' or connect external knowledge source.",
                    gap.domain
                ),
                severity: match gap.severity {
                    GapSeverity::High => DiagnosticSeverity::High,
                    GapSeverity::Medium => DiagnosticSeverity::Medium,
                    GapSeverity::Low => DiagnosticSeverity::Low,
                },
            });
        }

        for issue in self.identify_systemic_issues() {
            diagnostics.push(Diagnostic {
                diagnostic_type: DiagnosticType::SystemicIssue,
                title: issue.issue_type.clone(),
                description: issue.description.clone(),
                evidence: vec![format!("{} queries affected", issue.affected_count)],
                recommendation: match issue.issue_type.as_str() {
                    "ranking_quality" => "Recommendation: Review ranking algorithm, increase dwell time threshold, or improve result relevance scoring.".to_string(),
                    "query_ambiguity" => "Recommendation: Implement query expansion, provide disambiguation prompts, or strengthen context extraction.".to_string(),
                    _ => "Recommendation: Further analysis needed.".to_string(),
                },
                severity: match issue.severity {
                    IssueSeverity::High => DiagnosticSeverity::High,
                    IssueSeverity::Medium => DiagnosticSeverity::Medium,
                    IssueSeverity::Low => DiagnosticSeverity::Low,
                },
            });
        }

        diagnostics
    }

    fn classify_failure(&self, trajectory: &QueryTrajectory) -> FailureReason {
        if trajectory.query_text.split_whitespace().count() == 1 {
            return FailureReason::AmbiguousQuery;
        }

        if trajectory.clicks.is_empty() {
            return FailureReason::OutOfDomain;
        }

        FailureReason::LowConfidence
    }

    fn extract_domain(&self, query: &str) -> String {
        let keywords = [
            "medical",
            "medicine",
            "health",
            "legal",
            "law",
            "court",
            "physics",
            "quantum",
            "science",
            "computer",
            "programming",
            "software",
            "finance",
            "economics",
            "market",
        ];

        for keyword in &keywords {
            if query.to_lowercase().contains(keyword) {
                return keyword.to_string();
            }
        }

        "unknown".to_string()
    }
}

impl Default for FailureAggregator {
    fn default() -> Self {
        Self::new()
    }
}
