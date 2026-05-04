// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Comprehensive performance benchmarks for Phase 1 exit criteria validation
//!
//! This benchmark suite validates all critical performance requirements:
//! - FIX serialization latency: p99 < 100μs
//! - Market data parsing: > 10K msg/sec
//! - Session layer overhead
//! - Transport layer performance

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use loom_fix::{
    market_data::{
        MarketDataIncrementalRefresh, MarketDataRequestBuilder,
        SubscriptionRequestType as SubReqType,
    },
    order_entry::{ExecutionReport, NewOrderSingleBuilder, OrderSide, OrderType, TimeInForce},
    serialization::{build_fix_message, calculate_checksum},
    session::auth::compute_logon_hmac,
    FixMessage, MsgType, Tag,
};
use rust_decimal::Decimal;
use std::time::{SystemTime, UNIX_EPOCH};

/// Benchmark 1: FIX Message Serialization Latency
/// Target: p99 < 100μs
fn bench_serialization_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization_latency");

    // NewOrderSingle serialization
    group.bench_function("new_order_single", |b| {
        b.iter(|| {
            let msg = NewOrderSingleBuilder::new(
                1,
                "SENDER".to_string(),
                "TARGET".to_string(),
                "20240101-12:00:00".to_string(),
            )
            .cl_ord_id("ORDER123456".to_string())
            .symbol("BTC-USD".to_string())
            .side(OrderSide::Buy)
            .order_qty(Decimal::new(15000, 4)) // 1.5000 BTC
            .ord_type(OrderType::Limit)
            .price(Decimal::new(5000000, 2)) // 50000.00 USD
            .time_in_force(TimeInForce::ImmediateOrCancel)
            .build();
            black_box(msg)
        });
    });

    // MarketDataRequest serialization
    group.bench_function("market_data_request", |b| {
        b.iter(|| {
            let msg = MarketDataRequestBuilder::new(
                1,
                "SENDER".to_string(),
                "TARGET".to_string(),
                "20240101-12:00:00".to_string(),
                "MDR123456".to_string(),
            )
            .subscription_request_type(SubReqType::Subscribe)
            .market_depth(0)
            .add_symbol("BTC-USD".to_string())
            .add_symbol("ETH-USD".to_string())
            .build();
            black_box(msg)
        });
    });

    // Raw message building (lowest level)
    group.bench_function("raw_message_building", |b| {
        b.iter(|| {
            let mut msg = FixMessage::new(
                MsgType::NewOrderSingle,
                black_box(1),
                black_box("SENDER").to_string(),
                black_box("TARGET").to_string(),
                black_box("20240101-12:00:00").to_string(),
            );
            msg.add_field(Tag::ClOrdID, "ORDER123".to_string());
            msg.add_field(Tag::Symbol, "BTC-USD".to_string());
            msg.add_field(Tag::Side, "1".to_string());
            msg.add_field(Tag::OrderQty, "1.5000".to_string());
            msg.add_field(Tag::OrdType, "2".to_string());
            msg.add_field(Tag::Price, "50000.00".to_string());
            msg.add_field(Tag::TimeInForce, "3".to_string());
            let bytes = build_fix_message(&msg);
            black_box(bytes)
        });
    });

    // Checksum calculation overhead
    group.bench_function("checksum_calculation", |b| {
        let msg = b"8=FIX.5.0\x019=100\x0135=D\x0149=SENDER\x0156=TARGET\x0134=1\x0152=20240101-12:00:00\x01";
        b.iter(|| {
            let checksum = calculate_checksum(black_box(msg));
            black_box(checksum)
        });
    });

    group.finish();
}

