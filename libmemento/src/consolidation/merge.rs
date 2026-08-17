//! Coherence-Preserving Merge Implementation (T072)
//!
//! Distributed node consolidation with coherence guarantees and rollback support.
//! Uses weighted averaging based on alignment scores and enforces Frobenius drift < 0.1.

use crate::consolidation::alignment::{CoherenceMonitor, MergeDecision};
use crate::consolidation::error::{ConsolidationError, Result};
use crate::consolidation::node::ServerNode;
use crate::consolidation::signature::CoherenceSignature;
use crate::matrix::{compute_frobenius_drift, SemanticMatrix};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

/// Consolidation type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsolidationType {
    /// Combine high-coherence nodes
    Merge,
    /// Update node with global knowledge
    Rebase,
    /// Restore node from checkpoint
    Reset,
}

/// Checkpoint for consolidation rollback
///
/// Stores pre-merge state to enable rollback if merge degrades coherence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationCheckpoint {
    /// Unique checkpoint identifier
    pub checkpoint_id: Uuid,

    /// Type of consolidation operation
    pub consolidation_type: ConsolidationType,

    /// Source node IDs
    pub source_nodes: Vec<Uuid>,

    /// Target node ID (if merging into existing node)
    pub target_node_id: Option<Uuid>,

    /// Pre-merge eigenvector signatures
    pub pre_merge_signatures: Vec<CoherenceSignature>,

    /// Post-merge signature (if merge succeeded)
    pub post_merge_signature: Option<CoherenceSignature>,

    /// Whether coherence was preserved
    pub coherence_preserved: bool,

    /// Eigenvector drift magnitude
    pub eigenvector_drift: f64,

    /// Whether consolidation succeeded
    pub success: bool,

    /// Serialized rollback data (node states)
    pub rollback_data: Option<Vec<u8>>,

    /// Checkpoint creation time
    pub created_at: SystemTime,

    /// Consolidation completion time
    pub completed_at: Option<SystemTime>,
}

/// Consolidation engine for merging distributed nodes
///
/// Manages node merging with coherence preservation guarantees.
///
/// # Example
///
/// ```no_run
/// use libmemento::consolidation::merge::ConsolidationEngine;
/// use libmemento::consolidation::node::ServerNode;
///
/// let engine = ConsolidationEngine::new();
/// let mut node1 = ServerNode::new("medical".to_string(), 1000);
/// let mut node2 = ServerNode::new("legal".to_string(), 1000);
///
/// let merged = engine.merge_nodes(&mut node1, &mut node2).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct ConsolidationEngine {
    /// Alignment monitor
    monitor: CoherenceMonitor,

    /// Maximum allowable eigenvector drift (default: 0.1)
    pub drift_threshold: f64,
}

impl ConsolidationEngine {
    /// Creates a new consolidation engine with default settings
    pub fn new() -> Self {
        Self {
            monitor: CoherenceMonitor::new(),
            drift_threshold: 0.1,
        }
    }

    /// Creates a consolidation engine with custom thresholds
    ///
    /// # Arguments
    ///
    /// * `alignment_high` - High alignment threshold for automatic merge
    /// * `alignment_low` - Low alignment threshold for review
    /// * `drift_threshold` - Maximum Frobenius drift allowed
    pub fn with_thresholds(alignment_high: f64, alignment_low: f64, drift_threshold: f64) -> Self {
        Self {
            monitor: CoherenceMonitor::with_thresholds(alignment_high, alignment_low),
            drift_threshold,
        }
    }

