// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use loom_graph::precision::*;
use loom_graph::{DecimalApproach, F64Approach, HybridApproach, PrecisionApproach};

/// Benchmark single edge weight calculation
/// This is the hot path operation - happens on every L3 update that changes top-of-book
fn bench_edge_weight_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("edge_weight_calculation");

    // Realistic test data
    let rate = test_data::BTC_USD_RATE;
    let fee_bps = test_data::TAKER_FEE_BPS;

    group.bench_function("f64_everywhere", |b| {
        let approach = F64Approach::default();
        b.iter(|| black_box(approach.transform_rate(black_box(rate), black_box(fee_bps))));
    });

    group.bench_function("hybrid_f64_fixed", |b| {
        let approach = HybridApproach::default();
        b.iter(|| black_box(approach.transform_rate(black_box(rate), black_box(fee_bps))));
    });

    group.bench_function("rust_decimal", |b| {
        let approach = DecimalApproach::default();
        b.iter(|| black_box(approach.transform_rate(black_box(rate), black_box(fee_bps))));
    });

    group.finish();
}

/// Benchmark Bellman-Ford negative cycle detection
/// This runs on every edge weight change in the graph
fn bench_bellman_ford_cycle_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("bellman_ford_cycle");

    // Simulate a graph with 200 nodes and 500 edges
    // This is representative of Coinbase's trading pair graph
    const NUM_NODES: usize = 200;
    const NUM_EDGES: usize = 500;

    // Generate realistic edge weights
    let weights = generate_graph_weights(NUM_EDGES);

    group.bench_function("f64_everywhere", |b| {
        let approach = F64Approach::default();
        b.iter(|| {
            // Simulate Bellman-Ford iterations
            let result = simulate_bellman_ford(&approach, black_box(&weights), NUM_NODES);
            black_box(result)
        });
    });

    group.bench_function("hybrid_f64_fixed", |b| {
        let approach = HybridApproach::default();
        b.iter(|| {
            let result = simulate_bellman_ford(&approach, black_box(&weights), NUM_NODES);
            black_box(result)
        });
    });

    group.bench_function("rust_decimal", |b| {
        let approach = DecimalApproach::default();
        b.iter(|| {
            let result = simulate_bellman_ford(&approach, black_box(&weights), NUM_NODES);
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark graph update after L3 change
/// This is the full pipeline: parse update -> recalculate edge -> maybe run Bellman-Ford
fn bench_graph_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_update");

    // Simulate 10 edges that need recalculation
    let edges_to_update = 10;
    let rates: Vec<f64> = (0..edges_to_update)
        .map(|i| test_data::BTC_USD_RATE * (1.0 + i as f64 * 0.001))
        .collect();
    let fee_bps = test_data::TAKER_FEE_BPS;

    group.bench_function("f64_everywhere", |b| {
        let approach = F64Approach::default();
        b.iter(|| {
            let mut weights = Vec::new();
            for &rate in &rates {
                weights.push(approach.transform_rate(black_box(rate), black_box(fee_bps)));
            }
            black_box(weights)
        });
    });

    group.bench_function("hybrid_f64_fixed", |b| {
        let approach = HybridApproach::default();
        b.iter(|| {
            let mut weights = Vec::new();
            for &rate in &rates {
                weights.push(approach.transform_rate(black_box(rate), black_box(fee_bps)));
            }
            black_box(weights)
        });
    });

    group.bench_function("rust_decimal", |b| {
        let approach = DecimalApproach::default();
        b.iter(|| {
            let mut weights = Vec::new();
            for &rate in &rates {
                weights.push(approach.transform_rate(black_box(rate), black_box(fee_bps)));
            }
            black_box(weights)
        });
    });

    group.finish();
}

/// Benchmark cumulative weight calculation over a cycle
/// Tests how rounding errors accumulate
fn bench_cycle_weight_accumulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cycle_accumulation");

    // Test with different cycle lengths
    for cycle_length in [3, 5, 10, 20].iter() {
        let weights = generate_cycle_weights(*cycle_length);

        group.bench_with_input(
            BenchmarkId::new("f64_everywhere", cycle_length),
            cycle_length,
            |b, _| {
                let approach = F64Approach::default();
                b.iter(|| black_box(approach.cumulative_weight(black_box(&weights))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("hybrid_f64_fixed", cycle_length),
            cycle_length,
            |b, _| {
                let approach = HybridApproach::default();
                b.iter(|| black_box(approach.cumulative_weight(black_box(&weights))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("rust_decimal", cycle_length),
            cycle_length,
            |b, _| {
                let approach = DecimalApproach::default();
                b.iter(|| black_box(approach.cumulative_weight(black_box(&weights))));
            },
        );
    }

    group.finish();
}

/// Benchmark profitability check
/// This is called after cycle detection to validate opportunity
fn bench_profitability_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("profitability_check");

    // Test with various cumulative weights (profitable and not)
    let test_weights = vec![
        -0.001,  // Clearly profitable
        -0.0001, // Marginally profitable
        0.0001,  // Marginally unprofitable
        0.001,   // Clearly unprofitable
    ];

    for &weight in &test_weights {
        group.bench_with_input(
            BenchmarkId::new("f64_everywhere", format!("{:.5}", weight)),
            &weight,
            |b, &w| {
                let approach = F64Approach::default();
                b.iter(|| black_box(approach.is_profitable(black_box(&w), black_box(0.0))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("hybrid_f64_fixed", format!("{:.5}", weight)),
            &weight,
            |b, &w| {
                let approach = HybridApproach::default();
                b.iter(|| black_box(approach.is_profitable(black_box(&w), black_box(0.0))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("rust_decimal", format!("{:.5}", weight)),
            &weight,
            |b, &w| {
                let approach = DecimalApproach::default();
                b.iter(|| black_box(approach.is_profitable(black_box(&w), black_box(0.0))));
            },
        );
    }

    group.finish();
}

/// Benchmark end-to-end cycle execution simulation
/// This is the full pipeline from detection to profit calculation
fn bench_full_cycle_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_cycle_execution");

    // Realistic 3-hop cycle: USD -> BTC -> ETH -> USD
    let rates = vec![
        1.0 / test_data::BTC_USD_RATE,
        test_data::ETH_BTC_RATE.recip(),
        test_data::ETH_USD_RATE,
    ];
    let fees = vec![
        test_data::TAKER_FEE_BPS,
        test_data::TAKER_FEE_BPS,
        test_data::TAKER_FEE_BPS,
    ];

    group.bench_function("f64_everywhere", |b| {
        let approach = F64Approach::default();
        b.iter(|| {
            // Transform rates to weights
            let weights: Vec<f64> = rates
                .iter()
                .zip(fees.iter())
                .map(|(&r, &f)| approach.transform_rate(r, f))
                .collect();

            // Calculate cumulative weight
            let cumulative = approach.cumulative_weight(&weights);

            // Check profitability
            let profitable = approach.is_profitable(&cumulative, 0.0);

            black_box(profitable)
        });
    });

    group.bench_function("hybrid_f64_fixed", |b| {
        let approach = HybridApproach::default();
        b.iter(|| {
            let weights: Vec<f64> = rates
                .iter()
                .zip(fees.iter())
                .map(|(&r, &f)| approach.transform_rate(r, f))
                .collect();

            let cumulative = approach.cumulative_weight(&weights);
            let profitable = approach.is_profitable(&cumulative, 0.0);

            black_box(profitable)
        });
    });

    group.bench_function("rust_decimal", |b| {
        let approach = DecimalApproach::default();
        b.iter(|| {
            let weights: Vec<f64> = rates
                .iter()
                .zip(fees.iter())
                .map(|(&r, &f)| approach.transform_rate(r, f))
                .collect();

            let cumulative = approach.cumulative_weight(&weights);
            let profitable = approach.is_profitable(&cumulative, 0.0);

            black_box(profitable)
        });
    });

    group.finish();
}

// Helper functions

fn generate_graph_weights(num_edges: usize) -> Vec<f64> {
    (0..num_edges)
        .map(|i| {
            // Generate realistic edge weights in -ln space
            // Most weights should be small (close to 1.0 in rate space)
            -((1.0 + (i as f64) * 0.0001).ln())
        })
        .collect()
}

fn generate_cycle_weights(cycle_length: usize) -> Vec<f64> {
    (0..cycle_length)
        .map(|i| {
            // Generate weights that form a marginally profitable cycle
            -((1.0 + (i as f64) * 0.0001).ln())
        })
        .collect()
}

fn simulate_bellman_ford<A: PrecisionApproach>(
    _approach: &A,
    weights: &[f64],
    num_nodes: usize,
) -> f64 {
    // Simplified Bellman-Ford simulation
    // In reality, this would do V-1 relaxation iterations
    // For benchmarking, we just compute cumulative weights

    let mut distances = vec![f64::INFINITY; num_nodes];
    distances[0] = 0.0;

    // Simulate one iteration of edge relaxations
    for (i, &weight) in weights.iter().enumerate() {
        let src = i % num_nodes;
        let dst = (i + 1) % num_nodes;

        if distances[src] != f64::INFINITY {
            let new_dist = distances[src] + weight;
            if new_dist < distances[dst] {
                distances[dst] = new_dist;
            }
        }
    }

    // Return a distance (benchmarking doesn't care about the actual value)
    distances[0]
}

criterion_group!(
    benches,
    bench_edge_weight_calculation,
    bench_bellman_ford_cycle_detection,
    bench_graph_update,
    bench_cycle_weight_accumulation,
    bench_profitability_check,
    bench_full_cycle_execution,
);

criterion_main!(benches);
