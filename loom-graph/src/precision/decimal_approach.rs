// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use super::PrecisionApproach;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

/// Approach 3: rust_decimal for all currency amounts
///
/// Uses Decimal (128-bit decimal floating point) for all financial calculations.
/// Provides exact decimal arithmetic with no binary rounding errors.
///
/// Pros:
/// - Exact decimal arithmetic (no 0.1 + 0.2 != 0.3 issues)
/// - No cumulative rounding errors
/// - Safe for financial calculations
/// - Industry standard approach (similar to Java BigDecimal, Python Decimal)
///
/// Cons:
/// - Performance overhead (not hardware-accelerated)
/// - No native SIMD support
/// - Slower than f64 for large-scale computations
/// - ln() requires conversion to f64 (Decimal doesn't have transcendental functions)
#[derive(Clone, Copy, Debug)]
pub struct DecimalApproach {
    /// Epsilon for profitability checks (in f64 space after conversion)
    pub epsilon: f64,
}

impl DecimalApproach {
    pub fn new(epsilon: f64) -> Self {
        Self { epsilon }
    }

    /// Convert Decimal to f64 for logarithm operations
    ///
    /// Decimal doesn't support transcendental functions (ln, exp, etc.),
    /// so we must convert to f64 for these operations.
    #[inline]
    fn decimal_to_f64(value: Decimal) -> f64 {
        value.to_f64().unwrap_or(0.0)
    }

    /// Convert f64 to Decimal for exact arithmetic
    #[inline]
    fn f64_to_decimal(value: f64) -> Decimal {
        Decimal::from_f64(value).unwrap_or(Decimal::ZERO)
    }
}

impl Default for DecimalApproach {
    fn default() -> Self {
        Self::new(1e-10)
    }
}

impl PrecisionApproach for DecimalApproach {
    // We store f64 weights for Bellman-Ford (since we need ln anyway)
    // but use Decimal for intermediate currency calculations
    type Numeric = f64;

    fn transform_rate(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        // Convert to Decimal for exact fee calculation
        let rate_dec = Self::f64_to_decimal(rate);
        let fee_bps_dec = Decimal::from(fee_bps);
        let ten_thousand = Decimal::from(10000);

        // Exact calculation: rate * (1 - fee_bps/10000)
        let fee_multiplier = Decimal::ONE - (fee_bps_dec / ten_thousand);
        let rate_after_fee_dec = rate_dec * fee_multiplier;

        // Convert to f64 for logarithm
        let rate_after_fee = Self::decimal_to_f64(rate_after_fee_dec);

        // Return -ln (same as other approaches)
        -rate_after_fee.ln()
    }

    fn apply_fee(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        // Exact fee calculation in Decimal space
        let rate_dec = Self::f64_to_decimal(rate);
        let fee_bps_dec = Decimal::from(fee_bps);
        let ten_thousand = Decimal::from(10000);

        let fee_multiplier = Decimal::ONE - (fee_bps_dec / ten_thousand);
        let rate_after_fee = rate_dec * fee_multiplier;

        Self::decimal_to_f64(rate_after_fee)
    }

    fn cumulative_weight(&self, weights: &[Self::Numeric]) -> Self::Numeric {
        // Sum in f64 space (Bellman-Ford operates in -ln space)
        weights.iter().copied().sum()
    }

    fn is_profitable(&self, cumulative_weight: &Self::Numeric, _epsilon_override: f64) -> bool {
        // Check profitability with epsilon tolerance
        *cumulative_weight < -self.epsilon
    }

    fn to_f64(&self, value: &Self::Numeric) -> f64 {
        *value
    }

    fn name(&self) -> &'static str {
        "rust_decimal"
    }
}

/// Helper functions for exact currency calculations
impl DecimalApproach {
    /// Calculate exact profit through a cycle using Decimal arithmetic
    ///
    /// This is the "pure Decimal" version that maintains exact precision
    /// throughout the entire calculation.
    pub fn exact_cycle_profit(initial_amount: f64, rates: &[f64], fees_bps: &[u16]) -> Decimal {
        let mut amount = Decimal::from_f64(initial_amount).unwrap_or(Decimal::ZERO);
        let initial = amount;

        let ten_thousand = Decimal::from(10000);

        for (&rate, &fee_bps) in rates.iter().zip(fees_bps.iter()) {
            let rate_dec = Decimal::from_f64(rate).unwrap_or(Decimal::ZERO);
            let fee_bps_dec = Decimal::from(fee_bps);
            let fee_multiplier = Decimal::ONE - (fee_bps_dec / ten_thousand);

            amount = amount * rate_dec * fee_multiplier;
        }

        amount - initial
    }

    /// Check if a cycle is profitable using exact Decimal arithmetic
    pub fn is_cycle_profitable_exact(initial_amount: f64, rates: &[f64], fees_bps: &[u16]) -> bool {
        let profit = Self::exact_cycle_profit(initial_amount, rates, fees_bps);
        profit > Decimal::ZERO
    }

