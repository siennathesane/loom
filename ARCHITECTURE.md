# Loom Architecture

## Overview

Loom is a high-performance cryptocurrency arbitrage detection engine that connects to Coinbase via the FIX protocol. It maintains a real-time graph of trading pairs represented as negative log returns, detects profitable arbitrage cycles using Bellman-Ford cycle detection, and logs opportunities to SQLite. The system solves a classic problem in trading: finding cyclic currency conversion paths where the product of exchange rates exceeds 1 (profit after fees). By transforming rates into -ln(rate) space, Loom converts cycle detection into negative-cycle detection—a classic computer science problem with well-understood algorithms.

Loom is distinctive for its hand-rolled FIX implementation (not a library), its struct-of-arrays graph representation for cache locality, and gravitational gating—a configurable safety margin that filters opportunities by the highest cost required to return capital to a base currency.

## System Diagram

```
┌─────────────────────────────┐
│   Coinbase FIX Server       │
│  (sandbox or production)    │
└──────────────┬──────────────┘
               │ TLS
               │
        ┌──────▼──────────┐
        │   loom-fix      │  Hand-rolled FIX 5.0 SP2
        │  (protocol +    │  HMAC-SHA256 auth
        │  transport)     │  Zero-allocation hot paths
        └──────┬──────────┘
               │
        ┌──────▼──────────────────────────────────┐
        │      loom-daemon (main.rs)              │
        │  ┌──────────────────────────────────┐  │
        │  │    loom-engine event loop        │  │
        │  │  ┌──────────────────────────┐    │  │
        │  │  │  loom-graph              │    │  │
        │  │  │  (AdjacencyMatrix +      │    │  │
        │  │  │   Bellman-Ford)          │    │  │
        │  │  └──────────────────────────┘    │  │
        │  │  ┌──────────────────────────┐    │  │
        │  │  │  loom-store (SQLite)     │    │  │
        │  │  │  Opportunity logging     │    │  │
        │  │  └──────────────────────────┘    │  │
        │  └──────────────────────────────────┘  │
        │              │                         │
        │              │ gRPC                    │
        │  ┌───────────▼──────────────┐         │
        │  │  loom-server (gRPC)      │         │
        │  │  Unix socket or TCP      │         │
        │  └───────────┬──────────────┘         │
        │              │                        │
        └──────────────┼────────────────────────┘
                       │ gRPC
            ┌──────────▼──────────┐
            │    loomctl (CLI)    │
            │  (control plane)    │
            └─────────────────────┘
```

## Crate Responsibilities

### loom-fix
Hand-rolled FIX 5.0 SP2 protocol implementation. Provides:
- Zero-allocation message serialization (< 1µs p99)
- TLS transport with TCP_NODELAY for latency
- Session state machine (Logon, LoggedIn, Disconnected)
- HMAC-SHA256 authentication for Coinbase
- Message parsing with sequence number tracking
- Market data and order entry message types
- No external FIX library—built for control and performance on Coinbase's specific subset

### loom-graph
Trading-pair graph with arbitrage detection algorithms. Provides:
- `AdjacencyMatrix`: Struct-of-arrays adjacency matrix storing exchange rates as -ln(rate)
- `detect_negative_cycle()`: Bellman-Ford implementation (1.76µs p99)
- `gravitational_potential()`: Dijkstra shortest-path for return cost calculations
- `CycleSnapshot`: Immutable snapshots for testing and validation
- `GraphMaintenanceManager`: L3 order book updates → graph edge updates
- Precision strategies: f64, rust_decimal, and hybrid approaches for numeric accuracy
- 556x headroom vs 1ms cycle detection target

### loom-engine
Core arbitrage detection event loop. Provides:
- `ListenEventLoop`: Integrates FIX, graph, and opportunity logging
- `Opportunity`: Detected arbitrage cycle with profit, return cost, and timestamps
- `ListenConfig`: Safety margin, fees, and cycle check interval
- `L3Update` processing: bid/ask level changes → adjacency matrix updates
- Periodic Bellman-Ford invocation (default 100ms interval)
- Edge change tracking to minimize redundant cycle detection
- Database integration for opportunity persistence