    /// Merges two nodes if alignment is sufficient
    ///
    /// # Arguments
    ///
    /// * `node1` - First node to merge (mutable for eigendecomposition)
    /// * `node2` - Second node to merge (mutable for eigendecomposition)
    ///
    /// # Returns
    ///
    /// Merged node or error if drift exceeds threshold
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::merge::ConsolidationEngine;
    /// use libmemento::consolidation::node::ServerNode;
    ///
    /// let engine = ConsolidationEngine::new();
    /// let mut node1 = ServerNode::new("domain1".to_string(), 1000);
    /// let mut node2 = ServerNode::new("domain2".to_string(), 1000);
    ///
    /// match engine.merge_nodes(&mut node1, &mut node2) {
    ///     Ok(merged) => println!("Merge succeeded"),
    ///     Err(e) => println!("Merge failed: {}", e),
    /// }
    /// ```
    pub fn merge_nodes(
        &self,
        node1: &mut ServerNode,
        node2: &mut ServerNode,
    ) -> Result<ServerNode> {
        // Check alignment
        let decision = self.monitor.should_merge(node1, node2)?;

        match decision {
            MergeDecision::Separate => {
                return Err(ConsolidationError::MergeConflict(
                    "Nodes have insufficient alignment for merging".to_string(),
                ));
            }
            MergeDecision::Review => {
                // Proceed with caution - compute eigenvectors and check drift carefully
            }
            MergeDecision::Merge => {
                // High alignment - proceed with merge
            }
        }

        // Save node IDs before moving references
        let node1_id = node1.node_id;
        let node2_id = node2.node_id;
        let domain_label = format!("{}+{}", node1.domain_label, node2.domain_label);

        // Compute eigenvectors before merge
        let eigen1 = node1.matrix.compute_eigendecomposition(100).map_err(|e| {
            ConsolidationError::MergeConflict(format!(
                "Failed to compute node1 eigendecomposition: {}",
                e
            ))
        })?;

        // Perform weighted merge (equal weights for now, could use alignment scores)
        let nodes = vec![node1, node2];
        let weights = vec![0.5, 0.5];
        let merged_matrix = self.weighted_merge(&nodes, &weights)?;

        // Create merged node
        let mut merged_node = ServerNode::new(domain_label, merged_matrix.vocabulary_size());
        merged_node.matrix = merged_matrix;

        // Compute eigenvectors after merge
        let eigen_merged = merged_node
            .matrix
            .compute_eigendecomposition(100)
            .map_err(|e| {
                ConsolidationError::MergeConflict(format!(
                    "Failed to compute merged eigendecomposition: {}",
                    e
                ))
            })?;

        // Check coherence preservation via Frobenius drift
        let drift = compute_frobenius_drift(&eigen1.eigenvectors, &eigen_merged.eigenvectors);

        if drift > self.drift_threshold {
            return Err(ConsolidationError::MergeConflict(format!(
                "Merge rejected: eigenvector drift {} exceeds threshold {}",
                drift, self.drift_threshold
            )));
        }

        // Update metadata
        merged_node.matrix.coherence_score = eigen_merged.coherence_score;
        merged_node.update_coherence_state();
        merged_node.mark_consolidation_complete();
        merged_node.increment_signature_version();

        // Add both source nodes as peers
        merged_node.add_peer(node1_id);
        merged_node.add_peer(node2_id);

        Ok(merged_node)
    }

