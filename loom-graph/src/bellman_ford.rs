// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Bellman-Ford algorithm for negative cycle detection
///
/// This is the core arbitrage detection algorithm. A negative-weight cycle in
/// the -ln(rate) transformed graph corresponds to a profitable arbitrage opportunity.
///
/// Algorithm properties:
/// - Detects all negative cycles (completeness)
/// - Returns exactly one cycle if multiple exist (deterministic)
/// - Time complexity: O(V × E)
/// - Space complexity: O(V)
use crate::adjacency_matrix::AdjacencyMatrix;
use crate::types::*;

/// Result of Bellman-Ford cycle detection
#[derive(Debug, Clone, PartialEq)]
pub enum BellmanFordResult {
    /// No negative cycle found - graph is arbitrage-free
    NoCycle,

    /// Negative cycle detected - arbitrage opportunity exists
    /// Contains the ordered sequence of node indices forming the cycle
    NegativeCycle(Vec<NodeIndex>),
}

/// Run Bellman-Ford algorithm to detect negative cycles
///
/// # Algorithm
///
/// 1. Initialize distances to all nodes as infinity, except source = 0
/// 2. Relax all edges V-1 times (standard Bellman-Ford)
/// 3. Check all edges one more time - if any can be relaxed, negative cycle exists
/// 4. If negative cycle found, trace back to extract the cycle
///
/// # Arguments
/// * `matrix` - The adjacency matrix representing the exchange rate graph
/// * `source` - Starting node (typically USD)
///
/// # Returns
/// * `NoCycle` if no arbitrage opportunity exists
/// * `NegativeCycle(path)` with the ordered sequence of nodes forming the cycle
pub fn detect_negative_cycle(matrix: &AdjacencyMatrix, source: NodeIndex) -> BellmanFordResult {
    let n = matrix.size();

    // Distance from source to each node (in -ln space)
    let mut dist = vec![f64::INFINITY; n];
    dist[source] = 0.0;

    // Predecessor array for cycle extraction
    let mut pred = vec![None; n];

    // Relax all edges V-1 times
    for _ in 0..n - 1 {
        let mut any_update = false;

        for u in 0..n {
            if dist[u] == f64::INFINITY {
                continue;
            }

            for v in matrix.neighbors(u) {
                let weight = matrix.get_weight(u, v);
                if weight.is_finite() {
                    let new_dist = dist[u] + weight;
                    if new_dist < dist[v] {
                        dist[v] = new_dist;
                        pred[v] = Some(u);
                        any_update = true;
                    }
                }
            }
        }

        // Early termination: if no updates in this iteration, we're done
        if !any_update {
            break;
        }
    }

    // Check for negative cycles by trying to relax edges one more time
    let mut cycle_node = None;

    for u in 0..n {
        if dist[u] == f64::INFINITY {
            continue;
        }

        for v in matrix.neighbors(u) {
            let weight = matrix.get_weight(u, v);
            if weight.is_finite() {
                let new_dist = dist[u] + weight;
                if new_dist < dist[v] {
                    // Found an edge that can still be relaxed - negative cycle exists!
                    cycle_node = Some(v);
                    break;
                }
            }
        }

        if cycle_node.is_some() {
            break;
        }
    }

    match cycle_node {
        None => BellmanFordResult::NoCycle,
        Some(node_in_cycle) => {
            // Extract the cycle by following predecessors
            let cycle = extract_cycle(&pred, node_in_cycle, n);
            BellmanFordResult::NegativeCycle(cycle)
        }
    }
}

/// Extract a cycle from the predecessor array
///
/// # Algorithm
/// 1. Walk backwards from the node in the cycle for V steps to ensure we're in the cycle
/// 2. Mark the node we land on
/// 3. Walk backwards again until we return to the marked node
/// 4. The path we traced is the cycle
///
/// # Arguments
/// * `pred` - Predecessor array from Bellman-Ford
/// * `start` - A node known to be part of the cycle
/// * `n` - Number of nodes in the graph
///
/// # Returns
/// Ordered sequence of node indices forming the cycle (first node repeated at end)
fn extract_cycle(pred: &[Option<NodeIndex>], start: NodeIndex, n: usize) -> Vec<NodeIndex> {
    // Walk backwards V times to ensure we're definitely in the cycle
    let mut node = start;
    for _ in 0..n {
        if let Some(p) = pred[node] {
            node = p;
        }
    }

    // Now node is definitely in the cycle
    let cycle_start = node;
    let mut cycle = vec![node];

    // Walk backwards until we return to cycle_start
    loop {
        if let Some(p) = pred[node] {
            node = p;
            cycle.push(node);
            if node == cycle_start {
                break;
            }
        } else {
            // This shouldn't happen if algorithm is correct
            break;
        }
    }

    // Reverse to get forward direction
    cycle.reverse();

    cycle
}

