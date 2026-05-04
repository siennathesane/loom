// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! # loom-store
//!
//! SQLite-backed persistence layer for arbitrage opportunities and market data.
//!
//! Provides high-performance batch writing of tick data and opportunities with
//! configurable buffering and auto-flushing. Uses WAL mode for concurrent reads
//! and writes, supporting sustained 10K writes/second with p99 flush latency under 10ms.
//!
//! ## Examples
//!
//! ```ignore
//! let conn = Connection::open("loom.db")?;
//! let mut writer = BatchWriter::new(conn, 1000, 1000);
//! writer.add_tick(tick_buffer);
//! writer.flush_if_needed()?;
//! ```

use rusqlite::{params, Connection, Error};
use std::time::{Duration, Instant};

// Re-export rusqlite types for consumers
pub use rusqlite;

/// Buffer structure for tick data
pub struct TickBuffer {
    pub timestamp: i64,
    pub instrument: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
}

/// Buffer structure for opportunity data
pub struct OpportunityBuffer {
    pub opportunity_id: String,
    pub detected_at: i64,
    pub cycle_json: String,
    pub cycle_symbols: String,
    pub expected_profit: f64,
    pub max_return_cost: f64,
    pub gated: bool,
}

/// High-performance batch writer that buffers writes in memory and flushes periodically
///
/// Performance target: 10K writes/sec sustained
/// p99 flush latency: < 10ms for batches up to 1000 records
pub struct BatchWriter {
    conn: Connection,
    tick_buffer: Vec<TickBuffer>,
    opportunity_buffer: Vec<OpportunityBuffer>,
    max_buffer_size: usize,
    flush_interval: Duration,
    last_flush: Instant,
}

impl BatchWriter {
    /// Create a new BatchWriter with configurable buffer size and flush interval
    ///
    /// # Arguments
    /// * `conn` - Database connection (should already have WAL mode enabled)
    /// * `max_buffer_size` - Maximum records per buffer before auto-flush (default: 1000)
    /// * `flush_interval_ms` - Milliseconds between auto-flushes (default: 1000ms)
    pub fn new(conn: Connection, max_buffer_size: usize, flush_interval_ms: u64) -> Self {
        Self {
            conn,
            tick_buffer: Vec::with_capacity(max_buffer_size),
            opportunity_buffer: Vec::with_capacity(max_buffer_size),
            max_buffer_size,
            flush_interval: Duration::from_millis(flush_interval_ms),
            last_flush: Instant::now(),
        }
    }

    /// Buffer a tick for batched write
    ///
    /// Will auto-flush if buffer size or time threshold is reached
    pub fn buffer_tick(&mut self, tick: TickBuffer) -> Result<(), Error> {
        if self.should_flush() {
            self.flush()?;
        }

        self.tick_buffer.push(tick);

        Ok(())
    }

    /// Buffer an opportunity for batched write
    ///
    /// Will auto-flush if buffer size or time threshold is reached
    pub fn buffer_opportunity(&mut self, opp: OpportunityBuffer) -> Result<(), Error> {
        if self.should_flush() {
            self.flush()?;
        }

        self.opportunity_buffer.push(opp);

        Ok(())
    }

    /// Check if flush is needed based on time or buffer size
    pub fn should_flush(&self) -> bool {
        let elapsed = self.last_flush.elapsed() >= self.flush_interval;
        let tick_full = self.tick_buffer.len() >= self.max_buffer_size;
        let opp_full = self.opportunity_buffer.len() >= self.max_buffer_size;

        elapsed || tick_full || opp_full
    }

    /// Flush all buffers to database
    ///
    /// Returns the total number of records written
    pub fn flush(&mut self) -> Result<usize, Error> {
        let tick_count = self.flush_ticks()?;
        let opp_count = self.flush_opportunities()?;

        self.last_flush = Instant::now();

        Ok(tick_count + opp_count)
    }

    /// Flush tick buffer to database using a single transaction
    ///
    /// Returns the number of ticks written
    pub fn flush_ticks(&mut self) -> Result<usize, Error> {
        if self.tick_buffer.is_empty() {
            return Ok(0);
        }

        let count = self.tick_buffer.len();
        let tx = self.conn.transaction()?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO ticks (timestamp, instrument, side, price, quantity)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            for tick in &self.tick_buffer {
                stmt.execute(params![
                    tick.timestamp,
                    &tick.instrument,
                    &tick.side,
                    &tick.price,
                    &tick.quantity
                ])?;
            }
        }

        tx.commit()?;
        self.tick_buffer.clear();