### loom-store
SQLite-backed persistence with high-performance batching. Provides:
- `BatchWriter`: Buffers ticks and opportunities, flushes by time or size (1000 records or 1000ms)
- WAL mode for concurrent reads and writes (27K msg/sec sustained)
- Six tables: `ticks`, `opportunities`, `lump_sums`, `hops`, `reconciliations`, `topology_events`
- Foreign keys and indexes for query performance
- Sub-10ms p99 flush latency for batches up to 1000 records

### loom-config
Configuration loading and validation. Provides:
- `LoomConfig`: Top-level config struct with FIX, engine, server, and database sections
- TOML parsing via serde
- `LoomConfig::sandbox_example()`: Pre-configured sandbox credentials (placeholders)
- Validation of heartbeat intervals, safety margins, and transport types
- Support for both Unix socket and TCP server transports

### loom-proto
Protocol buffer definitions for gRPC. Provides:
- `LoomControl` service: status, pairs, opportunities, events, streaming
- Message types: `StatusRequest`, `OpportunitiesRequest`, `CycleSnapshot`, etc.
- Generated by tonic-build from `.proto` files at compile time
- No hand-written code—pure gRPC codegen

### loom-server
gRPC server for the daemon. Provides:
- Unix socket or TCP transport configuration
- `LoomControl` service implementation
- Status queries, trading pair enumeration, opportunity queries, event streaming
- Connection to `loom-daemon` for graph state and opportunity logs
- Graceful shutdown handling

### loom-daemon
Main binary that orchestrates the system. Provides:
- Configuration loading and validation
- FIX connection establishment with TLS
- Session authentication and keepalive
- Dynamic currency discovery via `SecurityListRequest`
- Graph initialization from discovered symbols
- Event loop startup and market data subscription
- Periodic cycle detection and heartbeat sending
- Graceful shutdown and logout

### loomctl
Command-line client for controlling loom-daemon. Provides:
- Subcommands: `status`, `pairs`, `opportunities`, `graph`, `events`, `ls`
- Unix socket and TCP endpoint support
- Query result filtering (time range, profit thresholds, gating)
- JSON and human-readable output modes
- Real-time event streaming

### loom-harness
FIX compliance test harness. Provides:
- Mock Coinbase FIX server for integration testing
- Simulates Logon, Heartbeat, TestRequest, SecurityList, MarketDataIncrementalRefresh
- Order entry and execution report messages
- 63+ compliance tests validating Loom's FIX implementation

## Data Flow

A market data update flows through Loom as follows:

1. **FIX bytes arrive** from Coinbase on the TLS transport (`loom-fix/transport`).
2. **Session parser** (`loom-fix/session`) extracts message fields and reconstructs FIX messages.
3. **Daemon** (`loom-daemon/main.rs`) routes the message by type. For `MarketDataIncrementalRefresh`:
4. **Message parsing** (`loom-fix/market_data`) extracts MD entries (bid/ask, quantity, etc.).
5. **Event loop** (`loom-engine/event_loop.rs`) calls `handle_market_data()` for each entry.
6. **Graph maintenance** (`loom-graph/graph_maintenance.rs`) converts L3 updates to edge changes:
   - Computes new rate from bid/ask via `volume_dependent_rate()`
   - Updates adjacency matrix: -ln(new_rate) for the pair
7. **Opportunity detection** runs periodically (default 100ms):
   - `detect_negative_cycle()` executes Bellman-Ford from USD
   - If cycle found: cycle nodes, symbols, expected profit, max return cost extracted
   - `gravitational_potential()` computes Dijkstra distances (return costs)
8. **Gravitational gating** filters by safety margin:
   - If `expected_profit > max_return_cost * safety_margin`, opportunity passes
9. **Opportunity logging** writes to SQLite:
   - `loom-store/BatchWriter` buffers the opportunity record
   - Auto-flush when buffer reaches 1000 or 1000ms expires
10. **Query lane** (loomctl) retrieves opportunities from SQLite for user inspection.

## Key Design Decisions

- **Hand-rolled FIX vs. library**: Loom implements FIX 5.0 SP2 from scratch. This gives full control over message layout, serialization paths (zero-allocation in hot paths), and the ability to skip unused FIX features. Production messaging libraries add flexibility at the cost of latency.

- **Adjacency matrix with struct-of-arrays layout**: Most graph libraries use adjacency lists or hash maps. Loom uses a dense adjacency matrix (indices to currencies, packed f64 edge weights). Struct-of-arrays packing improves cache locality and SIMD-ability for Bellman-Ford, yielding 1.76µs cycle detection vs. 1ms target.

