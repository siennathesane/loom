# Phase 3 Integration Plan: Listen Mode
## Loom Cryptocurrency Arbitrage Engine

**Document Version:** 1.0
**Date:** 2026-02-12
**Author:** protocol-specialist
**Status:** Draft for Review

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [Component Integration](#3-component-integration)
4. [Data Structures](#4-data-structures)
5. [Event Loop Design](#5-event-loop-design)
6. [SQLite Schema](#6-sqlite-schema)
7. [Performance Budget](#7-performance-budget)
8. [Error Handling](#8-error-handling)
9. [Testing Strategy](#9-testing-strategy)
10. [Implementation Roadmap](#10-implementation-roadmap)

---

## 1. Executive Summary

Phase 3 (Listen Mode) integrates the FIX protocol layer (Phase 1) with the graph engine (Phase 2) to create a fully functional arbitrage detection system that:

- Consumes real-time L3 market data from Coinbase FIX gateway
- Maintains an up-to-date exchange rate graph (adjacency matrix)
- Detects arbitrage opportunities via Bellman-Ford negative cycle detection
- Logs all opportunities to SQLite for analysis
- Tracks "lump sum" virtual positions through their lifecycle

**Mode:** Passive observation only (no order submission)

**Key Constraints:**
- Sub-100μs end-to-end latency target (FIX parse → opportunity logged)
- Zero missed opportunities (complete detection)
- Correct decimal arithmetic throughout pipeline
- Durable logging (SQLite WAL mode, fsync on flush)

**Performance Headroom from Phase 1:**
- FIX parsing: < 1μs (99μs available for graph + logging)
- Market data throughput: 27K msg/sec (vs 1-5K typical Coinbase rate)
- Memory: Zero-allocation hot paths, predictable usage

---

## 2. Architecture Overview

### 2.1 High-Level Data Flow

```
┌─────────────────┐
│  Coinbase FIX   │
│    Gateway      │
└────────┬────────┘
         │ TLS
         │ MarketDataIncrementalRefresh
         │ SecurityStatus
         │ ExecutionReport (if Active)
         ▼
┌─────────────────┐
│  FIX Transport  │◄─── Phase 1: loom-fix
│  + Session Mgr  │     (TLS, heartbeats, HMAC auth)
└────────┬────────┘
         │ Parsed FixMessage
         ▼
┌─────────────────┐
│  Event Router   │◄─── NEW: Phase 3 integration
│  (tokio loop)   │     (message dispatch, coordination)
└────────┬────────┘
         │
         ├────────► MarketDataIncrementalRefresh
         │          └──▶ Update AdjacencyMatrix
         │                └──▶ Trigger Bellman-Ford (if top-of-book changed)
         │                      └──▶ Negative cycle? ──▶ Log Opportunity
         │
         ├────────► SecurityStatus
         │          └──▶ Update trading halts
         │                └──▶ Invalidate affected edges
         │
         └────────► ExecutionReport (Active mode only)
                    └──▶ Update LumpSum state
                          └──▶ Log state transition

┌─────────────────┐
│  Graph Engine   │◄─── Phase 2: loom-graph
│  (AdjacencyMx)  │     (Bellman-Ford, Dijkstra, snapshots)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  SQLite Logger  │◄─── NEW: Phase 3 database
│  (WAL mode)     │     (ticks, opportunities, lump_sums)
└─────────────────┘
```

### 2.2 Thread Model

**Single-threaded event loop (tokio runtime):**

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(listen_mode())
}
```

**Rationale:**
- Phase 1 performance: sub-microsecond FIX parsing allows single-threaded
- Eliminates synchronization overhead (no locks, no atomic contention)
- Simpler reasoning about state updates (sequential execution)
- Bellman-Ford is O(VE) but fast enough for <200 currencies (~10-50μs)

**Future:** Can switch to multi-threaded if profiling reveals bottlenecks

### 2.3 Module Structure

```
loom/
├── loom-fix/         (Phase 1 - FIX protocol)
│   ├── src/
│   │   ├── transport.rs
│   │   ├── session/
│   │   ├── market_data.rs
│   │   └── order_entry.rs
│   └── tests/
│
├── loom-graph/       (Phase 2 - graph engine)
│   ├── src/
│   │   ├── adjacency_matrix.rs
│   │   ├── bellman_ford.rs
│   │   ├── dijkstra.rs
│   │   └── snapshot.rs
│   └── tests/
│
├── loom-listen/      (Phase 3 - Listen Mode) ← NEW
│   ├── src/
│   │   ├── main.rs              # Entry point
│   │   ├── event_loop.rs        # Main event loop (tokio::select!)
│   │   ├── graph_updater.rs     # Market data → AdjacencyMatrix
│   │   ├── opportunity_detector.rs  # Bellman-Ford trigger logic
│   │   ├── lump_sum.rs          # LumpSum state machine
│   │   ├── database.rs          # SQLite integration
│   │   └── config.rs            # Configuration
│   └── tests/
│
└── Cargo.toml
```

---

## 3. Component Integration

### 3.1 FIX Protocol → Graph Engine

**Integration Point:** `MarketDataIncrementalRefresh` → `AdjacencyMatrix::update_edge()`

#### Message Flow

```rust
// Parse FIX message (Phase 1)
let msg = MarketDataIncrementalRefresh::from_fix_message(&fix_msg)?;

// Extract market data entries
for entry in msg.entries {
    match entry.entry_type {
        MDEntryType::Bid => {
            // Best bid: BTC-USD @ 50000.00 (size: 1.5 BTC)
            // This is the rate to SELL BTC (buy USD)
            // Edge: BTC → USD with rate 50000.00

            let from_currency = "BTC";
            let to_currency = "USD";
            let rate = entry.price;  // Decimal: 50000.00
            let fee_bps = 30;  // 0.30% (30 basis points)

            graph.update_edge(from_currency, to_currency, rate, fee_bps)?;
        }

        MDEntryType::Offer => {
            // Best offer: BTC-USD @ 50001.00 (size: 2.0 BTC)
            // This is the rate to BUY BTC (sell USD)
            // Edge: USD → BTC with rate 1/50001.00 = 0.00001999...

            let from_currency = "USD";
            let to_currency = "BTC";
            let rate = Decimal::ONE / entry.price;  // Exact decimal arithmetic
            let fee_bps = 30;

            graph.update_edge(from_currency, to_currency, rate, fee_bps)?;
        }

        _ => {} // Ignore other entry types (Trade, etc.)
    }
}
```

**Key Design Decisions:**

1. **Currency Pair Parsing:**
   - FIX Symbol: "BTC-USD"
   - Parse into: base="BTC", quote="USD"
   - Bid edge: BTC → USD
   - Offer edge: USD → BTC

2. **Rate Inversion:**
   - Offer price 50001.00 → rate = 1/50001.00
   - Use rust_decimal for exact arithmetic (no f64 division!)
   - Handle zero prices (reject as invalid)

3. **Fee Model:**
   - Coinbase maker/taker fees: ~0.30-0.50% (30-50 bps)
   - Store as constant per exchange (configurable)
   - update_edge() computes: weight = -ln(rate × (1 - fee/10000))

4. **Top-of-Book Only:**
   - Only track best bid/offer (level 0)
   - Ignore deeper book levels (not needed for arbitrage detection)
   - Reduces update frequency and graph complexity

### 3.2 Graph Updates → Opportunity Detection

**Integration Point:** `AdjacencyMatrix::update_edge()` → `detect_negative_cycle()`

#### Trigger Strategy

**Option A: Trigger on Every Update (Naive)**
```rust
graph.update_edge(from, to, rate, fee)?;
let result = detect_negative_cycle(&graph, usd_index);
```
❌ Too expensive! Bellman-Ford is O(VE) ~10-50μs
❌ Most updates don't change arbitrage (e.g., mid-quote ticks)

**Option B: Trigger on Top-of-Book Changes (Smart)** ✅
```rust
let old_rate = graph.get_rate(from, to);
graph.update_edge(from, to, rate, fee)?;
let new_rate = graph.get_rate(from, to);

// Only run Bellman-Ford if best price actually changed
if new_rate != old_rate {
    let result = detect_negative_cycle(&graph, usd_index);
    if let BellmanFordResult::NegativeCycle(cycle) = result {
        log_opportunity(cycle, &graph, timestamp);
    }
}
```
✅ Bellman-Ford only on meaningful updates
✅ Reduces CPU usage by 10-100x (most ticks are mid-quote)

**Option C: Periodic Snapshot (Batched)** (Future optimization)
```rust
// Accumulate updates for 100ms, then run Bellman-Ford
// Tradeoff: Lower CPU but 100ms detection delay
```

**Recommendation:** Start with Option B (trigger on change), profile in production

### 3.3 Opportunity Detection → SQLite Logging

**Integration Point:** `BellmanFordResult::NegativeCycle` → SQLite insert

#### Deduplication Strategy

**Problem:** Same cycle might be detected on consecutive ticks
**Solution:** Deduplicate by cycle signature + time window

```rust
struct OpportunityDeduplicator {
    recent_opportunities: HashMap<CycleSignature, Instant>,
    dedup_window: Duration,  // e.g., 1 second
}

impl OpportunityDeduplicator {
    fn should_log(&mut self, cycle: &[NodeIndex], graph: &AdjacencyMatrix) -> bool {
        let signature = CycleSignature::from_cycle(cycle, graph);

        // Check if we've seen this cycle recently
        if let Some(last_seen) = self.recent_opportunities.get(&signature) {
            if last_seen.elapsed() < self.dedup_window {
                return false;  // Skip duplicate
            }
        }

        // New or expired - log it
        self.recent_opportunities.insert(signature, Instant::now());

        // Cleanup old entries (simple LRU)
        self.recent_opportunities.retain(|_, time| time.elapsed() < self.dedup_window);

        true
    }
}

#[derive(Hash, Eq, PartialEq)]
struct CycleSignature {
    /// Sorted list of currencies in cycle (e.g., ["BTC", "ETH", "USD"])
    currencies: Vec<String>,
    /// Sign of the weight (positive = profitable, negative should never happen)
    is_profitable: bool,
}
```

**Rationale:**
- 1-second dedup window: reasonable for human-readable logs
- Cycle signature: currency set (not order, since cycles can start anywhere)
- Cleanup: evict old entries to prevent unbounded memory growth

### 3.4 SQLite Write Strategy

**Batched Writes with Periodic Flush:**

```rust
struct DatabaseLogger {
    conn: rusqlite::Connection,
    pending_ticks: Vec<Tick>,
    pending_opportunities: Vec<Opportunity>,
    last_flush: Instant,
    flush_interval: Duration,  // e.g., 1 second
    max_batch_size: usize,     // e.g., 10,000 records
}

impl DatabaseLogger {
    async fn log_tick(&mut self, tick: Tick) {
        self.pending_ticks.push(tick);

        // Flush if batch full or interval elapsed
        if self.pending_ticks.len() >= self.max_batch_size
            || self.last_flush.elapsed() >= self.flush_interval
        {
            self.flush().await?;
        }
    }

    async fn flush(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;

        // Batch insert ticks
        for tick in self.pending_ticks.drain(..) {
            tx.execute(
                "INSERT INTO ticks (timestamp_ns, symbol, bid, offer, bid_size, offer_size) VALUES (?, ?, ?, ?, ?, ?)",
                params![tick.timestamp_ns, tick.symbol, tick.bid.to_string(), tick.offer.to_string(), tick.bid_size.to_string(), tick.offer_size.to_string()],
            )?;
        }

        // Batch insert opportunities
        for opp in self.pending_opportunities.drain(..) {
            tx.execute(
                "INSERT INTO opportunities (timestamp_ns, cycle_json, total_weight, profit_rate) VALUES (?, ?, ?, ?)",
                params![opp.timestamp_ns, opp.cycle_json, opp.total_weight, opp.profit_rate.to_string()],
            )?;
        }

        tx.commit()?;
        self.last_flush = Instant::now();
        Ok(())
    }
}
```

**Performance Trade-offs:**
- **1-second flush:** Good balance of durability and performance
- **Batch inserts:** ~100x faster than individual INSERTs
- **WAL mode:** Allows concurrent reads during writes
- **No fsync per row:** Rely on periodic flush (acceptable for Listen Mode)

---

## 4. Data Structures

### 4.1 LumpSum State Machine

**Purpose:** Track virtual positions ("lump sums") through their lifecycle

**State Diagram:**
```
     PARKED
        │
        │ (opportunity detected)
        ▼
   OPPORTUNITY ──────┐
        │            │
        │ (Active    │ (opportunity expired/filled by others)
        │  mode      │
        │  only)     │
        ▼            ▼
   EXECUTING      PARKED
        │
        ├──────► FILLED (success)
        │
        └──────► REJECTED (failure)
```

**Data Structure:**

```rust
use rust_decimal::Decimal;
use std::time::SystemTime;

/// Lump sum unique identifier
pub type LumpSumId = u64;

/// Current state of a lump sum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LumpSumState {
    /// Parked in base currency, no active opportunity
    Parked,

    /// Opportunity detected, awaiting execution (Active mode only)
    Opportunity,

    /// Orders submitted, awaiting fills (Active mode only)
    Executing,

    /// All orders filled successfully (Active mode only)
    Filled,

    /// Order rejected or partially filled (Active mode only)
    Rejected,
}

/// A lump sum represents a fixed quantity of base currency that we track
pub struct LumpSum {
    /// Unique identifier
    pub id: LumpSumId,

    /// Human-readable name (e.g., "Lump A", "Lump B")
    pub name: String,

    /// Base currency (e.g., "USD")
    pub base_currency: String,

    /// Current currency (e.g., "BTC" if we've traded into BTC)
    pub current_currency: String,

    /// Current quantity (exact decimal)
    pub current_size: Decimal,

    /// Current state
    pub state: LumpSumState,

    /// State transition history
    pub history: Vec<StateTransition>,
}

/// Record of a state transition
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub timestamp: SystemTime,
    pub from_state: LumpSumState,
    pub to_state: LumpSumState,
    pub reason: String,
    pub cycle: Option<Vec<String>>,  // Currency cycle if transitioning to Opportunity
}

impl LumpSum {
    /// Create a new lump sum in PARKED state
    pub fn new(id: LumpSumId, name: String, base_currency: String, initial_size: Decimal) -> Self {
        Self {
            id,
            name,
            base_currency: base_currency.clone(),
            current_currency: base_currency,
            current_size: initial_size,
            state: LumpSumState::Parked,
            history: vec![],
        }
    }

    /// Transition to a new state
    pub fn transition(&mut self, new_state: LumpSumState, reason: String, cycle: Option<Vec<String>>) {
        let transition = StateTransition {
            timestamp: SystemTime::now(),
            from_state: self.state,
            to_state: new_state,
            reason,
            cycle,
        };

        self.history.push(transition);
        self.state = new_state;
    }

    /// Check if lump sum is available for new opportunities
    pub fn is_available(&self) -> bool {
        matches!(self.state, LumpSumState::Parked)
    }
}
```

**Listen Mode Behavior:**
- All lump sums remain in PARKED state
- Opportunity detection updates LumpSum::history with detected cycles
- No actual trading occurs
- Provides foundation for Active mode implementation

### 4.2 Opportunity Record

```rust
use serde::{Serialize, Deserialize};

/// An arbitrage opportunity detected by Bellman-Ford
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    /// Nanosecond timestamp (SystemTime as nanos since UNIX_EPOCH)
    pub timestamp_ns: u128,

    /// Ordered sequence of currencies forming the cycle
    /// Example: ["USD", "BTC", "ETH", "USD"]
    pub cycle: Vec<String>,

    /// Total weight of the cycle (negative = profitable)
    /// In -ln space, so exp(-weight) = profit multiplier
    pub total_weight: f64,

    /// Profit rate (e.g., 1.05 = 5% profit)
    /// Computed as: exp(-total_weight)
    pub profit_rate: Decimal,

    /// Individual hop details (for analysis)
    pub hops: Vec<Hop>,
}

/// A single hop in an arbitrage cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hop {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: Decimal,           // Exact exchange rate
    pub fee_bps: u32,            // Fee in basis points
    pub effective_rate: Decimal, // rate × (1 - fee/10000)
}

impl Opportunity {
    /// Create from Bellman-Ford cycle and graph
    pub fn from_cycle(
        cycle: &[NodeIndex],
        graph: &AdjacencyMatrix,
        timestamp: SystemTime,
    ) -> Self {
        let timestamp_ns = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let mut total_weight = 0.0;
        let mut profit_rate = Decimal::ONE;
        let mut hops = Vec::new();

        // Convert NodeIndex to currency symbols and extract hop details
        let currencies: Vec<String> = cycle
            .iter()
            .map(|&idx| graph.get_symbol(idx).unwrap().to_string())
            .collect();

        for i in 0..cycle.len() {
            let from_idx = cycle[i];
            let to_idx = cycle[(i + 1) % cycle.len()];

            let weight = graph.get_weight(from_idx, to_idx);
            let rate = graph.get_rate(from_idx, to_idx);

            total_weight += weight;
            profit_rate *= rate;  // Exact decimal multiplication

            hops.push(Hop {
                from_currency: graph.get_symbol(from_idx).unwrap().to_string(),
                to_currency: graph.get_symbol(to_idx).unwrap().to_string(),
                rate,
                fee_bps: 30,  // TODO: Get from config
                effective_rate: rate * (Decimal::ONE - Decimal::new(30, 4)), // 1 - 0.0030
            });
        }

        Self {
            timestamp_ns,
            cycle: currencies,
            total_weight,
            profit_rate,
            hops,
        }
    }
}
```

---

## 5. Event Loop Design

### 5.1 Main Event Loop (tokio::select!)

```rust
use tokio::select;
use tokio::signal;
use tokio::time::{interval, Duration};

pub async fn listen_mode(config: Config) -> Result<()> {
    // Initialize components
    let mut fix_client = FixClient::connect(&config.fix).await?;
    let mut graph = AdjacencyMatrix::new(config.currencies.clone());
    let mut opportunity_detector = OpportunityDetector::new(config.usd_index);
    let mut database = DatabaseLogger::new(&config.database_path).await?;
    let mut lump_sums = initialize_lump_sums(&config);

    // Periodic flush timer
    let mut flush_timer = interval(Duration::from_secs(1));

    // Shutdown signal (Ctrl+C)
    let mut shutdown = signal::ctrl_c();

    println!("🚀 Loom Listen Mode started");
    println!("📡 Subscribed to {} currency pairs", config.symbols.len());

    loop {
        select! {
            // FIX message received
            msg_result = fix_client.recv() => {
                match msg_result {
                    Ok(fix_msg) => {
                        handle_fix_message(
                            &fix_msg,
                            &mut graph,
                            &mut opportunity_detector,
                            &mut database,
                            &mut lump_sums,
                        ).await?;
                    }
                    Err(e) => {
                        eprintln!("❌ FIX receive error: {}", e);
                        // Reconnection handled by FixClient internally
                    }
                }
            }

            // Periodic database flush
            _ = flush_timer.tick() => {
                database.flush().await?;
            }

            // Shutdown signal (Ctrl+C)
            _ = &mut shutdown => {
                println!("🛑 Shutdown signal received");
                break;
            }
        }
    }

    // Cleanup
    database.flush().await?;
    fix_client.disconnect().await?;

    println!("✅ Loom Listen Mode stopped gracefully");
    Ok(())
}
```

### 5.2 Message Handler

```rust
async fn handle_fix_message(
    fix_msg: &FixMessage,
    graph: &mut AdjacencyMatrix,
    detector: &mut OpportunityDetector,
    database: &mut DatabaseLogger,
    lump_sums: &mut Vec<LumpSum>,
) -> Result<()> {
    let msg_type = fix_msg.get_msg_type();

    match msg_type {
        // Market data update (primary message type)
        MsgType::MarketDataIncrementalRefresh => {
            let md = MarketDataIncrementalRefresh::from_fix_message(fix_msg)
                .ok_or("Failed to parse market data")?;

            handle_market_data(md, graph, detector, database, lump_sums).await?;
        }

        // Trading halt/resume
        MsgType::SecurityStatus => {
            let status = SecurityStatus::from_fix_message(fix_msg)
                .ok_or("Failed to parse security status")?;

            handle_security_status(status, graph, database).await?;
        }

        // Heartbeat (no action needed, handled by session layer)
        MsgType::Heartbeat => {
            // No-op, session layer handles this
        }

        // Execution report (Active mode only, ignore in Listen mode)
        MsgType::ExecutionReport => {
            // TODO: Implement in Phase 4 (Active mode)
        }

        _ => {
            // Ignore other message types
        }
    }

    Ok(())
}
```

### 5.3 Market Data Handler

```rust
async fn handle_market_data(
    md: MarketDataIncrementalRefresh,
    graph: &mut AdjacencyMatrix,
    detector: &mut OpportunityDetector,
    database: &mut DatabaseLogger,
    lump_sums: &mut Vec<LumpSum>,
) -> Result<()> {
    let timestamp = SystemTime::now();

    // Process each entry in the market data update
    for entry in md.entries {
        // Parse symbol (e.g., "BTC-USD" → base="BTC", quote="USD")
        let (base, quote) = parse_symbol(&entry.symbol)?;

        // Log tick to database
        database.log_tick(Tick {
            timestamp_ns: timestamp.duration_since(UNIX_EPOCH).unwrap().as_nanos(),
            symbol: entry.symbol.clone(),
            bid: entry.price,
            offer: entry.price,  // TODO: Track separately
            bid_size: entry.size,
            offer_size: entry.size,
        }).await;

        // Update graph based on entry type
        match entry.entry_type {
            MDEntryType::Bid => {
                // Best bid: rate to SELL base (base → quote)
                let old_rate = graph.get_rate(&base, &quote);
                graph.update_edge(&base, &quote, entry.price, 30)?;
                let new_rate = graph.get_rate(&base, &quote);

                // Check if top-of-book changed
                if new_rate != old_rate {
                    check_for_opportunities(graph, detector, database, lump_sums, timestamp).await?;
                }
            }

            MDEntryType::Offer => {
                // Best offer: rate to BUY base (quote → base)
                let rate = Decimal::ONE / entry.price;  // Exact inversion
                let old_rate = graph.get_rate(&quote, &base);
                graph.update_edge(&quote, &base, rate, 30)?;
                let new_rate = graph.get_rate(&quote, &base);

                // Check if top-of-book changed
                if new_rate != old_rate {
                    check_for_opportunities(graph, detector, database, lump_sums, timestamp).await?;
                }
            }

            _ => {}  // Ignore other entry types
        }
    }

    Ok(())
}
```

### 5.4 Opportunity Detection

```rust
async fn check_for_opportunities(
    graph: &AdjacencyMatrix,
    detector: &mut OpportunityDetector,
    database: &mut DatabaseLogger,
    lump_sums: &mut Vec<LumpSum>,
    timestamp: SystemTime,
) -> Result<()> {
    // Run Bellman-Ford from USD
    let result = detect_negative_cycle(graph, detector.usd_index);

    if let BellmanFordResult::NegativeCycle(cycle) = result {
        // Create opportunity record
        let opportunity = Opportunity::from_cycle(&cycle, graph, timestamp);

        // Deduplicate (skip if we've seen this cycle recently)
        if !detector.should_log(&cycle, graph) {
            return Ok(());
        }

        // Log to console
        println!(
            "💰 Opportunity detected! Cycle: {:?}, Profit: {:.4}%",
            opportunity.cycle,
            (opportunity.profit_rate - Decimal::ONE) * Decimal::from(100)
        );

        // Log to database
        database.log_opportunity(opportunity.clone()).await;

        // Update lump sum history (Listen Mode: just log, don't execute)
        for lump_sum in lump_sums.iter_mut() {
            if lump_sum.is_available() {
                lump_sum.history.push(StateTransition {
                    timestamp,
                    from_state: LumpSumState::Parked,
                    to_state: LumpSumState::Parked,  // No state change in Listen Mode
                    reason: format!("Opportunity detected: {:.4}% profit", (opportunity.profit_rate - Decimal::ONE) * Decimal::from(100)),
                    cycle: Some(opportunity.cycle.clone()),
                });
            }
        }
    }

    Ok(())
}
```

---

## 6. SQLite Schema

### 6.1 Database Configuration

```sql
-- Enable WAL mode for concurrent reads during writes
PRAGMA journal_mode = WAL;

-- Synchronous = NORMAL: Good balance of durability and performance
-- Writes are fsync'd at checkpoints, not every commit
PRAGMA synchronous = NORMAL;

-- Cache size: 10MB (2500 pages × 4KB)
PRAGMA cache_size = -10000;

-- Foreign keys enabled (data integrity)
PRAGMA foreign_keys = ON;
```

### 6.2 Table Schemas

#### Table: `ticks`

**Purpose:** Store every market data tick (L3 updates)

```sql
CREATE TABLE ticks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,  -- Nanoseconds since UNIX epoch
    symbol TEXT NOT NULL,           -- e.g., "BTC-USD"
    bid TEXT NOT NULL,              -- Decimal as string (exact)
    offer TEXT NOT NULL,            -- Decimal as string (exact)
    bid_size TEXT NOT NULL,         -- Decimal as string (exact)
    offer_size TEXT NOT NULL        -- Decimal as string (exact)
);

-- Index for time-range queries
CREATE INDEX idx_ticks_timestamp ON ticks(timestamp_ns);

-- Index for symbol queries
CREATE INDEX idx_ticks_symbol ON ticks(symbol);
```

**Storage Estimate:**
- ~100 bytes per row
- 10K ticks/sec × 3600 sec/hour = 36M ticks/hour
- ~3.6 GB/hour (~86 GB/day)
- **Mitigation:** Downsample or archive old data periodically

#### Table: `opportunities`

**Purpose:** Store detected arbitrage opportunities

```sql
CREATE TABLE opportunities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,  -- Nanoseconds since UNIX epoch
    cycle_json TEXT NOT NULL,       -- JSON array of currencies (e.g., ["USD","BTC","ETH","USD"])
    total_weight REAL NOT NULL,     -- Total weight in -ln space (negative = profitable)
    profit_rate TEXT NOT NULL,      -- Decimal profit rate (e.g., "1.05" = 5% profit)

    UNIQUE(timestamp_ns, cycle_json)  -- Prevent exact duplicates
);

-- Index for time-range queries
CREATE INDEX idx_opportunities_timestamp ON opportunities(timestamp_ns);

-- Index for profit analysis
CREATE INDEX idx_opportunities_profit ON opportunities(profit_rate);
```

**Storage Estimate:**
- ~200 bytes per row
- Depends on opportunity frequency (1-100 per hour expected)
- Manageable size even for long-running systems

#### Table: `hops`

**Purpose:** Store individual hops within each opportunity (for detailed analysis)

```sql
CREATE TABLE hops (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    opportunity_id INTEGER NOT NULL,
    hop_index INTEGER NOT NULL,     -- Position in cycle (0-indexed)
    from_currency TEXT NOT NULL,
    to_currency TEXT NOT NULL,
    rate TEXT NOT NULL,             -- Decimal exchange rate (exact)
    fee_bps INTEGER NOT NULL,       -- Fee in basis points
    effective_rate TEXT NOT NULL,   -- rate × (1 - fee/10000)

    FOREIGN KEY (opportunity_id) REFERENCES opportunities(id) ON DELETE CASCADE
);

-- Index for opportunity lookups
CREATE INDEX idx_hops_opportunity ON hops(opportunity_id);
```

#### Table: `lump_sums`

**Purpose:** Track lump sum lifecycle and state transitions

```sql
CREATE TABLE lump_sums (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    initial_size TEXT NOT NULL      -- Decimal as string
);

CREATE TABLE lump_sum_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    lump_sum_id INTEGER NOT NULL,
    timestamp_ns INTEGER NOT NULL,
    from_state TEXT NOT NULL,       -- "PARKED", "OPPORTUNITY", etc.
    to_state TEXT NOT NULL,
    reason TEXT NOT NULL,
    cycle_json TEXT,                -- Optional: cycle if transitioning to OPPORTUNITY

    FOREIGN KEY (lump_sum_id) REFERENCES lump_sums(id) ON DELETE CASCADE
);

