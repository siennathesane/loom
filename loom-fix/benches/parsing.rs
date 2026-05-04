// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

// Benchmark for FIX message parsing
// TODO: Implement proper benchmarks in Task 1.1

use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_placeholder(c: &mut Criterion) {
    c.bench_function("placeholder", |b| b.iter(|| 1 + 1));
}

criterion_group!(benches, benchmark_placeholder);
criterion_main!(benches);