    /// Calculate the profit percentage (exact)
    pub fn profit_percentage(initial_amount: f64, rates: &[f64], fees_bps: &[u16]) -> Decimal {
        let initial = Decimal::from_f64(initial_amount).unwrap_or(Decimal::ZERO);
        if initial.is_zero() {
            return Decimal::ZERO;
        }

        let profit = Self::exact_cycle_profit(initial_amount, rates, fees_bps);
        let hundred = Decimal::from(100);

        (profit / initial) * hundred
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_rate() {
        let approach = DecimalApproach::default();

        let rate = 50000.0;
        let fee_bps = 40;
        let weight = approach.transform_rate(rate, fee_bps);

        // Should match expected value
        let expected = -(50000.0 * 0.996_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_exact_fee_calculation() {
        let approach = DecimalApproach::default();

        // Test with a value that has binary rounding issues
        let rate = 1234.56789;
        let fee_bps = 45; // 0.45%
        let rate_after_fee = approach.apply_fee(rate, fee_bps);

        // Expected: 1234.56789 * (1 - 0.0045) = 1234.56789 * 0.9955
        let expected = 1234.56789 * 0.9955;

        // Decimal should give us exact result (within conversion tolerance)
        assert!((rate_after_fee - expected).abs() < 1e-10);
    }

    #[test]
    fn test_exact_cycle_profit() {
        // Simple profitable cycle
        let initial = 10000.0;
        let rates = vec![
            1.0 / 50000.0, // USD->BTC
            16.666667,     // BTC->ETH
            3000.0,        // ETH->USD
        ];
        let fees = vec![60, 60, 60]; // 0.6% each

        let profit = DecimalApproach::exact_cycle_profit(initial, &rates, &fees);

        // This cycle should result in approximately the initial amount (minus fees)
        // We're testing that Decimal gives us consistent, exact results
        let profit_f64 = profit.to_f64().unwrap();

        // The cycle isn't actually profitable due to fees, so profit should be negative
        assert!(profit < Decimal::ZERO);

        // But we can verify the calculation is exact by comparing to manual calc
        let expected = {
            let d_init = Decimal::from_f64(initial).unwrap();
            let d_rate1 = Decimal::from_f64(rates[0]).unwrap();
            let d_rate2 = Decimal::from_f64(rates[1]).unwrap();
            let d_rate3 = Decimal::from_f64(rates[2]).unwrap();

            let fee_mult = Decimal::from_f64(0.994).unwrap(); // 1 - 0.006

            let final_amount =
                d_init * d_rate1 * fee_mult * d_rate2 * fee_mult * d_rate3 * fee_mult;
            (final_amount - d_init).to_f64().unwrap()
        };

        assert!((profit_f64 - expected).abs() < 0.01); // Within 1 cent
    }

    #[test]
    fn test_no_decimal_rounding_errors() {
        // Classic f64 problem: 0.1 + 0.2 != 0.3
        let d1 = Decimal::from_f64(0.1).unwrap();
        let d2 = Decimal::from_f64(0.2).unwrap();
        let d3 = Decimal::from_f64(0.3).unwrap();

        // Decimal arithmetic is exact for decimal values
        assert_eq!(d1 + d2, d3);

        // Compare with f64
        let f1 = 0.1_f64;
        let f2 = 0.2_f64;
        let f3 = 0.3_f64;
        assert_ne!(f1 + f2, f3); // f64 fails this test
    }

    #[test]
    fn test_profit_percentage() {
        let initial = 10000.0;

        // Cycle with 2% total profit
        let rates = vec![1.02];
        let fees = vec![0]; // No fees for simplicity

        let profit_pct = DecimalApproach::profit_percentage(initial, &rates, &fees);

        // Should be exactly 2.0%
        let expected = Decimal::from_f64(2.0).unwrap();
        let diff = (profit_pct - expected).abs();

        assert!(diff < Decimal::from_f64(0.001).unwrap());
    }

    #[test]
    fn test_is_cycle_profitable() {
        let approach = DecimalApproach::default();

        // Profitable cycle (1% profit)
        let rates = vec![1.01];
        let fees = vec![0];
        assert!(DecimalApproach::is_cycle_profitable_exact(
            10000.0, &rates, &fees
        ));

        // Unprofitable cycle (1% loss)
        let rates = vec![0.99];
        let fees = vec![0];
        assert!(!DecimalApproach::is_cycle_profitable_exact(
            10000.0, &rates, &fees
        ));

        // Break-even (exactly equal)
        let rates = vec![1.0];
        let fees = vec![0];
        assert!(!DecimalApproach::is_cycle_profitable_exact(
            10000.0, &rates, &fees
        ));
    }

    #[test]
    fn test_cumulative_precision() {
        // Test that Decimal maintains precision over many operations
        let initial = Decimal::from_f64(1000.0).unwrap();
        let mut amount = initial;

        // Apply 100 small adjustments
        let adjustment = Decimal::from_f64(0.001).unwrap();
        for _ in 0..100 {
            amount += adjustment;
        }

        // Should be exactly 1000 + 100*0.001 = 1000.1
        let expected = Decimal::from_f64(1000.1).unwrap();
        assert_eq!(amount, expected);
    }
}
