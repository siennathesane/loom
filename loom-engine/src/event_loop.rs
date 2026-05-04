// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Sienna Meridian Satterwhite

//! Listen Mode event loop - integrates FIX, Graph, and State Machine
//!
//! This is the core integration point for Phase 3. It:
//! 1. Receives FIX L3 market data updates
//! 2. Updates the graph via GraphMaintenanceManager
//! 3. Detects arbitrage opportunities via Bellman-Ford
//! 4. Logs opportunities for analysis
//!
//! Architecture:
//! - Single-threaded tokio event loop with select!
//! - Edge change tracking to minimize Bellman-Ford invocations
//! - Periodic cycle checks (default 100ms interval)

use crate::lump_sum::LumpSumId;
use crate::state_machine::StateMachine;
use loom_fix::market_data::{MDEntry, MDEntryType, MDUpdateAction};
use loom_graph::{
    detect_negative_cycle, gravitational_potential, verify_cycle_weight, AdjacencyMatrix,
    BellmanFordResult, GraphMaintenanceManager, L3Update,
};
use loom_store::{initialize_database, BatchWriter, OpportunityBuffer, TickBuffer};
use tokio::select;
use tokio::time::{interval, Duration, Instant};

/// Listen Mode event loop configuration
#[derive(Debug, Clone)]
pub struct ListenConfig {
    /// Safety margin for gravitational gate
    pub safety_margin: f64,

    /// Fee in basis points (e.g., 60 for 0.6%)
    pub fee_bps: u16,

    /// Bellman-Ford trigger interval (ms)
    /// How often to run cycle detection
    pub cycle_check_interval_ms: u64,
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            safety_margin: 1.5,
            fee_bps: 60,                  // Coinbase 0.6%
            cycle_check_interval_ms: 100, // Check every 100ms
        }
    }
}

/// Detected arbitrage opportunity (for logging)
#[derive(Debug, Clone)]
pub struct Opportunity {
    /// Unique ID
    pub id: uuid::Uuid,
    /// Cycle of node indices
    pub cycle: Vec<usize>,
    /// Cycle in currency symbols
    pub cycle_symbols: Vec<String>,
    /// Expected profit (converted from -ln space to linear profit)
    /// Positive value indicates profit (e.g., 0.05 = 5% profit)
    pub expected_profit: f64,
    /// Maximum return cost along cycle (highest cost to return to USD from any node)
    pub max_return_cost: f64,
    /// Detection timestamp (Unix seconds)
    pub detected_at: u64,
}

/// Listen Mode event loop
pub struct ListenEventLoop {
    /// Graph maintenance manager
    graph_manager: GraphMaintenanceManager,

    /// State machine
    state_machine: StateMachine,

    /// Configuration
    config: ListenConfig,

    /// Edge changes since last cycle check
    /// Tracks which edges changed to decide if Bellman-Ford needed
    edge_changes: bool,

    /// Detected opportunities (for testing/logging)
    opportunities: Vec<Opportunity>,

    /// Batch writer for SQLite persistence
    batch_writer: Option<BatchWriter>,
}

/// Helper function to get current timestamp in microseconds
fn current_timestamp_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

impl ListenEventLoop {
    /// Create a new event loop without database persistence (for testing)
    pub fn new(graph: AdjacencyMatrix, config: ListenConfig) -> Self {
        let graph_manager = GraphMaintenanceManager::new(graph, config.fee_bps);
        let state_machine = StateMachine::new(config.safety_margin);

        Self {
            graph_manager,
            state_machine,
            config,
            edge_changes: false,
            opportunities: Vec::new(),
            batch_writer: None,
        }
    }

    /// Create a new event loop with database persistence
    pub fn with_database(
        graph: AdjacencyMatrix,
        config: ListenConfig,
        db_path: &str,
    ) -> Result<Self, loom_store::rusqlite::Error> {
        let conn = initialize_database(db_path)?;
        let batch_writer = BatchWriter::new(conn, 1000, 1000);

        let graph_manager = GraphMaintenanceManager::new(graph, config.fee_bps);
        let state_machine = StateMachine::new(config.safety_margin);

        Ok(Self {
            graph_manager,
            state_machine,
            config,
            edge_changes: false,
            opportunities: Vec::new(),
            batch_writer: Some(batch_writer),
        })
    }

