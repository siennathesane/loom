# loom-store

SQLite-backed opportunity and market data logging for the Loom arbitrage engine. This crate provides high-performance batch writing of opportunities, market ticks, and capital parcel state changes, with a schema optimized for analytics queries and on-disk persistence.

## Overview

`loom-store` manages a SQLite database with six tables: `ticks`, `opportunities`, `lump_sums`, `hops`, `reconciliations`, and `topology_events`. It implements a `BatchWriter` that buffers writes in memory and flushes on time or size thresholds, achieving 10K+ writes/second sustained throughput with sub-10ms p99 flush latency. All writes use transactions to ensure atomicity.

## Public API

- `BatchWriter` — Main interface for buffered inserts; auto-flushes on time or size limits
- `TickBuffer` — Raw market data (timestamp, instrument, side, price, quantity)
- `OpportunityBuffer` — Detected arbitrage cycle (profit, gating status, cycle path)
- `initialize_database(path)` — Create SQLite file and schema
- `enable_wal_mode(conn)` — Enable Write-Ahead Logging for concurrency
- `create_tables(conn)` — Schema creation

## Usage

```rust
use loom_store::{initialize_database, BatchWriter, TickBuffer};

// Create or open the database
let conn = initialize_database("loom.db")?;

// Create a writer with configurable buffer size and flush interval
let mut writer = BatchWriter::new(
    conn,
    1000,  // max_buffer_size (auto-flush after 1000 records)
    1000,  // flush_interval_ms
);

// Buffer a market tick (writes go to memory, not disk yet)
writer.buffer_tick(TickBuffer {
    timestamp: 1234567890,
    instrument: "BTC-USD".into(),
    side: "bid".into(),
    price: "50000.0".into(),
    quantity: "0.1".into(),
})?;

// On shutdown, flush any remaining buffered data
writer.shutdown()?;
```

## Database Schema

- `ticks` — Raw market data with indexes on (timestamp, instrument)
- `opportunities` — Detected cycles with indexes on detected_at and gating status
- `lump_sums` — Capital parcel state with indexes on state and currency
- `hops` — Trade execution log with foreign key to lump_sums
- `reconciliations` — Balance verification records
- `topology_events` — Market structure changes (pair suspension, node outages)

All operations use transactions and foreign key constraints are enabled.

## Dependencies on other Loom crates

None — `loom-store` is self-contained.

## License

Licensed under AGPL-3.0-or-later. See [../LICENSE](../LICENSE).