-- Index for lump sum history queries
CREATE INDEX idx_lump_sum_states_lump_sum ON lump_sum_states(lump_sum_id);
CREATE INDEX idx_lump_sum_states_timestamp ON lump_sum_states(timestamp_ns);
```

#### Table: `topology_events`

**Purpose:** Log graph topology changes (trading halts, new currencies, etc.)

```sql
CREATE TABLE topology_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,
    event_type TEXT NOT NULL,       -- "HALT", "RESUME", "NEW_CURRENCY", etc.
    symbol TEXT NOT NULL,
    details TEXT                    -- JSON or free-form description
);

-- Index for time-range queries
CREATE INDEX idx_topology_events_timestamp ON topology_events(timestamp_ns);
```

### 6.3 Database Initialization

```rust
use rusqlite::{Connection, Result};

pub fn initialize_database(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    // Configure SQLite
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA cache_size = -10000;
        PRAGMA foreign_keys = ON;
    ")?;

    // Create tables
    conn.execute_batch(include_str!("schema.sql"))?;

    Ok(conn)
}
```

---

## 7. Performance Budget

### 7.1 Latency Breakdown

**Target:** < 100μs end-to-end (market data received → opportunity logged)

| Component | Budget | Expected | Margin | Notes |
|-----------|--------|----------|--------|-------|
| FIX parsing | 10μs | **< 1μs** | +9μs | Phase 1 measured |
| Graph update | 10μs | ~2μs | +8μs | Single edge update |
| Bellman-Ford | 50μs | 10-50μs | 0-40μs | O(VE), ~200 currencies |
| Opportunity log | 20μs | ~5μs | +15μs | In-memory buffer |
| Database flush | N/A | ~1ms | N/A | Batched, async |
| **Total** | **100μs** | **~20-60μs** | **+40-80μs** | **✅ Within budget** |

**Key Observations:**
- FIX parsing has 9μs headroom (100x better than target)
- Bellman-Ford dominates latency (10-50μs depending on graph size)
- Database writes are batched and don't block critical path
- ~40-80μs margin for future features

### 7.2 Throughput Budget

**Coinbase FIX Market Data Rate:**
- Typical: 1,000-5,000 msg/sec
- Peak (volatile markets): 10,000-20,000 msg/sec

**Loom Capability:**
- FIX parsing: 27,000 msg/sec (Phase 1 measured)
- Graph updates: ~100,000 updates/sec (2μs each)
- Bellman-Ford: 1,000-10,000 runs/sec (10-50μs each)

**Bottleneck Analysis:**
- **Not FIX parsing:** 27K msg/sec >> 20K peak
- **Not graph updates:** 100K >> 20K peak
- **Potential bottleneck:** Bellman-Ford if triggered on every update
  - **Mitigation:** Only trigger on top-of-book changes (reduces by 10-100x)

**Recommendation:** Monitor Bellman-Ford frequency in production, implement batching if needed

### 7.3 Memory Budget

| Component | Memory | Notes |
|-----------|--------|-------|
| Adjacency matrix | ~320KB | 200 currencies × 200 × (8B f64 + 16B Decimal) |
| FIX messages | ~10MB | Ring buffer, ~1000 messages × 10KB avg |
| Database buffer | ~1MB | Pending ticks/opportunities before flush |
| Lump sums | ~10KB | 10 lump sums × 1KB each |
| **Total** | **~12MB** | Fits comfortably in L3 cache |

**Scaling:** Up to 1,000 currencies → ~8MB adjacency matrix (still manageable)

---

## 8. Error Handling

### 8.1 Error Categories

1. **FIX Protocol Errors**
   - Connection loss → Automatic reconnection (Phase 1 transport layer)
   - Invalid message → Log and skip
   - Authentication failure → Fatal, exit with error

2. **Graph Errors**
   - Unknown currency → Log warning, skip update
   - Invalid rate (zero/negative) → Log error, reject update
   - Cycle detection failure → Log error, continue (non-fatal)

3. **Database Errors**
   - Write failure → Retry 3 times, then log to stderr
   - Disk full → Fatal, exit with error
   - Corruption → Fatal, exit with error

4. **System Errors**
   - Out of memory → Fatal, exit with error
   - Signal (SIGTERM, SIGINT) → Graceful shutdown
   - Panic → Log stack trace, attempt database flush, exit

### 8.2 Error Recovery Strategy

```rust
pub enum ErrorSeverity {
    /// Ignore and continue (e.g., malformed message)
    Ignore,