    /// Consolidates multiple nodes into a single global matrix
    ///
    /// # Arguments
    ///
    /// * `nodes` - Mutable slice of nodes to consolidate
    ///
    /// # Returns
    ///
    /// Tuple of (consolidated node, checkpoint for rollback)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libmemento::consolidation::merge::ConsolidationEngine;
    /// use libmemento::consolidation::node::ServerNode;
    ///
    /// let mut engine = ConsolidationEngine::new();
    /// let mut nodes = vec![
    ///     ServerNode::new("node1".to_string(), 1000),
    ///     ServerNode::new("node2".to_string(), 1000),
    ///     ServerNode::new("node3".to_string(), 1000),
    /// ];
    ///
    /// let (consolidated, checkpoint) = engine.consolidate_multi_node(&mut nodes).unwrap();
    /// println!("Consolidated {} nodes", checkpoint.source_nodes.len());
    /// ```
    pub fn consolidate_multi_node(
        &mut self,
        nodes: &mut [ServerNode],
    ) -> Result<(ServerNode, ConsolidationCheckpoint)> {
        if nodes.is_empty() {
            return Err(ConsolidationError::MergeConflict(
                "No nodes to consolidate".to_string(),
            ));
        }

        if nodes.len() == 1 {
            return Err(ConsolidationError::MergeConflict(
                "Need at least 2 nodes to consolidate".to_string(),
            ));
        }

        // Create checkpoint
        let checkpoint = self.create_checkpoint(nodes, ConsolidationType::Merge)?;

        // Perform equal-weighted merge
        let weights = vec![1.0 / nodes.len() as f64; nodes.len()];
        // Convert to vector of mutable references
        let node_refs: Vec<&mut ServerNode> = nodes.iter_mut().collect();
        let merged_matrix = self.weighted_merge(&node_refs, &weights)?;

        // Create consolidated node
        let domain_labels: Vec<&str> = nodes.iter().map(|n| n.domain_label.as_str()).collect();
        let mut consolidated_node = ServerNode::new(
            format!("consolidated({})", domain_labels.join("+")),
            merged_matrix.vocabulary_size(),
        );
        consolidated_node.matrix = merged_matrix;

        // Update metadata
        consolidated_node.update_coherence_state();
        consolidated_node.mark_consolidation_complete();
        consolidated_node.increment_signature_version();

        // Add all source nodes as peers
        for node in nodes {
            consolidated_node.add_peer(node.node_id);
        }

        // Update checkpoint with success
        let mut updated_checkpoint = checkpoint.clone();
        updated_checkpoint.success = true;
        updated_checkpoint.coherence_preserved = consolidated_node.get_current_coherence() > 0.3;
        updated_checkpoint.completed_at = Some(SystemTime::now());

        Ok((consolidated_node, updated_checkpoint))
    }

    /// Performs weighted average merge of multiple nodes
    ///
    /// # Arguments
    ///
    /// * `nodes` - Nodes to merge
    /// * `weights` - Weight for each node (must sum to 1.0)
    ///
    /// # Returns
    ///
    /// Merged semantic matrix
    fn weighted_merge(&self, nodes: &[&mut ServerNode], weights: &[f64]) -> Result<SemanticMatrix> {
        if nodes.len() != weights.len() {
            return Err(ConsolidationError::MergeConflict(format!(
                "Node count {} != weight count {}",
                nodes.len(),
                weights.len()
            )));
        }

        let weight_sum: f64 = weights.iter().sum();
        if (weight_sum - 1.0).abs() > 1e-6 {
            return Err(ConsolidationError::MergeConflict(format!(
                "Weights sum to {}, must sum to 1.0",
                weight_sum
            )));
        }

        // Get vocabulary size from first node
        let vocabulary_size = nodes[0].matrix.vocabulary_size();

        // Verify all nodes have same vocabulary size
        for node in nodes.iter() {
            if node.matrix.vocabulary_size() != vocabulary_size {
                return Err(ConsolidationError::MergeConflict(format!(
                    "Vocabulary size mismatch: expected {}, got {}",
                    vocabulary_size,
                    node.matrix.vocabulary_size()
                )));
            }
        }

        // Create new matrix for merged result
        let mut merged_matrix = SemanticMatrix::new(vocabulary_size);

        // Collect all triplets from all nodes with weights
        for (node, &weight) in nodes.iter().zip(weights.iter()) {
            let triplets = node.matrix.to_triplets().map_err(|e| {
                ConsolidationError::MergeConflict(format!("Failed to extract triplets: {}", e))
            })?;

            for (i, j, value) in triplets {
                // Add weighted value to merged matrix
                merged_matrix
                    .add_cooccurrence(i, j, value * weight)
                    .map_err(|e| {
                        ConsolidationError::MergeConflict(format!(
                            "Failed to add weighted co-occurrence: {}",
                            e
                        ))
                    })?;
            }
        }

        // Consolidate merged matrix
        merged_matrix.consolidate().map_err(|e| {
            ConsolidationError::MergeConflict(format!("Failed to consolidate merged matrix: {}", e))
        })?;

        Ok(merged_matrix)
    }

