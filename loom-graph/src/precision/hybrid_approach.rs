// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use super::PrecisionApproach;
use fixed::types::I64F64;

/// Approach 2: Hybrid f64/fixed-point
///
/// Uses f64 for graph algorithms (Bellman-Ford, Dijkstra) where speed matters,
/// but fixed-point arithmetic for currency amounts and final profitability checks.
///
/// The key insight: graph algorithms work in -ln space where small rounding errors
/// don't matter much, but when we convert back to actual money amounts, we need
/// exactness.
///
/// Pros:
/// - Fast for hot path (graph algorithms use f64)
/// - Exact for money (fixed-point arithmetic)
/// - Good balance of performance and precision
///
/// Cons:
/// - Conversion overhead at boundaries
/// - More complex than pure f64
/// - Need to carefully define conversion boundaries
#[derive(Clone, Copy, Debug)]
pub struct HybridApproach {
    /// Epsilon for f64 comparisons in -ln space
    pub graph_epsilon: f64,
}

impl HybridApproach {
    pub fn new(graph_epsilon: f64) -> Self {
        Self { graph_epsilon }
    }

    /// Convert a fixed-point amount to f64 for graph algorithms
    #[inline]
    fn fixed_to_f64(value: I64F64) -> f64 {
        value.to_num()
    }

    /// Convert f64 back to fixed-point for money calculations
    #[inline]
    fn f64_to_fixed(value: f64) -> I64F64 {
        I64F64::from_num(value)
    }

    /// Convert exchange rate and fee to fixed-point before transformation
    fn rate_to_fixed(&self, rate: f64, fee_bps: u16) -> I64F64 {
        let fee_multiplier =
            I64F64::from_num(1.0) - I64F64::from_num(fee_bps) / I64F64::from_num(10000);
        I64F64::from_num(rate) * fee_multiplier
    }
}

impl Default for HybridApproach {
    fn default() -> Self {
        Self::new(1e-6)
    }
}

impl PrecisionApproach for HybridApproach {
    type Numeric = f64;

    fn transform_rate(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        // Calculate rate after fee using fixed-point for exactness
        let rate_after_fee_fixed = self.rate_to_fixed(rate, fee_bps);

        // Convert to f64 for logarithm (no exact fixed-point ln)
        let rate_after_fee = Self::fixed_to_f64(rate_after_fee_fixed);

        // Return -ln in f64 space (this is what Bellman-Ford operates on)
        -rate_after_fee.ln()
    }

    fn apply_fee(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        // Use fixed-point for fee calculation
        let rate_after_fee = self.rate_to_fixed(rate, fee_bps);
        Self::fixed_to_f64(rate_after_fee)
    }

    fn cumulative_weight(&self, weights: &[Self::Numeric]) -> Self::Numeric {
        // Sum in f64 space (fast, used by Bellman-Ford)
        weights.iter().copied().sum()
    }

    fn is_profitable(&self, cumulative_weight: &Self::Numeric, _epsilon_override: f64) -> bool {
        // For profitability check, we could convert back to fixed-point,
        // but in practice, the cumulative weight in -ln space is what matters
        // for cycle detection. The actual profit calculation (when executing)
        // would use fixed-point.
        *cumulative_weight < -self.graph_epsilon
    }

    fn to_f64(&self, value: &Self::Numeric) -> f64 {
        *value
    }

    fn name(&self) -> &'static str {
        "hybrid_f64_fixed"
    }
}

/// Helper functions for currency amount conversions
impl HybridApproach {
    /// Convert a currency amount to fixed-point for exact arithmetic
    pub fn currency_to_fixed(amount: f64) -> I64F64 {
        I64F64::from_num(amount)
    }