    /// Log warning but continue (e.g., unknown currency)
    Warn,

    /// Log error and retry (e.g., database write failure)
    Retry { max_attempts: u32 },

    /// Fatal error, trigger graceful shutdown
    Fatal,
}

pub async fn handle_error(error: Error, severity: ErrorSeverity) -> Result<()> {
    match severity {
        ErrorSeverity::Ignore => {
            // Silently skip
            Ok(())
        }

        ErrorSeverity::Warn => {
            eprintln!("⚠️  Warning: {}", error);
            Ok(())
        }

        ErrorSeverity::Retry { max_attempts } => {
            for attempt in 1..=max_attempts {
                eprintln!("⚠️  Error (attempt {}/{}): {}", attempt, max_attempts, error);
                tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;

                // Retry operation (caller should implement retry logic)
                // ...
            }
            Err(error)
        }

        ErrorSeverity::Fatal => {
            eprintln!("❌ Fatal error: {}", error);
            // Trigger graceful shutdown
            // (caller should initiate shutdown sequence)
            Err(error)
        }
    }
}
```

### 8.3 Graceful Shutdown

```rust
pub async fn shutdown_sequence(
    fix_client: &mut FixClient,
    database: &mut DatabaseLogger,
) -> Result<()> {
    println!("🛑 Initiating graceful shutdown...");

    // 1. Stop accepting new FIX messages
    println!("  📡 Closing FIX connection...");
    fix_client.disconnect().await?;

    // 2. Flush pending database writes
    println!("  💾 Flushing database...");
    database.flush().await?;

    // 3. Close database connection
    println!("  💾 Closing database...");
    database.close().await?;

    println!("✅ Shutdown complete");
    Ok(())
}
```

---

## 9. Testing Strategy

### 9.1 Unit Tests

**Component:** Graph Updater
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_data_to_graph_update() {
        let mut graph = AdjacencyMatrix::new(vec!["USD".to_string(), "BTC".to_string()]);

        // Simulate bid update: BTC-USD @ 50000.00
        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Bid,
            price: Decimal::new(50000, 0),
            size: Decimal::new(15000, 4),  // 1.5 BTC
            entry_id: Some("ENTRY123".to_string()),
        };

        apply_market_data_entry(&entry, &mut graph, "BTC-USD").unwrap();

        // Verify edge: BTC → USD with rate 50000.00
        let rate = graph.get_rate("BTC", "USD");
        assert_eq!(rate, Decimal::new(50000, 0));
    }
}
```

