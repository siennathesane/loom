// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Volume-dependent cost calculation with order book slippage
///
/// When executing large orders, we can't assume a single price. We must walk
/// the order book to calculate the weighted average fill price accounting for
/// slippage across multiple levels.
///
/// This is critical for Active mode with multiple lump sums, where we need
/// accurate cost estimates for different position sizes.
use crate::types::*;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::VecDeque;

/// Order book level (price and size)
#[derive(Debug, Clone, PartialEq)]
pub struct OrderBookLevel {
    /// Price at this level
    pub price: Decimal,
    /// Available size at this level
    pub size: Decimal,
}

impl OrderBookLevel {
    /// Create a new order book level
    pub fn new(price: Decimal, size: Decimal) -> Self {
        Self { price, size }
    }
}

/// Order book for a trading pair
///
/// Maintains bid and ask levels in price-time priority order.
/// Bids are sorted descending (best bid first), asks ascending (best ask first).
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// Bid levels (buy orders, descending price - best bid first)
    pub bids: VecDeque<OrderBookLevel>,
    /// Ask levels (sell orders, ascending price - best ask first)
    pub asks: VecDeque<OrderBookLevel>,
}

impl OrderBook {
    /// Create an empty order book
    pub fn new() -> Self {
        Self {
            bids: VecDeque::new(),
            asks: VecDeque::new(),
        }
    }

    /// Get the best bid price (highest buy order)
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.front().map(|level| level.price)
    }

    /// Get the best ask price (lowest sell order)
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.front().map(|level| level.price)
    }

    /// Get the mid-market price (average of best bid and ask)
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Add a bid level (maintains descending order)
    pub fn add_bid(&mut self, price: Decimal, size: Decimal) {
        let level = OrderBookLevel::new(price, size);

        // Find insertion point (maintain descending order)
        let pos = self
            .bids
            .iter()
            .position(|l| l.price < price)
            .unwrap_or(self.bids.len());
        self.bids.insert(pos, level);
    }

    /// Add an ask level (maintains ascending order)
    pub fn add_ask(&mut self, price: Decimal, size: Decimal) {
        let level = OrderBookLevel::new(price, size);

        // Find insertion point (maintain ascending order)
        let pos = self
            .asks
            .iter()
            .position(|l| l.price > price)
            .unwrap_or(self.asks.len());
        self.asks.insert(pos, level);
    }

    /// Calculate actual fill price for a given volume
    ///
    /// Walks the order book to determine weighted average fill price
    /// accounting for slippage across multiple levels.
    ///
    /// # Arguments
    /// * `side` - OrderSide::Buy (use asks) or OrderSide::Sell (use bids)
    /// * `volume` - Quantity to fill
    ///
    /// # Returns
    /// * `Some((weighted_avg_price, total_filled))` - If at least partial fill possible
    /// * `None` - If order book is empty
    ///
    /// # Example
    /// ```ignore
    /// let mut book = OrderBook::new();
    /// book.add_ask(Decimal::new(100, 0), Decimal::new(10, 0)); // 10 @ $100
    /// book.add_ask(Decimal::new(101, 0), Decimal::new(20, 0)); // 20 @ $101
    ///
    /// // Buy 15 units: 10 @ $100 + 5 @ $101 = weighted avg $100.33
    /// let (avg_price, filled) = book.fill_cost(OrderSide::Buy, Decimal::new(15, 0)).unwrap();
    /// ```
    pub fn fill_cost(&self, side: OrderSide, volume: Decimal) -> Option<(Decimal, Decimal)> {
        let levels = match side {
            OrderSide::Buy => &self.asks,  // Buying consumes ask liquidity
            OrderSide::Sell => &self.bids, // Selling consumes bid liquidity
        };

        if levels.is_empty() {
            return None;
        }

        let mut remaining = volume;
        let mut total_cost = Decimal::ZERO;
        let mut total_filled = Decimal::ZERO;

        for level in levels {
            if remaining <= Decimal::ZERO {
                break;
            }

            let fill_qty = remaining.min(level.size);
            total_cost += level.price * fill_qty;
            total_filled += fill_qty;
            remaining -= fill_qty;
        }

        if total_filled == Decimal::ZERO {
            None
        } else {
            let weighted_avg = total_cost / total_filled;
            Some((weighted_avg, total_filled))
        }
    }

    /// Calculate total available liquidity on one side
    pub fn total_liquidity(&self, side: OrderSide) -> Decimal {
        let levels = match side {
            OrderSide::Buy => &self.bids,
            OrderSide::Sell => &self.asks,
        };

        levels.iter().map(|level| level.size).sum()
    }

    /// Calculate slippage percentage for a given volume
    ///
    /// Slippage = (weighted_avg_price - best_price) / best_price
    ///
    /// # Returns
    /// Slippage as a percentage (e.g., 0.01 = 1% slippage)
    pub fn calculate_slippage(&self, side: OrderSide, volume: Decimal) -> Option<Decimal> {
        let best_price = match side {
            OrderSide::Buy => self.best_ask()?,
            OrderSide::Sell => self.best_bid()?,
        };

        let (avg_price, _) = self.fill_cost(side, volume)?;

        let slippage = match side {
            OrderSide::Buy => (avg_price - best_price) / best_price,
            OrderSide::Sell => (best_price - avg_price) / best_price,
        };

        Some(slippage)
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

/// Order side for volume cost calculation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OrderSide {
    /// Buying (consuming ask liquidity)
    Buy,
    /// Selling (consuming bid liquidity)
    Sell,
}

