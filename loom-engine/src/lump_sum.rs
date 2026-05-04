// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Lump sum data structures for capital tracking
//!
//! Lump sums are discrete parcels of capital that move through the trading graph.
//! Each lump sum tracks its current position, journey history, and P&L.

use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Unique identifier for a lump sum
pub type LumpSumId = Uuid;

/// Lump sum state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LumpSumState {
    /// Parked at a currency, idle
    Parked,
    /// Detected arbitrage opportunity, evaluating
    Opportunity,
    /// Executing hop (only in Active mode, not used in Listen)
    Executing,
    /// Hop filled successfully (only in Active mode)
    Filled,
    /// Hop rejected (only in Active mode)
    Rejected,
}

impl LumpSumState {
    /// Check if state allows new opportunities to be evaluated
    pub fn can_evaluate_opportunity(&self) -> bool {
        matches!(self, LumpSumState::Parked)
    }

    /// Check if state allows hop execution (Active mode only)
    pub fn can_execute_hop(&self) -> bool {
        matches!(self, LumpSumState::Opportunity)
    }
}

/// Single hop in a lump sum's journey
#[derive(Debug, Clone)]
pub struct Hop {
    /// Sequence number (1-indexed)
    pub seq: u32,
    /// From currency
    pub from_currency: String,
    /// To currency
    pub to_currency: String,
    /// Size before hop (in from_currency)
    pub size_before: Decimal,
    /// Size after hop (in to_currency)
    pub size_after: Decimal,
    /// Exchange rate used
    pub rate: Decimal,
    /// Fee paid (in from_currency)
    pub fee: Decimal,
    /// Timestamp (Unix epoch seconds)
    pub timestamp: u64,
    /// Opportunity cycle this hop was part of (if any)
    pub cycle_id: Option<Uuid>,
}

/// A lump sum - discrete parcel of capital moving through the graph
#[derive(Debug, Clone)]
pub struct LumpSum {
    /// Unique identifier
    pub id: LumpSumId,

    /// Human-readable name (e.g., "Alpha", "Bravo")
    pub name: String,

    /// Base currency (typically "USD")
    pub base_currency: String,

    /// Current currency node
    pub current_currency: String,

    /// Current size (in current_currency)
    pub current_size: Decimal,

    /// Initial size (in base_currency, at creation)
    pub initial_size: Decimal,

    /// Current state
    pub state: LumpSumState,

    /// Hop history (ordered, append-only)
    pub hops: Vec<Hop>,

    /// Creation timestamp (Unix epoch seconds)
    pub created_at: u64,

    /// Last updated timestamp
    pub updated_at: u64,
}

impl LumpSum {
    /// Create a new lump sum at the base currency
    pub fn new(name: String, base_currency: String, initial_size: Decimal) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: Uuid::new_v4(),
            name,
            base_currency: base_currency.clone(),
            current_currency: base_currency,
            current_size: initial_size,
            initial_size,
            state: LumpSumState::Parked,
            hops: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Record a hop (virtual or actual)
    pub fn record_hop(
        &mut self,
        to_currency: String,
        size_after: Decimal,
        rate: Decimal,
        fee: Decimal,
        cycle_id: Option<Uuid>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hop = Hop {
            seq: (self.hops.len() as u32) + 1,
            from_currency: self.current_currency.clone(),
            to_currency: to_currency.clone(),
            size_before: self.current_size,
            size_after,
            rate,
            fee,
            timestamp: now,
            cycle_id,
        };

        self.hops.push(hop);
        self.current_currency = to_currency;
        self.current_size = size_after;
        self.updated_at = now;
    }

    /// Calculate P&L in base currency using gravitational potential (return cost)
    ///
    /// P&L = current_size × exp(-return_cost) - initial_size
    ///
    /// This gives mark-to-market P&L assuming we could convert back to base at the
    /// current market rate (with fees).
    pub fn calculate_pnl(&self, return_cost: f64) -> Decimal {
        // Convert return cost to rate: rate = exp(-cost)
        let return_rate = (-return_cost).exp();

        // Current value in base currency
        let current_value_in_base = self.current_size * Decimal::from_f64(return_rate).unwrap();

        // P&L = current - initial
        current_value_in_base - self.initial_size
    }

    /// Get number of hops taken
    pub fn hop_count(&self) -> usize {
        self.hops.len()
    }

    /// Check if at base currency
    pub fn is_at_base(&self) -> bool {
        self.current_currency == self.base_currency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_lump_sum() {
        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0), // $1000
        );

        // Verify ID is set (non-nil UUID)
        assert_ne!(lump.id, Uuid::nil());

        // Verify name
        assert_eq!(lump.name, "Alpha");

        // Verify at base
        assert_eq!(lump.base_currency, "USD");
        assert_eq!(lump.current_currency, "USD");
        assert!(lump.is_at_base());

        // Verify size
        assert_eq!(lump.current_size, Decimal::new(1000, 0));
        assert_eq!(lump.initial_size, Decimal::new(1000, 0));