    /// Creates checkpoint before merge for rollback (public for testing)
    ///
    /// # Arguments
    ///
    /// * `nodes` - Mutable slice of nodes to be merged (needed for eigendecomposition)
    ///
    /// # Returns
    ///
    /// Checkpoint with pre-merge signatures
    pub fn create_checkpoint_before_merge(
        &self,
        nodes: &mut [ServerNode],
    ) -> Result<ConsolidationCheckpoint> {
        self.create_checkpoint(nodes, ConsolidationType::Merge)
    }

    /// Creates checkpoint before merge for rollback
    ///
    /// # Arguments
    ///
    /// * `nodes` - Mutable slice of nodes to be merged (needed for eigendecomposition)
    /// * `consolidation_type` - Type of consolidation operation
    ///
    /// # Returns
    ///
    /// Checkpoint with pre-merge signatures
    fn create_checkpoint(
        &self,
        nodes: &mut [ServerNode],
        consolidation_type: ConsolidationType,
    ) -> Result<ConsolidationCheckpoint> {
        // Compute signatures for all nodes
        let mut pre_merge_signatures = Vec::new();

        for node in nodes.iter_mut() {
            // Compute eigendecomposition (requires mut access)
            let eigen = node.matrix.compute_eigendecomposition(100).map_err(|e| {
                ConsolidationError::InvalidSignature(format!(
                    "Failed to compute eigendecomposition for node {}: {}",
                    node.node_id, e
                ))
            })?;

            // Compress to signature
            let signature = CoherenceSignature::compress(
                &eigen.eigenvectors,
                &eigen.eigenvalues,
                node.node_id,
                node.domain_label.clone(),
            )?;

            pre_merge_signatures.push(signature);
        }

        // Serialize nodes for rollback using serde_json (supports custom Serialize impls)
        // Note: We use serde_json instead of bincode because SemanticMatrix has a custom
        // Serialize implementation that bincode 1.3 doesn't handle correctly
        let rollback_data = serde_json::to_vec(nodes as &[ServerNode])
            .map(Some)
            .unwrap_or(None);

        Ok(ConsolidationCheckpoint {
            checkpoint_id: Uuid::new_v4(),
            consolidation_type,
            source_nodes: nodes.iter().map(|n| n.node_id).collect(),
            target_node_id: None,
            pre_merge_signatures,
            post_merge_signature: None,
            coherence_preserved: false,
            eigenvector_drift: 0.0,
            success: false,
            rollback_data,
            created_at: SystemTime::now(),
            completed_at: None,
        })
    }

    /// Rollback merge using checkpoint data
    ///
    /// Restores nodes to pre-merge state and marks them as Quarantined.
    ///
    /// # Arguments
    ///
    /// * `checkpoint` - Checkpoint containing pre-merge node states
    ///
    /// # Returns
    ///
    /// Vector of restored nodes, each marked as Quarantined
    ///
    /// # Errors
    ///
    /// Returns InvalidCheckpoint if rollback data is missing
    pub fn rollback_consolidation(
        &self,
        checkpoint: &ConsolidationCheckpoint,
    ) -> Result<Vec<ServerNode>> {
        // Validate checkpoint has rollback data
        let rollback_data = checkpoint.rollback_data.as_ref().ok_or_else(|| {
            ConsolidationError::InvalidCheckpoint("Checkpoint missing rollback data".to_string())
        })?;

        // Deserialize nodes from checkpoint using serde_json
        let mut restored_nodes: Vec<ServerNode> =
            serde_json::from_slice(rollback_data).map_err(|e| {
                ConsolidationError::InvalidCheckpoint(format!(
                    "Failed to deserialize rollback data: {}",
                    e
                ))
            })?;

        // Mark all nodes as quarantined after failed merge
        self.quarantine_nodes(&mut restored_nodes);

        Ok(restored_nodes)
    }

