// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! State machine for lump sum lifecycle management
//!
//! Manages transitions between states (PARKED, OPPORTUNITY, EXECUTING, etc.)
//! and enforces safety rules:
//! - Gravitational gate (profit threshold)
//! - Contention prevention (one lump sum per instrument)
//! - Rate limiting (token bucket)

use crate::lump_sum::{LumpSum, LumpSumId, LumpSumState};
use std::collections::HashMap;
use std::time::Instant;

/// Simple token bucket rate limiter
struct RateLimiter {
    /// Tokens available
    tokens: f64,
    /// Maximum tokens
    max_tokens: f64,
    /// Refill rate (tokens per second)
    refill_rate: f64,
    /// Last refill timestamp
    last_refill: Instant,
}

impl RateLimiter {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume a token, returns true if successful
    fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    /// Get current token count (for testing)
    fn available_tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// State machine manager for all lump sums
pub struct StateMachine {
    /// All lump sums indexed by ID
    lump_sums: HashMap<LumpSumId, LumpSum>,

    /// Contention tracking: instrument → lump sum currently using it
    /// Prevents multiple lump sums from trading the same pair simultaneously
    in_flight: HashMap<String, LumpSumId>,

    /// Rate limit tracking (token bucket)
    /// For Coinbase: 15 orders per second, use 10 for safety
    rate_limiter: RateLimiter,

    /// Gravitational safety margin (configurable, default 1.5)
    safety_margin: f64,
}

impl StateMachine {
    /// Create a new state machine
    pub fn new(safety_margin: f64) -> Self {
        Self {
            lump_sums: HashMap::new(),
            in_flight: HashMap::new(),
            // Coinbase limit: 15 orders/sec, use 10 for safety
            rate_limiter: RateLimiter::new(10.0, 10.0),
            safety_margin,
        }
    }

    /// Add a new lump sum to the system
    pub fn add_lump_sum(&mut self, lump_sum: LumpSum) {
        self.lump_sums.insert(lump_sum.id, lump_sum);
    }

    /// Get a lump sum by ID
    pub fn get_lump_sum(&self, id: &LumpSumId) -> Option<&LumpSum> {
        self.lump_sums.get(id)
    }

    /// Get a mutable lump sum by ID
    pub fn get_lump_sum_mut(&mut self, id: &LumpSumId) -> Option<&mut LumpSum> {
        self.lump_sums.get_mut(id)
    }

    /// Get all lump sums in PARKED state
    pub fn get_parked_lump_sums(&self) -> Vec<&LumpSum> {
        self.lump_sums
            .values()
            .filter(|ls| ls.state == LumpSumState::Parked)
            .collect()
    }

    /// Transition to OPPORTUNITY state
    ///
    /// Returns true if transition allowed, false if gated
    ///
    /// # Gravitational Gate
    /// Opportunity allowed only if: expected_profit > return_cost × safety_margin
    pub fn transition_to_opportunity(
        &mut self,
        lump_sum_id: &LumpSumId,
        expected_profit: f64,
        return_cost: f64,
    ) -> bool {
        let lump_sum = match self.lump_sums.get_mut(lump_sum_id) {
            Some(ls) => ls,
            None => return false,
        };

        // Must be in PARKED state
        if lump_sum.state != LumpSumState::Parked {
            return false;
        }

        // Gravitational gate check: profit > return_cost × safety_margin
        if expected_profit <= return_cost * self.safety_margin {
            return false; // Gated: opportunity not profitable enough
        }

        // Transition allowed
        lump_sum.state = LumpSumState::Opportunity;
        true
    }

    /// Check if hop is allowed (contention + rate limit)
    ///
    /// For Listen Mode: Always returns true (no actual execution)
    /// For Active Mode: Checks contention and rate limits
    ///
    /// # Arguments
    /// * `lump_sum_id` - Lump sum attempting the hop
    /// * `instrument` - Trading pair (e.g., "BTC-USD")
    /// * `active_mode` - Whether we're in Active mode
    pub fn check_hop_allowed(
        &mut self,
        lump_sum_id: &LumpSumId,
        instrument: &str,
        active_mode: bool,
    ) -> bool {
        if !active_mode {
            return true; // Listen Mode: always allowed (virtual)
        }

        // Check contention: is another lump sum already using this pair?
        if let Some(other_id) = self.in_flight.get(instrument) {
            if other_id != lump_sum_id {
                return false; // Contention: another lump sum using this pair
            }
        }

        // Check rate limit
        if !self.rate_limiter.try_consume() {
            return false; // Rate limited
        }

        // Mark as in-flight
        self.in_flight.insert(instrument.to_string(), *lump_sum_id);
        true
    }

    /// Clear in-flight status for an instrument
    pub fn clear_in_flight(&mut self, instrument: &str) {
        self.in_flight.remove(instrument);
    }

    /// Transition back to PARKED state
    pub fn transition_to_parked(&mut self, lump_sum_id: &LumpSumId) {
        if let Some(lump_sum) = self.lump_sums.get_mut(lump_sum_id) {
            lump_sum.state = LumpSumState::Parked;
        }
    }