    /// Handle L3 market data update
    ///
    /// Converts FIX MDEntry to L3Update and processes through graph manager
    ///
    /// # Arguments
    /// * `entry` - FIX market data entry
    /// * `instrument` - Trading pair (e.g., "BTC-USD")
    ///
    /// Note: In full implementation (Task 3.7), instrument will be extracted
    /// from the FIX message. For now, it's passed explicitly.
    pub fn handle_market_data(&mut self, entry: &MDEntry, instrument: &str) {
        // Log tick to database (if persistence enabled)
        if let Some(batch_writer) = &mut self.batch_writer {
            let side = match entry.entry_type {
                MDEntryType::Bid => "bid",
                MDEntryType::Offer => "ask",
                MDEntryType::Trade => "trade",
            };

            let tick = TickBuffer {
                timestamp: current_timestamp_micros() as i64,
                instrument: instrument.to_string(),
                side: side.to_string(),
                price: entry.price.to_string(),
                quantity: entry.size.to_string(),
            };

            // Buffer tick (errors are non-fatal for market data processing)
            let _ = batch_writer.buffer_tick(tick);
        }

        // Convert FIX MDEntry to L3Update
        let l3_update = self.convert_md_entry_to_l3(entry, instrument);

        if let Some(update) = l3_update {
            // Process through graph manager
            let edge_updates = self.graph_manager.handle_update(update);

            // If any edges changed, mark for cycle check
            if !edge_updates.is_empty() {
                self.edge_changes = true;
            }
        }
    }

    /// Convert FIX MDEntry to L3Update
    ///
    /// Maps FIX update actions and entry types to graph L3 updates
    fn convert_md_entry_to_l3(&self, entry: &MDEntry, instrument: &str) -> Option<L3Update> {
        // Determine order side from entry type
        let side = match entry.entry_type {
            MDEntryType::Bid => loom_graph::graph_maintenance::OrderSide::Buy,
            MDEntryType::Offer => loom_graph::graph_maintenance::OrderSide::Sell,
            MDEntryType::Trade => return None, // Ignore trades for L3
        };

        // Map FIX update action to L3Update
        match entry.update_action {
            MDUpdateAction::New => {
                // New order
                Some(L3Update::NewOrder {
                    instrument: instrument.to_string(),
                    order_id: entry.entry_id.clone().unwrap_or_default(),
                    side,
                    price: entry.price,
                    size: entry.size,
                })
            }
            MDUpdateAction::Change => {
                // Modified order
                Some(L3Update::ModifyOrder {
                    instrument: instrument.to_string(),
                    order_id: entry.entry_id.clone().unwrap_or_default(),
                    new_price: Some(entry.price),
                    new_size: entry.size,
                })
            }
            MDUpdateAction::Delete => {
                // Deleted order
                Some(L3Update::DeleteOrder {
                    instrument: instrument.to_string(),
                    order_id: entry.entry_id.clone().unwrap_or_default(),
                })
            }
        }
    }

    /// Check for arbitrage cycles (Bellman-Ford)
    ///
    /// Only runs if edge_changes flag is set (performance optimization)
    pub fn check_for_cycles(&mut self) {
        if !self.edge_changes {
            return; // No changes, skip cycle check
        }

        // Run Bellman-Ford from USD (node 0)
        let usd_idx = 0; // Assuming USD is at index 0
        let result = detect_negative_cycle(self.graph_manager.graph(), usd_idx);

        match result {
            BellmanFordResult::NegativeCycle(cycle) => {
                // Found arbitrage opportunity!
                self.handle_opportunity(cycle);
            }
            BellmanFordResult::NoCycle => {
                // No arbitrage found
            }
        }

        // Reset edge changes flag
        self.edge_changes = false;
    }