- **Bellman-Ford for negative cycles**: Loom uses the Bellman-Ford algorithm over alternatives (Floyd-Warshall, DFS-based cycle detection) because: (1) it's O(VE) with good constants for sparse graphs; (2) it naturally detects negative cycles in one pass; (3) cycle reconstruction is straightforward.

- **rust_decimal for financial arithmetic**: Floating-point (f64) alone introduces rounding errors in long conversion chains (BTC→ETH→USDC→USD = 4 multiplications). Loom uses `rust_decimal` for precise profit calculations where absolute accuracy matters, and f64 for graph edge weights (rates) where sub-microsecond latency is critical. A hybrid approach.

- **Gravitational gating**: Filtering opportunities by a safety margin (default 1.5x) avoids execution of marginal trades. The "gravitational" name reflects the use of Dijkstra shortest paths to measure the highest "cost" (minimum rate) to return capital back to base, ensuring execution is feasible across the cycle.

- **Unix socket + gRPC for control plane**: Loom uses gRPC over a Unix socket for loomctl communication. Unix sockets avoid the need for TCP port allocation and firewall rules in local deployment; gRPC provides schema evolution and type safety. TCP is optional for remote monitoring.

- **SQLite with WAL mode for persistence**: Loom stores opportunities and market ticks in SQLite with write-ahead logging (WAL) enabled. WAL allows concurrent reads (for queries from loomctl) while writes are in progress, supporting 27K msg/sec sustained throughput. Simpler than a message queue or distributed database, sufficient for single-daemon operation.

## Concurrency Model

Loom uses a single-threaded `tokio` runtime with `select!` branching:

- **FIX reader**: Awaits data on the TLS socket, triggers message parsing
- **Cycle check timer**: Periodic Bellman-Ford invocation (100ms default)
- **Heartbeat timer**: Periodic keepalive messages (75% of server heartbeat interval)
- **gRPC server task**: Handles loomctl connections concurrently (tokio manages thread pool)

The `ListenEventLoop` holds `Arc<RwLock<AdjacencyMatrix>>` (graph state) shared with the gRPC handler. The batch writer buffers writes and flushes to SQLite in its own task. No explicit message passing; state is shared via interior mutability. This model is simple and avoids the overhead of channels for frequent graph updates.

## Performance Targets

| Component | Target | Actual | Status |
|-----------|--------|--------|--------|
| FIX serialization (p99) | < 100µs | **< 1µs** | 100x better |
| Market data throughput | > 10K msg/sec | **27K msg/sec** | 2.7x better |
| Bellman-Ford cycle detection (p99) | < 1ms | **1.76µs** | 556x better |
| End-to-end latency (wire to DB) | < 100µs | **20–60µs** | Within budget |
| Batch flush latency (p99) | < 10ms | **< 5ms** | Within budget |

## Extension Points

- **Adding a new exchange**: Create a new crate (e.g., `loom-kraken-fix`) that implements the same FIX subset. Swap the transport/session in `loom-daemon/main.rs`.

- **Alternative cycle detection**: Implement a new algorithm in `loom-graph/` (e.g., DFS-based cycle detection, Tarjan's SCC). Replace the `detect_negative_cycle()` call in `event_loop.rs`.

- **Storage backend swap**: Implement the `BatchWriter` interface for a different database (PostgreSQL, RocksDB, ClickHouse). Swap the initialization in `event_loop.rs`.

- **Custom gating logic**: Modify `state_machine.rs` to use different filtering criteria instead of gravitational gating (e.g., volatility thresholds, time-of-day rules).

- **Order execution**: Phase 4 will add `loom-executor` to manage active lump sums, place orders, and reconcile fills. The event loop already defines `LumpSum` types and state machines.

## Non-Goals

- **General-purpose FIX library**: Loom's FIX implementation is Coinbase-specific. It does not attempt to support all FIX tags, sessions, or exchanges.

- **Multi-exchange**: Phase 1–3 are single-exchange (Coinbase). Multi-exchange arbitrage (triangulation across venues) is out of scope.

- **Order management system (OMS)**: Current phase (listen mode) logs opportunities only. Phase 4 will add execution, but Loom will not become a full OMS.

- **Backtesting framework**: Loom is a live trading engine. Historical simulation is out of scope.

- **User interface dashboard**: Loom exposes data via CLI and gRPC only. Web dashboards, visualization, or reporting tools are external.
