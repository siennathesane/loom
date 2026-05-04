// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Snapshot capture and invalidation detection for arbitrage cycles
///
/// When a profitable cycle is detected, we capture a snapshot of the edge weights
/// at that moment. Before executing each hop, we check if the snapshot is still
/// valid (no edge weights in the cycle have changed). If invalidated, we abort
/// the execution to avoid trading on stale data.
use crate::adjacency_matrix::AdjacencyMatrix;
use crate::types::*;
use std::collections::HashMap;

/// Snapshot of edge weights for a specific cycle
///
/// Captures the exact edge weights at the moment a cycle was detected,
/// allowing later validation that market conditions haven't changed.
#[derive(Debug, Clone, PartialEq)]
pub struct CycleSnapshot {
    /// Cycle path (ordered node indices)
    /// First and last nodes should be the same (closed cycle)
    cycle: Vec<NodeIndex>,

    /// Edge weights at time of snapshot
    /// Maps (from, to) -> weight
    weights: HashMap<(NodeIndex, NodeIndex), EdgeWeight>,

    /// Timestamp when snapshot was captured (optional, for debugging)
    #[allow(dead_code)]
    timestamp: Option<std::time::Instant>,
}

impl CycleSnapshot {
    /// Create a snapshot of current edge weights for the given cycle
    ///
    /// # Arguments
    /// * `matrix` - The adjacency matrix
    /// * `cycle` - Ordered sequence of node indices forming a cycle
    ///
    /// # Returns
    /// Snapshot capturing current state of all edges in the cycle
    ///
    /// # Example
    /// ```ignore
    /// let cycle = vec![0, 1, 2, 0]; // USD -> BTC -> ETH -> USD
    /// let snapshot = CycleSnapshot::capture(&matrix, &cycle);
    /// ```
    pub fn capture(matrix: &AdjacencyMatrix, cycle: &[NodeIndex]) -> Self {
        let mut weights = HashMap::new();

        // Capture weight for each edge in the cycle
        for window in cycle.windows(2) {
            let from = window[0];
            let to = window[1];
            let weight = matrix.get_weight(from, to);
            weights.insert((from, to), weight);
        }

        Self {
            cycle: cycle.to_vec(),
            weights,
            timestamp: Some(std::time::Instant::now()),
        }
    }

    /// Check if snapshot is still valid (no edge weights have changed)
    ///
    /// Compares current edge weights against captured values. Returns false
    /// if any edge in the cycle has changed, indicating the opportunity may
    /// no longer exist or profitability has changed.
    ///
    /// # Arguments
    /// * `matrix` - Current state of the adjacency matrix
    ///
    /// # Returns
    /// * `true` if all edge weights match the snapshot (within epsilon tolerance)
    /// * `false` if any edge weight has changed
    pub fn is_valid(&self, matrix: &AdjacencyMatrix) -> bool {
        self.is_valid_with_tolerance(matrix, 1e-10)
    }

    /// Check validity with custom tolerance
    ///
    /// # Arguments
    /// * `matrix` - Current adjacency matrix
    /// * `epsilon` - Maximum allowed difference in edge weights
    ///
    /// # Returns
    /// true if all edges match within epsilon tolerance
    pub fn is_valid_with_tolerance(&self, matrix: &AdjacencyMatrix, epsilon: f64) -> bool {
        for (&(from, to), &captured_weight) in &self.weights {
            let current_weight = matrix.get_weight(from, to);

            // Check if edge no longer exists (became INFINITY)
            if current_weight.is_infinite() && captured_weight.is_finite() {
                return false;
            }

            // Check if weight changed beyond tolerance
            if (current_weight - captured_weight).abs() > epsilon {
                return false;
            }
        }

        true
    }

    /// Get the cycle path
    pub fn cycle(&self) -> &[NodeIndex] {
        &self.cycle
    }

    /// Get the number of edges in the cycle
    pub fn edge_count(&self) -> usize {
        self.weights.len()
    }

    /// Check if a specific edge is part of this snapshot
    pub fn contains_edge(&self, from: NodeIndex, to: NodeIndex) -> bool {
        self.weights.contains_key(&(from, to))
    }

