// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

pub mod decimal_approach;
pub mod f64_approach;
pub mod hybrid_approach;

/// Common trait for all numerical precision approaches.
///
/// This trait defines the core operations needed for financial calculations
/// in the Loom arbitrage engine:
///
/// 1. Exchange rate transformation: -ln(rate) for graph weights
/// 2. Fee application: rate * (1 - fee_bps/10000)
/// 3. Cumulative product over cycle: product of edges
/// 4. Profitability test: is cycle negative-weight?
pub trait PrecisionApproach: Clone + Send + Sync {
    /// The numeric type used for calculations
    type Numeric: Clone + Send + Sync;

    /// Transform an exchange rate to a graph edge weight: -ln(rate * (1 - fee_bps/10000))
    ///
    /// # Arguments
    /// * `rate` - The exchange rate (e.g., 50000.0 for BTC-USD)
    /// * `fee_bps` - Fee in basis points (e.g., 40 for 0.40%)
    ///
    /// # Returns
    /// The transformed edge weight for use in Bellman-Ford
    fn transform_rate(&self, rate: f64, fee_bps: u16) -> Self::Numeric;

    /// Apply a fee to an exchange rate: rate * (1 - fee_bps/10000)
    ///
    /// # Arguments
    /// * `rate` - The exchange rate
    /// * `fee_bps` - Fee in basis points
    ///
    /// # Returns
    /// The rate after fee deduction
    fn apply_fee(&self, rate: f64, fee_bps: u16) -> Self::Numeric;

    /// Calculate the cumulative product along a cycle path
    ///
    /// # Arguments
    /// * `weights` - Slice of edge weights in the cycle
    ///
    /// # Returns
    /// The sum of weights (in -ln space, equivalent to product in rate space)
    fn cumulative_weight(&self, weights: &[Self::Numeric]) -> Self::Numeric;

    /// Test if a cycle is profitable (negative weight in -ln space)
    ///
    /// # Arguments
    /// * `cumulative_weight` - The sum of edge weights in the cycle
    /// * `epsilon` - Tolerance for "approximately zero" (may be ignored by exact approaches)
    ///
    /// # Returns
    /// true if the cycle represents a profitable arbitrage opportunity
    fn is_profitable(&self, cumulative_weight: &Self::Numeric, epsilon: f64) -> bool;

    /// Convert a numeric value to f64 for display/logging
    fn to_f64(&self, value: &Self::Numeric) -> f64;

    /// Name of this approach for reporting
    fn name(&self) -> &'static str;
}

/// Realistic cryptocurrency tick sizes and fee structures for testing
pub mod test_data {
    /// Bitcoin tick size: 1 satoshi = 0.00000001 BTC
    pub const BTC_TICK: f64 = 0.00000001;

    /// Ethereum tick size: 1 gwei = 0.000001 ETH (approximate for pricing)
    pub const ETH_TICK: f64 = 0.000001;

    /// USD tick size: 1 cent
    pub const USD_TICK: f64 = 0.01;

    /// Coinbase maker fee (tier 1): 0.40%
    pub const MAKER_FEE_BPS: u16 = 40;

    /// Coinbase taker fee (tier 1): 0.60%
    pub const TAKER_FEE_BPS: u16 = 60;

    /// Typical exchange rates for testing
    pub const BTC_USD_RATE: f64 = 50000.0;
    pub const ETH_USD_RATE: f64 = 3000.0;
    pub const ETH_BTC_RATE: f64 = 0.06;

    /// Number of hops to test for cumulative error
    pub const LONG_CYCLE_HOPS: usize = 10_000;
}