**Component:** Opportunity Detector
```rust
#[test]
fn test_opportunity_deduplication() {
    let mut detector = OpportunityDetector::new(0);
    let graph = create_test_graph_with_cycle();

    let cycle = vec![0, 1, 2, 0];  // USD → BTC → ETH → USD

    // First detection: should log
    assert!(detector.should_log(&cycle, &graph));

    // Second detection within 1 second: should not log
    assert!(!detector.should_log(&cycle, &graph));

    // Wait 1.5 seconds
    std::thread::sleep(Duration::from_millis(1500));

    // Third detection after window: should log
    assert!(detector.should_log(&cycle, &graph));
}
```

### 9.2 Integration Tests

**Test:** End-to-End Market Data → Opportunity
```rust
#[tokio::test]
async fn test_end_to_end_opportunity_detection() {
    let config = Config::test_default();
    let mut graph = AdjacencyMatrix::new(vec!["USD".into(), "BTC".into(), "ETH".into()]);
    let mut detector = OpportunityDetector::new(0);
    let mut database = DatabaseLogger::new_in_memory().await.unwrap();
    let mut lump_sums = vec![];

    // Set up graph to have an arbitrage opportunity
    // USD → BTC: 0.00002 BTC/USD (1 BTC = 50000 USD)
    // BTC → ETH: 15 ETH/BTC
    // ETH → USD: 3400 USD/ETH
    // Cycle: USD → BTC → ETH → USD = 0.00002 × 15 × 3400 = 1.02 (2% profit!)

    graph.update_edge("USD", "BTC", Decimal::new(2, 5), 0).unwrap();  // 0.00002
    graph.update_edge("BTC", "ETH", Decimal::new(15, 0), 0).unwrap();
    graph.update_edge("ETH", "USD", Decimal::new(3400, 0), 0).unwrap();

    // Trigger opportunity detection
    check_for_opportunities(&graph, &mut detector, &mut database, &mut lump_sums, SystemTime::now())
        .await
        .unwrap();

    // Verify opportunity was logged
    let opportunities = database.get_opportunities().await.unwrap();
    assert_eq!(opportunities.len(), 1);

    let opp = &opportunities[0];
    assert_eq!(opp.cycle, vec!["USD", "BTC", "ETH", "USD"]);
    assert!(opp.profit_rate > Decimal::ONE);  // Profitable
}
```

