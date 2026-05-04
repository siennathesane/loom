# loom-engine

Core arbitrage detection engine that monitors a trading pair graph and detects profitable cycles using gravitational gating. This crate houses the event loop, state machine, and lump sum (capital parcel) tracking for the Loom arbitrage system.

## Overview

`loom-engine` implements the main arbitrage detection logic using a state machine that tracks capital parcels (lump sums) and applies a novel "gravitational gating" mechanism to filter out cycles unlikely to execute profitably. The engine processes market data updates, maintains an in-memory graph of trading pairs and exchange rates, and logs detected opportunities to a SQLite database via the `loom-store` integration.

## Public API

- `LumpSum` — Represents a capital parcel with a unique ID, current currency, size, and execution state
- `Opportunity` — Describes a detected arbitrage cycle with expected profit and gating status
- `StateMachine` — Manages transitions between engine modes (LISTEN, OPPORTUNITY, EXECUTING, etc.)
- `ListenEventLoop` — Main event processor that ingests market data and checks for negative cycles

## Usage

```rust
use loom_engine::{ListenEventLoop, ListenConfig, Opportunity};
use loom_graph::AdjacencyMatrix;

// Create a graph with known currencies
let currencies = vec!["USD", "BTC", "ETH"];
let graph = AdjacencyMatrix::new(
    currencies.iter().map(|s| s.to_string()).collect()
);

// Configure the event loop with safety margins and fees
let config = ListenConfig {
    safety_margin: 1.5,      // Require 1.5% profit above fees
    fee_bps: 60,             // 0.6% fee per trade
    cycle_check_interval_ms: 100,
};

// Initialize and start processing market data
let mut event_loop = ListenEventLoop::new(graph, config);

// Feed market data into the loop; opportunities are logged automatically
event_loop.handle_market_data(&entry, "BTC-USD");
```

## Dependencies on other Loom crates

- `loom-fix` — FIX protocol support for market data entry point definitions
- `loom-graph` — Graph data structure and cycle detection algorithms
- `loom-store` — Database persistence for opportunities and market ticks

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