    /// Get the captured weight for a specific edge
    pub fn get_edge_weight(&self, from: NodeIndex, to: NodeIndex) -> Option<EdgeWeight> {
        self.weights.get(&(from, to)).copied()
    }

    /// Calculate how much an edge has changed since snapshot
    ///
    /// # Returns
    /// * `Some(delta)` - Difference between current and captured weight
    /// * `None` - Edge not in snapshot
    pub fn edge_delta(
        &self,
        matrix: &AdjacencyMatrix,
        from: NodeIndex,
        to: NodeIndex,
    ) -> Option<f64> {
        let captured = self.get_edge_weight(from, to)?;
        let current = matrix.get_weight(from, to);
        Some(current - captured)
    }

    /// Get all edges that have changed since snapshot
    ///
    /// # Arguments
    /// * `matrix` - Current adjacency matrix
    /// * `epsilon` - Threshold for considering a change significant
    ///
    /// # Returns
    /// Vector of (from, to, old_weight, new_weight) for changed edges
    pub fn changed_edges(
        &self,
        matrix: &AdjacencyMatrix,
        epsilon: f64,
    ) -> Vec<(NodeIndex, NodeIndex, f64, f64)> {
        let mut changed = Vec::new();

        for (&(from, to), &old_weight) in &self.weights {
            let new_weight = matrix.get_weight(from, to);
            if (new_weight - old_weight).abs() > epsilon {
                changed.push((from, to, old_weight, new_weight));
            }
        }

        changed
    }

    /// Calculate total cycle weight from snapshot (for verification)
    pub fn total_weight(&self) -> f64 {
        self.weights.values().sum()
    }
}

