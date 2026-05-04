// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

/// Topology management for trading pair suspension and relisting
///
/// Handles exchange-wide events like pair suspension (during volatility) and
/// relisting. Maintains graph connectivity and detects when currencies become
/// unreachable from USD (base currency).
///
/// Critical for risk management: If a currency becomes disconnected from USD,
/// any capital in that currency cannot return home.
use crate::adjacency_matrix::AdjacencyMatrix;
use crate::types::*;
use rust_decimal::Decimal;
use std::collections::{HashSet, VecDeque};

/// Topology event (FIX SecurityStatus messages)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyEvent {
    /// Trading pair suspended (no trading allowed)
    PairSuspended {
        instrument: String, // e.g., "BTC-USD"
    },
    /// Trading pair relisted (trading resumed)
    PairRelisted {
        instrument: String,
        best_bid: Option<Decimal>,
        best_ask: Option<Decimal>,
    },
}

/// Topology manager - handles pair suspension/relisting and connectivity
pub struct TopologyManager {
    /// Reference to graph
    graph: AdjacencyMatrix,

    /// Track suspended pairs
    suspended_pairs: HashSet<String>,

    /// Fee in basis points
    fee_bps: FeeBasisPoints,
}

impl TopologyManager {
    /// Create a new topology manager
    pub fn new(graph: AdjacencyMatrix, fee_bps: FeeBasisPoints) -> Self {
        Self {
            graph,
            suspended_pairs: HashSet::new(),
            fee_bps,
        }
    }

    /// Process a topology event (suspension or relisting)
    ///
    /// # Returns
    /// List of currencies that became disconnected from USD after this event.
    /// Empty vector if no currencies were disconnected.
    ///
    /// # Algorithm
    /// - For PairSuspended: Remove both edges (base→quote, quote→base)
    /// - For PairRelisted: Restore edges with given rates
    /// - After modification, check for disconnected currencies
    pub fn handle_event(&mut self, event: TopologyEvent) -> Vec<String> {
        match event {
            TopologyEvent::PairSuspended { instrument } => {
                // Parse instrument into currencies
                let (base, quote) = match Self::parse_instrument(&instrument) {
                    Some(pair) => pair,
                    None => return vec![], // Invalid format
                };

                // Remove both edges
                let _ = self.graph.remove_edge(&base, &quote);
                let _ = self.graph.remove_edge(&quote, &base);

                // Mark as suspended
                self.suspended_pairs.insert(instrument);

                // Check for disconnections
                self.get_disconnected_from_usd()
            }
            TopologyEvent::PairRelisted {
                instrument,
                best_bid,
                best_ask,
            } => {
                // Parse instrument into currencies
                let (base, quote) = match Self::parse_instrument(&instrument) {
                    Some(pair) => pair,
                    None => return vec![], // Invalid format
                };

                // Restore edges with given rates
                if let Some(bid_rate) = best_bid {
                    // base → quote edge (selling base for quote)
                    let _ = self
                        .graph
                        .update_edge(&base, &quote, bid_rate, self.fee_bps);
                }

                if let Some(ask_rate) = best_ask {
                    // quote → base edge (buying base with quote)
                    let inverse_rate = Decimal::ONE / ask_rate;
                    let _ = self
                        .graph
                        .update_edge(&quote, &base, inverse_rate, self.fee_bps);
                }

                // Mark as no longer suspended
                self.suspended_pairs.remove(&instrument);

                // Check for any remaining disconnections
                self.get_disconnected_from_usd()
            }
        }
    }

    /// Check if a path exists from one currency to another
    ///
    /// Uses breadth-first search (BFS) to determine reachability.
    ///
    /// # Returns
    /// - `true` if path exists
    /// - `false` if no path or currency not found
    pub fn is_reachable(&self, from: &str, to: &str) -> bool {
        // Get indices
        let from_idx = match self.graph.get_index(from) {
            Some(idx) => idx,
            None => return false,
        };

        let to_idx = match self.graph.get_index(to) {
            Some(idx) => idx,
            None => return false,
        };

        // Trivial case: same currency
        if from_idx == to_idx {
            return true;
        }

        // BFS
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from_idx);
        visited.insert(from_idx);

