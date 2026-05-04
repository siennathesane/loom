// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-graph
//!
//! Trading-pair graph representation with fast arbitrage detection algorithms.
//!
//! Maintains a currency graph with exchange rates stored as negative log returns
//! to transform arbitrage detection into negative-cycle detection. Provides an
//! adjacency matrix in struct-of-arrays layout, Bellman-Ford for cycle detection,
//! and Dijkstra for shortest-path queries. Uses rust_decimal for financial arithmetic
//! when needed; supports f64 and hybrid precision approaches for performance tuning.
//!
//! ## Examples
//!
//! ```ignore
//! let mut matrix = AdjacencyMatrix::new(num_currencies);
//! matrix.set_edge(usd, eur, ln_rate);
//! match detect_negative_cycle(&matrix, usd) {
//!     BellmanFordResult::NegativeCycle(cycle) => println!("Arbitrage: {:?}", cycle),
//!     BellmanFordResult::NoCycle => println!("No opportunity"),
//! }
//! ```

pub mod adjacency_matrix;
pub mod bellman_ford;
pub mod dijkstra;
pub mod graph_maintenance;
pub mod precision;
pub mod precision_analysis;
pub mod snapshot;
pub mod topology;
pub mod types;
pub mod volume_cost;

pub use precision::{
    decimal_approach::DecimalApproach, f64_approach::F64Approach, hybrid_approach::HybridApproach,
    PrecisionApproach,
};

pub use adjacency_matrix::AdjacencyMatrix;
pub use bellman_ford::{
    detect_negative_cycle, find_most_negative_cycle, verify_cycle_weight, BellmanFordResult,
};
pub use dijkstra::{gravitational_potential, shortest_path, shortest_paths_from};
pub use graph_maintenance::{EdgeUpdate, GraphMaintenanceManager, L3Update};
pub use snapshot::{CycleSnapshot, SnapshotManager};
pub use topology::{TopologyEvent, TopologyManager};
pub use types::*;
pub use volume_cost::{
    volume_dependent_rate, volume_dependent_weight, OrderBook, OrderBookLevel, OrderSide,
};