    /// Calculate exact profit from a cycle
    ///
    /// This is used after cycle detection to compute the actual profit
    /// using exact fixed-point arithmetic.
    pub fn exact_profit(initial_amount: f64, rates: &[f64], fees_bps: &[u16]) -> I64F64 {
        let mut amount = Self::currency_to_fixed(initial_amount);

        for (&rate, &fee_bps) in rates.iter().zip(fees_bps.iter()) {
            let fee_multiplier =
                I64F64::from_num(1.0) - I64F64::from_num(fee_bps) / I64F64::from_num(10000);
            let rate_fixed = I64F64::from_num(rate);
            amount = amount * rate_fixed * fee_multiplier;
        }

        amount - Self::currency_to_fixed(initial_amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_rate() {
        let approach = HybridApproach::default();

        let rate = 50000.0;
        let fee_bps = 40;
        let weight = approach.transform_rate(rate, fee_bps);

        // Should match f64 result closely
        let expected = -(50000.0 * 0.996_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_fee_application_exact() {
        let approach = HybridApproach::default();

        let rate = 50000.0;
        let fee_bps = 40;
        let rate_after_fee = approach.apply_fee(rate, fee_bps);

        // Should be exact (within fixed-point precision)
        let expected = 49800.0;
        assert!((rate_after_fee - expected).abs() < 1e-12);
    }

    #[test]
    fn test_exact_profit_calculation() {
        // Simple cycle: USD -> BTC -> ETH -> USD
        let initial = 10000.0; // $10,000
        let rates = vec![
            1.0 / 50000.0, // USD->BTC: buy BTC at $50k
            16.666667,     // BTC->ETH: 1 BTC = ~16.67 ETH
            3000.0,        // ETH->USD: sell ETH at $3k
        ];
        let fees = vec![60, 60, 60]; // 0.6% taker fee each hop

        let profit = HybridApproach::exact_profit(initial, &rates, &fees);

        // Calculate expected manually:
        // $10k -> 0.2 BTC -> 3.333 ETH -> $10k (minus fees at each step)
        // After fees: 10000 * (1-0.006) * (1/50000) * (1-0.006) * 16.666667 * (1-0.006) * 3000
        let expected_final = 10000.0 * 0.994 * (1.0 / 50000.0) * 0.994 * 16.666667 * 0.994 * 3000.0;
        let expected_profit = expected_final - 10000.0;

        let profit_f64: f64 = profit.to_num();
        assert!((profit_f64 - expected_profit).abs() < 1.0); // Within $1
    }

    #[test]
    fn test_fixed_point_no_accumulation_error() {
        // Test that fixed-point doesn't accumulate errors like f64
        let mut fixed_val = I64F64::from_num(1.0);
        let mut f64_val = 1.0;

        let increment = 0.1;
        for _ in 0..10 {
            fixed_val += I64F64::from_num(increment);
            f64_val += increment;
        }

        // Fixed-point should be exact
        let fixed_result: f64 = fixed_val.to_num();
        assert!((fixed_result - 2.0).abs() < 1e-15);

        // f64 has classic 0.1 accumulation error
        // 1.0 + 10*0.1 should equal 2.0, but with f64 it may not be exact
        // Allow small error for f64
        let diff_f64: f64 = f64_val - 2.0;
        assert!(diff_f64.abs() < 1e-14);

        // The point is fixed-point is more reliable for decimal arithmetic
    }

    #[test]
    fn test_conversion_boundary() {
        let approach = HybridApproach::default();

        // Test the conversion boundary: fixed -> f64 -> operations -> compare
        let rate = 12345.6789;
        let fee_bps = 45;

        // Method 1: Convert to fixed first, then transform
        let weight1 = approach.transform_rate(rate, fee_bps);

        // Method 2: Pure f64 (for comparison)
        let fee_mult = 1.0 - (fee_bps as f64) / 10000.0;
        let weight2 = -(rate * fee_mult).ln();

        // Should be very close (fixed-point provides exact fee calculation)
        assert!((weight1 - weight2).abs() < 1e-12);
    }
}