        while let Some(node) = queue.pop_front() {
            if node == to_idx {
                return true;
            }

            for neighbor in self.graph.neighbors(node) {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        false
    }

    /// Find all currencies unreachable from USD
    ///
    /// Critical for risk management: Any currency unreachable from USD means
    /// capital stuck in that currency cannot return to the base currency.
    ///
    /// # Returns
    /// Vector of currency symbols that cannot reach USD
    pub fn get_disconnected_from_usd(&self) -> Vec<String> {
        let usd_idx = match self.graph.get_index("USD") {
            Some(idx) => idx,
            None => return vec![], // USD not in graph
        };

        // BFS from USD to find all reachable nodes
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(usd_idx);
        visited.insert(usd_idx);

        while let Some(node) = queue.pop_front() {
            for neighbor in self.graph.neighbors(node) {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        // Find currencies not visited
        let mut disconnected = Vec::new();
        for i in 0..self.graph.size() {
            if !visited.contains(&i) {
                if let Some(symbol) = self.graph.get_symbol(i) {
                    disconnected.push(symbol.to_string());
                }
            }
        }

        disconnected
    }

    /// Check if an instrument is currently suspended
    pub fn is_suspended(&self, instrument: &str) -> bool {
        self.suspended_pairs.contains(instrument)
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

    /// Get count of suspended pairs
    pub fn suspended_count(&self) -> usize {
        self.suspended_pairs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> TopologyManager {
        // Create a simple graph: USD, BTC, ETH
        let currencies = vec!["USD".to_string(), "BTC".to_string(), "ETH".to_string()];
        let mut graph = AdjacencyMatrix::new(currencies);

        // Add initial edges:
        // USD ↔ BTC
        graph
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 1 USD = 0.00002 BTC
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap(); // 1 BTC = 50000 USD

        // USD ↔ ETH
        graph
            .update_edge("USD", "ETH", Decimal::new(33, 5), 60)
            .unwrap(); // 1 USD = 0.00033 ETH
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 60)
            .unwrap(); // 1 ETH = 3000 USD

        // BTC ↔ ETH
        graph
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap(); // 1 BTC = 20 ETH
        graph
            .update_edge("ETH", "BTC", Decimal::new(5, 2), 60)
            .unwrap(); // 1 ETH = 0.05 BTC

        TopologyManager::new(graph, 60)
    }

    #[test]
    fn test_suspend_pair_removes_edges() {
        let mut manager = create_test_manager();

        // Verify edges exist initially
        let btc_idx = manager.graph().get_index("BTC").unwrap();
        let usd_idx = manager.graph().get_index("USD").unwrap();
        assert!(manager.graph().has_edge(btc_idx, usd_idx));
        assert!(manager.graph().has_edge(usd_idx, btc_idx));

        // Suspend BTC-USD
        let event = TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        };
        manager.handle_event(event);

        // Verify edges removed
        assert!(!manager.graph().has_edge(btc_idx, usd_idx));
        assert!(!manager.graph().has_edge(usd_idx, btc_idx));
    }

    #[test]
    fn test_relist_pair_restores_edges() {
        let mut manager = create_test_manager();

        // Suspend BTC-USD
        let suspend = TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        };
        manager.handle_event(suspend);

        // Verify edges removed
        let btc_idx = manager.graph().get_index("BTC").unwrap();
        let usd_idx = manager.graph().get_index("USD").unwrap();
        assert!(!manager.graph().has_edge(btc_idx, usd_idx));

        // Relist BTC-USD
        let relist = TopologyEvent::PairRelisted {
            instrument: "BTC-USD".to_string(),
            best_bid: Some(Decimal::new(51000, 0)),
            best_ask: Some(Decimal::new(52000, 0)),
        };
        manager.handle_event(relist);

        // Verify edges restored
        assert!(manager.graph().has_edge(btc_idx, usd_idx));
        assert!(manager.graph().has_edge(usd_idx, btc_idx));

        // Verify rates are correct
        let rate_btc_to_usd = manager.graph().get_rate(btc_idx, usd_idx);
        assert_eq!(rate_btc_to_usd, Decimal::new(51000, 0));

        let rate_usd_to_btc = manager.graph().get_rate(usd_idx, btc_idx);
        let expected_inverse = Decimal::ONE / Decimal::new(52000, 0);
        assert_eq!(rate_usd_to_btc, expected_inverse);
    }

    #[test]
    fn test_suspension_detected() {
        let mut manager = create_test_manager();

        assert!(!manager.is_suspended("BTC-USD"));

        // Suspend
        let event = TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        };
        manager.handle_event(event);

        assert!(manager.is_suspended("BTC-USD"));
        assert_eq!(manager.suspended_count(), 1);
    }

    #[test]
    fn test_disconnection_detection() {
        let mut manager = create_test_manager();

        // Initially all connected to USD
        assert!(manager.is_reachable("BTC", "USD"));
        assert!(manager.is_reachable("ETH", "USD"));
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 0);

        // Suspend both BTC-USD and BTC-ETH (BTC becomes isolated)
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        });
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-ETH".to_string(),
        });

        // BTC should now be disconnected from USD
        assert!(!manager.is_reachable("BTC", "USD"));
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 1);
        assert!(disconnected.contains(&"BTC".to_string()));

        // ETH should still be connected
        assert!(manager.is_reachable("ETH", "USD"));
    }

    #[test]
    fn test_multiple_suspensions() {
        let mut manager = create_test_manager();

        // Suspend all pairs
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        });
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "ETH-USD".to_string(),
        });
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-ETH".to_string(),
        });

        // All should be suspended
        assert!(manager.is_suspended("BTC-USD"));
        assert!(manager.is_suspended("ETH-USD"));
        assert!(manager.is_suspended("BTC-ETH"));
        assert_eq!(manager.suspended_count(), 3);

        // Both BTC and ETH should be disconnected from USD
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 2);
        assert!(disconnected.contains(&"BTC".to_string()));
        assert!(disconnected.contains(&"ETH".to_string()));
    }

    #[test]
    fn test_relist_after_suspension() {
        let mut manager = create_test_manager();

        // Suspend BTC-USD and BTC-ETH (isolate BTC)
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        });
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-ETH".to_string(),
        });

        // Verify BTC is disconnected
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 1);
        assert!(disconnected.contains(&"BTC".to_string()));

        // Relist BTC-USD
        manager.handle_event(TopologyEvent::PairRelisted {
            instrument: "BTC-USD".to_string(),
            best_bid: Some(Decimal::new(50000, 0)),
            best_ask: Some(Decimal::new(51000, 0)),
        });

        // Verify BTC is now reconnected
        assert!(manager.is_reachable("BTC", "USD"));
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 0);
    }

    #[test]
    fn test_partial_disconnection() {
        let mut manager = create_test_manager();

        // Suspend BTC-USD (but BTC-ETH and ETH-USD still exist)
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        });

        // BTC should still be reachable via ETH
        // BTC → ETH → USD
        assert!(manager.is_reachable("BTC", "USD"));

        // No disconnections
        let disconnected = manager.get_disconnected_from_usd();
        assert_eq!(disconnected.len(), 0);
    }

    #[test]
    fn test_bidirectional_edges_removed() {
        let mut manager = create_test_manager();

        // Get indices
        let btc_idx = manager.graph().get_index("BTC").unwrap();
        let usd_idx = manager.graph().get_index("USD").unwrap();

        // Verify both directions exist
        assert!(manager.graph().has_edge(btc_idx, usd_idx));
        assert!(manager.graph().has_edge(usd_idx, btc_idx));

        // Suspend
        manager.handle_event(TopologyEvent::PairSuspended {
            instrument: "BTC-USD".to_string(),
        });

        // Verify both directions removed
        assert!(!manager.graph().has_edge(btc_idx, usd_idx));
        assert!(!manager.graph().has_edge(usd_idx, btc_idx));
    }

    #[test]
    fn test_parse_instrument() {
        assert_eq!(
            TopologyManager::parse_instrument("BTC-USD"),
            Some(("BTC".to_string(), "USD".to_string()))
        );
        assert_eq!(
            TopologyManager::parse_instrument("ETH-BTC"),
            Some(("ETH".to_string(), "BTC".to_string()))
        );
        assert_eq!(TopologyManager::parse_instrument("INVALID"), None);
    }

    #[test]
    fn test_is_reachable_same_currency() {
        let manager = create_test_manager();

        // Currency is always reachable to itself
        assert!(manager.is_reachable("USD", "USD"));
        assert!(manager.is_reachable("BTC", "BTC"));
    }

    #[test]
    fn test_invalid_instrument() {
        let mut manager = create_test_manager();

        // Invalid instrument format should not panic
        let event = TopologyEvent::PairSuspended {
            instrument: "INVALID".to_string(),
        };
        let disconnected = manager.handle_event(event);

        // Should return empty (no changes)
        assert_eq!(disconnected.len(), 0);
    }
}
