use super::*;

impl EditValidator {
    pub(super) fn new(quality_threshold: f64) -> Self {
        Self { quality_threshold }
    }

    /// Validates the quality of a delta context item.
    pub(super) fn validate_quality(&self, item: &DeltaContextItem) -> bool {
        item.confidence_score >= self.quality_threshold
    }
}

impl UpdateSuccessTracker {
    pub fn new() -> Self {
        Self {
            total_attempts: 0,
            successes: 0,
            failures: 0,
            rejected: 0,
        }
    }

    pub(super) fn record_success(&mut self) {
        self.total_attempts += 1;
        self.successes += 1;
    }

    pub(super) fn record_failure(&mut self) {
        self.total_attempts += 1;
        self.failures += 1;
    }

    pub(super) fn record_rejected(&mut self) {
        self.total_attempts += 1;
        self.rejected += 1;
    }

    /// Gets the success rate.
    pub fn success_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            return 0.0;
        }
        self.successes as f64 / self.total_attempts as f64
    }
}

impl Default for UpdateSuccessTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl EigenspaceStabilizer {
    pub fn new() -> Self {
        Self {
            accumulated_drift: 0.0,
            drift_threshold: 0.1,
        }
    }

    /// Checks if eigenspace recomputation is needed.
    pub fn needs_recomputation(&self) -> bool {
        self.accumulated_drift > self.drift_threshold
    }

    /// Records drift from an update.
    pub fn record_drift(&mut self, drift: f64) {
        self.accumulated_drift += drift;
    }

    /// Resets drift counter after recomputation.
    pub fn reset(&mut self) {
        self.accumulated_drift = 0.0;
    }
}

impl Default for EigenspaceStabilizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseAwareLearningRate {
    pub fn new() -> Self {
        Self {
            babbling_rate: 0.5,
            first_words_rate: 0.1,
            grammar_rate: 0.01,
        }
    }

    /// Gets the learning rate for a specific phase.
    pub fn get_rate_for_phase(&self, phase: &LearningPhase) -> f64 {
        match phase {
            LearningPhase::Babbling => self.babbling_rate,
            LearningPhase::FirstWords => self.first_words_rate,
            LearningPhase::Grammar => self.grammar_rate,
        }
    }
}

impl Default for PhaseAwareLearningRate {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CuratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CuratorError::CollapsePrevention(msg) => write!(f, "Collapse prevention: {}", msg),
            CuratorError::NoUpdatesApplied => write!(f, "No updates were applied"),
            CuratorError::InvalidDeltaFormat(msg) => write!(f, "Invalid delta format: {}", msg),
            CuratorError::MatrixError(msg) => write!(f, "Matrix error: {}", msg),
        }
    }
}

impl std::error::Error for CuratorError {}