        // Verify state
        assert_eq!(lump.state, LumpSumState::Parked);

        // Verify no hops
        assert_eq!(lump.hop_count(), 0);

        // Verify timestamps set
        assert!(lump.created_at > 0);
        assert_eq!(lump.created_at, lump.updated_at);
    }

    #[test]
    fn test_record_hop() {
        let mut lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        // Record hop: USD → BTC
        // Rate: 0.00002 BTC/USD (1 BTC = 50000 USD)
        // Fee: 0.6% = $6
        // Size after: 1000 × 0.00002 × (1 - 0.006) = 0.01988 BTC
        let rate = Decimal::new(2, 5); // 0.00002
        let fee = Decimal::new(6, 0); // $6
        let size_after = Decimal::new(1988, 5); // 0.01988 BTC

        lump.record_hop("BTC".to_string(), size_after, rate, fee, None);

        // Verify position updated
        assert_eq!(lump.current_currency, "BTC");
        assert_eq!(lump.current_size, size_after);
        assert!(!lump.is_at_base());

        // Verify hop recorded
        assert_eq!(lump.hop_count(), 1);

        let hop = &lump.hops[0];
        assert_eq!(hop.seq, 1);
        assert_eq!(hop.from_currency, "USD");
        assert_eq!(hop.to_currency, "BTC");
        assert_eq!(hop.size_before, Decimal::new(1000, 0));
        assert_eq!(hop.size_after, size_after);
        assert_eq!(hop.rate, rate);
        assert_eq!(hop.fee, fee);
        assert!(hop.timestamp > 0);
        assert_eq!(hop.cycle_id, None);

        // Verify updated timestamp changed
        assert!(lump.updated_at >= lump.created_at);
    }

    #[test]
    fn test_multiple_hops() {
        let mut lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        let cycle_id = Uuid::new_v4();

        // Hop 1: USD → BTC
        lump.record_hop(
            "BTC".to_string(),
            Decimal::new(2, 2), // 0.02 BTC
            Decimal::new(2, 5), // rate
            Decimal::new(6, 0), // fee
            Some(cycle_id),
        );

        // Hop 2: BTC → ETH
        lump.record_hop(
            "ETH".to_string(),
            Decimal::new(3, 1),  // 0.3 ETH
            Decimal::new(15, 0), // 15 ETH/BTC
            Decimal::new(1, 4),  // 0.0001 BTC fee
            Some(cycle_id),
        );

        // Hop 3: ETH → USD
        lump.record_hop(
            "USD".to_string(),
            Decimal::new(1020, 0), // $1020
            Decimal::new(3400, 0), // 3400 USD/ETH
            Decimal::new(18, 1),   // 1.8 USD fee
            Some(cycle_id),
        );

        // Verify hop count
        assert_eq!(lump.hop_count(), 3);

        // Verify all hops recorded with correct sequences
        assert_eq!(lump.hops[0].seq, 1);
        assert_eq!(lump.hops[1].seq, 2);
        assert_eq!(lump.hops[2].seq, 3);

        // Verify currency progression
        assert_eq!(lump.hops[0].from_currency, "USD");
        assert_eq!(lump.hops[0].to_currency, "BTC");
        assert_eq!(lump.hops[1].from_currency, "BTC");
        assert_eq!(lump.hops[1].to_currency, "ETH");
        assert_eq!(lump.hops[2].from_currency, "ETH");
        assert_eq!(lump.hops[2].to_currency, "USD");

        // Verify back at base
        assert!(lump.is_at_base());
        assert_eq!(lump.current_size, Decimal::new(1020, 0));

        // Verify cycle IDs match
        assert_eq!(lump.hops[0].cycle_id, Some(cycle_id));
        assert_eq!(lump.hops[1].cycle_id, Some(cycle_id));
        assert_eq!(lump.hops[2].cycle_id, Some(cycle_id));
    }

    #[test]
    fn test_pnl_calculation_profit() {
        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        // Simulate profitable position
        // return_cost = -0.02 means we can return with 2% profit
        // exp(-(-0.02)) = exp(0.02) ≈ 1.0202
        let return_cost = -0.02;
        let pnl = lump.calculate_pnl(return_cost);

        // Expected P&L ≈ 1000 × 1.0202 - 1000 ≈ $20.20
        assert!(pnl > Decimal::ZERO);
        assert!(pnl > Decimal::new(20, 0)); // > $20
        assert!(pnl < Decimal::new(21, 0)); // < $21
    }

    #[test]
    fn test_pnl_calculation_loss() {
        let mut lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        // Move to BTC (simulate current position)
        lump.current_currency = "BTC".to_string();
        lump.current_size = Decimal::new(2, 2); // 0.02 BTC

        // Simulate losing position
        // return_cost = 0.05 means 5% loss to return
        // exp(-0.05) ≈ 0.9512
        // P&L = 0.02 × 0.9512 - 1000
        // Since current_size is already in BTC (0.02), not USD value
        // The actual calculation: 0.02 × 0.9512 ≈ 0.019024 BTC equivalent in base
        // This is wrong - we need to clarify the test

        // Better: Assume current_size is already converted to "equivalent base currency units"
        // For this test, let's use a more straightforward loss scenario
        lump.current_size = Decimal::new(950, 0); // Simulated current value in base terms

        let return_cost = 0.05;
        let pnl = lump.calculate_pnl(return_cost);

        // Verify loss
        // 950 × exp(-0.05) - 1000 = 950 × 0.9512 - 1000 ≈ -96.36
        assert!(pnl < Decimal::ZERO);
        assert!(pnl < Decimal::new(-90, 0)); // More than $90 loss
        assert!(pnl > Decimal::new(-100, 0)); // Less than $100 loss
    }

    #[test]
    fn test_pnl_at_base_zero_cost() {
        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        // At base with zero return cost
        // exp(-0) = 1.0
        // P&L = 1000 × 1.0 - 1000 = 0
        let return_cost = 0.0;
        let pnl = lump.calculate_pnl(return_cost);

        // Should be approximately zero
        assert!(pnl.abs() < Decimal::new(1, 2)); // < $0.01
    }

    #[test]
    fn test_state_transitions() {
        // PARKED: can evaluate opportunities
        assert!(LumpSumState::Parked.can_evaluate_opportunity());
        assert!(!LumpSumState::Parked.can_execute_hop());

        // OPPORTUNITY: can execute hops
        assert!(!LumpSumState::Opportunity.can_evaluate_opportunity());
        assert!(LumpSumState::Opportunity.can_execute_hop());

        // EXECUTING: cannot evaluate or execute
        assert!(!LumpSumState::Executing.can_evaluate_opportunity());
        assert!(!LumpSumState::Executing.can_execute_hop());

        // FILLED: cannot evaluate or execute
        assert!(!LumpSumState::Filled.can_evaluate_opportunity());
        assert!(!LumpSumState::Filled.can_execute_hop());

        // REJECTED: cannot evaluate or execute
        assert!(!LumpSumState::Rejected.can_evaluate_opportunity());
        assert!(!LumpSumState::Rejected.can_execute_hop());
    }

    #[test]
    fn test_hop_sequencing() {
        let mut lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        // Record 5 hops
        for i in 0..5 {
            lump.record_hop(
                format!("CURR{}", i),
                Decimal::new(100, 0),
                Decimal::ONE,
                Decimal::ZERO,
                None,
            );
        }

        // Verify sequence numbers
        assert_eq!(lump.hop_count(), 5);
        for (i, hop) in lump.hops.iter().enumerate() {
            assert_eq!(hop.seq, (i + 1) as u32);
        }

        // Verify append-only (no hops removed)
        assert_eq!(lump.hops.len(), 5);
    }

    #[test]
    fn test_realistic_arbitrage_cycle() {
        let mut lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(10000, 0), // $10,000
        );

        let cycle_id = Uuid::new_v4();

        // Realistic arbitrage: USD → BTC → ETH → USD with 1% profit

        // Hop 1: USD → BTC at 50,000 USD/BTC, 0.5% fee
        // Get: 10000 × (1/50000) × (1 - 0.005) = 0.199 BTC
        lump.record_hop(
            "BTC".to_string(),
            Decimal::new(199, 3), // 0.199 BTC
            Decimal::new(2, 5),   // 0.00002 BTC/USD
            Decimal::new(50, 0),  // $50 fee
            Some(cycle_id),
        );

        // Hop 2: BTC → ETH at 15.0 ETH/BTC, 0.5% fee
        // Get: 0.199 × 15.0 × (1 - 0.005) = 2.97 ETH
        lump.record_hop(
            "ETH".to_string(),
            Decimal::new(297, 2),  // 2.97 ETH
            Decimal::new(15, 0),   // 15 ETH/BTC
            Decimal::new(9975, 7), // 0.0009975 BTC fee
            Some(cycle_id),
        );

        // Hop 3: ETH → USD at 3,400 USD/ETH, 0.5% fee
        // Get: 2.97 × 3400 × (1 - 0.005) = 10,049.46 USD
        lump.record_hop(
            "USD".to_string(),
            Decimal::new(1004946, 2), // $10,049.46
            Decimal::new(3400, 0),    // 3400 USD/ETH
            Decimal::new(5049, 2),    // $50.49 fee
            Some(cycle_id),
        );

        // Verify cycle complete
        assert!(lump.is_at_base());
        assert_eq!(lump.hop_count(), 3);

        // Verify profit: ~$49.46 on $10,000 (~0.5% after fees)
        let profit = lump.current_size - lump.initial_size;
        assert!(profit > Decimal::ZERO);
        assert!(profit > Decimal::new(40, 0));
        assert!(profit < Decimal::new(60, 0));
    }
}
