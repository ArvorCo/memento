//! Reflector Controller - Selects static vs adaptive based on confidence

/// Controller that selects between static and adaptive reflectors
#[derive(Debug, Clone)]
pub struct ReflectorController {
    /// Confidence threshold for using adaptive reflector
    adaptive_threshold: f64,
    /// Performance gap threshold for switching strategies
    performance_gap_threshold: f64,
}

impl ReflectorController {
    /// Creates a new reflector controller
    ///
    /// # Arguments
    /// * `adaptive_threshold` - Confidence threshold (e.g., 0.6)
    /// * `performance_gap_threshold` - Performance gap threshold (e.g., 0.15)
    pub fn new(adaptive_threshold: f64, performance_gap_threshold: f64) -> Self {
        Self {
            adaptive_threshold,
            performance_gap_threshold,
        }
    }

    /// Determines whether to use the adaptive reflector
    ///
    /// # Arguments
    /// * `confidence` - Query confidence score [0, 1]
    /// * `performance_gap` - Current performance gap between static and adaptive
    ///
    /// # Returns
    /// `true` if adaptive reflector should be used, `false` for static
    pub fn should_use_adaptive(&self, confidence: f64, performance_gap: f64) -> bool {
        // Use adaptive if:
        // 1. Confidence is above threshold AND
        // 2. Performance gap is below threshold (not too divergent)
        confidence >= self.adaptive_threshold && performance_gap <= self.performance_gap_threshold
    }

    /// Gets the adaptive threshold
    pub fn get_adaptive_threshold(&self) -> f64 {
        self.adaptive_threshold
    }

    /// Gets the performance gap threshold
    pub fn get_performance_gap_threshold(&self) -> f64 {
        self.performance_gap_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_selects_static_for_low_confidence() {
        let controller = ReflectorController::new(0.6, 0.15);
        assert!(!controller.should_use_adaptive(0.3, 0.0));
    }

    #[test]
    fn test_controller_selects_adaptive_for_high_confidence() {
        let controller = ReflectorController::new(0.6, 0.15);
        assert!(controller.should_use_adaptive(0.8, 0.0));
    }

    #[test]
    fn test_controller_selects_static_for_large_performance_gap() {
        let controller = ReflectorController::new(0.6, 0.15);
        assert!(!controller.should_use_adaptive(0.8, 0.25));
    }
}
