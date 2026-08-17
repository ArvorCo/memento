//! Adaptive Reflector - Learns from recent interactions

use crate::learning::interaction_tracker::QueryTrajectory;
use nalgebra::DMatrix;
use std::collections::VecDeque;

/// Adaptive reflector that learns from recent interactions
#[derive(Debug, Clone)]
pub struct AdaptiveReflector {
    /// Distribution embedding (learned representation)
    distribution_embedding: DMatrix<f64>,
    /// Quality predictor for edits
    quality_predictor: super::predictor::EditQualityPredictor,
    /// Recent trajectories for adaptation window
    adaptation_window: VecDeque<QueryTrajectory>,
    /// Learning rate for updates
    learning_rate: f64,
    /// Flag indicating if adaptation has occurred
    has_adapted: bool,
    /// Maximum window size
    max_window_size: usize,
}

impl AdaptiveReflector {
    /// Creates a new adaptive reflector
    pub fn new(learning_rate: f64) -> Self {
        Self {
            distribution_embedding: DMatrix::zeros(10, 10),
            quality_predictor: super::predictor::EditQualityPredictor::new(),
            adaptation_window: VecDeque::new(),
            learning_rate,
            has_adapted: false,
            max_window_size: 100,
        }
    }

    /// Adapts the reflector based on a trajectory outcome
    pub fn adapt(&mut self, trajectory: &QueryTrajectory) {
        // Add to adaptation window
        self.adaptation_window.push_back(trajectory.clone());
        if self.adaptation_window.len() > self.max_window_size {
            self.adaptation_window.pop_front();
        }

        // Simple adaptation: update distribution embedding based on query embedding
        let query_embedding = &trajectory.query_embedding;

        // Update embedding (simplified outer product update)
        for i in 0..self
            .distribution_embedding
            .nrows()
            .min(query_embedding.len())
        {
            for j in 0..self
                .distribution_embedding
                .ncols()
                .min(query_embedding.len())
            {
                let update = self.learning_rate * query_embedding[i] * query_embedding[j];
                self.distribution_embedding[(i, j)] += update;
            }
        }

        self.has_adapted = true;
    }

    /// Checks if the reflector has adapted
    pub fn has_adapted(&self) -> bool {
        self.has_adapted
    }

    /// Predicts the quality of a suggested edit
    pub fn predict_quality(&self, edit: &super::DisambiguationEdit) -> f64 {
        self.quality_predictor.predict_quality(edit)
    }

    /// Gets the current distribution embedding (for debugging)
    pub fn get_distribution_embedding(&self) -> &DMatrix<f64> {
        &self.distribution_embedding
    }
}
