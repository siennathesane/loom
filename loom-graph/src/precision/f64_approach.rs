// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

use super::PrecisionApproach;

/// Approach 1: f64 everywhere with epsilon comparisons
///
/// Uses native f64 for all calculations with hardware acceleration.
/// Trades precision for performance. Requires careful epsilon tuning
/// for profitability detection to avoid false positives/negatives.
///
/// Pros:
/// - Fast: native hardware support, SIMD-friendly
/// - Simple: no type conversions
/// - Well-understood performance characteristics
///
/// Cons:
/// - Rounding errors accumulate over long cycles
/// - Need to define and tune epsilon values
/// - Not exact for financial calculations
#[derive(Clone, Copy, Debug)]
pub struct F64Approach {
    /// Epsilon for "approximately zero" profit detection
    /// This is the threshold below which we consider a cycle
    /// to have zero profit (no arbitrage opportunity).
    ///
    /// Tuning guidance:
    /// - Too small: false positives (detect non-existent opportunities)
    /// - Too large: false negatives (miss real opportunities)
    /// - Should be > cumulative rounding error for expected cycle length
    pub epsilon: f64,
}

impl F64Approach {
    /// Create a new F64 approach with the given epsilon
    ///
    /// # Arguments
    /// * `epsilon` - Threshold for profit detection (in -ln space)
    ///               Typical values: 1e-10 to 1e-6
    pub fn new(epsilon: f64) -> Self {
        Self { epsilon }
    }

    /// Create with a conservative epsilon based on expected cycle length
    ///
    /// # Arguments
    /// * `max_cycle_length` - Maximum expected cycle length (number of hops)
    ///
    /// # Returns
    /// F64Approach with epsilon = max_cycle_length * machine_epsilon * safety_factor
    pub fn with_cycle_length(max_cycle_length: usize) -> Self {
        // Machine epsilon for f64 is ~2.22e-16
        // We use a safety factor of 1000 to account for:
        // - Multiple operations per hop (fee application, ln, negation)
        // - Accumulation effects
        // - Conservative headroom
        const MACHINE_EPSILON: f64 = std::f64::EPSILON;
        const SAFETY_FACTOR: f64 = 1000.0;
        let epsilon = max_cycle_length as f64 * MACHINE_EPSILON * SAFETY_FACTOR;
        Self { epsilon }
    }
}

impl Default for F64Approach {
    fn default() -> Self {
        // Default to 5-hop cycle with safety margin
        Self::with_cycle_length(5)
    }
}

impl PrecisionApproach for F64Approach {
    type Numeric = f64;

    fn transform_rate(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        // Apply fee: rate * (1 - fee_bps/10000)
        let fee_multiplier = 1.0 - (fee_bps as f64) / 10000.0;
        let rate_after_fee = rate * fee_multiplier;

        // Transform to graph weight: -ln(rate_after_fee)
        -rate_after_fee.ln()
    }

    fn apply_fee(&self, rate: f64, fee_bps: u16) -> Self::Numeric {
        let fee_multiplier = 1.0 - (fee_bps as f64) / 10000.0;
        rate * fee_multiplier
    }

    fn cumulative_weight(&self, weights: &[Self::Numeric]) -> Self::Numeric {
        // Sum the weights (addition in -ln space = multiplication in rate space)
        weights.iter().copied().sum()
    }

    fn is_profitable(&self, cumulative_weight: &Self::Numeric, _epsilon_override: f64) -> bool {
        // A cycle is profitable if the cumulative weight is negative
        // (meaning the product of rates > 1.0 in rate space)
        // We use our configured epsilon to handle rounding errors
        *cumulative_weight < -self.epsilon
    }

    fn to_f64(&self, value: &Self::Numeric) -> f64 {
        *value
    }

    fn name(&self) -> &'static str {
        "f64_everywhere"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_rate_basic() {
        let approach = F64Approach::default();

        // Simple test: BTC-USD at 50000 with 0.4% fee
        let rate = 50000.0;
        let fee_bps = 40;
        let weight = approach.transform_rate(rate, fee_bps);

        // Expected: -ln(50000 * 0.996) = -ln(49800) ≈ -10.8155
        let expected = -(50000.0 * 0.996_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_fee_application() {
        let approach = F64Approach::default();

        let rate = 50000.0;
        let fee_bps = 40; // 0.4%
        let rate_after_fee = approach.apply_fee(rate, fee_bps);

        // Expected: 50000 * (1 - 0.004) = 49800
        assert!((rate_after_fee - 49800.0).abs() < 1e-10);
    }

    #[test]
    fn test_cumulative_weight() {
        let approach = F64Approach::default();

        let weights = vec![1.0, 2.0, 3.0];
        let cumulative = approach.cumulative_weight(&weights);

        assert_eq!(cumulative, 6.0);
    }

    #[test]
    fn test_profitability_detection() {
        let approach = F64Approach::new(1e-6);

        // Clearly profitable: -0.01 (negative)
        assert!(approach.is_profitable(&-0.01, 0.0));

        // Clearly not profitable: 0.01 (positive)
        assert!(!approach.is_profitable(&0.01, 0.0));

        // Edge case: just below epsilon
        assert!(approach.is_profitable(&-2e-6, 0.0));

        // Edge case: just above epsilon threshold
        assert!(!approach.is_profitable(&-5e-7, 0.0));
    }

    #[test]
    fn test_epsilon_scaling() {
        let short_cycle = F64Approach::with_cycle_length(3);
        let long_cycle = F64Approach::with_cycle_length(10);

        // Longer cycles should have larger epsilon
        assert!(long_cycle.epsilon > short_cycle.epsilon);

        // Should scale approximately linearly
        let ratio = long_cycle.epsilon / short_cycle.epsilon;
        assert!((ratio - 10.0 / 3.0).abs() < 0.01);
    }
}
