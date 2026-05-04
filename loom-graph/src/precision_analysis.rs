// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Precision analysis module for evaluating cumulative rounding errors
///
/// This module provides tools to analyze how numerical errors accumulate
/// over long cycles, helping to make informed decisions about numerical
/// precision strategies.
use crate::precision::*;
use crate::{DecimalApproach, F64Approach, HybridApproach, PrecisionApproach};

/// Result of precision analysis over a simulated cycle
#[derive(Debug, Clone)]
pub struct PrecisionAnalysisResult {
    pub approach_name: String,
    pub num_hops: usize,
    pub initial_amount: f64,
    pub final_amount: f64,
    pub expected_amount: f64,
    pub absolute_error: f64,
    pub relative_error_bps: f64, // basis points
    pub worst_case_error: f64,
}

impl PrecisionAnalysisResult {
    pub fn is_acceptable(&self, max_error_bps: f64) -> bool {
        self.relative_error_bps.abs() <= max_error_bps
    }
}

/// Simulate a long cycle to measure cumulative error
///
/// This simulates trading through a cycle many times, accumulating
/// rounding errors at each step.
pub fn analyze_cumulative_error<A: PrecisionApproach>(
    approach: &A,
    initial_amount: f64,
    rates: &[f64],
    fees_bps: &[u16],
    num_iterations: usize,
) -> PrecisionAnalysisResult {
    let mut amount = initial_amount;
    let hops_per_iteration = rates.len();
    let total_hops = hops_per_iteration * num_iterations;

    // Track worst-case error at any point
    let mut worst_error: f64 = 0.0;

    // Calculate expected final amount (using high-precision reference)
    let expected_per_iteration = rates
        .iter()
        .zip(fees_bps.iter())
        .map(|(&r, &f)| r * (1.0 - f as f64 / 10000.0))
        .product::<f64>();
    let expected_final = initial_amount * expected_per_iteration.powi(num_iterations as i32);

    // Simulate the actual cycle using the approach
    for iter in 0..num_iterations {
        for (i, (&rate, &fee_bps)) in rates.iter().zip(fees_bps.iter()).enumerate() {
            let rate_after_fee = approach.apply_fee(rate, fee_bps);
            let rate_after_fee_f64 = approach.to_f64(&rate_after_fee);
            amount *= rate_after_fee_f64;

            // Track worst error at checkpoints
            if (iter * hops_per_iteration + i + 1) % 100 == 0 {
                let current_hops = iter * hops_per_iteration + i + 1;
                let expected_at_hop = initial_amount
                    * expected_per_iteration.powi((current_hops / hops_per_iteration) as i32);
                let error = (amount - expected_at_hop).abs();
                worst_error = worst_error.max(error);
            }
        }
    }

    let absolute_error = amount - expected_final;
    let relative_error_bps = if expected_final != 0.0 {
        (absolute_error / expected_final) * 10000.0
    } else {
        0.0
    };

    PrecisionAnalysisResult {
        approach_name: approach.name().to_string(),
        num_hops: total_hops,
        initial_amount,
        final_amount: amount,
        expected_amount: expected_final,
        absolute_error,
        relative_error_bps,
        worst_case_error: worst_error,
    }
}

/// Analyze precision for a realistic cryptocurrency arbitrage scenario
pub fn analyze_crypto_cycle() -> Vec<PrecisionAnalysisResult> {
    let initial = 10000.0; // $10,000

    // Realistic 3-hop cycle: USD -> BTC -> ETH -> USD
    let rates = vec![
        1.0 / test_data::BTC_USD_RATE, // USD -> BTC
        1.0 / test_data::ETH_BTC_RATE, // BTC -> ETH
        test_data::ETH_USD_RATE,       // ETH -> USD
    ];

    let fees = vec![
        test_data::TAKER_FEE_BPS,
        test_data::TAKER_FEE_BPS,
        test_data::TAKER_FEE_BPS,
    ];

    // Simulate 1000 iterations (3000 hops total)
    let iterations = 1000;

    let mut results = Vec::new();

    // Test f64 approach
    let f64_approach = F64Approach::default();
    results.push(analyze_cumulative_error(
        &f64_approach,
        initial,
        &rates,
        &fees,
        iterations,
    ));

    // Test hybrid approach
    let hybrid_approach = HybridApproach::default();
    results.push(analyze_cumulative_error(
        &hybrid_approach,
        initial,
        &rates,
        &fees,
        iterations,
    ));

    // Test decimal approach
    let decimal_approach = DecimalApproach::default();
    results.push(analyze_cumulative_error(
        &decimal_approach,
        initial,
        &rates,
        &fees,
        iterations,
    ));

    results
}