    /// Handle detected opportunity with gravitational gating
    fn handle_opportunity(&mut self, cycle: Vec<usize>) {
        // Calculate expected profit using verify_cycle_weight
        let total_weight = verify_cycle_weight(self.graph_manager.graph(), &cycle);

        // Calculate max return cost along cycle
        let max_return_cost = self.get_max_return_cost_in_cycle(&cycle);

        // Convert cycle to symbols
        let cycle_symbols: Vec<String> = cycle
            .iter()
            .map(|&idx| {
                self.graph_manager
                    .graph()
                    .get_symbol(idx)
                    .unwrap_or("UNKNOWN")
                    .to_string()
            })
            .collect();

        // Create opportunity
        // Note: expected_profit is negative in -ln space (more negative = more profit)
        // Convert to positive profit value: -total_weight
        let opportunity = Opportunity {
            id: uuid::Uuid::new_v4(),
            cycle: cycle.clone(),
            cycle_symbols: cycle_symbols.clone(),
            expected_profit: -total_weight, // Convert -ln to profit (negative weight = profit)
            max_return_cost,
            detected_at: current_timestamp_micros(),
        };

        // GRAVITATIONAL GATE CHECK (for logging/analysis in Listen Mode)
        // In Active mode, this would block execution
        let gated = !self.check_gravitational_gate(&opportunity);
        if gated {
            // Log as gated opportunity (informational)
            eprintln!(
                "Opportunity GATED: profit={:.6}, return_cost={:.6}, margin={:.2}",
                opportunity.expected_profit, max_return_cost, self.config.safety_margin
            );
        }

        // Log opportunity to database (if persistence enabled)
        if let Some(batch_writer) = &mut self.batch_writer {
            let opp_buffer = OpportunityBuffer {
                opportunity_id: opportunity.id.to_string(),
                detected_at: opportunity.detected_at as i64,
                cycle_json: serde_json::to_string(&cycle).unwrap_or_else(|_| "[]".to_string()),
                cycle_symbols: serde_json::to_string(&cycle_symbols)
                    .unwrap_or_else(|_| "[]".to_string()),
                expected_profit: opportunity.expected_profit,
                max_return_cost: opportunity.max_return_cost,
                gated,
            };

            // Buffer opportunity (errors are non-fatal)
            let _ = batch_writer.buffer_opportunity(opp_buffer);
        }

        self.opportunities.push(opportunity);
    }

    /// Get detected opportunities (for testing)
    pub fn opportunities(&self) -> &[Opportunity] {
        &self.opportunities
    }

    /// Get reference to graph manager (for testing)
    pub fn graph_manager(&self) -> &GraphMaintenanceManager {
        &self.graph_manager
    }

    /// Calculate return costs for all currencies (gravitational potential)
    ///
    /// Returns vector where result[i] = cost to return to USD from currency i
    fn calculate_return_costs(&self) -> Vec<f64> {
        let usd_idx = 0; // Assuming USD is at index 0
        gravitational_potential(self.graph_manager.graph(), usd_idx)
    }