/// Benchmark 2: Market Data Parsing Throughput
/// Target: > 10K msg/sec
fn bench_market_data_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("market_data_throughput");

    // Single entry parsing
    let single_entry_msg = create_market_data_message(1);
    group.throughput(Throughput::Elements(1));
    group.bench_function("parse_1_entry", |b| {
        b.iter(|| {
            let result =
                MarketDataIncrementalRefresh::from_fix_message(black_box(&single_entry_msg));
            black_box(result)
        });
    });

    // 10 entries parsing
    let ten_entries_msg = create_market_data_message(10);
    group.throughput(Throughput::Elements(10));
    group.bench_function("parse_10_entries", |b| {
        b.iter(|| {
            let result =
                MarketDataIncrementalRefresh::from_fix_message(black_box(&ten_entries_msg));
            black_box(result)
        });
    });

    // 100 entries parsing
    let hundred_entries_msg = create_market_data_message(100);
    group.throughput(Throughput::Elements(100));
    group.bench_function("parse_100_entries", |b| {
        b.iter(|| {
            let result =
                MarketDataIncrementalRefresh::from_fix_message(black_box(&hundred_entries_msg));
            black_box(result)
        });
    });

    // 1000 entries parsing (high-frequency stress test)
    let thousand_entries_msg = create_market_data_message(1000);
    group.throughput(Throughput::Elements(1000));
    group.bench_function("parse_1000_entries", |b| {
        b.iter(|| {
            let result =
                MarketDataIncrementalRefresh::from_fix_message(black_box(&thousand_entries_msg));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark 3: Session Layer Overhead
fn bench_session_layer(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_layer");

    // HMAC signature generation (Logon)
    group.bench_function("hmac_signature", |b| {
        let sending_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let api_secret = "dGVzdF9zZWNyZXRfa2V5X2Zvcl9obWFjX3Rlc3Rpbmc="; // base64

        b.iter(|| {
            let result = compute_logon_hmac(
                black_box(api_secret),
                black_box("SENDER"),
                black_box("Coinbase"),
                black_box("A"),
                black_box(1u64),
                black_box(&sending_time),
                black_box("test_passphrase"),
            );
            black_box(result)
        });
    });

    // Sequence number tracking (atomic increment)
    group.bench_function("sequence_increment", |b| {
        use std::sync::atomic::{AtomicU64, Ordering};
        let seq = AtomicU64::new(1);

        b.iter(|| {
            let current = seq.fetch_add(1, Ordering::SeqCst);
            black_box(current)
        });
    });

    // Heartbeat message construction
    group.bench_function("heartbeat_message", |b| {
        b.iter(|| {
            let msg = FixMessage::new(
                MsgType::Heartbeat,
                black_box(42),
                black_box("SENDER").to_string(),
                black_box("TARGET").to_string(),
                black_box("20240101-12:00:00").to_string(),
            );
            let bytes = build_fix_message(&msg);
            black_box(bytes)
        });
    });

    // TestRequest message construction
    group.bench_function("test_request_message", |b| {
        b.iter(|| {
            let mut msg = FixMessage::new(
                MsgType::TestRequest,
                black_box(42),
                black_box("SENDER").to_string(),
                black_box("TARGET").to_string(),
                black_box("20240101-12:00:00").to_string(),
            );
            msg.add_field(Tag::TestReqID, "TESTREQ123".to_string());
            let bytes = build_fix_message(&msg);
            black_box(bytes)
        });
    });

    group.finish();
}

/// Benchmark 4: Transport Layer Performance
fn bench_transport_layer(c: &mut Criterion) {
    let mut group = c.benchmark_group("transport_layer");

    // Exponential backoff calculation
    group.bench_function("exponential_backoff", |b| {
        use loom_fix::transport::ExponentialBackoff;

        b.iter(|| {
            let mut backoff = ExponentialBackoff::new(1000, 60000);
            for _ in 0..7 {
                black_box(backoff.next());
            }
        });
    });

    // Connection health check (should be O(1) instant)
    group.bench_function("health_check", |b| {
        use loom_fix::transport::{ConnectionConfig, FixConnection};

        let config = ConnectionConfig::default();
        let conn = FixConnection::new(config).unwrap();

        b.iter(|| {
            let healthy = conn.is_healthy();
            let degraded = conn.is_degraded();
            black_box((healthy, degraded))
        });
    });

    // Connection state tracking
    group.bench_function("state_tracking", |b| {
        use loom_fix::transport::{ConnectionConfig, FixConnection};

        let config = ConnectionConfig::default();
        let mut conn = FixConnection::new(config).unwrap();

        b.iter(|| {
            let state = conn.state();
            conn.update_health_state();
            black_box(state)
        });
    });

    group.finish();
}

/// Benchmark 5: End-to-End Round-Trip
fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    // Full serialize → parse round-trip for market data
    group.bench_function("market_data_roundtrip", |b| {
        b.iter(|| {
            let msg = create_market_data_message(10);
            let parsed = MarketDataIncrementalRefresh::from_fix_message(&msg);
            black_box(parsed)
        });
    });

    // ExecutionReport parsing
    group.bench_function("execution_report_parse", |b| {
        let msg = create_execution_report();

        b.iter(|| {
            let parsed = ExecutionReport::from_fix_message(black_box(&msg));
            black_box(parsed)
        });
    });

    group.finish();
}

// Helper: create a MarketDataIncrementalRefresh FixMessage with N entries
fn create_market_data_message(num_entries: usize) -> FixMessage {
    let mut msg = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        1,
        "SENDER".to_string(),
        "TARGET".to_string(),
        "20240101-12:00:00".to_string(),
    );
    msg.add_field(Tag::MDReqID, "MDR123".to_string());
    msg.add_field(Tag::NoMDEntries, num_entries.to_string());

    for i in 0..num_entries {
        msg.add_field(Tag::MDUpdateAction, "0".to_string()); // New
        msg.add_field(Tag::MDEntryType, "0".to_string()); // Bid
        msg.add_field(Tag::MDEntryPx, format!("{}.00", 50000 + i));
        msg.add_field(Tag::MDEntrySize, format!("{}.5000", i + 1));
        msg.add_field(Tag::MDEntryID, format!("ENTRY{}", i));
    }

    msg
}

// Helper: create an ExecutionReport FixMessage
fn create_execution_report() -> FixMessage {
    let mut msg = FixMessage::new(
        MsgType::ExecutionReport,
        1,
        "TARGET".to_string(),
        "SENDER".to_string(),
        "20240101-12:00:00".to_string(),
    );
    msg.add_field(Tag::OrderID, "ORD123".to_string());
    msg.add_field(Tag::ClOrdID, "CLORD123".to_string());
    msg.add_field(Tag::ExecID, "EXEC123".to_string());
    msg.add_field(Tag::ExecType, "F".to_string());
    msg.add_field(Tag::OrdStatus, "2".to_string());
    msg.add_field(Tag::Symbol, "BTC-USD".to_string());
    msg.add_field(Tag::Side, "1".to_string());
    msg.add_field(Tag::OrderQty, "1.0".to_string());
    msg.add_field(Tag::CumQty, "1.0".to_string());
    msg.add_field(Tag::LeavesQty, "0".to_string());
    msg
}

criterion_group!(
    benches,
    bench_serialization_latency,
    bench_market_data_throughput,
    bench_session_layer,
    bench_transport_layer,
    bench_end_to_end,
);
criterion_main!(benches);
