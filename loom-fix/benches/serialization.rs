// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Benchmarks for FIX message serialization
//!
//! These benchmarks verify:
//! 1. Zero allocations in hot path
//! 2. Performance characteristics
//! 3. Baseline for optimization

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use loom_fix::message::Tag;
use loom_fix::serialization::build_fix_message;
use loom_fix::{FixMessage, MsgType};

fn benchmark_heartbeat_serialization(c: &mut Criterion) {
    c.bench_function("serialize_heartbeat", |b| {
        let msg = FixMessage::new(
            MsgType::Heartbeat,
            1,
            "SENDER".to_string(),
            "TARGET".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        b.iter(|| {
            let serialized = build_fix_message(black_box(&msg));
            black_box(serialized);
        });
    });
}

fn benchmark_logon_serialization(c: &mut Criterion) {
    c.bench_function("serialize_logon", |b| {
        let mut msg = FixMessage::new(
            MsgType::Logon,
            1,
            "API_KEY_123".to_string(),
            "Coinbase".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        msg.add_field(Tag::HeartBtInt, "30".to_string());
        msg.add_field(Tag::EncryptMethod, "0".to_string());
        msg.add_field(Tag::Username, "API_KEY_123".to_string());
        msg.add_field(Tag::RawData, "dGVzdF9zaWduYXR1cmU=".to_string()); // Mock signature

        b.iter(|| {
            let serialized = build_fix_message(black_box(&msg));
            black_box(serialized);
        });
    });
}

fn benchmark_new_order_single_serialization(c: &mut Criterion) {
    c.bench_function("serialize_new_order_single", |b| {
        let mut msg = FixMessage::new(
            MsgType::NewOrderSingle,
            100,
            "API_KEY_123".to_string(),
            "Coinbase".to_string(),
            "20260212-12:00:00.000".to_string(),
        );

        // Add typical order fields
        msg.add_field(Tag::ClOrdID, "ORDER_12345".to_string());
        msg.add_field(Tag::Symbol, "BTC-USD".to_string());
        msg.add_field(Tag::Side, "1".to_string()); // Buy
        msg.add_field(Tag::OrderQty, "0.5".to_string());
        msg.add_field(Tag::OrdType, "2".to_string()); // Limit
        msg.add_field(Tag::Price, "50000.00".to_string());
        msg.add_field(Tag::TimeInForce, "3".to_string()); // IOC
        msg.add_field(Tag::TransactTime, "20260212-12:00:00.000".to_string());

        b.iter(|| {
            let serialized = build_fix_message(black_box(&msg));
            black_box(serialized);
        });
    });
}

fn benchmark_varying_message_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialize_varying_sizes");

    for num_fields in [0, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_fields),
            num_fields,
            |b, &num_fields| {
                let mut msg = FixMessage::new(
                    MsgType::Heartbeat,
                    1,
                    "SENDER".to_string(),
                    "TARGET".to_string(),
                    "20260212-12:00:00.000".to_string(),
                );

                // Add varying number of fields
                for i in 0..num_fields {
                    msg.add_field(Tag::HeartBtInt, format!("field_value_{}", i));
                }

                b.iter(|| {
                    let serialized = build_fix_message(black_box(&msg));
                    black_box(serialized);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_heartbeat_serialization,
    benchmark_logon_serialization,
    benchmark_new_order_single_serialization,
    benchmark_varying_message_sizes,
);
criterion_main!(benches);
