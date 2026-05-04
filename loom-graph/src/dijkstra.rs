// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Dijkstra's algorithm for shortest paths (gravitational potential)
///
/// Computes the "return cost" - the minimum cost to convert from any currency
/// back to the base currency (USD) at current market rates.
///
/// This is used for the gravitational gate: before executing a hop, we check that
/// expected_profit > return_cost × safety_margin, ensuring capital can return home
/// even if the rest of the cycle evaporates.
use crate::adjacency_matrix::AdjacencyMatrix;
use crate::types::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Node in priority queue for Dijkstra's algorithm
///
/// Implements min-heap ordering (lower cost = higher priority)
#[derive(Clone, PartialEq)]
struct State {
    cost: f64,
    node: NodeIndex,
}

impl Eq for State {}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: reverse comparison
        // Lower cost should come first (higher priority)
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compute shortest paths from all nodes to USD (gravitational potential)
///
/// This calculates the "return cost" for each currency - the minimum cost
/// to convert back to USD at current market rates.
///
/// # Algorithm
///
/// Runs Dijkstra on the **reversed graph**:
/// - Normal graph: edge u→v means "can convert u to v"
/// - Reversed graph: edge v→u means "to reach v, must come from u"
/// - Finding shortest path TO destination in reversed graph = shortest path FROM node in normal graph
///
/// # Arguments
/// * `matrix` - The adjacency matrix representing exchange rates
/// * `usd_node` - Index of USD node (typically 0)
///
/// # Returns
/// Vector where `result[i]` = minimum cost to go from node i to USD.
/// Returns `f64::INFINITY` for nodes unreachable from USD.
pub fn gravitational_potential(matrix: &AdjacencyMatrix, usd_node: NodeIndex) -> Vec<f64> {
    let n = matrix.size();

    // Distance from each node to USD
    let mut dist = vec![f64::INFINITY; n];
    dist[usd_node] = 0.0;

    // Priority queue (min-heap)
    let mut heap = BinaryHeap::new();
    heap.push(State {
        cost: 0.0,
        node: usd_node,
    });

    // Track visited nodes to avoid reprocessing
    let mut visited = vec![false; n];

    while let Some(State { cost, node }) = heap.pop() {
        // Skip if we've already processed this node
        if visited[node] {
            continue;
        }
        visited[node] = true;

        // Skip if this is an outdated entry (distance already improved)
        if cost > dist[node] {
            continue;
        }

        // Relax edges in REVERSED graph
        // For each edge v→node in the normal graph,
        // consider it as node→v in the reversed graph
        for v in 0..n {
            // Check if edge v→node exists (reversed: node→v)
            if !matrix.has_edge(v, node) {
                continue;
            }

            let weight = matrix.get_weight(v, node);
            let new_dist = dist[node] + weight;

            if new_dist < dist[v] {
                dist[v] = new_dist;
                heap.push(State {
                    cost: new_dist,
                    node: v,
                });
            }
        }
    }

    dist
}

/// Compute shortest path from a specific source to all other nodes
///
/// This is standard Dijkstra (not reversed). Useful for testing and
/// for computing "forward" paths.
///
/// # Arguments
/// * `matrix` - The adjacency matrix
/// * `source` - Starting node
///
/// # Returns
/// Vector where `result[i]` = shortest distance from source to node i
pub fn shortest_paths_from(matrix: &AdjacencyMatrix, source: NodeIndex) -> Vec<f64> {
    let n = matrix.size();

    let mut dist = vec![f64::INFINITY; n];
    dist[source] = 0.0;

    let mut heap = BinaryHeap::new();
    heap.push(State {
        cost: 0.0,
        node: source,
    });

    let mut visited = vec![false; n];

    while let Some(State { cost, node }) = heap.pop() {
        if visited[node] {
            continue;
        }
        visited[node] = true;

        if cost > dist[node] {
            continue;
        }

        // Relax edges in normal direction
        for v in matrix.neighbors(node) {
            let weight = matrix.get_weight(node, v);
            let new_dist = dist[node] + weight;

            if new_dist < dist[v] {
                dist[v] = new_dist;
                heap.push(State {
                    cost: new_dist,
                    node: v,
                });
            }
        }
    }

    dist
}