**Test:** Database Batching
```rust
#[tokio::test]
async fn test_database_batching() {
    let mut database = DatabaseLogger::new_in_memory().await.unwrap();

    // Add 10,000 ticks without flushing
    for i in 0..10_000 {
        database.log_tick(Tick {
            timestamp_ns: i as u128,
            symbol: "BTC-USD".to_string(),
            bid: Decimal::new(50000 + i, 0),
            offer: Decimal::new(50001 + i, 0),
            bid_size: Decimal::ONE,
            offer_size: Decimal::ONE,
        }).await;
    }

    // Verify pending count
    assert_eq!(database.pending_count(), 10_000);

    // Flush
    database.flush().await.unwrap();

    // Verify all written
    assert_eq!(database.pending_count(), 0);

    let tick_count = database.count_ticks().await.unwrap();
    assert_eq!(tick_count, 10_000);
}
```

### 9.3 Performance Tests

**Test:** Latency Under Load
```rust
#[tokio::test]
async fn test_latency_under_load() {
    let mut graph = AdjacencyMatrix::new(create_200_currencies());
    let mut detector = OpportunityDetector::new(0);

    let mut latencies = Vec::new();

    // Simulate 10,000 market data updates
    for _ in 0..10_000 {
        let start = Instant::now();

        // Random edge update
        let from = rand::random::<usize>() % 200;
        let to = rand::random::<usize>() % 200;
        let rate = Decimal::new(rand::random::<i64>() % 100000, 2);

        graph.update_edge_by_index(from, to, rate, 30).unwrap();

        // Check for opportunities (worst case: every update)
        let _ = detect_negative_cycle(&graph, 0);

        latencies.push(start.elapsed());
    }

    // Calculate percentiles
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p99 = latencies[latencies.len() * 99 / 100];

    println!("Latency p50: {:?}, p99: {:?}", p50, p99);

    // Verify < 100μs target
    assert!(p99 < Duration::from_micros(100));
}
```

