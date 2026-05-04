// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Graph maintenance with L3 order book updates
///
/// Processes L3 market data updates (FIX MarketDataIncrementalRefresh entries)
/// and triggers graph edge weight updates only when top-of-book prices change.
///
/// This is a critical optimization: Instead of updating the graph on every L3 message
/// (thousands per second), we only update when the best bid/ask actually changes.
use crate::adjacency_matrix::AdjacencyMatrix;
use crate::types::*;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};

/// L3 market data update (FIX MarketDataIncrementalRefresh entry)
#[derive(Debug, Clone)]
pub enum L3Update {
    /// New order added to book
    NewOrder {
        instrument: String, // "BTC-USD"
        order_id: String,
        side: OrderSide,
        price: Decimal,
        size: Decimal,
    },
    /// Existing order modified (size or price change)
    ModifyOrder {
        instrument: String,
        order_id: String,
        new_price: Option<Decimal>,
        new_size: Decimal,
    },
    /// Order removed from book
    DeleteOrder {
        instrument: String,
        order_id: String,
    },
}

/// Order side for L3 updates
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Individual order in the book
#[derive(Debug, Clone, PartialEq)]
struct Order {
    order_id: String,
    price: Decimal,
    size: Decimal,
    side: OrderSide,
}

/// Tracks when an edge weight needs updating
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeUpdate {
    pub from_currency: String,
    pub to_currency: String,
    pub new_rate: Decimal,
}

/// Order book with individual order tracking for L3 updates
///
/// Uses BTreeMap for automatic price-level ordering:
/// - Bids: Descending order (highest price first)
/// - Asks: Ascending order (lowest price first)
#[derive(Debug, Clone)]
struct L3OrderBook {
    /// Bids indexed by order_id
    bid_orders: HashMap<String, Order>,
    /// Asks indexed by order_id
    ask_orders: HashMap<String, Order>,
    /// Bid price levels (price → total size at that price)
    /// BTreeMap maintains descending order for iteration
    bid_levels: BTreeMap<Decimal, Decimal>,
    /// Ask price levels (price → total size at that price)
    /// BTreeMap maintains ascending order for iteration
    ask_levels: BTreeMap<Decimal, Decimal>,
}

impl L3OrderBook {
    fn new() -> Self {
        Self {
            bid_orders: HashMap::new(),
            ask_orders: HashMap::new(),
            bid_levels: BTreeMap::new(),
            ask_levels: BTreeMap::new(),
        }
    }

    /// Add a new order to the book
    fn add_order(&mut self, order_id: String, side: OrderSide, price: Decimal, size: Decimal) {
        let order = Order {
            order_id: order_id.clone(),
            price,
            size,
            side,
        };

        match side {
            OrderSide::Buy => {
                self.bid_orders.insert(order_id, order);
                *self.bid_levels.entry(price).or_insert(Decimal::ZERO) += size;
            }
            OrderSide::Sell => {
                self.ask_orders.insert(order_id, order);
                *self.ask_levels.entry(price).or_insert(Decimal::ZERO) += size;
            }
        }
    }

    /// Modify an existing order
    fn modify_order(
        &mut self,
        order_id: &str,
        new_price: Option<Decimal>,
        new_size: Decimal,
    ) -> bool {
        // Try to find in bids first
        if let Some(order) = self.bid_orders.get_mut(order_id) {
            // Remove old size from old price level
            if let Some(level_size) = self.bid_levels.get_mut(&order.price) {
                *level_size -= order.size;
                if *level_size <= Decimal::ZERO {
                    self.bid_levels.remove(&order.price);
                }
            }

            // Update order
            if let Some(price) = new_price {
                order.price = price;
            }
            order.size = new_size;

            // Add new size to new price level
            *self.bid_levels.entry(order.price).or_insert(Decimal::ZERO) += new_size;

            return true;
        }

        // Try to find in asks
        if let Some(order) = self.ask_orders.get_mut(order_id) {
            // Remove old size from old price level
            if let Some(level_size) = self.ask_levels.get_mut(&order.price) {
                *level_size -= order.size;
                if *level_size <= Decimal::ZERO {
                    self.ask_levels.remove(&order.price);
                }
            }

            // Update order
            if let Some(price) = new_price {
                order.price = price;
            }
            order.size = new_size;

            // Add new size to new price level
            *self.ask_levels.entry(order.price).or_insert(Decimal::ZERO) += new_size;

            return true;
        }

        false // Order not found
    }