/// Test with extreme values to find edge cases
pub fn analyze_extreme_values() -> Vec<PrecisionAnalysisResult> {
    let mut results = Vec::new();

    // Test 1: Very small amounts (satoshi-level)
    let small_amount = test_data::BTC_TICK * 100.0; // 100 satoshis
    let rates = vec![test_data::BTC_USD_RATE];
    let fees = vec![test_data::TAKER_FEE_BPS];

    let f64_small = F64Approach::default();
    results.push(analyze_cumulative_error(
        &f64_small,
        small_amount,
        &rates,
        &fees,
        100,
    ));

    // Test 2: Very large amounts
    let large_amount = 1_000_000.0; // $1M
    let hybrid_large = HybridApproach::default();
    results.push(analyze_cumulative_error(
        &hybrid_large,
        large_amount,
        &rates,
        &fees,
        100,
    ));

    // Test 3: High precision rates (many decimal places)
    let precise_rate = 1.123456789123456;
    let precise_rates = vec![precise_rate];
    let decimal_precise = DecimalApproach::default();
    results.push(analyze_cumulative_error(
        &decimal_precise,
        10000.0,
        &precise_rates,
        &fees,
        1000,
    ));

    results
}

/// Compare false positive/negative rates for profitability detection
pub fn analyze_profitability_thresholds() {
    println!("\n=== Profitability Threshold Analysis ===\n");

    // Test near-zero profits with different epsilons
    let test_profits = vec![
        -1e-15, -1e-14, -1e-13, -1e-12, -1e-11, -1e-10, -1e-9, -1e-8, -1e-7, -1e-6, -1e-5, -1e-4,
    ];

    let epsilons = vec![1e-10, 1e-8, 1e-6, 1e-4];

    for epsilon in epsilons {
        let approach = F64Approach::new(epsilon);
        println!("Epsilon: {:.2e}", epsilon);

        for &profit in &test_profits {
            let is_profitable = approach.is_profitable(&profit, 0.0);
            println!(
                "  Profit {:.2e}: {}",
                profit,
                if is_profitable {
                    "✓ PROFITABLE"
                } else {
                    "✗ not profitable"
                }
            );
        }
        println!();
    }
}

/// Generate a comprehensive precision report
pub fn generate_precision_report() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║          LOOM NUMERICAL PRECISION ANALYSIS              ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // Test 1: Crypto cycle with realistic values
    println!("Test 1: Realistic Cryptocurrency Cycle");
    println!("---------------------------------------");
    println!("Scenario: $10,000 through 3-hop cycle (USD->BTC->ETH->USD)");
    println!("Iterations: 1,000 (3,000 total hops)");
    println!("Fees: 0.60% taker fee per hop\n");

    let crypto_results = analyze_crypto_cycle();
    for result in &crypto_results {
        print_result(result);
    }

    // Test 2: Extreme values
    println!("\nTest 2: Extreme Value Scenarios");
    println!("--------------------------------");
    let extreme_results = analyze_extreme_values();
    for result in &extreme_results {
        print_result(result);
    }

    // Test 3: Profitability thresholds
    analyze_profitability_thresholds();

    // Summary and recommendations
    println!("\n=== Summary ===\n");

    let max_acceptable_error_bps = 0.1; // 0.1 basis points = 0.001%

    for result in &crypto_results {
        let status = if result.is_acceptable(max_acceptable_error_bps) {
            "✓ PASS"
        } else {
            "✗ FAIL"
        };

        println!(
            "{}: {} (error: {:.4} bps)",
            result.approach_name, status, result.relative_error_bps
        );
    }
}

fn print_result(result: &PrecisionAnalysisResult) {
    println!("Approach: {}", result.approach_name);
    println!("  Total hops: {}", result.num_hops);
    println!("  Initial: ${:.8}", result.initial_amount);
    println!("  Expected: ${:.8}", result.expected_amount);
    println!("  Actual: ${:.8}", result.final_amount);
    println!("  Absolute error: ${:.8}", result.absolute_error);
    println!("  Relative error: {:.4} bps", result.relative_error_bps);
    println!("  Worst-case error: ${:.8}", result.worst_case_error);
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precision_analysis_runs() {
        // Just ensure the analysis runs without panicking
        let results = analyze_crypto_cycle();
        assert_eq!(results.len(), 3);

        for result in results {
            assert!(result.num_hops > 0);
            assert!(result.final_amount > 0.0);
        }
    }

    #[test]
    fn test_extreme_values_analysis() {
        let results = analyze_extreme_values();
        assert!(results.len() >= 3);
    }

    #[test]
    fn test_acceptable_error_threshold() {
        let result = PrecisionAnalysisResult {
            approach_name: "test".to_string(),
            num_hops: 100,
            initial_amount: 10000.0,
            final_amount: 10001.0,
            expected_amount: 10000.0,
            absolute_error: 1.0,
            relative_error_bps: 1.0, // 0.01%
            worst_case_error: 1.0,
        };

        assert!(result.is_acceptable(2.0)); // Within 2 bps
        assert!(!result.is_acceptable(0.5)); // Not within 0.5 bps
    }
}