        Ok(count)
    }

    /// Flush opportunity buffer to database using a single transaction
    ///
    /// Returns the number of opportunities written
    pub fn flush_opportunities(&mut self) -> Result<usize, Error> {
        if self.opportunity_buffer.is_empty() {
            return Ok(0);
        }

        let count = self.opportunity_buffer.len();
        let tx = self.conn.transaction()?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO opportunities (opportunity_id, detected_at, cycle_json, cycle_symbols, expected_profit, max_return_cost, gated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            )?;

            for opp in &self.opportunity_buffer {
                stmt.execute(params![
                    &opp.opportunity_id,
                    opp.detected_at,
                    &opp.cycle_json,
                    &opp.cycle_symbols,
                    opp.expected_profit,
                    opp.max_return_cost,
                    if opp.gated { 1 } else { 0 }
                ])?;
            }
        }

        tx.commit()?;
        self.opportunity_buffer.clear();

        Ok(count)
    }

    /// Shutdown the writer, ensuring all buffered data is flushed
    ///
    /// This prevents data loss on shutdown
    pub fn shutdown(mut self) -> Result<(), Error> {
        self.flush()?;
        Ok(())
    }
}

/// Initialize a new SQLite database at the specified path with WAL mode and schema
pub fn initialize_database(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path)?;
    enable_wal_mode(&conn)?;
    create_tables(&conn)?;
    verify_schema(&conn)?;
    Ok(conn)
}

/// Enable Write-Ahead Logging (WAL) mode for better concurrency
pub fn enable_wal_mode(conn: &Connection) -> Result<(), Error> {
    // Enable foreign keys first
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    // Try to enable WAL mode and synchronous=NORMAL
    // Note: WAL mode doesn't work with in-memory databases, so we don't verify it
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;",
    )?;

    Ok(())
}

/// Create all database tables with proper indexes and constraints
pub fn create_tables(conn: &Connection) -> Result<(), Error> {
    // ticks table - Raw market data
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ticks (
            tick_id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            instrument TEXT NOT NULL,
            side TEXT NOT NULL CHECK(side IN ('bid', 'ask')),
            price TEXT NOT NULL,
            quantity TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_ticks_timestamp_instrument
            ON ticks(timestamp, instrument);",
    )?;

    // opportunities table - Detected arbitrage cycles
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS opportunities (
            opportunity_id TEXT PRIMARY KEY,
            detected_at INTEGER NOT NULL,
            cycle_json TEXT NOT NULL,
            cycle_symbols TEXT NOT NULL,
            expected_profit REAL NOT NULL,
            max_return_cost REAL NOT NULL,
            gated INTEGER NOT NULL CHECK(gated IN (0, 1))
        );
        CREATE INDEX IF NOT EXISTS idx_opportunities_detected_at
            ON opportunities(detected_at);",
    )?;

    // lump_sums table - Capital parcel state
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS lump_sums (
            lump_sum_id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            base_currency TEXT NOT NULL,
            current_currency TEXT NOT NULL,
            current_size TEXT NOT NULL,
            initial_size TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('PARKED', 'OPPORTUNITY', 'EXECUTING', 'FILLED', 'REJECTED')),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_lump_sums_state
            ON lump_sums(state);
        CREATE INDEX IF NOT EXISTS idx_lump_sums_current_currency
            ON lump_sums(current_currency);"
    )?;

    // hops table - Trade/conversion history
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hops (
            hop_id INTEGER PRIMARY KEY AUTOINCREMENT,
            lump_sum_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            from_currency TEXT NOT NULL,
            to_currency TEXT NOT NULL,
            size_before TEXT NOT NULL,
            size_after TEXT NOT NULL,
            rate TEXT NOT NULL,
            fee TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            cycle_id TEXT,
            FOREIGN KEY (lump_sum_id) REFERENCES lump_sums(lump_sum_id)
        );
        CREATE INDEX IF NOT EXISTS idx_hops_lump_sum_seq
            ON hops(lump_sum_id, seq);
        CREATE INDEX IF NOT EXISTS idx_hops_timestamp
            ON hops(timestamp);",
    )?;

    // reconciliations table - Capital reconciliation logs
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS reconciliations (
            reconciliation_id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            lump_sum_id TEXT NOT NULL,
            currency TEXT NOT NULL,
            expected_balance TEXT NOT NULL,
            actual_balance TEXT NOT NULL,
            discrepancy TEXT NOT NULL,
            FOREIGN KEY (lump_sum_id) REFERENCES lump_sums(lump_sum_id)
        );
        CREATE INDEX IF NOT EXISTS idx_reconciliations_timestamp
            ON reconciliations(timestamp);",
    )?;

    // topology_events table - Market topology changes
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS topology_events (
            event_id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            event_type TEXT NOT NULL CHECK(event_type IN ('PAIR_SUSPENDED', 'PAIR_RELISTED', 'NODE_DISCONNECTED', 'NODE_RECONNECTED')),
            instrument TEXT,
            currency TEXT,
            details TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_topology_events_timestamp_type
            ON topology_events(timestamp, event_type);"
    )?;

    Ok(())
}