/// Manager for multiple concurrent snapshots
///
/// In Active mode with multiple lump sums, we may have multiple snapshots
/// active simultaneously (one per lump sum currently executing).
#[derive(Debug, Default)]
pub struct SnapshotManager {
    snapshots: HashMap<String, CycleSnapshot>,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
        }
    }

    /// Register a new snapshot
    pub fn register(&mut self, id: String, snapshot: CycleSnapshot) {
        self.snapshots.insert(id, snapshot);
    }

    /// Remove a snapshot
    pub fn remove(&mut self, id: &str) -> Option<CycleSnapshot> {
        self.snapshots.remove(id)
    }

    /// Check if a snapshot is still valid
    pub fn is_valid(&self, id: &str, matrix: &AdjacencyMatrix) -> Option<bool> {
        self.snapshots.get(id).map(|s| s.is_valid(matrix))
    }

    /// Get a snapshot by id
    pub fn get(&self, id: &str) -> Option<&CycleSnapshot> {
        self.snapshots.get(id)
    }

    /// Count active snapshots
    pub fn count(&self) -> usize {
        self.snapshots.len()
    }

    /// Invalidate all snapshots (e.g., after topology change)
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn create_test_graph() -> AdjacencyMatrix {
        let currencies = vec!["USD".to_string(), "BTC".to_string(), "ETH".to_string()];
        AdjacencyMatrix::new(currencies)
    }

    #[test]
    fn test_snapshot_captures_weights() {
        let mut matrix = create_test_graph();

        // Create cycle: USD -> BTC -> ETH -> USD
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2100, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Verify all edges captured
        assert_eq!(snapshot.edge_count(), 3);
        assert!(snapshot.contains_edge(0, 1));
        assert!(snapshot.contains_edge(1, 2));
        assert!(snapshot.contains_edge(2, 0));

        // Verify weights match
        assert_eq!(
            snapshot.get_edge_weight(0, 1),
            Some(matrix.get_weight(0, 1))
        );
        assert_eq!(
            snapshot.get_edge_weight(1, 2),
            Some(matrix.get_weight(1, 2))
        );
        assert_eq!(
            snapshot.get_edge_weight(2, 0),
            Some(matrix.get_weight(2, 0))
        );
    }

    #[test]
    fn test_unchanged_graph_is_valid() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2100, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Graph hasn't changed - should be valid
        assert!(snapshot.is_valid(&matrix));
    }

    #[test]
    fn test_changed_edge_in_cycle_invalidates() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2100, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Change an edge in the cycle
        matrix
            .update_edge("BTC", "ETH", Decimal::new(21, 0), 60)
            .unwrap();

        // Snapshot should now be invalid
        assert!(!snapshot.is_valid(&matrix));
    }

    #[test]
    fn test_changed_edge_outside_cycle_still_valid() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2100, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Change an edge NOT in the cycle (reverse direction)
        matrix
            .update_edge("ETH", "BTC", Decimal::new(5, 2), 60)
            .unwrap();

        // Snapshot should still be valid (changed edge not in cycle)
        assert!(snapshot.is_valid(&matrix));
    }

    #[test]
    fn test_multiple_snapshots_independent() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2100, 0), 60)
            .unwrap();

        // Snapshot 1: USD -> BTC -> ETH -> USD
        let cycle1 = vec![0, 1, 2, 0];
        let snapshot1 = CycleSnapshot::capture(&matrix, &cycle1);

        // Snapshot 2: USD -> BTC -> USD (different cycle)
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        let cycle2 = vec![0, 1, 0];
        let snapshot2 = CycleSnapshot::capture(&matrix, &cycle2);

        // Both should be valid
        assert!(snapshot1.is_valid(&matrix));
        assert!(snapshot2.is_valid(&matrix));

        // Change edge in cycle1 but not cycle2
        matrix
            .update_edge("ETH", "USD", Decimal::new(2200, 0), 60)
            .unwrap();

        // Snapshot1 invalid, snapshot2 still valid
        assert!(!snapshot1.is_valid(&matrix));
        assert!(snapshot2.is_valid(&matrix));
    }

    #[test]
    fn test_edge_delta() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();

        let cycle = vec![0, 1, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Get initial weight
        let initial_weight = matrix.get_weight(0, 1);

        // Change the edge
        matrix
            .update_edge("USD", "BTC", Decimal::new(22, 6), 60)
            .unwrap(); // 0.000022
        let new_weight = matrix.get_weight(0, 1);

        // Calculate delta
        let delta = snapshot.edge_delta(&matrix, 0, 1).unwrap();
        let expected_delta = new_weight - initial_weight;

        assert!((delta - expected_delta).abs() < 1e-10);
    }

    #[test]
    fn test_changed_edges_detection() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Change one edge significantly
        matrix
            .update_edge("BTC", "ETH", Decimal::new(21, 0), 60)
            .unwrap();

        let changed = snapshot.changed_edges(&matrix, 1e-10);

        // Should detect one changed edge
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, 1); // from BTC
        assert_eq!(changed[0].1, 2); // to ETH
    }

    #[test]
    fn test_snapshot_manager() {
        let mut matrix = create_test_graph();
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();

        let mut manager = SnapshotManager::new();

        // Register snapshot
        let cycle = vec![0, 1, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);
        manager.register("lump1".to_string(), snapshot);

        assert_eq!(manager.count(), 1);
        assert!(manager.is_valid("lump1", &matrix).unwrap());

        // Change graph
        matrix
            .update_edge("USD", "BTC", Decimal::new(3, 5), 60)
            .unwrap();

        // Should be invalid
        assert!(!manager.is_valid("lump1", &matrix).unwrap());

        // Remove snapshot
        manager.remove("lump1");
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_removed_edge_invalidates() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 2, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        // Remove an edge in the cycle
        matrix.remove_edge("BTC", "ETH").unwrap();

        // Should be invalid (edge is now INFINITY)
        assert!(!snapshot.is_valid(&matrix));
    }

    #[test]
    fn test_total_weight_calculation() {
        let mut matrix = create_test_graph();

        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();

        let cycle = vec![0, 1, 0];
        let snapshot = CycleSnapshot::capture(&matrix, &cycle);

        let total = snapshot.total_weight();
        let expected = matrix.get_weight(0, 1) + matrix.get_weight(1, 0);

        assert!((total - expected).abs() < 1e-10);
    }
}