### 9.4 Failure Tests

**Test:** FIX Connection Loss Recovery
```rust
#[tokio::test]
async fn test_fix_reconnection() {
    let config = Config::test_default();
    let mut fix_client = FixClient::connect(&config.fix).await.unwrap();

    // Simulate connection loss
    fix_client.simulate_disconnect();

    // Attempt to receive message (should trigger reconnection)
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        fix_client.recv()
    ).await;

    // Should reconnect and receive message
    assert!(result.is_ok());
}
```

**Test:** Database Write Failure Recovery
```rust
#[tokio::test]
async fn test_database_write_failure_recovery() {
    let mut database = DatabaseLogger::new_in_memory().await.unwrap();

    // Fill database to trigger write failure
    database.simulate_disk_full();

    // Attempt to log tick
    let result = database.log_tick(create_test_tick()).await;

    // Should retry 3 times and then fail gracefully
    assert!(result.is_err());

    // Verify no data corruption
    assert_eq!(database.count_ticks().await.unwrap(), 0);
}
```

---

## 10. Implementation Roadmap

### 10.1 Phase 3.1: Core Integration (Week 1)

**Goal:** Basic FIX → Graph → SQLite pipeline working

**Tasks:**
1. ✅ Create `loom-listen` crate structure
2. ✅ Implement `Config` with TOML parsing
3. ✅ Implement `DatabaseLogger` with SQLite
4. ✅ Implement `GraphUpdater` (market data → adjacency matrix)
5. ✅ Implement `OpportunityDetector` (Bellman-Ford trigger)
6. ✅ Implement basic event loop (tokio::select!)
7. ✅ Unit tests for all components