    /// Get current safety margin
    pub fn safety_margin(&self) -> f64 {
        self.safety_margin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lump_sum::LumpSum;
    use rust_decimal::Decimal;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_create_state_machine() {
        let sm = StateMachine::new(1.5);

        // Verify safety margin
        assert_eq!(sm.safety_margin(), 1.5);

        // Verify empty state
        assert_eq!(sm.lump_sums.len(), 0);
        assert_eq!(sm.in_flight.len(), 0);
    }

    #[test]
    fn test_add_lump_sum() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;

        sm.add_lump_sum(lump);

        // Verify can retrieve by ID
        let retrieved = sm.get_lump_sum(&lump_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Alpha");
    }

    #[test]
    fn test_gravitational_gate_allows() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // profit=10, return_cost=5, margin=1.5
        // 10 > 5 × 1.5 = 7.5 → ALLOWED
        let allowed = sm.transition_to_opportunity(&lump_id, 10.0, 5.0);
        assert!(allowed);

        // Verify state changed
        let lump = sm.get_lump_sum(&lump_id).unwrap();
        assert_eq!(lump.state, LumpSumState::Opportunity);
    }

    #[test]
    fn test_gravitational_gate_blocks() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // profit=10, return_cost=10, margin=1.5
        // 10 <= 10 × 1.5 = 15 → BLOCKED
        let allowed = sm.transition_to_opportunity(&lump_id, 10.0, 10.0);
        assert!(!allowed);

        // Verify state unchanged
        let lump = sm.get_lump_sum(&lump_id).unwrap();
        assert_eq!(lump.state, LumpSumState::Parked);
    }

    #[test]
    fn test_gravitational_gate_edge_case() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // profit=15.0, return_cost=10, margin=1.5
        // 15.0 == 10 × 1.5 → BLOCKED (need strictly greater)
        let allowed = sm.transition_to_opportunity(&lump_id, 15.0, 10.0);
        assert!(!allowed);
    }

    #[test]
    fn test_contention_prevention() {
        let mut sm = StateMachine::new(1.5);

        let lump_a = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_a_id = lump_a.id;

        let lump_b = LumpSum::new(
            "Bravo".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_b_id = lump_b.id;

        sm.add_lump_sum(lump_a);
        sm.add_lump_sum(lump_b);

        // Lump A uses "BTC-USD" (Active mode)
        let allowed_a = sm.check_hop_allowed(&lump_a_id, "BTC-USD", true);
        assert!(allowed_a); // First use: allowed

        // Lump B tries "BTC-USD" → blocked by contention
        let allowed_b = sm.check_hop_allowed(&lump_b_id, "BTC-USD", true);
        assert!(!allowed_b); // Blocked: contention

        // Clear in-flight
        sm.clear_in_flight("BTC-USD");

        // Lump B tries again → allowed
        let allowed_b2 = sm.check_hop_allowed(&lump_b_id, "BTC-USD", true);
        assert!(allowed_b2); // Now allowed
    }

    #[test]
    fn test_rate_limiter() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // Consume 10 tokens rapidly (max capacity)
        for i in 0..10 {
            let allowed = sm.check_hop_allowed(
                &lump_id,
                &format!("PAIR{}", i),
                true, // Active mode
            );
            assert!(allowed, "Token {} should be available", i + 1);
        }

        // 11th attempt → blocked (no tokens left)
        let allowed_11 = sm.check_hop_allowed(&lump_id, "PAIR11", true);
        assert!(!allowed_11); // Rate limited

        // Wait 0.1s (should refill 1 token at 10 tokens/sec)
        thread::sleep(Duration::from_millis(100));

        // Try again → allowed (refilled)
        let allowed_12 = sm.check_hop_allowed(&lump_id, "PAIR12", true);
        assert!(allowed_12); // Refilled, now allowed
    }

    #[test]
    fn test_get_parked_lump_sums() {
        let mut sm = StateMachine::new(1.5);

        // Add 3 lump sums
        let lump1 = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump1_id = lump1.id;

        let lump2 = LumpSum::new(
            "Bravo".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        let lump3 = LumpSum::new(
            "Charlie".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );

        sm.add_lump_sum(lump1);
        sm.add_lump_sum(lump2);
        sm.add_lump_sum(lump3);

        // Transition lump1 to OPPORTUNITY
        sm.transition_to_opportunity(&lump1_id, 100.0, 10.0);

        // Get parked lump sums
        let parked = sm.get_parked_lump_sums();

        // Should return 2 (Bravo and Charlie)
        assert_eq!(parked.len(), 2);
        let names: Vec<&str> = parked.iter().map(|ls| ls.name.as_str()).collect();
        assert!(names.contains(&"Bravo"));
        assert!(names.contains(&"Charlie"));
        assert!(!names.contains(&"Alpha")); // In OPPORTUNITY state
    }

    #[test]
    fn test_listen_mode_always_allows() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // Listen Mode: active_mode=false
        // Should always allow regardless of contention or rate limits

        // Use same instrument multiple times
        for i in 0..20 {
            let allowed = sm.check_hop_allowed(
                &lump_id, "BTC-USD", // Same instrument every time
                false,     // Listen Mode
            );
            assert!(allowed, "Listen Mode iteration {} should always allow", i);
        }

        // Verify no in-flight tracking in Listen Mode
        assert_eq!(sm.in_flight.len(), 0);
    }

    #[test]
    fn test_transition_to_parked() {
        let mut sm = StateMachine::new(1.5);

        let lump = LumpSum::new(
            "Alpha".to_string(),
            "USD".to_string(),
            Decimal::new(1000, 0),
        );
        let lump_id = lump.id;
        sm.add_lump_sum(lump);

        // Transition to OPPORTUNITY
        sm.transition_to_opportunity(&lump_id, 100.0, 10.0);
        assert_eq!(
            sm.get_lump_sum(&lump_id).unwrap().state,
            LumpSumState::Opportunity
        );

        // Transition back to PARKED
        sm.transition_to_parked(&lump_id);
        assert_eq!(
            sm.get_lump_sum(&lump_id).unwrap().state,
            LumpSumState::Parked
        );
    }
}