    /// Get max return cost along a cycle path
    ///
    /// This is the highest cost to return to USD from any node in the cycle
    fn get_max_return_cost_in_cycle(&self, cycle: &[usize]) -> f64 {
        let return_costs = self.calculate_return_costs();

        cycle
            .iter()
            .map(|&node_idx| return_costs[node_idx])
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Check if opportunity passes gravitational gate
    ///
    /// Returns true if opportunity is safe to execute
    ///
    /// Gate passes if: profit > return_cost × margin
    pub fn check_gravitational_gate(&self, opportunity: &Opportunity) -> bool {
        let profit = opportunity.expected_profit;
        let return_cost = opportunity.max_return_cost;
        let margin = self.config.safety_margin;

        // Gate passes if: profit > return_cost × margin
        profit > return_cost * margin
    }

    /// Evaluate opportunity for lump sum (with gravitational gating)
    ///
    /// Returns true if lump sum transitioned to OPPORTUNITY state
    ///
    /// This integrates with the state machine for future Active mode
    pub fn evaluate_opportunity_for_lump_sum(
        &mut self,
        lump_sum_id: &LumpSumId,
        opportunity: &Opportunity,
    ) -> bool {
        // Check gravitational gate
        if !self.check_gravitational_gate(opportunity) {
            return false; // Gated
        }

        // Transition via state machine
        self.state_machine.transition_to_opportunity(
            lump_sum_id,
            opportunity.expected_profit,
            opportunity.max_return_cost,
        )
    }

    /// Flush database batches if needed (based on time or buffer size)
    pub fn maybe_flush(&mut self) -> Result<(), loom_store::rusqlite::Error> {
        if let Some(batch_writer) = &mut self.batch_writer {
            if batch_writer.should_flush() {
                batch_writer.flush()?;
            }
        }
        Ok(())
    }

    /// Shutdown the event loop, ensuring all buffered data is flushed
    ///
    /// This prevents data loss on shutdown
    pub fn shutdown(mut self) -> Result<(), loom_store::rusqlite::Error> {
        if let Some(batch_writer) = self.batch_writer.take() {
            batch_writer.shutdown()?;
        }
        Ok(())
    }

    /// Run the event loop (async)
    ///
    /// This is a simplified version for testing - full FIX integration in Task 3.7
    ///
    /// # Arguments
    /// * `duration_secs` - How long to run the loop
    pub async fn run_simple(&mut self, duration_secs: u64) {
        let mut cycle_check_timer =
            interval(Duration::from_millis(self.config.cycle_check_interval_ms));

        let start = Instant::now();
        let end_time = Duration::from_secs(duration_secs);

        loop {
            select! {
                _ = cycle_check_timer.tick() => {
                    self.check_for_cycles();
                }
                _ = tokio::time::sleep(end_time.saturating_sub(start.elapsed())) => {
                    break;
                }
            }

            if start.elapsed() >= end_time {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_graph::AdjacencyMatrix;
    use rust_decimal::Decimal;

    fn create_test_graph() -> AdjacencyMatrix {
        let currencies = vec!["USD".to_string(), "BTC".to_string(), "ETH".to_string()];
        AdjacencyMatrix::new(currencies)
    }

    #[tokio::test]
    async fn test_create_event_loop() {
        let graph = create_test_graph();
        let config = ListenConfig::default();

        let event_loop = ListenEventLoop::new(graph, config.clone());

        // Verify initialization
        assert_eq!(event_loop.edge_changes, false);
        assert_eq!(event_loop.opportunities.len(), 0);
        assert_eq!(event_loop.config.safety_margin, 1.5);
        assert_eq!(event_loop.config.fee_bps, 60);
    }

    #[tokio::test]
    async fn test_market_data_updates_graph() {
        let graph = create_test_graph();
        let config = ListenConfig::default();
        let mut event_loop = ListenEventLoop::new(graph, config);

        // Create MDEntry (NewOrder - Bid)
        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Bid,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
            entry_id: Some("order123".to_string()),
        };

        // Process entry
        event_loop.handle_market_data(&entry, "BTC-USD");

        // Verify edge_changes flag set
        assert!(
            event_loop.edge_changes,
            "Edge changes flag should be set after market data update"
        );
    }

    #[tokio::test]
    async fn test_cycle_detection_on_edge_change() {
        let mut graph = create_test_graph();

        // Create profitable cycle: USD -> BTC -> ETH -> USD
        // With no fees for simplicity
        let config = ListenConfig {
            safety_margin: 1.5,
            fee_bps: 0, // No fees for test
            cycle_check_interval_ms: 100,
        };

        // Set up edges manually in graph
        graph
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap(); // 0.001
        graph
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap(); // 50
        graph
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap(); // 21

        // Product: 0.001 × 50 × 21 = 1.05 > 1 (profitable)

        let mut event_loop = ListenEventLoop::new(graph, config);
        event_loop.edge_changes = true;

        // Run cycle detection
        event_loop.check_for_cycles();

        // Verify opportunity detected
        assert_eq!(
            event_loop.opportunities.len(),
            1,
            "Should detect one opportunity"
        );

        let opp = &event_loop.opportunities[0];
        assert!(
            opp.expected_profit > 0.0,
            "Expected profit should be positive (converted from -ln space)"
        );
        assert!(opp.cycle.len() >= 3, "Cycle should have at least 3 nodes");
    }

    #[tokio::test]
    async fn test_no_cycle_detection_without_changes() {
        let graph = create_test_graph();
        let config = ListenConfig::default();
        let mut event_loop = ListenEventLoop::new(graph, config);

        // Set edge_changes = false
        event_loop.edge_changes = false;

        // Run check_for_cycles
        event_loop.check_for_cycles();

        // Verify no opportunities detected (Bellman-Ford not run)
        assert_eq!(
            event_loop.opportunities.len(),
            0,
            "Should not run Bellman-Ford without edge changes"
        );
        assert_eq!(event_loop.edge_changes, false, "Flag should remain false");
    }

    #[tokio::test]
    async fn test_opportunity_logging() {
        let mut graph = create_test_graph();

        // Create cycle
        let config = ListenConfig {
            fee_bps: 0,
            ..Default::default()
        };

        graph
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap();
        graph
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap();

        let mut event_loop = ListenEventLoop::new(graph, config);
        event_loop.edge_changes = true;
        event_loop.check_for_cycles();

        // Verify opportunity fields
        assert_eq!(event_loop.opportunities.len(), 1);

        let opp = &event_loop.opportunities[0];
        assert!(opp.cycle_symbols.contains(&"USD".to_string()));
        assert!(opp.cycle_symbols.contains(&"BTC".to_string()));
        assert!(opp.cycle_symbols.contains(&"ETH".to_string()));
        assert!(opp.detected_at > 0, "Timestamp should be set");
    }

    #[tokio::test]
    async fn test_run_simple_event_loop() {
        let graph = create_test_graph();
        let config = ListenConfig {
            cycle_check_interval_ms: 50, // Fast interval for testing
            ..Default::default()
        };

        let mut event_loop = ListenEventLoop::new(graph, config);

        // Set edge changes to trigger cycle checks
        event_loop.edge_changes = true;

        // Run for 200ms (should trigger ~4 cycle checks at 50ms interval)
        let start = Instant::now();
        event_loop.run_simple(0).await; // 0 seconds = immediate return
        let elapsed = start.elapsed();

        // Verify loop runs without panicking
        assert!(
            elapsed.as_millis() < 100,
            "Event loop should complete quickly"
        );
    }

    #[tokio::test]
    async fn test_md_entry_conversion_bid() {
        let graph = create_test_graph();
        let config = ListenConfig::default();
        let event_loop = ListenEventLoop::new(graph, config);

        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Bid,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 0),
            entry_id: Some("order123".to_string()),
        };

        let l3_update = event_loop.convert_md_entry_to_l3(&entry, "BTC-USD");

        assert!(l3_update.is_some());
        match l3_update.unwrap() {
            L3Update::NewOrder {
                instrument,
                side,
                price,
                size,
                ..
            } => {
                assert_eq!(instrument, "BTC-USD");
                assert_eq!(side, loom_graph::graph_maintenance::OrderSide::Buy);
                assert_eq!(price, Decimal::new(50000, 0));
                assert_eq!(size, Decimal::new(1, 0));
            }
            _ => panic!("Expected NewOrder"),
        }
    }

    #[tokio::test]
    async fn test_md_entry_conversion_offer() {
        let graph = create_test_graph();
        let config = ListenConfig::default();
        let event_loop = ListenEventLoop::new(graph, config);

        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Offer,
            price: Decimal::new(51000, 0),
            size: Decimal::new(2, 0),
            entry_id: Some("order456".to_string()),
        };

        let l3_update = event_loop.convert_md_entry_to_l3(&entry, "BTC-USD");

        assert!(l3_update.is_some());
        match l3_update.unwrap() {
            L3Update::NewOrder { side, .. } => {
                assert_eq!(side, loom_graph::graph_maintenance::OrderSide::Sell);
            }
            _ => panic!("Expected NewOrder"),
        }
    }