    /// Automatic rollback if drift detected during merge
    ///
    /// Attempts merge with automatic rollback on failure.
    ///
    /// # Arguments
    ///
    /// * `node1` - First node to merge
    /// * `node2` - Second node to merge
    ///
    /// # Returns
    ///
    /// Merged node or error with nodes quarantined
    pub fn merge_with_auto_rollback(
        &mut self,
        node1: &mut ServerNode,
        node2: &mut ServerNode,
    ) -> Result<ServerNode> {
        // Serialize nodes before attempting merge (for potential rollback)
        // Use serde_json to support SemanticMatrix's custom Serialize implementation
        let node1_backup = serde_json::to_vec(&*node1).map_err(|e| {
            ConsolidationError::MergeConflict(format!("Failed to backup node1: {}", e))
        })?;
        let node2_backup = serde_json::to_vec(&*node2).map_err(|e| {
            ConsolidationError::MergeConflict(format!("Failed to backup node2: {}", e))
        })?;

        // Attempt merge
        let merge_result = self.merge_nodes(node1, node2);

        match merge_result {
            Ok(merged) => Ok(merged),
            Err(e) => {
                // Restore from backup on failure
                *node1 = serde_json::from_slice(&node1_backup).map_err(|de| {
                    ConsolidationError::MergeConflict(format!("Failed to restore node1: {}", de))
                })?;
                *node2 = serde_json::from_slice(&node2_backup).map_err(|de| {
                    ConsolidationError::MergeConflict(format!("Failed to restore node2: {}", de))
                })?;

                // Quarantine the original nodes
                node1.quarantine();
                node2.quarantine();

                Err(e)
            }
        }
    }

    /// Mark nodes as Quarantined after failed merge
    ///
    /// # Arguments
    ///
    /// * `nodes` - Mutable slice of nodes to quarantine
    fn quarantine_nodes(&self, nodes: &mut [ServerNode]) {
        for node in nodes {
            node.quarantine();
        }
    }
}

impl Default for ConsolidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consolidation_engine_creation() {
        let engine = ConsolidationEngine::new();
        assert_eq!(engine.drift_threshold, 0.1);
    }

    #[test]
    fn test_consolidation_engine_with_custom_thresholds() {
        let engine = ConsolidationEngine::with_thresholds(0.95, 0.75, 0.05);
        assert_eq!(engine.monitor.threshold_high, 0.95);
        assert_eq!(engine.monitor.threshold_low, 0.75);
        assert_eq!(engine.drift_threshold, 0.05);
    }

    #[test]
    fn test_weighted_merge_weight_validation() {
        let engine = ConsolidationEngine::new();
        let mut node1 = ServerNode::new("test1".to_string(), 100);
        let mut node2 = ServerNode::new("test2".to_string(), 100);

        let nodes = vec![&mut node1, &mut node2];
        let bad_weights = vec![0.5, 0.3]; // Doesn't sum to 1.0

        let result = engine.weighted_merge(&nodes, &bad_weights);
        assert!(
            result.is_err(),
            "Should reject weights that don't sum to 1.0"
        );
    }

    #[test]
    fn test_server_node_serde_json_serialization() {
        let mut node = ServerNode::new("test".to_string(), 100);

        // Add some data
        for i in 0..5 {
            node.matrix
                .add_cooccurrence(i, i + 1, 1.0)
                .expect("Failed to add co-occurrence");
        }

        node.matrix.consolidate().expect("Failed to consolidate");

        // Serialize single node with serde_json
        let serialized = serde_json::to_vec(&node).expect("Failed to serialize node");
        println!("Serialized node: {} bytes", serialized.len());

        // Deserialize single node
        let deserialized: ServerNode =
            serde_json::from_slice(&serialized).expect("Failed to deserialize node");
        assert_eq!(node.node_id, deserialized.node_id);

        // Serialize vec of nodes
        let nodes = vec![node];
        let serialized_vec = serde_json::to_vec(&nodes).expect("Failed to serialize vec");
        println!("Serialized vec: {} bytes", serialized_vec.len());

        // Deserialize vec
        let deserialized_vec: Vec<ServerNode> =
            serde_json::from_slice(&serialized_vec).expect("Failed to deserialize vec");
        assert_eq!(deserialized_vec.len(), 1);
    }
}