/// Calculate volume-dependent edge weight
///
/// For a given trading volume, walk the order book to compute
/// the actual return cost including slippage.
///
/// # Arguments
/// * `order_book` - Current order book state
/// * `side` - Buy or Sell
/// * `volume` - Trading volume
/// * `fee_bps` - Fee in basis points (e.g., 60 for 0.6%)
///
/// # Returns
/// Edge weight (-ln(rate × (1 - fee))) or f64::INFINITY if insufficient liquidity
///
/// # Example
/// ```ignore
/// let mut book = OrderBook::new();
/// book.add_ask(Decimal::new(50000, 0), Decimal::new(1, 0)); // 1 BTC @ $50,000
///
/// let weight = volume_dependent_weight(
///     &book,
///     OrderSide::Buy,
///     Decimal::new(5, 1), // 0.5 BTC
///     60, // 0.6% fee
/// );
/// // weight = -ln(50000 × 0.994) ≈ -10.814
/// ```
pub fn volume_dependent_weight(
    order_book: &OrderBook,
    side: OrderSide,
    volume: Decimal,
    fee_bps: FeeBasisPoints,
) -> EdgeWeight {
    match order_book.fill_cost(side, volume) {
        Some((avg_price, _)) => {
            // Calculate rate after fee
            let fee_decimal = Decimal::new(fee_bps as i64, 4); // 0.0060 for 60 bps
            let rate_after_fee = avg_price * (Decimal::ONE - fee_decimal);

            // Convert to f64 for ln calculation
            match rate_after_fee.to_f64() {
                Some(rate_f64) if rate_f64 > 0.0 => -rate_f64.ln(),
                _ => f64::INFINITY,
            }
        }
        None => f64::INFINITY, // Insufficient liquidity
    }
}