**Deliverable:** Can connect to FIX, update graph, detect opportunities, log to SQLite

### 10.2 Phase 3.2: Optimization & Polish (Week 2)

**Goal:** Production-ready performance and reliability

**Tasks:**
1. ✅ Implement opportunity deduplication
2. ✅ Implement database batching and periodic flush
3. ✅ Add comprehensive error handling
4. ✅ Add graceful shutdown
5. ✅ Integration tests
6. ✅ Performance profiling and optimization
7. ✅ Documentation and deployment guide

**Deliverable:** Production-ready Listen Mode

### 10.3 Phase 3.3: Observability (Week 3)

**Goal:** Monitoring, metrics, and debugging tools

**Tasks:**
1. ✅ Add Prometheus metrics (opportunities/sec, latency percentiles)
2. ✅ Add structured logging (tracing crate)
3. ✅ Add SQLite analysis queries (most profitable cycles, frequency analysis)
4. ✅ Add Grafana dashboard template
5. ✅ Add CLI tool for querying database

**Deliverable:** Full observability stack

### 10.4 Exit Criteria

Phase 3 is **complete** when:

- [ ] Can connect to Coinbase FIX sandbox and receive market data
- [ ] Graph updates correctly for all currency pairs
- [ ] Bellman-Ford detects negative cycles with 100% accuracy
- [ ] Opportunities logged to SQLite with exact decimal arithmetic
- [ ] End-to-end latency < 100μs (p99)
- [ ] Zero memory leaks (24-hour run)
- [ ] Zero panics or crashes (24-hour run)
- [ ] Comprehensive test suite (>90% coverage)
- [ ] Documentation complete (API docs, deployment guide)

