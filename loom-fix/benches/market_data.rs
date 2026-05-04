// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use loom_fix::market_data::{MDEntryType, MDUpdateAction, MarketDataIncrementalRefresh};
use loom_fix::message::{FixMessage, MsgType, Tag};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Create a sample MarketDataIncrementalRefresh message with N entries
fn create_sample_incremental_refresh(num_entries: usize) -> FixMessage {
    let mut msg = FixMessage::new(
        MsgType::MarketDataIncrementalRefresh,
        1,
        "COINBASE".to_string(),
        "CLIENT".to_string(),
        "20260212-12:00:00.000".to_string(),
    );

    msg.add_field(Tag::MDReqID, "REQ123".to_string());
    msg.add_field(Tag::NoMDEntries, num_entries.to_string());

    for i in 0..num_entries {
        let action = match i % 3 {
            0 => "0", // New
            1 => "1", // Change
            _ => "2", // Delete
        };
        let entry_type = if i % 2 == 0 { "0" } else { "1" }; // Bid or Offer
        let price = format!("{}.{:02}", 50000 + i, i % 100);
        let size = format!("{}.{}", 1 + (i % 10), i % 10);
        let entry_id = format!("ORDER{}", i);

        msg.add_field(Tag::MDUpdateAction, action.to_string());
        msg.add_field(Tag::MDEntryType, entry_type.to_string());
        msg.add_field(Tag::MDEntryPx, price);
        msg.add_field(Tag::MDEntrySize, size);
        msg.add_field(Tag::MDEntryID, entry_id);
    }

    msg
}

fn bench_parse_incremental_refresh_1_entry(c: &mut Criterion) {
    let msg = create_sample_incremental_refresh(1);

    c.bench_function("parse_incremental_refresh_1_entry", |b| {
        b.iter(|| MarketDataIncrementalRefresh::from_fix_message(black_box(&msg)));
    });
}

fn bench_parse_incremental_refresh_10_entries(c: &mut Criterion) {
    let msg = create_sample_incremental_refresh(10);

    c.bench_function("parse_incremental_refresh_10_entries", |b| {
        b.iter(|| MarketDataIncrementalRefresh::from_fix_message(black_box(&msg)));
    });
}

fn bench_parse_incremental_refresh_100_entries(c: &mut Criterion) {
    let msg = create_sample_incremental_refresh(100);

    c.bench_function("parse_incremental_refresh_100_entries", |b| {
        b.iter(|| MarketDataIncrementalRefresh::from_fix_message(black_box(&msg)));
    });
}

fn bench_parse_incremental_refresh_1000_entries(c: &mut Criterion) {
    let msg = create_sample_incremental_refresh(1000);

    c.bench_function("parse_incremental_refresh_1000_entries", |b| {
        b.iter(|| MarketDataIncrementalRefresh::from_fix_message(black_box(&msg)));
    });
}

criterion_group!(
    benches,
    bench_parse_incremental_refresh_1_entry,
    bench_parse_incremental_refresh_10_entries,
    bench_parse_incremental_refresh_100_entries,
    bench_parse_incremental_refresh_1000_entries,
);
criterion_main!(benches);
