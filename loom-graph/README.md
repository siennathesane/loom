# loom-graph

Trading pair graph with negative-cycle detection for identifying profitable arbitrage opportunities. This crate implements the core graph algorithms: Bellman-Ford for cycle detection, Dijkstra for shortest-path queries, and volume-aware cost functions.

## Overview

`loom-graph` models the cryptocurrency exchange network as a directed weighted multigraph where nodes are currencies and edges are trading pairs with time-varying exchange rates and liquidity constraints. It uses the Bellman-Ford algorithm to detect negative cycles (profitable arbitrage paths) and supports multiple precision models (float, decimal, hybrid) for numerical stability during arbitrage detection.

## Public API

- `AdjacencyMatrix` — Dense in-memory graph representation with currency nodes
- `detect_negative_cycle()` — Bellman-Ford-based cycle detection
- `find_most_negative_cycle()` — Identify the most profitable cycle
- `gravitational_potential()` — Dijkstra-based potential field for gating
- `volume_dependent_weight()` — Cost function incorporating liquidity constraints
- `OrderBook` / `OrderSide` — Bid/ask price levels with size
- `CycleSnapshot`, `SnapshotManager` — Capture graph state for analysis

## Usage

```rust
use loom_graph::{AdjacencyMatrix, detect_negative_cycle, OrderBook, OrderSide};

// Create graph with known currencies
let graph = AdjacencyMatrix::new(vec![
    "USD".to_string(),
    "BTC".to_string(),
    "ETH".to_string(),
]);

// Add an edge (trading pair) with order book data
let book = OrderBook {
    bids: vec![OrderBookLevel { price: 50000.0, quantity: 1.0 }],
    asks: vec![OrderBookLevel { price: 50010.0, quantity: 1.0 }],
};

graph.add_edge("USD", "BTC", book)?;

// Detect negative cycles (profitable arbitrage)
if let Some(cycle) = detect_negative_cycle(&graph) {
    println!("Found profitable cycle: {:?}", cycle);
}
```

## Dependencies on other Loom crates

None — `loom-graph` is self-contained.

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
