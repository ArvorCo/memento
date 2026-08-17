//! Dual-Reflector System for Trajectory Analysis
//!
//! ATLAS-inspired architecture with static + adaptive reflectors
//! and confidence-based controller selection.
//!
//! ACE Enhancements (arXiv:2510.04618):
//! - Success pattern extraction (clicks → co-occurrence deltas)
//! - Failure pattern detection (abandoned queries → corrective deltas)
//! - Context collapse prevention (Frobenius norm validation)
//!
//! Early Experience Enhancements (arXiv:2510.08558):
//! - Implicit world model (query-result pattern learning)
//! - Self-reflection engine (learn from failures without ground truth)
//! - Bootstrap support (adaptive learning rates across phases)

use crate::learning::interaction_tracker::QueryTrajectory;
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub mod ace_patterns;
pub mod adaptive;
pub mod collapse_prevention;
pub mod controller;
pub mod predictor;
pub mod self_reflection;
pub mod static_reflector;
pub mod world_model;

pub use ace_patterns::{ACEDelta, FailurePatternDetector, FailureReason, SuccessPatternAnalyzer};
pub use adaptive::AdaptiveReflector;
pub use collapse_prevention::{CollapsePreventionSystem, CollapseRisk};
pub use controller::ReflectorController;
pub use predictor::EditQualityPredictor;
pub use self_reflection::{FailureHypothesis, SelfImprovementEdit, SuboptimalActionAnalyzer};
pub use static_reflector::StaticReflector;
pub use world_model::{QueryPattern, QueryResultWorldModel, ResultPattern};

/// Legacy reflector (stub for backward compatibility)
#[derive(Debug, Clone)]
pub struct Reflector {
    // Configuration fields will be added in implementation
}

impl Reflector {
    /// Creates a new reflector.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Reflector {
    fn default() -> Self {
        Self::new()
    }
}

/// ACE Reflector - Complete implementation with pattern extraction and collapse prevention
#[derive(Debug)]
pub struct ACEReflector {
    // ATLAS dual-reflector (baseline) - reserved for future adaptive selection
    _static_reflector: StaticReflector,
    _adaptive_reflector: AdaptiveReflector,
    _controller: ReflectorController,

    // ACE pattern extraction (core)
    success_pattern_extractor: SuccessPatternAnalyzer,
    failure_pattern_detector: FailurePatternDetector,
    context_collapse_preventer: CollapsePreventionSystem,

    // Early Experience enhancements
    implicit_world_model: QueryResultWorldModel,
    self_reflection_engine: SuboptimalActionAnalyzer,

    // Bootstrap state
    learning_phase: LearningPhase,
    coherence_history: VecDeque<f64>,
}

impl ACEReflector {
    /// Creates a new ACE reflector with default configuration
    pub fn new() -> Self {
        Self {
            _static_reflector: StaticReflector::new(),
            _adaptive_reflector: AdaptiveReflector::new(0.01),
            _controller: ReflectorController::new(0.6, 0.15),
            success_pattern_extractor: SuccessPatternAnalyzer::default(),
            failure_pattern_detector: FailurePatternDetector::default(),
            context_collapse_preventer: CollapsePreventionSystem::default(),
            implicit_world_model: QueryResultWorldModel::default(),
            self_reflection_engine: SuboptimalActionAnalyzer::default(),
            learning_phase: LearningPhase::Babbling,
            coherence_history: VecDeque::with_capacity(100),
        }
    }

    /// Extracts success patterns from a trajectory with clicks (ACE core method)
    ///
    /// # ACE Pattern
    /// Click → Extract (query_term, result_term) pairs → Build co-occurrence deltas
    pub fn extract_success_patterns(&self, trajectory: &QueryTrajectory) -> Vec<ACEDelta> {
        self.success_pattern_extractor.extract_patterns(trajectory)
    }

    /// Detects failure patterns from abandoned or reformulated queries (ACE core method)
    ///
    /// # ACE Pattern
    /// Abandoned query → Diagnose reason → Generate corrective deltas
    pub fn detect_failure_patterns(&self, trajectory: &QueryTrajectory) -> Vec<ACEDelta> {
        self.failure_pattern_detector.detect_patterns(trajectory)
    }

    /// Prevents context collapse via eigenspace stability check (ACE core method)
    ///
    /// # ACE Validation
    /// - Check Frobenius norm drift < 0.1
    /// - Check spectral gap > 0.3 (minimum coherence)
    pub fn prevent_context_collapse(
        &self,
        delta: &ACEDelta,
        current_eigenvectors: &DMatrix<f64>,
        current_eigenvalues: &DVector<f64>,
    ) -> Result<(), CollapseRisk> {
        let learning_rate = self.get_adaptive_rate();
        self.context_collapse_preventer.validate_delta(
            delta,
            current_eigenvectors,
            current_eigenvalues,
            learning_rate,
        )
    }

    /// Builds implicit world model from interaction (Early Experience)
    pub fn build_world_model(&mut self, trajectory: &QueryTrajectory) {
        if trajectory.clicks.is_empty() {
            return;
        }

        let query_pattern = self
            .implicit_world_model
            .extract_query_pattern(&trajectory.query_text);

        for click in &trajectory.clicks {
            let result_pattern = self
                .implicit_world_model
                .extract_result_pattern(&click.result_id, 0.8);
            let dwell_time_ms = (click.dwell_time_seconds * 1000.0) as u64;

            self.implicit_world_model.learn_mapping(
                query_pattern.clone(),
                result_pattern,
                dwell_time_ms,
            );
        }
    }

    /// Self-reflects on failures without ground truth (Early Experience)
    pub fn reflect_on_failures(
        &mut self,
        trajectory: &QueryTrajectory,
    ) -> Vec<SelfImprovementEdit> {
        self.self_reflection_engine.reflect_on_failure(trajectory)
    }