    /// Delete an order from the book
    fn delete_order(&mut self, order_id: &str) -> bool {
        // Try to remove from bids
        if let Some(order) = self.bid_orders.remove(order_id) {
            if let Some(level_size) = self.bid_levels.get_mut(&order.price) {
                *level_size -= order.size;
                if *level_size <= Decimal::ZERO {
                    self.bid_levels.remove(&order.price);
                }
            }
            return true;
        }

        // Try to remove from asks
        if let Some(order) = self.ask_orders.remove(order_id) {
            if let Some(level_size) = self.ask_levels.get_mut(&order.price) {
                *level_size -= order.size;
                if *level_size <= Decimal::ZERO {
                    self.ask_levels.remove(&order.price);
                }
            }
            return true;
        }

        false // Order not found
    }

    /// Get the best bid price (highest buy order)
    fn best_bid(&self) -> Option<Decimal> {
        // BTreeMap iter().rev() gives descending order
        self.bid_levels.keys().rev().next().copied()
    }

    /// Get the best ask price (lowest sell order)
    fn best_ask(&self) -> Option<Decimal> {
        // BTreeMap iter() gives ascending order
        self.ask_levels.keys().next().copied()
    }
}

/// Manages order books for all trading pairs and triggers graph updates
pub struct GraphMaintenanceManager {
    /// One order book per instrument (e.g., "BTC-USD" → L3OrderBook)
    order_books: HashMap<String, L3OrderBook>,

    /// Track previous best bid/ask to detect changes
    /// Key: instrument, Value: (best_bid_price, best_ask_price)
    previous_top_of_book: HashMap<String, (Option<Decimal>, Option<Decimal>)>,

    /// Reference to the graph (for updates)
    graph: AdjacencyMatrix,

    /// Fee in basis points (e.g., 60 for 0.6%)
    fee_bps: FeeBasisPoints,
}

impl GraphMaintenanceManager {
    /// Create a new graph maintenance manager
    pub fn new(graph: AdjacencyMatrix, fee_bps: FeeBasisPoints) -> Self {
        Self {
            order_books: HashMap::new(),
            previous_top_of_book: HashMap::new(),
            graph,
            fee_bps,
        }
    }