---

## Appendix A: Configuration Example

```toml
# loom-listen.toml

[fix]
address = "fix.coinbase.com:4198"
server_name = "fix.coinbase.com"
api_key = "your_api_key_here"
api_secret = "your_api_secret_here"
api_passphrase = "your_passphrase_here"
heartbeat_interval_secs = 30

[symbols]
# Currency pairs to subscribe to
pairs = [
    "BTC-USD",
    "ETH-USD",
    "BTC-ETH",
    # ... more pairs ...
]

[graph]
# Base currency for Bellman-Ford (typically USD)
base_currency = "USD"

# Fee in basis points (0.30% = 30 bps)
default_fee_bps = 30

[database]
path = "./loom_data.db"
flush_interval_secs = 1
max_batch_size = 10000

[opportunities]
# Deduplication window in seconds
dedup_window_secs = 1

# Minimum profit rate to log (e.g., 1.001 = 0.1% profit)
min_profit_rate = "1.001"

[lump_sums]
# Initial lump sums (Listen Mode: tracking only, no trading)
[[lump_sums.entries]]
id = 1
name = "Lump A"
base_currency = "USD"
initial_size = "10000.00"

[[lump_sums.entries]]
id = 2
name = "Lump B"
base_currency = "USD"
initial_size = "10000.00"
```

---

## Appendix B: Performance Optimization Checklist

- [x] **Phase 1 optimizations applied**
  - [x] Zero-allocation serialization
  - [x] rust_decimal for exact arithmetic
  - [x] TCP_NODELAY enabled
  - [x] Lock-free sequence numbers

- [ ] **Phase 3 optimizations**
  - [ ] Bellman-Ford only on top-of-book changes
  - [ ] Opportunity deduplication (1-second window)
  - [ ] Database batch writes (1-second flush)
  - [ ] SQLite WAL mode + normal sync
  - [ ] Single-threaded event loop (no context switching)

- [ ] **Future optimizations** (if needed)
  - [ ] SIMD for Bellman-Ford relaxation
  - [ ] Custom memory allocator (jemalloc)
  - [ ] CPU core pinning (Linux only)
  - [ ] Zero-copy FIX parsing (lifetime management)

---

**Document Status:** Draft for Review
**Next Steps:** Review with team, begin Phase 3.1 implementation

---

*Integration plan created by: protocol-specialist*
*Date: 2026-02-12*
*Phase 3: Listen Mode Architecture*