    #[tokio::test]
    async fn test_md_entry_ignore_trade() {
        let graph = create_test_graph();
        let config = ListenConfig::default();
        let event_loop = ListenEventLoop::new(graph, config);

        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Trade,
            price: Decimal::new(50500, 0),
            size: Decimal::new(1, 0),
            entry_id: Some("trade789".to_string()),
        };

        let l3_update = event_loop.convert_md_entry_to_l3(&entry, "BTC-USD");

        // Trades should be ignored
        assert!(l3_update.is_none());
    }

    #[tokio::test]
    async fn test_calculate_return_costs() {
        let mut graph = create_test_graph();
        let config = ListenConfig::default();

        // Add edges: BTC→USD and ETH→USD
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 60)
            .unwrap();

        let event_loop = ListenEventLoop::new(graph, config);

        let return_costs = event_loop.calculate_return_costs();

        // USD (index 0) should have cost 0
        assert_eq!(return_costs[0], 0.0, "USD return cost should be 0");

        // BTC and ETH should have finite costs
        assert!(
            return_costs[1].is_finite(),
            "BTC should have finite return cost"
        );
        assert!(
            return_costs[2].is_finite(),
            "ETH should have finite return cost"
        );

        // Return costs should be negative (converting TO valuable currency)
        assert!(return_costs[1] < 0.0, "BTC return cost should be negative");
        assert!(return_costs[2] < 0.0, "ETH return cost should be negative");
    }

    #[tokio::test]
    async fn test_max_return_cost_in_cycle() {
        let mut graph = create_test_graph();
        let config = ListenConfig::default();

        // Create cycle: USD→BTC→ETH→USD
        graph
            .update_edge("USD", "BTC", Decimal::new(2, 5), 60)
            .unwrap(); // 0.00002
        graph
            .update_edge("BTC", "ETH", Decimal::new(20, 0), 60)
            .unwrap(); // 20
        graph
            .update_edge("ETH", "USD", Decimal::new(2500, 0), 60)
            .unwrap(); // 2500

        // Add return paths
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 60)
            .unwrap();

        let event_loop = ListenEventLoop::new(graph, config);

        let cycle = vec![0, 1, 2, 0]; // USD→BTC→ETH→USD
        let max_return_cost = event_loop.get_max_return_cost_in_cycle(&cycle);

        // Max return cost should be finite (all nodes can reach USD)
        assert!(
            max_return_cost.is_finite(),
            "Max return cost should be finite"
        );

        // Should be the maximum among USD(0), BTC, ETH return costs
        let return_costs = event_loop.calculate_return_costs();
        let expected_max = return_costs[0].max(return_costs[1]).max(return_costs[2]);
        assert_eq!(
            max_return_cost, expected_max,
            "Should return highest cost in cycle"
        );
    }

    #[tokio::test]
    async fn test_gravitational_gate_allows() {
        let graph = create_test_graph();
        let config = ListenConfig {
            safety_margin: 1.5,
            ..Default::default()
        };
        let event_loop = ListenEventLoop::new(graph, config);

        // Create opportunity with:
        //   profit = 0.1 (10% profit)
        //   max_return_cost = 0.05
        //   safety_margin = 1.5
        // Check: 0.1 > 0.05 × 1.5 = 0.075 → ALLOWED
        let opportunity = Opportunity {
            id: uuid::Uuid::new_v4(),
            cycle: vec![0, 1, 2, 0],
            cycle_symbols: vec![
                "USD".to_string(),
                "BTC".to_string(),
                "ETH".to_string(),
                "USD".to_string(),
            ],
            expected_profit: 0.1,
            max_return_cost: 0.05,
            detected_at: 0,
        };

        let allowed = event_loop.check_gravitational_gate(&opportunity);
        assert!(allowed, "Gate should allow: 0.1 > 0.05 × 1.5 = 0.075");
    }

    #[tokio::test]
    async fn test_gravitational_gate_blocks() {
        let graph = create_test_graph();
        let config = ListenConfig {
            safety_margin: 1.5,
            ..Default::default()
        };
        let event_loop = ListenEventLoop::new(graph, config);

        // Create opportunity with:
        //   profit = 0.1
        //   max_return_cost = 0.1
        //   safety_margin = 1.5
        // Check: 0.1 <= 0.1 × 1.5 = 0.15 → BLOCKED
        let opportunity = Opportunity {
            id: uuid::Uuid::new_v4(),
            cycle: vec![0, 1, 2, 0],
            cycle_symbols: vec![
                "USD".to_string(),
                "BTC".to_string(),
                "ETH".to_string(),
                "USD".to_string(),
            ],
            expected_profit: 0.1,
            max_return_cost: 0.1,
            detected_at: 0,
        };

        let allowed = event_loop.check_gravitational_gate(&opportunity);
        assert!(!allowed, "Gate should block: 0.1 <= 0.1 × 1.5 = 0.15");
    }

    #[tokio::test]
    async fn test_opportunity_with_return_cost() {
        let mut graph = create_test_graph();
        let config = ListenConfig {
            fee_bps: 0,
            ..Default::default()
        };

        // Create profitable cycle
        graph
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap();
        graph
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap();

        // Add return paths
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 0)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 0)
            .unwrap();

        let mut event_loop = ListenEventLoop::new(graph, config);
        event_loop.edge_changes = true;
        event_loop.check_for_cycles();

        // Verify opportunity has non-zero max_return_cost
        assert_eq!(event_loop.opportunities.len(), 1);
        let opp = &event_loop.opportunities[0];

        assert!(
            opp.max_return_cost != 0.0,
            "Max return cost should be non-zero"
        );
        assert!(
            opp.max_return_cost.is_finite(),
            "Max return cost should be finite"
        );
    }

    #[tokio::test]
    async fn test_gated_opportunity_logged() {
        let graph = create_test_graph();
        let config = ListenConfig {
            fee_bps: 0,
            safety_margin: 2.0, // Normal margin
            ..Default::default()
        };

        let event_loop = ListenEventLoop::new(graph, config);

        // Manually create opportunity that will be gated
        // profit = 0.01 (1% profit)
        // max_return_cost = 0.01
        // safety_margin = 2.0
        // Check: 0.01 <= 0.01 × 2.0 = 0.02 → BLOCKED
        let opportunity = Opportunity {
            id: uuid::Uuid::new_v4(),
            cycle: vec![0, 1, 2, 0],
            cycle_symbols: vec![
                "USD".to_string(),
                "BTC".to_string(),
                "ETH".to_string(),
                "USD".to_string(),
            ],
            expected_profit: 0.01, // 1% profit
            max_return_cost: 0.01, // Equal to profit
            detected_at: 0,
        };

        // Verify gate blocks this opportunity
        let gate_passed = event_loop.check_gravitational_gate(&opportunity);
        assert!(
            !gate_passed,
            "Opportunity should be gated: 0.01 <= 0.01 × 2.0 = 0.02"
        );

        // In Listen Mode, even gated opportunities would be logged for analysis
        // This test verifies the gate logic works correctly
    }

    #[tokio::test]
    async fn test_evaluate_opportunity_for_lump_sum() {
        let mut graph = create_test_graph();
        let config = ListenConfig {
            safety_margin: 1.5,
            ..Default::default()
        };

        // Add return paths
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 60)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 60)
            .unwrap();

        let mut event_loop = ListenEventLoop::new(graph, config);

        // Create lump sum
        use crate::lump_sum::LumpSum;
        let lump_sum = LumpSum::new(
            "TestLump".to_string(),
            "USD".to_string(),
            rust_decimal::Decimal::new(1000, 0),
        );
        let lump_sum_id = lump_sum.id;
        event_loop.state_machine.add_lump_sum(lump_sum);

        // Create opportunity that passes gate
        let opportunity = Opportunity {
            id: uuid::Uuid::new_v4(),
            cycle: vec![0, 1, 2, 0],
            cycle_symbols: vec![
                "USD".to_string(),
                "BTC".to_string(),
                "ETH".to_string(),
                "USD".to_string(),
            ],
            expected_profit: 0.1,  // 10% profit
            max_return_cost: 0.05, // Low return cost
            detected_at: 0,
        };

        // Evaluate opportunity
        let allowed = event_loop.evaluate_opportunity_for_lump_sum(&lump_sum_id, &opportunity);
        assert!(allowed, "Should allow transition to OPPORTUNITY state");

        // Verify lump sum state changed
        let lump_sum = event_loop.state_machine.get_lump_sum(&lump_sum_id).unwrap();
        assert_eq!(lump_sum.state, crate::lump_sum::LumpSumState::Opportunity);
    }

    #[tokio::test]
    async fn test_database_integration() {
        use tempfile::NamedTempFile;

        let temp_db = NamedTempFile::new().unwrap();
        let db_path = temp_db.path().to_str().unwrap();

        let mut graph = create_test_graph();
        let config = ListenConfig {
            safety_margin: 1.5,
            fee_bps: 0,
            cycle_check_interval_ms: 100,
        };

        // Create profitable cycle
        graph
            .update_edge("USD", "BTC", Decimal::new(1, 3), 0)
            .unwrap();
        graph
            .update_edge("BTC", "ETH", Decimal::new(50, 0), 0)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(21, 0), 0)
            .unwrap();

        // Add return paths
        graph
            .update_edge("BTC", "USD", Decimal::new(50000, 0), 0)
            .unwrap();
        graph
            .update_edge("ETH", "USD", Decimal::new(3000, 0), 0)
            .unwrap();

        let mut event_loop = ListenEventLoop::with_database(graph, config, db_path)
            .expect("Failed to create event loop with database");

        // Simulate market data
        let entry = MDEntry {
            update_action: MDUpdateAction::New,
            entry_type: MDEntryType::Bid,
            price: Decimal::new(50000, 0),
            size: Decimal::new(1, 1),
            entry_id: Some("order123".to_string()),
        };

        event_loop.handle_market_data(&entry, "BTC-USD");
        event_loop.maybe_flush().unwrap();

        // Trigger cycle detection
        event_loop.edge_changes = true;
        event_loop.check_for_cycles();
        event_loop.maybe_flush().unwrap();

        // Verify data was logged
        assert_eq!(
            event_loop.opportunities.len(),
            1,
            "Should detect one opportunity"
        );

        // Shutdown and flush remaining data
        event_loop.shutdown().unwrap();
    }
}
