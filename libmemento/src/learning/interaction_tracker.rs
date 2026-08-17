//! User Interaction Tracking System
//!
//! This module tracks user interactions including:
//! - Click events with position and dwell time
//! - Query reformulations with edit distance
//! - Explicit feedback (thumbs up/down, comments)
//! - Session success metrics
//! - Distribution shift detection (ATLAS enhancement)

use chrono::{DateTime, Utc};
use nalgebra::DVector;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// Main interaction tracker managing all user interactions
#[derive(Debug)]
pub struct InteractionTracker {
    /// Active query trajectories
    active_trajectories: HashMap<Uuid, QueryTrajectory>,
    /// Distribution monitor for query pattern shift detection (ATLAS)
    distribution_monitor: DistributionMonitor,
    /// Maximum number of active trajectories to keep in memory
    max_trajectories: usize,
}

impl InteractionTracker {
    /// Creates a new interaction tracker
    pub fn new(window_size: usize) -> Self {
        Self {
            active_trajectories: HashMap::new(),
            distribution_monitor: DistributionMonitor::new(window_size, 0.3),
            max_trajectories: 1000,
        }
    }

    /// Starts a new query trajectory
    pub fn start_trajectory(&mut self, query_id: Uuid, query_text: String) {
        let embedding = self.compute_query_embedding(&query_text);
        self.distribution_monitor
            .add_query_embedding(embedding.clone());

        let trajectory = QueryTrajectory {
            query_id,
            query_text,
            query_embedding: embedding,
            clicks: Vec::new(),
            reformulations: Vec::new(),
            explicit_feedback: Vec::new(),
            session_status: SessionStatus::Active,
            created_at: Utc::now(),
        };

        self.active_trajectories.insert(query_id, trajectory);

        // Evict oldest trajectories if exceeding limit
        if self.active_trajectories.len() > self.max_trajectories {
            if let Some(oldest_id) = self.find_oldest_trajectory() {
                self.active_trajectories.remove(&oldest_id);
            }
        }
    }

    /// Tracks a click event
    pub fn track_click(
        &mut self,
        query_id: Uuid,
        query_text: String,
        result_id: String,
        position: usize,
        dwell_time_seconds: f64,
    ) {
        if !self.active_trajectories.contains_key(&query_id) {
            self.start_trajectory(query_id, query_text);
        }

        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.clicks.push(ClickEvent {
                result_id,
                position,
                dwell_time_seconds,
                timestamp: Utc::now(),
            });
        }
    }

    /// Tracks a query reformulation
    pub fn track_reformulation(
        &mut self,
        query_id: Uuid,
        original_query: String,
        reformulated_query: String,
    ) {
        let edit_distance = compute_edit_distance(&original_query, &reformulated_query);

        if !self.active_trajectories.contains_key(&query_id) {
            self.start_trajectory(query_id, original_query.clone());
        }

        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.reformulations.push(ReformulationEvent {
                original_query,
                reformulated_query,
                edit_distance,
                timestamp: Utc::now(),
            });
        }
    }

    /// Tracks positive feedback
    pub fn track_positive_feedback(
        &mut self,
        query_id: Uuid,
        query_text: String,
        result_id: String,
        comment: Option<String>,
    ) {
        if !self.active_trajectories.contains_key(&query_id) {
            self.start_trajectory(query_id, query_text);
        }

        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.explicit_feedback.push(FeedbackEvent {
                result_id,
                is_positive: true,
                comment,
                timestamp: Utc::now(),
            });
        }
    }

    /// Tracks negative feedback
    pub fn track_negative_feedback(
        &mut self,
        query_id: Uuid,
        query_text: String,
        result_id: String,
        comment: Option<String>,
    ) {
        if !self.active_trajectories.contains_key(&query_id) {
            self.start_trajectory(query_id, query_text);
        }

        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.explicit_feedback.push(FeedbackEvent {
                result_id,
                is_positive: false,
                comment,
                timestamp: Utc::now(),
            });
        }
    }

    /// Marks a session as completed
    pub fn mark_session_completed(&mut self, query_id: Uuid) {
        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.session_status = SessionStatus::Completed;
        }
    }

    /// Marks a session as abandoned
    pub fn mark_session_abandoned(&mut self, query_id: Uuid) {
        if let Some(trajectory) = self.active_trajectories.get_mut(&query_id) {
            trajectory.session_status = SessionStatus::Abandoned;
        }
    }

    /// Gets a trajectory by ID
    pub fn get_trajectory(&self, query_id: &Uuid) -> Option<&QueryTrajectory> {
        self.active_trajectories.get(query_id)
    }

    /// Gets all active trajectories
    pub fn get_all_trajectories(&self) -> Vec<&QueryTrajectory> {
        self.active_trajectories.values().collect()
    }

    /// Computes a simple query embedding for distribution monitoring
    pub fn compute_query_embedding(&self, query_text: &str) -> DVector<f64> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        query_text.hash(&mut hasher);
        let hash = hasher.finish();

        // Create a simple 10-dimensional embedding based on hash
        let mut vec = DVector::zeros(10);
        for i in 0..10 {
            vec[i] = ((hash >> (i * 6)) & 0x3F) as f64 / 63.0;
        }
        vec
    }

    /// Computes the baseline distribution
    pub fn compute_distribution_baseline(&mut self) {
        self.distribution_monitor.compute_baseline();
    }

    /// Checks if distribution has shifted
    pub fn has_distribution_shifted(&self) -> bool {
        self.distribution_monitor.has_distribution_shifted()
    }

    /// Finds the oldest trajectory
    fn find_oldest_trajectory(&self) -> Option<Uuid> {
        self.active_trajectories
            .iter()
            .min_by_key(|(_, t)| t.created_at)
            .map(|(id, _)| *id)
    }
}