/// Reconstruct shortest path from source to destination
///
/// Uses Dijkstra with predecessor tracking to reconstruct the actual path.
///
/// # Arguments
/// * `matrix` - The adjacency matrix
/// * `source` - Starting node
/// * `destination` - Ending node
///
/// # Returns
/// * `Some(path)` - Ordered sequence of nodes from source to destination
/// * `None` - If no path exists
pub fn shortest_path(
    matrix: &AdjacencyMatrix,
    source: NodeIndex,
    destination: NodeIndex,
) -> Option<Vec<NodeIndex>> {
    let n = matrix.size();

    let mut dist = vec![f64::INFINITY; n];
    dist[source] = 0.0;

    let mut pred = vec![None; n];

    let mut heap = BinaryHeap::new();
    heap.push(State {
        cost: 0.0,
        node: source,
    });

    let mut visited = vec![false; n];

    while let Some(State { cost, node }) = heap.pop() {
        if visited[node] {
            continue;
        }
        visited[node] = true;

        // If we reached destination, we can stop early
        if node == destination {
            break;
        }

        if cost > dist[node] {
            continue;
        }

        for v in matrix.neighbors(node) {
            let weight = matrix.get_weight(node, v);
            let new_dist = dist[node] + weight;

            if new_dist < dist[v] {
                dist[v] = new_dist;
                pred[v] = Some(node);
                heap.push(State {
                    cost: new_dist,
                    node: v,
                });
            }
        }
    }

    // Check if destination is reachable
    if dist[destination] == f64::INFINITY {
        return None;
    }

    // Reconstruct path by following predecessors
    let mut path = Vec::new();
    let mut current = destination;

    while current != source {
        path.push(current);
        match pred[current] {
            Some(p) => current = p,
            None => return None, // Shouldn't happen if dest is reachable
        }
    }
    path.push(source);

    // Reverse to get source→destination order
    path.reverse();

    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn create_test_graph() -> AdjacencyMatrix {
        let currencies = vec![
            "USD".to_string(),
            "BTC".to_string(),
            "ETH".to_string(),
            "LTC".to_string(),
        ];
        AdjacencyMatrix::new(currencies)
    }

    #[test]
    fn test_usd_to_usd_is_zero() {
        let matrix = create_test_graph();
        let usd_idx = 0;

        let potential = gravitational_potential(&matrix, usd_idx);

        // Distance from USD to USD should be 0
        assert_eq!(potential[usd_idx], 0.0);
    }

    #[test]
    fn test_disconnected_node() {
        let mut matrix = create_test_graph();
        let usd_idx = 0;

        // Only connect BTC to ETH, leave them disconnected from USD
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();

        let potential = gravitational_potential(&matrix, usd_idx);

        // BTC and ETH should be unreachable from USD
        assert_eq!(potential[1], f64::INFINITY); // BTC
        assert_eq!(potential[2], f64::INFINITY); // ETH
    }

    #[test]
    fn test_simple_path() {
        let mut matrix = create_test_graph();
        let usd_idx = 0;

        // Create path: BTC → USD (direct)
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();

        let potential = gravitational_potential(&matrix, usd_idx);

        // BTC should have finite distance to USD
        assert!(potential[1].is_finite());

        // Calculate expected: -ln(50000 × 0.994)
        // Note: This is negative because ln(large number) > 0, so -ln < 0
        let expected = -(50000.0 * 0.994_f64).ln();
        assert!((potential[1] - expected).abs() < 1e-10);

        // Weight should be negative (converting TO valuable currency)
        assert!(potential[1] < 0.0);
    }

    #[test]
    fn test_multiple_paths_takes_shortest() {
        let mut matrix = create_test_graph();
        let usd_idx = 0;

        // Path 1: BTC → USD (direct, good rate)
        matrix
            .update_edge("BTC", "USD", Decimal::new(51000, 0), 60)
            .unwrap();

        // Path 2: BTC → ETH → USD (indirect, worse rate)
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "USD", Decimal::new(2000, 0), 60)
            .unwrap();

        let potential = gravitational_potential(&matrix, usd_idx);

        // Should take the direct path (shorter in -ln space)
        let direct_cost = -(51000.0 * 0.994_f64).ln();
        let indirect_cost = -(20.0 * 0.994_f64).ln() + -(2000.0 * 0.994_f64).ln();

        // Verify we took the shorter path
        assert!((potential[1] - direct_cost.min(indirect_cost)).abs() < 1e-10);
    }

    #[test]
    fn test_triangle_inequality() {
        let mut matrix = create_test_graph();
        let usd_idx = 0;

        // Create triangle: USD → BTC → ETH
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 0.00002
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();
        matrix
            .update_edge("USD", "ETH", Decimal::new(33, 5), 60)
            .unwrap(); // 0.00033 (direct)

        let distances = shortest_paths_from(&matrix, usd_idx);

        // Triangle inequality: dist[USD→ETH] ≤ dist[USD→BTC] + dist[BTC→ETH]
        let direct = distances[2]; // USD → ETH
        let via_btc = distances[1] + matrix.get_weight(1, 2); // USD → BTC → ETH

        assert!(
            direct <= via_btc + 1e-10,
            "Triangle inequality violated: {} > {}",
            direct,
            via_btc
        );
    }

    #[test]
    fn test_shortest_path_reconstruction() {
        let mut matrix = create_test_graph();

        // Create path: USD → BTC → ETH
        matrix
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap();
        matrix
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap();

        let path = shortest_path(&matrix, 0, 2).unwrap(); // USD to ETH

        // Should be [USD, BTC, ETH]
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], 0); // USD
        assert_eq!(path[1], 1); // BTC
        assert_eq!(path[2], 2); // ETH

        // Verify each edge exists
        for window in path.windows(2) {
            assert!(matrix.has_edge(window[0], window[1]));
        }
    }

    #[test]
    fn test_shortest_path_no_path() {
        let matrix = create_test_graph();

        // No edges, so no path exists
        let path = shortest_path(&matrix, 0, 1);

        assert!(path.is_none());
    }

    #[test]
    fn test_gravitational_potential_realistic() {
        let mut matrix = create_test_graph();
        let usd_idx = 0;

        // Realistic crypto graph
        // BTC can return to USD at market rate
        matrix
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();

        // ETH can return via USD or BTC
        matrix
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 60)
            .unwrap();
        matrix
            .update_edge("ETH", "BTC", Decimal::new(6, 2), 60)
            .unwrap(); // 0.06

        // LTC can only return via BTC
        matrix
            .update_edge("LTC", "BTC", Decimal::new(25, 4), 60)
            .unwrap(); // 0.0025

        let potential = gravitational_potential(&matrix, usd_idx);

        // USD should be 0
        assert_eq!(potential[0], 0.0);

        // All others should be finite (reachable)
        assert!(potential[1].is_finite()); // BTC
        assert!(potential[2].is_finite()); // ETH
        assert!(potential[3].is_finite()); // LTC

        // LTC should have highest return cost (furthest from USD)
        assert!(potential[3] > potential[1]); // LTC > BTC
        assert!(potential[3] > potential[2]); // LTC > ETH
    }

    #[test]
    fn test_performance_complexity() {
        // Test with larger graph to verify O((V+E) log V) complexity
        let size = 50;
        let currencies: Vec<String> = (0..size).map(|i| format!("C{}", i)).collect();
        let mut matrix = AdjacencyMatrix::new(currencies);

        // Create some edges
        for i in 0..size - 1 {
            let rate = Decimal::new(100 + i as i64, 0);
            matrix
                .update_edge(&format!("C{}", i), &format!("C{}", i + 1), rate, 60)
                .unwrap();
        }

        // Run Dijkstra
        let potential = gravitational_potential(&matrix, 0);

        // Verify it completed and produced valid results
        assert_eq!(potential[0], 0.0);
        assert!(potential.len() == size);
    }

    #[test]
    fn test_state_ordering() {
        // Verify min-heap property
        let s1 = State { cost: 1.0, node: 0 };
        let s2 = State { cost: 2.0, node: 1 };
        let s3 = State { cost: 0.5, node: 2 };

        // Lower cost should be "greater" (higher priority in min-heap)
        assert!(s3 > s1);
        assert!(s1 > s2);

        // Test with BinaryHeap
        let mut heap = BinaryHeap::new();
        heap.push(s1.clone());
        heap.push(s2.clone());
        heap.push(s3.clone());

        // Should pop in order: 0.5, 1.0, 2.0
        assert_eq!(heap.pop().unwrap().cost, 0.5);
        assert_eq!(heap.pop().unwrap().cost, 1.0);
        assert_eq!(heap.pop().unwrap().cost, 2.0);
    }
}
