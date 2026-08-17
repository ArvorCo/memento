//! Curator: Update knowledge structures based on insights.
//!
//! ACE-enhanced adaptive curator with ATLAS-inspired learning rate that scales with
//! query confidence and distribution shifts. Implements the ACE Curator pattern from
//! Stanford's arXiv:2510.04618 with Early Experience bootstrap support.

mod ace;
mod adaptive;
mod support;
#[cfg(test)]
mod tests;

use crate::learning::bootstrap::LearningPhase;
use crate::learning::delta_context::{
    CooccurrenceTriple, DeltaContextItem, DeltaItemType, DeltaSource,
};
use crate::learning::reflector::ace_patterns::ACEDelta;
use crate::learning::reflector::collapse_prevention::CollapsePreventionSystem;
use crate::learning::reflector::DisambiguationEdit;
use crate::matrix::{EigenDecomposition, SemanticMatrix};
use serde::{Deserialize, Serialize};

/// ACE Curator: Delta updates + coherence preservation + bootstrap support.
#[derive(Debug, Clone)]
pub struct ACECurator {
    // ATLAS adaptive learning (baseline)
    base_learning_rate: f64,
    confidence_multiplier: f64,
    distribution_shift_boost: f64,
    validator: EditValidator,
    success_tracker: UpdateSuccessTracker,

    // ACE enhancements
    collapse_preventer: CollapsePreventionSystem,
    eigenspace_stabilizer: EigenspaceStabilizer,

    // Bootstrap support
    learning_phase: LearningPhase,
    phase_aware_rate: PhaseAwareLearningRate,
}

/// Adaptive curator with ATLAS-inspired learning rate (legacy interface).
#[derive(Debug, Clone)]
pub struct AdaptiveCurator {
    /// Base learning rate for matrix updates.
    base_learning_rate: f64,
    /// Multiplier for confidence-based scaling.
    confidence_multiplier: f64,
    /// Boost factor when distribution shifts.
    distribution_shift_boost: f64,
    /// Validator for edit quality.
    validator: EditValidator,
    /// Success tracker for update outcomes.
    success_tracker: UpdateSuccessTracker,
}

/// Edit validator using ATLAS speculative validation.
#[derive(Debug, Clone)]
pub struct EditValidator {
    /// Minimum quality threshold.
    quality_threshold: f64,
}

/// Tracks success/failure of updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSuccessTracker {
    pub total_attempts: usize,
    pub successes: usize,
    pub failures: usize,
    pub rejected: usize,
}

/// Legacy curator (stub for backward compatibility).
#[derive(Debug, Clone)]
pub struct Curator {}

/// Eigenspace stabilizer (ACE-specific).
///
/// Monitors eigenspace stability and triggers recomputation when needed.
#[derive(Debug, Clone)]
pub struct EigenspaceStabilizer {
    /// Accumulated drift since last recomputation.
    accumulated_drift: f64,
    /// Drift threshold for triggering recomputation.
    drift_threshold: f64,
}

/// Phase-aware learning rate (Early Experience bootstrap).
///
/// Provides adaptive learning rates based on the current learning phase.
#[derive(Debug, Clone)]
pub struct PhaseAwareLearningRate {
    babbling_rate: f64,
    first_words_rate: f64,
    grammar_rate: f64,
}

/// Update metrics returned by ACE curator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMetrics {
    /// Spectral gap (context density measure).
    pub spectral_gap: f64,
    /// Eigenvector drift (Frobenius norm).
    pub eigenvector_drift: f64,
    /// Learning rate used for this update.
    pub learning_rate_used: f64,
    /// Number of co-occurrences applied.
    pub applied_count: usize,
}

/// Strategic guideline for high-level knowledge injection.
#[derive(Debug, Clone)]
pub struct StrategicGuideline {
    /// Type of guideline.
    pub guideline_type: GuidelineType,
    /// Confidence in this guideline [0, 1].
    pub confidence: f64,
}

/// Types of strategic guidelines.
#[derive(Debug, Clone)]
pub enum GuidelineType {
    /// Synonym relationship: strengthen connection between two terms.
    Synonym {
        source_token: usize,
        target_token: usize,
        strength: f64,
    },
    /// Category membership: associate item with category terms.
    CategoryMembership {
        item_token: usize,
        category_tokens: Vec<usize>,
        strength: f64,
    },
    /// Relational pattern: strengthen connections between related term pairs.
    RelationalPattern {
        token_pairs: Vec<(usize, usize)>,
        strength: f64,
    },
}

/// Curator errors.
#[derive(Debug, Clone)]
pub enum CuratorError {
    /// Collapse prevention rejected the update.
    CollapsePrevention(String),
    /// No updates were applied (all rejected or invalid).
    NoUpdatesApplied,
    /// Invalid delta format.
    InvalidDeltaFormat(String),
    /// Matrix operation failed.
    MatrixError(String),
}