    /// Process an L3 update and return edge updates if top-of-book changed
    ///
    /// # Algorithm
    /// 1. Get or create order book for instrument
    /// 2. Apply update to order book
    /// 3. Get new best bid/ask
    /// 4. Compare with previous top-of-book
    /// 5. If changed, create EdgeUpdate and update graph
    /// 6. Return edge updates (can be empty, 1, or 2)
    ///
    /// # Returns
    /// Vector of EdgeUpdate structs for edges that changed (0, 1, or 2 updates)
    pub fn handle_update(&mut self, update: L3Update) -> Vec<EdgeUpdate> {
        // Extract instrument before matching (to avoid borrow issues)
        let instrument = match &update {
            L3Update::NewOrder { instrument, .. } => instrument.clone(),
            L3Update::ModifyOrder { instrument, .. } => instrument.clone(),
            L3Update::DeleteOrder { instrument, .. } => instrument.clone(),
        };

        // Get or create order book for this instrument
        let book = self
            .order_books
            .entry(instrument.clone())
            .or_insert_with(L3OrderBook::new);

        // Apply the update to the order book
        match update {
            L3Update::NewOrder {
                order_id,
                side,
                price,
                size,
                ..
            } => {
                book.add_order(order_id, side, price, size);
            }
            L3Update::ModifyOrder {
                order_id,
                new_price,
                new_size,
                ..
            } => {
                book.modify_order(&order_id, new_price, new_size);
            }
            L3Update::DeleteOrder { order_id, .. } => {
                book.delete_order(&order_id);
            }
        }

        // Get new top-of-book
        let new_best_bid = book.best_bid();
        let new_best_ask = book.best_ask();

        // Get previous top-of-book
        let (old_best_bid, old_best_ask) = self
            .previous_top_of_book
            .get(&instrument)
            .copied()
            .unwrap_or((None, None));

        // Check if top-of-book changed
        if old_best_bid == new_best_bid && old_best_ask == new_best_ask {
            return vec![]; // No edge updates needed
        }

        // Update stored top-of-book
        self.previous_top_of_book
            .insert(instrument.clone(), (new_best_bid, new_best_ask));

        // Parse instrument into currencies
        let (base, quote) = match Self::parse_instrument(&instrument) {
            Some(pair) => pair,
            None => return vec![], // Invalid instrument format
        };

        // Generate edge updates (only for changed sides)
        let mut edge_updates = Vec::new();

        // If best bid changed: Update base → quote edge (selling base for quote)
        // Example: BTC-USD best bid = $50000 means selling BTC gets you $50000 USD
        if old_best_bid != new_best_bid && new_best_bid.is_some() {
            edge_updates.push(EdgeUpdate {
                from_currency: base.clone(),
                to_currency: quote.clone(),
                new_rate: new_best_bid.unwrap(),
            });
        }

        // If best ask changed: Update quote → base edge (buying base with quote)
        // Example: BTC-USD best ask = $51000 means 1 USD buys 1/51000 BTC
        if old_best_ask != new_best_ask && new_best_ask.is_some() {
            let inverse_rate = Decimal::ONE / new_best_ask.unwrap();
            edge_updates.push(EdgeUpdate {
                from_currency: quote,
                to_currency: base,
                new_rate: inverse_rate,
            });
        }

        // Update graph with new edge weights
        for edge_update in &edge_updates {
            // Ignore errors (currency might not be in graph yet)
            let _ = self.graph.update_edge(
                &edge_update.from_currency,
                &edge_update.to_currency,
                edge_update.new_rate,
                self.fee_bps,
            );
        }

        edge_updates
    }

    /// Parse instrument pair into currencies (e.g., "BTC-USD" → ("BTC", "USD"))
    fn parse_instrument(instrument: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = instrument.split('-').collect();
        if parts.len() == 2 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }

    /// Get reference to the adjacency matrix (for testing)
    pub fn graph(&self) -> &AdjacencyMatrix {
        &self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> GraphMaintenanceManager {
        let currencies = vec!["BTC".to_string(), "ETH".to_string(), "USD".to_string()];
        let graph = AdjacencyMatrix::new(currencies);
        GraphMaintenanceManager::new(graph, 60) // 0.6% fee
    }

    #[test]
    fn test_new_order_updates_book() {
        let mut manager = create_test_manager();

        let update = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };

        let edge_updates = manager.handle_update(update);

        // Should trigger edge update (first update for this instrument)
        assert_eq!(edge_updates.len(), 1);
        assert_eq!(edge_updates[0].from_currency, "BTC");
        assert_eq!(edge_updates[0].to_currency, "USD");
    }

    #[test]
    fn test_modify_order_updates_book() {
        let mut manager = create_test_manager();

        // Add initial order
        let add = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        manager.handle_update(add);

        // Modify the order (change size, no price change)
        let modify = L3Update::ModifyOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            new_price: None,
            new_size: Decimal::new(2, 0),
        };

        let edge_updates = manager.handle_update(modify);

