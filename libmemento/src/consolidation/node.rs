//! ServerNode - Domain-specific learning node with independent semantic matrix
//!
//! Each ServerNode represents a specialized learning domain (e.g., "medical", "legal", "physics")
//! that maintains its own semantic matrix and participates in distributed consolidation.

use crate::matrix::SemanticMatrix;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Coherence state indicating the health of a node's semantic structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoherenceState {
    /// Healthy: Spectral gap > 0.5, strong semantic structure
    Healthy,
    /// Degraded: Spectral gap 0.3-0.5, weakening structure
    Degraded,
    /// Quarantined: Spectral gap < 0.3 or failed consolidation, isolated from cluster
    Quarantined,
}

/// Domain-specific learning node with independent semantic matrix
///
/// # State Transitions
///
/// ```text
/// Healthy ←→ Degraded → Quarantined
///   ↑                       ↓
///   └───── (manual review) ─┘
/// ```
///
/// # Example
///
/// ```no_run
/// use libmemento::consolidation::node::ServerNode;
///
/// let mut node = ServerNode::new("medical".to_string(), 10000);
/// node.increment_query_count();
/// node.update_coherence_state();
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerNode {
    /// Unique identifier (immutable)
    pub node_id: Uuid,

    /// Domain specialization label (e.g., "medical", "legal")
    pub domain_label: String,

    /// Local semantic matrix
    pub matrix: SemanticMatrix,

    /// Connected nodes for consolidation
    pub peer_nodes: Vec<Uuid>,

    /// Current coherence state
    pub coherence_state: CoherenceState,

    /// Last successful merge timestamp
    pub last_consolidation_at: Option<SystemTime>,

    /// Eigenvector signature version (increments on significant changes)
    pub signature_version: u32,

    /// Total queries processed by this node
    pub query_count: u64,

    /// Node creation timestamp (immutable)
    pub created_at: SystemTime,
}

impl ServerNode {
    /// Creates a new ServerNode with specified domain and vocabulary size.
    ///
    /// # Arguments
    ///
    /// * `domain_label` - Domain specialization (e.g., "medical", "physics")
    /// * `vocabulary_size` - Maximum number of unique tokens
    ///
    /// # Returns
    ///
    /// A new ServerNode in Healthy state with empty matrix
    ///
    /// # Example
    ///
    /// ```
    /// use libmemento::consolidation::node::ServerNode;
    ///
    /// let node = ServerNode::new("legal".to_string(), 50000);
    /// assert_eq!(node.domain_label, "legal");
    /// ```
    pub fn new(domain_label: String, vocabulary_size: usize) -> Self {
        let mut matrix = SemanticMatrix::new(vocabulary_size);
        matrix.domain_label = domain_label.clone();

        Self {
            node_id: Uuid::new_v4(),
            domain_label,
            matrix,
            peer_nodes: Vec::new(),
            coherence_state: CoherenceState::Healthy,
            last_consolidation_at: None,
            signature_version: 0,
            query_count: 0,
            created_at: SystemTime::now(),
        }
    }

    /// Updates coherence state based on matrix's spectral gap.
    ///
    /// State transitions:
    /// - gap > 0.5: Healthy
    /// - 0.3 ≤ gap ≤ 0.5: Degraded
    /// - gap < 0.3: Quarantined
    ///
    /// Degraded → Healthy recovery is allowed, but Quarantined requires manual review.
    pub fn update_coherence_state(&mut self) {
        let coherence = self.matrix.coherence_score;

        match self.coherence_state {
            CoherenceState::Quarantined => {
                // Once quarantined, stays quarantined until manual review
                // (don't auto-recover to prevent oscillation)
            }
            _ => {
                // Normal state transitions
                if coherence >= 0.5 {
                    self.coherence_state = CoherenceState::Healthy;
                } else if coherence >= 0.3 {
                    self.coherence_state = CoherenceState::Degraded;
                } else {
                    self.coherence_state = CoherenceState::Quarantined;
                }
            }
        }
    }

    /// Increments the query count.
    ///
    /// Call this method every time the node processes a query.
    pub fn increment_query_count(&mut self) {
        self.query_count += 1;
    }

    /// Adds a peer node for consolidation.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - UUID of the peer node
    ///
    /// # Note
    ///
    /// Duplicate peer IDs are automatically deduplicated.
    pub fn add_peer(&mut self, peer_id: Uuid) {
        if !self.peer_nodes.contains(&peer_id) {
            self.peer_nodes.push(peer_id);
        }
    }

    /// Marks a consolidation as complete.
    ///
    /// Updates the `last_consolidation_at` timestamp to now.
    pub fn mark_consolidation_complete(&mut self) {
        self.last_consolidation_at = Some(SystemTime::now());
    }

    /// Increments the signature version.
    ///
    /// Call this when eigenvectors change significantly (e.g., after recomputation).
    pub fn increment_signature_version(&mut self) {
        self.signature_version += 1;
    }

    /// Returns the current coherence score from the matrix.
    ///
    /// # Returns
    ///
    /// Coherence score in [0, 1] range
    pub fn get_current_coherence(&self) -> f64 {
        self.matrix.coherence_score
    }

    /// Returns the current coherence state.
    ///
    /// # Returns
    ///
    /// Current CoherenceState (Healthy, Degraded, or Quarantined)
    pub fn get_coherence_state(&self) -> CoherenceState {
        self.coherence_state
    }

    /// Marks the node as quarantined due to failed consolidation.
    ///
    /// This method should be called when a merge operation fails or
    /// when the node's coherence drops below acceptable levels.
    pub fn quarantine(&mut self) {
        self.coherence_state = CoherenceState::Quarantined;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_node_has_valid_defaults() {
        let node = ServerNode::new("test".to_string(), 100);

        assert!(!node.node_id.is_nil());
        assert_eq!(node.domain_label, "test");
        assert_eq!(node.matrix.vocabulary_size(), 100);
        assert_eq!(node.coherence_state, CoherenceState::Healthy);
        assert_eq!(node.query_count, 0);
        assert_eq!(node.signature_version, 0);
        assert!(node.peer_nodes.is_empty());
        assert!(node.last_consolidation_at.is_none());
    }

    #[test]
    fn test_coherence_state_transitions() {
        let mut node = ServerNode::new("test".to_string(), 100);

        // Healthy → Degraded
        node.matrix.coherence_score = 0.4;
        node.update_coherence_state();
        assert_eq!(node.coherence_state, CoherenceState::Degraded);

        // Degraded → Quarantined
        node.matrix.coherence_score = 0.2;
        node.update_coherence_state();
        assert_eq!(node.coherence_state, CoherenceState::Quarantined);

        // Quarantined → (stays Quarantined even if coherence improves)
        node.matrix.coherence_score = 0.6;
        node.update_coherence_state();
        assert_eq!(
            node.coherence_state,
            CoherenceState::Quarantined,
            "Once quarantined, requires manual review"
        );
    }

    #[test]
    fn test_degraded_can_recover_to_healthy() {
        let mut node = ServerNode::new("test".to_string(), 100);

        // Healthy → Degraded
        node.matrix.coherence_score = 0.4;
        node.update_coherence_state();
        assert_eq!(node.coherence_state, CoherenceState::Degraded);

        // Degraded → Healthy (recovery allowed)
        node.matrix.coherence_score = 0.6;
        node.update_coherence_state();
        assert_eq!(node.coherence_state, CoherenceState::Healthy);
    }
}