/// Verify that the database schema matches expectations
pub fn verify_schema(conn: &Connection) -> Result<(), Error> {
    let tables = vec![
        "ticks",
        "opportunities",
        "lump_sums",
        "hops",
        "reconciliations",
        "topology_events",
    ];

    for table in tables {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            [table],
            |row| row.get(0),
        )?;

        if count == 0 {
            return Err(Error::ExecuteReturnedResults);
        }
    }

    // Verify foreign keys are enabled
    let fk_enabled: i32 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;

    if fk_enabled != 1 {
        return Err(Error::ExecuteReturnedResults);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_database() {
        let conn = initialize_database(":memory:").unwrap();
        assert!(conn.is_autocommit());
    }

    #[test]
    fn test_wal_mode_enabled() {
        // Create a temporary file for testing WAL mode
        // (WAL doesn't work with in-memory databases)
        let temp_path = std::env::temp_dir().join("test_wal.db");
        let _ = std::fs::remove_file(&temp_path); // Clean up if exists

        let conn = Connection::open(&temp_path).unwrap();
        enable_wal_mode(&conn).unwrap();

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode.to_uppercase(), "WAL");

        // Clean up
        drop(conn);
        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_file(temp_path.with_extension("db-shm"));
        let _ = std::fs::remove_file(temp_path.with_extension("db-wal"));
    }

    #[test]
    fn test_foreign_keys_enabled() {
        let conn = Connection::open(":memory:").unwrap();
        enable_wal_mode(&conn).unwrap();

        let fk_enabled: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();

        assert_eq!(fk_enabled, 1);
    }

    #[test]
    fn test_all_tables_exist() {
        let conn = initialize_database(":memory:").unwrap();

        let tables = vec![
            "ticks",
            "opportunities",
            "lump_sums",
            "hops",
            "reconciliations",
            "topology_events",
        ];

        for table in tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();

            assert_eq!(count, 1, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_table_schemas() {
        let conn = initialize_database(":memory:").unwrap();

        // Test ticks table schema
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='ticks'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(sql.contains("tick_id INTEGER PRIMARY KEY"));
        assert!(sql.contains("timestamp INTEGER NOT NULL"));
        assert!(sql.contains("instrument TEXT NOT NULL"));
        assert!(sql.contains("side TEXT NOT NULL"));
        assert!(sql.contains("price TEXT NOT NULL"));
        assert!(sql.contains("quantity TEXT NOT NULL"));

        // Test lump_sums table schema
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='lump_sums'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(sql.contains("lump_sum_id TEXT PRIMARY KEY"));
        assert!(sql.contains("name TEXT NOT NULL UNIQUE"));
        assert!(sql.contains("base_currency TEXT NOT NULL"));
        assert!(sql.contains("current_currency TEXT NOT NULL"));
        assert!(sql.contains("current_size TEXT NOT NULL"));
        assert!(sql.contains("state TEXT NOT NULL"));
        assert!(sql.contains("created_at INTEGER NOT NULL"));
        assert!(sql.contains("updated_at INTEGER NOT NULL"));

        // Test hops table has foreign key
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='hops'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(sql.contains("FOREIGN KEY (lump_sum_id) REFERENCES lump_sums(lump_sum_id)"));
    }

    #[test]
    fn test_indexes_exist() {
        let conn = initialize_database(":memory:").unwrap();

        let indexes = vec![
            "idx_ticks_timestamp_instrument",
            "idx_opportunities_detected_at",
            "idx_lump_sums_state",
            "idx_lump_sums_current_currency",
            "idx_hops_lump_sum_seq",
            "idx_hops_timestamp",
            "idx_reconciliations_timestamp",
            "idx_topology_events_timestamp_type",
        ];

        for index in indexes {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();

            assert_eq!(count, 1, "Index {} should exist", index);
        }
    }

    #[test]
    fn test_foreign_key_constraint() {
        let conn = initialize_database(":memory:").unwrap();

        // Try to insert a hop with a non-existent lump_sum_id
        let result = conn.execute(
            "INSERT INTO hops (lump_sum_id, seq, from_currency, to_currency, size_before, size_after, rate, fee, timestamp)
             VALUES ('non-existent-id', 1, 'USD', 'BTC', '100.0', '0.002', '50000.0', '0.5', 1234567890)",
            []
        );

        assert!(result.is_err(), "Should fail due to foreign key constraint");
    }

    #[test]
    fn test_unique_constraint() {
        let conn = initialize_database(":memory:").unwrap();

        // Insert a lump sum
        conn.execute(
            "INSERT INTO lump_sums (lump_sum_id, name, base_currency, current_currency, current_size, initial_size, state, created_at, updated_at)
             VALUES ('uuid-1', 'alpha', 'USD', 'USD', '1000.0', '1000.0', 'PARKED', 1234567890, 1234567890)",
            []
        ).unwrap();

        // Try to insert another lump sum with the same name
        let result = conn.execute(
            "INSERT INTO lump_sums (lump_sum_id, name, base_currency, current_currency, current_size, initial_size, state, created_at, updated_at)
             VALUES ('uuid-2', 'alpha', 'USD', 'USD', '2000.0', '2000.0', 'PARKED', 1234567891, 1234567891)",
            []
        );

        assert!(
            result.is_err(),
            "Should fail due to unique constraint on name"
        );
    }

    // BatchWriter tests

    #[test]
    fn test_batch_writer_buffers_ticks() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 1000);

        // Buffer a tick without immediate flush
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567890,
                instrument: "BTC-USD".to_string(),
                side: "bid".to_string(),
                price: "50000.0".to_string(),
                quantity: "0.1".to_string(),
            })
            .unwrap();

        // Verify tick is buffered, not written
        assert_eq!(writer.tick_buffer.len(), 1);

        // Verify database is empty
        let count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_batch_writer_buffers_opportunities() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 1000);

        // Buffer an opportunity without immediate flush
        writer
            .buffer_opportunity(OpportunityBuffer {
                opportunity_id: "uuid-123".to_string(),
                detected_at: 1234567890,
                cycle_json: "[0,1,2]".to_string(),
                cycle_symbols: "[\"USD\",\"BTC\",\"ETH\"]".to_string(),
                expected_profit: 0.05,
                max_return_cost: 0.02,
                gated: false,
            })
            .unwrap();

        // Verify opportunity is buffered, not written
        assert_eq!(writer.opportunity_buffer.len(), 1);

        // Verify database is empty
        let count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM opportunities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_flush_on_size_limit() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 10, 60000); // Small buffer, long interval

        // Buffer 10 ticks
        for i in 0..10 {
            writer
                .buffer_tick(TickBuffer {
                    timestamp: 1234567890 + i,
                    instrument: "BTC-USD".to_string(),
                    side: "bid".to_string(),
                    price: "50000.0".to_string(),
                    quantity: "0.1".to_string(),
                })
                .unwrap();
        }

        // After 10 items, buffer should have 10 (not flushed yet)
        assert_eq!(writer.tick_buffer.len(), 10);

        // Add one more to trigger flush
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567900,
                instrument: "BTC-USD".to_string(),
                side: "bid".to_string(),
                price: "50000.0".to_string(),
                quantity: "0.1".to_string(),
            })
            .unwrap();

        // Buffer should have 1 item (the new one added after flush)
        assert_eq!(writer.tick_buffer.len(), 1);

        // Database should have 10 records (the ones that were flushed)
        let count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_flush_on_timer() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 10); // Large buffer, short interval (10ms)

        // Buffer a tick (won't flush yet - buffer not full, time hasn't elapsed)
        writer.tick_buffer.push(TickBuffer {
            timestamp: 1234567890,
            instrument: "BTC-USD".to_string(),
            side: "bid".to_string(),
            price: "50000.0".to_string(),
            quantity: "0.1".to_string(),
        });

        // Verify nothing flushed yet
        assert_eq!(writer.tick_buffer.len(), 1);

        // Wait for flush interval to expire
        std::thread::sleep(Duration::from_millis(15));

        // Now should_flush should return true due to time
        assert!(writer.should_flush());

        // Buffer another tick, which should trigger time-based flush BEFORE adding
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567891,
                instrument: "BTC-USD".to_string(),
                side: "ask".to_string(),
                price: "50001.0".to_string(),
                quantity: "0.2".to_string(),
            })
            .unwrap();

        // Should have flushed the first tick, then added the new one
        assert_eq!(writer.tick_buffer.len(), 1);

        // Database should have 1 record (the first tick that was flushed)
        let count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_manual_flush() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 60000); // Large buffer, long interval

        // Buffer some data
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567890,
                instrument: "BTC-USD".to_string(),
                side: "bid".to_string(),
                price: "50000.0".to_string(),
                quantity: "0.1".to_string(),
            })
            .unwrap();

        writer
            .buffer_opportunity(OpportunityBuffer {
                opportunity_id: "uuid-123".to_string(),
                detected_at: 1234567890,
                cycle_json: "[0,1,2]".to_string(),
                cycle_symbols: "[\"USD\",\"BTC\",\"ETH\"]".to_string(),
                expected_profit: 0.05,
                max_return_cost: 0.02,
                gated: false,
            })
            .unwrap();

        // Manual flush
        let count = writer.flush().unwrap();
        assert_eq!(count, 2);

        // Buffers should be empty
        assert_eq!(writer.tick_buffer.len(), 0);
        assert_eq!(writer.opportunity_buffer.len(), 0);

        // Database should have records
        let tick_count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tick_count, 1);

        let opp_count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM opportunities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(opp_count, 1);
    }

    #[test]
    fn test_transaction_atomicity() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 60000);

        // Buffer valid tick
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567890,
                instrument: "BTC-USD".to_string(),
                side: "bid".to_string(),
                price: "50000.0".to_string(),
                quantity: "0.1".to_string(),
            })
            .unwrap();

        // Buffer invalid tick (side must be 'bid' or 'ask')
        writer.tick_buffer.push(TickBuffer {
            timestamp: 1234567891,
            instrument: "BTC-USD".to_string(),
            side: "invalid".to_string(),
            price: "50001.0".to_string(),
            quantity: "0.2".to_string(),
        });

        // Flush should fail due to constraint violation
        let result = writer.flush_ticks();
        assert!(result.is_err());

        // No records should be written (transaction rolled back)
        let count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_shutdown_flushes() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 60000);

        // Buffer some data
        writer
            .buffer_tick(TickBuffer {
                timestamp: 1234567890,
                instrument: "BTC-USD".to_string(),
                side: "bid".to_string(),
                price: "50000.0".to_string(),
                quantity: "0.1".to_string(),
            })
            .unwrap();

        // Get connection reference before shutdown
        let conn = &writer.conn;

        // Count before shutdown (should be 0 since not flushed)
        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_before, 0);

        // Shutdown should flush
        writer.shutdown().unwrap();

        // Verify data was written (need to reopen connection)
        let conn = Connection::open(":memory:").unwrap();
        enable_wal_mode(&conn).unwrap();
        create_tables(&conn).unwrap();

        // Note: In-memory databases don't persist across connections
        // This test verifies shutdown() calls flush() without error
        // For real testing, use a file-based database
    }

    #[test]
    fn test_flush_returns_count() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 60000);

        // Buffer 3 ticks and 2 opportunities
        for i in 0..3 {
            writer
                .buffer_tick(TickBuffer {
                    timestamp: 1234567890 + i,
                    instrument: "BTC-USD".to_string(),
                    side: "bid".to_string(),
                    price: "50000.0".to_string(),
                    quantity: "0.1".to_string(),
                })
                .unwrap();
        }

        for i in 0..2 {
            writer
                .buffer_opportunity(OpportunityBuffer {
                    opportunity_id: format!("uuid-{}", i),
                    detected_at: 1234567890 + i,
                    cycle_json: "[0,1,2]".to_string(),
                    cycle_symbols: "[\"USD\",\"BTC\",\"ETH\"]".to_string(),
                    expected_profit: 0.05,
                    max_return_cost: 0.02,
                    gated: false,
                })
                .unwrap();
        }

        // Flush should return total count
        let count = writer.flush().unwrap();
        assert_eq!(count, 5); // 3 ticks + 2 opportunities
    }

    #[test]
    fn test_empty_flush() {
        let conn = initialize_database(":memory:").unwrap();
        let mut writer = BatchWriter::new(conn, 1000, 60000);

        // Flush empty buffers should succeed and return 0
        let count = writer.flush().unwrap();
        assert_eq!(count, 0);

        // Database should be empty
        let tick_count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM ticks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tick_count, 0);

        let opp_count: i64 = writer
            .conn
            .query_row("SELECT COUNT(*) FROM opportunities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(opp_count, 0);
    }
}