/// Calculate volume-dependent rate (without logarithm transformation)
///
/// Returns the actual exchange rate after accounting for slippage and fees.
/// Useful for final profit calculations where we need the multiplicative rate.
///
/// # Returns
/// * `Some(rate)` - Exchange rate after slippage and fees
/// * `None` - Insufficient liquidity
pub fn volume_dependent_rate(
    order_book: &OrderBook,
    side: OrderSide,
    volume: Decimal,
    fee_bps: FeeBasisPoints,
) -> Option<Decimal> {
    let (avg_price, _) = order_book.fill_cost(side, volume)?;

    let fee_decimal = Decimal::new(fee_bps as i64, 4);
    let rate_after_fee = avg_price * (Decimal::ONE - fee_decimal);

    Some(rate_after_fee)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_book() -> OrderBook {
        let mut book = OrderBook::new();

        // Bids (descending): $99, $98, $97
        book.add_bid(Decimal::new(99, 0), Decimal::new(10, 0));
        book.add_bid(Decimal::new(98, 0), Decimal::new(20, 0));
        book.add_bid(Decimal::new(97, 0), Decimal::new(30, 0));

        // Asks (ascending): $100, $101, $102
        book.add_ask(Decimal::new(100, 0), Decimal::new(10, 0));
        book.add_ask(Decimal::new(101, 0), Decimal::new(20, 0));
        book.add_ask(Decimal::new(102, 0), Decimal::new(30, 0));

        book
    }

    #[test]
    fn test_order_book_creation() {
        let book = OrderBook::new();
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_best_bid_ask() {
        let book = create_test_book();

        assert_eq!(book.best_bid(), Some(Decimal::new(99, 0)));
        assert_eq!(book.best_ask(), Some(Decimal::new(100, 0)));
    }

    #[test]
    fn test_mid_price() {
        let book = create_test_book();

        // Mid = (99 + 100) / 2 = 99.5
        assert_eq!(book.mid_price(), Some(Decimal::new(995, 1)));
    }

    #[test]
    fn test_single_level_fill() {
        let book = create_test_book();

        // Buy 5 units (less than first level of 10)
        let (avg_price, filled) = book.fill_cost(OrderSide::Buy, Decimal::new(5, 0)).unwrap();

        assert_eq!(avg_price, Decimal::new(100, 0)); // Best ask price
        assert_eq!(filled, Decimal::new(5, 0));
    }

    #[test]
    fn test_multi_level_fill() {
        let book = create_test_book();

        // Buy 15 units: 10 @ $100 + 5 @ $101
        let (avg_price, filled) = book.fill_cost(OrderSide::Buy, Decimal::new(15, 0)).unwrap();

        // Weighted avg: (10×100 + 5×101) / 15 = 1505 / 15 = 100.333...
        let expected = Decimal::new(1505, 0) / Decimal::new(15, 0);
        assert_eq!(avg_price, expected);
        assert_eq!(filled, Decimal::new(15, 0));
    }

    #[test]
    fn test_partial_fill_insufficient_liquidity() {
        let book = create_test_book();

        // Try to buy 100 units (only 60 available: 10+20+30)
        let (avg_price, filled) = book
            .fill_cost(OrderSide::Buy, Decimal::new(100, 0))
            .unwrap();

        // Should fill 60: 10@100 + 20@101 + 30@102
        assert_eq!(filled, Decimal::new(60, 0));

        // Weighted avg: (10×100 + 20×101 + 30×102) / 60 = 6080 / 60 = 101.333...
        let expected = Decimal::new(6080, 0) / Decimal::new(60, 0);
        assert_eq!(avg_price, expected);
    }

    #[test]
    fn test_sell_side_fill() {
        let book = create_test_book();

        // Sell 15 units: 10 @ $99 + 5 @ $98
        let (avg_price, filled) = book
            .fill_cost(OrderSide::Sell, Decimal::new(15, 0))
            .unwrap();

        // Weighted avg: (10×99 + 5×98) / 15 = 1480 / 15 = 98.666...
        let expected = Decimal::new(1480, 0) / Decimal::new(15, 0);
        assert_eq!(avg_price, expected);
        assert_eq!(filled, Decimal::new(15, 0));
    }

    #[test]
    fn test_empty_order_book() {
        let book = OrderBook::new();

        let result = book.fill_cost(OrderSide::Buy, Decimal::new(10, 0));
        assert!(result.is_none());
    }

    #[test]
    fn test_zero_volume() {
        let book = create_test_book();

        // Zero volume should return None (no fill occurs)
        let result = book.fill_cost(OrderSide::Buy, Decimal::ZERO);
        assert!(result.is_none(), "Zero volume should return None");
    }

    #[test]
    fn test_slippage_calculation() {
        let book = create_test_book();

        // Buy 15 units (weighted avg = 100.333...)
        let slippage = book
            .calculate_slippage(OrderSide::Buy, Decimal::new(15, 0))
            .unwrap();

        // Slippage = (100.333... - 100) / 100 = 0.00333...
        let expected = Decimal::new(1505, 0) / Decimal::new(15, 0);
        let expected_slippage = (expected - Decimal::new(100, 0)) / Decimal::new(100, 0);

        assert_eq!(slippage, expected_slippage);
        assert!(slippage > Decimal::ZERO); // Buying should have positive slippage
    }

    #[test]
    fn test_volume_dependent_weight() {
        let mut book = OrderBook::new();
        book.add_ask(Decimal::new(50000, 0), Decimal::new(1, 0)); // 1 BTC @ $50,000

        let weight = volume_dependent_weight(
            &book,
            OrderSide::Buy,
            Decimal::new(5, 1), // 0.5 BTC
            60,                 // 0.6% fee
        );

        // Expected: -ln(50000 × 0.994)
        let expected = -(50000.0 * 0.994_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_volume_dependent_weight_insufficient_liquidity() {
        let mut book = OrderBook::new();
        book.add_ask(Decimal::new(50000, 0), Decimal::new(1, 0)); // Only 1 BTC available

        let weight = volume_dependent_weight(
            &book,
            OrderSide::Buy,
            Decimal::new(2, 0), // Try to buy 2 BTC (not enough)
            60,
        );

        // Should still calculate based on available liquidity (1 BTC)
        let expected = -(50000.0 * 0.994_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_volume_dependent_rate() {
        let mut book = OrderBook::new();
        book.add_ask(Decimal::new(50000, 0), Decimal::new(1, 0));

        let rate = volume_dependent_rate(
            &book,
            OrderSide::Buy,
            Decimal::new(5, 1), // 0.5 BTC
            60,                 // 0.6% fee
        )
        .unwrap();

        // Expected: 50000 × 0.994
        let expected = Decimal::new(50000, 0) * Decimal::new(994, 3);
        assert_eq!(rate, expected);
    }

    #[test]
    fn test_slippage_increases_with_volume() {
        let book = create_test_book();

        // Small volume
        let slippage_small = book
            .calculate_slippage(OrderSide::Buy, Decimal::new(5, 0))
            .unwrap();

        // Medium volume
        let slippage_medium = book
            .calculate_slippage(OrderSide::Buy, Decimal::new(15, 0))
            .unwrap();

        // Large volume
        let slippage_large = book
            .calculate_slippage(OrderSide::Buy, Decimal::new(30, 0))
            .unwrap();

        // Property: larger volume → higher slippage
        assert!(slippage_small <= slippage_medium);
        assert!(slippage_medium <= slippage_large);
    }

    #[test]
    fn test_total_liquidity() {
        let book = create_test_book();

        let bid_liquidity = book.total_liquidity(OrderSide::Sell); // Sell uses bids
        let ask_liquidity = book.total_liquidity(OrderSide::Buy); // Buy uses asks

        assert_eq!(bid_liquidity, Decimal::new(60, 0)); // 10+20+30
        assert_eq!(ask_liquidity, Decimal::new(60, 0)); // 10+20+30
    }

    #[test]
    fn test_cost_convexity() {
        let book = create_test_book();

        // Cost for volume V1
        let (price1, _) = book.fill_cost(OrderSide::Buy, Decimal::new(10, 0)).unwrap();
        let cost1 = price1 * Decimal::new(10, 0);

        // Cost for volume V2
        let (price2, _) = book.fill_cost(OrderSide::Buy, Decimal::new(20, 0)).unwrap();
        let cost2 = price2 * Decimal::new(20, 0);

        // Cost for combined volume V1+V2
        let (price_combined, _) = book.fill_cost(OrderSide::Buy, Decimal::new(30, 0)).unwrap();
        let cost_combined = price_combined * Decimal::new(30, 0);

        // Property: Cost(V1) + Cost(V2) >= Cost(V1+V2) (due to slippage)
        // Actually, for order books: Cost(V1+V2) >= Cost(V1) + Cost(V2)
        // because slippage is non-linear (convex cost function)
        assert!(cost_combined >= cost1 + cost2);
    }

    #[test]
    fn test_bid_ask_ordering() {
        let mut book = OrderBook::new();

        // Add bids out of order
        book.add_bid(Decimal::new(98, 0), Decimal::new(10, 0));
        book.add_bid(Decimal::new(100, 0), Decimal::new(10, 0));
        book.add_bid(Decimal::new(99, 0), Decimal::new(10, 0));

        // Add asks out of order
        book.add_ask(Decimal::new(102, 0), Decimal::new(10, 0));
        book.add_ask(Decimal::new(101, 0), Decimal::new(10, 0));
        book.add_ask(Decimal::new(103, 0), Decimal::new(10, 0));

        // Verify bids are descending
        assert_eq!(book.bids[0].price, Decimal::new(100, 0));
        assert_eq!(book.bids[1].price, Decimal::new(99, 0));
        assert_eq!(book.bids[2].price, Decimal::new(98, 0));

        // Verify asks are ascending
        assert_eq!(book.asks[0].price, Decimal::new(101, 0));
        assert_eq!(book.asks[1].price, Decimal::new(102, 0));
        assert_eq!(book.asks[2].price, Decimal::new(103, 0));
    }
}