    /// Gets adaptive learning rate based on bootstrap phase
    pub fn get_adaptive_rate(&self) -> f64 {
        match self.learning_phase {
            LearningPhase::Babbling => 0.5,
            LearningPhase::FirstWords => 0.1,
            LearningPhase::Grammar => 0.01,
        }
    }

    /// Updates learning phase based on coherence
    pub fn update_learning_phase(&mut self, current_coherence: f64) {
        self.coherence_history.push_back(current_coherence);
        if self.coherence_history.len() > 100 {
            self.coherence_history.pop_front();
        }

        let avg_coherence = if self.coherence_history.is_empty() {
            0.0
        } else {
            self.coherence_history.iter().sum::<f64>() / self.coherence_history.len() as f64
        };

        self.learning_phase = match avg_coherence {
            c if c < 0.3 => LearningPhase::Babbling,
            c if c < 0.7 => LearningPhase::FirstWords,
            _ => LearningPhase::Grammar,
        };
    }

    /// Gets current learning phase
    pub fn get_learning_phase(&self) -> LearningPhase {
        self.learning_phase
    }

    /// Gets world model pattern count (for monitoring)
    pub fn world_model_patterns(&self) -> usize {
        self.implicit_world_model.pattern_count()
    }

    /// Gets failure count from self-reflection
    pub fn failure_count(&self) -> usize {
        self.self_reflection_engine.failure_count()
    }
}

impl Default for ACEReflector {
    fn default() -> Self {
        Self::new()
    }
}

/// Bootstrap learning phases (Early Experience)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearningPhase {
    /// β=0.5, coherence <0.3 - Rapid structure building
    Babbling,
    /// β=0.1, coherence 0.3-0.7 - Pattern refinement
    FirstWords,
    /// β=0.01, coherence >0.7 - Stable knowledge
    Grammar,
}

/// Dual-reflector system with confidence-aware control (ATLAS baseline)
#[derive(Debug)]
pub struct DualReflectorSystem {
    static_reflector: StaticReflector,
    adaptive_reflector: AdaptiveReflector,
    controller: ReflectorController,
    performance_tracker: ReflectorPerformanceTracker,
}

impl DualReflectorSystem {
    /// Creates a new dual-reflector system
    ///
    /// # Arguments
    /// * `adaptive_threshold` - Confidence threshold for using adaptive reflector (e.g., 0.6)
    /// * `performance_gap_threshold` - Performance gap threshold for switching (e.g., 0.15)
    pub fn new(adaptive_threshold: f64, performance_gap_threshold: f64) -> Self {
        Self {
            static_reflector: StaticReflector::new(),
            adaptive_reflector: AdaptiveReflector::new(0.01),
            controller: ReflectorController::new(adaptive_threshold, performance_gap_threshold),
            performance_tracker: ReflectorPerformanceTracker::new(),
        }
    }

    /// Analyzes a trajectory using confidence-based reflector selection
    pub fn analyze_trajectory(&mut self, trajectory: &QueryTrajectory, confidence: f64) {
        let performance_gap = self.performance_tracker.get_performance_gap();
        let use_adaptive = self
            .controller
            .should_use_adaptive(confidence, performance_gap);

        if use_adaptive {
            self.adaptive_reflector.adapt(trajectory);
            self.performance_tracker.record_adaptive_use();
        } else {
            // Use static reflector
            self.performance_tracker.record_static_use();
        }

        self.performance_tracker.increment_total();
    }

    /// Gets the static reflector (for testing)
    pub fn get_static_reflector(&self) -> Option<&StaticReflector> {
        Some(&self.static_reflector)
    }

    /// Gets the adaptive reflector (for testing)
    pub fn get_adaptive_reflector(&self) -> Option<&AdaptiveReflector> {
        Some(&self.adaptive_reflector)
    }

    /// Gets the controller (for testing)
    pub fn get_controller(&self) -> Option<&ReflectorController> {
        Some(&self.controller)
    }

    /// Gets performance statistics
    pub fn get_performance_stats(&self) -> &ReflectorPerformanceTracker {
        &self.performance_tracker
    }
}

/// Tracks performance of static vs adaptive reflectors
#[derive(Debug, Clone)]
pub struct ReflectorPerformanceTracker {
    pub total_analyses: usize,
    pub static_uses: usize,
    pub adaptive_uses: usize,
}

impl ReflectorPerformanceTracker {
    fn new() -> Self {
        Self {
            total_analyses: 0,
            static_uses: 0,
            adaptive_uses: 0,
        }
    }

    fn record_static_use(&mut self) {
        self.static_uses += 1;
    }

    fn record_adaptive_use(&mut self) {
        self.adaptive_uses += 1;
    }

    fn increment_total(&mut self) {
        self.total_analyses += 1;
    }

    fn get_performance_gap(&self) -> f64 {
        if self.total_analyses == 0 {
            return 0.0;
        }
        // Simple heuristic: ratio difference
        let static_ratio = self.static_uses as f64 / self.total_analyses as f64;
        let adaptive_ratio = self.adaptive_uses as f64 / self.total_analyses as f64;
        (adaptive_ratio - static_ratio).abs()
    }
}

/// Pattern identified from user interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionPattern {
    pub is_positive: bool,
    pub is_abandoned: bool,
    pub clicked_result_ids: Vec<String>,
}

/// Co-occurrence strengthening candidate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooccurrenceCandidate {
    pub term1: String,
    pub term2: String,
    pub confidence: f64,
}

/// Disambiguation edit suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisambiguationEdit {
    pub original_term: String,
    pub additional_context: Vec<String>,
}