/// Represents a complete query trajectory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTrajectory {
    pub query_id: Uuid,
    pub query_text: String,
    pub query_embedding: DVector<f64>,
    pub clicks: Vec<ClickEvent>,
    pub reformulations: Vec<ReformulationEvent>,
    pub explicit_feedback: Vec<FeedbackEvent>,
    pub session_status: SessionStatus,
    pub created_at: DateTime<Utc>,
}

/// Represents a click event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickEvent {
    pub result_id: String,
    pub position: usize,
    pub dwell_time_seconds: f64,
    pub timestamp: DateTime<Utc>,
}

/// Represents a query reformulation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReformulationEvent {
    pub original_query: String,
    pub reformulated_query: String,
    pub edit_distance: usize,
    pub timestamp: DateTime<Utc>,
}

/// Represents explicit user feedback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEvent {
    pub result_id: String,
    pub is_positive: bool,
    pub comment: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

/// Enumeration of all interaction events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionEvent {
    Click(ClickEvent),
    Reformulation(ReformulationEvent),
    Feedback(FeedbackEvent),
    SessionCompleted(Uuid),
    SessionAbandoned(Uuid),
}

/// Distribution monitor for query pattern shift detection (ATLAS)
#[derive(Debug)]
pub struct DistributionMonitor {
    /// History of query embeddings
    query_history: VecDeque<DVector<f64>>,
    /// Window size for monitoring
    window_size: usize,
    /// Threshold for detecting distribution shift
    shift_threshold: f64,
    /// Baseline distribution (computed from initial queries)
    baseline_distribution: Option<QueryDistribution>,
}

impl DistributionMonitor {
    /// Creates a new distribution monitor
    pub fn new(window_size: usize, shift_threshold: f64) -> Self {
        Self {
            query_history: VecDeque::with_capacity(window_size),
            window_size,
            shift_threshold,
            baseline_distribution: None,
        }
    }

    /// Adds a query embedding to the history
    pub fn add_query_embedding(&mut self, embedding: DVector<f64>) {
        self.query_history.push_back(embedding);
        if self.query_history.len() > self.window_size {
            self.query_history.pop_front();
        }
    }

    /// Computes the baseline distribution
    pub fn compute_baseline(&mut self) {
        if self.query_history.len() < 2 {
            return;
        }

        let mean = compute_mean(&self.query_history);
        let variance = compute_variance(&self.query_history, &mean);

        self.baseline_distribution = Some(QueryDistribution { mean, variance });
    }

    /// Checks if the distribution has shifted
    pub fn has_distribution_shifted(&self) -> bool {
        if self.baseline_distribution.is_none() || self.query_history.len() < 2 {
            return false;
        }

        let baseline = self.baseline_distribution.as_ref().unwrap();
        let current_mean = compute_mean(&self.query_history);

        // Compute distance between baseline and current mean
        let distance = (&current_mean - &baseline.mean).norm();

        // Normalize by baseline variance
        let normalized_distance = distance / (baseline.variance.sqrt() + 1e-6);

        normalized_distance > self.shift_threshold
    }
}

/// Represents a query distribution
#[derive(Debug, Clone)]
pub struct QueryDistribution {
    pub mean: DVector<f64>,
    pub variance: f64,
}

/// Computes the mean of a collection of vectors
fn compute_mean(vectors: &VecDeque<DVector<f64>>) -> DVector<f64> {
    if vectors.is_empty() {
        return DVector::zeros(10);
    }

    let dim = vectors[0].len();
    let mut sum = DVector::zeros(dim);

    for vec in vectors {
        sum += vec;
    }

    sum / vectors.len() as f64
}

/// Computes the variance of a collection of vectors
fn compute_variance(vectors: &VecDeque<DVector<f64>>, mean: &DVector<f64>) -> f64 {
    if vectors.is_empty() {
        return 0.0;
    }

    let mut sum_sq_diff = 0.0;
    for vec in vectors {
        sum_sq_diff += (vec - mean).norm_squared();
    }

    sum_sq_diff / vectors.len() as f64
}

/// Computes Levenshtein edit distance between two strings
#[allow(clippy::needless_range_loop)]
fn compute_edit_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_distance() {
        assert_eq!(compute_edit_distance("kitten", "sitting"), 3);
        assert_eq!(compute_edit_distance("matrix", "matrices"), 3); // insert 'e', 's' at end, substitute 'x'→'c'
        assert_eq!(compute_edit_distance("same", "same"), 0);
    }

    #[test]
    fn test_compute_mean() {
        let mut vectors = VecDeque::new();
        vectors.push_back(DVector::from_vec(vec![1.0, 2.0, 3.0]));
        vectors.push_back(DVector::from_vec(vec![3.0, 4.0, 5.0]));

        let mean = compute_mean(&vectors);
        assert_eq!(mean[0], 2.0);
        assert_eq!(mean[1], 3.0);
        assert_eq!(mean[2], 4.0);
    }

    #[test]
    fn test_compute_variance() {
        let mut vectors = VecDeque::new();
        vectors.push_back(DVector::from_vec(vec![1.0, 2.0]));
        vectors.push_back(DVector::from_vec(vec![3.0, 4.0]));

        let mean = compute_mean(&vectors);
        let variance = compute_variance(&vectors, &mean);
        assert!(variance > 0.0);
    }
}