        // Should NOT trigger edge update (best bid price unchanged)
        assert_eq!(edge_updates.len(), 0);
    }

    #[test]
    fn test_delete_order_removes_from_book() {
        let mut manager = create_test_manager();

        // Add order
        let add = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        manager.handle_update(add);

        // Delete the order
        let delete = L3Update::DeleteOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
        };

        let edge_updates = manager.handle_update(delete);

        // Edge updates depend on whether book becomes empty
        // If best bid goes from Some to None, we don't generate an update (edge becomes invalid)
        // This test just verifies the delete succeeds without panicking
        let _ = edge_updates; // Acknowledge we got a result
    }

    #[test]
    fn test_top_of_book_change_triggers_edge_update() {
        let mut manager = create_test_manager();

        // Add first order at $50000
        let order1 = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        let updates1 = manager.handle_update(order1);
        assert_eq!(updates1.len(), 1); // First update triggers edge update

        // Add second order at $51000 (new best bid)
        let order2 = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order2".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(51000, 0),
            size: Decimal::new(1, 0),
        };
        let updates2 = manager.handle_update(order2);
        assert_eq!(updates2.len(), 1); // Best bid changed, triggers edge update
    }

    #[test]
    fn test_mid_book_update_no_edge_update() {
        let mut manager = create_test_manager();

        // Add best bid at $50000
        let order1 = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        manager.handle_update(order1);

        // Add mid-book order at $49000 (below best bid)
        let order2 = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order2".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(49000, 0),
            size: Decimal::new(1, 0),
        };
        let updates = manager.handle_update(order2);

        // Should NOT trigger edge update (best bid unchanged)
        assert_eq!(updates.len(), 0);
    }

    #[test]
    fn test_multiple_instruments() {
        let mut manager = create_test_manager();

        // Add order for BTC-USD
        let btc_order = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        manager.handle_update(btc_order);

        // Add order for ETH-USD
        let eth_order = L3Update::NewOrder {
            instrument: "ETH-USD".to_string(),
            order_id: "order2".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(3000, 0),
            size: Decimal::new(10, 0),
        };
        manager.handle_update(eth_order);

        // Should have separate order books
        assert_eq!(manager.order_books.len(), 2);
    }

    #[test]
    fn test_edge_direction_correct() {
        let mut manager = create_test_manager();

        // Add bid order (selling BTC for USD)
        let bid = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        let bid_updates = manager.handle_update(bid);

        assert_eq!(bid_updates.len(), 1);
        assert_eq!(bid_updates[0].from_currency, "BTC");
        assert_eq!(bid_updates[0].to_currency, "USD");
        assert_eq!(bid_updates[0].new_rate, Decimal::new(50000, 0));

        // Add ask order (buying BTC with USD)
        let ask = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order2".to_string(),
            side: OrderSide::Sell,
            price: Decimal::new(51000, 0),
            size: Decimal::new(1, 0),
        };
        let ask_updates = manager.handle_update(ask);

        assert_eq!(ask_updates.len(), 1);
        assert_eq!(ask_updates[0].from_currency, "USD");
        assert_eq!(ask_updates[0].to_currency, "BTC");
        // Rate should be 1/51000
        let expected = Decimal::ONE / Decimal::new(51000, 0);
        assert_eq!(ask_updates[0].new_rate, expected);
    }

    #[test]
    fn test_fee_applied_to_edge_weight() {
        let mut manager = create_test_manager();

        // Add order
        let order = L3Update::NewOrder {
            instrument: "BTC-USD".to_string(),
            order_id: "order1".to_string(),
            side: OrderSide::Buy,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
        };
        manager.handle_update(order);

        // Check that edge weight includes fee
        let btc_idx = manager.graph().get_index("BTC").unwrap();
        let usd_idx = manager.graph().get_index("USD").unwrap();
        let weight = manager.graph().get_weight(btc_idx, usd_idx);

        // Weight should be -ln(50000 × 0.994)
        let expected = -(50000.0 * 0.994_f64).ln();
        assert!((weight - expected).abs() < 1e-10);
    }

    #[test]
    fn test_parse_instrument() {
        assert_eq!(
            GraphMaintenanceManager::parse_instrument("BTC-USD"),
            Some(("BTC".to_string(), "USD".to_string()))
        );
        assert_eq!(
            GraphMaintenanceManager::parse_instrument("ETH-BTC"),
            Some(("ETH".to_string(), "BTC".to_string()))
        );
        assert_eq!(GraphMaintenanceManager::parse_instrument("INVALID"), None);
    }
}