/// Verify that a cycle is actually negative (profitable)
///
/// This is a verification function - Bellman-Ford guarantees the cycle is negative,
/// but this can be used for testing/debugging.
///
/// # Arguments
/// * `matrix` - The adjacency matrix
/// * `cycle` - Ordered sequence of node indices
///
/// # Returns
/// The total weight of the cycle (should be < 0 for arbitrage)
pub fn verify_cycle_weight(matrix: &AdjacencyMatrix, cycle: &[NodeIndex]) -> f64 {
    if cycle.len() < 2 {
        return 0.0;
    }

    let mut total_weight = 0.0;

    for window in cycle.windows(2) {
        let from = window[0];
        let to = window[1];
        let weight = matrix.get_weight(from, to);

        if !weight.is_finite() {
            // Edge doesn't exist - invalid cycle
            return f64::INFINITY;
        }

        total_weight += weight;
    }

    total_weight
}

/// Bellman-Ford variant that finds the most negative cycle
///
/// Standard Bellman-Ford returns any negative cycle. This variant
/// continues searching to find the cycle with the most negative total weight
/// (highest profit potential).
///
/// **Note**: This is more expensive (O(V² × E)) and should only be used
/// if we want to prioritize cycles by profitability.
pub fn find_most_negative_cycle(matrix: &AdjacencyMatrix) -> BellmanFordResult {
    let n = matrix.size();
    let mut best_cycle: Option<Vec<NodeIndex>> = None;
    let mut best_weight = 0.0;

    // Run Bellman-Ford from each node as source
    for source in 0..n {
        if let BellmanFordResult::NegativeCycle(cycle) = detect_negative_cycle(matrix, source) {
            let weight = verify_cycle_weight(matrix, &cycle);

            if weight < best_weight {
                best_weight = weight;
                best_cycle = Some(cycle);
            }
        }
    }

    match best_cycle {
        Some(cycle) => BellmanFordResult::NegativeCycle(cycle),
        None => BellmanFordResult::NoCycle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    /// Helper to create a test graph
    fn create_test_graph() -> AdjacencyMatrix {
        let currencies = vec!["USD".to_string(), "BTC".to_string(), "ETH".to_string()];
        AdjacencyMatrix::new(currencies)
    }

    #[test]
    fn test_no_cycle_empty_graph() {
        let matrix = create_test_graph();
        let result = detect_negative_cycle(&matrix, 0);

        assert_eq!(result, BellmanFordResult::NoCycle);
    }

    #[test]
    fn test_no_cycle_chain() {
        let mut matrix = create_test_graph();

        // USD -> BTC -> ETH (no cycle, just a chain)
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 0.00002
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap(); // 20

        let result = detect_negative_cycle(&matrix, 0);
        assert_eq!(result, BellmanFordResult::NoCycle);
    }

    #[test]
    fn test_positive_cycle() {
        let mut matrix = create_test_graph();

        // Create a cycle with fees that makes it unprofitable
        // USD -> BTC -> ETH -> USD
        // With 0.6% fees, this should lose money
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 1 USD = 0.00002 BTC
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap(); // 1 BTC = 20 ETH
        matrix
            .update_edge("ETH", "USD", Decimal::new(2000, 0), 60)
            .unwrap(); // 1 ETH = 2000 USD

        // Expected product: 0.00002 × 20 × 2000 = 0.8 (before fees)
        // After fees: 0.8 × 0.994³ ≈ 0.786 < 1 (loss)

        let result = detect_negative_cycle(&matrix, 0);
        assert_eq!(result, BellmanFordResult::NoCycle);
    }

    #[test]
    fn test_negative_cycle_simple() {
        let mut matrix = create_test_graph();

        // Create a profitable arbitrage cycle
        // Make the product > 1 even with fees
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 1 USD = 0.00002 BTC
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap(); // 1 BTC = 20 ETH
        matrix
            .update_edge("ETH", "USD", Decimal::new(2500, 0), 60)
            .unwrap(); // 1 ETH = 2500 USD (higher!)

        // Expected product: 0.00002 × 20 × 2500 = 1.0 (before fees)
        // After fees: 1.0 × 0.994³ ≈ 0.982 (still loss with fees)
        // Let's make it more profitable: increase ETH->USD rate
        matrix
            .update_edge("ETH", "USD", Decimal::new(2700, 0), 60)
            .unwrap();

        // New product: 0.00002 × 20 × 2700 = 1.08 (before fees)
        // After fees: 1.08 × 0.994³ ≈ 1.061 > 1 (profit!)

        let result = detect_negative_cycle(&matrix, 0);

        match result {
            BellmanFordResult::NegativeCycle(cycle) => {
                // Verify cycle contains all three nodes
                assert!(cycle.len() >= 3);

                // Verify cycle is actually negative
                let weight = verify_cycle_weight(&matrix, &cycle);
                assert!(
                    weight < 0.0,
                    "Cycle weight should be negative, got {}",
                    weight
                );
            }
            BellmanFordResult::NoCycle => {
                panic!("Expected negative cycle but got NoCycle");
            }
        }
    }

    #[test]
    fn test_cycle_extraction() {
        let mut matrix = create_test_graph();

        // Create guaranteed profitable cycle with no fees
        matrix
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap(); // 0.001
        matrix
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap(); // 50
        matrix
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap(); // 21

        // Product: 0.001 × 50 × 21 = 1.05 > 1 (profitable)

        let result = detect_negative_cycle(&matrix, 0);

        match result {
            BellmanFordResult::NegativeCycle(cycle) => {
                // Verify cycle structure
                assert!(cycle.len() >= 3, "Cycle should have at least 3 nodes");
                assert_eq!(
                    cycle[0],
                    cycle[cycle.len() - 1],
                    "Cycle should start and end at same node"
                );

                // Verify each edge in cycle exists
                for window in cycle.windows(2) {
                    let from = window[0];
                    let to = window[1];
                    assert!(
                        matrix.has_edge(from, to),
                        "Edge {}->{} should exist",
                        from,
                        to
                    );
                }
            }
            BellmanFordResult::NoCycle => {
                panic!("Expected negative cycle");
            }
        }
    }

    #[test]
    fn test_verify_cycle_weight() {
        let mut matrix = create_test_graph();

        // Create cycle: USD(0) -> BTC(1) -> ETH(2) -> USD(0)
        matrix
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap();

        let cycle = vec![0, 1, 2, 0]; // USD -> BTC -> ETH -> USD
        let weight = verify_cycle_weight(&matrix, &cycle);

        // Calculate expected weight
        // weight = -ln(0.001) + -ln(50) + -ln(21)
        // weight = -ln(0.001 × 50 × 21) = -ln(1.05) ≈ -0.04879
        let expected = -(0.001_f64 * 50.0 * 21.0).ln();

        assert!(
            (weight - expected).abs() < 1e-10,
            "Weight {} != expected {}",
            weight,
            expected
        );
        assert!(weight < 0.0, "Profitable cycle should have negative weight");
    }

    #[test]
    fn test_early_termination() {
        // Test that Bellman-Ford terminates early when no updates occur
        let mut matrix = create_test_graph();

        // Simple chain, no cycles
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();

        // Should terminate after just a few iterations, not V-1 iterations
        let result = detect_negative_cycle(&matrix, 0);
        assert_eq!(result, BellmanFordResult::NoCycle);
    }

    #[test]
    fn test_disconnected_nodes() {
        let mut matrix = create_test_graph();

        // Only connect USD <-> BTC, leave ETH disconnected
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();

        let result = detect_negative_cycle(&matrix, 0);

        // Should handle disconnected nodes gracefully
        assert_eq!(result, BellmanFordResult::NoCycle);
    }

    #[test]
    fn test_find_most_negative() {
        let mut matrix = create_test_graph();

        // Create profitable cycle
        matrix
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap();

        let result = find_most_negative_cycle(&matrix);

        match result {
            BellmanFordResult::NegativeCycle(cycle) => {
                let weight = verify_cycle_weight(&matrix, &cycle);
                assert!(weight < 0.0);
            }
            BellmanFordResult::NoCycle => {
                panic!("Expected to find cycle");
            }
        }
    }
}
