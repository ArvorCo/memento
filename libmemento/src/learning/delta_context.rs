//! Delta context items for structured learning insights.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Type of delta context item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaItemType {
    /// Clicked result → reinforce token relationships
    SuccessPattern,

    /// Ignored result → weaken token relationships
    FailurePattern,

    /// Low confidence → suggest external retrieval
    KnowledgeGap,
}

/// Source of a delta update (ACE pattern)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaSource {
    /// From user click on result
    UserClick,

    /// From query reformulation
    Reformulation,

    /// From abandoned query analysis
    AbandonedQuery,

    /// From explicit feedback
    ExplicitFeedback,

    /// From synthetic interaction (bootstrap)
    Synthetic,

    /// From web retrieval (fetched external content)
    WebRetrieval,
}

/// Co-occurrence update triple (term1_id, term2_id, weight)
pub type CooccurrenceTriple = (usize, usize, f64);

/// Structured insight from reflector analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaContextItem {
    /// Unique identifier
    pub item_id: Uuid,

    /// Source query trajectory
    pub trajectory_id: Uuid,

    /// Type of insight
    pub item_type: DeltaItemType,

    /// What pattern was identified
    pub pattern_description: String,

    /// Matrix update proposal (token pairs, weights)
    pub suggested_update: serde_json::Value,

    /// Reflector confidence in this insight
    pub confidence_score: f64,

    /// Expected improvement in query quality
    pub impact_estimate: f64,

    /// Whether curator applied this update
    pub applied: bool,

    /// When update was applied
    pub applied_at: Option<SystemTime>,

    /// When insight was generated
    pub created_at: SystemTime,
}

impl DeltaContextItem {
    /// Creates a new delta context item.
    pub fn new(
        trajectory_id: Uuid,
        item_type: DeltaItemType,
        pattern_description: String,
        suggested_update: serde_json::Value,
        confidence_score: f64,
        impact_estimate: f64,
    ) -> Self {
        Self {
            item_id: Uuid::new_v4(),
            trajectory_id,
            item_type,
            pattern_description,
            suggested_update,
            confidence_score,
            impact_estimate,
            applied: false,
            applied_at: None,
            created_at: SystemTime::now(),
        }
    }

    /// Marks this item as applied.
    pub fn mark_applied(&mut self) {
        self.applied = true;
        self.applied_at = Some(SystemTime::now());
    }
}
